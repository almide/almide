use std::process::Command;
use crate::{parse_file, canonicalize, check, diagnostic, resolve, project, project_fetch};

pub fn cmd_build(file: &str, output: Option<&str>, target: Option<&str>, release: bool, fast: bool, _unchecked_index: bool, no_check: bool, repr_c: bool, cdylib: bool, emit_unverified: bool, verified: bool, native_verified: bool) {
    // The npm/JavaScript target was removed with the TS backend; reject it with a
    // clear pointer instead of emitting a non-functional stub package.
    if matches!(target, Some("npm" | "js" | "ts" | "javascript" | "typescript")) {
        let t = target.unwrap_or("npm");
        eprintln!(
            "error: the npm/JavaScript build target has been removed\n  \
             in `almide build --target {t}`\n  \
             supported targets: rust (default, native binary), wasm\n  \
             hint: use `--target wasm` for a portable build"
        );
        std::process::exit(2);
    }
    let is_wasm = matches!(target, Some("wasm" | "wasm32" | "wasi"));
    let is_wasm_direct = matches!(target, Some("wasm"));

    // Direct WASM emit: .almd → IR → WASM binary (no rustc)
    if is_wasm_direct {
        cmd_build_wasm_direct(file, output, no_check, emit_unverified, verified);
        return;
    }

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
    let output = if cfg!(target_os = "windows") && !is_wasm
        && !output_raw.ends_with(".exe") && !output_raw.ends_with(".wasm")
    {
        format!("{}.exe", output_raw)
    } else {
        output_raw.to_string()
    };

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
        let source_text = std::fs::read_to_string(file).unwrap_or_default();
        match almide_mir::pipeline::try_render_rust_source(&source_text) {
            Ok(v1_code) => {
                if std::env::var("ALMIDE_VERIFIED_DEBUG").is_ok() {
                    eprintln!("native: v1 trust-spine render");
                }
                v1_code
            }
            Err(e) => {
                if std::env::var("ALMIDE_VERIFIED_DEBUG").is_ok() {
                    eprintln!("native: v1 walled ({e:?}) — falling back to v0 codegen");
                }
                rs_code
            }
        }
    } else {
        rs_code
    };

    // Load native deps from almide.toml (search in input file's directory, then
    // CWD). BOTH the cdylib and bin paths need them: a cdylib with a `native/*.rs`
    // shim or a `[native-deps]` crate must wire them in exactly like a bin, or it
    // fails with E0433 / a missing dep (#719). source_root is the directory
    // containing almide.toml (where native/ lives).
    let use_release = release || fast;
    let file_dir = std::path::Path::new(file).parent()
        .map(|p| if p.as_os_str().is_empty() { std::path::PathBuf::from(".") } else { p.to_path_buf() })
        .unwrap_or_else(|| std::path::PathBuf::from("."));
    let toml_path = {
        let candidate = file_dir.join("almide.toml");
        if candidate.exists() { candidate } else { std::path::PathBuf::from("almide.toml") }
    };
    let parsed = toml_path.exists()
        .then(|| {
            // Mirror run.rs: a broken almide.toml silently dropped
            // [native-deps]/native/ injection → opaque E0433 downstream.
            project::parse_toml(&toml_path)
                .map_err(|e| eprintln!("warning: {} ignored: {}", toml_path.display(), e))
                .ok()
        })
        .flatten();
    let native_deps = parsed.as_ref().map(|p| p.native_deps.as_slice()).unwrap_or(&[]);
    let toml_dir = toml_path.parent()
        .map(|p| if p.as_os_str().is_empty() { std::path::PathBuf::from(".") } else { p.to_path_buf() })
        .unwrap_or_else(|| std::path::PathBuf::from("."));
    let has_deps = parsed.as_ref().map_or(false, |p| !p.dependencies.is_empty());
    let source_root = if !native_deps.is_empty() || has_deps { Some(toml_dir.as_path()) } else { None };

    // cdylib target: build shared library (.dylib/.so)
    if cdylib {
        let project_dir = std::env::temp_dir().join("almide-build-cdylib");
        // Strip fn main() from the code — cdylib has no entry point
        let lib_code = rs_code.replace("fn main()", "fn __almide_unused_main()");
        // Serialize across processes: the shared scratch dir's src + target would
        // otherwise be corrupted by a concurrent `almide build`.
        let _ = std::fs::create_dir_all(&project_dir);
        let _flock = super::run::BuildDirLock::acquire(&project_dir)
            .unwrap_or_else(|e| { eprintln!("{}", e); std::process::exit(1); });
        match super::cargo_build_cdylib(&lib_code, &project_dir, &output, use_release, native_deps, source_root) {
            Ok(lib_path) => {
                eprintln!("Built {}", lib_path.display());
            }
            Err(e) => {
                eprintln!("Compile error:\n{}", e);
                std::process::exit(1);
            }
        }
        return;
    }

    // Native target: use cargo to resolve rustls/webpki-roots for HTTPS support
    let project_dir = std::env::temp_dir().join("almide-build");
    // Serialize across processes: the shared scratch dir's src + target would
    // otherwise be corrupted by a concurrent `almide build`. Held through the
    // copy-out below so the generated binary can't be overwritten between
    // build and copy.
    let _ = std::fs::create_dir_all(&project_dir);
    let _flock = super::run::BuildDirLock::acquire(&project_dir)
        .unwrap_or_else(|e| { eprintln!("{}", e); std::process::exit(1); });
    match super::cargo_build_generated_with_native(&rs_code, &project_dir, use_release, native_deps, source_root) {
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
            if let Err(e) = std::fs::copy(&bin_path, &output) {
                eprintln!("Failed to copy binary to {}: {}", output, e);
                std::process::exit(1);
            }
            eprintln!("Built {}", output);
        }
        Err(e) => {
            eprintln!("Compile error:\n{}", e);
            std::process::exit(1);
        }
    }
}

