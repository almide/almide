//! From the parsed command line to the command that runs it: one `dispatch_*`
//! per subcommand that needs more than a call, and the manifest gate every
//! command but the exempt ones passes first.

use crate::cli;
use crate::cli_args::{Commands, IdeCommand};
use crate::{collect_almd_files, print_error_explanation, resolve_file, warn_no_verified_deprecated, DIAGNOSTIC_DOCS};
use crate::{diagnostic_render, err, out, project, project_fetch};

/// `dispatch`'s `Commands::Run` arm. Extracted verbatim.
fn dispatch_run(file: Option<String>, no_check: bool, release: bool, target: Option<String>, no_verified: bool, time_report: bool, program_args: Vec<String>) {
    let file = resolve_file(file);
    if time_report {
        // The deterministic meter is the probe machinery: setting the env here
        // (before any compile thread exists) makes the pipeline insert charge
        // ops, and the run leg formats the probe line into the D5 dual-time
        // report instead of printing it raw.
        // SAFETY: single-threaded at this point (process entry, pre-dispatch).
        unsafe { std::env::set_var("ALMIDE_FUEL_PROBE", "1") };
    }
    // 0.29.0: v1-first verified wasm is the DEFAULT; `--no-verified` opts out.
    // 0.30.0 (#764 rung-5 complete): the v1 NATIVE trust-spine renderer is
    // likewise the DEFAULT (byte-identical to v0 where it lowers — the
    // differential rows + the 18/18 wasm_cross native byte sweep — and an
    // honest wall falls back to v0). `--no-verified` opts out of BOTH legs;
    // `--verified` is kept as a no-op for compatibility.
    warn_no_verified_deprecated(no_verified);
    cli::cmd_run(cli::RunArgs {
        file: &file,
        program_args: &program_args,
        no_check,
        release,
        target: target.as_deref(),
        verified: !no_verified,
        native_verified: !no_verified,
        time_report,
    });
}

/// `Commands::Test`'s fields, carried as one value into [`dispatch_test`].
struct TestArgs {
    file: Option<String>,
    run: Option<String>,
    no_check: bool,
    json: bool,
    target: Option<String>,
    update_snapshots: bool,
    ci: bool,
    allow_no_tests: bool,
    show_output: bool,
}

/// `dispatch`'s `Commands::Test` arm. Extracted verbatim.
fn dispatch_test(args: TestArgs) {
    let TestArgs { file, run, no_check, json, target, update_snapshots, ci, allow_no_tests, show_output } = args;
    let file_str = file.as_deref().unwrap_or("");
    // The accept step (#1314). CI mode never writes: snapshots are committed
    // and reviewed like code, so a new or drifted snapshot fails the run
    // there, with the ordinary report's accept hint pointing at a local run.
    let update = update_snapshots || almide_base::env::flag("ALMIDE_UPDATE_SNAPSHOTS");
    let ci = ci || env_flag("CI");
    if update && ci {
        eprintln!("CI mode (--ci / CI=true): --update-snapshots writes nothing — accept snapshots locally with `almide test --update-snapshots <file>` and commit the change");
    } else if update {
        cli::cmd_test_update_snapshots(file_str, no_check, run.as_deref(), target.as_deref() == Some("wasm"));
        return;
    }
    if target.as_deref() == Some("wasm") {
        cli::cmd_test_wasm(file_str, run.as_deref(), allow_no_tests, show_output);
    } else if json {
        cli::cmd_test_json(file_str, run.as_deref(), allow_no_tests);
    } else if matches!(target.as_deref(), Some("rust" | "native")) {
        // Explicit pure-native run (e.g. CI's "Test Rust" job).
        cli::cmd_test(file_str, no_check, run.as_deref(), allow_no_tests, show_output);
    } else {
        // Default: fast rustc-free WASM path, native fallback for gaps.
        cli::cmd_test_fast(file_str, no_check, run.as_deref(), allow_no_tests, show_output);
    }
}

