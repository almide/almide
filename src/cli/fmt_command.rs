//! `almide fmt`: format, check, or show each file, auto-managing imports
//! against the package's dependencies.

use crate::{parse_file, fmt, out, out_no_nl, err};

/// Load dependency names and submodule map from almide.toml for fmt auto-import.
fn load_dep_info_for_fmt() -> (Vec<String>, std::collections::HashMap<String, String>) {
    let toml_path = std::path::Path::new("almide.toml");
    if !toml_path.exists() {
        return (vec![], std::collections::HashMap::new());
    }
    let project = match crate::project::parse_toml(toml_path) {
        Ok(p) => p,
        Err(_) => return (vec![], std::collections::HashMap::new()),
    };
    let dep_names: Vec<String> = project.dependencies.iter().map(|d| d.name.clone()).collect();

    // Discover submodules for each dependency by scanning cached source directories
    let mut submodules = std::collections::HashMap::new();
    for dep in &project.dependencies {
        // Check cache dir: ~/.almide/cache/{name}/.src-{source}/{tag_or_commit}/.
        // Rooted at THIS dependency's source (#2523) — the scan used to start at
        // `{name}/` and take whatever was cached there first, so a same-named
        // package from another URL could name this one's submodules.
        let dep_cache = crate::project_fetch::dep_cache_root(dep);
        if dep_cache.is_dir() {
            // Use the first checkout found. Directories starting with a dot are
            // layout, not checkouts (and a `.tmp-` one is a clone in progress).
            if let Ok(entries) = std::fs::read_dir(&dep_cache) {
                if let Some(version_dir) = entries.flatten().find(|e| {
                    e.path().is_dir() && !e.file_name().to_string_lossy().starts_with('.')
                }) {
                    // A `subdir` dependency's checkout is the whole
                    // repository; its package is the subdir (#3381).
                    let package_dir = match &dep.subdir {
                        Some(sub) => version_dir.path().join(sub),
                        None => version_dir.path(),
                    };
                    scan_submodules(&package_dir, &dep.name, &mut submodules);
                }
            }
        }
        // Also check local: {name}/ next to almide.toml
        let local_dir = std::path::Path::new(&dep.name);
        if local_dir.is_dir() {
            scan_submodules(local_dir, &dep.name, &mut submodules);
        }
    }
    (dep_names, submodules)
}

/// Recursively scan a package's src/ directory to discover submodules.
/// Maps last path segment → full dotted path (e.g., "python" → "bindgen.bindings.python").
fn scan_submodules(base_dir: &std::path::Path, pkg_name: &str, out: &mut std::collections::HashMap<String, String>) {
    let src_dir = base_dir.join("src");
    let scan_dir = if src_dir.is_dir() { &src_dir } else { base_dir };
    scan_submodules_recursive(scan_dir, pkg_name, out);
}

fn scan_submodules_recursive(dir: &std::path::Path, prefix: &str, out: &mut std::collections::HashMap<String, String>) {
    let entries = match std::fs::read_dir(dir) {
        Ok(e) => e,
        Err(_) => return,
    };
    for entry in entries.flatten() {
        let path = entry.path();
        let name = entry.file_name().to_string_lossy().to_string();
        if path.is_file() && name.ends_with(".almd") {
            let stem = name.trim_end_matches(".almd");
            if stem == "mod" || stem == "lib" || stem == "main" { continue; }
            let full = format!("{}.{}", prefix, stem);
            out.insert(stem.to_string(), full);
        } else if path.is_dir() && !name.starts_with('.') {
            let sub_prefix = format!("{}.{}", prefix, name);
            scan_submodules_recursive(&path, &sub_prefix, out);
        }
    }
}

/// What `almide fmt` does with the formatted text, per `docs/specs/cli.md`.
///
/// `Check` and `DryRun` were one `write_back: bool` that printed the formatted text and
/// exited 0 unconditionally, so `almide fmt --check` reported nothing and every CI gate
/// written against it was a no-op (#919). They are distinct in the spec and now in the
/// code: `--check` is the GATE (say which files differ, exit 1), `--dry-run` SHOWS the
/// formatted text without touching the file.
#[derive(Clone, Copy, PartialEq, Eq)]
pub enum FmtMode {
    /// Format and write the file back (the default).
    Write,
    /// Compare only: report each file that is not already formatted, exit 1 if any is.
    Check,
    /// `Check`, reported as one JSON object on stdout (`--json`). Same gate,
    /// same exit code — the difference is who reads it. Added for the MCP
    /// server (`almide mcp`), which must not parse the human report.
    CheckJson,
    /// Print the formatted text; never write, never fail.
    DryRun,
}

