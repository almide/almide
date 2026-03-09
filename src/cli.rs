/// CLI command implementations.

use std::process::Command;
use crate::{compile, compile_with_options, parse_file, find_rustc, emit_rust, emit_ts, check, diagnostic, resolve, fmt, project};

pub fn cmd_run_inner(file: &str, program_args: &[String], no_check: bool) -> i32 {
    let rs_code = compile(file, no_check);

    let tmp_dir = std::env::temp_dir().join("almide-run");
    if let Err(e) = std::fs::create_dir_all(&tmp_dir) {
        eprintln!("Failed to create temp directory {}: {}", tmp_dir.display(), e);
        std::process::exit(1);
    }

    let rs_path = tmp_dir.join("main.rs");
    let bin_path = tmp_dir.join("main");

    if let Err(e) = std::fs::write(&rs_path, &rs_code) {
        eprintln!("Failed to write {}: {}", rs_path.display(), e);
        std::process::exit(1);
    }

    // Detect test-only files (no main function)
    let is_test_only = !rs_code.contains("\nfn almide_main(") && !rs_code.contains("\nfn main(");

    let mut rustc_cmd = Command::new(&find_rustc());
    rustc_cmd.arg(&rs_path)
        .arg("-o")
        .arg(&bin_path)
        .arg("-C").arg("overflow-checks=no")
        .arg("-C").arg("opt-level=1")
        .arg("--edition").arg("2021");
    if is_test_only {
        rustc_cmd.arg("--test");
    }
    let rustc = rustc_cmd.output()
        .unwrap_or_else(|e| { eprintln!("Failed to run rustc: {}", e); std::process::exit(1); });

    if !rustc.status.success() {
        let stderr = String::from_utf8_lossy(&rustc.stderr);
        eprintln!("Compile error:\n{}", stderr);
        return 1;
    }

    let status = Command::new(&bin_path)
        .env("RUST_MIN_STACK", "8388608")
        .args(program_args)
        .status()
        .unwrap_or_else(|e| { eprintln!("Failed to execute: {}", e); std::process::exit(1); });

    status.code().unwrap_or(1)
}

pub fn cmd_run(file: &str, program_args: &[String], no_check: bool) {
    std::process::exit(cmd_run_inner(file, program_args, no_check));
}

pub fn cmd_init() {
    if std::path::Path::new("almide.toml").exists() {
        eprintln!("almide.toml already exists");
        std::process::exit(1);
    }
    let dir_name = std::env::current_dir()
        .ok()
        .and_then(|p| p.file_name().map(|n| n.to_string_lossy().to_string()))
        .unwrap_or_else(|| "myapp".to_string());

    let toml = format!("[package]\nname = \"{}\"\nversion = \"0.1.0\"\n", dir_name);

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

    eprintln!("Initialized project in ./");
    eprintln!("  almide.toml");
    eprintln!("  src/main.almd");
    eprintln!("  tests/");
}

