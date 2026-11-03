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
        let is_test = resolved.test_modules.contains(&module.0);
        lower_one_user_module(checker, module, module_irs, ir_program, &sources, &mut module_diags, is_test);
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
    is_test_file: bool,
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
    let mod_ir_module = if is_test_file {
        almide::lower::lower_test_module(name, mod_prog, &checker.env, &checker.type_map)
    } else {
        almide::lower::lower_module(name, mod_prog, &checker.env, &checker.type_map, versioned)
    };
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
    entry: (&str, &str),
    verify: impl FnOnce(&almide::ir::IrProgram) -> Result<(), String>,
) -> Result<(), String> {
    // E109 (#4151): judged on the lowered program before the optimizer
    // rewrites it, the IR `check` judges too.
    enforce_effect_values(ir, entry)?;
    almide_driver::optimize_half(ir);
    verify(ir)?;
    if let Some(proj) = proj {
        cli::enforce_project_permissions(ir, proj)?;
    } else {
        almide_wasm_run::set_fs_scope(None);
    }
    almide_driver::link_half(ir);
    // E112 (ADR-0030): linking stopped at the `generic_instances` budget.
    if let Some(d) = almide_driver::budget_refusal() {
        err(&diagnostic_render::display_with_source(&d, entry.1));
        err("\n1 error(s) found");
        return Err("1 error(s) found".to_string());
    }
    Ok(())
}

/// The whole lowered program — the entry and every user module — for the
/// gates `almide check` runs after its own type check (E109 and
/// `[permissions]`): check lowers only the entry, so a module's function was
/// never judged there. Re-runs the build's front end with warnings silenced
/// (check has printed them); `None` when it fails, which check has reported.
pub(crate) fn whole_program_ir(file: &str) -> Option<almide::ir::IrProgram> {
    let was = crate::SUPPRESS_WARNINGS.swap(true, std::sync::atomic::Ordering::Relaxed);
    // A debug-build assertion in module lowering (`increment_use` on a
    // `self.` module lowered from outside its package directory) must not
    // turn a clean `check` into a crash: the gates then judge the entry's own
    // IR, as before this change, and the build route reports the defect.
    let hook = std::panic::take_hook();
    std::panic::set_hook(Box::new(|_| {}));
    let ir = std::panic::catch_unwind(|| {
        let (mut program, source_text, parse_errors, has_parse_errors, mut resolved, _, self_versioned) =
            parse_and_resolve_for_compile(file).ok()?;
        let parsed = ParsedSource { file, source_text: &source_text, parse_errors: &parse_errors, has_parse_errors };
        let mut module_irs = std::collections::HashMap::new();
        typecheck_and_lower_for_compile(parsed, &mut program, &mut resolved, &self_versioned, &mut module_irs).ok()?.0
    })
    .ok()
    .flatten();
    std::panic::set_hook(hook);
    crate::SUPPRESS_WARNINGS.store(was, std::sync::atomic::Ordering::Relaxed);
    ir
}

/// E109 (#4151): refuse a pure fn that runs an `effect fn` reached through a
/// function value. `entry` is the entry file and its source, for rendering.
pub(crate) fn enforce_effect_values(ir: &almide::ir::IrProgram, entry: (&str, &str)) -> Result<(), String> {
    let diags = almide::effect_value_gate::effect_value_diagnostics(ir, entry.0);
    if diags.is_empty() {
        return Ok(());
    }
    for d in &diags {
        err(&format!("{}", diagnostic_render::display_with_source(d, entry.1)));
    }
    err(&format!("\n{} error(s) found", diags.len()));
    Err(format!("{} error(s) found", diags.len()))
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
) -> Result<(Option<almide::ir::IrProgram>, Vec<String>), String> {
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
    // #4008: the entry and every user module are checked by now.
    Ok((ir_program, std::mem::take(&mut checker.outside_guarantee)))
}