/// A boolean environment switch that is NOT ours (`CI`): the same one
/// semantics `almide_base::env::flag` gives every `ALMIDE_*` switch.
fn env_flag(name: &str) -> bool {
    std::env::var_os(name).is_some_and(|v| almide_base::env::is_on(&v))
}

/// `dispatch`'s `Commands::Check` arm. Extracted verbatim — `explain` still
/// returns early into the caller via its own `bool` return (`true` = already
/// handled, caller should return).
fn dispatch_check(file: Option<String>, deny_warnings: bool, json: bool, explain: Option<String>, effects: bool, timings: bool, stamp: bool, profile: Option<String>, allow: Vec<String>, target: Option<String>) {
    if let Some(code) = explain {
        print_error_explanation(&code);
        return;
    }
    // #567: `--profile critical` — validate the profile name and expand the
    // capability grants to module names HERE, so the checker below the CLI
    // never sees capability vocabulary.
    let critical: Option<Vec<String>> = match profile.as_deref() {
        None => {
            if !allow.is_empty() {
                eprintln!("error: --allow requires --profile critical");
                std::process::exit(1);
            }
            None
        }
        Some("critical") => Some(expand_capability_grants(&allow)),
        Some(other) => {
            eprintln!("error: unknown profile `{other}` — the only profile is `critical`");
            std::process::exit(1);
        }
    };
    let wasm_target = match target.as_deref() {
        None => false,
        Some("wasm") => true,
        Some(other) => {
            eprintln!("error: `almide check --target` accepts only `wasm` (got `{other}`) — the native target is what `almide check` already judges");
            std::process::exit(1);
        }
    };
    // #2165: the bare form inside a package judges EVERY entry under `src/`,
    // not the first one `resolve_file` happens to find. `--json` walks the
    // same entries in the same order (#2253): every row already names its
    // `file`, so the report needs no other multi-file shape. `--effects` is a
    // per-file report and keeps the single-entry resolution.
    let package_entries = if file.is_none() && !effects { package_check_entries() } else { None };
    let file = match &package_entries {
        Some(entries) => entries[0].clone(),
        None => resolve_file(file),
    };
    if wasm_target && (effects || json) {
        eprintln!("error: --target wasm is not supported with --effects or --json");
        std::process::exit(1);
    }
    if effects {
        if critical.is_some() {
            eprintln!("error: --profile is not supported with --effects");
            std::process::exit(1);
        }
        cli::cmd_check_effects(&file);
    } else if json {
        match &package_entries {
            Some(entries) => cli::cmd_check_json_package(entries, critical.as_deref()),
            None => cli::cmd_check_json(&file, critical.as_deref()),
        }
    } else if let Some(entries) = package_entries {
        cli::cmd_check_package(&entries, deny_warnings, timings, stamp, critical.as_deref(), wasm_target);
    } else {
        cli::cmd_check(&file, deny_warnings, timings, stamp, critical.as_deref(), wasm_target);
    }
}

/// Every `.almd` under `src/` when the cwd is a package, `src/mod.almd` and
/// `src/main.almd` first so the output reads in the order a reader expects.
/// `None` outside a package, or when `src/` holds no `.almd` at all — both
/// fall through to `resolve_file`, whose messages already cover them.
fn package_check_entries() -> Option<Vec<String>> {
    if !std::path::Path::new("almide.toml").exists() {
        return None;
    }
    let mut found: Vec<String> = Vec::new();
    collect_almd(std::path::Path::new("src"), &mut found);
    if found.is_empty() {
        return None;
    }
    found.sort();
    let mut ordered: Vec<String> = Vec::new();
    for lead in ["src/mod.almd", "src/main.almd"] {
        if let Some(pos) = found.iter().position(|f| f == lead) {
            ordered.push(found.remove(pos));
        }
    }
    ordered.extend(found);
    Some(ordered)
}