pub fn cmd_test(file: &str, no_check: bool, run_filter: Option<&str>) {
    let test_files: Vec<String> = if !file.is_empty() {
        vec![file.to_string()]
    } else if std::path::Path::new("tests").is_dir() {
        let mut files: Vec<String> = std::fs::read_dir("tests")
            .unwrap_or_else(|e| { eprintln!("Failed to read tests/: {}", e); std::process::exit(1); })
            .filter_map(|e| e.ok())
            .map(|e| e.path().to_string_lossy().to_string())
            .filter(|f| f.ends_with(".almd"))
            .collect();
        files.sort();
        files
    } else {
        eprintln!("No test files found. Create tests/*.almd or specify a file.");
        std::process::exit(1);
    };

    if test_files.is_empty() {
        eprintln!("No test files found in tests/");
        std::process::exit(1);
    }

    let mut program_args: Vec<String> = Vec::new();
    if let Some(filter) = run_filter {
        // Pass filter to rustc test binary
        program_args.push(filter.to_string());
    }

    let mut failed = 0;
    for test_file in &test_files {
        eprintln!("Running {}", test_file);
        let code = cmd_run_inner(test_file, &program_args, no_check);
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

pub fn cmd_build(args: &[String], no_check: bool) {
    let file = if args.len() >= 3 && !args[2].starts_with('-') {
        args[2].clone()
    } else if std::path::Path::new("almide.toml").exists() && std::path::Path::new("src/main.almd").exists() {
        "src/main.almd".to_string()
    } else {
        eprintln!("No file specified and no almide.toml found.");
        eprintln!("Run 'almide init' to create a project, or specify a file: almide build <file.almd>");
        std::process::exit(1);
    };
    let build_target = args.iter()
        .position(|a| a == "--target")
        .and_then(|i| args.get(i + 1))
        .map(|s| s.as_str());

    let is_wasm = matches!(build_target, Some("wasm" | "wasm32" | "wasi"));

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
    let output = args.iter()
        .position(|a| a == "-o")
        .and_then(|i| args.get(i + 1))
        .map(|s| s.to_string())
        .unwrap_or(default_output);

    let emit_options = emit_rust::EmitOptions { no_thread_wrap: is_wasm };
    let rs_code = compile_with_options(&file, no_check, &emit_options);

    let tmp_rs = format!("{}.rs", output.strip_suffix(".wasm").unwrap_or(&output));
    if let Err(e) = std::fs::write(&tmp_rs, &rs_code) {
        eprintln!("Failed to write {}: {}", tmp_rs, e);
        std::process::exit(1);
    }

    let is_release = args.iter().any(|a| a == "--release");

    let mut rustc_cmd = Command::new(&find_rustc());
    rustc_cmd.arg(&tmp_rs)
        .arg("-o")
        .arg(&output)
        .arg("-C").arg("overflow-checks=no")
        .arg("--edition").arg("2021");

    if is_wasm {
        rustc_cmd.arg("--target").arg("wasm32-wasip1")
            .arg("-C").arg("opt-level=s")
            .arg("-C").arg("lto=yes");
    } else if is_release {
        rustc_cmd.arg("-C").arg("opt-level=2");
    }

    let rustc = rustc_cmd.output()
        .unwrap_or_else(|e| { eprintln!("Failed to run rustc: {}", e); std::process::exit(1); });

    let _ = std::fs::remove_file(&tmp_rs);

    if !rustc.status.success() {
        let stderr = String::from_utf8_lossy(&rustc.stderr);
        eprintln!("Compile error:\n{}", stderr);
        std::process::exit(1);
    }

    eprintln!("Built {}", output);
}

pub fn cmd_emit(file: &str, target: &str, emit_ast: bool, no_check: bool) {
    let mut program = parse_file(file);
    let source_text = std::fs::read_to_string(file).unwrap_or_default();

    let dep_paths: Vec<(project::PkgId, std::path::PathBuf)> = if std::path::Path::new("almide.toml").exists() {
        if let Ok(proj) = project::parse_toml(std::path::Path::new("almide.toml")) {
            project::fetch_all_deps(&proj)
                .unwrap_or_else(|e| { eprintln!("{}", e); std::process::exit(1); })
                .into_iter()
                .map(|fd| (fd.pkg_id, fd.source_dir))
                .collect()
        } else {
            vec![]
        }
    } else {
        vec![]
    };

    let resolved = resolve::resolve_imports_with_deps(file, &program, &dep_paths)
        .unwrap_or_else(|e| { eprintln!("{}", e); std::process::exit(1); });

    // Extract user-level import aliases (import pkg as alias, or implicit aliases for multi-segment imports)
    let import_aliases: Vec<(String, String)> = program.imports.iter().filter_map(|imp| {
        if let crate::ast::Decl::Import { path, alias, .. } = imp {
            if let Some(a) = alias {
                Some((a.clone(), path.join(".")))
            } else if path.len() > 1 && path.first().map(|s| s.as_str()) != Some("self") {
                let last = path.last().unwrap().clone();
                Some((last, path.join(".")))
            } else {
                None
            }
        } else {
            None
        }
    }).collect();

    if !no_check && !emit_ast {
        let mut checker = check::Checker::new();
        checker.set_source(file, &source_text);
        for (name, mod_prog, pkg_id, is_self) in &resolved.modules {
            checker.register_module(name, mod_prog, pkg_id.as_ref(), *is_self);
        }
        for (alias, target) in &import_aliases {
            checker.register_alias(alias, target);
        }
        let diagnostics = checker.check_program(&mut program);
        let errors: Vec<_> = diagnostics.iter()
            .filter(|d| d.level == diagnostic::Level::Error)
            .collect();
        if !errors.is_empty() {
            for d in &errors {
                eprintln!("{}", d.display_with_source(&source_text));
            }
            eprintln!("\n{} error(s) found", errors.len());
            std::process::exit(1);
        }
        for d in diagnostics.iter().filter(|d| d.level == diagnostic::Level::Warning) {
            eprintln!("{}", d.display_with_source(&source_text));
        }
    }

    if emit_ast {
        let json = serde_json::to_string_pretty(&program)
            .unwrap_or_else(|e| { eprintln!("JSON serialize error: {}", e); std::process::exit(1); });
        println!("{}", json);
    } else {
        let code = match target {
            "rust" | "rs" => emit_rust::emit_with_options(&program, &resolved.modules, &emit_rust::EmitOptions::default(), &import_aliases),
            "ts" | "typescript" => {
                // Convert to legacy format for emit_ts (no PkgId support)
                let legacy_modules: Vec<(String, crate::ast::Program)> = resolved.modules.iter()
                    .map(|(n, p, _, _)| (n.clone(), p.clone()))
                    .collect();
                emit_ts::emit_with_modules(&program, &legacy_modules)
            }
            "js" | "javascript" => {
                let legacy_modules: Vec<(String, crate::ast::Program)> = resolved.modules.iter()
                    .map(|(n, p, _, _)| (n.clone(), p.clone()))
                    .collect();
                emit_ts::emit_js_with_modules(&program, &legacy_modules)
            }
            other => { eprintln!("Unknown target: {}. Use rust, ts, or js.", other); std::process::exit(1); }
        };
        print!("{}", code);
    }
}

pub fn cmd_check(file: &str) {
    let mut program = parse_file(file);
    let source_text = std::fs::read_to_string(file).unwrap_or_default();

    let dep_paths: Vec<(project::PkgId, std::path::PathBuf)> = if std::path::Path::new("almide.toml").exists() {
        if let Ok(proj) = project::parse_toml(std::path::Path::new("almide.toml")) {
            project::fetch_all_deps(&proj)
                .unwrap_or_else(|e| { eprintln!("{}", e); std::process::exit(1); })
                .into_iter()
                .map(|fd| (fd.pkg_id, fd.source_dir))
                .collect()
        } else {
            vec![]
        }
    } else {
        vec![]
    };

    let resolved = resolve::resolve_imports_with_deps(file, &program, &dep_paths)
        .unwrap_or_else(|e| { eprintln!("{}", e); std::process::exit(1); });

    let mut checker = check::Checker::new();
    checker.set_source(file, &source_text);
    for (name, mod_prog, pkg_id, is_self) in &resolved.modules {
        checker.register_module(name, mod_prog, pkg_id.as_ref(), *is_self);
    }
    let diagnostics = checker.check_program(&mut program);

    for d in diagnostics.iter().filter(|d| d.level == diagnostic::Level::Warning) {
        eprintln!("{}", d.display_with_source(&source_text));
    }

    let errors: Vec<_> = diagnostics.iter()
        .filter(|d| d.level == diagnostic::Level::Error)
        .collect();
    if !errors.is_empty() {
        for d in &errors {
            eprintln!("{}", d.display_with_source(&source_text));
        }
        eprintln!("\n{} error(s) found", errors.len());
        std::process::exit(1);
    }

    eprintln!("No errors found");
}

pub fn cmd_fmt(files: &[String], write_back: bool) {
    for file in files {
        let program = parse_file(file);
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
    let cache = project::cache_dir();
    if cache.exists() {
        std::fs::remove_dir_all(&cache)
            .unwrap_or_else(|e| { eprintln!("Failed to clean cache: {}", e); std::process::exit(1); });
        eprintln!("Cleaned {}", cache.display());
    } else {
        eprintln!("Cache directory does not exist: {}", cache.display());
    }
}
