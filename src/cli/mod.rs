/// CLI command implementations.

mod run;
mod build;
mod bench;
mod compile;
mod emit;
mod check;
mod dialect_stamp;
mod commands;
mod test_scratch;
pub mod test_report;
mod install;
mod selfupdate;
pub mod lsp;
pub mod mcp;
mod mcp_tools;
pub mod repl;
mod ide;
mod fix;
mod docs_gen;
mod cargo_build;

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
pub use check::{cmd_check, cmd_check_json, cmd_check_effects};
pub use commands::{cmd_init, cmd_test, cmd_test_fast, cmd_test_json, cmd_test_wasm, cmd_fmt, cmd_clean, FmtMode};
pub use install::cmd_install;
pub use selfupdate::cmd_self_update;
pub use ide::{cmd_ide_outline, cmd_ide_doc, cmd_ide_stdlib_snapshot};
pub use fix::cmd_fix;
pub use docs_gen::cmd_docs_gen;

use std::hash::{Hash, Hasher};
use crate::err;

/// Check that all effects used in the program are allowed by [permissions].allow in almide.toml.
/// Returns Ok(()) if no violations, or Err with a description of violations.
pub fn check_permissions(ir: &almide::ir::IrProgram, permissions: &[String]) -> Result<(), String> {
    use almide::codegen::pass_effect_inference::{EffectInferencePass, Effect};
    use almide::codegen::pass::NanoPass;

    let result = EffectInferencePass.run(ir.clone(), almide::codegen::pass::Target::Rust);
    let ir_after = result.program;

    let allowed: std::collections::HashSet<Effect> = permissions.iter()
        .filter_map(|s| match s.as_str() {
            "IO" => Some(Effect::IO),
            "Net" => Some(Effect::Net),
            "Env" => Some(Effect::Env),
            "Time" => Some(Effect::Time),
            "Rand" => Some(Effect::Rand),
            "Fan" => Some(Effect::Fan),
            _ => None,
        })
        .collect();

    let mut violations = 0;
    for (name, fe) in &ir_after.effect_map.functions {
        let forbidden: Vec<_> = fe.transitive.iter()
            .filter(|e| !allowed.contains(e))
            .collect();
        if !forbidden.is_empty() {
            err(&format!("error: capability violation in `{}`", name));
            for e in &forbidden {
                err(&format!("  {} is not in [permissions].allow", e));
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
pub(crate) fn render_v1_native_or_fallback(file: &str, rs_code: String) -> String {
    let source_text = std::fs::read_to_string(file).unwrap_or_default();
    match almide_mir::pipeline::try_render_rust_source(&source_text) {
        Ok(v1_code) => {
            if std::env::var("ALMIDE_VERIFIED_DEBUG").is_ok() {
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
            // Budget semantics (fan.bounded / fan.race) exist only on the v1
            // trust spine — the v0 pipeline emits calls to budget runtime fns
            // that do not exist, so the fallback would die later as an opaque
            // rustc E0425 in generated code. Refuse with the wall reason
            // instead (same honesty rule as the probe above).
            if rs_code.contains("almide_rt_prim_budget_") || rs_code.contains("almide_rt_prim_timeout_") {
                err(&format!(
                    "error: fan.bounded / fan.race require the v1 native render, but it walled\n  reason: {e}\n  hint: budgets are metered on the trust spine only; simplify the program to the v1 subset or build for --target wasm"
                ));
                std::process::exit(1);
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
            if std::env::var("ALMIDE_VERBOSE").is_ok() || std::env::var("ALMIDE_VERIFIED_DEBUG").is_ok() {
                err(&format!("note: verified native render walled — building via the standard codegen\n  reason: {e}"));
            }
            rs_code
        }
    }
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
/// check a `.almd` file for a `test` block. Extracted verbatim.
fn collect_test_files_entry(path: &std::path::Path, files: &mut Vec<String>) {
    if path.is_dir() {
        files.extend(collect_test_files(path));
    } else if path.extension().map(|e| e == "almd").unwrap_or(false) {
        // Check if file contains a test block
        if let Ok(content) = std::fs::read_to_string(path) {
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
            collect_test_files_entry(&entry.path(), &mut files);
        }
    }
    files
}
