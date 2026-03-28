// Re-export library modules (shared with playground WASM crate)
pub use almide::ast;
pub use almide::codegen;
pub use almide::diagnostic;
pub use almide::fmt;
pub use almide::lexer;
pub use almide::parser;
pub use almide::stdlib;
pub use almide::types;
pub use almide::intern;

// CLI-only modules
mod check;
mod cli;

mod project;
mod project_fetch;
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
        /// Add #[repr(C)] to structs/enums for stable C ABI
        #[arg(long)]
        repr_c: bool,
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
        /// Output test results as JSON (one per line)
        #[arg(long)]
        json: bool,
        /// Target: wasm (wasmtime), ts/typescript (deno/node)
        #[arg(long)]
        target: Option<String>,
    },
    /// Type check only
    Check {
        /// Source file (default: src/main.almd)
        file: Option<String>,
        /// Treat warnings as errors
        #[arg(long)]
        deny_warnings: bool,
        /// Output diagnostics as JSON (one per line)
        #[arg(long)]
        json: bool,
        /// Explain an error code (e.g., --explain E001)
        #[arg(long)]
        explain: Option<String>,
        /// Show effect/capability analysis for each function
        #[arg(long)]
        effects: bool,
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
    /// Compile source to .almdi (module interface + IR artifact)
    Compile {
        /// Module name (e.g., "json", "parser") or file path; defaults to project
        module: Option<String>,
        /// Output interface as machine-readable JSON (no artifact)
        #[arg(long)]
        json: bool,
        /// Print human-readable interface (no artifact)
        #[arg(long)]
        dry_run: bool,
        /// Output directory for .almdi files (default: target/compile)
        #[arg(long, short)]
        output: Option<String>,
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
    /// Print the cached source directory of a dependency
    DepPath {
        /// Dependency name (as declared in almide.toml)
        name: String,
    },
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
        /// Add #[repr(C)] to structs/enums for stable C ABI
        #[arg(long)]
        repr_c: bool,
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

fn try_compile(file: &str, no_check: bool) -> Result<String, String> {
    try_compile_with_ir(file, no_check, &codegen::CodegenOptions::default()).map(|(code, _)| code)
}

pub(crate) fn try_compile_with_ir(file: &str, no_check: bool, codegen_opts: &codegen::CodegenOptions) -> Result<(String, Option<almide::ir::IrProgram>), String> {
    let (mut program, source_text, parse_errors) = parse_file(file);
    let has_parse_errors = !parse_errors.is_empty();

    let parsed_project = if std::path::Path::new("almide.toml").exists() {
        project::parse_toml(std::path::Path::new("almide.toml")).ok()
    } else {
        None
    };

    let dep_paths: Vec<(project::PkgId, std::path::PathBuf)> = if let Some(ref proj) = parsed_project {
        project_fetch::fetch_all_deps(proj)
            .map_err(|e| { eprintln!("{}", e); e.to_string() })?
            .into_iter()
            .map(|fd| (fd.pkg_id, fd.source_dir))
            .collect()
    } else {
        vec![]
    };

    let mut resolved = resolve::resolve_imports_with_deps(file, &program, &dep_paths)
        .map_err(|e| { eprintln!("{}", e); e.clone() })?;

    let import_aliases = build_import_aliases(&program, &resolved);

    let mut ir_program = None;
    let mut module_irs = std::collections::HashMap::new();
    if !no_check {
        let mut checker = check::Checker::new();
        checker.set_source(file, &source_text);
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
            return Err(format!("{} error(s) found", all_errors.len()));
        }
        for d in diagnostics.iter().filter(|d| d.level == diagnostic::Level::Warning) {
            eprintln!("{}", d.display_with_source(&source_text));
        }
        // Lower to IR only if no parse errors (partial AST can't produce valid IR)
        if !has_parse_errors {
            let ir = almide::lower::lower_program(&program, &checker.expr_types, &checker.env);
            // Emit unused variable warnings
            let unused_warnings = almide::ir::collect_unused_var_warnings(&ir, file);
            for d in &unused_warnings {
                eprintln!("{}", d.display_with_source(&source_text));
            }
            ir_program = Some(ir);
        }
        // Lower user modules to IR (skip TOML-defined stdlib — they use generated codegen)
        for (name, mod_prog, pkg_id, _) in &mut resolved.modules {
            if almide::stdlib::is_stdlib_module(name) { continue; }
            let mod_types = checker.check_module_bodies(mod_prog);
            let versioned = pkg_id.as_ref().map(|pid| {
                let base = pid.mod_name(); // e.g. "bindgen_v0"
                // For submodules (e.g. name="bindgen.scaffolding"), append the suffix
                if let Some(suffix) = name.strip_prefix(&pid.name) {
                    format!("{}{}", base, suffix) // "bindgen_v0.scaffolding"
                } else {
                    base
                }
            });
            let mod_ir_module = almide::lower::lower_module(name, mod_prog, &mod_types, &checker.env, versioned);
            // Also keep in module_irs for backward compat (borrow analysis, etc.)
            let mod_ir_program = almide::lower::lower_program(mod_prog, &mod_types, &checker.env);
            module_irs.insert(name.clone(), mod_ir_program);
            if let Some(ref mut ir) = ir_program {
                ir.modules.push(mod_ir_module);
            }
        }
    }

    // Optimize IR: constant folding + dead code elimination
    if let Some(ref mut ir) = ir_program {
        almide::optimize::optimize_program(ir);
        // Reclassify top-level lets after optimization (cross-reference const detection)
        almide::ir::reclassify_top_lets(ir);
    }

    // Verify IR integrity
    if let Some(ref ir) = ir_program {
        let verify_errors = almide::ir::verify_program(ir);
        if !verify_errors.is_empty() {
            for e in &verify_errors {
                eprintln!("internal compiler error: {}", e);
            }
            return Err(format!("{} IR verification error(s)", verify_errors.len()));
        }
    }

    // Security Layer 2: check permissions if defined in almide.toml
    if let Some(ref proj) = parsed_project {
        if !proj.permissions.is_empty() {
            if let Some(ref ir) = ir_program {
                cli::check_permissions(ir, &proj.permissions)?;
            }
        }
    }

    // Monomorphize row-polymorphic functions (Rust target only)
    if let Some(ref mut ir) = ir_program {
        almide::mono::monomorphize(ir);
    }

    // Codegen v3: three-layer pipeline (Nanopass + Templates)
    let ir = ir_program.as_mut().expect("IR required for codegen");
    let code = match codegen::codegen_with(ir, codegen::pass::Target::Rust, codegen_opts) {
        codegen::CodegenOutput::Source(s) => s,
        codegen::CodegenOutput::Binary(_) => unreachable!(),
    };
    Ok((code, ir_program))
}

fn compile_with_ir(file: &str, no_check: bool) -> (String, Option<almide::ir::IrProgram>) {
    try_compile_with_ir(file, no_check, &codegen::CodegenOptions::default())
        .unwrap_or_else(|_| std::process::exit(1))
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

/// Build import alias mappings from a program's import declarations.
/// Used by check, emit, and compile pipelines to register module aliases.
pub(crate) fn build_import_aliases(program: &ast::Program, resolved: &resolve::ResolvedModules) -> Vec<(String, String)> {
    program.imports.iter().filter_map(|imp| {
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
                let last = path.last().expect("path.len() > 1").to_string();
                Some((last, path.iter().map(|s| s.as_str()).collect::<Vec<_>>().join(".")))
            } else {
                None
            }
        } else {
            None
        }
    }).collect()
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

fn print_error_explanation(code: &str) {
    let explanation = match code {
        "E001" => "E001: Type mismatch\n\n  The expression's type does not match what was expected.\n\n  Example:\n    fn f() -> Int = \"hello\"  // error: expected Int but got String\n\n  Fix: Change the expression to match the expected type, or use a\n  conversion function like int.to_string() or int.parse().",
        "E002" => "E002: Undefined function\n\n  The function name was not found in the current scope, stdlib, or imports.\n\n  Example:\n    fn f() -> Int = nonexistent()  // error: undefined function\n\n  Fix: Check the function name for typos, or import the module that defines it.",
        "E003" => "E003: Undefined variable\n\n  The variable name was not found in the current scope.\n\n  Example:\n    fn f() -> Int = x + 1  // error: undefined variable 'x'\n\n  Fix: Check the variable name for typos, or declare it with `let` or `var`\n  before use. If it's a function parameter, ensure it's in the parameter list.",
        "E004" => "E004: Wrong argument count\n\n  The function was called with the wrong number of arguments.\n\n  Example:\n    fn add(a: Int, b: Int) -> Int = a + b\n    let x = add(1)  // error: expects 2 arguments but got 1\n\n  Fix: Provide the correct number of arguments.",
        "E005" => "E005: Argument type mismatch\n\n  A function argument's type does not match the parameter type.\n\n  Example:\n    fn greet(name: String) -> String = name\n    greet(42)  // error: expects String but got Int\n\n  Fix: Pass the correct type, or use a conversion function.",
        "E006" => "E006: Effect isolation violation\n\n  A pure function (fn) is calling an effect function (effect fn).\n  This violates Almide's security model — pure functions cannot perform I/O.\n\n  Example:\n    fn f() -> String = fs.read_text(\"file.txt\")  // error\n\n  Fix: Mark the calling function as `effect fn`.",
        "E007" => "E007: Fan block in pure function\n\n  A `fan` block can only be used inside an `effect fn`.\n  Fan expressions perform concurrent I/O, which requires effect context.\n\n  Example:\n    fn f() -> (Int, Int) = fan { a(); b() }  // error\n\n  Fix: Mark the enclosing function as `effect fn`.",
        "E008" => "E008: Mutable variable capture in fan\n\n  A `fan` block cannot capture mutable variables (var) from the outer scope.\n  This prevents data races in concurrent execution.\n\n  Example:\n    var x = 0\n    fan { use(x); ... }  // error: cannot capture var x\n\n  Fix: Use a `let` binding instead of `var`.",
        "E009" => "E009: Assignment to immutable variable\n\n  Cannot assign to a variable declared with `let` or a function parameter.\n\n  Example:\n    let x = 1\n    x = 2  // error\n\n  Fix: Use `var` instead of `let` if the variable needs to be mutable.",
        "E010" => "E010: Non-exhaustive match\n\n  The match expression does not cover all possible cases of the subject type.\n\n  Example:\n    type Color = | Red | Green | Blue\n    match c { Red => 1 }  // error: missing Green, Blue\n\n  Fix: Add the missing arms, or use `_` as a catch-all.",
        _ => {
            eprintln!("Unknown error code: {}", code);
            std::process::exit(1);
        }
    };
    println!("{}", explanation);
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
        Commands::Build { file, o, target, release, fast, unchecked_index, no_check, repr_c } => {
            let file = resolve_file(file);
            cli::cmd_build(&file, o.as_deref(), target.as_deref(), release || fast, fast, unchecked_index, no_check, repr_c);
        }
        Commands::Test { file, run, no_check, json, target } => {
            let file_str = file.as_deref().unwrap_or("");
            if target.as_deref() == Some("wasm") {
                cli::cmd_test_wasm(file_str, run.as_deref());
            } else if matches!(target.as_deref(), Some("ts" | "typescript")) {
                cli::cmd_test_ts(file_str, run.as_deref());
            } else if json {
                cli::cmd_test_json(file_str, run.as_deref());
            } else {
                cli::cmd_test(file_str, no_check, run.as_deref());
            }
        }
        Commands::Check { file, deny_warnings, json, explain, effects } => {
            if let Some(code) = explain {
                print_error_explanation(&code);
                return;
            }
            let file = resolve_file(file);
            if effects {
                cli::cmd_check_effects(&file);
            } else if json {
                cli::cmd_check_json(&file);
            } else {
                cli::cmd_check(&file, deny_warnings);
            }
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
        Commands::Compile { module, json, dry_run, output } => {
            cli::cmd_compile(module.as_deref(), json, dry_run, output.as_deref());
        }
        Commands::Clean => cli::cmd_clean(),
        Commands::Add { pkg, git, tag } => {
            let (name, git_url, tag) = if let Some(git_url) = git {
                (pkg, git_url, tag)
            } else {
                project_fetch::resolve_package_spec(&pkg)
            };
            project_fetch::add_dep_to_toml(&name, &git_url, tag.as_deref())
                .unwrap_or_else(|e| { eprintln!("{}", e); std::process::exit(1); });
            let dep = project::Dependency {
                name: name.clone(),
                git: git_url,
                tag,
                branch: None,
                version: None,
            };
            project_fetch::fetch_dep(&dep)
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
        Commands::DepPath { name } => {
            if !std::path::Path::new("almide.toml").exists() {
                eprintln!("No almide.toml found");
                std::process::exit(1);
            }
            let proj = project::parse_toml(std::path::Path::new("almide.toml"))
                .unwrap_or_else(|e| { eprintln!("{}", e); std::process::exit(1); });
            let fetched = project_fetch::fetch_all_deps(&proj)
                .unwrap_or_else(|e| { eprintln!("{}", e); std::process::exit(1); });
            match fetched.iter().find(|fd| fd.pkg_id.name == name) {
                Some(fd) => println!("{}", fd.source_dir.display()),
                None => {
                    eprintln!("Dependency '{}' not found in almide.toml", name);
                    std::process::exit(1);
                }
            }
        }
        Commands::Emit { file, target, emit_ast, emit_ir, no_check, repr_c } => {
            cli::cmd_emit(&file, &target, emit_ast, emit_ir, no_check, repr_c);
        }
    }
}
