use std::process::Command;
use crate::{parse_file, canonicalize, check, diagnostic, resolve, project, project_fetch, err};

/// Flags for [`cmd_build`] — bundled into one struct (was 12 positional
/// params, a max-params violation on its own) so the function signature
/// stays under the params threshold. Field names match `Commands::Build`'s
/// clap fields 1:1, so the call site in `main.rs` builds it directly from
/// the destructured match arm.
pub struct BuildArgs<'a> {
    pub file: &'a str,
    pub output: Option<&'a str>,
    pub target: Option<&'a str>,
    pub release: bool,
    pub fast: bool,
    pub unchecked_index: bool,
    pub no_check: bool,
    pub repr_c: bool,
    pub cdylib: bool,
    pub emit_unverified: bool,
    pub verified: bool,
    pub native_verified: bool,
    pub wasm_opt: bool,
}

/// The npm/JavaScript target was removed with the TS backend; reject it with
/// a clear pointer instead of emitting a non-functional stub package. Exits
/// the process (never returns) when `target` names a removed target.
fn reject_removed_target(target: Option<&str>) {
    if matches!(target, Some("npm" | "js" | "ts" | "javascript" | "typescript")) {
        let t = target.unwrap_or("npm");
        err(&format!(
            "error: the npm/JavaScript build target has been removed\n  \
             in `almide build --target {t}`\n  \
             supported targets: rust (default, native binary), wasm\n  \
             hint: use `--target wasm` for a portable build"
        ));
        std::process::exit(2);
    }
}

/// Compute the build's output path: `file`/`almide.toml`-derived default
/// unless `-o` was given, plus the Windows `.exe` auto-suffix for native
/// builds. Extracted verbatim from `cmd_build`.
fn compute_output_path(file: &str, output: Option<&str>, is_wasm: bool) -> String {
    let default_output = if is_wasm {
        format!("{}.wasm", file.strip_suffix(".almd").unwrap_or("a.out"))
    } else if std::path::Path::new("almide.toml").exists() {
        let toml_content = std::fs::read_to_string("almide.toml").unwrap_or_default();
        toml_content.lines()
            .find(|l| l.starts_with("name"))
            .and_then(|l| l.split('=').nth(1))
            .map(|s| s.trim().trim_matches('"').to_string())
            .unwrap_or_else(|| file.strip_suffix(".almd").unwrap_or("a.out").to_string())
    } else {
        file.strip_suffix(".almd").unwrap_or("a.out").to_string()
    };
    let output_raw = output.unwrap_or(&default_output);

    // On Windows, auto-append .exe for native builds
    if cfg!(target_os = "windows") && !is_wasm
        && !output_raw.ends_with(".exe") && !output_raw.ends_with(".wasm")
    {
        format!("{}.exe", output_raw)
    } else {
        output_raw.to_string()
    }
}

/// `cmd_build`'s cdylib target: build a shared library (.dylib/.so).
/// Extracted verbatim — exits the process on a compile error, otherwise
/// prints the built path and returns.
fn cmd_build_cdylib(rs_code: &str, output: &str, use_release: bool, native_deps: &[project::NativeDep], source_root: Option<&std::path::Path>) {
    let project_dir = std::env::temp_dir().join("almide-build-cdylib");
    // Strip fn main() from the code — cdylib has no entry point
    let lib_code = rs_code.replace("fn main()", "fn __almide_unused_main()");
    // Serialize across processes: the shared scratch dir's src + target would
    // otherwise be corrupted by a concurrent `almide build`.
    let _ = std::fs::create_dir_all(&project_dir);
    let _flock = super::run::BuildDirLock::acquire(&project_dir)
        .unwrap_or_else(|e| { err(&format!("{}", e)); std::process::exit(1); });
    match super::cargo_build_cdylib(&lib_code, &project_dir, output, use_release, native_deps, source_root) {
        Ok(lib_path) => {
            err(&format!("Built {}", lib_path.display()));
        }
        Err(e) => {
            err(&format!("Compile error:\n{}", e));
            std::process::exit(1);
        }
    }
}

