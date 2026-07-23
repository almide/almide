//! The `.almd` → Rust-source compile pipeline: parse, resolve imports,
//! type-check, lower to IR, optimize/verify/link, and codegen. Split out of
//! `main.rs` (which had grown past the max-lines threshold) — a pure text
//! move, no behavior change. `parse_file`, `try_compile`,
//! `register_versioned_module_names`, `lower_one_user_module` and
//! `try_compile_with_ir` are `pub(crate)` because `cli/*.rs` call them via
//! `crate::<name>`; everything else here is used only within this file.

use crate::{ast, canonicalize, check, codegen, diagnostic, diagnostic_render, err, lexer, parser, project, project_fetch, resolve};
use crate::{cli, warnings_suppressed};

pub(crate) fn parse_file(file: &str) -> (ast::Program, String, Vec<diagnostic::Diagnostic>) {
    let input = std::fs::read_to_string(file)
        .unwrap_or_else(|e| { err(&format!("Error reading {}: {}", file, e)); std::process::exit(1); });

    if file.ends_with(".json") {
        let prog = serde_json::from_str(&input)
            .unwrap_or_else(|e| { err(&format!("JSON parse error: {}", e)); std::process::exit(1); });
        (prog, input, Vec::new())
    } else {
        let tokens = lexer::Lexer::tokenize(&input);
        let mut parser = parser::Parser::new(tokens).with_file(file);
        let prog = parser.parse()
            .unwrap_or_else(|e| { err(&format!("{}", e)); std::process::exit(1); });
        let parse_errors = std::mem::take(&mut parser.errors);
        (prog, input, parse_errors)
    }
}

pub(crate) fn try_compile(file: &str, no_check: bool) -> Result<String, String> {
    try_compile_with_ir(file, no_check, &codegen::CodegenOptions::default()).map(|(code, _)| code)
}

/// Combine parse + checker errors and print them; returns `Err` if either
/// would abort compilation. Also prints (non-fatal) warnings when not
/// suppressed. Extracted from `try_compile_with_ir`'s error-reporting block —
/// a pure diagnostics-formatting step with no shared mutable state; the
/// original early `return` is preserved via `?` at the call site.
fn report_check_diagnostics(
    parse_errors: &[diagnostic::Diagnostic],
    diagnostics: &[diagnostic::Diagnostic],
    source_text: &str,
) -> Result<(), String> {
    let mut all_errors: Vec<&diagnostic::Diagnostic> = parse_errors.iter().collect();
    let checker_errors: Vec<_> = diagnostics.iter()
        .filter(|d| d.level == diagnostic::Level::Error)
        .collect();
    all_errors.extend(checker_errors);
    if !all_errors.is_empty() {
        for d in &all_errors {
            err(&format!("{}", diagnostic_render::display_with_source(d, source_text)));
        }
        err(&format!("\n{} error(s) found", all_errors.len()));
        return Err(format!("{} error(s) found", all_errors.len()));
    }
    if !warnings_suppressed() {
        for d in diagnostics.iter().filter(|d| d.level == diagnostic::Level::Warning) {
            err(&format!("{}", diagnostic_render::display_with_source(d, source_text)));
        }
    }
    Ok(())
}