fn collect_almd(dir: &std::path::Path, out: &mut Vec<String>) {
    let Ok(entries) = std::fs::read_dir(dir) else { return };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            collect_almd(&path, out);
        } else if path.extension().and_then(|e| e.to_str()) == Some("almd") {
            out.push(path.to_string_lossy().replace('\\', "/"));
        }
    }
}

/// `--allow` capability names → the effect-module names the bounded checker
/// denies (E076). The table is `check::CAPABILITY_GRANTS`, held next to the
/// denied set so the two are tested against each other and the registry.
fn expand_capability_grants(allow: &[String]) -> Vec<String> {
    let mut modules: Vec<String> = Vec::new();
    for cap in allow {
        let Some((_, granted)) = almide::check::CAPABILITY_GRANTS.iter().find(|(c, _)| c == cap) else {
            let names: Vec<&str> = almide::check::CAPABILITY_GRANTS.iter().map(|(c, _)| *c).collect();
            eprintln!("error: {}", project::unknown_capability_message(cap, "--allow", &names));
            std::process::exit(1);
        };
        for m in *granted {
            if !modules.iter().any(|x| x == m) {
                modules.push(m.to_string());
            }
        }
    }
    modules
}

/// `dispatch`'s `Commands::Ide` arm (nested `IdeCommand` match). Extracted verbatim.
fn dispatch_ide(cmd: IdeCommand) {
    match cmd {
        IdeCommand::Outline { target, filter, json } => {
            let target = match target {
                Some(t) if t.starts_with("@stdlib/") => t,
                other => resolve_file(other),
            };
            cli::cmd_ide_outline(&target, filter.as_deref(), json);
        }
        IdeCommand::Doc { symbol, file } => {
            let file = resolve_file(file);
            cli::cmd_ide_doc(&symbol, &file);
        }
        IdeCommand::StdlibSnapshot { modules, json } => {
            cli::cmd_ide_stdlib_snapshot(modules.as_deref(), json);
        }
    }
}

/// `dispatch`'s `Commands::Fmt` arm.
///
/// A DIRECTORY argument recurses (`almide fmt --check spec/`) — it used to reach
/// `parse_file` as-is and report "Is a directory", which the always-zero exit then
/// swallowed, so a CI job pointed at a tree silently checked nothing (#919). With no
/// argument at all the `src/` sweep is unchanged.
fn dispatch_fmt(files: Vec<String>, check: bool, json: bool, dry_run: bool, no_import_edit: bool) {
    let mode = match (json, check, dry_run) {
        // `--json` is the machine-readable spelling of the SAME gate: it never
        // writes, and it exits non-zero on drift exactly like `--check`.
        (true, _, _) => cli::FmtMode::CheckJson,
        (false, true, _) => cli::FmtMode::Check,
        (false, false, true) => cli::FmtMode::DryRun,
        (false, false, false) => cli::FmtMode::Write,
    };
    let fmt_files = if files.is_empty() {
        let mut found = Vec::new();
        if std::path::Path::new("src").is_dir() {
            collect_almd_files(std::path::Path::new("src"), &mut found);
        }
        if found.is_empty() {
            err(&format!("No .almd files found in src/"));
            std::process::exit(1);
        }
        found
    } else {
        let mut expanded = Vec::new();
        for f in files {
            let p = std::path::Path::new(&f);
            if p.is_dir() {
                collect_almd_files(p, &mut expanded);
            } else {
                expanded.push(f);
            }
        }
        expanded
    };
    cli::cmd_fmt(&fmt_files, mode, no_import_edit);
}