/// `cmd_build`'s native binary target: the content-cached build shared with
/// `almide run` — the cache key is the generated code, so identical output
/// from any caller (or any source path) reuses one binary and skips cargo
/// entirely. Locking and atomic binary staging live inside
/// `build_native_cached`; the copy-out below reads a content-named,
/// atomically-renamed file, so it needs no lock. Extracted verbatim.
fn cmd_build_native(rs_code: &str, output: &str, use_release: bool, native_deps: &[project::NativeDep], source_root: Option<&std::path::Path>) {
    match super::run::build_native_cached(rs_code, false, use_release, None, native_deps, source_root) {
        Ok(bin_path) => {
            // Copy the built binary to the desired output location. Create the
            // output's parent directory first — `-o build/app` must not fail
            // just because `build/` doesn't exist yet (it's the natural place
            // to put a binary, and every caller otherwise needs a manual mkdir).
            if let Some(parent) = std::path::Path::new(&output).parent() {
                if !parent.as_os_str().is_empty() {
                    let _ = std::fs::create_dir_all(parent);
                }
            }
            if let Err(e) = std::fs::copy(&bin_path, output) {
                err(&format!("Failed to copy binary to {}: {}", output, e));
                std::process::exit(1);
            }
            err(&format!("Built {}", output));
        }
        Err(e) => {
            err(&format!("Compile error:\n{}", e));
            std::process::exit(1);
        }
    }
}

pub fn cmd_build(args: BuildArgs) {
    // Destructured immediately so the function body below is untouched
    // (verbatim) — this is purely a call-site params bundling.
    let BuildArgs {
        file, output, target, release, fast, unchecked_index: _unchecked_index,
        no_check, repr_c, cdylib, emit_unverified, verified, native_verified, wasm_opt,
    } = args;
    reject_removed_target(target);
    let is_wasm = matches!(target, Some("wasm" | "wasm32" | "wasi"));
    let is_wasm_direct = matches!(target, Some("wasm"));

    // Direct WASM emit: .almd → IR → WASM binary (no rustc)
    if is_wasm_direct {
        cmd_build_wasm_direct(file, output, no_check, emit_unverified, verified, wasm_opt);
        return;
    }

    let output = compute_output_path(file, output, is_wasm);

    let opts = crate::codegen::CodegenOptions { repr_c, allow_unverified: false };
    let (rs_code, _ir) = crate::try_compile_with_ir(file, no_check, &opts)
        .unwrap_or_else(|_| std::process::exit(1));

    // WASI target: use bare rustc (no external crate deps needed for WASM)
    if is_wasm {
        cmd_build_wasi_rustc(&rs_code, &output);
        return;
    }

    // NATIVE trust spine (#764, rung 1) — OPT-IN `--verified` (explicit flag, not
    // the wasm default): try the v1 MIR renderer (same Perceus MIR as the wasm
    // leg; Drop erased to Rust scope-end, ownership verified pre-render). A WALL
    // falls back to the v0 source above — honest-wall discipline: a v1-rendered
    // program is never wrong.
    let rs_code = if native_verified && !repr_c && !cdylib {
        super::render_v1_native_or_fallback(file, rs_code)
    } else {
        rs_code
    };

    // Load native deps from almide.toml (search in input file's directory, then
    // CWD). BOTH the cdylib and bin paths need them: a cdylib with a `native/*.rs`
    // shim or a `[native-deps]` crate must wire them in exactly like a bin, or it
    // fails with E0433 / a missing dep (#719). source_root is the directory
    // containing almide.toml (where native/ lives).
    let use_release = release || fast;
    let (native_deps, source_root) = super::load_native_build_config(file);

    // cdylib target: build shared library (.dylib/.so)
    if cdylib {
        cmd_build_cdylib(&rs_code, &output, use_release, &native_deps, source_root.as_deref());
        return;
    }

    cmd_build_native(&rs_code, &output, use_release, &native_deps, source_root.as_deref());
}

