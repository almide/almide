mod cli;

// Bring library modules into binary crate scope so cli/ submodules can use `crate::*`.
pub use almide::{
    ast, canonicalize, check, codegen, diagnostic, diagnostic_render, fmt,
    import_table, intern, ir, lexer, lower, mono, optimize,
    parser, project, project_fetch, resolve, stdlib, types,
};

use std::process::Command;

/// When true, suppress warning output during compilation (used by REPL).
pub(crate) static SUPPRESS_WARNINGS: std::sync::atomic::AtomicBool =
    std::sync::atomic::AtomicBool::new(false);

fn warnings_suppressed() -> bool {
    SUPPRESS_WARNINGS.load(std::sync::atomic::Ordering::Relaxed)
}
use clap::{Parser, Subcommand};

#[derive(Parser)]
#[command(name = "almide", version)]
struct Cli {
    #[command(subcommand)]
    command: Option<Commands>,
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
        /// Build with optimisations (cargo --release). Required for any
        /// performance-sensitive comparison; without it the generated
        /// Rust runs in dev profile.
        #[arg(long)]
        release: bool,
        /// Execution target: `rust` (default, native binary) or `wasm`
        /// (build a wasm32-wasi module and execute it on the `wasmtime`
        /// CLI). Both targets must produce byte-identical observable
        /// behavior — the cross-target equivalence guarantee.
        #[arg(long)]
        target: Option<String>,
        /// (wasm target) The v1 PCC-verified trust-spine renderer is the DEFAULT
        /// since 0.29.0 (v1-first, v0 fallback where v1 walls; byte-identical
        /// where it lowers and never wrong — honest-wall). `--verified` is kept
        /// as an accepted no-op for compatibility.
        #[arg(long)]
        verified: bool,
        /// (wasm target) Opt out of the v1-first verified renderer and use the
        /// legacy v0 codegen path directly.
        #[arg(long)]
        no_verified: bool,
        /// Arguments passed to the program. Almide's own flags (`--target`,
        /// `--no-check`, `--release`) are consumed before these; anything
        /// after a `--` separator is forwarded verbatim to the program.
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
        /// Build target (wasm)
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
        /// Build as shared library (.dylib/.so) instead of executable
        #[arg(long)]
        cdylib: bool,
        /// Emit the WASM module even if the Perceus RC gate fails (waiver for a
        /// known compiler-RC bug; the artifact may leak memory). Without this a
        /// verification failure is a hard error.
        #[arg(long)]
        emit_unverified: bool,
        /// (wasm target) The v1 PCC-verified trust-spine renderer is the DEFAULT
        /// since 0.29.0 (v1-first, v0 fallback where v1 walls). A v1-produced
        /// module ships VERBATIM (wasm-opt skipped — post-processing would
        /// replace the verified bytes); a v0-fallback build still gets wasm-opt.
        /// (rust target) OPT-IN: try the v1 native trust-spine renderer (#764
        /// rung 1 — same Perceus MIR, Drop erased to Rust scope-end drop,
        /// ownership verified pre-render); walls fall back to v0 codegen.
        #[arg(long)]
        verified: bool,
        /// (wasm target) Opt out of the v1-first verified renderer and use the
        /// legacy v0 codegen path directly (its module gets wasm-opt).
        #[arg(long)]
        no_verified: bool,
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
    /// Start the Language Server Protocol server (for editor integration)
    Lsp,
    /// Explain a diagnostic code (e.g., almide explain E001)
    Explain {
        /// Diagnostic code such as E001
        code: String,
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
    /// Install an Almide CLI from a git repo into ~/.local/bin (or
    /// $ALMIDE_INSTALL). Like `go install`: clone, build --release,
    /// drop the binary on PATH.
    Install {
        /// Package spec: `github.com/<owner>/<repo>`, a full git URL,
        /// or a local path
        spec: String,
        /// Git tag to install (default: latest commit on the default branch)
        #[arg(long)]
        tag: Option<String>,
        /// Git branch
        #[arg(long)]
        branch: Option<String>,
        /// Override the binary name (default: [package].name from almide.toml)
        #[arg(long)]
        name: Option<String>,
        /// Override the install directory (default: $ALMIDE_INSTALL or ~/.local/bin)
        #[arg(long = "bin-dir")]
        bin_dir: Option<std::path::PathBuf>,
        /// Build target (default: native)
        #[arg(long)]
        target: Option<String>,
    },
    /// Update almide to the latest version
    #[command(name = "self-update")]
    SelfUpdate {
        /// Target version (e.g., v0.13.0); defaults to latest
        version: Option<String>,
    },
    /// Agent/LLM semantic queries (outline, doc, peek-def, find-refs)
    Ide {
        #[command(subcommand)]
        cmd: IdeCommand,
    },
    /// Apply mechanically-safe fixes to a file (auto-import; reports remaining
    /// try: snippets for manual application).
    Fix {
        /// Source file (default: src/main.almd)
        file: Option<String>,
        /// Show what would change without modifying the file
        #[arg(long)]
        dry_run: bool,
        /// Emit a machine-readable JSON report (for harness integration)
        #[arg(long)]
        json: bool,
    },
    /// Check canonical docs (llms.txt, etc.) against source-of-truth inputs
    /// (Cargo version, diagnostic code inventory, stdlib auto-import list).
    /// Fails CI when drift is detected.
    #[command(name = "docs-gen")]
    DocsGen {
        /// Verify mode: exit 1 if any drift is found, 0 otherwise.
        #[arg(long)]
        check: bool,
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
        /// Emit Almide dialect (MLIR-like textual form)
        #[arg(long)]
        emit_dialect: bool,
        /// Skip type checking
        #[arg(long)]
        no_check: bool,
        /// Add #[repr(C)] to structs/enums for stable C ABI
        #[arg(long)]
        repr_c: bool,
    },
}

#[derive(clap::Subcommand, Debug)]
enum IdeCommand {
    /// Print one-line summary of each public decl (fn / type / let).
    /// Use this instead of `grep` to discover a package's API.
    /// Accepts a file path or `@stdlib/<module>` (e.g. `@stdlib/string`).
    Outline {
        /// Source file or `@stdlib/<module>` (default: src/main.almd)
        target: Option<String>,
        /// Filter to a substring (e.g. `upper` or `to_`)
        #[arg(long)]
        filter: Option<String>,
        /// Emit JSON instead of one-line text
        #[arg(long)]
        json: bool,
    },
    /// Show signature + doc for a symbol. Accepts `string.to_upper`, `list.fold`,
    /// or a bare user-defined name.
    Doc {
        /// Symbol name (e.g. `string.to_upper`)
        symbol: String,
        /// File context (default: src/main.almd — used for user-defined lookup)
        #[arg(long)]
        file: Option<String>,
    },
    /// Dump concatenated stdlib outlines in one call.
    /// Intended for LLM harnesses that embed a stdlib API inventory in SYSTEM_PROMPT.
    /// Default modules: string, list, int, option, result, map, set.
    StdlibSnapshot {
        /// Comma-separated module list (e.g. `string,list,int`). Defaults to the core set.
        #[arg(long)]
        modules: Option<String>,
        /// Emit JSON array instead of concatenated text sections
        #[arg(long)]
        json: bool,
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

pub(crate) fn try_compile(file: &str, no_check: bool) -> Result<String, String> {
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

    if let Some(ref proj) = parsed_project {
        project::check_compiler_version(proj)
            .map_err(|e| { eprintln!("{}", e); e })?;
    }

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

    let mut ir_program: Option<almide::ir::IrProgram> = None;
    let mut module_irs = std::collections::HashMap::new();
    if !no_check {
        let canon = canonicalize::canonicalize_program(
            &program,
            resolved.modules.iter().map(|(n, p, _, s)| (n.as_str(), p, *s)),
        );
        let mut checker = check::Checker::from_env(canon.env);
        checker.set_source(file, &source_text);
        checker.diagnostics = canon.diagnostics;
        let diagnostics = checker.infer_program(&mut program);
        // Combine parse errors + checker errors
        let mut all_errors: Vec<&diagnostic::Diagnostic> = parse_errors.iter().collect();
        let checker_errors: Vec<_> = diagnostics.iter()
            .filter(|d| d.level == diagnostic::Level::Error)
            .collect();
        all_errors.extend(checker_errors);
        if !all_errors.is_empty() {
            for d in &all_errors {
                eprintln!("{}", diagnostic_render::display_with_source(d, &source_text));
            }
            eprintln!("\n{} error(s) found", all_errors.len());
            return Err(format!("{} error(s) found", all_errors.len()));
        }
        if !warnings_suppressed() {
            for d in diagnostics.iter().filter(|d| d.level == diagnostic::Level::Warning) {
                eprintln!("{}", diagnostic_render::display_with_source(d, &source_text));
            }
        }
        // Pre-register versioned names BEFORE root lowering so cross-module
        // top_let references (mc_bot.DEFAULT_CONFIG) get correct V0 prefix.
        for (name, _, pkg_id, _) in &resolved.modules {
            if let Some(pid) = pkg_id.as_ref() {
                let base = pid.mod_name();
                let versioned = if let Some(suffix) = name.strip_prefix(&pid.name) {
                    format!("{}{}", base, suffix)
                } else {
                    base
                };
                checker.env.module_versioned_names.insert(almide::intern::sym(name), almide::intern::sym(&versioned));
            }
        }

        // Lower root program (versioned names now available)
        if !has_parse_errors {
            let ir = almide::lower::lower_program(&program, &checker.env, &checker.type_map);
            if !warnings_suppressed() {
                let unused_warnings = almide::ir::collect_unused_var_warnings(&ir, file);
                for d in &unused_warnings {
                    eprintln!("{}", diagnostic_render::display_with_source(d, &source_text));
                }
            }
            ir_program = Some(ir);
        }

        // Lower user modules
        for (name, mod_prog, pkg_id, _) in &mut resolved.modules {
            if almide::stdlib::is_stdlib_module(name) && !almide::stdlib::is_bundled_module(name) { continue; }
            // For dependency modules, temporarily set self_module_name to the package root
            // so `import self` in sub-modules resolves to the dependency, not the main project
            let saved_self = checker.env.self_module_name;
            if let Some(pid) = pkg_id.as_ref() {
                checker.env.self_module_name = Some(almide::intern::sym(&pid.name));
            }
            checker.infer_module(mod_prog, name);
            let versioned = pkg_id.as_ref().map(|pid| {
                let base = pid.mod_name();
                if let Some(suffix) = name.strip_prefix(&pid.name) {
                    format!("{}{}", base, suffix)
                } else {
                    base
                }
            });
            if let Some(ref v) = versioned {
                checker.env.module_versioned_names.insert(almide::intern::sym(name), almide::intern::sym(v));
            }
            // Set module's import table for lowering, then restore
            let self_name = checker.env.self_module_name.map(|s| s.to_string());
            let import_table_name = self_name.as_deref().unwrap_or(name);
            let (mod_table, _) = almide::import_table::build_import_table(mod_prog, Some(import_table_name), &checker.env.user_modules);
            let saved_table = std::mem::replace(&mut checker.env.import_table, mod_table);
            let mod_ir_module = almide::lower::lower_module(name, mod_prog, &checker.env, &checker.type_map, versioned);
            // Stdlib Declarative Unification arc complete: stdlib/defs/ is
            // gone, every stdlib fn lives in `stdlib/<m>.almd`. Fns with
            // `@inline_rust` / `@wasm_intrinsic` carry no real body (the
            // Rust walker / WASM emitter skip them), but their attributes
            // are consumed by `StdlibLoweringPass` to rewrite call sites
            // into `IrExprKind::InlineRust`. Fns without those attrs
            // (e.g. helpers like `split_at`) emit normally. No prune.
            let mod_ir_program = almide::lower::lower_program(mod_prog, &checker.env, &checker.type_map);
            checker.env.import_table = saved_table;
            checker.env.self_module_name = saved_self;
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

    // IR link: merge dependency modules into root program
    if let Some(ref mut ir) = ir_program {
        almide::ir_link::ir_link(ir);
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


fn resolve_file(file: Option<String>) -> String {
    file.unwrap_or_else(|| {
        if std::path::Path::new("almide.toml").exists() {
            // Early-validate package name before looking for entry point
            match crate::project::parse_toml(std::path::Path::new("almide.toml")) {
                Err(e) if e.contains("hyphens") => {
                    eprintln!("error: {}", e);
                    std::process::exit(1);
                }
                _ => {}
            }
            // src/mod.almd = package entry point, src/main.almd = executable entry point
            for entry in &["src/mod.almd", "src/main.almd"] {
                if std::path::Path::new(entry).exists() {
                    return entry.to_string();
                }
            }
            eprintln!("almide.toml found but no entry point (src/mod.almd or src/main.almd).");
            eprintln!("Create src/mod.almd (library) or src/main.almd (executable).");
            std::process::exit(1);
        } else {
            eprintln!("No file specified and no almide.toml found.");
            eprintln!("Run 'almide init' to create a project, or specify a file.");
            std::process::exit(1);
        }
    })
}

fn print_error_explanation(code: &str) {
    // Prefer richer markdown reference under docs/diagnostics/<CODE>.md when
    // running from a checkout (or when ALMIDE_DIAGNOSTICS_DIR is set).
    let candidates: Vec<std::path::PathBuf> = {
        let mut paths = Vec::new();
        if let Ok(dir) = std::env::var("ALMIDE_DIAGNOSTICS_DIR") {
            paths.push(std::path::PathBuf::from(dir).join(format!("{}.md", code)));
        }
        if let Ok(exe) = std::env::current_exe() {
            // Walk up parent dirs, looking for docs/diagnostics/CODE.md.
            // Handles target/release/almide and ~/.local/bin/almide layouts.
            let mut cur = exe.as_path();
            for _ in 0..6 {
                paths.push(cur.join("docs/diagnostics").join(format!("{}.md", code)));
                if let Some(p) = cur.parent() { cur = p; } else { break; }
            }
        }
        paths.push(std::path::PathBuf::from(format!("docs/diagnostics/{}.md", code)));
        paths
    };
    for path in &candidates {
        if let Ok(content) = std::fs::read_to_string(path) {
            println!("{}", content);
            return;
        }
    }

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

/// The parse → check → lower → emit pipeline is deeply recursive: AST and IR
/// walks, and type-directed codegen (e.g. `emit_eq_typed` / `emit_ord_cmp3`,
/// container-literal emission) recurse with the shape of the input program. A
/// sufficiently deep or wide machine-generated expression can therefore exhaust
/// the OS's *default* main-thread stack — which is platform-dependent: ~8 MiB on
/// Linux/macOS but only ~1 MiB on Windows. That made compilation depth a silent
/// cross-platform divergence (a program that compiled on Unix overflowed the
/// stack mid-codegen on Windows). We follow the same strategy production
/// compilers use (rustc spawns its driver on an enlarged thread): run the whole
/// driver on a worker thread with a large, fixed stack, so the achievable
/// recursion depth is bounded by heap and identical on every host. The size is a
/// virtual reservation — pages are committed lazily as the stack grows — so it
/// costs no physical memory for shallow programs.
///
/// Overridable via `ALMIDE_COMPILER_STACK` (bytes), the analogue of rustc's
/// `RUST_MIN_STACK`: a non-numeric or empty value falls back to the default. The
/// override exists mainly so a regression test can pin a deliberately small
/// stack and prove that compilation stays within bounded native stack on wide /
/// deep input (see `tests/compiler_stack_test.rs`).
const COMPILER_STACK_SIZE: usize = 256 * 1024 * 1024; // 256 MiB

fn compiler_stack_size() -> usize {
    match std::env::var("ALMIDE_COMPILER_STACK") {
        Ok(v) => v.trim().parse::<usize>().ok().filter(|n| *n > 0).unwrap_or(COMPILER_STACK_SIZE),
        Err(_) => COMPILER_STACK_SIZE,
    }
}

fn main() {
    let child = std::thread::Builder::new()
        .name("almide-main".to_string())
        .stack_size(compiler_stack_size())
        .spawn(run_main)
        .expect("failed to spawn the compiler driver thread");
    // A panic on the worker thread has already printed via the default hook;
    // mirror the main-thread panic exit code (101) so behavior is unchanged.
    if child.join().is_err() {
        std::process::exit(101);
    }
}

fn run_main() {
    crate::diagnostic_render::init_color();
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
    let command = match cli.command {
        Some(cmd) => cmd,
        None => {
            cli::repl::run_repl();
            return;
        }
    };
    match command {
        Commands::Init => cli::cmd_init(),
        Commands::Run { file, no_check, release, target, verified, no_verified, program_args } => {
            let file = resolve_file(file);
            // 0.29.0: v1-first verified wasm is the DEFAULT; `--no-verified` opts out,
            // `--verified` stays an accepted no-op (org byte-verify gate: 0 V1-MISMATCH).
            let _ = verified;
            cli::cmd_run(&file, &program_args, no_check, release, target.as_deref(), !no_verified);
        }
        Commands::Build { file, o, target, release, fast, unchecked_index, no_check, repr_c, cdylib, emit_unverified, verified, no_verified } => {
            let file = resolve_file(file);
            cli::cmd_build(&file, o.as_deref(), target.as_deref(), release || fast, fast, unchecked_index, no_check, repr_c, cdylib, emit_unverified, !no_verified, verified);
        }
        Commands::Test { file, run, no_check, json, target } => {
            let file_str = file.as_deref().unwrap_or("");
            if target.as_deref() == Some("wasm") {
                cli::cmd_test_wasm(file_str, run.as_deref());
            } else if matches!(target.as_deref(), Some("ts" | "typescript")) {
                cli::cmd_test_ts(file_str, run.as_deref());
            } else if json {
                cli::cmd_test_json(file_str, run.as_deref());
            } else if matches!(target.as_deref(), Some("rust" | "native")) {
                // Explicit pure-native run (e.g. CI's "Test Rust" job).
                cli::cmd_test(file_str, no_check, run.as_deref());
            } else {
                // Default: fast rustc-free WASM path, native fallback for gaps.
                cli::cmd_test_fast(file_str, no_check, run.as_deref());
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
        Commands::Fix { file, dry_run, json } => {
            let file = resolve_file(file);
            cli::cmd_fix(&file, dry_run, json);
        }
        Commands::DocsGen { check } => {
            cli::cmd_docs_gen(check);
        }
        Commands::Lsp => {
            cli::lsp::run_lsp();
        }
        Commands::Explain { code } => {
            print_error_explanation(&code);
        }
        Commands::Ide { cmd } => {
            match cmd {
                IdeCommand::Outline { target, filter, json } => {
                    let target = match target {
                        Some(t) if t.starts_with("@stdlib/") => t,
                        other => resolve_file(other),
                    };
                    cli::cmd_ide_outline(&target, filter.as_deref(), json);
                }
                IdeCommand::Doc { symbol, file } => {
                    let file = resolve_file(file);
                    cli::cmd_ide_doc(&symbol, &file);
                }
                IdeCommand::StdlibSnapshot { modules, json } => {
                    cli::cmd_ide_stdlib_snapshot(modules.as_deref(), json);
                }
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
                path: None,
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
        Commands::Install { spec, tag, branch, name, bin_dir, target } => {
            cli::cmd_install(
                &spec,
                tag.as_deref(),
                branch.as_deref(),
                name.as_deref(),
                bin_dir.as_deref(),
                target.as_deref(),
            );
        }
        Commands::SelfUpdate { version } => {
            cli::cmd_self_update(version.as_deref());
        }
        Commands::Emit { file, target, emit_ast, emit_ir, emit_dialect, no_check, repr_c } => {
            cli::cmd_emit(&file, &target, emit_ast, emit_ir, emit_dialect, no_check, repr_c);
        }
    }
}