/// Register each resolved module's versioned name (dependency modules get a
/// `pkg_id`-derived prefix) before root lowering. Extracted verbatim from
/// `try_compile_with_ir`'s pre-registration loop — writes only to
/// `checker.env.module_versioned_names`, reads only `resolved_modules`.
pub(crate) fn register_versioned_module_names(
    checker: &mut check::Checker,
    resolved_modules: &[(String, ast::Program, Option<project::PkgId>, bool)],
) {
    for (name, _, pkg_id, _) in resolved_modules {
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
}

/// Lower the root program to IR once parsing succeeded, printing unused-var
/// warnings along the way. Extracted verbatim from `try_compile_with_ir`'s
/// root-lowering block — reads only its parameters, returns the new IR
/// (`None` when parse errors already blocked lowering) instead of mutating a
/// shared `Option` in place.
fn lower_root_program_if_ready(
    has_parse_errors: bool,
    program: &ast::Program,
    checker: &check::Checker,
    source_text: &str,
    file: &str,
) -> Option<almide::ir::IrProgram> {
    if has_parse_errors {
        return None;
    }
    let ir = almide::lower::lower_program(program, &checker.env, &checker.type_map);
    if !warnings_suppressed() {
        let unused_warnings = almide::ir::collect_unused_var_warnings(&ir, file);
        for d in &unused_warnings {
            err(&format!("{}", diagnostic_render::display_with_source(d, source_text)));
        }
    }
    Some(ir)
}

/// Verify IR integrity, printing internal-compiler-error diagnostics and
/// returning `Err` on failure. Extracted verbatim from
/// `try_compile_with_ir`'s post-optimization verification block.
/// Type-check and lower a single user (non-stdlib) module discovered by
/// import resolution, appending its IR onto `ir_program` and `module_irs`.
/// Extracted from `try_compile_with_ir`'s per-module loop body — same
/// checker/env mutation order, `continue` becomes an early `return`. Shared
/// with `cmd_emit`, which ran an identical loop body.
pub(crate) fn lower_one_user_module(
    checker: &mut check::Checker,
    name: &mut String,
    mod_prog: &mut ast::Program,
    pkg_id: &mut Option<project::PkgId>,
    module_irs: &mut std::collections::HashMap<String, almide::ir::IrProgram>,
    ir_program: &mut Option<almide::ir::IrProgram>,
) {
    if almide::stdlib::is_stdlib_module(name) && !almide::stdlib::is_bundled_module(name) { return; }
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
    if let Some(ir) = ir_program {
        ir.modules.push(mod_ir_module);
    }
}

fn verify_ir_or_err(ir_program: &Option<almide::ir::IrProgram>) -> Result<(), String> {
    if let Some(ir) = ir_program {
        let verify_errors = almide::ir::verify_program(ir);
        if !verify_errors.is_empty() {
            for e in &verify_errors {
                err(&format!("internal compiler error: {}", e));
            }
            return Err(format!("{} IR verification error(s)", verify_errors.len()));
        }
    }
    Ok(())
}

/// `try_compile_with_ir`'s parse + project/dep resolution phase. Extracted
/// verbatim — each error arm prints via `err` before returning, exactly
/// matching the original `.map_err(|e| { err(...); e })` chain.
#[allow(clippy::type_complexity)]
fn parse_and_resolve_for_compile(file: &str) -> Result<(ast::Program, String, Vec<diagnostic::Diagnostic>, bool, resolve::ResolvedModules, Option<project::Project>), String> {
    let (program, source_text, parse_errors) = parse_file(file);
    let has_parse_errors = !parse_errors.is_empty();

    let parsed_project = if std::path::Path::new("almide.toml").exists() {
        project::parse_toml(std::path::Path::new("almide.toml")).ok()
    } else {
        None
    };

    if let Some(ref proj) = parsed_project {
        project::check_compiler_version(proj)
            .map_err(|e| { err(&format!("{}", e)); e })?;
    }

    let dep_paths: Vec<(project::PkgId, std::path::PathBuf)> = if let Some(ref proj) = parsed_project {
        project_fetch::fetch_all_deps(proj)
            .map_err(|e| { err(&format!("{}", e)); e.to_string() })?
            .into_iter()
            .map(|fd| (fd.pkg_id, fd.source_dir))
            .collect()
    } else {
        vec![]
    };

    let resolved = resolve::resolve_imports_with_deps(file, &program, &dep_paths)
        .map_err(|e| { err(&format!("{}", e)); e.clone() })?;

    Ok((program, source_text, parse_errors, has_parse_errors, resolved, parsed_project))
}

/// `try_compile_with_ir`'s parse-phase output needed by the type-check
/// phase — bundled into one struct (`typecheck_and_lower_for_compile` was
/// at 7 positional params, a max-params violation on its own) so the
/// signature stays under the params threshold. Field names mirror
/// `parse_and_resolve_for_compile`'s return tuple 1:1.
struct ParsedSource<'a> {
    file: &'a str,
    source_text: &'a str,
    parse_errors: &'a [diagnostic::Diagnostic],
    has_parse_errors: bool,
}