/// Build for WASI target using bare rustc (no external crate deps).
fn cmd_build_wasi_rustc(rs_code: &str, output: &str) {
    let stem = output.strip_suffix(".wasm").unwrap_or(output);
    let tmp_rs = format!("{}.rs", stem);
    if let Err(e) = std::fs::write(&tmp_rs, rs_code) {
        err(&format!("Failed to write {}: {}", tmp_rs, e));
        std::process::exit(1);
    }

    let rustc = Command::new(&crate::find_rustc())
        .arg(&tmp_rs)
        .arg("-o").arg(output)
        .arg("-C").arg("overflow-checks=no")
        .arg("--edition").arg("2021")
        .arg("--target").arg("wasm32-wasip1")
        .arg("-C").arg("opt-level=3")
        .arg("-C").arg("lto=yes")
        // Enable WASM SIMD128 — all modern runtimes support it (wasmtime,
        // browsers since ~2022). Unlocks LLVM auto-vectorization for matmul.
        .arg("-C").arg("target-feature=+simd128")
        .output()
        .unwrap_or_else(|e| { err(&format!("Failed to run rustc: {}", e)); std::process::exit(1); });

    let _ = std::fs::remove_file(&tmp_rs);

    if !rustc.status.success() {
        let stderr = String::from_utf8_lossy(&rustc.stderr);
        err(&format!("Compile error:\n{}", stderr));
        std::process::exit(1);
    }

    err(&format!("Built {}", output));
}

/// Direct WASM emit: parse → check → lower → optimize → monomorphize → emit WASM binary.
fn cmd_build_wasm_direct(file: &str, output: Option<&str>, _no_check: bool, allow_unverified: bool, verified: bool, wasm_opt: bool) {
    let default_output = format!("{}.wasm", file.strip_suffix(".almd").unwrap_or("a.out"));
    let output = output.unwrap_or(&default_output);

    // The whole parse→check→lower→emit pipeline lives in `compile_to_wasm_bytes`
    // so `almide run --target wasm` produces the byte-identical module this
    // command writes — the cross-target equivalence guarantee depends on both
    // entry points sharing one code path. Any compile diagnostic was already
    // printed there; we just propagate the exit.
    let (bytes, _produced_by_v1) = match compile_to_wasm_bytes(file, allow_unverified, verified) {
        Ok(b) => b,
        Err(()) => std::process::exit(1),
    };

    let pre_size = bytes.len();
    if let Err(e) = std::fs::write(output, &bytes) {
        err(&format!("Failed to write {}: {}", output, e));
        std::process::exit(1);
    }

    // The trust-spine ships the bytes ITS OWN rendering process produced —
    // reachability DCE and the name-section trim already ran inside that
    // pipeline (docs/WASM-OUTPUT.md). `wasm-opt` is a different kind of
    // thing: an EXTERNAL, unverified transform applied to the renderer's
    // finished output, so running it replaces bytes the trust-spine produced
    // with bytes a separate, un-certified tool rewrote. That is why it stays
    // an explicit, default-off opt-in (`--wasm-opt`) rather than automatic —
    // see the wasm-opt parity leg (`tests/wasm_opt_parity_test.rs`) for the
    // differential-testing evidence backing this tier's own guarantee.
    if !wasm_opt {
        err(&format!(
            "Built {} ({} bytes, v1-verified — wasm-opt skipped; pass --wasm-opt for a smaller, non-verified build)",
            output, pre_size
        ));
        return;
    }

    match run_wasm_opt(output) {
        Ok(post_size) => {
            let pct = if pre_size > 0 { 100.0 * (pre_size - post_size) as f64 / pre_size as f64 } else { 0.0 };
            err(&format!(
                "Built {} ({} bytes → {} bytes, -{:.1}%) — wasm-opt applied: this is NOT the trust-spine-verified module",
                output, pre_size, post_size, pct
            ));
        }
        Err(_) => {
            err(&format!(
                "Built {} ({} bytes) — --wasm-opt requested but wasm-opt is not installed; shipped the verified module unoptimized",
                output, pre_size
            ));
        }
    }
}

