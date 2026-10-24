/// CLI command implementations.

mod run;
mod build_dir;
pub(crate) mod build;
mod fs_scope_twin;
mod build_wasm;
mod wasm_compile;
mod bench;
mod compile;
mod emit;
mod check;
mod dialect_stamp;
mod commands;
mod test_wasm;
mod fmt_command;
mod clean;
mod worker_pool;
mod test_scratch;
mod cache_bound;
pub mod test_report;
mod test_output;
pub mod snapshot;
mod install;
mod selfupdate;
mod verify;
pub mod lsp;
pub mod mcp;
mod mcp_tools;
pub mod repl;
mod ide;
mod fix;
pub mod explain;
mod survive;
mod survive_delta;
mod survive_edit;
mod survive_legs;
mod docs_gen;
mod cargo_build;
pub(crate) mod rustc_floor;
mod cargo_ice;
mod cargo_toml;
mod crate_inputs;
mod native_target;
mod js_host;
mod wasm_debug;

// `cargo_build_cdylib`/`cargo_build_generated`/`cargo_build_generated_with_native`/
// `cargo_build_test_with_native` are called from sibling modules (`build.rs`,
// `repl.rs`, `run.rs`) as `super::cargo_build_*` — re-exported here (private,
// visible to `cli`'s descendants) so those call sites don't need to change.
use cargo_build::{cargo_build_cdylib, cargo_build_generated, cargo_build_generated_with_native, cargo_build_test_with_native};

pub use run::{cmd_run, RunArgs};
pub use build::{cmd_build, BuildArgs};
pub use bench::cmd_bench;
pub use compile::cmd_compile;
pub use emit::{cmd_emit, EmitArgs};
pub use check::{cmd_check, cmd_check_package, cmd_check_json, cmd_check_json_package, cmd_check_effects};
pub use commands::{cmd_init, cmd_test, cmd_test_fast, cmd_test_json, cmd_test_update_snapshots};
pub use test_wasm::cmd_test_wasm;
pub use fmt_command::{cmd_fmt, FmtMode};
pub use clean::cmd_clean;
pub use install::cmd_install;
pub use selfupdate::cmd_self_update;
pub use verify::cmd_verify;
pub use ide::{cmd_ide_outline, cmd_ide_doc, cmd_ide_stdlib_snapshot};
pub use fix::cmd_fix;
pub use survive::{cmd_apply, cmd_survive, SurviveArgs};
pub use survive_legs::cmd_survive_test_leg;
pub use docs_gen::cmd_docs_gen;

use std::hash::{Hash, Hasher};
use crate::err;

/// Check that all effects used in the program are allowed by [permissions].allow in almide.toml.
/// Returns Ok(()) if no violations, or Err with a description of violations.
pub fn check_permissions(ir: &almide::ir::IrProgram, permissions: &[String]) -> Result<(), String> {
    use almide::codegen::pass_effect_inference::EffectInferencePass;
    use almide::codegen::pass::NanoPass;

    let allowed = allowed_permissions_or_report(permissions)?;
    let result = EffectInferencePass.run(ir.clone(), almide::codegen::pass::Target::Rust);
    let ir_after = result.program;

    // Report in name order so the same program prints the same report on every run and route.
    // A stdlib function is judged where the program uses it: its set is
    // charged to every caller. Judging it as a root refused a program for the
    // `panic` inside an unused `list.range_step` (#4155: `Abort` is a leaf).
    let is_stdlib_fn = |name: &str| {
        name.split_once('.').is_some_and(|(m, _)| almide::stdlib::is_stdlib_module(m) || almide::stdlib::is_bundled_module(m))
    };
    let mut functions: Vec<_> = ir_after.effect_map.functions.iter().filter(|(n, _)| !is_stdlib_fn(n)).collect();
    functions.sort_by(|a, b| a.0.cmp(b.0));
    let mut violations = 0;
    for (name, fe) in functions {
        let mut forbidden: Vec<_> = fe.transitive.iter()
            .filter(|e| !allowed.contains(e))
            .collect();
        forbidden.sort_by_key(|e| e.to_string());
        if !forbidden.is_empty() {
            err(&format!("error: capability violation in `{}`", name));
            for e in &forbidden {
                err(&format!("  {} is not in [permissions].allow", e));
                // ADR-0026 D4: the path the category reaches this function by.
                if let Some(path) = fe.paths.get(*e) {
                    err(&format!("  path: {path}"));
                }
                if let Some(note) = epoch15_category_note(**e, &allowed) {
                    err(&format!("  note: {note}"));
                }
            }
            violations += 1;
        }
    }
    if violations > 0 {
        err(&format!("\n{} capability violation(s)", violations));
        return Err(format!("{} capability violation(s)", violations));
    }
    Ok(())
}

