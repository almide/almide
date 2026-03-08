mod ast;
mod emit_rust;
mod emit_ts;
mod lexer;
mod parser;

use std::process::Command;

fn find_rustc() -> String {
    // Try PATH first
    if Command::new("rustc").arg("--version").output().is_ok() {
        return "rustc".to_string();
    }
    // Fallback: ~/.cargo/bin/rustc
    if let Some(home) = std::env::var_os("HOME") {
        let cargo_rustc = std::path::PathBuf::from(home).join(".cargo/bin/rustc");
        if cargo_rustc.exists() {
            return cargo_rustc.to_string_lossy().to_string();
        }
    }
    "rustc".to_string()
}

fn compile(file: &str) -> String {
    let input = std::fs::read_to_string(file)
        .unwrap_or_else(|e| { eprintln!("Error reading {}: {}", file, e); std::process::exit(1); });

    let program = if file.ends_with(".json") {
        serde_json::from_str(&input)
            .unwrap_or_else(|e| { eprintln!("JSON parse error: {}", e); std::process::exit(1); })
    } else {
        let tokens = lexer::Lexer::tokenize(&input);
        let mut parser = parser::Parser::new(tokens);
        parser.parse()
            .unwrap_or_else(|e| { eprintln!("Parse error: {}", e); std::process::exit(1); })
    };

    emit_rust::emit(&program)
}

fn cmd_run(file: &str, program_args: &[String]) {
    let rs_code = compile(file);

    let tmp_dir = std::env::temp_dir().join("almide-run");
    std::fs::create_dir_all(&tmp_dir).unwrap();

    let rs_path = tmp_dir.join("main.rs");
    let bin_path = tmp_dir.join("main");

    std::fs::write(&rs_path, &rs_code).unwrap();

    let rustc = Command::new(&find_rustc())
        .arg(&rs_path)
        .arg("-o")
        .arg(&bin_path)
        .arg("-C").arg("overflow-checks=no")
        .arg("-C").arg("opt-level=1")
        .output()
        .unwrap_or_else(|e| { eprintln!("Failed to run rustc: {}", e); std::process::exit(1); });

    if !rustc.status.success() {
        let stderr = String::from_utf8_lossy(&rustc.stderr);
        eprintln!("Compile error:\n{}", stderr);
        std::process::exit(1);
    }

    // Set larger stack size to avoid overflow with recursive code
    let status = Command::new(&bin_path)
        .env("RUST_MIN_STACK", "8388608")
        .args(program_args)
        .status()
        .unwrap_or_else(|e| { eprintln!("Failed to execute: {}", e); std::process::exit(1); });

    std::process::exit(status.code().unwrap_or(1));
}

fn main() {
    let args: Vec<String> = std::env::args().collect();

    // almide run file.almd [-- args...]
    if args.len() >= 3 && args[1] == "run" {
        let file = &args[2];
        let program_args: Vec<String> = if let Some(pos) = args.iter().position(|a| a == "--") {
            args[pos + 1..].to_vec()
        } else {
            args[3..].to_vec()
        };
        cmd_run(file, &program_args);
        return;
    }

    // almide build file.almd [-o output]
    if args.len() >= 3 && args[1] == "build" {
        let file = &args[2];
        let output = args.iter()
            .position(|a| a == "-o")
            .and_then(|i| args.get(i + 1))
            .map(|s| s.as_str())
            .unwrap_or_else(|| {
                // Default: strip .almd extension
                file.strip_suffix(".almd").unwrap_or("a.out")
            });

        let rs_code = compile(file);
        let tmp_rs = format!("{}.rs", output);
        std::fs::write(&tmp_rs, &rs_code).unwrap();

        let rustc = Command::new(&find_rustc())
            .arg(&tmp_rs)
            .arg("-o")
            .arg(output)
            .output()
            .unwrap_or_else(|e| { eprintln!("Failed to run rustc: {}", e); std::process::exit(1); });

        let _ = std::fs::remove_file(&tmp_rs);

        if !rustc.status.success() {
            let stderr = String::from_utf8_lossy(&rustc.stderr);
            eprintln!("Compile error:\n{}", stderr);
            std::process::exit(1);
        }

        eprintln!("Built {}", output);
        return;
    }

    // Legacy: almide file.almd [--target rust|ts] [--emit-ast]
    let files: Vec<&str> = args.iter().skip(1)
        .filter(|a| !a.starts_with("--"))
        .map(|s| s.as_str())
        .collect();

    if files.is_empty() {
        eprintln!("Usage: almide run <file.almd> [args...]");
        eprintln!("       almide build <file.almd> [-o output]");
        eprintln!("       almide <file.almd> [--target rust|ts] [--emit-ast]");
        std::process::exit(1);
    }

    let file = files[0];
    let emit_ast = args.iter().any(|a| a == "--emit-ast");

    let target = args.iter()
        .position(|a| a == "--target")
        .and_then(|i| args.get(i + 1))
        .map(|s| s.as_str())
        .unwrap_or("rust");

    let input = std::fs::read_to_string(file)
        .unwrap_or_else(|e| { eprintln!("Error reading {}: {}", file, e); std::process::exit(1); });

    let program = if file.ends_with(".json") {
        serde_json::from_str(&input)
            .unwrap_or_else(|e| { eprintln!("JSON parse error: {}", e); std::process::exit(1); })
    } else {
        let tokens = lexer::Lexer::tokenize(&input);
        let mut parser = parser::Parser::new(tokens);
        parser.parse()
            .unwrap_or_else(|e| { eprintln!("Parse error: {}", e); std::process::exit(1); })
    };

    if emit_ast {
        let json = serde_json::to_string_pretty(&program)
            .unwrap_or_else(|e| { eprintln!("JSON serialize error: {}", e); std::process::exit(1); });
        println!("{}", json);
    } else {
        let code = match target {
            "rust" | "rs" => emit_rust::emit(&program),
            "ts" | "typescript" => emit_ts::emit(&program),
            other => { eprintln!("Unknown target: {}. Use rust or ts.", other); std::process::exit(1); }
        };
        print!("{}", code);
    }
}