/// Compile an `.almd` file to a raw wasm32-wasi module (no wasm-opt, no file IO).
///
/// This is the single source of truth for the direct-WASM pipeline, shared by
/// `almide build --target wasm` and `almide run --target wasm`, so both emit
/// the byte-identical module the cross-target equivalence guarantee promises.
/// Compile diagnostics are rendered to stderr here; on any error it returns
/// `Err(())` and the caller decides how to terminate.
/// Returns `(wasm_bytes, produced_by_v1)`. When the second field is `true`, the module IS the
/// PCC-verified v1 trust-spine output — the caller MUST NOT post-process it (wasm-opt would replace
/// the verified bytes with an unverified transform), so `--verified` ships exactly what was verified.
/// Type-check and lower one user module for the WASM path, appending its IR
/// directly onto `ir_program`. Extracted verbatim from
/// `compile_to_wasm_bytes`'s per-module loop body — same checker/env
/// mutation order, `continue` becomes an early `return`. (Sibling of
/// `crate::lower_one_user_module` in main.rs, which additionally tracks a
/// per-module `module_irs` map the WASM path doesn't need.)
pub(super) fn lower_one_wasm_module(
    checker: &mut check::Checker,
    name: &mut String,
    mod_prog: &mut almide::ast::Program,
    pkg_id: &mut Option<project::PkgId>,
    ir_program: &mut almide::ir::IrProgram,
) {
    if almide::stdlib::is_stdlib_module(name) && !almide::stdlib::is_bundled_module(name) { return; }
    let saved_self = checker.env.self_module_name;
    if let Some(pid) = pkg_id.as_ref() {
        checker.env.self_module_name = Some(almide::intern::sym(&pid.name));
    }
    checker.infer_module(mod_prog, name);
    let versioned = pkg_id.as_ref().map(|pid| {
        let base = pid.mod_name();
        if let Some(suffix) = name.strip_prefix(&pid.name) {
            format!("{}{}", base, suffix)
        } else {
            base
        }
    });
    if let Some(ref v) = versioned {
        checker.env.module_versioned_names.insert(almide::intern::sym(name), almide::intern::sym(v));
    }
    let self_name = checker.env.self_module_name.map(|s| s.to_string());
    let import_table_name = self_name.as_deref().unwrap_or(name);
    let (mod_table, _) = almide::import_table::build_import_table(mod_prog, Some(import_table_name), &checker.env.user_modules);
    let saved_table = std::mem::replace(&mut checker.env.import_table, mod_table);
    let mod_ir_module = almide::lower::lower_module(name, mod_prog, &checker.env, &checker.type_map, versioned);
    checker.env.import_table = saved_table;
    checker.env.self_module_name = saved_self;
    ir_program.modules.push(mod_ir_module);
}

/// `compile_to_wasm_bytes`'s parse + dependency-fetch + import-resolution
/// phase. Extracted verbatim — prints diagnostics and returns `Err(())` on
/// any parse/fetch/resolve failure, mirroring the original early returns.
fn parse_and_resolve_wasm(file: &str) -> Result<(almide::ast::Program, String, resolve::ResolvedModules), ()> {
    let (program, source_text, parse_errors) = parse_file(file);

    if !parse_errors.is_empty() {
        for e in &parse_errors {
            err(&format!("{}", crate::diagnostic_render::display_with_source(e, &source_text)));
        }
        return Err(());
    }

    // Resolve dependencies
    let dep_paths: Vec<(project::PkgId, std::path::PathBuf)> = if std::path::Path::new("almide.toml").exists() {
        if let Ok(proj) = project::parse_toml(std::path::Path::new("almide.toml")) {
            match project_fetch::fetch_all_deps(&proj) {
                Ok(deps) => deps.into_iter().map(|fd| (fd.pkg_id, fd.source_dir)).collect(),
                Err(e) => { err(&format!("{}", e)); return Err(()); }
            }
        } else {
            vec![]
        }
    } else {
        vec![]
    };

    let resolved = match resolve::resolve_imports_with_deps(file, &program, &dep_paths) {
        Ok(r) => r,
        Err(e) => { err(&format!("{}", e)); return Err(()); }
    };

    Ok((program, source_text, resolved))
}

/// `compile_to_wasm_bytes`'s type-check phase: canonicalize, build the
/// `Checker`, refresh module top-let types (#785), and infer the entry
/// program. Extracted verbatim — prints diagnostics and returns `Err(())`
/// on any type error.
fn typecheck_wasm_program(file: &str, source_text: &str, program: &mut almide::ast::Program, resolved: &resolve::ResolvedModules) -> Result<check::Checker, ()> {
    let canon = canonicalize::canonicalize_program(
        program,
        resolved.modules.iter().map(|(n, p, _, s)| (n.as_str(), p, *s)),
    );
    let mut checker = check::Checker::from_env(canon.env);
    checker.set_source(file, source_text);
    checker.diagnostics = canon.diagnostics;
    // #785: module top-let types must be fully inferred before the entry
    // program reads them (drivers infer the entry FIRST; without this the
    // readers see the registration seed — Unknown for non-literal inits).
    almide::resolve::refresh_module_toplets(&mut checker, &resolved.modules);
    let diagnostics = checker.infer_program(program);
    if diagnostics.iter().any(|d| d.level == diagnostic::Level::Error) {
        for d in &diagnostics {
            err(&format!("{}", crate::diagnostic_render::display_with_source(d, source_text)));
        }
        return Err(());
    }
    Ok(checker)
}