/// What a manifest written before dialect epoch 15 meant by a name that now
/// means less (#4155, ADR-0026 D2): `IO` was files and the standard streams
/// and is the streams only, `Env` admitted starting a process and no longer
/// does. Said once per refused leaf the old name would have covered.
pub(crate) fn epoch15_category_note(
    e: almide::ir::effect::Effect,
    allowed: &std::collections::HashSet<almide::ir::effect::Effect>,
) -> Option<&'static str> {
    use almide::ir::effect::Effect;
    match e {
        Effect::FsRead | Effect::FsWrite if allowed.contains(&Effect::IoStdout) => Some(
            "files are `FS` (`FS.read`, `FS.write`) since dialect epoch 15; `IO` is the standard streams only",
        ),
        Effect::Proc if allowed.contains(&Effect::EnvRead) => Some(
            "starting a process is `Proc` since dialect epoch 15 — `Env` no longer grants it, because a started \
             command can reach every other capability",
        ),
        _ => None,
    }
}

/// `[permissions].allow` granting `Proc` with no `proc = [...]` command list
/// (#4155): the program may start any command, and through it reach every
/// other capability. A warning on every route; refused when `refuse` (the
/// `--profile critical` check).
pub fn proc_grant_without_list(proj: &crate::project::Project, refuse: bool) -> Result<(), String> {
    let Some(allow) = &proj.permissions else { return Ok(()) };
    let Ok(allowed) = crate::project::allowed_effects(allow) else { return Ok(()) };
    if !allowed.contains(&almide::ir::effect::Effect::Proc) || proj.proc_allow.is_some() {
        return Ok(());
    }
    let level = if refuse { "error" } else { "warning" };
    err(&format!(
        "{level}: [permissions].allow grants `Proc` with no `proc = [...]` command list: the program may start \
         any command, and a started command can reach every other capability"
    ));
    err("  hint: list the commands it may start — `proc = [\"git\"]` under [permissions] (ADR-0025)");
    if refuse {
        return Err("`Proc` granted without a `proc` list under --profile critical".to_string());
    }
    Ok(())
}

/// The subprocess fns that START a child, with the index of the argument
/// that names the command (#2589).
const PROC_SPAWNING: &[(&str, usize)] = &[
    ("exec", 0),
    ("exec_in", 1),
    ("exec_with_stdin", 0),
    ("exec_status", 0),
    ("exec_status_timeout", 0),
    ("run", 0),
    ("run_in", 1),
    ("exec_attached", 0),
    ("spawn", 0),
];

