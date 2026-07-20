use crate::{parse_file, fmt, project};
use super::{collect_test_files, incremental_cache_dir};

pub fn cmd_init() {
    if std::path::Path::new("almide.toml").exists() {
        eprintln!("almide.toml already exists");
        std::process::exit(1);
    }
    let dir_name = std::env::current_dir()
        .ok()
        .and_then(|p| p.file_name().map(|n| n.to_string_lossy().to_string()))
        .unwrap_or_else(|| "myapp".to_string());

    let toml = format!("[package]\nname = \"{}\"\nversion = \"0.1.0\"\nedition = \"2026\"\n", dir_name);

    if let Err(e) = std::fs::write("almide.toml", toml) {
        eprintln!("Failed to write almide.toml: {}", e);
        std::process::exit(1);
    }
    if let Err(e) = std::fs::create_dir_all("src") {
        eprintln!("Failed to create src/: {}", e);
        std::process::exit(1);
    }
    if let Err(e) = std::fs::create_dir_all("tests") {
        eprintln!("Failed to create tests/: {}", e);
        std::process::exit(1);
    }

    if !std::path::Path::new("src/main.almd").exists() {
        if let Err(e) = std::fs::write("src/main.almd", "effect fn main() -> Unit = {\n  println(\"Hello, Almide!\")\n}\n") {
            eprintln!("Failed to write src/main.almd: {}", e);
            std::process::exit(1);
        }
    }

    // Generate CLAUDE.md for AI-assisted development
    if !std::path::Path::new("CLAUDE.md").exists() {
        let claude_md = include_str!("../../docs/CLAUDE_TEMPLATE.md");
        if let Err(e) = std::fs::write("CLAUDE.md", claude_md) {
            eprintln!("Failed to write CLAUDE.md: {}", e);
            std::process::exit(1);
        }
    }

    eprintln!("Initialized project in ./");
    eprintln!("  almide.toml");
    eprintln!("  src/main.almd");
    eprintln!("  tests/");
    eprintln!("  CLAUDE.md");
}

