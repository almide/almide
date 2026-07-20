/// CLI command implementations.

mod run;
mod build;
mod compile;
mod emit;
mod check;
mod commands;
mod install;
mod selfupdate;
pub mod lsp;
pub mod repl;
mod ide;
mod fix;
mod docs_gen;

pub use run::{cmd_run, cmd_run_inner};
pub use build::cmd_build;
pub use compile::cmd_compile;
pub use emit::cmd_emit;
pub use check::{cmd_check, cmd_check_json, cmd_check_effects};
pub use commands::{cmd_init, cmd_test, cmd_test_fast, cmd_test_json, cmd_test_wasm, cmd_fmt, cmd_clean};
pub use install::cmd_install;
pub use selfupdate::cmd_self_update;
pub use ide::{cmd_ide_outline, cmd_ide_doc, cmd_ide_stdlib_snapshot};
pub use fix::cmd_fix;
pub use docs_gen::cmd_docs_gen;

use std::hash::{Hash, Hasher};

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
            eprintln!("error: capability violation in `{}`", name);
            for e in &forbidden {
                eprintln!("  {} is not in [permissions].allow", e);
            }
            violations += 1;
        }
    }
    if violations > 0 {
        eprintln!("\n{} capability violation(s)", violations);
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

/// Cargo.toml template for generated Rust projects (without HTTP/TLS).
const GENERATED_CARGO_TOML: &str = r#"[package]
name = "almide-out"
version = "0.1.0"
edition = "2021"

# Self-isolate from any ENCLOSING cargo workspace: without this, running almide
# with a project dir nested inside a Rust workspace (a repo's tools/ tree, the
# fuzzer's .scratch) makes cargo resolve the parent workspace and refuse the
# build ("current package believes it's in a workspace when it's not").
[workspace]

[profile.dev]
opt-level = 1
overflow-checks = false

[profile.release]
opt-level = 3
lto = true
codegen-units = 1
"#;

/// Cargo.toml template with HTTP/TLS dependencies (only when http runtime is used).
const GENERATED_CARGO_TOML_HTTP: &str = r#"[package]
name = "almide-out"
version = "0.1.0"
edition = "2021"

[workspace]

[dependencies]
rustls = { version = "0.23", default-features = false, features = ["ring", "logging", "std", "tls12"] }
webpki-roots = "0.26"

[profile.dev]
opt-level = 1
overflow-checks = false

[profile.release]
opt-level = 3
lto = true
codegen-units = 1
"#;

/// `--cfg almide_par` enables the rayon-backed parallel runtime paths. The cfg
/// follows the DEPENDENCY: inject it only when the generated project's Cargo.toml
/// declares rayon (e.g. via `[native-deps]` — the nn repos do) — the base template
/// carries no external crates (#739), so an unconditional cfg would make ANY
/// matrix-using program fail to resolve `rayon::prelude` (E0433). Without the cfg
/// the runtime compiles its serial side, exactly like the raw-rustc test harness.
fn inject_almide_par_if_rayon(cmd: &mut std::process::Command, project_dir: &std::path::Path) {
    let has_rayon = std::fs::read_to_string(project_dir.join("Cargo.toml"))
        .map(|t| {
            t.lines().any(|l| {
                l.trim_start()
                    .strip_prefix("rayon")
                    .is_some_and(|r| r.trim_start().starts_with('='))
            })
        })
        .unwrap_or(false);
    if has_rayon {
        cmd.env(
            "RUSTFLAGS",
            format!("{} --cfg almide_par", std::env::var("RUSTFLAGS").unwrap_or_default()),
        );
    }
}

/// Build a Cargo.toml string by inserting native deps into the [dependencies] section.
fn build_cargo_toml(base_toml: &str, native_deps: &[crate::project::NativeDep]) -> String {
    if native_deps.is_empty() {
        return base_toml.to_string();
    }
    let mut toml = base_toml.to_string();
    let mut extra_deps = String::new();
    for dep in native_deps {
        // The base template may already carry this crate (e.g. rayon in the ML
        // profile); a second key is a Cargo hard error. Skip the [native-deps]
        // entry when the template already declares it — declaring it in
        // [native-deps] then stays the portable answer for both build AND the
        // test harness's manifest. (#646)
        let already = toml.lines().any(|l| {
            l.trim_start().strip_prefix(&dep.name)
                .is_some_and(|rest| rest.trim_start().starts_with('='))
        });
        if already {
            continue;
        }
        let dep_line = if dep.spec.starts_with('{') {
            format!("{} = {}\n", dep.name, dep.spec)
        } else {
            format!("{} = \"{}\"\n", dep.name, dep.spec)
        };
        extra_deps.push_str(&dep_line);
    }
    if let Some(pos) = toml.find("[dependencies]") {
        let insert_pos = toml[pos..].find('\n').map(|i| pos + i + 1).unwrap_or(toml.len());
        toml.insert_str(insert_pos, &extra_deps);
    } else {
        toml.push_str("\n[dependencies]\n");
        toml.push_str(&extra_deps);
    }
    toml
}

/// Copy native/*.rs files from source_root into src_dir and inject mod declarations into code.
fn copy_dir_recursive(from: &std::path::Path, to: &std::path::Path) -> Result<(), String> {
    std::fs::create_dir_all(to).map_err(|e| format!("failed to create {}: {}", to.display(), e))?;
    let entries = std::fs::read_dir(from).map_err(|e| format!("failed to read {}: {}", from.display(), e))?;
    for entry in entries.flatten() {
        let src = entry.path();
        let dst = to.join(entry.file_name());
        if src.is_dir() {
            copy_dir_recursive(&src, &dst)?;
        } else {
            std::fs::copy(&src, &dst)
                .map_err(|e| format!("failed to copy {}: {}", src.display(), e))?;
        }
    }
    Ok(())
}

fn inject_native_modules(code: &mut String, source_root: Option<&std::path::Path>, src_dir: &std::path::Path) -> Result<(), String> {
    let root = match source_root {
        Some(r) => r,
        None => return Ok(()),
    };
    let native_dir = root.join("native");
    if !native_dir.is_dir() { return Ok(()); }
    let mut mod_decls = String::new();
    if let Ok(entries) = std::fs::read_dir(&native_dir) {
        for entry in entries.flatten() {
            let path = entry.path();
            if path.extension().map_or(false, |e| e == "rs") {
                let stem = path.file_stem().unwrap().to_string_lossy().to_string();
                let content = std::fs::read_to_string(&path)
                    .map_err(|e| format!("failed to read {}: {}", path.display(), e))?;
                std::fs::write(src_dir.join(entry.file_name()), &content)
                    .map_err(|e| format!("failed to write native module {}: {}", stem, e))?;
                mod_decls.push_str(&format!("mod {};\n", stem));
            } else if path.is_dir() {
                // asset subdirectories (e.g. native/wgsl/*.wgsl) travel with the
                // modules so include_str!("wgsl/...") resolves in the generated crate
                copy_dir_recursive(&path, &src_dir.join(entry.file_name()))?;
            }
        }
    }
    if !mod_decls.is_empty() {
        if let Some(pos) = code.find("\nuse ") {
            code.insert_str(pos, &format!("\n{}", mod_decls));
        } else if let Some(pos) = code.find("\nfn ") {
            code.insert_str(pos, &format!("\n{}", mod_decls));
        } else {
            *code = format!("{}\n{}", mod_decls, code);
        }
    }
    Ok(())
}

/// Inject native modules and native-deps from all dependency packages (recursive).
fn inject_dep_natives(code: &mut String, source_root: Option<&std::path::Path>, src_dir: &std::path::Path, project_dir: &std::path::Path) -> Result<(), String> {
    let mut visited = std::collections::HashSet::new();
    inject_dep_natives_rec(code, source_root, src_dir, project_dir, &mut visited)
}

fn inject_dep_natives_rec(code: &mut String, source_root: Option<&std::path::Path>, src_dir: &std::path::Path, project_dir: &std::path::Path, visited: &mut std::collections::HashSet<String>) -> Result<(), String> {
    let root = match source_root { Some(r) => r, None => return Ok(()) };
    let toml_path = root.join("almide.toml");
    if !toml_path.exists() { return Ok(()); }
    let proj = crate::project::parse_toml(&toml_path).map_err(|e| format!("parse almide.toml: {}", e))?;
    for dep in &proj.dependencies {
        let dep_dir = resolve_dep_dir(dep);
        let Some(dep_dir) = dep_dir else { continue };
        let key = dep_dir.to_string_lossy().to_string();
        if visited.contains(&key) { continue; }
        visited.insert(key);
        inject_native_modules(code, Some(&dep_dir), src_dir)?;
        propagate_native_deps(&dep_dir, project_dir);
        inject_dep_natives_rec(code, Some(&dep_dir), src_dir, project_dir, visited)?;
    }
    Ok(())
}

fn resolve_dep_dir(dep: &crate::project::Dependency) -> Option<std::path::PathBuf> {
    if let Some(ref p) = dep.path { Some(std::path::PathBuf::from(p)) }
    else { crate::project_fetch::fetch_dep(dep).ok() }
}

fn propagate_native_deps(dep_dir: &std::path::Path, project_dir: &std::path::Path) {
    let dep_toml = dep_dir.join("almide.toml");
    let dep_proj = match dep_toml.exists().then(|| crate::project::parse_toml(&dep_toml).ok()).flatten() {
        Some(p) => p, None => return,
    };
    if dep_proj.native_deps.is_empty() { return; }
    let cargo_path = project_dir.join("Cargo.toml");
    let mut cargo = std::fs::read_to_string(&cargo_path).unwrap_or_default();
    for nd in &dep_proj.native_deps {
        if cargo.contains(&nd.name) { continue; }
        append_cargo_dep(&mut cargo, &nd.name, &nd.spec);
    }
    let _ = std::fs::write(&cargo_path, &cargo);
}

fn append_cargo_dep(cargo: &mut String, name: &str, spec: &str) {
    let line = if spec.starts_with('{') { format!("{} = {}\n", name, spec) }
        else { format!("{} = \"{}\"\n", name, spec) };
    if let Some(pos) = cargo.find("[dependencies]") {
        let insert = cargo[pos..].find('\n').map(|i| pos + i + 1).unwrap_or(cargo.len());
        cargo.insert_str(insert, &line);
    } else {
        cargo.push_str(&format!("\n[dependencies]\n{}", line));
    }
}

/// Build generated Rust code as a cdylib shared library (.dylib/.so).
fn cargo_build_cdylib(rs_code: &str, project_dir: &std::path::Path, lib_name: &str, release: bool, native_deps: &[crate::project::NativeDep], source_root: Option<&std::path::Path>) -> Result<std::path::PathBuf, String> {
    let src_dir = project_dir.join("src");
    std::fs::create_dir_all(&src_dir).map_err(|e| format!("failed to create {}: {}", src_dir.display(), e))?;
    // Base manifest for a cdylib; `build_cargo_toml` folds in `[native-deps]` and
    // `inject_dep_natives` appends any dependency-package native deps below — so a
    // cdylib wires native crates exactly like the bin path (#719). Previously this
    // wrote a dep-free manifest and `rs_code` verbatim, so `@extern(rust, …)`
    // modules were undeclared (E0433) and `[native-deps]` never reached cargo.
    let cdylib_base = format!(r#"[package]
name = "almide-cdylib"
version = "0.1.0"
edition = "2021"

[workspace]

[lib]
name = "{}"
crate-type = ["cdylib"]
path = "src/lib.rs"

[profile.dev]
opt-level = 1
overflow-checks = false

[profile.release]
opt-level = 3
lto = true
codegen-units = 1
"#, lib_name.replace('-', "_"));
    let cargo_toml = build_cargo_toml(&cdylib_base, native_deps);
    std::fs::write(project_dir.join("Cargo.toml"), &cargo_toml)
        .map_err(|e| format!("failed to write Cargo.toml: {}", e))?;

    // Copy `native/*.rs` shims into src/ + inject `mod <stem>;`, then pull native
    // modules + `[native-deps]` from dependency packages — same wiring as
    // `cargo_build_generated_with_native`.
    let mut lib_code = rs_code.to_string();
    inject_native_modules(&mut lib_code, source_root, &src_dir)?;
    inject_dep_natives(&mut lib_code, source_root, &src_dir, project_dir)?;
    std::fs::write(src_dir.join("lib.rs"), &lib_code)
        .map_err(|e| format!("failed to write lib.rs: {}", e))?;

    let mut cmd = std::process::Command::new("cargo");
    inject_almide_par_if_rayon(&mut cmd, project_dir);
    cmd.arg("build").current_dir(project_dir).arg("--quiet");
    if release { cmd.arg("--release"); }
    let output = cmd.output().map_err(|e| format!("failed to run cargo: {}", e))?;
    if !output.status.success() {
        return Err(String::from_utf8_lossy(&output.stderr).to_string());
    }

    let profile = if release { "release" } else { "debug" };
    let prefix = if cfg!(target_os = "macos") || cfg!(target_os = "linux") { "lib" } else { "" };
    let ext = if cfg!(target_os = "macos") { "dylib" } else if cfg!(target_os = "windows") { "dll" } else { "so" };
    let lib_filename = format!("{}{}.{}", prefix, lib_name.replace('-', "_"), ext);
    let lib_path = project_dir.join("target").join(profile).join(&lib_filename);
    if !lib_path.exists() {
        return Err(format!("expected library not found at {}", lib_path.display()));
    }

    // Copy to current directory
    let dest = std::path::Path::new(".").join(&lib_filename);
    std::fs::copy(&lib_path, &dest)
        .map_err(|e| format!("failed to copy library: {}", e))?;
    Ok(dest)
}

/// Build generated Rust code using cargo.
/// Returns the path to the built binary on success.
pub(crate) fn cargo_build_generated(rs_code: &str, project_dir: &std::path::Path, release: bool) -> Result<std::path::PathBuf, String> {
    cargo_build_generated_with_native(rs_code, project_dir, release, &[], None)
}

/// Build generated Rust code with optional native Rust dependencies and source files.
fn cargo_build_generated_with_native(
    rs_code: &str,
    project_dir: &std::path::Path,
    release: bool,
    native_deps: &[crate::project::NativeDep],
    source_root: Option<&std::path::Path>,
) -> Result<std::path::PathBuf, String> {
    let uses_matrix = rs_code.contains("almide_rt_matrix_");
    let uses_http = rs_code.contains("almide_rt_http_") || rs_code.contains("use rustls");
    let uses_zlib = rs_code.contains("almide_rt_zlib_") || rs_code.contains("use flate2");

    // rlib fast path (dep-free): link the precompiled runtime instead of
    // recompiling its ~2000 lines per build. The runtime is compiled at the same
    // opt-level as the binary (3 for `--release`, 1 for debug/`almide run`), so
    // it is fully optimized; only its compilation is amortized. Net: shipping
    // builds drop from ~27-33s to ~1-2s (~20x).
    //
    // Tradeoff: because the runtime is a separate crate (no LTO), non-generic
    // non-#[inline] runtime fns (e.g. string.trim/split) aren't inlined across
    // the crate boundary, costing up to ~10% runtime on string/list-heavy hot
    // loops (typically 2-5%). ThinLTO would recover it but erases the build win
    // (it re-optimizes the runtime at link time). The planned fix is #[inline] on
    // the hot runtime fns — see docs/roadmap. ALMIDE_NO_RTLIB=1 forces the
    // monolithic cargo build (full cross-crate inlining) when a shipped binary
    // must squeeze out that last few percent. Any rustc failure also falls
    // through to the cargo path below, so correctness never regresses.
    if std::env::var_os("ALMIDE_NO_RTLIB").is_none()
        && !uses_matrix && !uses_http && !uses_zlib
        && native_deps.is_empty() && source_root.is_none()
    {
        let opt_level = if release { "3" } else { "1" };
        if let (Ok(rlib), Some(mut slim)) = (
            ensure_runtime_rlib(opt_level),
            crate::codegen::slim_main_with_external_runtime(rs_code),
        ) {
            if !slim.contains("fn main(") && !slim.contains("fn almide_main(") {
                slim.push_str("\nfn main() {}\n");
            }
            let rlib_dir = rlib.parent().unwrap_or_else(|| std::path::Path::new("."));
            let rs_path = project_dir.join("almide_gen_main.rs");
            let bin_path = project_dir.join(if cfg!(windows) { "almide-out.exe" } else { "almide-out" });
            if std::fs::write(&rs_path, &slim).is_ok() {
                let mut cmd = std::process::Command::new(crate::find_rustc());
                cmd.arg(&rs_path)
                    .arg("-o").arg(&bin_path)
                    .arg("--edition").arg("2021")
                    .arg("-C").arg(format!("opt-level={opt_level}"))
                    .arg("--extern").arg(format!("almide_rt={}", rlib.display()))
                    .arg("-L").arg(rlib_dir)
                    .arg("-A").arg("warnings");
                if let Ok(output) = cmd.output() {
                    if output.status.success() {
                        return Ok(bin_path);
                    }
                    // else: fall through to the self-contained cargo build
                }
            }
        }
    }

    let src_dir = project_dir.join("src");
    std::fs::create_dir_all(&src_dir).map_err(|e| format!("failed to create {}: {}", src_dir.display(), e))?;

    // Build Cargo.toml: start with base template, append native deps + auto-detected deps.
    // Matrix programs need NO extra deps: the flat AlmideMatrix runtime + the embedded
    // almide-kernel SIMD modules are pure Rust (the burn/BLAS splice retired with the
    // 0.28 flat-matrix runtime — its Vec<Vec<f64>> marker no longer existed anywhere
    // except inside the embedded kernel bridge, where the splicer misfired, #739).
    let base_toml = if uses_http { GENERATED_CARGO_TOML_HTTP } else { GENERATED_CARGO_TOML };
    let mut all_deps = native_deps.to_vec();
    if uses_zlib {
        all_deps.push(crate::project::NativeDep { name: "flate2".into(), spec: "1".into() });
    }
    let cargo_toml = build_cargo_toml(base_toml, &all_deps);
    std::fs::write(project_dir.join("Cargo.toml"), &cargo_toml)
        .map_err(|e| format!("failed to write Cargo.toml: {}", e))?;

    let mut final_code = rs_code.to_string();

    inject_native_modules(&mut final_code, source_root, &src_dir)?;
    // Inject native modules + native-deps from dependency packages
    inject_dep_natives(&mut final_code, source_root, &src_dir, project_dir)?;

    // Library modules may not define main — auto-generate an empty one
    if !final_code.contains("fn main(") && !final_code.contains("fn almide_main(") {
        final_code.push_str("\nfn main() {}\n");
    }

    std::fs::write(src_dir.join("main.rs"), &final_code)
        .map_err(|e| format!("failed to write main.rs: {}", e))?;

    let mut cmd = std::process::Command::new("cargo");
    inject_almide_par_if_rayon(&mut cmd, project_dir);
    cmd.arg("build").current_dir(project_dir);
    if release {
        cmd.arg("--release");
    }
    // Suppress cargo's chatty output
    cmd.arg("--quiet");

    let output = cmd.output().map_err(|e| format!("failed to run cargo: {}", e))?;
    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr).to_string();
        return Err(wrap_codegen_leak(stderr));
    }

    let profile = if release { "release" } else { "debug" };
    let exe = if cfg!(windows) { "almide-out.exe" } else { "almide-out" };
    let bin_path = project_dir.join("target").join(profile).join(exe);
    if !bin_path.exists() {
        return Err(format!("expected binary not found at {}", bin_path.display()));
    }
    Ok(bin_path)
}

