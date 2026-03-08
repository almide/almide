mod ast;
mod check;
mod diagnostic;
mod emit_rust;
mod emit_ts;
mod emit_ts_runtime;
mod lexer;
mod parser;
mod project;
mod resolve;
mod stdlib;
mod types;

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

fn parse_file(file: &str) -> ast::Program {
    let input = std::fs::read_to_string(file)
        .unwrap_or_else(|e| { eprintln!("Error reading {}: {}", file, e); std::process::exit(1); });

    if file.ends_with(".json") {
        serde_json::from_str(&input)
            .unwrap_or_else(|e| { eprintln!("JSON parse error: {}", e); std::process::exit(1); })
    } else {
        let tokens = lexer::Lexer::tokenize(&input);
        let mut parser = parser::Parser::new(tokens);
        parser.parse()
            .unwrap_or_else(|e| { eprintln!("Parse error: {}", e); std::process::exit(1); })
    }
}

fn compile(file: &str, no_check: bool) -> String {
    let program = parse_file(file);

    // Fetch dependencies from almide.toml if present
    let dep_paths: Vec<(String, std::path::PathBuf)> = if std::path::Path::new("almide.toml").exists() {
        if let Ok(proj) = project::parse_toml(std::path::Path::new("almide.toml")) {
            project::fetch_all_deps(&proj)
                .unwrap_or_else(|e| { eprintln!("{}", e); std::process::exit(1); })
                .into_iter().collect()
        } else {
            vec![]
        }
    } else {
        vec![]
    };

    // Resolve imported modules (local + dependencies)
    let resolved = resolve::resolve_imports_with_deps(file, &program, &dep_paths)
        .unwrap_or_else(|e| { eprintln!("{}", e); std::process::exit(1); });

    // Type check (unless --no-check)
    if !no_check {
        let mut checker = check::Checker::new();
        // Register imported modules first
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

    emit_rust::emit(&program, &resolved.modules)
}

fn cmd_run_inner(file: &str, program_args: &[String], no_check: bool) -> i32 {
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

fn cmd_run(file: &str, program_args: &[String], no_check: bool) {
    std::process::exit(cmd_run_inner(file, program_args, no_check));
}

fn cmd_init() {
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

fn cmd_test(file: &str, no_check: bool) {
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

fn main() {
    let args: Vec<String> = std::env::args().collect();

    if args.len() >= 2 && (args[1] == "--version" || args[1] == "-V") {
        println!("almide {}", env!("CARGO_PKG_VERSION"));
        return;
    }

    let no_check = args.iter().any(|a| a == "--no-check");

    // almide add <spec>
    // Examples:
    //   almide add fizzbuzz                           → github.com/almide/fizzbuzz
    //   almide add O6lvl4/almide-fizzbuzz@v0.1.0      → github.com/O6lvl4/almide-fizzbuzz, tag v0.1.0
    //   almide add github.com/org/repo                → full URL
    //   almide add gitlab.com/org/repo                → other hosts
    //   almide add fizzbuzz --git <url> [--tag <tag>]  → explicit (legacy)
    if args.len() >= 3 && args[1] == "add" {
        let spec = &args[2];
        let (name, git_url, tag) = if args.iter().any(|a| a == "--git") {
            // Legacy explicit mode
            let git = args.iter().position(|a| a == "--git")
                .and_then(|i| args.get(i + 1))
                .unwrap_or_else(|| { eprintln!("--git requires a URL"); std::process::exit(1); });
            let tag = args.iter().position(|a| a == "--tag")
                .and_then(|i| args.get(i + 1))
                .map(|s| s.to_string());
            (spec.to_string(), git.to_string(), tag)
        } else {
            project::resolve_package_spec(spec)
        };
        project::add_dep_to_toml(&name, &git_url, tag.as_deref())
            .unwrap_or_else(|e| { eprintln!("{}", e); std::process::exit(1); });
        let dep = project::Dependency {
            name: name.clone(),
            git: git_url,
            tag,
            branch: None,
        };
        project::fetch_dep(&dep)
            .unwrap_or_else(|e| { eprintln!("{}", e); std::process::exit(1); });
        return;
    }

    // almide deps
    if args.len() >= 2 && args[1] == "deps" {
        if std::path::Path::new("almide.toml").exists() {
            let proj = project::parse_toml(std::path::Path::new("almide.toml"))
                .unwrap_or_else(|e| { eprintln!("{}", e); std::process::exit(1); });
            if proj.dependencies.is_empty() {
                println!("No dependencies");
            } else {
                for dep in &proj.dependencies {
                    let ref_name = dep.tag.as_deref().or(dep.branch.as_deref()).unwrap_or("main");
                    println!("{} = {} ({})", dep.name, dep.git, ref_name);
                }
            }
        } else {
            eprintln!("No almide.toml found");
        }
        return;
    }

    // almide init
    if args.len() >= 2 && args[1] == "init" {
        cmd_init();
        return;
    }

    // almide test [file.almd]
    if args.len() >= 2 && args[1] == "test" {
        let file = if args.len() >= 3 { &args[2] } else { "" };
        cmd_test(file, no_check);
        return;
    }

    // almide run [file.almd] [-- args...]
    if args.len() >= 2 && args[1] == "run" {
        let (file, arg_start) = if args.len() >= 3 && !args[2].starts_with('-') {
            (args[2].clone(), 3)
        } else if std::path::Path::new("almide.toml").exists() && std::path::Path::new("src/main.almd").exists() {
            ("src/main.almd".to_string(), 2)
        } else {
            eprintln!("No file specified and no almide.toml found.");
            eprintln!("Run 'almide init' to create a project, or specify a file: almide run <file.almd>");
            std::process::exit(1);
        };
        let program_args: Vec<String> = if let Some(pos) = args.iter().position(|a| a == "--") {
            args[pos + 1..].to_vec()
        } else {
            args[arg_start..].iter().filter(|a| a.as_str() != "--no-check").cloned().collect()
        };
        cmd_run(&file, &program_args, no_check);
        return;
    }

    // almide build [file.almd] [-o output] [--target wasm]
    if args.len() >= 2 && args[1] == "build" {
        let file = if args.len() >= 3 && !args[2].starts_with('-') {
            args[2].clone()
        } else {
            // Look for almide.toml → src/main.almd
            if std::path::Path::new("almide.toml").exists() && std::path::Path::new("src/main.almd").exists() {
                "src/main.almd".to_string()
            } else {
                eprintln!("No file specified and no almide.toml found.");
                eprintln!("Run 'almide init' to create a project, or specify a file: almide build <file.almd>");
                std::process::exit(1);
            }
        };
        let file = &file;
        let build_target = args.iter()
            .position(|a| a == "--target")
            .and_then(|i| args.get(i + 1))
            .map(|s| s.as_str());

        let is_wasm = matches!(build_target, Some("wasm" | "wasm32" | "wasi"));

        let default_output = if is_wasm {
            format!("{}.wasm", file.strip_suffix(".almd").unwrap_or("a.out"))
        } else if std::path::Path::new("almide.toml").exists() {
            // Read package name from almide.toml
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

        let mut rs_code = compile(file, no_check);

        // WASM: replace thread-based main with direct main (threads unsupported in WASI)
        if is_wasm {
            // Replace the thread::Builder wrapper with a direct call
            let thread_main = "fn main() {\n    let t = std::thread::Builder::new().stack_size(8 * 1024 * 1024).spawn(|| {";
            let thread_end = "    }).unwrap();\n    t.join().unwrap();\n}";
            if rs_code.contains(thread_main) {
                // Extract the body between spawn(|| { ... }).unwrap()
                if let Some(start) = rs_code.find(thread_main) {
                    if let Some(end) = rs_code.find(thread_end) {
                        let body = &rs_code[start + thread_main.len()..end];
                        // De-indent body by one level (remove leading 8 spaces → 4 spaces)
                        let body_lines: Vec<String> = body.lines()
                            .map(|l| if l.starts_with("        ") { l[4..].to_string() } else { l.to_string() })
                            .collect();
                        let new_main = format!("fn main() {{\n{}}}", body_lines.join("\n"));
                        rs_code = format!("{}{}", &rs_code[..start], new_main);
                    }
                }
            }
        }

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
        return;
    }

    // Legacy: almide file.almd [--target rust|ts] [--emit-ast]
    let files: Vec<&str> = args.iter().skip(1)
        .filter(|a| !a.starts_with("--"))
        .map(|s| s.as_str())
        .collect();

    if files.is_empty() {
        eprintln!("Usage: almide init");
        eprintln!("       almide run <file.almd> [args...]");
        eprintln!("       almide build <file.almd> [-o output] [--target wasm]");
        eprintln!("       almide test [file.almd]");
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

    let program = parse_file(file);

    // Resolve imports
    let resolved = resolve::resolve_imports(file, &program)
        .unwrap_or_else(|e| { eprintln!("{}", e); std::process::exit(1); });

    // Type check (unless --no-check or --emit-ast)
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