/// `compile_to_wasm_bytes`'s IR construction phase: pre-register versioned
/// module names, lower the entry program, lower each resolved user module
/// (bundled stdlib included so `@inline_rust` fns reach the bundled-dispatch
/// path), link, optimize, and monomorphize. Extracted verbatim.
fn lower_and_link_wasm_ir(program: &almide::ast::Program, checker: &mut check::Checker, resolved: &mut resolve::ResolvedModules) -> almide::ir::IrProgram {
    // Pre-register versioned names before root lowering
    for (name, _, pkg_id, _) in &resolved.modules {
        if let Some(pid) = pkg_id.as_ref() {
            let base = pid.mod_name();
            let v = if let Some(suffix) = name.strip_prefix(&pid.name) { format!("{}{}", base, suffix) } else { base };
            checker.env.module_versioned_names.insert(almide::intern::sym(name), almide::intern::sym(&v));
        }
    }
    let mut ir_program = almide::lower::lower_program(program, &checker.env, &checker.type_map);

    // Lower user modules to IR. Bundled stdlib modules (stdlib/<m>.almd) are
    // included so their fns can be invoked through the bundled-dispatch path;
    // colliding TOML-runtime fns are pruned to avoid duplicate definitions.
    for (name, mod_prog, pkg_id, _) in &mut resolved.modules {
        lower_one_wasm_module(checker, name, mod_prog, pkg_id, &mut ir_program);
    }

    // IR link: merge dependency modules into root
    almide::ir_link::ir_link(&mut ir_program);

    // Optimize
    almide::optimize::optimize_program(&mut ir_program);

    // Monomorphize
    almide::mono::monomorphize(&mut ir_program);

    ir_program
}

/// `compile_to_wasm_bytes`'s IR-integrity gate — the same check the native
/// path (main.rs) enforces. Without this an invalid IR (e.g. an unresolved
/// closure-call result type) is emitted as a structurally-broken module that
/// `almide build` reports as success (rc 0) but wasmtime refuses to load.
/// Extracted verbatim.
fn verify_wasm_ir(ir_program: &almide::ir::IrProgram) -> Result<(), ()> {
    let verify_errors = almide::ir::verify_program(ir_program);
    if !verify_errors.is_empty() {
        for e in &verify_errors {
            err(&format!("internal compiler error: {}", e));
        }
        err(&format!("{} IR verification error(s) — no WASM emitted", verify_errors.len()));
        return Err(());
    }
    Ok(())
}

/// `compile_to_wasm_bytes`'s native-only-matrix-op guard: native-only matrix
/// ops (e.g. qwen3_block_q1_0_kv: a packed-GGUF block with no primitive
/// decomposition) have no WASM lowering. Reject at build time with a clear
/// message rather than letting the emitter ICE deep in codegen. Extracted
/// verbatim.
fn check_no_native_only_matrix(ir_program: &almide::ir::IrProgram) -> Result<(), ()> {
    if let Some(op) = almide::codegen::program_uses_native_only_matrix_on_wasm(ir_program) {
        err(&format!(
            "error: matrix.{op} is native-only (a packed-GGUF fast path with no WASM \
             lowering) — not available on the WASM target. Use --target rust, or compose \
             the block from the primitive matrix ops."
        ));
        return Err(());
    }
    Ok(())
}

/// `compile_to_wasm_bytes`'s v1 PCC-verified trust-spine render — the ONLY
/// wasm path (#782: the v0 wasm emitter is retired). A v1 wall is an
/// honest, diagnosed hard error, never a silent fallback into unverified
/// codegen: a program that compiles is verified, a program the renderer
/// cannot verify is refused with the wall reason (refusal over risk — the
/// medical-grade bar). Extracted verbatim.
fn render_wasm_module(source_text: &str, v1_self_modules: &[(String, almide_lang::ast::Program, bool)]) -> Result<(Vec<u8>, bool), ()> {
    match almide_mir::pipeline::try_render_wasm_source(
        source_text,
        v1_self_modules,
        std::env::var("ALMIDE_VERIFIED_DEBUG").is_ok(),
    ) {
        Ok(wat) => match wat::parse_str(&wat) {
            Ok(bytes) => {
                let bytes = strip_wasm_name_section(bytes);
                if std::env::var("ALMIDE_VERIFIED_DEBUG").is_ok() {
                    err(&format!(
                        "[almide] v1 trust-spine emitted the module ({} bytes)",
                        bytes.len()
                    ));
                }
                Ok((bytes, true))
            }
            Err(e) => {
                err(&format!("error: the v1 renderer produced unparsable WAT — this is an Almide bug: {e}"));
                Err(())
            }
        },
        Err(e) => {
            err(&format!(
                "error: this program shape is not yet supported by the verified wasm renderer\n  \
                 wall: {e:?}\n  \
                 The unverified v0 wasm emitter was retired (#782): a wall is now an honest\n  \
                 error instead of a silent fallback. Please file an issue with the wall reason\n  \
                 above and the source shape that triggered it:\n  \
                 https://github.com/almide/almide/issues"
            ));
            Err(())
        }
    }
}

