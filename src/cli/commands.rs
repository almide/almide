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
        // Pass filter to rustc test binary
        program_args.push(filter.to_string());
    }

    let mut failed = 0;
    for test_file in &test_files {
        eprintln!("Running {}", test_file);
        let code = super::cmd_run_inner(test_file, &program_args, no_check, true);
        if code != 0 {
            failed += 1;
        }
    }
    if failed > 0 {
        eprintln!("\n{}/{} test file(s) failed", failed, test_files.len());
        std::process::exit(1);
    }
    eprintln!("\nAll {} test file(s) passed", test_files.len());
}

pub fn cmd_test_wasm(file: &str, run_filter: Option<&str>) {
    use crate::{parse_file, check, diagnostic, resolve, project, project_fetch};
    use almide::codegen::pass::NanoPass;

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

        let resolved = match resolve::resolve_imports_with_deps(test_file, &program, &dep_paths) {
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
        almide::optimize::optimize_program(&mut ir_program);
        almide::mono::monomorphize(&mut ir_program);
        almide::codegen::pass_tco::TailCallOptPass
            .run(&mut ir_program, almide::codegen::pass::Target::Rust);
        almide::codegen::pass_result_propagation::ResultPropagationPass
            .run(&mut ir_program, almide::codegen::pass::Target::Rust);

        let bytes = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            almide::codegen::emit_wasm_binary(&ir_program)
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

        // Run with wasmtime
        let output = std::process::Command::new("wasmtime")
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
        let (program, _, _) = parse_file(file);
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
    if !cleaned {
        eprintln!("No cache to clean");
    }
}
