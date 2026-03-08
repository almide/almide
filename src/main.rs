// Re-export library modules (shared with playground WASM crate)
pub use almide::ast;
pub use almide::diagnostic;
pub use almide::emit_common;
pub use almide::emit_ts;
pub use almide::emit_ts_runtime;
pub use almide::fmt;
pub use almide::lexer;
pub use almide::parser;
pub use almide::stdlib;
pub use almide::types;

// CLI-only modules
mod check;
mod cli;
mod emit_rust;
mod project;
mod resolve;

use std::process::Command;

fn find_rustc() -> String {
    if Command::new("rustc").arg("--version").output().is_ok() {
        return "rustc".to_string();
    }
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
    compile_with_options(file, no_check, &emit_rust::EmitOptions::default())
}

fn compile_with_options(file: &str, no_check: bool, emit_options: &emit_rust::EmitOptions) -> String {
    let program = parse_file(file);

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

    if !no_check {
        let source_text = std::fs::read_to_string(file).unwrap_or_default();
        let mut checker = check::Checker::new();
        checker.set_source(file, &source_text);
        for (name, mod_prog, pkg_id) in &resolved.modules {
            checker.register_module(name, mod_prog, pkg_id.as_ref());
        }
        let diagnostics = checker.check_program(&program);
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

    emit_rust::emit_with_options(&program, &resolved.modules, emit_options)
}

fn main() {
    let args: Vec<String> = std::env::args().collect();

    if args.len() >= 2 && (args[1] == "--version" || args[1] == "-V") {
        println!("almide {}", env!("CARGO_PKG_VERSION"));
        return;
    }

    let no_check = args.iter().any(|a| a == "--no-check");

    if args.len() >= 3 && args[1] == "add" {
        let spec = &args[2];
        let (name, git_url, tag) = if args.iter().any(|a| a == "--git") {
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
            version: None,
        };
        project::fetch_dep(&dep)
            .unwrap_or_else(|e| { eprintln!("{}", e); std::process::exit(1); });
        return;
    }

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

    if args.len() >= 2 && args[1] == "init" {
        cli::cmd_init();
        return;
    }

    if args.len() >= 2 && args[1] == "test" {
        let file = if args.len() >= 3 { &args[2] } else { "" };
        cli::cmd_test(file, no_check);
        return;
    }

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
            // Filter out compiler flags that shouldn't be passed to the program
            let mut filtered = Vec::new();
            let mut skip_next = false;
            for a in &args[arg_start..] {
                if skip_next { skip_next = false; continue; }
                if a == "--no-check" { continue; }
                if a == "--target" || a == "-o" { skip_next = true; continue; }
                if a.starts_with("--target=") || a.starts_with("-o=") { continue; }
                filtered.push(a.clone());
            }
            filtered
        };
        cli::cmd_run(&file, &program_args, no_check);
        return;
    }

    if args.len() >= 2 && args[1] == "build" {
        cli::cmd_build(&args, no_check);
        return;
    }

    if args.len() >= 2 && args[1] == "check" {
        let file = if args.len() >= 3 && !args[2].starts_with('-') {
            args[2].clone()
        } else if std::path::Path::new("almide.toml").exists() && std::path::Path::new("src/main.almd").exists() {
            "src/main.almd".to_string()
        } else {
            eprintln!("No file specified and no almide.toml found.");
            std::process::exit(1);
        };
        cli::cmd_check(&file);
        return;
    }

    if args.len() >= 2 && args[1] == "fmt" {
        let write_back = !args.iter().any(|a| a == "--dry-run");
        let fmt_files: Vec<String> = args.iter().skip(2)
            .filter(|a| !a.starts_with("--"))
            .cloned()
            .collect();
        if fmt_files.is_empty() {
            eprintln!("Usage: almide fmt <file.almd> [files...] [--dry-run]");
            std::process::exit(1);
        }
        cli::cmd_fmt(&fmt_files, write_back);
        return;
    }

    if args.len() >= 2 && args[1] == "clean" {
        cli::cmd_clean();
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
        eprintln!("       almide check [file.almd]");
        eprintln!("       almide fmt <file.almd> [--dry-run]");
        eprintln!("       almide clean");
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

    cli::cmd_emit(file, target, emit_ast, no_check);
}