pub(crate) fn compile_to_wasm_bytes(file: &str, allow_unverified: bool, verified: bool) -> Result<(Vec<u8>, bool), ()> {
    let (mut program, source_text, mut resolved) = parse_and_resolve_wasm(file)?;

    // v1 `--verified`: capture the FRESH (un-inferred) cross-module siblings now, before the loop
    // below mutates them in place — the v1 pipeline re-runs its own canonicalize/infer/lower from
    // raw programs (exactly the render_program example's `discover_self_modules` input).
    // #782: always collected — the v1 renderer is the only wasm path.
    let v1_self_modules: Vec<(String, almide_lang::ast::Program, bool)> =
        resolved.modules.iter().map(|(n, p, _pkg, s)| (n.clone(), p.clone(), *s)).collect();

    let mut checker = typecheck_wasm_program(file, &source_text, &mut program, &resolved)?;
    let mut ir_program = lower_and_link_wasm_ir(&program, &mut checker, &mut resolved);
    verify_wasm_ir(&ir_program)?;
    check_no_native_only_matrix(&ir_program)?;

    // v1 OPT-IN verified codegen: after every v0 gate above (type-check, IR-verify, native-matrix
    // guard) has passed, TRY the PCC-verified trust-spine renderer. It is byte-
    // identical to v0 where it lowers and WALLS (`Err`) otherwise — on a wall we fall through to
    // v0 codegen below. Honest-wall: a v1 module is never wrong; a walled program builds via v0
    // exactly as without `--verified`.
    let _ = (&mut ir_program, allow_unverified, verified);
    render_wasm_module(&source_text, &v1_self_modules)
}

/// Trim every "name"-id custom section down to its function-names
/// subsection, dropping local-names and any other subsection.
///
/// `wat::parse_str` always emits a name section recording every symbolic
/// `$name` the WAT source used — functions AND every per-function local
/// (`$v1`, `$v2`, ...). `docs/WASM-OUTPUT.md` commits to keeping function
/// names because they're what a wasmtime trap backtrace prints
/// (`<unknown>!funcname`) — the one piece of this metadata with real
/// diagnostic value. Local names carry none (wasmtime backtraces never
/// print them) and dominate the section's size: measured on
/// `closure.almd`, 251 named locals cost 1.6KB versus keeping only the 20
/// function names. The wasm spec defines custom sections — and every
/// subsection within the "name" section — as ignorable by any consumer
/// that doesn't recognize them (§2.5.9), so dropping subsections can never
/// change what the module computes; this is exactly as safe as the
/// preamble reachability DCE (`render_wasm_dce.rs`), just one level lower:
/// a format-legal removal, not a black-box "optimization".
fn strip_wasm_name_section(bytes: Vec<u8>) -> Vec<u8> {
    const HEADER_LEN: usize = 8; // b"\0asm" + version u32
    if bytes.len() < HEADER_LEN {
        return bytes;
    }
    let mut out = bytes[..HEADER_LEN].to_vec();
    let mut i = HEADER_LEN;
    while i < bytes.len() {
        let id = bytes[i];
        let Some((payload_len, len_bytes)) = read_leb128_u32(&bytes[i + 1..]) else {
            // Malformed length — bail out and keep everything from here on
            // verbatim rather than risk corrupting the module.
            out.extend_from_slice(&bytes[i..]);
            return out;
        };
        let payload_start = i + 1 + len_bytes;
        let payload_end = (payload_start + payload_len as usize).min(bytes.len());
        let is_name_section = id == 0 && custom_section_name(&bytes[payload_start..payload_end]) == Some("name");
        if is_name_section {
            if let Some(trimmed) = trim_name_section_to_function_names(&bytes[payload_start..payload_end]) {
                out.push(0);
                out.extend_from_slice(&write_leb128_u32(trimmed.len() as u32));
                out.extend_from_slice(&trimmed);
            }
            // Malformed name-section payload: drop it whole rather than risk
            // shipping a corrupt custom section — still format-legal (the
            // section is optional metadata, never load-bearing).
        } else {
            out.extend_from_slice(&bytes[i..payload_end]);
        }
        i = payload_end;
    }
    out
}

