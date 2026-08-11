use crate::{parse_file, fmt, project, project_fetch, resolve, canonicalize, check, diagnostic, out, out_no_nl, err, err_no_nl};
use super::{collect_test_files, incremental_cache_dir};

pub fn cmd_init() {
    if std::path::Path::new("almide.toml").exists() {
        err(&format!("almide.toml already exists"));
        std::process::exit(1);
    }
    let dir_name = std::env::current_dir()
        .ok()
        .and_then(|p| p.file_name().map(|n| n.to_string_lossy().to_string()))
        .unwrap_or_else(|| "myapp".to_string());

    let toml = format!("[package]\nname = \"{}\"\nversion = \"0.1.0\"\nedition = \"2026\"\n", dir_name);

    if let Err(e) = std::fs::write("almide.toml", toml) {
        err(&format!("Failed to write almide.toml: {}", e));
        std::process::exit(1);
    }
    if let Err(e) = std::fs::create_dir_all("src") {
        err(&format!("Failed to create src/: {}", e));
        std::process::exit(1);
    }
    if let Err(e) = std::fs::create_dir_all("tests") {
        err(&format!("Failed to create tests/: {}", e));
        std::process::exit(1);
    }

    if !std::path::Path::new("src/main.almd").exists() {
        if let Err(e) = std::fs::write("src/main.almd", "effect fn main() -> Unit = {\n  println(\"Hello, Almide!\")\n}\n") {
            err(&format!("Failed to write src/main.almd: {}", e));
            std::process::exit(1);
        }
    }

    // Generate CLAUDE.md for AI-assisted development
    if !std::path::Path::new("CLAUDE.md").exists() {
        let claude_md = include_str!("../../docs/project/CLAUDE_TEMPLATE.md");
        if let Err(e) = std::fs::write("CLAUDE.md", claude_md) {
            err(&format!("Failed to write CLAUDE.md: {}", e));
            std::process::exit(1);
        }
    }

    err(&format!("Initialized project in ./"));
    err(&format!("  almide.toml"));
    err(&format!("  src/main.almd"));
    err(&format!("  tests/"));
    err(&format!("  CLAUDE.md"));
}

/// Shared "resolve `almide test [file]`'s target file list" logic — used by
/// `cmd_test`/`cmd_test_fast` (search `spec/` and `exercises/`, `.`
/// fallback) and `cmd_test_wasm` (search `.` directly, i.e. an empty
/// `fallback_dirs`). Extracted verbatim from `cmd_test`'s identical block —
/// exits the process on an empty result, exactly as all three call sites
/// already did.
fn discover_test_files(file: &str, fallback_dirs: &[&str]) -> Vec<String> {
    if !file.is_empty() {
        let path = std::path::Path::new(file);
        if path.is_dir() {
            let mut files = collect_test_files(path);
            files.sort();
            if files.is_empty() {
                err(&format!("No .almd files with test blocks found in {}", file));
                std::process::exit(1);
            }
            files
        } else {
            vec![file.to_string()]
        }
    } else {
        // Default: recursively find test files in the given standard
        // directories (e.g. spec/, exercises/); "." otherwise.
        let mut files = Vec::new();
        for dir in fallback_dirs {
            let path = std::path::Path::new(dir);
            if path.exists() {
                files.extend(collect_test_files(path));
            }
        }
        // Fallback: search current directory if no standard dirs found
        if files.is_empty() {
            files = collect_test_files(std::path::Path::new("."));
        }
        files.sort();
        if files.is_empty() {
            err(&format!("No .almd files with test blocks found."));
            std::process::exit(1);
        }
        files
    }
}

/// `cmd_test`'s Phase 1: compile every test file in parallel (bounded by
/// CPU count), each in its own scratch dir so cold rustc builds parallelize
/// instead of serializing on the shared dir's BUILD_LOCK. Extracted
/// verbatim.
fn compile_test_files_parallel(test_files: &[String], no_check: bool) -> Vec<(String, Result<std::path::PathBuf, String>)> {
    let cpus = std::thread::available_parallelism().map(|n| n.get()).unwrap_or(4);
    let (tx, rx) = std::sync::mpsc::channel();
    let (sem_tx, sem_rx) = std::sync::mpsc::sync_channel::<()>(cpus);
    for _ in 0..cpus { let _ = sem_tx.send(()); }
    let sem_tx = std::sync::Arc::new(sem_tx);
    let sem_rx = std::sync::Arc::new(std::sync::Mutex::new(sem_rx));
    let mut handles = Vec::new();
    for test_file in test_files.to_vec() {
        let tx = tx.clone();
        let sem_rx = sem_rx.clone();
        let sem_tx = sem_tx.clone();
        handles.push(std::thread::spawn(move || {
            let _ = sem_rx.lock().unwrap().recv();
            // Per-file scratch dir so cold rustc builds parallelize instead
            // of serializing on the shared dir's BUILD_LOCK.
            let worker_dir = std::env::temp_dir()
                .join("almide-test")
                .join(test_file.replace('/', "_").replace('.', "_"));
            let result = super::run::compile_to_binary(&test_file, no_check, true, false, Some(&worker_dir));
            let _ = sem_tx.send(());
            let _ = tx.send((test_file, result));
        }));
    }
    drop(tx);
    let mut results: Vec<_> = rx.iter().collect();
    for h in handles { let _ = h.join(); }
    results.sort_by(|a, b| a.0.cmp(&b.0));
    results
}