/// `[permissions] proc` (#2589, ADR-0025), statically: every call that starts
/// a child must name its command as a string literal on the list. A command
/// computed at run time, or a spawning fn passed as a value, cannot be checked
/// here and is refused too — so a program that compiles never starts a
/// command outside the list, on any target.
pub fn check_proc_allowlist(ir: &almide::ir::IrProgram, allow: &[String]) -> Result<(), String> {
    use almide::ir::visit::IrVisitor;
    use almide::ir::{CallTarget, IrExpr, IrExprKind};
    struct Scan<'a> {
        allow: &'a [String],
        bad: Vec<String>,
    }
    impl IrVisitor for Scan<'_> {
        fn visit_expr(&mut self, e: &IrExpr) {
            match &e.kind {
                IrExprKind::Call { target: CallTarget::Module { module, func, .. }, args, .. }
                    if module.as_str() == "process" =>
                {
                    if let Some(&(_, at)) = PROC_SPAWNING.iter().find(|(f, _)| *f == func.as_str()) {
                        let line = e.span.map_or(String::new(), |s| format!(" (line {})", s.line));
                        match args.get(at).map(|a| &a.kind) {
                            Some(IrExprKind::LitStr { value }) if self.allow.iter().any(|c| c == value) => {}
                            Some(IrExprKind::LitStr { value }) => {
                                self.bad.push(format!("process.{func}(\"{value}\"){line}: `{value}` is not in [permissions] proc"))
                            }
                            _ => self.bad.push(format!(
                                "process.{func}{line}: the command is not a string literal, so [permissions] proc cannot check it"
                            )),
                        }
                    }
                }
                IrExprKind::FnRef { name } => {
                    let n = name.as_str();
                    if let Some(f) = n.strip_prefix("process.").filter(|f| PROC_SPAWNING.iter().any(|(p, _)| p == f)) {
                        self.bad.push(format!("process.{f} passed as a value: [permissions] proc cannot check its command"));
                    }
                }
                _ => {}
            }
            almide::ir::visit::walk_expr(self, e);
        }
    }
    let mut scan = Scan { allow, bad: Vec::new() };
    for f in ir.functions.iter().chain(ir.modules.iter().flat_map(|m| m.functions.iter())) {
        scan.visit_expr(&f.body);
    }
    if scan.bad.is_empty() {
        return Ok(());
    }
    for b in &scan.bad {
        err(&format!("error: {b}"));
    }
    err(&format!("  hint: [permissions] proc = {allow:?} in almide.toml lists the commands process.* may start"));
    Err(format!("{} [permissions] proc violation(s)", scan.bad.len()))
}

/// Apply `[permissions] proc`: the static gate above, and the same list as
/// the embedded wasm host's run-time bound (`almide:process/spawn` answers a
/// command outside it with an err naming the command).
/// The whole `[permissions]` gate: the capability allow-list (`allow`, via
/// [`check_permissions`]) and the subprocess allow-list (`proc`, #2589).
/// `[permissions]` is a property of the program, not of the target (#3275):
/// the build routes reach it through `compile_driver::optimize_gate_and_link`,
/// and `check` calls it directly. Each violation is printed as it is found.
pub fn enforce_project_permissions(ir: &almide::ir::IrProgram, proj: &crate::project::Project) -> Result<(), String> {
    // `[permissions] fs` (#4022): the directories the program may touch. Paths
    // are run-time values, so the bound is enforced where they are used: the
    // embedded wasm host checks every fs op and preopens only these roots, and
    // the native build embeds them in `main` (`build::inject_fs_scope`).
    almide_wasm_run::set_fs_scope(proj.fs_allow.clone());
    // An absent `allow` is unchecked; an explicit `allow = []` allows no
    // capability at all (#4101).
    if let Some(allow) = &proj.permissions {
        check_permissions(ir, allow)?;
        proc_grant_without_list(proj, false)?;
    }
    enforce_proc_allowlist(ir, proj.proc_allow.as_deref())
}

/// `[permissions] native` (#4093): `Err` with the refusal when a fetched
/// dependency of `proj` brings native code the manifest does not grant.
/// With `announce`, a native build names each granted dependency that brings
/// native code, once per package per process (a test run builds many files).
pub(crate) fn enforce_native_grants(
    proj: &crate::project::Project,
    fetched: &[crate::project::FetchedDep],
    announce: bool,
) -> Result<(), String> {
    let deps = crate::native_grant::survey(proj, fetched);
    if let Some(refusal) = crate::native_grant::refusal(proj, &deps) {
        return Err(refusal);
    }
    if announce {
        static ANNOUNCED: std::sync::Mutex<Vec<std::path::PathBuf>> = std::sync::Mutex::new(Vec::new());
        let mut seen = ANNOUNCED.lock().unwrap_or_else(|p| p.into_inner());
        if !seen.contains(&proj.root) {
            seen.push(proj.root.clone());
            for line in crate::native_grant::granted_lines(&deps) {
                err(&line);
            }
        }
    }
    Ok(())
}