/// `try_compile_with_ir`'s post-typecheck IR pipeline: optimize, verify
/// integrity, check `[permissions]`, monomorphize, and link dependency
/// modules into the root. Extracted verbatim.
fn optimize_verify_and_link(ir_program: &mut Option<almide::ir::IrProgram>, parsed_project: &Option<project::Project>, entry: (&str, &str)) -> Result<(), String> {
    // The driver's two halves with the integrity check and the `[permissions]`
    // gate (Security Layer 2) between them — the sequence the wasm route shares.
    match ir_program.as_mut() {
        Some(ir) => optimize_gate_and_link(ir, parsed_project.as_ref(), entry, verify_ir_or_err),
        None => Ok(()),
    }
}

/// The generated Rust, and — for a program that calls the deterministic-meter
/// prims (`fan.bounded` / `fan.race` / `fan.timeout`) — its linked IR as it
/// stood before codegen, so the native route can build it again with the
/// codegen's own meter ([`codegen_metered`], #3673) without a second
/// type-check.
pub(crate) fn try_compile_with_ir(file: &str, no_check: bool, codegen_opts: &codegen::CodegenOptions) -> Result<(String, Option<almide::ir::IrProgram>), String> {
    try_compile_reporting(file, no_check, codegen_opts).map(|(code, metered, _)| (code, metered))
}

/// [`try_compile_with_ir`], plus where the user code leaves the
/// memory-safety guarantee (#4008), which `almide build` names on its
/// `Built` line. Empty under `--no-check`, which checks nothing.
pub(crate) fn try_compile_reporting(file: &str, no_check: bool, codegen_opts: &codegen::CodegenOptions) -> Result<(String, Option<almide::ir::IrProgram>, Vec<String>), String> {
    let t = cli::PhaseTimer::start();
    let (mut ir_program, outside_guarantee) = compile_to_linked_ir(file, no_check, &t)?;
    let metered = ir_program.as_ref().filter(|ir| almide::ir::runtime_use::uses_metered_prims(ir)).cloned();
    // Codegen v3: three-layer pipeline (Nanopass + Templates)
    let ir = ir_program.as_mut().expect("IR required for codegen");
    let code = match codegen::codegen_with(ir, codegen::pass::Target::Rust, codegen_opts) {
        codegen::CodegenOutput::Source(s) => s,
        codegen::CodegenOutput::Binary(_) => unreachable!(),
    };
    t.lap("codegen");
    Ok((code, metered, outside_guarantee))
}

/// Everything [`try_compile_reporting`] does before codegen — every step that
/// prints a diagnostic or refuses the program (type errors, warnings, E109,
/// `[permissions]`, the instance budget) — for a caller that wants a file's
/// verdict without its Rust: the native test run judges each file alone and
/// then generates one program for all of them (#4441).
pub(crate) fn check_through_link(file: &str) -> Result<(), String> {
    compile_to_linked_ir(file, false, &cli::PhaseTimer::start()).map(|_| ())
}

fn compile_to_linked_ir(file: &str, no_check: bool, t: &cli::PhaseTimer) -> Result<(Option<almide::ir::IrProgram>, Vec<String>), String> {
    let (mut program, source_text, parse_errors, has_parse_errors, mut resolved, parsed_project, self_versioned) = parse_and_resolve_for_compile(file)?;
    t.lap("parse+resolve");

    let mut ir_program: Option<almide::ir::IrProgram> = None;
    let mut module_irs = std::collections::HashMap::new();
    let mut outside_guarantee = Vec::new();
    if !no_check {
        let parsed = ParsedSource { file, source_text: &source_text, parse_errors: &parse_errors, has_parse_errors };
        (ir_program, outside_guarantee) = typecheck_and_lower_for_compile(parsed, &mut program, &mut resolved, &self_versioned, &mut module_irs)?;
    }
    t.lap("check+lower");

    // `[permissions]` come from the entry file's own project (#4159 audit),
    // whatever directory the command runs in.
    let permissions_project = project::permissions_project_for(file).or(parsed_project);
    optimize_verify_and_link(&mut ir_program, &permissions_project, (file, &source_text))?;
    t.lap("optimize+link");
    Ok((ir_program, outside_guarantee))
}