/// `cmd_test`'s Phase 2: execute every compiled test binary in parallel
/// (bounded by CPU count). Extracted verbatim.
fn run_test_binaries_parallel(compiled: Vec<(String, Result<std::path::PathBuf, String>)>, program_args: &std::sync::Arc<Vec<String>>) -> Vec<(String, i32)> {
    let cpus = std::thread::available_parallelism().map(|n| n.get()).unwrap_or(4);
    let (sem_tx, sem_rx) = std::sync::mpsc::sync_channel::<()>(cpus);
    for _ in 0..cpus { let _ = sem_tx.send(()); }
    let sem_tx = std::sync::Arc::new(sem_tx);
    let sem_rx = std::sync::Arc::new(std::sync::Mutex::new(sem_rx));
    let (tx, rx) = std::sync::mpsc::channel();
    let mut handles = Vec::new();
    for (file, compile_result) in compiled {
        let tx = tx.clone();
        let args = program_args.clone();
        let sem_rx = sem_rx.clone();
        let sem_tx = sem_tx.clone();
        handles.push(std::thread::spawn(move || {
            let _ = sem_rx.lock().unwrap().recv();
            let code = match compile_result {
                Ok(bin) => super::run::run_binary(&bin, &args),
                Err(e) => {
                    err(&format!("Compile error for {}:\n{}", file, e));
                    1
                }
            };
            let _ = sem_tx.send(());
            let _ = tx.send((file, code));
        }));
    }
    drop(tx);
    let mut results: Vec<(String, i32)> = rx.iter().collect();
    for h in handles { let _ = h.join(); }
    results.sort_by(|a, b| a.0.cmp(&b.0));
    results
}

pub fn cmd_test(file: &str, no_check: bool, run_filter: Option<&str>) {
    let test_files: Vec<String> = discover_test_files(file, &["spec", "exercises"]);

    let mut program_args: Vec<String> = Vec::new();
    if let Some(filter) = run_filter {
        program_args.push(filter.to_string());
    }
    let program_args = std::sync::Arc::new(program_args);

    // Phase 1: Compile all test files in parallel (bounded by CPU count)
    let compiled = compile_test_files_parallel(&test_files, no_check);

    // Phase 2: Execute test binaries in parallel (bounded by CPU count)
    let results = run_test_binaries_parallel(compiled, &program_args);

    let mut failed = 0;
    for (file, code) in &results {
        if *code != 0 {
            err(&format!("FAILED: {}", file));
            failed += 1;
        }
    }
    if failed > 0 {
        err(&format!("\n{}/{} test file(s) failed", failed, test_files.len()));
        std::process::exit(1);
    }
    err(&format!("\nAll {} test file(s) passed", test_files.len()));
}

enum WasmTestOutcome {
    Pass { file: String, count: usize, bytes: usize },
    Fail { file: String, detail: String },
    /// The file does not compile on ANY target: resolve/type errors in the
    /// entry file or an imported module. Distinct from `Skip` — a SKIP means
    /// "correct program outside the verified renderer's subset", and the skip
    /// ledger must never absorb diagnostics (#957). The default harness
    /// routes these to the native fallback (which reports them
    /// authoritatively); the standalone `--target wasm` harness counts them
    /// FAILED, matching the default harness's verdict on the same file.
    CompileError { file: String, detail: String },
    Skip { file: String, reason: String },
}

/// Compile one `.almd` file to WASM and run it under wasmtime. Pure per-file
/// work (no shared mutable state) so it runs in parallel — the WASM path takes
/// no rustc/cargo, so there's no global build lock to serialize on.
/// `compile_and_run_wasm_test`'s independent pre-flight gates: `// wasm:skip`
/// marker, parse errors (a real failure, not a skip — see the comment at the
/// call site), and the main+test co-presence gap in the v1 test-mode runner.
/// Extracted verbatim — each check only reads its parameters; whichever
/// fires first determines the outcome, matching the original code's
/// early-return order exactly.
fn wasm_test_preflight_outcome(
    test_file: &str,
    program: &almide_lang::ast::Program,
    source_text: &str,
    parse_errors: &[crate::diagnostic::Diagnostic],
) -> Option<WasmTestOutcome> {
    if source_text.lines().take(3).any(|line| line.contains("// wasm:skip")) {
        return Some(WasmTestOutcome::Skip { file: test_file.to_string(), reason: "wasm:skip".to_string() });
    }
    if parse_errors.iter().any(|d| d.level == crate::diagnostic::Level::Error) {
        let mut detail = String::new();
        for d in parse_errors.iter().filter(|d| d.level == crate::diagnostic::Level::Error).take(3) {
            detail.push_str(&format!("  parse error: {}\n", d.message));
        }
        return Some(WasmTestOutcome::Fail { file: test_file.to_string(), detail });
    }
    None
}

