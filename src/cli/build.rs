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
    pub heap_cap: Option<u32>,
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
            // Stage-and-RENAME, never copy onto an existing executable. A bare
            // `fs::copy` rewrites the destination IN PLACE (same inode), and on
            // macOS the kernel's code-signature cache is keyed by vnode: a
            // binary overwritten at the same inode after its previous content
            // was executed gets SIGKILLed on the next exec — no exit code, no
            // stderr, nothing to debug. `almide build app.almd -o app` twice in
            // a row then `./app` reproduced it sporadically, and the fuzzer's
            // per-worker reused output path hit it reliably deep into a
            // campaign (seed 1785165458340124000 index 572: a phantom
            // "native run failed while wasm succeeded"). The rename gives the
            // destination a fresh inode atomically; the staging temp lives in
            // the SAME directory so the rename cannot cross a filesystem.
            let staged = {
                let out_path = std::path::Path::new(&output);
                let file_name = out_path.file_name().map(|n| n.to_string_lossy().into_owned())
                    .unwrap_or_else(|| output.to_string());
                out_path.with_file_name(format!(".{}.staged-{}", file_name, std::process::id()))
            };
            let copy_then_rename = std::fs::copy(&bin_path, &staged)
                .and_then(|_| std::fs::rename(&staged, output));
            if let Err(e) = copy_then_rename {
                let _ = std::fs::remove_file(&staged);
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
        no_check, repr_c, cdylib, emit_unverified, verified, native_verified, wasm_opt, heap_cap,
    } = args;
    reject_removed_target(target);
    let is_wasm = matches!(target, Some("wasm" | "wasm32" | "wasi"));
    let is_wasm_direct = matches!(target, Some("wasm"));
    let heap_cap = heap_cap.filter(|n| *n > 0);

    // Direct WASM emit: .almd → IR → WASM binary (no rustc)
    if is_wasm_direct {
        // The knob rides a render-scoped guard, not a params thread-through:
        // the whole CLI runs on the one `almide-main` worker thread, so the
        // thread-local is exactly as scoped as this call.
        let _cap = heap_cap.map(almide_mir::heap_cap::HeapCapGuard::set);
        cmd_build_wasm_direct(file, output, no_check, emit_unverified, verified, wasm_opt);
        return;
    }

    let output = compute_output_path(file, output, is_wasm);

    let opts = crate::codegen::CodegenOptions { repr_c, allow_unverified: false };
    let (rs_code, _ir) = crate::try_compile_with_ir(file, no_check, &opts)
        .unwrap_or_else(|_| std::process::exit(1));

    // WASI target: use bare rustc (no external crate deps needed for WASM)
    if is_wasm {
        let rs_code = match heap_cap {
            Some(n) => inject_heap_cap(&rs_code, n),
            None => rs_code,
        };
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

    // --heap-cap (#1530): wrap the binary's allocator AFTER the v1/v0 source
    // decision so both native paths carry the same ceiling.
    let rs_code = match heap_cap {
        Some(n) => {
            // The rlib fast path SLIMS the generated source down to its main
            // body and splices a prebuilt runtime in — the injected allocator
            // block would be stripped with the prelude it sits in, and the
            // "capped" binary would silently carry no cap (exactly the
            // skip-as-pass shape this knob exists to kill). A cap build is a
            // harness build: take the self-contained cargo path.
            // SAFETY: the whole CLI runs sequentially on the one `almide-main`
            // worker thread and no other thread is alive to read the
            // environment concurrently.
            unsafe { std::env::set_var("ALMIDE_NO_RTLIB", "1") };
            inject_heap_cap(&rs_code, n)
        }
        None => rs_code,
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

/// `--heap-cap` (#1530): wrap the generated program's allocator in a counting
/// `GlobalAlloc` with a hard live-bytes ceiling. Exceeding it is the DEFINED
/// abort — "Error: out of memory" on stderr, exit 1 — the exact shape the wasm
/// leg's `$oom` prints when its bump frontier passes the same knob, so a leak
/// harness can drive both targets to one deterministic boundary. The block is
/// inserted AFTER the leading inner attributes (`#![...]` must stay first in a
/// crate root); item order is otherwise free in Rust.
fn inject_heap_cap(rs_code: &str, cap: u32) -> String {
    let runtime = format!(
        r#"// --heap-cap runtime (#1530): hard ceiling on live heap bytes.
const __ALMIDE_HEAP_CAP: usize = {cap};
static __ALMIDE_HEAP_LIVE: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);
static __ALMIDE_HEAP_TRIPPED: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);
struct __AlmideCapAlloc;
impl __AlmideCapAlloc {{
    fn charge(&self, n: usize) {{
        use std::sync::atomic::Ordering::Relaxed;
        let live = __ALMIDE_HEAP_LIVE.fetch_add(n, Relaxed) + n;
        // TRIPPED gates the abort to fire once; allocations made by the exit
        // path itself pass through instead of re-entering the abort.
        if live > __ALMIDE_HEAP_CAP && !__ALMIDE_HEAP_TRIPPED.swap(true, Relaxed) {{
            use std::io::Write;
            let _ = std::io::stderr().write_all(b"Error: out of memory\n");
            std::process::exit(1);
        }}
    }}
}}
unsafe impl std::alloc::GlobalAlloc for __AlmideCapAlloc {{
    unsafe fn alloc(&self, l: std::alloc::Layout) -> *mut u8 {{
        self.charge(l.size());
        std::alloc::System.alloc(l)
    }}
    unsafe fn alloc_zeroed(&self, l: std::alloc::Layout) -> *mut u8 {{
        self.charge(l.size());
        std::alloc::System.alloc_zeroed(l)
    }}
    unsafe fn dealloc(&self, p: *mut u8, l: std::alloc::Layout) {{
        __ALMIDE_HEAP_LIVE.fetch_sub(l.size(), std::sync::atomic::Ordering::Relaxed);
        std::alloc::System.dealloc(p, l)
    }}
    unsafe fn realloc(&self, p: *mut u8, l: std::alloc::Layout, new: usize) -> *mut u8 {{
        if new > l.size() {{
            self.charge(new - l.size());
        }} else {{
            __ALMIDE_HEAP_LIVE.fetch_sub(l.size() - new, std::sync::atomic::Ordering::Relaxed);
        }}
        std::alloc::System.realloc(p, l, new)
    }}
}}
#[global_allocator]
static __ALMIDE_CAP_ALLOC: __AlmideCapAlloc = __AlmideCapAlloc;
"#
    );
    // Inner attributes must precede all items: split after the leading run of
    // `#![...]` / blank lines, then place the runtime between the two halves.
    let mut split = 0;
    for line in rs_code.split_inclusive('\n') {
        if line.trim().is_empty() || line.trim_start().starts_with("#![") {
            split += line.len();
        } else {
            break;
        }
    }
    format!("{}{}{}", &rs_code[..split], runtime, &rs_code[split..])
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
    let (bytes, structural) = match compile_to_wasm_bytes(file, allow_unverified, verified, true) {
        Ok(b) => b,
        Err(()) => std::process::exit(1),
    };
    // The structural leg's module imports `almide.*` (the embedded host's
    // surface). A BUILD artifact must run on stock runtimes, so it ships in
    // the WASI form — same index space, shimmed imports, proc_exit on trap
    // (the #1588 transform; the 578-fixture stock-wasmtime gate is its
    // reproduction witness).
    let bytes = if structural {
        match almide_wasm_run::wasi::to_wasi(&bytes) {
            Ok(w) => w,
            Err(e) => {
                err(&format!("error: WASI transform failed — this is an Almide bug: {e}"));
                std::process::exit(1);
            }
        }
    } else {
        bytes
    };

    let pre_size = bytes.len();
    if let Err(e) = std::fs::write(output, &bytes) {
        err(&format!("Failed to write {}: {}", output, e));
        std::process::exit(1);
    }

    // The trust-spine ships the bytes ITS OWN rendering process produced —
    // reachability DCE and the name-section trim already ran inside that
    // pipeline (docs/wasm/WASM-OUTPUT.md). `wasm-opt` is a different kind of
    // thing: an EXTERNAL, unverified transform applied to the renderer's
    // finished output, so running it replaces bytes the trust-spine produced
    // with bytes a separate, un-certified tool rewrote. That is why it stays
    // an explicit, default-off opt-in (`--wasm-opt`) rather than automatic —
    // see the wasm-opt parity leg (`tests/wasm_runtime_test.rs::wasm_opt_parity_spec`) for the
    // differential-testing evidence backing this tier's own guarantee.
    // Name the LEG in the one line every build prints: "which renderer
    // produced these bytes" was invisible by default (the line said
    // v1-verified even for structural output), and that opacity cost real
    // diagnosis time — a "wasm doesn't work" report cannot be split
    // between legs without it.
    let leg = if structural { "structural leg" } else { "incumbent v1 leg" };
    if !wasm_opt {
        err(&format!(
            "Built {} ({} bytes, {}, verified — wasm-opt skipped; pass --wasm-opt for a smaller, non-verified build)",
            output, pre_size, leg
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
    sources: &std::collections::HashMap<String, (String, String)>,
    module_diags: &mut Vec<(String, String, Vec<crate::diagnostic::Diagnostic>)>,
) {
    if almide::stdlib::is_stdlib_module(name) && !almide::stdlib::is_bundled_module(name) { return; }
    let saved_self = checker.env.self_module_name;
    if let Some(pid) = pkg_id.as_ref() {
        checker.env.self_module_name = Some(almide::intern::sym(&pid.name));
    }
    crate::compile_driver::infer_module_capturing(checker, name, mod_prog, sources, module_diags);
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
#[allow(clippy::type_complexity)]
fn parse_and_resolve_wasm(file: &str) -> Result<(almide::ast::Program, String, resolve::ResolvedModules, Vec<(project::PkgId, std::path::PathBuf)>), ()> {
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

    Ok((program, source_text, resolved, dep_paths))
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
fn lower_and_link_wasm_ir(program: &almide::ast::Program, checker: &mut check::Checker, resolved: &mut resolve::ResolvedModules) -> Result<almide::ir::IrProgram, ()> {
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
    let mut module_diags = Vec::new();
    let sources = std::mem::take(&mut resolved.sources);
    for (name, mod_prog, pkg_id, _) in &mut resolved.modules {
        lower_one_wasm_module(
            checker, name, mod_prog, pkg_id, &mut ir_program, &sources, &mut module_diags,
        );
    }
    resolved.sources = sources;
    // An imported module's own type errors abort the wasm build too (#862).
    crate::compile_driver::report_module_diagnostics(&module_diags).map_err(|_| ())?;

    // The ONE driver (crates/almide-driver). This site used to spell the order itself, and
    // spelled it DIFFERENTLY from the shipped wasm path: ir_link FIRST here, ir_link LAST in
    // `almide_mir::pipeline`. Both were green, so the cross-target equivalence claim was
    // resting on "the position of ir_link never matters" rather than on a shared driver
    // (#925, and #785 is a recorded bug from exactly that divergence).
    almide_driver::link_ir(&mut ir_program);

    Ok(ir_program)
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

/// The commissioned wasm leg (Stage 2 switchover): route between the
/// structural emitter (`almide::wasm_leg` + `almide_wasm::emit_program`,
/// the greenfield engine — measured 610/610 byte-identical to native on
/// the full wasm_cross corpus) and the incumbent WAT trust-spine.
///
/// Routing has TWO tiers. Tier 1 is by PROJECT SHAPE (the enumerated
/// conditions below). Tier 2: a program the shape routing gives to the
/// structural leg whose lowering then WALLS is re-rendered by the incumbent
/// — a verified-to-verified handover, NOT the retired v0 fallback (#782's
/// sin was falling into UNVERIFIED codegen). A program NEITHER leg lowers
/// still fails hard with the incumbent's wall diagnostics. The handover is
/// named on stderr under `ALMIDE_VERIFIED_DEBUG=1`, and the leg that
/// produced the bytes is named in the `Built …` line / `--time-report`:
///   - `ALMIDE_WASM_INCUMBENT=1`     → incumbent (the reversible switch,
///                                      kept for one release)
///   - main-less library module      → incumbent (#881 export mode — the
///                                      structural emitter has no library
///                                      form yet)
///   - host-variant program on the BUILD path → incumbent (its artifact
///                                      speaks real WASI fs/env; the WASI
///                                      transform's shims cover less)
///   - everything else               → structural emitter
///
/// The second tuple field is true when the STRUCTURAL leg produced the
/// bytes (they import `almide.*` and run on the embedded host; the build
/// path converts them with `to_wasi` for stock runtimes).
fn render_wasm_module_routed(
    file: &str,
    source_text: &str,
    v1_self_modules: &[(String, almide_lang::ast::Program, bool)],
    library_ok: bool,
    has_main: bool,
    dep_paths: &[(project::PkgId, std::path::PathBuf)],
    uses_incumbent_features: bool,
    host_variant: bool,
) -> Result<(Vec<u8>, bool), ()> {
    //   - `ALMIDE_FUEL_PROBE` set         → incumbent (the charge-trace
    //     probe line is that leg's Σ-probe instrumentation — contract
    //     evidence keeps its measured meaning; the structural leg's C-320
    //     conformance has its own gates in crates/almide-wasm)
    // `ALMIDE_WASM_STRUCTURAL=1` — the frontier-development probe switch:
    // force the STRUCTURAL leg on shapes the routing would send to the
    // incumbent, and turn every wall/decline into a hard error instead of
    // the reroute (a probe that silently rerouted would measure nothing).
    // This is how a routed-away shape (#1596 self-import, #1598 matrix/io)
    // is exercised while its structural support is built, and the lever the
    // eventual route flip is verified with.
    let force_structural = std::env::var_os("ALMIDE_WASM_STRUCTURAL").is_some();
    let incumbent = !force_structural
        && (std::env::var_os("ALMIDE_WASM_INCUMBENT").is_some()
            || std::env::var_os("ALMIDE_FUEL_PROBE").is_some()
            || !has_main
            || uses_incumbent_features
            || (library_ok && host_variant));
    if incumbent {
        return render_wasm_module(source_text, v1_self_modules, library_ok).map(|(b, _)| (b, false));
    }
    // A structural WALL routes to the incumbent renderer. This is NOT the
    // retired v0 fallback (#782's sin was falling into UNVERIFIED codegen):
    // both legs here are verified renderers, so the product never regresses
    // on a shape only the incumbent lowers — and a program NEITHER leg can
    // lower still fails hard with the incumbent's rich wall diagnostics.
    // ALMIDE_VERIFIED_DEBUG=1 names the wall that rerouted.
    let reroute = |why: &str| {
        if force_structural {
            err(&format!("error: structural leg walled under ALMIDE_WASM_STRUCTURAL ({why})"));
            return Err(());
        }
        if std::env::var_os("ALMIDE_VERIFIED_DEBUG").is_some() {
            err(&format!("[almide] structural leg declined ({why}) — incumbent renderer"));
        }
        render_wasm_module(source_text, v1_self_modules, library_ok).map(|(b, _)| (b, false))
    };
    let ir = match almide::wasm_leg::lower_to_ir_with_deps(file, source_text, dep_paths) {
        Ok(ir) => ir,
        Err(e) => return reroute(&format!("front: {e}")),
    };
    match almide_wasm::emit_program(&ir) {
        Ok(bytes) => {
            // Same emit-time validation discipline as the incumbent leg:
            // never ship bytes wasmtime would refuse at load.
            if let Err(e) = wasmparser::validate(&bytes) {
                return reroute(&format!("validation: {}", e.message()));
            }
            // With the debug env, ALWAYS name the winning leg — the
            // incumbent path and the reroute already speak, so a silent
            // structural success made the env an incomplete oracle.
            if std::env::var_os("ALMIDE_VERIFIED_DEBUG").is_some() {
                err(&format!(
                    "[almide] structural leg emitted the module ({} bytes)",
                    bytes.len()
                ));
            }
            Ok((bytes, true))
        }
        Err(almide_wasm::EmitError::Unsupported(reason)) => reroute(&reason),
    }
}

/// The incumbent v1 PCC-verified trust-spine render (#782: the v0 wasm
/// emitter is retired). A v1 wall is an honest, diagnosed hard error, never
/// a silent fallback into unverified codegen: a program that compiles is
/// verified, a program the renderer cannot verify is refused with the wall
/// reason (refusal over risk — the medical-grade bar). Extracted verbatim.
fn render_wasm_module(source_text: &str, v1_self_modules: &[(String, almide_lang::ast::Program, bool)], library_ok: bool) -> Result<(Vec<u8>, bool), ()> {
    // `almide build` may produce a main-less LIBRARY module (pub-fn exports,
    // synthesized empty `_start` — #881); `almide run` must keep the no-main
    // wall so the wasm leg fails exactly where native compilation does.
    let render = if library_ok {
        almide_mir::pipeline::try_render_wasm_source_library
    } else {
        almide_mir::pipeline::try_render_wasm_source
    };
    match render(
        source_text,
        v1_self_modules,
        std::env::var("ALMIDE_VERIFIED_DEBUG").is_ok(),
    ) {
        Ok(wat) => match wat::parse_str(&wat) {
            Ok(bytes) => {
                // Unconditional emit-time validation (the grain pattern:
                // Binaryen `Module.validate` or die). `wat` ASSEMBLES without
                // full stack-shape validation, so a renderer bug that types
                // out (e.g. almide#1431's i32/i64 mismatch) would otherwise
                // ship invalid bytes and surface as a wasmtime translation
                // error at load — a runtime failure wearing the runner's
                // vocabulary. Validate here, before the name section is
                // stripped, so the wall can name the offending function.
                if let Err(e) = wasmparser::validate(&bytes) {
                    let site = wasm_function_at(&bytes, e.offset())
                        .unwrap_or_else(|| "an unidentified function".to_string());
                    err(&format!(
                        "error: emitted wasm failed validation — this is an Almide bug: {} \
                         (offset {:#x}, in {site})",
                        e.message(),
                        e.offset()
                    ));
                    err(
                        "  hint: please file this with the source that triggered it: \
                         https://github.com/almide/almide/issues",
                    );
                    return Err(());
                }
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
            // The reason renders through `LowerError`'s Display — one readable
            // sentence, however deep the wall nested — never the `{:?}` form,
            // whose per-level `Unsupported("…")` wrappers and escaped quotes
            // were the worst diagnostic in the compiler (#931). A wall whose
            // construction site had a span renders through the Diagnostic
            // machinery — source line, caret, the works — so the user sees
            // WHERE the shape lives, not just what it is. A KNOWN WallShape
            // additionally headlines the construct in surface-language
            // vocabulary and hints its documented rewrite; the raw reason —
            // compiler-internal vocabulary and all — moves to a trailing
            // `note:` where it still serves a bug report.
            if let Some(span) = e.span() {
                let shape = e.shape();
                let (message, hint, reason_note) =
                    match (shape.headline(), shape.rewrite_hint()) {
                        (Some(headline), Some(rewrite)) => {
                            (headline.to_string(), rewrite.to_string(), Some(e.reason()))
                        }
                        _ => (
                            e.reason().to_string(),
                            "the unverified v0 wasm emitter was retired (#782): a wall is an \
                             honest error instead of a silent fallback. If this names a missing \
                             capability, please file it with the source shape that triggered it: \
                             https://github.com/almide/almide/issues"
                                .to_string(),
                            None,
                        ),
                    };
                let mut d = crate::diagnostic::Diagnostic::error(
                    message,
                    hint,
                    "the verified wasm render (v1 trust spine) — this shape is not yet in its subset",
                );
                d.line = Some(span.line);
                d.col = Some(span.col);
                if span.end_col > span.col {
                    d.end_col = Some(span.end_col);
                }
                let mut rendered =
                    crate::diagnostic_render::display_with_source(&d, source_text);
                if let Some(reason) = reason_note {
                    rendered.push_str(&format!(
                        "\n  note: {reason}\n  note: if the rewrite does not apply, file the \
                         source shape that triggered this: \
                         https://github.com/almide/almide/issues"
                    ));
                }
                err(&rendered);
            } else {
                err(&format!(
                    "error: this program shape is not yet supported by the verified wasm renderer\n\n  \
                     {e}\n\n  \
                     The unverified v0 wasm emitter was retired (#782): a wall is an honest error\n  \
                     instead of a silent fallback. If this names a missing capability, please file\n  \
                     it with the source shape that triggered it:\n  \
                     https://github.com/almide/almide/issues"
                ));
            }
            // The one machine-readable line in a wall's stderr, on BOTH render
            // paths: `wall: <reason>`, whitespace-flattened to stay a single
            // line. The nightly fuzzer's honest-wall classifier keys on
            // crate::WASM_WALL_MARKER (shared through its `almide` path-dep) —
            // the human diagnostic above may be reworked freely, this line may
            // not (tests/wall_shape_rendering_test.rs pins it).
            let reason_one_line =
                e.to_string().split_whitespace().collect::<Vec<_>>().join(" ");
            err(&format!("{}{reason_one_line}", crate::WASM_WALL_MARKER));
            Err(())
        }
    }
}

pub(crate) fn compile_to_wasm_bytes(file: &str, allow_unverified: bool, verified: bool, library_ok: bool) -> Result<(Vec<u8>, bool), ()> {
    let (mut program, source_text, mut resolved, dep_paths) = parse_and_resolve_wasm(file)?;

    // v1 `--verified`: capture the FRESH (un-inferred) cross-module siblings now, before the loop
    // below mutates them in place — the v1 pipeline re-runs its own canonicalize/infer/lower from
    // raw programs (exactly the render_program example's `discover_self_modules` input).
    // #782: always collected — the v1 renderer is the only wasm path.
    let v1_self_modules: Vec<(String, almide_lang::ast::Program, bool)> =
        resolved.modules.iter().map(|(n, p, _pkg, s)| (n.clone(), p.clone(), *s)).collect();

    let mut checker = typecheck_wasm_program(file, &source_text, &mut program, &resolved)?;
    let mut ir_program = lower_and_link_wasm_ir(&program, &mut checker, &mut resolved)?;
    verify_wasm_ir(&ir_program)?;
    check_no_native_only_matrix(&ir_program)?;

    // Routing inputs (see render_wasm_module_routed): project shape, decided
    // from what the v0 gates already computed — never from a failure.
    let has_main = ir_program.functions.iter().any(|f| f.name.as_str() == "main");
    // `@export`-attributed fns must survive as wasm exports (the DCE-root
    // contract, wasm_export_dce_root_test) — the structural leg has no
    // export mode yet (#1598's sibling surface), so those modules stay on
    // the incumbent leg.
    let has_exports = ir_program.functions.iter().any(|f| !f.export_attrs.is_empty());
    let host_variant = std::iter::once(&program)
        .chain(resolved.modules.iter().map(|(_, p, _, _)| p))
        .flat_map(|p| p.imports.iter())
        .any(|d| {
            matches!(d, almide::ast::Decl::Import { path, .. }
                if path.first().is_some_and(|r| matches!(r.as_str(), "fs" | "env" | "process")))
        });
    // #1598 CLOSED as per-fn auto-flip: the matrix/io module pre-scan is
    // GONE. The linked surfaces (io.read_all via the host's op-31 drain
    // joined io.print/write/write_bytes/read_n_bytes; the measured matrix
    // arms) lower structurally; anything still unlinked (the qwen/llama
    // matrix long tail, io.read_line/read_byte) WALLS at lowering and the
    // tier-2 verified-to-verified reroute hands it to the incumbent — so
    // every future linked fn flips its own route with no hand-mirrored
    // list to drift.
    // #1596 CLOSED: `import self as m` projects run the structural leg.
    // The spaced globals machinery ((space, VarId) keys — separately-
    // lowered modules each restart VarIds at 0) fixed the top-let storage
    // misalignment; the full crossmod matrix passes on the forced
    // structural leg, and a shape it still cannot lower (a module
    // initializer with inner binds) walls honestly and reroutes.
    let _ = (&mut ir_program, allow_unverified, verified);
    render_wasm_module_routed(
        file,
        &source_text,
        &v1_self_modules,
        library_ok,
        has_main,
        &dep_paths,
        has_exports,
        host_variant,
    )
}

/// Best-effort map of a validation-error byte offset to the function that
/// contains it: which code-section body covers the offset, named through
/// the name section (still present — validation runs before
/// [`strip_wasm_name_section`]). `None` only if the module is too broken
/// to walk section-by-section; the caller still reports the offset.
fn wasm_function_at(bytes: &[u8], offset: usize) -> Option<String> {
    use wasmparser::{KnownCustom, Name, Parser, Payload, TypeRef};
    let mut imported_funcs: u32 = 0;
    let mut code_index: u32 = 0;
    let mut hit: Option<u32> = None;
    let mut names: Vec<(u32, String)> = Vec::new();
    for payload in Parser::new(0).parse_all(bytes) {
        match payload.ok()? {
            Payload::ImportSection(imports) => {
                // 0.255 groups imports by module; each group yields
                // `(byte_offset, Import)` items.
                for group in imports.into_iter().flatten() {
                    for (_, imp) in group.into_iter().flatten() {
                        if matches!(imp.ty, TypeRef::Func(_)) {
                            imported_funcs += 1;
                        }
                    }
                }
            }
            Payload::CodeSectionEntry(body) => {
                if body.range().contains(&offset) {
                    hit = Some(imported_funcs + code_index);
                }
                code_index += 1;
            }
            Payload::CustomSection(c) => {
                if let KnownCustom::Name(name_reader) = c.as_known() {
                    for sub in name_reader.into_iter().flatten() {
                        if let Name::Function(map) = sub {
                            for naming in map.into_iter().flatten() {
                                names.push((naming.index, naming.name.to_string()));
                            }
                        }
                    }
                }
            }
            _ => {}
        }
    }
    let idx = hit?;
    Some(match names.iter().find(|(i, _)| *i == idx) {
        Some((_, name)) => format!("function `{name}` (index {idx})"),
        None => format!("function index {idx}"),
    })
}

/// Trim every "name"-id custom section down to its function-names
/// subsection, dropping local-names and any other subsection.
///
/// `wat::parse_str` always emits a name section recording every symbolic
/// `$name` the WAT source used — functions AND every per-function local
/// (`$v1`, `$v2`, ...). `docs/wasm/WASM-OUTPUT.md` commits to keeping function
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

/// Run `wasm-opt -Oz` on the output file, in-place.
/// Returns the new file size on success.
fn run_wasm_opt(path: &str) -> Result<usize, String> {
    // `-Oz`, matching the flag's documented contract: `--wasm-opt` exists to
    // shrink the module (the published size tables are -Oz numbers), and the
    // implementation silently ran `-O3 --enable-simd` instead — a speed
    // profile with a feature the v1 renderer does not emit (no v128 in the
    // default output, #864/#916), so the documented numbers were not
    // reproducible through the flag.
    // --enable-nontrapping-float-to-int: float→int renders as
    // `i64.trunc_sat_f64_s` (the saturating truncate, lib_b.rs).
    // --enable-tail-call: mutual/self tail recursion renders `return_call`.
    let status = std::process::Command::new("wasm-opt")
        .args([
            "-Oz",
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