impl FmtMode {
    /// True for the two comparing modes — neither writes a file.
    fn is_check(self) -> bool {
        matches!(self, FmtMode::Check | FmtMode::CheckJson)
    }
}

/// `cmd_fmt`'s `--check` verdict. Text mode prints it to stderr and JSON mode
/// prints it as one object to stdout; both exit 1 on any drift, so a gate
/// written against either spelling fails identically.
fn report_fmt_check(mode: FmtMode, total: usize, unformatted: &[String], unreadable: &[String], verify_failed: bool) {
    let ok = unformatted.is_empty() && unreadable.is_empty() && !verify_failed;
    if mode == FmtMode::CheckJson {
        let report = serde_json::json!({
            "checked": total,
            "unformatted": unformatted,
            "unreadable": unreadable,
            "verify_failed": verify_failed,
            "ok": ok,
        });
        out(&format!("{}", report));
        if !ok { std::process::exit(1); }
        return;
    }
    if ok {
        err(&format!("fmt: {} file(s) already formatted", total));
        return;
    }
    for f in unformatted {
        err(&format!("not formatted: {}", f));
    }
    err(&format!(
        "fmt --check: {} of {} file(s) need formatting — run `almide fmt <path>`",
        unformatted.len(),
        total
    ));
    std::process::exit(1);
}

pub fn cmd_fmt(files: &[String], mode: FmtMode, no_import_edit: bool) {
    // Load dependency info from almide.toml (if present)
    let (dep_names, dep_submodules) = load_dep_info_for_fmt();
    // `--check` is a gate: a file that differs, and a file that cannot even be parsed,
    // both make the run fail. Under the other modes a parse error stays a skip.
    let mut unformatted: Vec<String> = Vec::new();
    let mut unreadable: Vec<String> = Vec::new();
    let mut verify_failed = false;

    for file in files {
        let (mut program, source_text, parse_errors) = parse_file(file);
        if !parse_errors.is_empty() {
            // A partially-parsed program silently drops unparseable top-level
            // items — formatting it and writing back would delete that code
            // from the file on disk. Report and skip instead.
            for e in &parse_errors {
                err(&format!("{}", crate::diagnostic_render::display_with_source(e, &source_text)));
            }
            err(&format!("{}: {} parse error(s), skipping", file, parse_errors.len()));
            unreadable.push(file.clone());
            continue;
        }
        // Auto-manage imports: add missing, remove unused. `--no-import-edit`
        // keeps the import list byte-for-byte — the stdlib's splice-context
        // sources would be corrupted by an inserted import line.
        if !no_import_edit {
            let import_changes = fmt::auto_imports(&mut program, &source_text, &dep_names, &dep_submodules);
            for msg in &import_changes {
                err(&format!("{}: {}", file, msg));
            }
        }
        let formatted = fmt::format_program(&program);
        // #1309 safety verifier (Black's --safe model): if formatting would
        // change the file, the output must re-parse, carry the same AST, and
        // keep every line comment — otherwise the file is left untouched and
        // the run fails. A formatter that corrupts is worse than none.
        if formatted != source_text {
            if let Err(why) = fmt::verify_format(&source_text, &program, &formatted) {
                // E054 (#1464): the verifier's refusal is a coded diagnostic,
                // not an anonymous string — `almide explain E054` owns the story.
                err(&fmt::verify_format_diagnostic(file, &why).display());
                verify_failed = true;
                continue;
            }
        }
        match mode {
            FmtMode::Write => {
                std::fs::write(file, &formatted)
                    .unwrap_or_else(|e| { err(&format!("Failed to write {}: {}", file, e)); std::process::exit(1); });
                err(&format!("Formatted {}", file));
            }
            FmtMode::Check | FmtMode::CheckJson => {
                if formatted != source_text {
                    unformatted.push(file.clone());
                }
            }
            FmtMode::DryRun => out_no_nl(&format!("{}", formatted)),
        }
    }

    if verify_failed && !mode.is_check() {
        std::process::exit(1);
    }
    if !mode.is_check() {
        return;
    }
    report_fmt_check(mode, files.len(), &unformatted, &unreadable, verify_failed);
}