/// `dispatch`'s `Commands::Add` arm. Extracted verbatim.
///
/// The dependency is fetched BEFORE almide.toml is written, so a `--subdir`
/// (#3381) or a tag the repository does not have leaves the manifest as it
/// was rather than holding an entry no command can resolve.
fn dispatch_add(pkg: String, git: Option<String>, tag: Option<String>, subdir: Option<String>) {
    let subdir = subdir.map(|s| {
        project::normalize_subdir(&s).unwrap_or_else(|why| {
            err(&format!("invalid --subdir `{s}`: {why}"));
            std::process::exit(1);
        })
    });
    let (name, git_url, tag) = project_fetch::resolve_add_target(pkg, git, tag, subdir.as_deref());
    let dep = project::Dependency {
        name: name.clone(),
        git: git_url.clone(),
        tag: tag.clone(),
        branch: None,
        version: None,
        path: None,
        subdir: subdir.clone(),
        declared_at: None,
    };
    project_fetch::fetch_dep(&dep)
        .unwrap_or_else(|e| { err(&format!("{}", e)); std::process::exit(1); });
    project_fetch::add_dep_to_toml(&name, &git_url, tag.as_deref(), subdir.as_deref())
        .unwrap_or_else(|e| { err(&e.to_string()); std::process::exit(1); });
}

/// `dispatch`'s `Commands::Update` arm (#1131): the sanctioned path FORWARD
/// for a locked git dependency — `add` re-pins the old commit and `clean`
/// only clears the cache, so before this the sole escape was hand-editing
/// the lock the file itself says not to edit.
fn dispatch_update(dep: Option<String>) {
    if !std::path::Path::new("almide.toml").exists() {
        err(&format!("No almide.toml found"));
        std::process::exit(1);
    }
    let proj = project::parse_toml(std::path::Path::new("almide.toml"))
        .unwrap_or_else(|e| { err(&format!("{}", e)); std::process::exit(1); });
    let changed = project_fetch::update_locked_deps(&proj, dep.as_deref())
        .unwrap_or_else(|e| { err(&format!("{}", e)); std::process::exit(1); });
    if changed.is_empty() {
        out(&format!("No dependencies updated"));
        return;
    }
    for (name, before, after) in &changed {
        let short = |h: &String| h.chars().take(12).collect::<String>();
        match before.is_empty() {
            true => out(&format!("{} -> {}", name, short(after))),
            false => out(&format!("{} {} -> {}", name, short(before), short(after))),
        }
    }
}

/// `dispatch`'s `Commands::Deps` arm. Extracted verbatim.
fn dispatch_deps() {
    if std::path::Path::new("almide.toml").exists() {
        let proj = project::parse_toml(std::path::Path::new("almide.toml"))
            .unwrap_or_else(|e| { err(&format!("{}", e)); std::process::exit(1); });
        if proj.dependencies.is_empty() {
            out(&format!("No dependencies"));
        } else {
            for dep in &proj.dependencies {
                if let Some(ref path) = dep.path {
                    out(&format!("{} = path {}", dep.name, path));
                    continue;
                }
                let ref_name = dep.tag.as_deref().or(dep.branch.as_deref()).unwrap_or("main");
                match &dep.subdir {
                    Some(sub) => out(&format!("{} = {} ({}) subdir {}", dep.name, dep.git, ref_name, sub)),
                    None => out(&format!("{} = {} ({})", dep.name, dep.git, ref_name)),
                }
            }
        }
    } else {
        err(&format!("No almide.toml found"));
    }
}

/// `dispatch`'s `Commands::DepPath` arm. Extracted verbatim.
fn dispatch_dep_path(name: String) {
    if !std::path::Path::new("almide.toml").exists() {
        err(&format!("No almide.toml found"));
        std::process::exit(1);
    }
    let proj = project::parse_toml(std::path::Path::new("almide.toml"))
        .unwrap_or_else(|e| { err(&format!("{}", e)); std::process::exit(1); });
    let fetched = project_fetch::fetch_all_deps(&proj)
        .unwrap_or_else(|e| { err(&format!("{}", e)); std::process::exit(1); });
    match fetched.iter().find(|fd| fd.pkg_id.name == name) {
        Some(fd) => out(&format!("{}", fd.source_dir.display())),
        None => {
            err(&format!("Dependency '{}' not found in almide.toml", name));
            std::process::exit(1);
        }
    }
}