/// Scrub rustc output when our codegen produces invalid Rust — replaces
/// generated paths with placeholders and prepends a bug-report banner so
/// users (and harness classifiers) don't mistake a compiler bug for a
/// user-facing language error. No-op when the output is clean.
fn wrap_codegen_leak(stderr: String) -> String {
    let mentions_main_rs = stderr.contains("src/main.rs");
    let leaks_rustc_code = contains_rustc_error_code(&stderr);
    if !(mentions_main_rs || leaks_rustc_code) {
        return stderr;
    }
    let scrubbed = stderr
        .replace("src/main.rs", "<generated.rs>")
        .replace("almide-out", "almide-generated");
    format!(
        "codegen produced invalid Rust — this is an Almide bug.\n\
         Please file a minimal repro at https://github.com/almide/almide/issues\n\
         \n\
         --- rustc output (edited to hide generated paths) ---\n\
         {}",
        scrubbed
    )
}

/// Build generated Rust code using cargo for test mode (--test harness).
/// Returns the path to the built test binary on success.
fn cargo_build_test(rs_code: &str, project_dir: &std::path::Path) -> Result<std::path::PathBuf, String> {
    cargo_build_test_with_native(rs_code, project_dir, &[], None)
}

/// Path to the precompiled `almide_rt` runtime rlib, built once per process.
///
/// The runtime (string/list/map/... ops, RcCow, AlmideConcat, the equality
/// macros) is identical across every compiled file, yet the inline-source model
/// makes rustc recompile all ~2000 lines of it for each one (~2.2s/file). The
/// rlib model — Rust's own — compiles it once into an `.rlib`; per-file rustc
/// then just links it (~0.4s/file).
///
/// Keyed by a hash of the runtime source + rustc version + opt profile, so a
/// compiler upgrade or a runtime edit transparently rebuilds. Cross-process
/// builders serialize on a per-dir advisory lock; a warm cache is a stat.
///
/// `opt_level` selects the optimization the runtime is compiled at: "1" for
/// tests/debug (where runtime speed is irrelevant), "3" for release shipping
/// (so the linked runtime keeps full optimization). Each level is cached
/// separately and built at most once per process.
fn ensure_runtime_rlib(opt_level: &str) -> Result<std::path::PathBuf, String> {
    use std::sync::{Mutex, OnceLock};
    static CACHE: OnceLock<Mutex<std::collections::HashMap<String, Result<std::path::PathBuf, String>>>> = OnceLock::new();
    let cache = CACHE.get_or_init(|| Mutex::new(std::collections::HashMap::new()));
    {
        let guard = cache.lock().unwrap();
        if let Some(r) = guard.get(opt_level) {
            return r.clone();
        }
    }
    let result = build_runtime_rlib(opt_level);
    cache.lock().unwrap().insert(opt_level.to_string(), result.clone());
    result
}