pub fn cmd_test(file: &str, no_check: bool, run_filter: Option<&str>) {
    let test_files: Vec<String> = if !file.is_empty() {
        let path = std::path::Path::new(file);
        if path.is_dir() {
            let mut files = collect_test_files(path);
            files.sort();
            if files.is_empty() {
                eprintln!("No .almd files with test blocks found in {}", file);
                std::process::exit(1);
            }
            files
        } else {
            vec![file.to_string()]
        }
    } else {
        // Default: recursively find test files in spec/ and exercises/ (standard test directories)
        let mut files = Vec::new();
        for dir in &["spec", "exercises"] {
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
            eprintln!("No .almd files with test blocks found.");
            std::process::exit(1);
        }
        files
    };

    let mut program_args: Vec<String> = Vec::new();
    if let Some(filter) = run_filter {
        program_args.push(filter.to_string());
    }

    // Phase 1: Compile all test files in parallel (bounded by CPU count)
    let compiled: Vec<(String, Result<std::path::PathBuf, String>)> = {
        let cpus = std::thread::available_parallelism().map(|n| n.get()).unwrap_or(4);
        let (tx, rx) = std::sync::mpsc::channel();
        let (sem_tx, sem_rx) = std::sync::mpsc::sync_channel::<()>(cpus);
        for _ in 0..cpus { let _ = sem_tx.send(()); }
        let sem_tx = std::sync::Arc::new(sem_tx);
        let sem_rx = std::sync::Arc::new(std::sync::Mutex::new(sem_rx));
        let mut handles = Vec::new();
        for test_file in test_files.clone() {
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
    };

    // Phase 2: Execute test binaries in parallel (bounded by CPU count)
    let program_args = std::sync::Arc::new(program_args);
    let results: Vec<(String, i32)> = {
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
                        eprintln!("Compile error for {}:\n{}", file, e);
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
    };

    let mut failed = 0;
    for (file, code) in &results {
        if *code != 0 {
            eprintln!("FAILED: {}", file);
            failed += 1;
        }
    }
    if failed > 0 {
        eprintln!("\n{}/{} test file(s) failed", failed, test_files.len());
        std::process::exit(1);
    }
    eprintln!("\nAll {} test file(s) passed", test_files.len());
}

enum WasmTestOutcome {
    Pass { file: String, count: usize, bytes: usize },
    Fail { file: String, detail: String },
    Skip { file: String, reason: String },
}

/// Compile one `.almd` file to WASM and run it under wasmtime. Pure per-file
/// work (no shared mutable state) so it runs in parallel — the WASM path takes
/// no rustc/cargo, so there's no global build lock to serialize on.
fn compile_and_run_wasm_test(test_file: &str, tmp_dir: &std::path::Path) -> WasmTestOutcome {
    use crate::{parse_file, canonicalize, check, diagnostic, resolve, project, project_fetch};
    let skip = |reason: String| WasmTestOutcome::Skip { file: test_file.to_string(), reason };
    let prof = std::env::var_os("ALMIDE_PROFILE").is_some();
    let mut marks: Vec<(&str, std::time::Instant)> = vec![("start", std::time::Instant::now())];

    let wasm_name = test_file.replace('/', "_").replace('.', "_") + ".wasm";
    let wasm_path = tmp_dir.join(&wasm_name);

    let (mut program, source_text, parse_errors) = parse_file(test_file);
    if prof { marks.push(("parse", std::time::Instant::now())); }
    if source_text.lines().take(3).any(|line| line.contains("// wasm:skip")) {
        return skip("wasm:skip".to_string());
    }
    // A parse error leaves an error-recovered (partial) AST. Compiling and
    // running that mangled module would report a PASS, so a broken file looked
    // green on the WASM path (only the rust path surfaced it). It is a real
    // failure — NOT a benign skip like `// wasm:skip` — so report it as Fail:
    // `cmd_test_wasm` then counts it failed, and `cmd_test_fast` routes it to the
    // authoritative native fallback, which prints the full diagnostics.
    if parse_errors.iter().any(|d| d.level == diagnostic::Level::Error) {
        let mut detail = String::new();
        for d in parse_errors.iter().filter(|d| d.level == diagnostic::Level::Error).take(3) {
            detail.push_str(&format!("  parse error: {}\n", d.message));
        }
        return WasmTestOutcome::Fail { file: test_file.to_string(), detail };
    }
    // A file with BOTH `main` and `test` blocks: the v1 wasm test-mode renderer
    // (almide_mir::pipeline::synthesize_test_runner_main) intentionally leaves
    // `main`-bearing files on the ordinary `__main_runner` protocol and never
    // synthesizes the `__test_runner` — so the wasm leg compiles and runs ONLY
    // `main`, never the `test` blocks, and reports the whole file Pass as long
    // as `main` exits cleanly. A file's tests can be silently unexecuted (not
    // merely mis-scored) on this leg — the false-green class in
    // feedback_wasm_test_parse_error_false_pass, a fresh trigger (runtime
    // assertions, not parse errors). Skip straight to the authoritative native
    // fallback rather than trust a Pass that never checked the tests at all.
    let has_main = program
        .decls
        .iter()
        .any(|d| matches!(d, almide_lang::ast::Decl::Fn { name, .. } if name.as_str() == "main"));
    let has_test = program.decls.iter().any(|d| matches!(d, almide_lang::ast::Decl::Test { .. }));
    if has_main && has_test {
        return skip("main + test blocks: wasm test-mode runs main only, not the tests".to_string());
    }

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

    let mut resolved = match resolve::resolve_imports_with_deps(test_file, &program, &dep_paths) {
        Ok(r) => r,
        Err(e) => return skip(format!("resolve: {}", e)),
    };
    if prof { marks.push(("resolve", std::time::Instant::now())); }

    // v1 verified leg (the DEFAULT build/run wasm path since 0.29.0): capture the FRESH
    // (un-inferred) cross-module siblings now — the infer loop below mutates them in
    // place, and the v1 pipeline re-runs its own canonicalize/infer/lower from raw
    // programs (exactly `compile_to_wasm_bytes`'s capture). Tried after the v0 gates
    // below; a wall falls through to the v0 emit — same honest-wall contract as build.
    let v1_self_modules: Vec<(String, almide_lang::ast::Program, bool)> =
        resolved.modules.iter().map(|(n, p, _pkg, s)| (n.clone(), p.clone(), *s)).collect();

    let canon = canonicalize::canonicalize_program(
        &program,
        resolved.modules.iter().map(|(n, p, _, s)| (n.as_str(), p, *s)),
    );
    let mut checker = check::Checker::from_env(canon.env);
    checker.set_source(test_file, &source_text);
    checker.diagnostics = canon.diagnostics;
    // #785: module top-let types must be fully inferred before the entry
    // program reads them (drivers infer the entry FIRST; without this the
    // readers see the registration seed — Unknown for non-literal inits).
    almide::resolve::refresh_module_toplets(&mut checker, &resolved.modules);
    let diagnostics = checker.infer_program(&mut program);
    if diagnostics.iter().any(|d| d.level == diagnostic::Level::Error) {
        return skip("type errors".to_string());
    }
    if prof { marks.push(("check_user", std::time::Instant::now())); }

    for (name, _, pkg_id, _) in &resolved.modules {
        if let Some(pid) = pkg_id.as_ref() {
            let base = pid.mod_name();
            let v = if let Some(suffix) = name.strip_prefix(&pid.name) { format!("{}{}", base, suffix) } else { base };
            checker.env.module_versioned_names.insert(almide::intern::sym(name), almide::intern::sym(&v));
        }
    }
    let mut ir_program = almide::lower::lower_program(&program, &checker.env, &checker.type_map);
    for (name, mod_prog, pkg_id, _) in &mut resolved.modules {
        if almide::stdlib::is_stdlib_module(name) && !almide::stdlib::is_bundled_module(name) { continue; }
        let saved_self = checker.env.self_module_name;
        if let Some(pid) = pkg_id.as_ref() {
            checker.env.self_module_name = Some(almide::intern::sym(&pid.name));
        }
        checker.infer_module(mod_prog, name);
        let versioned = pkg_id.as_ref().map(|pid| {
            let base = pid.mod_name();
            if let Some(suffix) = name.strip_prefix(&pid.name) { format!("{}{}", base, suffix) } else { base }
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
    if prof { marks.push(("lower_modules", std::time::Instant::now())); }
    almide::ir_link::ir_link(&mut ir_program);
    almide::optimize::optimize_program(&mut ir_program);
    almide::mono::monomorphize(&mut ir_program);
    if prof { marks.push(("opt_mono", std::time::Instant::now())); }
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
    // as a wall and fall through to the v0 emit (honest, and the file stays on wasm).
    let v1_bytes: Option<Vec<u8>> = almide_mir::pipeline::try_render_wasm_source_tests(
        &source_text,
        &v1_self_modules,
        false,
    )
    .ok()
    .and_then(|wat_text| wat::parse_str(&wat_text).ok())
    .filter(|bytes| wasmparser::validate(bytes).is_ok());
    let v0_bytes = |ir_program: &mut almide_ir::IrProgram| -> Result<Vec<u8>, ()> {
        std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            match almide::codegen::codegen(ir_program, almide::codegen::pass::Target::Wasm) {
                almide::codegen::CodegenOutput::Binary(b) => b,
                almide::codegen::CodegenOutput::Source(_) => unreachable!(),
            }
        }))
        .map_err(|_| ())
    };
    // Write the module and run it under wasmtime. `-S inherit-env=y` mirrors
    // `cmd_run_wasm`: `env.get` in a test observes the same host variables native
    // does (the env cross-target contract).
    let run_module = |bytes: &[u8]| -> WasmTestOutcome {
        if let Err(e) = std::fs::write(&wasm_path, bytes) {
            return skip(format!("write: {}", e));
        }
        let output = std::process::Command::new("wasmtime")
            .arg("--dir=/")
            .arg("-S")
            .arg("inherit-env=y")
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
        marks.push(("codegen", std::time::Instant::now()));
        let total = marks.last().unwrap().1.duration_since(marks[0].1).as_secs_f64();
        let mut line = format!("[prof] {} total={:.3}s", test_file, total);
        for w in marks.windows(2) {
            line.push_str(&format!(" | {}={:.3}", w[1].0, w[1].1.duration_since(w[0].1).as_secs_f64()));
        }
        eprintln!("{}", line);
    }
    // v1 first, and where v1 RENDERS its verdict is FINAL: a v1 run failure routes
    // to the authoritative NATIVE fallback, never to a v0 retry. The old v0 retry
    // existed for the #790 vein (v1 runtime defects trapping where v0 ran) — that
    // vein is closed, and the retry's real effect had inverted: v0 DCEs whole test
    // bodies (#792 vacuous ok), so a GENUINELY failing test (v1 correctly aborting
    // on `none!`) was overwritten by a hollow v0 "pass". v0 still carries the files
    // v1 WALLS (no render) — the shrinking #782 remainder.
    if let Some(b) = v1_bytes {
        return run_module(&b);
    }
    match v0_bytes(&mut ir_program) {
        Ok(b) => run_module(&b),
        Err(()) => skip("WASM codegen panic".to_string()),
    }
}

pub fn cmd_test_wasm(file: &str, _run_filter: Option<&str>) {
    let test_files: Vec<String> = if !file.is_empty() {
        let path = std::path::Path::new(file);
        if path.is_dir() {
            let mut files = collect_test_files(path);
            files.sort();
            if files.is_empty() {
                eprintln!("No .almd files with test blocks found in {}", file);
                std::process::exit(1);
            }
            files
        } else {
            vec![file.to_string()]
        }
    } else {
        let mut files = collect_test_files(std::path::Path::new("."));
        files.sort();
        if files.is_empty() {
            eprintln!("No .almd files with test blocks found.");
            std::process::exit(1);
        }
        files
    };

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
        | WasmTestOutcome::Skip { file, .. } => file.clone(),
    };
    outcomes.sort_by(|a, b| file_of(a).cmp(&file_of(b)));

    let mut failed = 0;
    let mut passed = 0;
    let mut skipped = 0;
    for o in &outcomes {
        match o {
            WasmTestOutcome::Pass { file, count, bytes } => {
                eprintln!("{}: {} tests passed ({} bytes)", file, count, bytes);
                passed += 1;
            }
            WasmTestOutcome::Fail { file, detail } => {
                eprintln!("FAIL {}", file);
                eprint!("{}", detail);
                failed += 1;
            }
            WasmTestOutcome::Skip { file, reason } => {
                eprintln!("SKIP {} ({})", file, reason);
                skipped += 1;
            }
        }
    }

    eprintln!();
    if skipped > 0 {
        eprintln!("{} passed, {} failed, {} skipped (of {} files)",
            passed, failed, skipped, test_files.len());
    } else {
        eprintln!("{} passed, {} failed (of {} files)",
            passed, failed, test_files.len());
    }
    if failed > 0 {
        std::process::exit(1);
    }
}

