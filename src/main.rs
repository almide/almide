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
        let prog = parser.parse()
            .unwrap_or_else(|e| { eprintln!("Parse error: {}", e); std::process::exit(1); });
        if !parser.errors.is_empty() {
            for e in &parser.errors {
                eprintln!("Parse error: {}", e);
            }
            std::process::exit(1);
        }
        prog
    }
}

fn compile(file: &str, no_check: bool) -> String {
    compile_with_options(file, no_check, &emit_rust::EmitOptions::default(), None)
}

fn compile_with_options(file: &str, no_check: bool, emit_options: &emit_rust::EmitOptions, build_target: Option<&str>) -> String {
    let mut program = parse_file(file);

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
        if let ast::Decl::Import { path, alias, .. } = imp {
            if let Some(a) = alias {
                // Explicit alias: import pkg as alias
                Some((a.clone(), path.join(".")))
            } else if path.len() > 1 && path.first().map(|s| s.as_str()) != Some("self") {
                // Implicit alias: import pkg.sub → "sub" maps to "pkg.sub"
                let last = path.last().unwrap().clone();
                Some((last, path.join(".")))
            } else {
                None
            }
        } else {
            None
        }
    }).collect();

    if !no_check {
        let source_text = std::fs::read_to_string(file).unwrap_or_default();
        let mut checker = check::Checker::new();
        checker.set_source(file, &source_text);
        if let Some(t) = build_target {
            checker.set_target(t);
        }
        for (name, mod_prog, pkg_id, is_self) in &resolved.modules {
            checker.register_module(name, mod_prog, pkg_id.as_ref(), *is_self);
        }
        // Register user-level import aliases
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

    emit_rust::emit_with_options(&program, &resolved.modules, emit_options, &import_aliases)
}

fn collect_almd_files(dir: &std::path::Path, out: &mut Vec<String>) {
    if let Ok(entries) = std::fs::read_dir(dir) {
        let mut entries: Vec<_> = entries.filter_map(|e| e.ok()).collect();
        entries.sort_by_key(|e| e.path());
        for entry in entries {
            let path = entry.path();
            if path.is_dir() {
                collect_almd_files(&path, out);
            } else if path.extension().map_or(false, |ext| ext == "almd") {
                out.push(path.to_string_lossy().to_string());
            }
        }
    }
}

fn print_help() {
    println!("almide {}", env!("CARGO_PKG_VERSION"));
    println!();
    println!("Usage: almide <command> [options]");
    println!();
    println!("Commands:");
    println!("  init                        Create a new Almide project");
    println!("  run [file.almd] [args...]   Compile and execute (default: src/main.almd)");
    println!("  build [file.almd] [opts]    Build a binary");
    println!("  test [file.almd] [-run pat] Run tests");
    println!("  check [file.almd]           Type check only");
    println!("  fmt [files...] [--check]    Format source files (default: src/**/*.almd)");
    println!("  clean                       Clear dependency cache");
    println!("  add <pkg> [--git url]       Add a dependency");
    println!("  deps                        List dependencies");
    println!();
    println!("Build options:");
    println!("  -o <output>                 Output file name");
    println!("  --target wasm               Build for WebAssembly");
    println!("  --release                   Optimize for performance (opt-level=2)");
    println!("  --no-check                  Skip type checking");
    println!();
    println!("Emit options:");
    println!("  almide <file> --target rust  Emit Rust source");
    println!("  almide <file> --target ts    Emit TypeScript source");
    println!("  almide <file> --emit-ast     Emit AST as JSON");
    println!();
    println!("Examples:");
    println!("  almide init");
    println!("  almide run");
    println!("  almide run hello.almd");
    println!("  almide build --release");
    println!("  almide build app.almd --target wasm -o app.wasm");
    println!("  almide test -run \"parser\"");
    println!("  almide fmt");
    println!("  almide fmt src/main.almd --check");
}

fn main() {
    let args: Vec<String> = std::env::args().collect();

    if args.len() >= 2 && (args[1] == "--version" || args[1] == "-V") {
        println!("almide {}", env!("CARGO_PKG_VERSION"));
        return;
    }

    if args.len() >= 2 && (args[1] == "--help" || args[1] == "-h" || args[1] == "help") {
        print_help();
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
        let mut file = "";
        let mut run_filter = None;
        let mut i = 2;
        while i < args.len() {
            if args[i] == "-run" || args[i] == "--run" {
                if i + 1 < args.len() {
                    run_filter = Some(args[i + 1].clone());
                    i += 2;
                    continue;
                }
            } else if !args[i].starts_with('-') && file.is_empty() {
                file = &args[i];
            }
            i += 1;
        }
        cli::cmd_test(file, no_check, run_filter.as_deref());
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
        let write_back = !args.iter().any(|a| a == "--check" || a == "--dry-run");
        let fmt_files: Vec<String> = args.iter().skip(2)
            .filter(|a| !a.starts_with("--"))
            .cloned()
            .collect();
        if fmt_files.is_empty() {
            // No files specified — format all src/**/*.almd recursively
            let mut files = Vec::new();
            if std::path::Path::new("src").is_dir() {
                collect_almd_files(std::path::Path::new("src"), &mut files);
            }
            if files.is_empty() {
                eprintln!("No .almd files found in src/");
                std::process::exit(1);
            }
            cli::cmd_fmt(&files, write_back);
        } else {
            cli::cmd_fmt(&fmt_files, write_back);
        }
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
        eprintln!("Usage: almide <command> [options]");
        eprintln!();
        eprintln!("Run 'almide --help' for detailed usage.");
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
