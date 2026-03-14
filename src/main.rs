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
use clap::{Parser, Subcommand};

#[derive(Parser)]
#[command(name = "almide", version)]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    /// Create a new Almide project
    Init,
    /// Compile and execute
    #[command(trailing_var_arg = true)]
    Run {
        /// Source file (default: src/main.almd)
        file: Option<String>,
        /// Skip type checking
        #[arg(long)]
        no_check: bool,
        /// Arguments passed to the program
        #[arg(allow_hyphen_values = true)]
        program_args: Vec<String>,
    },
    /// Build a binary
    Build {
        /// Source file (default: src/main.almd)
        file: Option<String>,
        /// Output file name
        #[arg(short)]
        o: Option<String>,
        /// Build target (wasm, npm)
        #[arg(long)]
        target: Option<String>,
        /// Optimize for performance (opt-level=2)
        #[arg(long)]
        release: bool,
        /// Maximum performance: native CPU, fast-math, opt-level=3, LTO
        #[arg(long)]
        fast: bool,
        /// Use unchecked index access (unsafe, no bounds checking)
        #[arg(long)]
        unchecked_index: bool,
        /// Skip type checking
        #[arg(long)]
        no_check: bool,
    },
    /// Run tests
    Test {
        /// Test file
        file: Option<String>,
        /// Filter test names by pattern
        #[arg(short = 'r', long)]
        run: Option<String>,
        /// Skip type checking
        #[arg(long)]
        no_check: bool,
    },
    /// Type check only
    Check {
        /// Source file (default: src/main.almd)
        file: Option<String>,
    },
    /// Format source files
    Fmt {
        /// Files to format (default: src/**/*.almd)
        files: Vec<String>,
        /// Check formatting without writing
        #[arg(long)]
        check: bool,
        /// Check formatting without writing
        #[arg(long)]
        dry_run: bool,
    },
    /// Clear dependency cache
    Clean,
    /// Add a dependency
    Add {
        /// Package specifier
        pkg: String,
        /// Git repository URL
        #[arg(long)]
        git: Option<String>,
        /// Git tag
        #[arg(long)]
        tag: Option<String>,
    },
    /// List dependencies
    Deps,
    /// Emit source code or AST
    #[command(hide = true)]
    Emit {
        /// Source file
        file: String,
        /// Target language (rust, ts, js)
        #[arg(long, default_value = "rust")]
        target: String,
        /// Emit AST as JSON
        #[arg(long)]
        emit_ast: bool,
        /// Emit typed IR as JSON
        #[arg(long)]
        emit_ir: bool,
        /// Skip type checking
        #[arg(long)]
        no_check: bool,
    },
}

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

fn parse_file(file: &str) -> (ast::Program, String, Vec<diagnostic::Diagnostic>) {
    let input = std::fs::read_to_string(file)
        .unwrap_or_else(|e| { eprintln!("Error reading {}: {}", file, e); std::process::exit(1); });

    if file.ends_with(".json") {
        let prog = serde_json::from_str(&input)
            .unwrap_or_else(|e| { eprintln!("JSON parse error: {}", e); std::process::exit(1); });
        (prog, input, Vec::new())
    } else {
        let tokens = lexer::Lexer::tokenize(&input);
        let mut parser = parser::Parser::new(tokens).with_file(file);
        let prog = parser.parse()
            .unwrap_or_else(|e| { eprintln!("{}", e); std::process::exit(1); });
        let parse_errors = std::mem::take(&mut parser.errors);
        (prog, input, parse_errors)
    }
}

fn compile(file: &str, no_check: bool) -> String {
    compile_with_options(file, no_check, &emit_rust::EmitOptions::default(), None).0
}