/// Each dependency package's capability set (#4093, `almide deps`): the
/// effect categories its functions reach, by the same inference
/// `[permissions].allow` is checked on (`EffectInferencePass`, where an
/// `@extern` fn is every category). `entry` is a program that imports the
/// cwd package's direct dependencies; a package reached only through them is
/// measured over the modules they load. Keyed by package name; a package
/// none of whose modules was loaded is absent. `Err` when the dependencies do
/// not compile (the diagnostics were printed).
pub(crate) fn dependency_capabilities(entry: &str) -> Result<std::collections::HashMap<String, std::collections::BTreeSet<String>>, String> {
    use almide::codegen::pass::NanoPass;
    let (mut program, source_text, parse_errors, has_parse_errors, mut resolved, _proj, self_versioned) = parse_and_resolve_for_compile(entry)?;
    let package_of: std::collections::HashMap<String, String> = resolved.modules.iter()
        .filter_map(|(name, _, pkg, _)| pkg.as_ref().map(|p| (name.clone(), p.name.clone())))
        .collect();
    let parsed = ParsedSource { file: entry, source_text: &source_text, parse_errors: &parse_errors, has_parse_errors };
    let mut module_irs = std::collections::HashMap::new();
    let (ir, _) = typecheck_and_lower_for_compile(parsed, &mut program, &mut resolved, &self_versioned, &mut module_irs)?;
    let ir = ir.ok_or("the dependencies did not parse")?;
    let ir = almide::codegen::pass_effect_inference::EffectInferencePass.run(ir, codegen::pass::Target::Rust).program;
    let mut caps: std::collections::HashMap<String, std::collections::BTreeSet<String>> = std::collections::HashMap::new();
    for module in &ir.modules {
        let Some(pkg) = package_of.get(&module.name.to_string()) else { continue };
        let set = caps.entry(pkg.clone()).or_default();
        for func in &module.functions {
            if let Some(fe) = ir.effect_map.functions.get(&format!("{}.{}", module.name, func.name)) {
                set.extend(fe.transitive.iter().map(|e| e.to_string()));
            }
        }
    }
    Ok(caps)
}

/// #3673: build a metered program's linked IR with the codegen's own
/// deterministic meter (`MeterChargePass`), its runtime appended.
pub(crate) fn codegen_metered(mut ir: almide::ir::IrProgram, codegen_opts: &codegen::CodegenOptions) -> String {
    let opts = codegen::CodegenOptions { meter_in_codegen: true, ..codegen_opts.clone() };
    match codegen::codegen_with(&mut ir, codegen::pass::Target::Rust, &opts) {
        codegen::CodegenOutput::Source(s) => s + &almide_mir::pipeline::native_meter_runtime(),
        codegen::CodegenOutput::Binary(_) => unreachable!(),
    }
}

/// One program of the native test run (#4441): a batch of test files
/// compiled together by [`compile_test_batch`].
pub(crate) struct TestRunProgram {
    /// The generated Rust of the whole program.
    pub rs_code: String,
    /// Per file of the batch, in its order: the prefix its test fns carry in
    /// the generated crate (`almide_rt_<module>_`, the flattened name of a
    /// module fn), so the runner can select and report one file's tests.
    pub test_prefixes: Vec<String>,
}

/// A batch that did not compile, and the files that made it fail when the
/// compiler named them (a type error inside a test file's module). Empty
/// `culprits` means the failure was not attributed.
pub(crate) struct BatchFailure {
    pub message: String,
    pub culprits: Vec<String>,
}

/// A test file of the run, and its module in the one program.
pub(crate) struct PlannedTestFile {
    pub file: String,
    /// Its position in the run: its module is `almide_test_file_<index>`.
    pub index: usize,
}

