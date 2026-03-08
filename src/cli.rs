/// CLI command implementations.

use std::process::Command;
use crate::{compile, compile_with_options, parse_file, find_rustc, emit_rust, emit_ts, check, diagnostic, resolve};

pub fn cmd_run_inner(file: &str, program_args: &[String], no_check: bool) -> i32 {
    let rs_code = compile(file, no_check);

    let tmp_dir = std::env::temp_dir().join("almide-run");
    std::fs::create_dir_all(&tmp_dir).unwrap();

    let rs_path = tmp_dir.join("main.rs");
    let bin_path = tmp_dir.join("main");

    std::fs::write(&rs_path, &rs_code).unwrap();

    // Detect test-only files (no main function)
    let is_test_only = !rs_code.contains("\nfn almide_main(") && !rs_code.contains("\nfn main(");

    let mut rustc_cmd = Command::new(&find_rustc());
    rustc_cmd.arg(&rs_path)
        .arg("-o")
        .arg(&bin_path)
        .arg("-C").arg("overflow-checks=no")
        .arg("-C").arg("opt-level=1");
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

    std::fs::write("almide.toml", toml).unwrap();
    std::fs::create_dir_all("src").unwrap();
    std::fs::create_dir_all("tests").unwrap();

    if !std::path::Path::new("src/main.almd").exists() {
        std::fs::write("src/main.almd", "module main\n\neffect fn main(args: List[String]) -> Result[Unit, String] = {\n  println(\"Hello, Almide!\")\n  ok(())\n}\n").unwrap();
    }

    eprintln!("Initialized project in ./");
    eprintln!("  almide.toml");
    eprintln!("  src/main.almd");
    eprintln!("  tests/");
}

pub fn cmd_test(file: &str, no_check: bool) {
    let test_files: Vec<String> = if !file.is_empty() {
        vec![file.to_string()]
    } else if std::path::Path::new("tests").is_dir() {
        let mut files: Vec<String> = std::fs::read_dir("tests")
            .unwrap()
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

    let mut failed = 0;
    for test_file in &test_files {
        eprintln!("Running {}", test_file);
        let code = cmd_run_inner(test_file, &[], no_check);
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
    std::fs::write(&tmp_rs, &rs_code).unwrap();

    let mut rustc_cmd = Command::new(&find_rustc());
    rustc_cmd.arg(&tmp_rs)
        .arg("-o")
        .arg(&output)
        .arg("-C").arg("overflow-checks=no");

    if is_wasm {
        rustc_cmd.arg("--target").arg("wasm32-wasip1")
            .arg("-C").arg("opt-level=s")
            .arg("-C").arg("lto=yes");
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
    let program = parse_file(file);

    let resolved = resolve::resolve_imports(file, &program)
        .unwrap_or_else(|e| { eprintln!("{}", e); std::process::exit(1); });

    if !no_check && !emit_ast {
        let mut checker = check::Checker::new();
        for (name, mod_prog) in &resolved.modules {
            checker.register_module(name, mod_prog);
        }
        let diagnostics = checker.check_program(&program);
        let errors: Vec<_> = diagnostics.iter()
            .filter(|d| d.level == diagnostic::Level::Error)
            .collect();
        if !errors.is_empty() {
            for d in &errors {
                eprintln!("{}", d.display());
            }
            eprintln!("\n{} error(s) found", errors.len());
            std::process::exit(1);
        }
        for d in diagnostics.iter().filter(|d| d.level == diagnostic::Level::Warning) {
            eprintln!("{}", d.display());
        }
    }

    if emit_ast {
        let json = serde_json::to_string_pretty(&program)
            .unwrap_or_else(|e| { eprintln!("JSON serialize error: {}", e); std::process::exit(1); });
        println!("{}", json);
    } else {
        let code = match target {
            "rust" | "rs" => emit_rust::emit(&program, &resolved.modules),
            "ts" | "typescript" => emit_ts::emit_with_modules(&program, &resolved.modules),
            "js" | "javascript" => emit_ts::emit_js_with_modules(&program, &resolved.modules),
            other => { eprintln!("Unknown target: {}. Use rust, ts, or js.", other); std::process::exit(1); }
        };
        print!("{}", code);
    }
}