fn build_runtime_rlib(opt_level: &str) -> Result<std::path::PathBuf, String> {
    let src = crate::codegen::emit_runtime_crate();
    let rustc = crate::find_rustc();
    let rustc_ver = std::process::Command::new(&rustc)
        .arg("--version")
        .output()
        .ok()
        .map(|o| String::from_utf8_lossy(&o.stdout).trim().to_string())
        .unwrap_or_default();
    // The profile string here MUST match the per-file rustc invocation that links it.
    let key = format!("{:016x}", hash64(format!("{src}|{rustc_ver}|opt{opt_level}|ed2021").as_bytes()));
    let dir = std::env::temp_dir().join(format!("almide-rtlib-{key}"));
    let rlib = dir.join("libalmide_rt.rlib");
    if rlib.exists() {
        return Ok(rlib);
    }
    std::fs::create_dir_all(&dir).map_err(|e| format!("rtlib dir: {e}"))?;
    let _lock = run::BuildDirLock::acquire(&dir)?;
    if rlib.exists() {
        return Ok(rlib); // another builder won the race while we waited
    }
    let src_path = dir.join("almide_rt.rs");
    std::fs::write(&src_path, &src).map_err(|e| format!("rtlib src: {e}"))?;
    let output = std::process::Command::new(&rustc)
        .arg(&src_path)
        .arg("--crate-type").arg("lib")
        .arg("--crate-name").arg("almide_rt")
        .arg("--edition").arg("2021")
        .arg("-C").arg(format!("opt-level={opt_level}"))
        .arg("-C").arg("overflow-checks=no")
        .arg("-A").arg("warnings")
        .arg("--out-dir").arg(&dir)
        .output()
        .map_err(|e| format!("rtlib rustc: {e}"))?;
    if !output.status.success() {
        return Err(String::from_utf8_lossy(&output.stderr).to_string());
    }
    if !rlib.exists() {
        return Err("rtlib build produced no .rlib".to_string());
    }
    Ok(rlib)
}

