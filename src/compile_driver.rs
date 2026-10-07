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
    let input = almide::source_overlay::read_to_string(file)
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

/// Report the type errors an IMPORTED user module produced during
/// `infer_module`, rendered against that module's own source (#862).
///
/// Before this, `infer_module`'s diagnostics were appended to the shared
/// `checker.diagnostics` and never read: only the ENTRY program's return value
/// from `infer_program` was reported. A module could carry an E006 (effect fn
/// called from a pure fn) for weeks while every importer's `almide check` /
/// `almide build` / `almide test` stayed green.
pub(crate) fn report_module_diagnostics(
    module_diags: &[(String, String, Vec<diagnostic::Diagnostic>)],
) -> Result<(), String> {
    let mut errors = 0usize;
    for (path, source, diags) in module_diags {
        for d in diags.iter().filter(|d| d.level == diagnostic::Level::Error) {
            let mut d = d.clone();
            if d.file.is_none() {
                d.file = Some(path.clone());
            }
            err(&format!("{}", diagnostic_render::display_with_source(&d, source)));
            errors += 1;
        }
    }
    if errors > 0 {
        err(&format!("\n{} error(s) found in imported module(s)", errors));
        return Err(format!("{} error(s) found in imported module(s)", errors));
    }
    Ok(())
}

/// Run `infer_module` for `name` with the checker's reported source switched to
/// that module's own file, and return the diagnostics it produced. The entry
/// file's source is restored before returning, so the caller's own reporting is
/// unaffected.
pub(crate) fn infer_module_capturing(
    checker: &mut check::Checker,
    name: &str,
    mod_prog: &mut ast::Program,
    sources: &std::collections::HashMap<String, (String, String)>,
    out: &mut Vec<(String, String, Vec<diagnostic::Diagnostic>)>,
) {
    // The single copy lives in the lib (almide::wasm_leg) since the
    // commissioning: the structural-wasm driver, this driver, and
    // almide-spine's s3 all share it.
    almide::wasm_leg::infer_module_capturing(checker, name, mod_prog, sources, out)
}