/// Resolve `./almide.toml`'s dependencies before a command that builds runs
/// (#4093): a fetch error (an untrusted owner, a source changed under the
/// lock, a `--locked` miss, a missing repository) and an ungranted native
/// dependency ([`enforce_native_grants`]) are printed and exit 1. Several
/// build routes read the dependencies with errors swallowed (the wasm test
/// lane resolved against an empty set), so the refusal is made once, here,
/// for every route. A manifest that does not parse is left to the command.
pub(crate) fn resolve_cwd_deps_or_exit() {
    let Some(proj) = crate::compile_driver::cwd_project() else { return };
    if proj.dependencies.is_empty() {
        return;
    }
    let fetched = crate::project_fetch::fetch_all_deps(&proj).unwrap_or_else(|e| {
        err(&e);
        std::process::exit(1);
    });
    if let Err(refusal) = enforce_native_grants(&proj, &fetched, false) {
        err(&refusal);
        std::process::exit(1);
    }
}

pub fn enforce_proc_allowlist(ir: &almide::ir::IrProgram, allow: Option<&[String]>) -> Result<(), String> {
    almide_wasm_run::set_proc_allowlist(allow.map(<[String]>::to_vec));
    match allow {
        Some(list) => check_proc_allowlist(ir, list),
        None => Ok(()),
    }
}

/// `[permissions].allow` → the categories it grants, via the one shared
/// matcher (`project::allowed_effects`). The manifest gate in `main` refuses
/// an unknown name on its line before any command runs; this reports it
/// again for a caller that reached here without that gate (#3247).
pub(crate) fn allowed_permissions_or_report(
    permissions: &[String],
) -> Result<std::collections::HashSet<almide::ir::effect::Effect>, String> {
    crate::project::allowed_effects(permissions).inspect_err(|e| err(&format!("error: {e}")))
}

/// Compute a 64-bit hash of a byte slice (using DefaultHasher).
fn hash64(data: &[u8]) -> u64 {
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    data.hash(&mut hasher);
    hasher.finish()
}

/// Cache directory for incremental compilation.
fn incremental_cache_dir() -> std::path::PathBuf {
    std::path::PathBuf::from(".almide/cache")
}

/// The v1 MIR trust-spine render used by `--verified`/`--native-verified`:
/// try `almide_mir::pipeline::try_render_rust_source`, falling back to the v0
/// `rs_code` on any WALL — a v1-rendered program is never wrong, so a wall
/// just means "build via v0 exactly as without the flag". Shared by
/// `cmd_build`'s native path (`build.rs`) and `compile_to_binary_with`
/// (`run.rs`), which had identical copies of this try/fallback logic gated
/// behind their own (different) `native_verified` conditions.
///
/// `metered` is the linked IR of a program that calls the meter prims
/// (`almide_ir::runtime_use::uses_metered_prims`, read from the IR, never
/// from `rs_code`: a user string literal is emitted into that text verbatim,
/// and `println("almide_rt_prim_budget_x")` was refused here, #3486).
pub(crate) fn render_v1_native_or_fallback(
    file: &str,
    rs_code: String,
    metered: Option<almide::ir::IrProgram>,
    opts: &crate::codegen::CodegenOptions,
) -> String {
    let source_text = almide::source_overlay::read_to_string(file).unwrap_or_default();
    // ALMIDE_FORCE_METER_CODEGEN=1 meters the program in the standard
    // codegen even when the whole program renders — the differential that
    // holds the codegen meter to the v1 render (tests/fan_region_floor_test.rs).
    let metered = match metered {
        Some(ir) if almide_base::env::flag("ALMIDE_FORCE_METER_CODEGEN") => {
            return meter_in_codegen(ir, opts, "forced by ALMIDE_FORCE_METER_CODEGEN");
        }
        other => other,
    };
    // ALMIDE_FORCE_METER_SPLICE=1 takes the region-splice route even when the
    // whole program renders — the differential that holds the two routes'
    // spends equal (tests/fan_meter_splice_test.rs).
    let force_splice = metered.is_some() && almide_base::env::flag("ALMIDE_FORCE_METER_SPLICE");
    match almide_mir::pipeline::try_render_rust_source(&source_text) {
        Ok(_) if force_splice => {
            let ir = metered.expect("force_splice implies a metered program");
            link_metered_regions(&source_text, rs_code, "forced by ALMIDE_FORCE_METER_SPLICE", ir, opts)
        }
        Ok(v1_code) => {
            if almide_base::env::flag("ALMIDE_VERIFIED_DEBUG") {
                err(&format!("native: v1 trust-spine render"));
            }
            v1_code
        }
        Err(e) => {
            // Stage 1 probe: fuel is built on the trust spine ONLY. A v0
            // fallback has no Charge ops, so the probe would silently report
            // nothing — the exact silent-miss the probe exists to prevent.
            // Hard error instead of a quiet unmeasured run.
            if almide_mir::charge_probe::probe_enabled() {
                err(&format!(
                    "error: ALMIDE_FUEL_PROBE / --time-report require the v1 native render, but it walled\n  reason: {e}\n  hint: the deterministic meter cannot measure a v0-codegen binary; fix the wall or drop the flag"
                ));
                std::process::exit(1);
            }
            // Budget semantics (fan.bounded / fan.race / fan.timeout): the
            // program walled on the whole-program v1 render, so the standard
            // codegen builds it with the v1-rendered regions linked in
            // (#3669), or meters the regions itself (#3673).
            if let Some(ir) = metered {
                return link_metered_regions(&source_text, rs_code, &e.to_string(), ir, opts);
            }
            // #931 made this note loud because verified was an explicit
            // opt-in; #1074 makes it opt-in again because verified is now the
            // DEFAULT and a wall is normal operation for a large legitimate
            // class of programs (every derived Codec returns Value). The
            // routing changes nothing observable — the standard codegen
            // produces a correct binary — so ambient stderr noise reads as a
            // false warning. `-v` / ALMIDE_VERBOSE=1 (or the deeper
            // ALMIDE_VERIFIED_DEBUG) surfaces it for native-rung-coverage
            // debugging. The wasm leg is untouched: there a wall IS an error.
            if almide_base::env::flag("ALMIDE_VERBOSE") || almide_base::env::flag("ALMIDE_VERIFIED_DEBUG") {
                err(&format!("note: verified native render walled — building via the standard codegen\n  reason: {e}"));
            }
            rs_code
        }
    }
}