fn cargo_build_test_with_native(
    rs_code: &str,
    project_dir: &std::path::Path,
    native_deps: &[crate::project::NativeDep],
    source_root: Option<&std::path::Path>,
) -> Result<std::path::PathBuf, String> {
    let uses_http = rs_code.contains("almide_rt_http_") || rs_code.contains("use rustls");
    let uses_zlib = rs_code.contains("almide_rt_zlib_") || rs_code.contains("use flate2");
    let base_toml = if uses_http { GENERATED_CARGO_TOML_HTTP } else { GENERATED_CARGO_TOML };
    let mut all_deps = native_deps.to_vec();
    if uses_zlib {
        all_deps.push(crate::project::NativeDep { name: "flate2".into(), spec: "1".into() });
    }

    // Fast path: the generated test crate is dependency-free (the runtime is
    // inlined as source). `cargo test --no-run` serializes concurrent builds on
    // cargo's global `~/.cargo/.package-cache` lock — even across separate
    // project dirs — so a parallel test run is effectively sequential. A bare
    // `rustc --test` has no such lock, so per-file builds run truly in parallel.
    if !uses_http && !uses_zlib && native_deps.is_empty() && source_root.is_none() {
        let bin_path = project_dir.join("almide_test_bin");

        // rlib fast path: link the precompiled runtime instead of recompiling
        // its ~2000 lines per file (~2.2s → ~0.4s). Any failure (e.g. a runtime
        // vs. user type collision that only manifests cross-crate) falls through
        // to the inline path below, so this never regresses correctness — at
        // worst a file pays one extra rustc. Opt out with ALMIDE_NO_RTLIB=1.
        if std::env::var_os("ALMIDE_NO_RTLIB").is_none() {
            if let (Ok(rlib), Some(mut slim)) = (
                ensure_runtime_rlib("1"),
                crate::codegen::slim_main_with_external_runtime(rs_code),
            ) {
                if !slim.contains("fn main(") && !slim.contains("fn almide_main(") {
                    slim.push_str("\nfn main() {}\n");
                }
                let rlib_dir = rlib.parent().unwrap_or_else(|| std::path::Path::new("."));
                let rs_path = project_dir.join("almide_test_main.rs");
                if std::fs::write(&rs_path, &slim).is_ok() {
                    // opt-level=0 for the slim main: the runtime (the part that
                    // benefits from optimization) is already compiled into the
                    // rlib at opt-level=1, and test user code is short-lived, so
                    // optimizing it just burns compile time. opt0 + rlib is ~2.7x
                    // over the inline opt1 path; opt1 + rlib only ~1.6x.
                    let output = std::process::Command::new(crate::find_rustc())
                        .arg(&rs_path)
                        .arg("--test")
                        .arg("-o").arg(&bin_path)
                        .arg("--edition").arg("2021")
                        .arg("-C").arg("opt-level=0")
                        .arg("-C").arg("overflow-checks=no")
                        .arg("--extern").arg(format!("almide_rt={}", rlib.display()))
                        .arg("-L").arg(rlib_dir)
                        .arg("-A").arg("warnings")
                        .output();
                    if let Ok(output) = output {
                        if output.status.success() {
                            return Ok(bin_path);
                        }
                        // else: fall through to the self-contained inline build
                    }
                }
            }
        }

        let mut final_code = rs_code.to_string();
        if !final_code.contains("fn main(") && !final_code.contains("fn almide_main(") {
            final_code.push_str("\nfn main() {}\n");
        }
        let rs_path = project_dir.join("almide_test_main.rs");
        std::fs::write(&rs_path, &final_code)
            .map_err(|e| format!("failed to write {}: {}", rs_path.display(), e))?;
        let output = std::process::Command::new(crate::find_rustc())
            .arg(&rs_path)
            .arg("--test")
            .arg("-o").arg(&bin_path)
            .arg("--edition").arg("2021")
            .arg("-C").arg("opt-level=1")
            .arg("-C").arg("overflow-checks=no")
            .arg("-A").arg("warnings")
            .output()
            .map_err(|e| format!("failed to run rustc: {}", e))?;
        if !output.status.success() {
            return Err(String::from_utf8_lossy(&output.stderr).to_string());
        }
        return Ok(bin_path);
    }

    let cargo_toml = build_cargo_toml(base_toml, &all_deps);
    let src_dir = project_dir.join("src");
    std::fs::create_dir_all(&src_dir).map_err(|e| format!("failed to create {}: {}", src_dir.display(), e))?;
    std::fs::write(project_dir.join("Cargo.toml"), &cargo_toml)
        .map_err(|e| format!("failed to write Cargo.toml: {}", e))?;

    let mut final_code = rs_code.to_string();
    inject_native_modules(&mut final_code, source_root, &src_dir)?;
    inject_dep_natives(&mut final_code, source_root, &src_dir, project_dir)?;

    // Library modules may not define main — auto-generate an empty one for test builds
    if !final_code.contains("fn main(") && !final_code.contains("fn almide_main(") {
        final_code.push_str("\nfn main() {}\n");
    }

    std::fs::write(src_dir.join("main.rs"), &final_code)
        .map_err(|e| format!("failed to write main.rs: {}", e))?;

    // Use `cargo test --no-run` to build the test binary without running it
    let mut cmd = std::process::Command::new("cargo");
    inject_almide_par_if_rayon(&mut cmd, project_dir);
    cmd.arg("test").arg("--no-run").arg("--quiet").arg("--message-format=json")
        .current_dir(project_dir);

    let output = cmd.output().map_err(|e| format!("failed to run cargo: {}", e))?;
    if !output.status.success() {
        // --quiet suppresses cargo's own error display, but rustc messages
        // come through stdout as JSON (--message-format=json). Extract the
        // "rendered" field from each compiler-message so the user sees the
        // real error spans, not just "1 previous error; N warnings emitted".
        let mut rendered: Vec<String> = Vec::new();
        let verbose = std::env::var("ALMIDE_TEST_VERBOSE")
            .map(|v| v == "1" || v == "true")
            .unwrap_or(false);
        for line in String::from_utf8_lossy(&output.stdout).lines() {
            if let Ok(json) = serde_json::from_str::<serde_json::Value>(line) {
                if json.get("reason").and_then(|r| r.as_str()) == Some("compiler-message") {
                    let level = json.get("message")
                        .and_then(|m| m.get("level"))
                        .and_then(|l| l.as_str())
                        .unwrap_or("");
                    if level == "error" || verbose {
                        if let Some(msg) = json.get("message")
                            .and_then(|m| m.get("rendered"))
                            .and_then(|r| r.as_str())
                        {
                            rendered.push(msg.to_string());
                        }
                    }
                }
            }
        }
        let stderr = String::from_utf8_lossy(&output.stderr).to_string();
        let combined = if rendered.is_empty() {
            stderr
        } else {
            format!("{}\n{}", rendered.join("\n"), stderr)
        };
        return Err(wrap_codegen_leak(combined));
    }

    // Parse the JSON output to find the test binary path
    let stdout = String::from_utf8_lossy(&output.stdout);
    for line in stdout.lines() {
        if let Ok(json) = serde_json::from_str::<serde_json::Value>(line) {
            if json.get("reason").and_then(|r| r.as_str()) == Some("compiler-artifact") {
                if let Some(exe) = json.get("executable").and_then(|e| e.as_str()) {
                    return Ok(std::path::PathBuf::from(exe));
                }
            }
        }
    }

    Err("could not determine test binary path from cargo output".to_string())
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
            let path = entry.path();
            if path.is_dir() {
                files.extend(collect_test_files(&path));
            } else if path.extension().map(|e| e == "almd").unwrap_or(false) {
                // Check if file contains a test block
                if let Ok(content) = std::fs::read_to_string(&path) {
                    if content.contains("\ntest ") || content.starts_with("test ") {
                        files.push(path.to_string_lossy().to_string());
                    }
                }
            }
        }
    }
    files
}