/// Default `almide test`: run each file on the fast rustc-free WASM path; for
/// any file the WASM path can't pass (emitter gap, wasm:skip, or a trap), fall
/// back to the native rustc path, which is authoritative. The common case (most
/// tests pass on WASM) is ~9x faster; the native fallback preserves correctness.
pub fn cmd_test_fast(file: &str, no_check: bool, run_filter: Option<&str>) {
    let test_files: Vec<String> = if !file.is_empty() {
        let path = std::path::Path::new(file);
        if path.is_dir() {
            let mut files = collect_test_files(path);
            files.sort();
            if files.is_empty() {
                eprintln!("No .almd files with test blocks found in {}", file);
                std::process::exit(1);
            }
            files
        } else {
            vec![file.to_string()]
        }
    } else {
        let mut files = Vec::new();
        for dir in &["spec", "exercises"] {
            let path = std::path::Path::new(dir);
            if path.exists() { files.extend(collect_test_files(path)); }
        }
        if files.is_empty() { files = collect_test_files(std::path::Path::new(".")); }
        files.sort();
        if files.is_empty() {
            eprintln!("No .almd files with test blocks found.");
            std::process::exit(1);
        }
        files
    };

    let cpus = std::thread::available_parallelism().map(|n| n.get()).unwrap_or(4);
    let tmp_dir = std::sync::Arc::new(std::env::temp_dir().join("almide-wasm-test"));
    std::fs::create_dir_all(&*tmp_dir).ok();

    // Phase 1: WASM (fast, rustc-free), parallel.
    let wasm_outcomes: Vec<WasmTestOutcome> = {
        let (tx, rx) = std::sync::mpsc::channel();
        let (sem_tx, sem_rx) = std::sync::mpsc::sync_channel::<()>(cpus);
        for _ in 0..cpus { let _ = sem_tx.send(()); }
        let sem_tx = std::sync::Arc::new(sem_tx);
        let sem_rx = std::sync::Arc::new(std::sync::Mutex::new(sem_rx));
        let mut handles = Vec::new();
        for tf in test_files.clone() {
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
    };

    let mut wasm_pass = 0usize;
    let mut fallback: Vec<String> = Vec::new();
    for o in wasm_outcomes {
        match o {
            WasmTestOutcome::Pass { .. } => wasm_pass += 1,
            WasmTestOutcome::Fail { file, .. } | WasmTestOutcome::Skip { file, .. } => fallback.push(file),
        }
    }

    // Phase 2: native rustc fallback (authoritative) for everything the WASM
    // path didn't pass, parallel with per-file scratch dirs.
    let mut program_args: Vec<String> = Vec::new();
    if let Some(f) = run_filter { program_args.push(f.to_string()); }
    let program_args = std::sync::Arc::new(program_args);

    let native_results: Vec<(String, i32)> = {
        let (tx, rx) = std::sync::mpsc::channel();
        let (sem_tx, sem_rx) = std::sync::mpsc::sync_channel::<()>(cpus);
        for _ in 0..cpus { let _ = sem_tx.send(()); }
        let sem_tx = std::sync::Arc::new(sem_tx);
        let sem_rx = std::sync::Arc::new(std::sync::Mutex::new(sem_rx));
        let mut handles = Vec::new();
        for tf in fallback.clone() {
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
                    Err(e) => { eprintln!("Compile error for {}:\n{}", tf, e); 1 }
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
    };

    let mut failed = 0;
    for (file, code) in &native_results {
        if *code != 0 { eprintln!("FAILED: {}", file); failed += 1; }
    }
    eprintln!("\n{} via WASM, {} via native fallback, {} failed (of {} files)",
        wasm_pass, fallback.len().saturating_sub(failed), failed, test_files.len());
    if failed > 0 {
        std::process::exit(1);
    }
    eprintln!("All {} test file(s) passed", test_files.len());
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
        println!(
            r#"{{"file":"{}","status":"{}","exit_code":{}}}"#,
            test_file.replace('"', r#"\""#), status, code
        );
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

pub fn cmd_fmt(files: &[String], write_back: bool) {
    // Load dependency info from almide.toml (if present)
    let (dep_names, dep_submodules) = load_dep_info_for_fmt();

    for file in files {
        let (mut program, source_text, parse_errors) = parse_file(file);
        if !parse_errors.is_empty() {
            // A partially-parsed program silently drops unparseable top-level
            // items — formatting it and writing back would delete that code
            // from the file on disk. Report and skip instead.
            for e in &parse_errors {
                eprintln!("{}", crate::diagnostic_render::display_with_source(e, &source_text));
            }
            eprintln!("{}: {} parse error(s), skipping", file, parse_errors.len());
            continue;
        }
        // Auto-manage imports: add missing, remove unused
        let import_changes = fmt::auto_imports(&mut program, &source_text, &dep_names, &dep_submodules);
        for msg in &import_changes {
            eprintln!("{}: {}", file, msg);
        }
        let formatted = fmt::format_program(&program);
        if write_back {
            std::fs::write(file, &formatted)
                .unwrap_or_else(|e| { eprintln!("Failed to write {}: {}", file, e); std::process::exit(1); });
            eprintln!("Formatted {}", file);
        } else {
            print!("{}", formatted);
        }
    }
}

pub fn cmd_clean() {
    let mut cleaned = false;
    let dep_cache = project::cache_dir();
    if dep_cache.exists() {
        std::fs::remove_dir_all(&dep_cache)
            .unwrap_or_else(|e| { eprintln!("Failed to clean cache: {}", e); std::process::exit(1); });
        eprintln!("Cleaned {}", dep_cache.display());
        cleaned = true;
    }
    let inc_cache = incremental_cache_dir();
    if inc_cache.exists() {
        std::fs::remove_dir_all(&inc_cache)
            .unwrap_or_else(|e| { eprintln!("Failed to clean incremental cache: {}", e); std::process::exit(1); });
        eprintln!("Cleaned {}", inc_cache.display());
        cleaned = true;
    }
    let compile_cache = std::path::PathBuf::from("target/compile");
    if compile_cache.exists() {
        std::fs::remove_dir_all(&compile_cache)
            .unwrap_or_else(|e| { eprintln!("Failed to clean compile cache: {}", e); std::process::exit(1); });
        eprintln!("Cleaned {}", compile_cache.display());
        cleaned = true;
    }
    if !cleaned {
        eprintln!("No cache to clean");
    }
}