fn verbose() -> bool {
    almide_base::env::flag("ALMIDE_VERBOSE") || almide_base::env::flag("ALMIDE_VERIFIED_DEBUG")
}

/// #3669: link the v1-rendered metered regions of `source_text` into the
/// standard codegen's `rs_code` (whose region fns already delegate to them —
/// `MeteredRegionDelegatePass`). `whole_wall` is why the whole-program render
/// was not used. When a region itself is outside the v1 subset, the program
/// `metered` is built again with the codegen's own meter (#3673) — never
/// without one, which would change which arm wins.
pub(crate) fn link_metered_regions(
    source_text: &str,
    rs_code: String,
    whole_wall: &str,
    metered: almide::ir::IrProgram,
    opts: &crate::codegen::CodegenOptions,
) -> String {
    match almide_mir::pipeline::try_render_metered_splice(source_text) {
        Ok(splice) => {
            if verbose() {
                err(&format!("note: metered program built via the standard codegen, its regions rendered by the trust spine\n  reason: {whole_wall}"));
            }
            rs_code + &splice
        }
        Err(e) => meter_in_codegen(metered, opts, &format!("a region is outside the v1 native render: {e}")),
    }
}

/// #3673: build the metered program with the standard codegen's own
/// deterministic meter (`MeterChargePass` + its runtime).
fn meter_in_codegen(metered: almide::ir::IrProgram, opts: &crate::codegen::CodegenOptions, why: &str) -> String {
    if verbose() {
        err(&format!("note: metered program built via the standard codegen, its regions metered by the standard codegen\n  reason: {why}"));
    }
    crate::codegen_metered(metered, opts)
}

/// Resolve `[dependencies]` search paths from `almide.toml` in the current
/// directory (if present), exiting the process on a fetch failure. Shared
/// by `cmd_emit`, `resolve_module_to_file` and `cmd_compile` (`compile.rs`)
/// — all three had identical copies of this almide.toml lookup + exit-on-
/// error handling.
pub(crate) fn dep_paths_from_cwd_toml() -> Vec<(crate::project::PkgId, std::path::PathBuf)> {
    if std::path::Path::new("almide.toml").exists() {
        if let Ok(proj) = crate::project::parse_toml(std::path::Path::new("almide.toml")) {
            crate::project_fetch::fetch_all_deps(&proj)
                .unwrap_or_else(|e| { err(&format!("{}", e)); std::process::exit(1); })
                .into_iter()
                .map(|fd| (fd.pkg_id, fd.source_dir))
                .collect()
        } else {
            vec![]
        }
    } else {
        vec![]
    }
}