/// `dispatch`'s second half: the "tooling" commands (LSP, diagnostics
/// explain, IDE queries, fmt, compile, clean, package management, self
/// update, emit). Split out of `dispatch`'s single flat match — cyclomatic
/// complexity counts one branch per match arm regardless of how thin the
/// arm body is, and `Commands` has ~19 variants, so the single match alone
/// tripped the threshold. Extracted verbatim; the split point is arbitrary
/// (arm count, not domain semantics) — `other` is exhaustive over exactly
/// the variants `dispatch`'s own match doesn't handle.
fn dispatch_rest(command: Commands) {
    match command {
        Commands::Lsp => {
            cli::lsp::run_lsp();
        }
        Commands::Mcp => {
            cli::mcp::run_mcp();
        }
        Commands::Explain { code, list, json } => match code {
            Some(code) if !list => print_error_explanation(&code),
            _ => cli::explain::print_list(DIAGNOSTIC_DOCS, json),
        },
        Commands::Ide { cmd } => dispatch_ide(cmd),
        Commands::Fmt { files, check, json, dry_run, no_import_edit } => dispatch_fmt(files, check, json, dry_run, no_import_edit),
        Commands::Compile { module, json, dry_run, output } => {
            cli::cmd_compile(module.as_deref(), json, dry_run, output.as_deref());
        }
        Commands::Clean => cli::cmd_clean(),
        Commands::Add { pkg, git, tag, subdir } => dispatch_add(pkg, git, tag, subdir),
        Commands::Update { dep } => dispatch_update(dep),
        Commands::Deps => dispatch_deps(),
        Commands::DepPath { name } => dispatch_dep_path(name),
        Commands::Install { spec, tag, branch, name, bin_dir, target } => {
            cli::cmd_install(
                &spec,
                tag.as_deref(),
                branch.as_deref(),
                name.as_deref(),
                bin_dir.as_deref(),
                target.as_deref(),
            );
        }
        Commands::SelfUpdate { version } => {
            cli::cmd_self_update(version.as_deref());
        }
        Commands::Verify { args } => std::process::exit(cli::cmd_verify(&args)),
        Commands::Survive { file, with, as_kind, json, timeout } => {
            cli::cmd_survive(cli::SurviveArgs { file, with, as_kind, json, timeout_secs: timeout });
        }
        Commands::Apply { file, with, as_kind, if_survives, force, json, timeout } => {
            cli::cmd_apply(cli::SurviveArgs { file, with, as_kind, json, timeout_secs: timeout }, if_survives, force);
        }
        Commands::SurviveTestLeg { file } => cli::cmd_survive_test_leg(&file),
        Commands::Emit { file, target, emit_ast, emit_ir, emit_dialect, no_check, repr_c, trace_map } => {
            cli::cmd_emit(cli::EmitArgs { file: &file, target: &target, emit_ast, emit_ir, emit_dialect, no_check, repr_c, trace_map });
        }
        // `command`'s static type is the full `Commands` enum — Rust can't
        // narrow it to "one of the 12 variants `dispatch` doesn't handle"
        // across the function boundary, so this match must stay exhaustive.
        // `dispatch`'s own match already handles the other 7 variants
        // before ever calling this function, so this arm is genuinely
        // unreachable at runtime.
        _ => unreachable!("dispatch's match should have handled this Commands variant"),
    }
}

/// Refuse a `./almide.toml` that declares a key twice (#2583), is not TOML
/// (#3253), or whose `[permissions].allow` names something that is not a
/// capability (#3247), before any command runs. Most readers of the manifest treat a parse error as "no
/// project" (`parse_toml(..).ok()`), which is right for a missing file but
/// would turn this error into a silent run without dependencies; one gate
/// here makes the refusal the same on every command. The commands that must
/// keep working in a broken project are exempt: `init`, `clean`, the editor
/// servers (an exit would kill the session; their manifest reads already
/// fail closed), and the ones that never read the manifest.
fn refuse_invalid_manifest(command: &Commands) {
    if matches!(
        command,
        Commands::Init
            | Commands::Clean
            | Commands::Lsp
            | Commands::Mcp
            | Commands::SelfUpdate { .. }
            | Commands::DocsGen { .. }
            | Commands::Switches { .. }
            | Commands::Explain { .. }
    ) {
        return;
    }
    let path = std::path::Path::new("almide.toml");
    let Ok(content) = std::fs::read_to_string(path) else { return };
    if let Err(e) = project::check_manifest(path, &content) {
        err(&format!("error: {}", e));
        std::process::exit(1);
    }
    report_manifest_warnings(command, path, &content);
}