impl PlannedTestFile {
    fn module(&self) -> String {
        format!("almide_test_file_{}", self.index)
    }
}

/// What every compile of the run shares: the project and its fetched
/// dependencies.
pub(crate) struct TestRunContext {
    dep_paths: Vec<(project::PkgId, std::path::PathBuf)>,
    project: Option<project::Project>,
}

pub(crate) fn test_run_context() -> Result<TestRunContext, String> {
    let project = cwd_project();
    let dep_paths = match project {
        Some(ref proj) => {
            project::check_compiler_version(proj)?;
            project_fetch::fetch_all_deps(proj).map_err(|e| e.to_string())?.into_iter().map(|fd| (fd.pkg_id, fd.source_dir)).collect()
        }
        None => vec![],
    };
    Ok(TestRunContext { dep_paths, project })
}

/// The native test run's plan: which files share which program.
pub(crate) struct TestRunPlan {
    /// The batches; each compiles as one program.
    pub batches: Vec<Vec<PlannedTestFile>>,
    /// Files that are built alone, as before: their own compile reports why.
    pub alone: Vec<String>,
}

/// Plan the native test run of `files` (already checked one by one): each
/// test file becomes a module of one program, compiled the way `almide
/// build` compiles one — every module the files reach loaded, checked,
/// lowered and emitted once, the runtime the whole program needs — with the
/// test files' `test` blocks kept (another module's are not, #2550).
///
/// Files share a program unless that would give a module name, or a name a
/// table may key bare ([`declared_names`]), a second owner: two test files
/// in different directories can each import their own `self.helpers`, and
/// two test files can each declare a `type Shape` — a name is one identity
/// per module, but some of the checker's and the codegen's tables still key
/// a type, a variant case or a convention method by its bare name. A file
/// that declares `main` or a `scoped fn` (both admitted in an entry program
/// only), or that loads no module from disk, is built alone.
pub(crate) fn plan_test_run(ctx: &TestRunContext, files: &[String]) -> TestRunPlan {
    type Owners = std::collections::HashMap<String, std::collections::BTreeSet<String>>;
    let mut batches: Vec<(Owners, Vec<PlannedTestFile>)> = Vec::new();
    let mut alone = Vec::new();
    for (index, file) in files.iter().enumerate() {
        let Ok((program, resolved, _)) = resolve_test_file(file, &ctx.dep_paths) else {
            alone.push(file.clone());
            continue;
        };
        if program.decls.iter().any(|d| matches!(d, ast::Decl::Fn { name, scoped, .. } if *scoped || name.as_str() == "main")) {
            alone.push(file.clone());
            continue;
        }
        // A file that loads no module of its project or its dependencies
        // shares nothing a program could compile once — the runtime it
        // shares is precompiled already — and builds fastest alone, in
        // parallel with the rest.
        if !resolved.sources.keys().any(|m| m != file) {
            alone.push(file.clone());
            continue;
        }
        let mut mine: Owners = Owners::new();
        let mut own = |name: String, owner: &str| {
            mine.entry(name).or_default().insert(owner.to_string());
        };
        for n in declared_names(&program) {
            own(n, file);
        }
        for (name, mod_prog, _, _) in &resolved.modules {
            let owner = resolved.sources.get(name).map_or_else(|| format!("<{name}>"), |(p, _)| p.clone());
            // The module's name itself: one program loads one module per name.
            own(format!("module {name}"), &owner);
            for n in declared_names(mod_prog) {
                own(n, &owner);
            }
        }
        // Joining a batch may not give a name an owner set that neither a
        // member nor this file already had.
        let fits = |have: &Owners| mine.iter().all(|(n, o)| have.get(n).is_none_or(|h| h.is_subset(o) || o.is_subset(h)));
        let planned = PlannedTestFile { file: file.clone(), index };
        match batches.iter_mut().find(|(have, _)| fits(have)) {
            Some((have, members)) => {
                for (n, o) in mine {
                    let h = have.entry(n).or_default();
                    if o.len() > h.len() {
                        *h = o;
                    }
                }
                members.push(planned);
            }
            None => batches.push((mine, vec![planned])),
        }
    }
    TestRunPlan { batches: batches.into_iter().map(|(_, b)| b).collect(), alone }
}