/// Register each resolved module's versioned name (dependency modules get a
/// `pkg_id`-derived prefix) before root lowering. Extracted verbatim from
/// `try_compile_with_ir`'s pre-registration loop — writes only to
/// `checker.env.module_versioned_names`, reads only `resolved_modules`.
pub(crate) fn register_versioned_module_names(
    checker: &mut check::Checker,
    resolved_modules: &[(String, ast::Program, Option<project::PkgId>, bool)],
) {
    // The single copy lives in the lib, shared with the structural wasm leg
    // (#3286: that leg had no pre-registration and walled a dependency's
    // top-let read with var:unmapped).
    almide::wasm_leg::register_versioned_module_names(checker, resolved_modules)
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
/// Type-check and lower every user module import resolution found, in
/// resolution order, appending each one's IR onto `ir_program` and
/// `module_irs`; returns the modules' own diagnostics. Shared by
/// `try_compile_with_ir` and `cmd_emit`, which ran identical loops.
pub(crate) fn lower_user_modules(
    checker: &mut check::Checker,
    resolved: &mut crate::resolve::ResolvedModules,
    module_irs: &mut std::collections::HashMap<String, almide::ir::IrProgram>,
    ir_program: &mut Option<almide::ir::IrProgram>,
) -> Vec<(String, String, Vec<diagnostic::Diagnostic>)> {
    let mut module_diags = Vec::new();
    let sources = std::mem::take(&mut resolved.sources);
    for module in &mut resolved.modules {
        lower_one_user_module(checker, module, module_irs, ir_program, &sources, &mut module_diags);
    }
    resolved.sources = sources;
    module_diags
}

/// Type-check and lower a single user (non-stdlib) module discovered by
/// import resolution, appending its IR onto `ir_program` and `module_irs` —
/// same checker/env mutation order as the loop body it was.
fn lower_one_user_module(
    checker: &mut check::Checker,
    module: &mut (String, ast::Program, Option<project::PkgId>, bool),
    module_irs: &mut std::collections::HashMap<String, almide::ir::IrProgram>,
    ir_program: &mut Option<almide::ir::IrProgram>,
    sources: &std::collections::HashMap<String, (String, String)>,
    module_diags: &mut Vec<(String, String, Vec<diagnostic::Diagnostic>)>,
) {
    let (name, mod_prog, pkg_id, _) = module;
    if almide::stdlib::is_stdlib_module(name) && !almide::stdlib::is_bundled_module(name) { return; }
    // For dependency modules, temporarily set self_module_name to the package root
    // so `import self` in sub-modules resolves to the dependency, not the main project
    let saved_self = checker.env.self_module_name;
    if let Some(pid) = pkg_id.as_ref() {
        checker.env.self_module_name = Some(almide::intern::sym(&pid.name));
    }
    infer_module_capturing(checker, name, mod_prog, sources, module_diags);
    let versioned = pkg_id.as_ref().map(|pid| {
        let base = pid.mod_name();
        if let Some(suffix) = name.strip_prefix(&pid.name) {
            format!("{}{}", base, suffix)
        } else {
            base
        }
    }).or_else(|| {
        // A `self` module the package's native code calls back into is
        // pre-registered under its versioned name (#3424).
        checker.env.module_versioned_names.get(&almide::intern::sym(name)).map(|v| v.to_string())
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

fn verify_ir_or_err(ir: &almide::ir::IrProgram) -> Result<(), String> {
    let verify_errors = almide::ir::verify_program(ir);
    if !verify_errors.is_empty() {
        for e in &verify_errors {
            err(&format!("internal compiler error: {}", e));
        }
        return Err(format!("{} IR verification error(s)", verify_errors.len()));
    }
    Ok(())
}

/// The project manifest in the working directory, if there is one that
/// parses — the same lookup every command makes.
pub(crate) fn cwd_project() -> Option<project::Project> {
    let path = std::path::Path::new("almide.toml");
    if path.exists() { project::parse_toml(path).ok() } else { None }
}

/// The post-lowering pipeline every build route runs: the driver's optimize
/// half, the route's integrity check (`verify`), the `[permissions]` gate,
/// then monomorphize + link. The gate inspects the post-optimize, pre-mono
/// IR on every route, and lives here once so a route cannot reach codegen
/// around it: the wasm build/run route skipped it entirely while the native
/// build refused the same program (#3275).
pub(crate) fn optimize_gate_and_link(
    ir: &mut almide::ir::IrProgram,
    proj: Option<&project::Project>,
    verify: impl FnOnce(&almide::ir::IrProgram) -> Result<(), String>,
) -> Result<(), String> {
    almide_driver::optimize_half(ir);
    verify(ir)?;
    if let Some(proj) = proj {
        cli::enforce_project_permissions(ir, proj)?;
    }
    almide_driver::link_half(ir);
    Ok(())
}

/// `try_compile_with_ir`'s parse + project/dep resolution phase. Extracted
/// verbatim — each error arm prints via `err` before returning, exactly
/// matching the original `.map_err(|e| { err(...); e })` chain.
#[allow(clippy::type_complexity)]
fn parse_and_resolve_for_compile(file: &str) -> Result<(ast::Program, String, Vec<diagnostic::Diagnostic>, bool, resolve::ResolvedModules, Option<project::Project>, resolve::SelfVersionedNames), String> {
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

    let mut resolved = resolve::resolve_imports_with_deps(file, &program, &dep_paths)
        .map_err(|e| { err(&format!("{}", e)); e.clone() })?;
    // #3424: this is the native (Rust) build, the one that compiles the
    // package's `native/*.rs` — so it also loads the modules that native code
    // calls back into, under the names it spells them by.
    let self_versioned = resolve::include_native_callback_modules(file, &dep_paths, &mut resolved)
        .map_err(|e| { err(&format!("{}", e)); e.clone() })?;

    Ok((program, source_text, parse_errors, has_parse_errors, resolved, parsed_project, self_versioned))
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
    self_versioned: &resolve::SelfVersionedNames,
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
    // The package's own modules, when its native code calls them by their
    // versioned name (#3424); `lower_one_user_module` reads them back.
    for (name, versioned) in self_versioned {
        checker.env.module_versioned_names.insert(almide::intern::sym(name), almide::intern::sym(versioned));
    }

    // Lower root program (versioned names now available)
    let mut ir_program = lower_root_program_if_ready(parsed.has_parse_errors, program, &checker, parsed.source_text, parsed.file);

    // Lower user modules
    let module_diags = lower_user_modules(&mut checker, resolved, module_irs, &mut ir_program);
    // An imported module's own type errors are fatal for the importer too:
    // a program that cannot be checked cannot be trusted to build (#862).
    report_module_diagnostics(&module_diags)?;
    Ok(ir_program)
}

/// `try_compile_with_ir`'s post-typecheck IR pipeline: optimize, verify
/// integrity, check `[permissions]`, monomorphize, and link dependency
/// modules into the root. Extracted verbatim.
fn optimize_verify_and_link(ir_program: &mut Option<almide::ir::IrProgram>, parsed_project: &Option<project::Project>) -> Result<(), String> {
    // The driver's two halves with the integrity check and the `[permissions]`
    // gate (Security Layer 2) between them — the sequence the wasm route shares.
    match ir_program.as_mut() {
        Some(ir) => optimize_gate_and_link(ir, parsed_project.as_ref(), verify_ir_or_err),
        None => Ok(()),
    }
}

pub(crate) fn try_compile_with_ir(file: &str, no_check: bool, codegen_opts: &codegen::CodegenOptions) -> Result<(String, Option<almide::ir::IrProgram>), String> {
    let (mut program, source_text, parse_errors, has_parse_errors, mut resolved, parsed_project, self_versioned) = parse_and_resolve_for_compile(file)?;

    let mut ir_program: Option<almide::ir::IrProgram> = None;
    let mut module_irs = std::collections::HashMap::new();
    if !no_check {
        let parsed = ParsedSource { file, source_text: &source_text, parse_errors: &parse_errors, has_parse_errors };
        ir_program = typecheck_and_lower_for_compile(parsed, &mut program, &mut resolved, &self_versioned, &mut module_irs)?;
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
