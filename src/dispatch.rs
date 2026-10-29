//! From the parsed command line to the command that runs it: one `dispatch_*`
//! per subcommand that needs more than a call, and the manifest gate every
//! command but the exempt ones passes first.

use crate::cli;
use crate::cli_args::{Commands, IdeCommand};
use crate::{collect_almd_files, print_error_explanation, resolve_file, warn_no_verified_deprecated, DIAGNOSTIC_DOCS};
use crate::{diagnostic_render, err, out, project, project_fetch};

/// `Commands::Run`'s fields, carried as one value into [`dispatch_run`].
struct RunCmd {
    file: Option<String>,
    no_check: bool,
    release: bool,
    target: Option<String>,
    no_verified: bool,
    time_report: bool,
    program_args: Vec<String>,
}

/// `dispatch`'s `Commands::Run` arm.
fn dispatch_run(args: RunCmd) {
    let RunCmd { file, no_check, release, target, no_verified, time_report, program_args } = args;
    let file = resolve_file(file);
    if time_report {
        // The deterministic meter is the probe machinery: setting the env here
        // (before any compile thread exists) makes the pipeline insert charge
        // ops, and the run leg formats the probe line into the D5 dual-time
        // report instead of printing it raw.
        // SAFETY: single-threaded at this point (process entry, pre-dispatch).
        unsafe { almide_base::env::arm("ALMIDE_FUEL_PROBE", "1") };
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

/// `Commands::Check`'s fields, carried as one value into [`dispatch_check`].
struct CheckCmd {
    file: Option<String>,
    deny_warnings: bool,
    json: bool,
    explain: Option<String>,
    effects: bool,
    timings: bool,
    stamp: bool,
    profile: Option<String>,
    allow: Vec<String>,
    target: Option<String>,
}

/// `dispatch`'s `Commands::Check` arm. `--explain` prints the code's text
/// and nothing else runs.
fn dispatch_check(args: CheckCmd) {
    let CheckCmd { file, deny_warnings, json, explain, effects, timings, stamp, profile, allow, target } = args;
    if let Some(code) = explain {
        print_error_explanation(&code);
        return;
    }
    let critical = critical_profile_grants(profile.as_deref(), &allow);
    if critical.is_some() {
        refuse_build_switches_under_critical();
    }
    let wasm_target = check_wasm_target(target.as_deref());
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

/// #567: `--profile critical` — validate the profile name and expand the
/// capability grants to module names HERE, so the checker below the CLI
/// never sees capability vocabulary. `None` is the ordinary (unbounded) check.
fn critical_profile_grants(profile: Option<&str>, allow: &[String]) -> Option<Vec<String>> {
    match profile {
        None => {
            if !allow.is_empty() {
                eprintln!("error: --allow requires --profile critical");
                std::process::exit(1);
            }
            None
        }
        Some("critical") => Some(expand_capability_grants(allow)),
        Some(other) => {
            eprintln!("error: unknown profile `{other}` — the only profile is `critical`");
            std::process::exit(1);
        }
    }
}

/// #4129: `--profile critical` judges the program as written, so an inherited
/// switch that changes what is compiled or which check runs is refused, not
/// ignored — `--allow-switch` does not lift it.
fn refuse_build_switches_under_critical() {
    let on = almide_base::env::inherited_build_switches();
    if !on.is_empty() {
        eprintln!(
            "error: --profile critical refuses a build-changing environment switch: {} (unset it; `almide switches` says what each does)",
            on.join(", ")
        );
        std::process::exit(1);
    }
}

/// `almide check --target`: `wasm` adds the wasm build route's verdicts;
/// nothing else is accepted.
fn check_wasm_target(target: Option<&str>) -> bool {
    match target {
        None => false,
        Some("wasm") => true,
        Some(other) => {
            eprintln!("error: `almide check --target` accepts only `wasm` (got `{other}`) — the native target is what `almide check` already judges");
            std::process::exit(1);
        }
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
        // A symlink is neither descended nor collected (#4140).
        let Ok(kind) = entry.file_type() else { continue };
        if kind.is_dir() {
            collect_almd(&path, out);
        } else if kind.is_file() && path.extension().and_then(|e| e.to_str()) == Some("almd") {
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
/// `Commands::Add`'s fields, carried as one value into [`dispatch_add`].
struct AddCmd {
    pkg: String,
    git: Option<String>,
    tag: Option<String>,
    subdir: Option<String>,
    trust: Option<String>,
    dry_run: bool,
}

/// `dispatch`'s `Commands::Add` arm (#4093). In order: resolve the spec to a
/// source, check its owner against `[trust] owners` (and `--trust`), probe
/// that the repository exists, fetch it, and only then write almide.toml and
/// almide.lock. Any refusal or failure exits 1 and leaves both files as they
/// were.
fn dispatch_add(cmd: AddCmd) {
    let fail = |msg: &str| -> ! {
        err(msg);
        std::process::exit(1);
    };
    let manifest = std::path::Path::new("almide.toml");
    if !manifest.exists() {
        fail("almide.toml not found. Run 'almide init' first.");
    }
    let proj = project::parse_toml(manifest).unwrap_or_else(|e| fail(&e));
    let subdir = cmd.subdir.as_ref().map(|s| {
        project::normalize_subdir(s).unwrap_or_else(|why| fail(&format!("invalid --subdir `{s}`: {why}")))
    });
    let retry = add_command_line(&cmd);
    let (name, git_url, tag) = project_fetch::resolve_add_target(cmd.pkg, cmd.git, cmd.tag, subdir.as_deref());
    let owner = crate::trust::source_owner(&git_url).unwrap_or_else(|e| fail(&format!("error: {e}")));
    let granted = match (&cmd.trust, &owner) {
        (Some(t), Some(o)) if crate::trust::owner_arg(t) == *o => true,
        (Some(t), Some(o)) => fail(&format!(
            "error: --trust {t} does not match the owner of {git_url} ({o})\n  \
             hint: --trust must name the owner the source belongs to; check that this is the repository the user asked for"
        )),
        (Some(t), None) => fail(&format!("error: --trust {t}: {git_url} is on the local filesystem, which needs no trust")),
        (None, _) => false,
    };
    let verdict = match &owner {
        None => "a local source (no trust needed)".to_string(),
        Some(o) if o == crate::trust::DEFAULT_TRUSTED_OWNER => format!("owner {o}, trusted by default"),
        Some(o) if crate::trust::is_trusted(&proj.trust_owners, o) => format!("owner {o}, trusted by [trust] owners"),
        Some(o) if granted => format!("owner {o}, trusted by --trust"),
        Some(o) => format!("owner {o}, NOT trusted"),
    };
    err(&format!("Resolved {name} → {git_url} ({verdict})"));
    if let Some(o) = owner.as_deref().filter(|o| !granted && !crate::trust::is_trusted(&proj.trust_owners, o)) {
        fail(&crate::trust::add_refusal(&name, &git_url, o, &retry));
    }
    match project_fetch::probe_source(&git_url) {
        Ok(()) => {}
        Err(project_fetch::ProbeFailure::NotFound(detail)) => {
            let hint = crate::trust::suggestion(&git_url).unwrap_or_else(|| {
                format!("hint: no package under github.com/almide is close to `{name}`; check the name with the user")
            });
            fail(&format!(
                "error: {git_url} does not exist (or is not visible to you)\n  git: {}\n  {hint}",
                detail.lines().last().unwrap_or("")
            ));
        }
        Err(project_fetch::ProbeFailure::Unreachable(detail)) => {
            fail(&format!("error: could not reach {git_url}\n  git: {detail}"));
        }
    }
    if cmd.dry_run {
        let tag_part = tag.as_deref().map(|t| format!(" at {t}")).unwrap_or_default();
        out(&format!("would add {name} = {git_url}{tag_part} ({verdict}); the repository exists; nothing written (--dry-run)"));
        return;
    }
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
    project_fetch::fetch_dep(&dep).unwrap_or_else(|e| fail(&e));
    // Write the manifest (and the grant), then lock it the way a build
    // would. Any failure puts both files back.
    let lock = proj.root.join("almide.lock");
    let saved_manifest = project::read_project_file_bytes(manifest).ok();
    let saved_lock = project::read_project_file_bytes(&lock).ok();
    let restore = || {
        if let Some(bytes) = &saved_manifest {
            let _ = std::fs::write(manifest, bytes);
        }
        match &saved_lock {
            Some(bytes) => { let _ = std::fs::write(&lock, bytes); }
            None => { let _ = std::fs::remove_file(&lock); }
        }
    };
    let written = (|| -> Result<bool, String> {
        let trusted_now = match (granted, owner.as_deref()) {
            (true, Some(o)) => project::add_trust_owner(manifest, o)?,
            _ => false,
        };
        project_fetch::add_dep_to_toml(&name, &git_url, tag.as_deref(), subdir.as_deref())?;
        let updated = project::parse_toml(manifest)?;
        project_fetch::fetch_all_deps(&updated)?;
        Ok(trusted_now)
    })();
    match written {
        Ok(trusted_now) => {
            if trusted_now {
                err(&format!("Trusted {} in almide.toml [trust] owners", owner.as_deref().unwrap_or("")));
            }
            err(&format!("Added {name} to almide.toml"));
        }
        Err(e) => {
            restore();
            fail(&format!("{e}\n  note: almide.toml and almide.lock are unchanged"));
        }
    }
}

/// The `almide add` command line `cmd` was, for the refusal's retry hint.
fn add_command_line(cmd: &AddCmd) -> String {
    let mut line = format!("almide add {}", cmd.pkg);
    for (flag, value) in [("--git", &cmd.git), ("--tag", &cmd.tag), ("--subdir", &cmd.subdir)] {
        if let Some(v) = value {
            line.push_str(&format!(" {flag} {v}"));
        }
    }
    line
}

/// `dispatch`'s `Commands::Trust` arm (#4093): add an owner to `[trust]
/// owners`, for a dependency that no `almide add` brings (a transitive one,
/// or a hand-written manifest line).
fn dispatch_trust(owner: String) {
    let manifest = std::path::Path::new("almide.toml");
    if !manifest.exists() {
        err("almide.toml not found. Run 'almide init' first.");
        std::process::exit(1);
    }
    let owner = crate::trust::owner_arg(&owner);
    if owner == crate::trust::DEFAULT_TRUSTED_OWNER {
        err(&format!("{owner} is trusted by default; nothing to add"));
        return;
    }
    match project::add_trust_owner(manifest, &owner) {
        Ok(true) => err(&format!("Trusted {owner} in almide.toml [trust] owners")),
        Ok(false) => err(&format!("{owner} is already in [trust] owners")),
        Err(e) => {
            err(&format!("error: {e}"));
            std::process::exit(1);
        }
    }
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
    if !std::path::Path::new("almide.toml").exists() {
        err(&format!("No almide.toml found"));
        return;
    }
    let proj = project::parse_toml(std::path::Path::new("almide.toml"))
        .unwrap_or_else(|e| { err(&format!("{}", e)); std::process::exit(1); });
    if proj.dependencies.is_empty() {
        out(&format!("No dependencies"));
        return;
    }
    // #4093: each package with what it can reach and the native code it
    // brings, so a dependency is judged before `almide build` runs its code.
    let fetched = project_fetch::fetch_all_deps(&proj)
        .unwrap_or_else(|e| { err(&format!("{}", e)); std::process::exit(1); });
    let natives = crate::native_grant::survey(&proj, &fetched);
    let caps = dependency_capabilities_report(&proj);
    for dep in &proj.dependencies {
        out(&dependency_source_line(dep));
        if let Some(n) = natives.iter().find(|n| n.name == dep.name) {
            print_dependency_details(n, caps.as_ref());
        }
    }
    for n in natives.iter().filter(|n| !n.direct) {
        out(&format!("{} (required by another dependency) = {}", n.name, n.package_dir.display()));
        print_dependency_details(n, caps.as_ref());
    }
    // ADR-0032 D7 (Q23): every media owner, and what grants it.
    for line in crate::media_grant::owner_lines(&proj, &crate::media_grant::survey(&proj, &fetched)) {
        out(&line);
    }
}

/// `almide deps`'s first line for a declared dependency: where it comes from.
fn dependency_source_line(dep: &project::Dependency) -> String {
    if let Some(ref path) = dep.path {
        return format!("{} = path {}", dep.name, path);
    }
    let ref_name = dep.tag.as_deref().or(dep.branch.as_deref()).unwrap_or("main");
    match &dep.subdir {
        Some(sub) => format!("{} = {} ({}) subdir {}", dep.name, dep.git, ref_name, sub),
        None => format!("{} = {} ({})", dep.name, dep.git, ref_name),
    }
}

/// The capability sets of every dependency package, measured over a program
/// that imports the direct ones; `None` when they do not compile.
fn dependency_capabilities_report(proj: &project::Project) -> Option<std::collections::HashMap<String, std::collections::BTreeSet<String>>> {
    let dir = std::env::temp_dir().join(format!("almide-deps-{}", std::process::id()));
    std::fs::create_dir_all(&dir).ok()?;
    let entry = dir.join("deps_entry.almd");
    let imports: String = proj.dependencies.iter().map(|d| format!("import {}\n", d.name)).collect();
    std::fs::write(&entry, imports).ok()?;
    crate::SUPPRESS_WARNINGS.store(true, std::sync::atomic::Ordering::Relaxed);
    let caps = crate::compile_driver::dependency_capabilities(&entry.to_string_lossy()).ok();
    crate::SUPPRESS_WARNINGS.store(false, std::sync::atomic::Ordering::Relaxed);
    let _ = std::fs::remove_dir_all(&dir);
    caps
}

/// `almide deps`'s indented lines for one package: version, capabilities,
/// native code and its grant.
fn print_dependency_details(n: &crate::native_grant::DepNative, caps: Option<&std::collections::HashMap<String, std::collections::BTreeSet<String>>>) {
    out(&format!("  version: {}", n.version));
    let caps_text = match caps.map(|c| c.get(&n.name)) {
        None => "unknown (the dependencies do not compile)".to_string(),
        Some(None) => "unknown (no module of it is imported)".to_string(),
        Some(Some(set)) if set.is_empty() => "none".to_string(),
        Some(Some(set)) => set.iter().cloned().collect::<Vec<_>>().join(", "),
    };
    out(&format!("  capabilities: {}", caps_text));
    if n.code.is_empty() {
        out("  native code: none");
    } else if n.granted {
        out(&format!("  native code: {} (granted by [permissions].native)", n.code.describe()));
    } else {
        out(&format!(
            "  native code: {} (NOT granted: build, run and test refuse it until [permissions].native names \"{}\")",
            n.code.describe(), n.name
        ));
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

/// `dispatch`'s tooling commands: the editor and agent servers, diagnostics
/// explain, IDE queries, fmt, compile, emit, verify and the survival checks.
/// Everything else falls through to [`dispatch_packages`].
fn dispatch_tooling(command: Commands) {
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
        Commands::Verify { args } => std::process::exit(cli::cmd_verify(&args)),
        Commands::Survive { file, with, as_kind, json, timeout, allow_removed } => {
            cli::cmd_survive(cli::SurviveArgs { file, with, as_kind, json, timeout_secs: timeout, allow_removed });
        }
        Commands::Apply { file, with, as_kind, if_survives, force, json, timeout, allow_removed } => {
            cli::cmd_apply(cli::SurviveArgs { file, with, as_kind, json, timeout_secs: timeout, allow_removed }, if_survives, force);
        }
        Commands::SurviveTestLeg { file } => cli::cmd_survive_test_leg(&file),
        Commands::Emit { file, target, emit_ast, emit_ir, emit_dialect, no_check, repr_c, trace_map } => {
            cli::cmd_emit(cli::EmitArgs { file: &file, target: &target, emit_ast, emit_ir, emit_dialect, no_check, repr_c, trace_map });
        }
        other => dispatch_packages(other),
    }
}

/// `dispatch`'s package and toolchain management commands: caches,
/// dependencies, installs and self-update.
fn dispatch_packages(command: Commands) {
    match command {
        Commands::Clean => cli::cmd_clean(),
        Commands::Add { pkg, git, tag, subdir, trust, dry_run } => dispatch_add(AddCmd { pkg, git, tag, subdir, trust, dry_run }),
        Commands::Trust { owner } => dispatch_trust(owner),
        Commands::Update { dep } => dispatch_update(dep),
        Commands::Deps => dispatch_deps(),
        Commands::DepPath { name } => dispatch_dep_path(name),
        Commands::Install { spec, tag, branch, name, bin_dir, target, force, trust, allow_native } => {
            cli::cmd_install(
                &spec,
                tag.as_deref(),
                branch.as_deref(),
                name.as_deref(),
                bin_dir.as_deref(),
                target.as_deref(),
                cli::InstallGrants { force, trust: trust.as_deref(), allow_native },
            );
        }
        Commands::SelfUpdate { version, skip_attestation } => {
            cli::cmd_self_update(version.as_deref(), skip_attestation);
        }
        // `command`'s static type is the full `Commands` enum — Rust can't
        // narrow it to the variants `dispatch` and `dispatch_tooling` leave
        // unhandled across the function boundary, so this match must stay
        // exhaustive. Both handle every other variant before calling this
        // function, so this arm is genuinely unreachable at runtime.
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
    if !path.exists() {
        return;
    }
    // Bounded and regular-file only (#4194): a manifest linked to /dev/zero
    // is refused here, on every command, instead of read until memory runs out.
    let content = match project::read_project_file(path) {
        Ok(c) => c,
        Err(e) => {
            err(&format!("error: {}", e));
            std::process::exit(1);
        }
    };
    if let Err(e) = project::check_manifest(path, &content) {
        err(&format!("error: {}", e));
        std::process::exit(1);
    }
    refuse_older_compiler(command, path);
    report_manifest_warnings(command, path, &content);
}

/// #4172: the package's `almide = "X.Y.Z"` minimum holds on every command
/// that checks or compiles it — not only the native build, whose compile
/// driver was the one reader. A package pinning the release that carries a
/// fix (a soundness or a permission fix) must not be built, checked or run
/// by an older compiler on any target. Package management (`add`, `deps`,
/// `update`, `install`) and `fmt` still run, so the pin can be read and
/// changed.
fn refuse_older_compiler(command: &Commands, path: &std::path::Path) {
    let compiles = matches!(
        command,
        Commands::Run { .. }
            | Commands::Bench { .. }
            | Commands::Build { .. }
            | Commands::Test { .. }
            | Commands::Check { .. }
            | Commands::Compile { .. }
            | Commands::Fix { .. }
            | Commands::Survive { .. }
            | Commands::Apply { .. }
            | Commands::SurviveTestLeg { .. }
            | Commands::Emit { .. }
            | Commands::Verify { .. }
    );
    if !compiles {
        return;
    }
    let Ok(proj) = project::parse_toml(path) else { return };
    if let Err(e) = project::check_compiler_version(&proj) {
        err(&format!("error: {}", e));
        std::process::exit(1);
    }
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

/// #4129: a release build locks the switches that change what is compiled or
/// which check runs — an inherited one is ignored unless `--allow-switch`
/// names it — and `almide survive`'s overlay arrives only as an argument.
/// Before any switch is read.
fn arm_switch_lock(cli: &crate::cli_args::Cli) {
    for name in &cli.allow_switch {
        if let Err(e) = almide_base::env::check_allowed(name) {
            eprintln!("error: --allow-switch: {e}");
            std::process::exit(1);
        }
    }
    if cli.lock_switches || crate::cli_args::is_release_build() {
        almide_base::env::lock(cli.allow_switch.clone());
    }
    if let (Some(target), Some(text)) = (&cli.survive_overlay, &cli.survive_overlay_text) {
        almide::source_overlay::arm_from_args(target, text);
    }
}

pub(crate) fn dispatch(cli: crate::cli_args::Cli) {
    arm_switch_lock(&cli);
    if cli.verbose {
        // Deep pipeline code reads the env var so verbosity needs no
        // plumbing through every call chain (same pattern as
        // ALMIDE_FUEL_PROBE above).
        // SAFETY: single-threaded at this point (process entry, pre-dispatch).
        unsafe { std::env::set_var("ALMIDE_VERBOSE", "1") };
    }
    for dir in &cli.stdlib_authority {
        if let Err(e) = almide::check::grant_stdlib_authority(dir) {
            err(&format!("error: {e}"));
            std::process::exit(2);
        }
    }
    let command = match cli.command {
        Some(cmd) => cmd,
        None => {
            cli::repl::run_repl();
            return;
        }
    };
    refuse_invalid_manifest(&command);
    // #4093: `--locked` holds for every fetch this process makes.
    if matches!(command, Commands::Run { locked: true, .. } | Commands::Build { locked: true, .. } | Commands::Test { locked: true, .. }) {
        project_fetch::set_locked_mode(true);
    }
    // #4093: a command that builds resolves the dependencies first, before
    // anything is compiled and on every `--target`: an untrusted owner, a
    // source changed under the lock, a `--locked` miss, and a dependency's
    // ungranted native code (compiled and run by cargo, outside every
    // `[permissions]` check) all stop it here.
    if matches!(
        command,
        Commands::Run { .. } | Commands::Test { .. } | Commands::Bench { .. }
            | Commands::Build { print_release_rustflags: false, .. }
    ) {
        cli::resolve_cwd_deps_or_exit();
    }
    if matches!(command, Commands::Check { .. }) {
        cli::check_cwd_media_grants_or_exit();
    }
    match command {
        Commands::Init => cli::cmd_init(),
        Commands::Run { file, no_check, locked: _, release, target, verified: _, no_verified, time_report, program_args } =>
            dispatch_run(RunCmd { file, no_check, release, target, no_verified, time_report, program_args }),
        Commands::Bench { file, runs, target, program_args } => {
            let file = resolve_file(file);
            cli::cmd_bench(&file, runs, target.as_deref(), &program_args);
        }
        Commands::Build { file, o, target, release, fast, target_cpu, print_release_rustflags, unchecked_index, no_check, locked: _, repr_c, cdylib, emit_unverified, verified: _, no_verified, wasm_opt, component, heap_cap, host, debug } => {
            // `--print-release-rustflags` builds nothing, so it needs no source file.
            let file = if print_release_rustflags { file.unwrap_or_default() } else { resolve_file(file) };
            warn_no_verified_deprecated(no_verified);
            cli::cmd_build(cli::BuildArgs {
                file: &file,
                output: o.as_deref(),
                target: target.as_deref(),
                release: release || fast,
                fast,
                target_cpu: target_cpu.as_deref(),
                print_release_rustflags,
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
        Commands::Test { file, run, no_check, locked: _, json, target, update_snapshots, ci, allow_no_tests, show_output } => {
            dispatch_test(TestArgs { file, run, no_check, json, target, update_snapshots, ci, allow_no_tests, show_output })
        }
        Commands::Check { file, deny_warnings, json, explain, effects, timings, stamp, profile, allow, target } => {
            dispatch_check(CheckCmd { file, deny_warnings, json, explain, effects, timings, stamp, profile, allow, target })
        }
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
        other => dispatch_tooling(other),
    }
}