fn resolve_test_file(
    file: &str,
    dep_paths: &[(project::PkgId, std::path::PathBuf)],
) -> Result<(ast::Program, resolve::ResolvedModules, resolve::SelfVersionedNames), String> {
    let source = almide::source_overlay::read_to_string(file).map_err(|e| format!("{file}: {e}"))?;
    let tokens = lexer::Lexer::tokenize(&source);
    let mut parser = parser::Parser::new(tokens).with_file(file);
    let program = parser.parse().map_err(|e| format!("{file}: {e}"))?;
    if !parser.errors.is_empty() {
        return Err(format!("{file}: parse errors"));
    }
    let mut resolved = resolve::resolve_imports_with_deps(file, &program, dep_paths)?;
    let self_versioned = resolve::include_native_callback_modules(file, dep_paths, &mut resolved)?;
    resolved.sources.insert(file.to_string(), (file.to_string(), source));
    Ok((program, resolved, self_versioned))
}

/// The top-level names a program declares that a table may still key by
/// their bare spelling: types and their variant cases, convention methods
/// (`fn Type.method`), protocols. A plain fn or a top-level let is always
/// qualified by its module, so two test files' `fn helper`s never meet.
fn declared_names(program: &ast::Program) -> Vec<String> {
    let mut names = Vec::new();
    for decl in &program.decls {
        match decl {
            ast::Decl::Fn { name, .. } if name.as_str().contains('.') => names.push(name.to_string()),
            ast::Decl::Protocol { name, .. } => names.push(name.to_string()),
            ast::Decl::Type { name, ty, .. } => {
                names.push(name.to_string());
                if let ast::TypeExpr::Variant { cases, .. } = ty {
                    names.extend(cases.iter().map(|c| match c {
                        ast::VariantCase::Unit { name } | ast::VariantCase::Tuple { name, .. } | ast::VariantCase::Record { name, .. } => name.to_string(),
                    }));
                }
            }
            _ => {}
        }
    }
    names
}

/// The test files a rustc error output names, by their module's symbol
/// prefix (`almide_rt_almide_0test_0file_0<n>_…`).
pub(crate) fn culprits_in(text: &str, batch: &[PlannedTestFile]) -> Vec<String> {
    batch
        .iter()
        .filter(|p| text.contains(&format!("almide_rt_{}_", almide_base::names::module_ident(&p.module()))))
        .map(|p| p.file.clone())
        .collect()
}

/// Compile one batch of the plan as one program. It runs in a process of its
/// own (`almide test-run-compile`), which prints nothing the run shows: every
/// diagnostic was printed by the files' own checks, and a gate that ends the
/// process ends only this compile.
pub(crate) fn compile_test_batch(ctx: &TestRunContext, batch: &[PlannedTestFile]) -> Result<TestRunProgram, BatchFailure> {
    compile_test_batch_inner(ctx, batch)
}