fn compile_with_options(file: &str, no_check: bool, emit_options: &emit_rust::EmitOptions, build_target: Option<&str>) -> (String, Option<almide::ir::IrProgram>) {
    let (mut program, source_text, parse_errors) = parse_file(file);
    let has_parse_errors = !parse_errors.is_empty();

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

    let mut resolved = resolve::resolve_imports_with_deps(file, &program, &dep_paths)
        .unwrap_or_else(|e| { eprintln!("{}", e); std::process::exit(1); });

    let import_aliases: Vec<(String, String)> = program.imports.iter().filter_map(|imp| {
        if let ast::Decl::Import { path, alias, .. } = imp {
            if let Some(a) = alias {
                let is_self_import = path.first().map(|s| s.as_str()) == Some("self");
                let target = if is_self_import && path.len() >= 2 {
                    path.last().unwrap().clone()
                } else if is_self_import {
                    resolved.modules.iter()
                        .find(|(_, _, _, is_self)| *is_self)
                        .map(|(name, _, _, _)| name.clone())
                        .unwrap_or_else(|| path.join("."))
                } else {
                    path.join(".")
                };
                Some((a.clone(), target))
            } else if path.len() > 1 && path.first().map(|s| s.as_str()) != Some("self") {
                let last = path.last().expect("path.len() > 1 checked above").clone();
                Some((last, path.join(".")))
            } else {
                None
            }
        } else {
            None
        }
    }).collect();

    let mut ir_program = None;
    let mut module_irs = std::collections::HashMap::new();
    if !no_check {
        let mut checker = check::Checker::new();
        checker.set_source(file, &source_text);
        if let Some(t) = build_target {
            checker.set_target(t);
        }
        for (name, mod_prog, pkg_id, is_self) in &resolved.modules {
            checker.register_module(name, mod_prog, pkg_id.as_ref(), *is_self);
        }
        for (alias, target) in &import_aliases {
            checker.register_alias(alias, target);
        }
        let diagnostics = checker.check_program(&mut program);
        // Combine parse errors + checker errors
        let mut all_errors: Vec<&diagnostic::Diagnostic> = parse_errors.iter().collect();
        let checker_errors: Vec<_> = diagnostics.iter()
            .filter(|d| d.level == diagnostic::Level::Error)
            .collect();
        all_errors.extend(checker_errors);
        if !all_errors.is_empty() {
            for d in &all_errors {
                eprintln!("{}", d.display_with_source(&source_text));
            }
            eprintln!("\n{} error(s) found", all_errors.len());
            std::process::exit(1);
        }
        for d in diagnostics.iter().filter(|d| d.level == diagnostic::Level::Warning) {
            eprintln!("{}", d.display_with_source(&source_text));
        }
        // Lower to IR only if no parse errors (partial AST can't produce valid IR)
        if !has_parse_errors {
            ir_program = Some(almide::lower::lower_program(&program, &checker.expr_types, &checker.env));
        }
        // Lower user modules to IR (skip TOML-defined stdlib — they use generated codegen)
        for (name, mod_prog, pkg_id, _) in &mut resolved.modules {
            if almide::stdlib::is_stdlib_module(name) { continue; }
            let mod_types = checker.check_module_bodies(mod_prog);
            let versioned = pkg_id.as_ref().map(|pid| pid.mod_name());
            let mod_ir_module = almide::lower::lower_module(name, mod_prog, &mod_types, &checker.env, versioned);
            // Also keep in module_irs for backward compat (borrow analysis, etc.)
            let mod_ir_program = almide::lower::lower_program(mod_prog, &mod_types, &checker.env);
            module_irs.insert(name.clone(), mod_ir_program);
            if let Some(ref mut ir) = ir_program {
                ir.modules.push(mod_ir_module);
            }
        }
    }

    let code = emit_rust::emit_with_options(&program, &resolved.modules, emit_options, &import_aliases, ir_program.as_ref(), &module_irs);
    (code, ir_program)
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

fn resolve_file(file: Option<String>) -> String {
    file.unwrap_or_else(|| {
        if std::path::Path::new("almide.toml").exists() && std::path::Path::new("src/main.almd").exists() {
            "src/main.almd".to_string()
        } else {
            eprintln!("No file specified and no almide.toml found.");
            eprintln!("Run 'almide init' to create a project, or specify a file.");
            std::process::exit(1);
        }
    })
}

fn main() {
    almide::diagnostic::init_color();
    // Legacy mode: `almide file.almd [--target X]` → rewrite as `almide emit file.almd [--target X]`
    let raw_args: Vec<String> = std::env::args().collect();
    if raw_args.len() >= 2
        && !raw_args[1].starts_with('-')
        && (raw_args[1].ends_with(".almd") || raw_args[1].ends_with(".json"))
    {
        let mut new_args = vec![raw_args[0].clone(), "emit".to_string()];
        new_args.extend_from_slice(&raw_args[1..]);
        let cli = Cli::parse_from(new_args);
        dispatch(cli);
        return;
    }

    let cli = Cli::parse();
    dispatch(cli);
}

fn dispatch(cli: Cli) {
    match cli.command {
        Commands::Init => cli::cmd_init(),
        Commands::Run { file, no_check, program_args } => {
            let file = resolve_file(file);
            cli::cmd_run(&file, &program_args, no_check);
        }
        Commands::Build { file, o, target, release, fast, unchecked_index, no_check } => {
            let file = resolve_file(file);
            cli::cmd_build(&file, o.as_deref(), target.as_deref(), release || fast, fast, unchecked_index, no_check);
        }
        Commands::Test { file, run, no_check } => {
            let file_str = file.as_deref().unwrap_or("");
            cli::cmd_test(file_str, no_check, run.as_deref());
        }
        Commands::Check { file } => {
            let file = resolve_file(file);
            cli::cmd_check(&file);
        }
        Commands::Fmt { files, check, dry_run } => {
            let write_back = !check && !dry_run;
            let fmt_files = if files.is_empty() {
                let mut found = Vec::new();
                if std::path::Path::new("src").is_dir() {
                    collect_almd_files(std::path::Path::new("src"), &mut found);
                }
                if found.is_empty() {
                    eprintln!("No .almd files found in src/");
                    std::process::exit(1);
                }
                found
            } else {
                files
            };
            cli::cmd_fmt(&fmt_files, write_back);
        }
        Commands::Clean => cli::cmd_clean(),
        Commands::Add { pkg, git, tag } => {
            let (name, git_url, tag) = if let Some(git_url) = git {
                (pkg, git_url, tag)
            } else {
                project::resolve_package_spec(&pkg)
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
        }
        Commands::Deps => {
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
        }
        Commands::Emit { file, target, emit_ast, emit_ir, no_check } => {
            cli::cmd_emit(&file, &target, emit_ast, emit_ir, no_check);
        }
    }
}