/// Push an `ALMIDE_PROFILE` timing mark when profiling is enabled — guards
/// `compile_and_run_wasm_test`'s repeated `if prof { marks.push(...) }`
/// call sites behind one named function instead of six inline branches.
fn mark(prof: bool, marks: &mut Vec<(&'static str, std::time::Instant)>, label: &'static str) {
    if prof {
        marks.push((label, std::time::Instant::now()));
    }
}

/// Print the `ALMIDE_PROFILE` per-phase timing breakdown for one test file.
/// Extracted verbatim from `compile_and_run_wasm_test`'s trailing profiling
/// block.
fn print_wasm_test_profile(test_file: &str, marks: &[(&'static str, std::time::Instant)]) {
    let total = marks.last().unwrap().1.duration_since(marks[0].1).as_secs_f64();
    let mut line = format!("[prof] {} total={:.3}s", test_file, total);
    for w in marks.windows(2) {
        line.push_str(&format!(" | {}={:.3}", w[1].0, w[1].1.duration_since(w[0].1).as_secs_f64()));
    }
    err(&format!("{}", line));
}

/// `compile_and_run_wasm_test`'s dependency-fetch + import-resolution
/// phase. Extracted verbatim.
fn resolve_wasm_test_deps(test_file: &str, program: &almide_lang::ast::Program) -> Result<resolve::ResolvedModules, String> {
    let dep_paths: Vec<(project::PkgId, std::path::PathBuf)> =
        if std::path::Path::new("almide.toml").exists() {
            if let Ok(proj) = project::parse_toml(std::path::Path::new("almide.toml")) {
                project_fetch::fetch_all_deps(&proj)
                    .unwrap_or_else(|_| vec![])
                    .into_iter()
                    .map(|fd| (fd.pkg_id, fd.source_dir))
                    .collect()
            } else { vec![] }
        } else { vec![] };

    resolve::resolve_imports_with_deps(test_file, program, &dep_paths)
}

/// `compile_and_run_wasm_test`'s type-check phase. Unlike `almide
/// build`/`run --target wasm` (which print full diagnostics on a type
/// error), test mode returns the errors compactly: the default harness
/// routes the file to the native fallback, which re-runs and reports them
/// authoritatively (printing here would duplicate every diagnostic); the
/// standalone `--target wasm` harness prints this summary itself (#957).
fn typecheck_wasm_test_program(test_file: &str, source_text: &str, program: &mut almide_lang::ast::Program, resolved: &resolve::ResolvedModules) -> Result<check::Checker, String> {
    let canon = canonicalize::canonicalize_program(
        program,
        resolved.modules.iter().map(|(n, p, _, s)| (n.as_str(), p, *s)),
    );
    let mut checker = check::Checker::from_env(canon.env);
    checker.set_source(test_file, source_text);
    checker.diagnostics = canon.diagnostics;
    // #785: module top-let types must be fully inferred before the entry
    // program reads them (drivers infer the entry FIRST; without this the
    // readers see the registration seed — Unknown for non-literal inits).
    almide::resolve::refresh_module_toplets(&mut checker, &resolved.modules);
    let diagnostics = checker.infer_program(program);
    if diagnostics.iter().any(|d| d.level == diagnostic::Level::Error) {
        return Err(render_error_summary(diagnostics.iter().filter(|d| d.level == diagnostic::Level::Error)));
    }
    Ok(checker)
}

/// Compact per-file error summary for the standalone wasm harness's FAIL
/// output: the first three error diagnostics, one line each.
fn render_error_summary<'a>(errors: impl Iterator<Item = &'a diagnostic::Diagnostic>) -> String {
    let mut detail = String::new();
    for d in errors.take(3) {
        let loc = match (d.line, d.col) {
            (Some(l), Some(c)) => format!(":{}:{}", l, c),
            _ => String::new(),
        };
        detail.push_str(&format!("  type error{}: {}\n", loc, d.message));
    }
    detail
}

/// `compile_and_run_wasm_test`'s pre-register + lower phase: pre-register
/// versioned module names, lower the entry program, then lower each
/// resolved user module via the shared `build::lower_one_wasm_module` (the
/// same per-module lowering `compile_to_wasm_bytes` uses — this loop body
/// used to be a byte-for-byte duplicate of it). link/optimize/monomorphize
/// stay in the caller so the ALMIDE_PROFILE "lower_modules" mark lands at
/// the same point as before. Extracted verbatim.
fn lower_wasm_test_modules(program: &almide_lang::ast::Program, checker: &mut check::Checker, resolved: &mut resolve::ResolvedModules) -> Result<almide::ir::IrProgram, String> {
    for (name, _, pkg_id, _) in &resolved.modules {
        if let Some(pid) = pkg_id.as_ref() {
            let base = pid.mod_name();
            let v = if let Some(suffix) = name.strip_prefix(&pid.name) { format!("{}{}", base, suffix) } else { base };
            checker.env.module_versioned_names.insert(almide::intern::sym(name), almide::intern::sym(&v));
        }
    }
    let mut ir_program = almide::lower::lower_program(program, &checker.env, &checker.type_map);
    let mut module_diags = Vec::new();
    let sources = std::mem::take(&mut resolved.sources);
    for (name, mod_prog, pkg_id, _) in &mut resolved.modules {
        super::build::lower_one_wasm_module(
            checker, name, mod_prog, pkg_id, &mut ir_program, &sources, &mut module_diags,
        );
    }
    resolved.sources = sources;
    // An imported module's own type errors are NOT printed here: like the
    // entry program's, they route the file to the authoritative native
    // fallback, which prints them (#862) — printing twice would duplicate
    // every diagnostic in `almide test`'s output. They ARE returned
    // compactly, so the standalone `--target wasm` harness can report its
    // own FAIL verdict instead of absorbing them as a skip (#957).
    if module_diags.iter().any(|(_, _, ds)| ds.iter().any(|d| d.level == diagnostic::Level::Error)) {
        return Err(render_error_summary(
            module_diags.iter().flat_map(|(_, _, ds)| ds.iter()).filter(|d| d.level == diagnostic::Level::Error),
        ));
    }
    Ok(ir_program)
}

fn compile_and_run_wasm_test(test_file: &str, tmp_dir: &std::path::Path) -> WasmTestOutcome {
    let skip = |reason: String| WasmTestOutcome::Skip { file: test_file.to_string(), reason };
    let compile_error = |detail: String| WasmTestOutcome::CompileError { file: test_file.to_string(), detail };
    let prof = std::env::var_os("ALMIDE_PROFILE").is_some();
    let mut marks: Vec<(&'static str, std::time::Instant)> = vec![("start", std::time::Instant::now())];

    let wasm_name = test_file.replace('/', "_").replace('.', "_") + ".wasm";
    let wasm_path = tmp_dir.join(&wasm_name);

    let (mut program, source_text, parse_errors) = parse_file(test_file);
    mark(prof, &mut marks, "parse");
    // `// wasm:skip` marker / parse errors (a real Fail, not a benign skip —
    // see `wasm_test_preflight_outcome`'s doc comment) / the main+test
    // co-presence gap in the v1 test-mode runner.
    if let Some(outcome) = wasm_test_preflight_outcome(test_file, &program, &source_text, &parse_errors) {
        return outcome;
    }

    let mut resolved = match resolve_wasm_test_deps(test_file, &program) {
        Ok(r) => r,
        // An unresolvable import is broken on every target, not a wall.
        Err(e) => return compile_error(format!("  resolve error: {}\n", e)),
    };
    mark(prof, &mut marks, "resolve");

    // v1 verified leg (the DEFAULT build/run wasm path since 0.29.0): capture the FRESH
    // (un-inferred) cross-module siblings now — the infer loop below mutates them in
    // place, and the v1 pipeline re-runs its own canonicalize/infer/lower from raw
    // programs (exactly `compile_to_wasm_bytes`'s capture). Tried after the v0 gates
    // below; a wall falls through to the v0 emit — same honest-wall contract as build.
    let v1_self_modules: Vec<(String, almide_lang::ast::Program, bool)> =
        resolved.modules.iter().map(|(n, p, _pkg, s)| (n.clone(), p.clone(), *s)).collect();

    let mut checker = match typecheck_wasm_test_program(test_file, &source_text, &mut program, &resolved) {
        Ok(c) => c,
        Err(detail) => return compile_error(detail),
    };
    mark(prof, &mut marks, "check_user");

    let mut ir_program = match lower_wasm_test_modules(&program, &mut checker, &mut resolved) {
        Ok(ir) => ir,
        Err(detail) => return compile_error(detail),
    };
    mark(prof, &mut marks, "lower_modules");
    // The ONE driver — see the note in src/cli/build.rs. This is the site whose order the
    // migration FLIPPED (ir_link first → last), so its acceptance check is byte-identity of
    // spec/wasm_cross against the pre-migration capture, not merely a green suite.
    almide_driver::link_ir(&mut ir_program);
    mark(prof, &mut marks, "opt_mono");
    // Native-only matrix ops (e.g. qwen3_block_q1_0_kv) have no WASM lowering;
    // skip with a clear reason instead of reaching the emitter (whose panic would
    // surface as a generic "WASM codegen panic" skip).
    if let Some(op) = almide::codegen::program_uses_native_only_matrix_on_wasm(&ir_program) {
        return skip(format!("matrix.{op} is native-only — no WASM lowering"));
    }
    // v1 verified leg FIRST (mirrors `compile_to_wasm_bytes`): byte-identical to v0
    // where it lowers, honest wall (fall through to the v0 emit) otherwise. This is
    // what `almide build`/`run --target wasm` already default to; without it the test
    // harness exercised ONLY the v0 emitter — a main+tests file whose shapes v1 ships
    // but v0's closure-env traps on (closure_capturing_fn_wasm) could never pass here.
    // The `_tests` variant keeps the SAME per-file protocol as v0: a file with `main`
    // runs main only (`__main_runner`); a test-only file gets a synthesized runner
    // (`__test_runner`'s `test: <name> ... ` / `ok` lines the pass-counter reads).
    // `wat` ASSEMBLES without full stack-shape validation, so a structurally invalid v1
    // module (a def/callsite ABI residue) would only surface at wasmtime load — a FAIL
    // that routes to the slow native fallback. VALIDATE here instead: invalid → treat
    // as a wall (honest, and the file routes to the authoritative native leg).
    //
    // `ALMIDE_WALL_REASON=1` prints WHICH of the three stages declined. Without it a
    // fallback file reports only "v1 wall", and diagnosing the #813 remainder meant
    // re-deriving each one by hand through `render_program`.
    let explain = std::env::var_os("ALMIDE_WALL_REASON").is_some();

    let v1_bytes: Option<Vec<u8>> =
        match almide_mir::pipeline::try_render_wasm_source_tests(&source_text, &v1_self_modules, explain) {
            Err(e) => {
                if explain { err(&format!("[wall] {}: render: {:?}", test_file, e)); }
                None
            }
            Ok(wat_text) => match wat::parse_str(&wat_text) {
                Err(e) => {
                    if explain { err(&format!("[wall] {}: wat assemble: {}", test_file, e)); }
                    None
                }
                Ok(bytes) => match wasmparser::validate(&bytes) {
                    Err(e) => {
                        if explain { err(&format!("[wall] {}: validate: {}", test_file, e)); }
                        None
                    }
                    Ok(_) => Some(bytes),
                },
            },
        };
    // Write the module and run it under wasmtime. `-S inherit-env=y` mirrors
    // `cmd_run_wasm`: `env.get` in a test observes the same host variables native
    // does (the env cross-target contract).
    let run_module = |bytes: &[u8]| -> WasmTestOutcome {
        if let Err(e) = std::fs::write(&wasm_path, bytes) {
            return skip(format!("write: {}", e));
        }
        let mut cmd = std::process::Command::new("wasmtime");
        super::run::wasmtime_fs_args(&mut cmd);
        cmd.arg("-S").arg("inherit-env=y");
        // Same ALMIDE_CWD pin as `cmd_run_wasm` (#874): relative fs paths in a
        // test resolve against the real launcher cwd, not a stale PWD. On
        // Windows the guest spelling (`.`) comes from `wasmtime_fs_args`.
        if !cfg!(windows) {
            if let Some(cwd) = super::run::almide_cwd() {
                cmd.arg(format!("--env=ALMIDE_CWD={}", cwd));
            }
        }
        let output = cmd
            .arg(wasm_path.to_str().unwrap())
            .output();
        match output {
            Ok(result) => {
                let stdout = String::from_utf8_lossy(&result.stdout);
                let stderr = String::from_utf8_lossy(&result.stderr);
                if result.status.success() {
                    WasmTestOutcome::Pass {
                        file: test_file.to_string(),
                        count: stdout.matches("ok\n").count(),
                        bytes: bytes.len(),
                    }
                } else {
                    let mut last_test = String::new();
                    for line in stdout.lines() {
                        if line.starts_with("test: ") { last_test = line.to_string(); }
                    }
                    let mut detail = String::new();
                    if !last_test.is_empty() { detail.push_str(&format!("  trapped at: {}\n", last_test)); }
                    for line in stderr.lines().take(2) { detail.push_str(&format!("  {}\n", line)); }
                    WasmTestOutcome::Fail { file: test_file.to_string(), detail }
                }
            }
            Err(e) => skip(format!("wasmtime: {}", e)),
        }
    };
    if prof {
        mark(prof, &mut marks, "codegen");
        print_wasm_test_profile(test_file, &marks);
    }
    // v1 is the ONLY wasm path (#782), and where it renders its verdict is FINAL:
    // a v1 run failure routes to the authoritative NATIVE fallback, never to a
    // retry on unverified codegen. The old v0 retry existed for the #790 vein
    // (v1 runtime defects trapping where v0 ran) — that vein is closed, and the
    // retry's real effect had inverted: v0 DCEs whole test bodies (#792 vacuous
    // ok), so a GENUINELY failing test (v1 correctly aborting on `none!`) was
    // overwritten by a hollow v0 "pass". A v1 WALL is an honest skip that routes
    // the file to native — the shrinking #813 remainder.
    match v1_bytes {
        Some(b) => run_module(&b),
        None => skip("v1 wall — no verified wasm rendering".to_string()),
    }
}

pub fn cmd_test_wasm(file: &str, _run_filter: Option<&str>) {
    let test_files: Vec<String> = discover_test_files(file, &[]);

    let tmp_dir = std::env::temp_dir().join("almide-wasm-test");
    std::fs::create_dir_all(&tmp_dir).ok();

    // Parallel: each file's compile+run is independent and rustc/cargo-free,
    // so there's no global build lock to serialize on (unlike the native path).
    let cpus = std::thread::available_parallelism().map(|n| n.get()).unwrap_or(4);
    let tmp_dir = std::sync::Arc::new(tmp_dir);
    let (tx, rx) = std::sync::mpsc::channel();
    let (sem_tx, sem_rx) = std::sync::mpsc::sync_channel::<()>(cpus);
    for _ in 0..cpus { let _ = sem_tx.send(()); }
    let sem_tx = std::sync::Arc::new(sem_tx);
    let sem_rx = std::sync::Arc::new(std::sync::Mutex::new(sem_rx));
    let mut handles = Vec::new();
    for test_file in test_files.clone() {
        let tx = tx.clone();
        let tmp_dir = tmp_dir.clone();
        let sem_rx = sem_rx.clone();
        let sem_tx = sem_tx.clone();
        handles.push(std::thread::spawn(move || {
            let _ = sem_rx.lock().unwrap().recv();
            let outcome = compile_and_run_wasm_test(&test_file, &tmp_dir);
            let _ = sem_tx.send(());
            let _ = tx.send(outcome);
        }));
    }
    drop(tx);
    let mut outcomes: Vec<WasmTestOutcome> = rx.iter().collect();
    for h in handles { let _ = h.join(); }
    let file_of = |o: &WasmTestOutcome| match o {
        WasmTestOutcome::Pass { file, .. }
        | WasmTestOutcome::Fail { file, .. }
        | WasmTestOutcome::CompileError { file, .. }
        | WasmTestOutcome::Skip { file, .. } => file.clone(),
    };
    outcomes.sort_by(|a, b| file_of(a).cmp(&file_of(b)));

    let mut failed = 0;
    let mut passed = 0;
    let mut skipped = 0;
    for o in &outcomes {
        match o {
            WasmTestOutcome::Pass { file, count, bytes } => {
                err(&format!("{}: {} tests passed ({} bytes)", file, count, bytes));
                passed += 1;
            }
            WasmTestOutcome::Fail { file, detail } => {
                err(&format!("FAIL {}", file));
                err_no_nl(&format!("{}", detail));
                failed += 1;
            }
            // Broken on every target — a FAIL verdict here, matching the
            // default harness; SKIP is reserved for honest walls (#957).
            WasmTestOutcome::CompileError { file, detail } => {
                err(&format!("FAIL {} (does not compile)", file));
                err_no_nl(&format!("{}", detail));
                failed += 1;
            }
            WasmTestOutcome::Skip { file, reason } => {
                err(&format!("SKIP {} ({})", file, reason));
                skipped += 1;
            }
        }
    }

    err("");
    if skipped > 0 {
        err(&format!("{} passed, {} failed, {} skipped (of {} files)",
            passed, failed, skipped, test_files.len()));
    } else {
        err(&format!("{} passed, {} failed (of {} files)",
            passed, failed, test_files.len()));
    }
    if failed > 0 {
        std::process::exit(1);
    }
}

/// `cmd_test_fast`'s Phase 1: run every file on the fast rustc-free WASM
/// path, in parallel (bounded by `cpus`). Extracted verbatim.
fn run_wasm_test_phase(test_files: &[String], tmp_dir: &std::sync::Arc<std::path::PathBuf>, cpus: usize) -> Vec<WasmTestOutcome> {
    let (tx, rx) = std::sync::mpsc::channel();
    let (sem_tx, sem_rx) = std::sync::mpsc::sync_channel::<()>(cpus);
    for _ in 0..cpus { let _ = sem_tx.send(()); }
    let sem_tx = std::sync::Arc::new(sem_tx);
    let sem_rx = std::sync::Arc::new(std::sync::Mutex::new(sem_rx));
    let mut handles = Vec::new();
    for tf in test_files.to_vec() {
        let tx = tx.clone();
        let td = tmp_dir.clone();
        let sr = sem_rx.clone();
        let st = sem_tx.clone();
        handles.push(std::thread::spawn(move || {
            let _ = sr.lock().unwrap().recv();
            let o = compile_and_run_wasm_test(&tf, &td);
            let _ = st.send(());
            let _ = tx.send(o);
        }));
    }
    drop(tx);
    let v: Vec<_> = rx.iter().collect();
    for h in handles { let _ = h.join(); }
    v
}

/// `cmd_test_fast`'s Phase 2: native rustc fallback (authoritative) for
/// everything the WASM path didn't pass, parallel with per-file scratch
/// dirs. Extracted verbatim.
fn run_native_fallback_phase(fallback: &[String], program_args: &std::sync::Arc<Vec<String>>, no_check: bool, cpus: usize) -> Vec<(String, i32)> {
    let (tx, rx) = std::sync::mpsc::channel();
    let (sem_tx, sem_rx) = std::sync::mpsc::sync_channel::<()>(cpus);
    for _ in 0..cpus { let _ = sem_tx.send(()); }
    let sem_tx = std::sync::Arc::new(sem_tx);
    let sem_rx = std::sync::Arc::new(std::sync::Mutex::new(sem_rx));
    let mut handles = Vec::new();
    for tf in fallback.to_vec() {
        let tx = tx.clone();
        let args = program_args.clone();
        let sr = sem_rx.clone();
        let st = sem_tx.clone();
        handles.push(std::thread::spawn(move || {
            let _ = sr.lock().unwrap().recv();
            let worker_dir = std::env::temp_dir()
                .join("almide-test")
                .join(tf.replace('/', "_").replace('.', "_"));
            let code = match super::run::compile_to_binary(&tf, no_check, true, false, Some(&worker_dir)) {
                Ok(bin) => super::run::run_binary(&bin, &args),
                Err(e) => { err(&format!("Compile error for {}:\n{}", tf, e)); 1 }
            };
            let _ = st.send(());
            let _ = tx.send((tf, code));
        }));
    }
    drop(tx);
    let mut v: Vec<(String, i32)> = rx.iter().collect();
    for h in handles { let _ = h.join(); }
    v.sort_by(|a, b| a.0.cmp(&b.0));
    v
}

/// Default `almide test`: run each file on the fast rustc-free WASM path; for
/// any file the WASM path can't pass (emitter gap, wasm:skip, or a trap), fall
/// back to the native rustc path, which is authoritative. The common case (most
/// tests pass on WASM) is ~9x faster; the native fallback preserves correctness.
pub fn cmd_test_fast(file: &str, no_check: bool, run_filter: Option<&str>) {
    let test_files: Vec<String> = discover_test_files(file, &["spec", "exercises"]);

    let cpus = std::thread::available_parallelism().map(|n| n.get()).unwrap_or(4);
    let tmp_dir = std::sync::Arc::new(std::env::temp_dir().join("almide-wasm-test"));
    std::fs::create_dir_all(&*tmp_dir).ok();

    // Phase 1: WASM (fast, rustc-free), parallel.
    let wasm_outcomes = run_wasm_test_phase(&test_files, &tmp_dir, cpus);

    let mut wasm_pass = 0usize;
    let mut fallback: Vec<String> = Vec::new();
    let mut trapped: Vec<(String, String)> = Vec::new();
    for o in wasm_outcomes {
        match o {
            WasmTestOutcome::Pass { .. } => wasm_pass += 1,
            // A `Fail` is DIFFERENT IN KIND from the benign fallback classes
            // (#1166): the wasm leg COMPILED the file, claimed it, and produced
            // a runtime failure. Whether that is a plain failing test or a
            // MISCOMPILE only the native re-run can say — so hold the detail
            // and judge after phase 2: native red → an ordinary FAILED (no
            // wasm-specific noise); native green → the divergence class the
            // walls exist to prevent, reported loudly below. Silently folding
            // it into "via native fallback" hid the #1165 `indirect call type
            // mismatch` for its whole life locally while CI's Test WASM failed
            // the PR.
            WasmTestOutcome::Fail { file, detail } => {
                trapped.push((file.clone(), detail));
                fallback.push(file);
            }
            // CompileError routes to the fallback like everything else here:
            // the native leg re-runs it and reports the diagnostics
            // authoritatively (#862), turning it into a counted FAILED.
            WasmTestOutcome::CompileError { file, .. }
            | WasmTestOutcome::Skip { file, .. } => fallback.push(file),
        }
    }

    // Phase 2: native rustc fallback (authoritative) for everything the WASM
    // path didn't pass, parallel with per-file scratch dirs.
    let mut program_args: Vec<String> = Vec::new();
    if let Some(f) = run_filter { program_args.push(f.to_string()); }
    let program_args = std::sync::Arc::new(program_args);

    let native_results = run_native_fallback_phase(&fallback, &program_args, no_check, cpus);

    let mut failed = 0;
    for (file, code) in &native_results {
        if *code != 0 { err(&format!("FAILED: {}", file)); failed += 1; }
    }
    // The #1166 divergence class: the wasm leg compiled the file and failed at
    // runtime, but the AUTHORITATIVE native re-run passed — a wasm-only
    // miscompile signal, exactly what CI's Test WASM job reds a PR for. Report
    // each one loudly (still counted "via native fallback": the suite verdict
    // is native's). A trap whose native re-run ALSO failed is a plain FAILED
    // test — no wasm-specific noise for those.
    let native_code: std::collections::HashMap<&String, i32> =
        native_results.iter().map(|(f, c)| (f, *c)).collect();
    let diverged: Vec<&(String, String)> = trapped
        .iter()
        .filter(|(f, _)| native_code.get(f).copied() == Some(0))
        .collect();
    for (file, detail) in &diverged {
        err(&format!("WASM TRAP {} (compiled for wasm, failed at runtime; native re-run PASSED — CI's Test WASM will fail this)", file));
        err_no_nl(detail);
    }
    // The wasm COVERAGE ratchet's data feed (mission-critical arc): every
    // file the wasm leg did not pass, one per line, so
    // proofs/check-wasm-fallback.sh can diff the set against its shrink-only
    // baseline. Names only under the flag — the summary line stays stable.
    if std::env::var_os("ALMIDE_FALLBACK_NAMES").is_some() {
        let mut sorted = fallback.clone();
        sorted.sort();
        for f in &sorted {
            err(&format!("FALLBACK {}", f));
        }
    }
    let trap_note = if diverged.is_empty() {
        String::new()
    } else {
        format!(" ({} after a wasm TRAP)", diverged.len())
    };
    err(&format!("\n{} via WASM, {} via native fallback{}, {} failed (of {} files)",
        wasm_pass, fallback.len().saturating_sub(failed), trap_note, failed, test_files.len()));
    if failed > 0 {
        std::process::exit(1);
    }
    // Pre-push strict mode: a diverged trap fails the run even though the
    // native re-run passed — CI's Test WASM verdict, surfaced locally instead
    // of on the PR (#1166).
    if !diverged.is_empty() && std::env::var_os("ALMIDE_TEST_STRICT_WASM").is_some() {
        err(&format!(
            "STRICT WASM: {} file(s) trapped on the wasm leg (native re-run passed; CI's Test WASM will fail)",
            diverged.len()
        ));
        std::process::exit(1);
    }
    err(&format!("All {} test file(s) passed", test_files.len()));
}

pub fn cmd_test_json(file: &str, run_filter: Option<&str>) {
    let test_files: Vec<String> = if !file.is_empty() {
        let path = std::path::Path::new(file);
        if path.is_dir() {
            let mut files = collect_test_files(path);
            files.sort();
            files
        } else {
            vec![file.to_string()]
        }
    } else {
        let mut files = collect_test_files(std::path::Path::new("."));
        files.sort();
        files
    };

    let mut program_args: Vec<String> = Vec::new();
    if let Some(filter) = run_filter {
        program_args.push(filter.to_string());
    }

    for test_file in &test_files {
        let code = super::cmd_run_inner(test_file, &program_args, false, true, false, false);
        // Emit JSON per file
        let status = if code == 0 { "pass" } else { "fail" };
        out(&format!(
            r#"{{"file":"{}","status":"{}","exit_code":{}}}"#,
            test_file.replace('"', r#"\""#), status, code
        ));
    }
}

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
    let cache = crate::project::cache_dir();
    for dep in &project.dependencies {
        // Check cache dir: ~/.almide/cache/{name}/{tag_or_latest}/
        let dep_cache = cache.join(&dep.name);
        if dep_cache.is_dir() {
            // Use the first subdirectory (version) found
            if let Ok(entries) = std::fs::read_dir(&dep_cache) {
                if let Some(version_dir) = entries.flatten().find(|e| e.path().is_dir()) {
                    scan_submodules(&version_dir.path(), &dep.name, &mut submodules);
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
    /// Print the formatted text; never write, never fail.
    DryRun,
}

pub fn cmd_fmt(files: &[String], mode: FmtMode, no_import_edit: bool) {
    // Load dependency info from almide.toml (if present)
    let (dep_names, dep_submodules) = load_dep_info_for_fmt();
    // `--check` is a gate: a file that differs, and a file that cannot even be parsed,
    // both make the run fail. Under the other modes a parse error stays a skip.
    let mut unformatted: Vec<String> = Vec::new();
    let mut unreadable = false;

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
            unreadable = true;
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
        match mode {
            FmtMode::Write => {
                std::fs::write(file, &formatted)
                    .unwrap_or_else(|e| { err(&format!("Failed to write {}: {}", file, e)); std::process::exit(1); });
                err(&format!("Formatted {}", file));
            }
            FmtMode::Check => {
                if formatted != source_text {
                    unformatted.push(file.clone());
                }
            }
            FmtMode::DryRun => out_no_nl(&format!("{}", formatted)),
        }
    }

    if mode != FmtMode::Check {
        return;
    }
    if unformatted.is_empty() && !unreadable {
        err(&format!("fmt: {} file(s) already formatted", files.len()));
        return;
    }
    for f in &unformatted {
        err(&format!("not formatted: {}", f));
    }
    err(&format!(
        "fmt --check: {} of {} file(s) need formatting — run `almide fmt <path>`",
        unformatted.len(),
        files.len()
    ));
    std::process::exit(1);
}

pub fn cmd_clean() {
    let mut cleaned = false;
    let dep_cache = project::cache_dir();
    if dep_cache.exists() {
        std::fs::remove_dir_all(&dep_cache)
            .unwrap_or_else(|e| { err(&format!("Failed to clean cache: {}", e)); std::process::exit(1); });
        err(&format!("Cleaned {}", dep_cache.display()));
        cleaned = true;
    }
    let inc_cache = incremental_cache_dir();
    if inc_cache.exists() {
        std::fs::remove_dir_all(&inc_cache)
            .unwrap_or_else(|e| { err(&format!("Failed to clean incremental cache: {}", e)); std::process::exit(1); });
        err(&format!("Cleaned {}", inc_cache.display()));
        cleaned = true;
    }
    let compile_cache = std::path::PathBuf::from("target/compile");
    if compile_cache.exists() {
        std::fs::remove_dir_all(&compile_cache)
            .unwrap_or_else(|e| { err(&format!("Failed to clean compile cache: {}", e)); std::process::exit(1); });
        err(&format!("Cleaned {}", compile_cache.display()));
        cleaned = true;
    }
    if !cleaned {
        err(&format!("No cache to clean"));
    }
}