/// `try_compile_with_ir`'s type-check + root/module lowering phase — only
/// run when `!no_check`. Extracted verbatim.
fn typecheck_and_lower_for_compile(
    parsed: ParsedSource,
    program: &mut ast::Program,
    resolved: &mut resolve::ResolvedModules,
    module_irs: &mut std::collections::HashMap<String, almide::ir::IrProgram>,
) -> Result<Option<almide::ir::IrProgram>, String> {
    let canon = canonicalize::canonicalize_program(
        program,
        resolved.modules.iter().map(|(n, p, _, s)| (n.as_str(), p, *s)),
    );
    let mut checker = check::Checker::from_env(canon.env);
    checker.set_source(parsed.file, parsed.source_text);
    checker.diagnostics = canon.diagnostics;
    // #785: module top-let types must be fully inferred before the entry
    // program reads them (drivers infer the entry FIRST; without this the
    // readers see the registration seed — Unknown for non-literal inits).
    almide::resolve::refresh_module_toplets(&mut checker, &resolved.modules);
    let diagnostics = checker.infer_program(program);
    report_check_diagnostics(parsed.parse_errors, &diagnostics, parsed.source_text)?;
    // Pre-register versioned names BEFORE root lowering so cross-module
    // top_let references (mc_bot.DEFAULT_CONFIG) get correct V0 prefix.
    register_versioned_module_names(&mut checker, &resolved.modules);

    // Lower root program (versioned names now available)
    let mut ir_program = lower_root_program_if_ready(parsed.has_parse_errors, program, &checker, parsed.source_text, parsed.file);

    // Lower user modules
    for (name, mod_prog, pkg_id, _) in &mut resolved.modules {
        lower_one_user_module(&mut checker, name, mod_prog, pkg_id, module_irs, &mut ir_program);
    }
    Ok(ir_program)
}

/// `try_compile_with_ir`'s post-typecheck IR pipeline: optimize, verify
/// integrity, check `[permissions]`, monomorphize, and link dependency
/// modules into the root. Extracted verbatim.
fn optimize_verify_and_link(ir_program: &mut Option<almide::ir::IrProgram>, parsed_project: &Option<project::Project>) -> Result<(), String> {
    // Optimize IR: constant folding + dead code elimination
    if let Some(ir) = ir_program.as_mut() {
        almide::optimize::optimize_program(ir);
        // Reclassify top-level lets after optimization (cross-reference const detection)
        almide::ir::reclassify_top_lets(ir);
    }

    // Verify IR integrity
    verify_ir_or_err(ir_program)?;

    // Security Layer 2: check permissions if defined in almide.toml
    if let Some(proj) = parsed_project {
        if !proj.permissions.is_empty() {
            if let Some(ir) = ir_program.as_ref() {
                cli::check_permissions(ir, &proj.permissions)?;
            }
        }
    }

    // Monomorphize row-polymorphic functions (Rust target only)
    if let Some(ir) = ir_program.as_mut() {
        almide::mono::monomorphize(ir);
    }

    // IR link: merge dependency modules into root program
    if let Some(ir) = ir_program.as_mut() {
        almide::ir_link::ir_link(ir);
    }

    Ok(())
}

pub(crate) fn try_compile_with_ir(file: &str, no_check: bool, codegen_opts: &codegen::CodegenOptions) -> Result<(String, Option<almide::ir::IrProgram>), String> {
    let (mut program, source_text, parse_errors, has_parse_errors, mut resolved, parsed_project) = parse_and_resolve_for_compile(file)?;

    let mut ir_program: Option<almide::ir::IrProgram> = None;
    let mut module_irs = std::collections::HashMap::new();
    if !no_check {
        let parsed = ParsedSource { file, source_text: &source_text, parse_errors: &parse_errors, has_parse_errors };
        ir_program = typecheck_and_lower_for_compile(parsed, &mut program, &mut resolved, &mut module_irs)?;
    }

    optimize_verify_and_link(&mut ir_program, &parsed_project)?;

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