/// A key the manifest writes and no reader reads (#3382) is a warning, once
/// per command, printed here with the manifest refusals above. `check
/// --json` carries it as a diagnostic row on stdout; every other command
/// prints it on stderr, so no other machine-readable stdout changes. The
/// exit code never changes: the build runs exactly as if the key were absent.
/// `survive-test-leg` is a child of `survive`, which already warned.
fn report_manifest_warnings(command: &Commands, path: &std::path::Path, content: &str) {
    if matches!(command, Commands::SurviveTestLeg { .. }) {
        return;
    }
    let json = matches!(command, Commands::Check { json: true, .. });
    for d in project::manifest_warnings(path, content) {
        if json {
            out(&diagnostic_render::to_json(&d));
        } else {
            err(&diagnostic_render::display(&d));
        }
    }
}

pub(crate) fn dispatch(cli: crate::cli_args::Cli) {
    if cli.verbose {
        // Deep pipeline code reads the env var so verbosity needs no
        // plumbing through every call chain (same pattern as
        // ALMIDE_FUEL_PROBE above).
        // SAFETY: single-threaded at this point (process entry, pre-dispatch).
        unsafe { std::env::set_var("ALMIDE_VERBOSE", "1") };
    }
    let command = match cli.command {
        Some(cmd) => cmd,
        None => {
            cli::repl::run_repl();
            return;
        }
    };
    refuse_invalid_manifest(&command);
    match command {
        Commands::Init => cli::cmd_init(),
        Commands::Run { file, no_check, release, target, verified: _, no_verified, time_report, program_args } =>
            dispatch_run(file, no_check, release, target, no_verified, time_report, program_args),
        Commands::Bench { file, runs, target, program_args } => {
            let file = resolve_file(file);
            cli::cmd_bench(&file, runs, target.as_deref(), &program_args);
        }
        Commands::Build { file, o, target, release, fast, unchecked_index, no_check, repr_c, cdylib, emit_unverified, verified: _, no_verified, wasm_opt, component, heap_cap, host, debug } => {
            let file = resolve_file(file);
            warn_no_verified_deprecated(no_verified);
            cli::cmd_build(cli::BuildArgs {
                file: &file,
                output: o.as_deref(),
                target: target.as_deref(),
                release: release || fast,
                fast,
                unchecked_index,
                no_check,
                repr_c,
                cdylib,
                emit_unverified,
                verified: !no_verified,
                native_verified: !no_verified,
                wasm_opt,
                component,
                heap_cap,
                host: host.as_deref(),
                debug,
                });
        }
        Commands::Test { file, run, no_check, json, target, update_snapshots, ci, allow_no_tests, show_output } => {
            dispatch_test(TestArgs { file, run, no_check, json, target, update_snapshots, ci, allow_no_tests, show_output })
        }
        Commands::Check { file, deny_warnings, json, explain, effects, timings, stamp, profile, allow, target } => dispatch_check(file, deny_warnings, json, explain, effects, timings, stamp, profile, allow, target),
        Commands::Fix { file, dry_run, json } => {
            let file = resolve_file(file);
            cli::cmd_fix(&file, dry_run, json);
        }
        Commands::DocsGen { check } => {
            cli::cmd_docs_gen(check);
        }
        Commands::Switches { md } => {
            print!("{}", if md { almide_base::env::markdown_table() } else { almide_base::env::plain_table() });
        }
        other => dispatch_rest(other),
    }
}
