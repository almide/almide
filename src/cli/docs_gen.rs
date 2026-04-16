//! `almide docs-gen` — canonical-docs consistency check.
//!
//! MVP scope: `--check` mode only. Verifies that `llms.txt` and the
//! `docs/diagnostics/` tree agree with the compiler's source of truth:
//!
//! 1. **Version**: `Cargo.toml`'s `package.version` appears in llms.txt.
//! 2. **Diagnostic codes referenced in llms.txt**: every `EXXX` under
//!    `docs/diagnostics/` is mentioned at least once in llms.txt.
//! 3. **Auto-imported stdlib list**: every module in
//!    `almide_lang::stdlib_info::AUTO_IMPORT_BUNDLED` appears in llms.txt.
//! 4. **Diagnostic registry bijection**: every `EXXX` emitted by the
//!    compiler (via `with_code("EXXX")` in source) has a matching
//!    `docs/diagnostics/EXXX.md`, and vice versa.
//!
//! Non-goals (deferred — see `docs/roadmap/active/llms-txt-autogen.md`):
//! - Full regeneration of llms.txt from templates + sources.
//! - Parsing CHEATSHEET.md / DESIGN.md for idiom sections.
//! - CLI reference extraction from clap.

use std::path::{Path, PathBuf};

const LLMS_TXT: &str = "llms.txt";
const DIAGNOSTICS_DIR: &str = "docs/diagnostics";

pub fn cmd_docs_gen(check: bool) {
    if !check {
        eprintln!("error: `almide docs-gen` currently only supports `--check`.");
        eprintln!("       Full regeneration is planned — see docs/roadmap/active/llms-txt-autogen.md");
        std::process::exit(2);
    }

    let llms = match std::fs::read_to_string(LLMS_TXT) {
        Ok(s) => s,
        Err(e) => {
            eprintln!("error: cannot read `{}`: {}", LLMS_TXT, e);
            std::process::exit(2);
        }
    };

    let mut drifts: Vec<String> = Vec::new();

    drifts.extend(check_version(&llms));
    drifts.extend(check_diagnostic_codes(&llms));
    drifts.extend(check_auto_imported(&llms));
    drifts.extend(check_diagnostic_registry_bijection());

    if drifts.is_empty() {
        println!("docs-gen: ok (version, llms.txt diagnostic refs, auto-imported, registry bijection)");
        return;
    }

    eprintln!("docs-gen: {} drift(s) detected in `{}`:", drifts.len(), LLMS_TXT);
    for d in &drifts {
        eprintln!("  - {}", d);
    }
    eprintln!("\nEdit `llms.txt` to match, or bump the source if the doc is the authority.");
    std::process::exit(1);
}

// ── Checks ──────────────────────────────────────────────────────────

fn check_version(llms: &str) -> Vec<String> {
    let version = env!("CARGO_PKG_VERSION");
    if !llms.contains(version) {
        vec![format!("Cargo.toml version `{}` not mentioned anywhere in llms.txt", version)]
    } else {
        vec![]
    }
}

fn check_diagnostic_codes(llms: &str) -> Vec<String> {
    let mut drifts = Vec::new();
    let codes = match collect_diagnostic_codes() {
        Ok(c) => c,
        Err(e) => {
            drifts.push(format!("cannot read {}: {}", DIAGNOSTICS_DIR, e));
            return drifts;
        }
    };
    for code in &codes {
        if !llms.contains(code) {
            drifts.push(format!("diagnostic code `{}` (file `{}/{}.md`) not referenced in llms.txt", code, DIAGNOSTICS_DIR, code));
        }
    }
    drifts
}

fn check_auto_imported(llms: &str) -> Vec<String> {
    // The Tier 1 (never-needs-import) set + bundled auto-imports.
    // If llms.txt lists a module as "needs explicit import" but source
    // of truth says otherwise (or vice versa), that's drift.
    let auto_bundled: Vec<&str> = almide_lang::stdlib_info::AUTO_IMPORT_BUNDLED
        .iter().copied().collect();
    let mut drifts = Vec::new();
    for m in &auto_bundled {
        if !llms.contains(m) {
            drifts.push(format!(
                "auto-imported stdlib module `{}` (from AUTO_IMPORT_BUNDLED) not mentioned in llms.txt",
                m
            ));
        }
    }
    drifts
}

// ── Helpers ─────────────────────────────────────────────────────────

