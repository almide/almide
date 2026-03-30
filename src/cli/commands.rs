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
        if let Err(e) = std::fs::write("src/main.almd", "effect fn main(args: List[String]) -> Result[Unit, String] = {\n  println(\"Hello, Almide!\")\n  ok(())\n}\n") {
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

    // Phase 1: Compile all test files sequentially (shared cargo workspace)
    let mut compiled: Vec<(String, Result<std::path::PathBuf, String>)> = Vec::new();
    for test_file in &test_files {
        eprintln!("Compiling {}", test_file);
        let result = super::run::compile_to_binary(test_file, no_check, true);
        compiled.push((test_file.clone(), result));
    }

    // Phase 2: Execute test binaries in parallel (bounded by CPU count)
    let program_args = std::sync::Arc::new(program_args);
    let results: Vec<(String, i32)> = {
        let cpus = std::thread::available_parallelism().map(|n| n.get()).unwrap_or(4);
        // Channel-based semaphore: pre-fill with `cpus` tokens
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
                // Acquire semaphore token
                let _ = sem_rx.lock().unwrap().recv();
                let code = match compile_result {
                    Ok(bin) => super::run::run_binary(&bin, &args),
                    Err(e) => {
                        eprintln!("Compile error for {}:\n{}", file, e);
                        1
                    }
                };
                // Release semaphore token
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

pub fn cmd_test_wasm(file: &str, _run_filter: Option<&str>) {
    use crate::{parse_file, check, diagnostic, resolve, project, project_fetch};

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

    let mut failed = 0;
    let mut passed = 0;
    let mut skipped = 0;

    for test_file in &test_files {
        // Build WASM binary
        let wasm_name = test_file.replace('/', "_").replace('.', "_") + ".wasm";
        let wasm_path = tmp_dir.join(&wasm_name);

        let (mut program, source_text, _parse_errors) = parse_file(test_file);

        // Skip files marked with // wasm:skip
        if source_text.lines().take(3).any(|line| line.contains("// wasm:skip")) {
            skipped += 1;
            continue;
        }

        // Resolve dependencies
        let dep_paths: Vec<(project::PkgId, std::path::PathBuf)> =
            if std::path::Path::new("almide.toml").exists() {
                if let Ok(proj) = project::parse_toml(std::path::Path::new("almide.toml")) {
                    project_fetch::fetch_all_deps(&proj)
                        .unwrap_or_else(|e| { eprintln!("{}", e); vec![] })
                        .into_iter()
                        .map(|fd| (fd.pkg_id, fd.source_dir))
                        .collect()
                } else { vec![] }
            } else { vec![] };

        let mut resolved = match resolve::resolve_imports_with_deps(test_file, &program, &dep_paths) {
            Ok(r) => r,
            Err(e) => {
                eprintln!("SKIP {} (resolve: {})", test_file, e);
                skipped += 1;
                continue;
            }
        };

        let mut checker = check::Checker::new();
        checker.set_source(test_file, &source_text);
        for (name, mod_prog, pkg_id, is_self) in &resolved.modules {
            checker.register_module(name, mod_prog, pkg_id.as_ref(), *is_self);
        }
        let diagnostics = checker.check_program(&mut program);
        if diagnostics.iter().any(|d| d.level == diagnostic::Level::Error) {
            eprintln!("SKIP {} (type errors)", test_file);
            skipped += 1;
            continue;
        }

        let mut ir_program = almide::lower::lower_program(&program, &checker.expr_types, &checker.env);
        // Lower user modules to IR
        for (name, mod_prog, pkg_id, _) in &mut resolved.modules {
            if almide::stdlib::is_stdlib_module(name) { continue; }
            let mod_types = checker.check_module_bodies(mod_prog);
            let versioned = pkg_id.as_ref().map(|pid| {
                let base = pid.mod_name();
                if let Some(suffix) = name.strip_prefix(&pid.name) { format!("{}{}", base, suffix) } else { base }
            });
            let mod_ir_module = almide::lower::lower_module(name, mod_prog, &mod_types, &checker.env, versioned);
            ir_program.modules.push(mod_ir_module);
        }
        almide::optimize::optimize_program(&mut ir_program);
        almide::mono::monomorphize(&mut ir_program);
        let bytes = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            match almide::codegen::codegen(&mut ir_program, almide::codegen::pass::Target::Wasm) {
                almide::codegen::CodegenOutput::Binary(b) => b,
                almide::codegen::CodegenOutput::Source(_) => unreachable!(),
            }
        }));
        let bytes = match bytes {
            Ok(b) => b,
            Err(_) => {
                eprintln!("SKIP {} (WASM codegen panic)", test_file);
                skipped += 1;
                continue;
            }
        };

        if let Err(e) = std::fs::write(&wasm_path, &bytes) {
            eprintln!("SKIP {} (write: {})", test_file, e);
            skipped += 1;
            continue;
        }

        // Run with wasmtime (preopened root dir for full filesystem access)
        let output = std::process::Command::new("wasmtime")
            .arg("--dir=/")
            .arg(wasm_path.to_str().unwrap())
            .output();

        match output {
            Ok(result) => {
                let stdout = String::from_utf8_lossy(&result.stdout);
                let stderr = String::from_utf8_lossy(&result.stderr);

                if result.status.success() {
                    // Count tests from output
                    let test_count = stdout.matches("ok\n").count();
                    eprintln!("{}: {} tests passed ({} bytes)", test_file, test_count, bytes.len());
                    passed += 1;
                } else {
                    // Find which test failed (last "test: NAME" before trap)
                    let lines: Vec<&str> = stdout.lines().collect();
                    let mut last_test = "";
                    for line in &lines {
                        if line.starts_with("test: ") {
                            last_test = line;
                        }
                    }
                    eprintln!("FAIL {}", test_file);
                    if !last_test.is_empty() {
                        eprintln!("  trapped at: {}", last_test);
                    }
                    if !stderr.is_empty() {
                        // Show just the trap message, not the full stack trace
                        for line in stderr.lines().take(2) {
                            eprintln!("  {}", line);
                        }
                    }
                    failed += 1;
                }
            }
            Err(e) => {
                eprintln!("SKIP {} (wasmtime: {})", test_file, e);
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

pub fn cmd_test_ts(file: &str, _run_filter: Option<&str>) {
    use crate::{parse_file, check, diagnostic, resolve, project, project_fetch, ast};

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
            if path.exists() {
                files.extend(collect_test_files(path));
            }
        }
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

    // Detect runtime: prefer deno, fallback to node
    let (runtime, runtime_args): (&str, Vec<&str>) = if std::process::Command::new("deno")
        .arg("--version")
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false)
    {
        ("deno", vec!["test", "--allow-all"])
    } else if std::process::Command::new("node")
        .arg("--version")
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false)
    {
        ("node", vec![])
    } else {
        eprintln!("Neither deno nor node found. Install Deno (recommended) or Node.js.");
        std::process::exit(1);
    };

    let tmp_dir = std::env::temp_dir().join("almide-ts-test");
    std::fs::create_dir_all(&tmp_dir).ok();

    let mut failed = 0;
    let mut passed = 0;
    let mut skipped = 0;

    for test_file in &test_files {
        let ts_name = test_file.replace('/', "_").replace('.', "_") + ".ts";
        let ts_path = tmp_dir.join(&ts_name);

        let (mut program, source_text, _parse_errors) = parse_file(test_file);

        // Skip files marked with // ts:skip
        if source_text.lines().take(3).any(|line| line.contains("// ts:skip")) {
            skipped += 1;
            continue;
        }

        // Resolve dependencies
        let dep_paths: Vec<(project::PkgId, std::path::PathBuf)> =
            if std::path::Path::new("almide.toml").exists() {
                if let Ok(proj) = project::parse_toml(std::path::Path::new("almide.toml")) {
                    project_fetch::fetch_all_deps(&proj)
                        .unwrap_or_else(|e| { eprintln!("{}", e); vec![] })
                        .into_iter()
                        .map(|fd| (fd.pkg_id, fd.source_dir))
                        .collect()
                } else { vec![] }
            } else { vec![] };

        let mut resolved = match resolve::resolve_imports_with_deps(test_file, &program, &dep_paths) {
            Ok(r) => r,
            Err(e) => {
                eprintln!("SKIP {} (resolve: {})", test_file, e);
                skipped += 1;
                continue;
            }
        };

        // Extract import aliases
        let import_aliases: Vec<(String, String)> = program.imports.iter().filter_map(|imp| {
            if let ast::Decl::Import { path, alias, .. } = imp {
                if let Some(a) = alias {
                    let is_self_import = path.first().map(|s| s.as_str()) == Some("self");
                    let target = if is_self_import && path.len() >= 2 {
                        path.last().map(|s| s.to_string()).unwrap_or_default()
                    } else if is_self_import {
                        resolved.modules.iter()
                            .find(|(_, _, _, is_self)| *is_self)
                            .map(|(name, _, _, _)| name.clone())
                            .unwrap_or_else(|| path.iter().map(|s| s.as_str()).collect::<Vec<_>>().join("."))
                    } else {
                        path.iter().map(|s| s.as_str()).collect::<Vec<_>>().join(".")
                    };
                    Some((a.to_string(), target))
                } else if path.len() > 1 && path.first().map(|s| s.as_str()) != Some("self") {
                    let last = path.last().expect("path.len() > 1 checked above").to_string();
                    Some((last, path.iter().map(|s| s.as_str()).collect::<Vec<_>>().join(".")))
                } else {
                    None
                }
            } else {
                None
            }
        }).collect();

        let mut checker = check::Checker::new();
        checker.set_source(test_file, &source_text);
        for (name, mod_prog, pkg_id, is_self) in &resolved.modules {
            checker.register_module(name, mod_prog, pkg_id.as_ref(), *is_self);
        }
        for (alias, target) in &import_aliases {
            checker.register_alias(alias, target);
        }
        let diagnostics = checker.check_program(&mut program);
        if diagnostics.iter().any(|d| d.level == diagnostic::Level::Error) {
            eprintln!("SKIP {} (type errors)", test_file);
            for d in diagnostics.iter().filter(|d| d.level == diagnostic::Level::Error) {
                eprintln!("  {}", d.display_with_source(&source_text));
            }
            skipped += 1;
            continue;
        }

        let mut ir_program = almide::lower::lower_program(&program, &checker.expr_types, &checker.env);

        // Lower user modules to IR
        for (name, mod_prog, pkg_id, _) in &mut resolved.modules {
            if almide::stdlib::is_stdlib_module(name) { continue; }
            let mod_types = checker.check_module_bodies(mod_prog);
            let versioned = pkg_id.as_ref().map(|pid| pid.mod_name());
            let mod_ir_module = almide::lower::lower_module(name, mod_prog, &mod_types, &checker.env, versioned);
            ir_program.modules.push(mod_ir_module);
        }

        almide::optimize::optimize_program(&mut ir_program);
        almide::mono::monomorphize(&mut ir_program);

        let ts_code = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            match almide::codegen::codegen(&mut ir_program, almide::codegen::pass::Target::TypeScript) {
                almide::codegen::CodegenOutput::Source(s) => s,
                almide::codegen::CodegenOutput::Binary(_) => unreachable!(),
            }
        }));
        let ts_code = match ts_code {
            Ok(s) => s,
            Err(_) => {
                eprintln!("SKIP {} (TS codegen panic)", test_file);
                skipped += 1;
                continue;
            }
        };

        if let Err(e) = std::fs::write(&ts_path, &ts_code) {
            eprintln!("SKIP {} (write: {})", test_file, e);
            skipped += 1;
            continue;
        }

        // Run with deno or node
        let mut cmd = std::process::Command::new(runtime);
        for arg in &runtime_args {
            cmd.arg(arg);
        }
        cmd.arg(ts_path.to_str().unwrap());
        let output = cmd.output();

        match output {
            Ok(result) => {
                let stdout = String::from_utf8_lossy(&result.stdout);
                let stderr = String::from_utf8_lossy(&result.stderr);

                if result.status.success() {
                    // Count tests: for deno, count "ok" lines; for node, count successful executions
                    let test_count = if runtime == "deno" {
                        // Deno test output: "test_name ... ok"
                        stdout.lines().filter(|l| l.ends_with("... ok")).count()
                            .max(stderr.lines().filter(|l| l.ends_with("... ok")).count())
                    } else {
                        1
                    };
                    eprintln!("{}: {} tests passed", test_file, test_count);
                    passed += 1;
                } else {
                    eprintln!("FAIL {}", test_file);
                    // Show relevant error output
                    let error_output = if !stderr.is_empty() { &stderr } else { &stdout };
                    for line in error_output.lines().take(10) {
                        eprintln!("  {}", line);
                    }
                    failed += 1;
                }
            }
            Err(e) => {
                eprintln!("SKIP {} ({}: {})", test_file, runtime, e);
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
        let code = super::cmd_run_inner(test_file, &program_args, false, true);
        // Emit JSON per file
        let status = if code == 0 { "pass" } else { "fail" };
        println!(
            r#"{{"file":"{}","status":"{}","exit_code":{}}}"#,
            test_file.replace('"', r#"\""#), status, code
        );
    }
}

pub fn cmd_fmt(files: &[String], write_back: bool) {
    for file in files {
        let (mut program, _, _) = parse_file(file);
        // Auto-manage imports: add missing, remove unused
        let import_changes = fmt::auto_imports(&mut program);
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