/// A custom section's payload starts with its own length-prefixed name string.
fn custom_section_name(payload: &[u8]) -> Option<&str> {
    let (name_len, len_bytes) = read_leb128_u32(payload)?;
    let name_bytes = payload.get(len_bytes..len_bytes + name_len as usize)?;
    std::str::from_utf8(name_bytes).ok()
}

/// A "name" custom section's payload is its own length-prefixed "name"
/// identifier string, followed by a sequence of subsections (id byte +
/// LEB128 length + payload) — id 1 is function names, the only one kept.
/// Returns `None` if the payload is too short to even contain the leading
/// identifier string (malformed).
fn trim_name_section_to_function_names(payload: &[u8]) -> Option<Vec<u8>> {
    let (name_len, len_bytes) = read_leb128_u32(payload)?;
    let prefix_end = len_bytes + name_len as usize;
    if prefix_end > payload.len() {
        return None;
    }
    let mut out = payload[..prefix_end].to_vec();
    let mut i = prefix_end;
    while i < payload.len() {
        let id = payload[i];
        let Some((sub_len, sub_len_bytes)) = read_leb128_u32(&payload[i + 1..]) else {
            return None;
        };
        let sub_start = i + 1 + sub_len_bytes;
        let sub_end = (sub_start + sub_len as usize).min(payload.len());
        if id == 1 {
            out.extend_from_slice(&payload[i..sub_end]);
        }
        i = sub_end;
    }
    Some(out)
}

/// Decode an unsigned LEB128 `u32` at the start of `bytes`. Returns the
/// decoded value and how many bytes it occupied, or `None` on overflow /
/// truncated input.
fn read_leb128_u32(bytes: &[u8]) -> Option<(u32, usize)> {
    let mut result: u32 = 0;
    let mut shift = 0u32;
    for (i, &byte) in bytes.iter().enumerate() {
        result |= ((byte & 0x7f) as u32).checked_shl(shift)?;
        if byte & 0x80 == 0 {
            return Some((result, i + 1));
        }
        shift += 7;
        if shift >= 32 {
            return None;
        }
    }
    None
}

/// Encode a `u32` as unsigned LEB128.
fn write_leb128_u32(mut v: u32) -> Vec<u8> {
    let mut out = Vec::new();
    loop {
        let byte = (v & 0x7f) as u8;
        v >>= 7;
        if v == 0 {
            out.push(byte);
            return out;
        }
        out.push(byte | 0x80);
    }
}

/// Run `wasm-opt -O3 --enable-simd` on the output file, in-place.
/// Returns the new file size on success.
fn run_wasm_opt(path: &str) -> Result<usize, String> {
    // --enable-bulk-memory required: matrix runtime emits memory.fill for
    // result buffer zero-init. --enable-simd preserves f64x2 instructions
    // from matrix.scale / add / sub / div / fma / fma3.
    // --enable-nontrapping-float-to-int: sized numeric conversions now
    // emit `i32.trunc_sat_f64_s` etc. (post-Stdlib-Unification, all
    // float→int routes through `emit_sized_conv_call`).
    let status = std::process::Command::new("wasm-opt")
        .args([
            "-O3",
            "--enable-simd",
            "--enable-bulk-memory",
            "--enable-nontrapping-float-to-int",
            "--enable-tail-call",
            path,
            "-o",
            path,
        ])
        .status()
        .map_err(|e| format!("wasm-opt not available ({})", e))?;
    if !status.success() {
        return Err(format!("wasm-opt failed (exit {:?})", status.code()));
    }
    let meta = std::fs::metadata(path).map_err(|e| format!("stat {}: {}", path, e))?;
    Ok(meta.len() as usize)
}