/// Bijection check: every diagnostic code emitted by the compiler
/// (`with_code("EXXX")` in source) must have a matching
/// `docs/diagnostics/EXXX.md`, and every doc file must correspond to
/// a code that is actually emitted. Catches E010/E011-class drift
/// (doc content describing a different code than what's emitted) and
/// E420-class drift (code emitted but never documented).
fn check_diagnostic_registry_bijection() -> Vec<String> {
    let mut drifts = Vec::new();
    let emitted = match collect_emitted_codes() {
        Ok(v) => v,
        Err(e) => { drifts.push(format!("cannot scan source for with_code(...): {}", e)); return drifts; }
    };
    let documented = match collect_diagnostic_codes() {
        Ok(v) => v,
        Err(e) => { drifts.push(format!("cannot read {}: {}", DIAGNOSTICS_DIR, e)); return drifts; }
    };
    let emitted_set: std::collections::HashSet<&String> = emitted.iter().collect();
    let documented_set: std::collections::HashSet<&String> = documented.iter().collect();

    for code in &emitted {
        if !documented_set.contains(code) {
            drifts.push(format!("code `{}` is emitted (via `with_code`) but has no `docs/diagnostics/{}.md`", code, code));
        }
    }
    for code in &documented {
        if !emitted_set.contains(code) {
            drifts.push(format!("doc `docs/diagnostics/{}.md` exists but the code is never emitted (`with_code(\"{}\")` not found in source)", code, code));
        }
    }
    drifts
}

/// Scan the `crates/` tree for `with_code("EXXX")` literals. We use a
/// simple substring match rather than a real parser because this is a
/// drift guard, not a refactoring tool — false negatives are unlikely
/// and false positives produce obvious error messages.
fn collect_emitted_codes() -> Result<Vec<String>, String> {
    let mut codes = std::collections::BTreeSet::new();
    walk_rs_files(Path::new("crates"), &mut |path, contents| {
        for mat in contents.match_indices("with_code(\"") {
            let start = mat.0 + "with_code(\"".len();
            let rest = &contents[start..];
            let end = rest.find('"').unwrap_or(0);
            if end == 0 { continue; }
            let code = &rest[..end];
            if looks_like_diag_code(code) {
                codes.insert(code.to_string());
            } else if !code.is_empty() {
                // Four-digit codes (E420 etc) are not canonical Almide
                // codes; flag them but include so the bijection check
                // can report them.
                codes.insert(code.to_string());
                let _ = path;
            }
        }
    })?;
    Ok(codes.into_iter().collect())
}

fn walk_rs_files(
    dir: &Path,
    f: &mut impl FnMut(&Path, &str),
) -> Result<(), String> {
    let entries = std::fs::read_dir(dir).map_err(|e| format!("read_dir {}: {}", dir.display(), e))?;
    for entry in entries.flatten() {
        let p = entry.path();
        if p.is_dir() {
            // Skip target/ and similar build artifacts.
            let name = p.file_name().and_then(|s| s.to_str()).unwrap_or("");
            if name == "target" || name == ".git" { continue; }
            walk_rs_files(&p, f)?;
        } else if p.extension().and_then(|s| s.to_str()) == Some("rs") {
            if let Ok(text) = std::fs::read_to_string(&p) {
                f(&p, &text);
            }
        }
    }
    Ok(())
}

fn collect_diagnostic_codes() -> Result<Vec<String>, String> {
    let dir = Path::new(DIAGNOSTICS_DIR);
    let entries = std::fs::read_dir(dir)
        .map_err(|e| format!("read_dir {}: {}", dir.display(), e))?;
    let mut codes: Vec<String> = Vec::new();
    for entry in entries.flatten() {
        let path: PathBuf = entry.path();
        let stem = path.file_stem().and_then(|s| s.to_str()).unwrap_or("");
        if looks_like_diag_code(stem) {
            codes.push(stem.to_string());
        }
    }
    codes.sort();
    Ok(codes)
}

fn looks_like_diag_code(s: &str) -> bool {
    // E + 3 digits, e.g. E001..E999. The `docs/diagnostics/README.md`
    // and similar non-code files are filtered out by this shape check.
    s.len() == 4
        && s.starts_with('E')
        && s[1..].chars().all(|c| c.is_ascii_digit())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn diag_code_shape() {
        assert!(looks_like_diag_code("E001"));
        assert!(looks_like_diag_code("E013"));
        assert!(looks_like_diag_code("E999"));
        assert!(!looks_like_diag_code("E0001"));       // 4-digit = rustc
        assert!(!looks_like_diag_code("README"));
        assert!(!looks_like_diag_code("E01"));
        assert!(!looks_like_diag_code(""));
    }
}