/// Detect rustc-style `error[E\d{4}]` codes leaking through our checker.
/// Almide's diagnostic codes are 3 digits (E001..E099); rustc uses 4 digits
/// (E0001..E9999). A 4-digit code in the output unambiguously means our
/// codegen produced invalid Rust — flag it for the bug-report wrapper so
/// dojo classifiers don't mistake it for a user-facing language error.
fn contains_rustc_error_code(text: &str) -> bool {
    let bytes = text.as_bytes();
    let needle = b"error[E";
    let mut i = 0;
    while i + needle.len() < bytes.len() {
        if &bytes[i..i + needle.len()] == needle {
            // Count consecutive digits after `error[E`
            let mut j = i + needle.len();
            let mut digits = 0;
            while j < bytes.len() && bytes[j].is_ascii_digit() {
                digits += 1;
                j += 1;
            }
            if digits >= 4 && j < bytes.len() && bytes[j] == b']' {
                return true;
            }
            i = j;
        } else {
            i += 1;
        }
    }
    false
}

#[cfg(test)]
mod tests {
    use super::contains_rustc_error_code;

    #[test]
    fn detects_4_digit_rustc_code() {
        assert!(contains_rustc_error_code("error[E0599]: no method named 'foo' found"));
        assert!(contains_rustc_error_code("blah\nerror[E0382]: use of moved value\nblah"));
    }

    #[test]
    fn ignores_3_digit_almide_code() {
        assert!(!contains_rustc_error_code("error[E001]: type mismatch"));
        assert!(!contains_rustc_error_code("error[E013]: ..."));
    }

    #[test]
    fn ignores_no_brackets() {
        assert!(!contains_rustc_error_code("error: something went wrong"));
        assert!(!contains_rustc_error_code(""));
    }
}