/// Build for WASI target using bare rustc (no external crate deps).
fn cmd_build_wasi_rustc(rs_code: &str, output: &str) {
    let stem = output.strip_suffix(".wasm").unwrap_or(output);
    let tmp_rs = format!("{}.rs", stem);
    if let Err(e) = std::fs::write(&tmp_rs, rs_code) {
        eprintln!("Failed to write {}: {}", tmp_rs, e);
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
        .unwrap_or_else(|e| { eprintln!("Failed to run rustc: {}", e); std::process::exit(1); });

    let _ = std::fs::remove_file(&tmp_rs);

    if !rustc.status.success() {
        let stderr = String::from_utf8_lossy(&rustc.stderr);
        eprintln!("Compile error:\n{}", stderr);
        std::process::exit(1);
    }

    eprintln!("Built {}", output);
}

/// Direct WASM emit: parse → check → lower → optimize → monomorphize → emit WASM binary.
fn cmd_build_wasm_direct(file: &str, output: Option<&str>, _no_check: bool, allow_unverified: bool, verified: bool) {
    let default_output = format!("{}.wasm", file.strip_suffix(".almd").unwrap_or("a.out"));
    let output = output.unwrap_or(&default_output);

    // The whole parse→check→lower→emit pipeline lives in `compile_to_wasm_bytes`
    // so `almide run --target wasm` produces the byte-identical module this
    // command writes — the cross-target equivalence guarantee depends on both
    // entry points sharing one code path. Any compile diagnostic was already
    // printed there; we just propagate the exit.
    let (bytes, produced_by_v1) = match compile_to_wasm_bytes(file, allow_unverified, verified) {
        Ok(b) => b,
        Err(()) => std::process::exit(1),
    };

    let pre_size = bytes.len();
    if let Err(e) = std::fs::write(output, &bytes) {
        eprintln!("Failed to write {}: {}", output, e);
        std::process::exit(1);
    }

    // A PCC-VERIFIED v1 module is shipped AS-IS: wasm-opt is an UNVERIFIED transform, so running it
    // would replace the exact bytes the trust-spine verified. `--verified` therefore emits the
    // verified module verbatim (a v0 fallback build still gets wasm-opt, exactly as without the flag).
    if produced_by_v1 {
        eprintln!("Built {} ({} bytes, v1-verified — wasm-opt skipped)", output, pre_size);
        return;
    }

    // Post-process: wasm-opt -O3 (binaryen) shrinks size + sometimes helps
    // perf via constant prop / dead-store elim across stdlib calls. Default-on
    // (round-trip verified across spec/ on WASM target — 197 pass identical
    // with and without wasm-opt). Opt-out via ALMIDE_NO_WASM_OPT=1.
    // Silent skip if wasm-opt is not installed.
    let opt_out = std::env::var("ALMIDE_NO_WASM_OPT").map(|v| v == "1" || v == "true").unwrap_or(false);
    if !opt_out {
        if let Ok(post_size) = run_wasm_opt(output) {
            let pct = if pre_size > 0 { 100.0 * (pre_size - post_size) as f64 / pre_size as f64 } else { 0.0 };
            eprintln!("Built {} ({} bytes → {} bytes, -{:.1}%)", output, pre_size, post_size, pct);
            return;
        }
        // wasm-opt not available → silent fallback to unoptimized output.
    }
    eprintln!("Built {} ({} bytes)", output, pre_size);
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
pub(crate) fn compile_to_wasm_bytes(file: &str, allow_unverified: bool, verified: bool) -> Result<(Vec<u8>, bool), ()> {
    let (mut program, source_text, parse_errors) = parse_file(file);

    if !parse_errors.is_empty() {
        for e in &parse_errors {
            eprintln!("{}", crate::diagnostic_render::display_with_source(e, &source_text));
        }
        return Err(());
    }

    // Resolve dependencies
    let dep_paths: Vec<(project::PkgId, std::path::PathBuf)> = if std::path::Path::new("almide.toml").exists() {
        if let Ok(proj) = project::parse_toml(std::path::Path::new("almide.toml")) {
            match project_fetch::fetch_all_deps(&proj) {
                Ok(deps) => deps.into_iter().map(|fd| (fd.pkg_id, fd.source_dir)).collect(),
                Err(e) => { eprintln!("{}", e); return Err(()); }
            }
        } else {
            vec![]
        }
    } else {
        vec![]
    };

    let mut resolved = match resolve::resolve_imports_with_deps(file, &program, &dep_paths) {
        Ok(r) => r,
        Err(e) => { eprintln!("{}", e); return Err(()); }
    };

    // v1 `--verified`: capture the FRESH (un-inferred) cross-module siblings now, before the loop
    // below mutates them in place — the v1 pipeline re-runs its own canonicalize/infer/lower from
    // raw programs (exactly the render_program example's `discover_self_modules` input).
    let v1_self_modules: Vec<(String, almide_lang::ast::Program, bool)> = if verified {
        resolved.modules.iter().map(|(n, p, _pkg, s)| (n.clone(), p.clone(), *s)).collect()
    } else {
        Vec::new()
    };

    // Type check
    let canon = canonicalize::canonicalize_program(
        &program,
        resolved.modules.iter().map(|(n, p, _, s)| (n.as_str(), p, *s)),
    );
    let mut checker = check::Checker::from_env(canon.env);
    checker.set_source(file, &source_text);
    checker.diagnostics = canon.diagnostics;
    // #785: module top-let types must be fully inferred before the entry
    // program reads them (drivers infer the entry FIRST; without this the
    // readers see the registration seed — Unknown for non-literal inits).
    almide::resolve::refresh_module_toplets(&mut checker, &resolved.modules);
    let diagnostics = checker.infer_program(&mut program);
    if diagnostics.iter().any(|d| d.level == diagnostic::Level::Error) {
        for d in &diagnostics {
            eprintln!("{}", crate::diagnostic_render::display_with_source(d, &source_text));
        }
        return Err(());
    }

    // Pre-register versioned names before root lowering
    for (name, _, pkg_id, _) in &resolved.modules {
        if let Some(pid) = pkg_id.as_ref() {
            let base = pid.mod_name();
            let v = if let Some(suffix) = name.strip_prefix(&pid.name) { format!("{}{}", base, suffix) } else { base };
            checker.env.module_versioned_names.insert(almide::intern::sym(name), almide::intern::sym(&v));
        }
    }
    let mut ir_program = almide::lower::lower_program(&program, &checker.env, &checker.type_map);

    // Lower user modules to IR. Bundled stdlib modules (stdlib/<m>.almd) are
    // included so their fns can be invoked through the bundled-dispatch path;
    // colliding TOML-runtime fns are pruned to avoid duplicate definitions.
    for (name, mod_prog, pkg_id, _) in &mut resolved.modules {
        if almide::stdlib::is_stdlib_module(name) && !almide::stdlib::is_bundled_module(name) { continue; }
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

    // IR link: merge dependency modules into root
    almide::ir_link::ir_link(&mut ir_program);

    // Optimize
    almide::optimize::optimize_program(&mut ir_program);

    // Monomorphize
    almide::mono::monomorphize(&mut ir_program);

    // Verify IR integrity — gate the WASM emit on the same check the native
    // path (main.rs) enforces. Without this an invalid IR (e.g. an unresolved
    // closure-call result type) is emitted as a structurally-broken module that
    // `almide build` reports as success (rc 0) but wasmtime refuses to load.
    let verify_errors = almide::ir::verify_program(&ir_program);
    if !verify_errors.is_empty() {
        for e in &verify_errors {
            eprintln!("internal compiler error: {}", e);
        }
        eprintln!("{} IR verification error(s) — no WASM emitted", verify_errors.len());
        return Err(());
    }

    // Native-only matrix ops (e.g. qwen3_block_q1_0_kv: a packed-GGUF block with
    // no primitive decomposition) have no WASM lowering. Reject at build time with
    // a clear message rather than letting the emitter ICE deep in codegen.
    if let Some(op) = almide::codegen::program_uses_native_only_matrix_on_wasm(&ir_program) {
        eprintln!(
            "error: matrix.{op} is native-only (a packed-GGUF fast path with no WASM \
             lowering) — not available on the WASM target. Use --target rust, or compose \
             the block from the primitive matrix ops."
        );
        return Err(());
    }

    // v1 OPT-IN verified codegen: after every v0 gate above (type-check, IR-verify, native-matrix
    // guard) has passed, TRY the PCC-verified trust-spine renderer. It is byte-
    // identical to v0 where it lowers and WALLS (`Err`) otherwise — on a wall we fall through to
    // v0 codegen below. Honest-wall: a v1 module is never wrong; a walled program builds via v0
    // exactly as without `--verified`.
    if verified {
        match almide_mir::pipeline::try_render_wasm_source(&source_text, &v1_self_modules, false) {
            Ok(wat) => {
                if let Ok(bytes) = wat::parse_str(&wat) {
                    if std::env::var("ALMIDE_VERIFIED_DEBUG").is_ok() {
                        eprintln!(
                            "[almide] --verified: v1 trust-spine emitted the module ({} bytes)",
                            bytes.len()
                        );
                    }
                    return Ok((bytes, true));
                }
            }
            Err(e) => {
                if std::env::var("ALMIDE_VERIFIED_DEBUG").is_ok() {
                    // Name WHICH wall fired — the burn-down (and any user staring at a
                    // fallback) needs the reason, exactly like the native leg's message.
                    eprintln!("[almide] --verified: v1 walled ({e:?}) — falling back to v0 codegen");
                }
            }
        }
    }

    // Codegen (nanopass pipeline + WASM binary emit). The Perceus RC gate
    // (`Verified::verify`) runs inside this path; `allow_unverified` selects
    // hard-error (default) vs the `--emit-unverified` waiver. It does not change
    // emitted bytes, so the build/run cross-target byte-identity still holds.
    let opts = almide::codegen::CodegenOptions { repr_c: false, allow_unverified };
    let bytes = match almide::codegen::codegen_with(&mut ir_program, almide::codegen::pass::Target::Wasm, &opts) {
        almide::codegen::CodegenOutput::Binary(b) => b,
        almide::codegen::CodegenOutput::Source(_) => unreachable!(),
    };
    Ok((bytes, false))
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