/// Load `[native-deps]` and dependency-presence info from `almide.toml`,
/// searching first in `file`'s directory then CWD. `source_root` (the
/// directory containing `almide.toml`, where `native/*.rs` lives) is
/// `Some` whenever there's anything to inject — native deps or
/// `[dependencies]` — `None` otherwise, matching the previous inline
/// `!native_deps.is_empty() || has_deps` gate. Shared by `cmd_build`
/// (`build.rs`) and `compile_to_binary_with` (`run.rs`), which had
/// identical copies of this almide.toml lookup.
pub(crate) fn load_native_build_config(file: &str) -> (Vec<crate::project::NativeDep>, Option<std::path::PathBuf>) {
    let file_dir = std::path::Path::new(file).parent()
        .map(|p| if p.as_os_str().is_empty() { std::path::PathBuf::from(".") } else { p.to_path_buf() })
        .unwrap_or_else(|| std::path::PathBuf::from("."));
    let toml_path = {
        let candidate = file_dir.join("almide.toml");
        if candidate.exists() { candidate } else { std::path::PathBuf::from("almide.toml") }
    };
    let parsed = toml_path.exists().then(|| {
        // A broken almide.toml must not be SILENT: dropping it here also drops
        // [native-deps]/native/ injection, and the build then fails later as an
        // opaque E0433 on the native module.
        crate::project::parse_toml(&toml_path)
            .map_err(|e| err(&format!("warning: {} ignored: {}", toml_path.display(), e)))
            .ok()
    }).flatten();
    let native_deps = parsed.as_ref().map(|p| p.native_deps.clone()).unwrap_or_default();
    let toml_dir = toml_path.parent()
        .map(|p| if p.as_os_str().is_empty() { std::path::PathBuf::from(".") } else { p.to_path_buf() })
        .unwrap_or_else(|| std::path::PathBuf::from("."));
    let has_deps = parsed.as_ref().map_or(false, |p| !p.dependencies.is_empty());
    // A `native/` DIRECTORY is itself something to inject (#886): a package
    // whose only native requirement is a `native/*.rs` module — no
    // [native-deps], no [dependencies] — got `None` here, so the module was
    // never copied into the generated crate and the build failed with an
    // opaque `E0433: could not find <mod> in the crate root` pointing at
    // generated code the user cannot see.
    let has_native_dir = toml_dir.join("native").is_dir();
    let source_root =
        if !native_deps.is_empty() || has_deps || has_native_dir { Some(toml_dir) } else { None };
    (native_deps, source_root)
}

/// `collect_test_files`'s per-entry body: recurse into subdirectories, or
/// check a `.almd` file for a `test` block. A symlinked directory is not
/// descended (#4140): a link loop would collect the same files at every
/// level, and a link out would run tests from outside the tree.
fn collect_test_files_entry(path: &std::path::Path, files: &mut Vec<String>) {
    let is_link = std::fs::symlink_metadata(path).is_ok_and(|m| m.file_type().is_symlink());
    if path.is_dir() && !is_link {
        files.extend(collect_test_files(path));
    } else if path.extension().map(|e| e == "almd").unwrap_or(false) {
        // Check if file contains a test block
        if let Ok(content) = almide::source_overlay::read_to_string(path) {
            if content.contains("\ntest ") || content.starts_with("test ") {
                files.push(path.to_string_lossy().to_string());
            }
        }
    }
}

/// Recursively collect .almd files that contain `test` blocks.
fn collect_test_files(dir: &std::path::Path) -> Vec<String> {
    let mut files = Vec::new();
    // Skip hidden directories, target/, node_modules/, etc.
    let dir_name = dir.file_name().and_then(|n| n.to_str()).unwrap_or("");
    if dir_name.starts_with('.') || dir_name == "target" || dir_name == "node_modules" {
        return files;
    }
    if let Ok(entries) = std::fs::read_dir(dir) {
        for entry in entries.filter_map(|e| e.ok()) {
            if entry.file_type().is_ok_and(|t| t.is_symlink()) && entry.path().is_dir() {
                continue;
            }
            collect_test_files_entry(&entry.path(), &mut files);
        }
    }
    files
}