fn compile_test_batch_inner(plan: &TestRunContext, batch: &[PlannedTestFile]) -> Result<TestRunProgram, BatchFailure> {
    let unattributed = |message: String| BatchFailure { message, culprits: vec![] };
    let first = batch.first().map(|p| p.file.clone()).ok_or_else(|| unattributed("no test file".into()))?;
    // Every module any file of the batch reaches, once each, in an order
    // that keeps each file's own resolution order (leaves first).
    let mut modules: Vec<(String, ast::Program, Option<project::PkgId>, bool)> = Vec::new();
    let mut loaded: std::collections::HashSet<String> = std::collections::HashSet::new();
    let mut sources = std::collections::HashMap::new();
    let mut self_versioned = resolve::SelfVersionedNames::new();
    let mut test_files = Vec::new();
    for p in batch {
        let (program, resolved, versioned) = resolve_test_file(&p.file, &plan.dep_paths).map_err(unattributed)?;
        self_versioned.extend(versioned);
        for m in resolved.modules {
            if loaded.insert(m.0.clone()) {
                modules.push(m);
            }
        }
        let mut file_sources = resolved.sources;
        if let Some(own) = file_sources.remove(&p.file) {
            sources.insert(p.module(), own);
        }
        sources.extend(file_sources);
        test_files.push((p.module(), program));
    }
    let mut test_modules = std::collections::HashSet::new();
    let mut test_prefixes = Vec::new();
    for (name, program) in test_files {
        test_prefixes.push(format!("almide_rt_{}_", almide_base::names::module_ident(&name)));
        test_modules.insert(name.clone());
        modules.push((name, program, None, false));
    }
    let mut resolved = resolve::ResolvedModules { modules, sources, test_modules };

    let mut entry = parser::Parser::new(lexer::Lexer::tokenize("")).parse().map_err(|e| unattributed(e.to_string()))?;
    let canon = canonicalize::canonicalize_program(&entry, resolved.modules.iter().map(|(n, p, _, s)| (n.as_str(), p, *s)));
    let mut checker = check::Checker::from_env(canon.env);
    checker.set_source(&first, "");
    checker.diagnostics = canon.diagnostics;
    almide::resolve::refresh_module_toplets(&mut checker, &resolved.modules);
    let diagnostics = checker.infer_program(&mut entry);
    let errors: Vec<&diagnostic::Diagnostic> = diagnostics.iter().filter(|d| d.level == diagnostic::Level::Error).collect();
    if !errors.is_empty() {
        let culprits = batch.iter().filter(|p| errors.iter().any(|d| d.file.as_deref() == Some(p.file.as_str()))).map(|p| p.file.clone()).collect();
        return Err(BatchFailure { message: format!("{} error(s) found", errors.len()), culprits });
    }
    register_versioned_module_names(&mut checker, &resolved.modules);
    for (name, versioned) in &self_versioned {
        checker.env.module_versioned_names.insert(almide::intern::sym(name), almide::intern::sym(versioned));
    }
    let mut ir_program = lower_root_program_if_ready(false, &entry, &checker, "", &first);
    let mut module_irs = std::collections::HashMap::new();
    let module_diags = lower_user_modules(&mut checker, &mut resolved, &mut module_irs, &mut ir_program);
    let failing: Vec<&String> = module_diags
        .iter()
        .filter(|(_, _, diags)| diags.iter().any(|d| d.level == diagnostic::Level::Error))
        .map(|(path, _, _)| path)
        .collect();
    if !failing.is_empty() {
        let culprits = batch.iter().filter(|p| failing.iter().any(|f| **f == p.file)).map(|p| p.file.clone()).collect();
        return Err(BatchFailure { message: format!("errors in {} module(s)", failing.len()), culprits });
    }
    let permissions_project = project::permissions_project_for(&first).or_else(|| plan.project.clone());
    optimize_verify_and_link(&mut ir_program, &permissions_project, (&first, "")).map_err(unattributed)?;
    let ir = ir_program.as_mut().ok_or_else(|| unattributed("the test run did not lower".into()))?;
    // A metered program (`fan.bounded` & co.) links v1-rendered regions
    // against its entry file's source: that stays a per-file build.
    if almide::ir::runtime_use::uses_metered_prims(ir) {
        return Err(unattributed("a test file uses the deterministic meter".into()));
    }
    let rs_code = match codegen::codegen_with(ir, codegen::pass::Target::Rust, &codegen::CodegenOptions::default()) {
        codegen::CodegenOutput::Source(s) => s,
        codegen::CodegenOutput::Binary(_) => unreachable!(),
    };
    Ok(TestRunProgram { rs_code, test_prefixes })
}
