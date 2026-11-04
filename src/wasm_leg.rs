//! The structural-wasm leg's front driver (the commissioning switchover):
//! parse → resolve → canonicalize → check → lower → module lowering →
//! self-host registry linking → `almide_driver::link_ir` → C-132 move-mode.
//!
//! HISTORY: authored in the greenfield arc as almide-spine/src/s5.rs (itself
//! replicating src/compile_driver.rs's module loop with attribution), moved
//! here at commissioning so the PRODUCT driver and the spine gates judge one
//! implementation — almide-spine re-exports this module for its parity gates.
//! The 610/610 corpus acceptance was measured through exactly this pipeline.

/// Front half of `run_file`: parse → check → lower → link, returning the
/// linked IR (unit 6's emission gate shares this exact pipeline so the
/// interpreter and the wasm backend judge the SAME IR). The dep-free form —
/// the spine gates and the corpus judge through here.
pub fn lower_to_ir(path: &str, source_text: &str) -> Result<crate::ir::IrProgram, String> {
    lower_to_ir_with_deps(path, source_text, &[])
}

/// The full-front form: external package dependencies resolve through the
/// SAME table the incumbent driver uses (`resolve_imports_with_deps`), and
/// dependency modules lower under their VERSIONED name (`pkg_v0_1_0[...]`)
/// so two major versions of one package coexist without symbol collision —
/// the incumbent's `lower_one_user_module` versioning, replicated here.
pub fn lower_to_ir_with_deps(
    path: &str,
    source_text: &str,
    dep_paths: &[(crate::project::PkgId, std::path::PathBuf)],
) -> Result<crate::ir::IrProgram, String> {
    lower_to_ir_impl(path, source_text, dep_paths, None)
}

/// The TEST-MODE form (#2179): the same front, with the leg-independent
/// `__test_runner` synthesis (`almide_driver::test_runner`) applied where the
/// incumbent applies it — after module lowering, BEFORE `link_ir`, so the
/// runner `main` is what keeps the promoted test fns alive through DCE. This
/// is what lets `almide test --target wasm` render through the structural leg
/// with the same routing `almide build --target wasm` uses.
pub fn lower_to_ir_tests_with_deps(
    path: &str,
    source_text: &str,
    dep_paths: &[(crate::project::PkgId, std::path::PathBuf)],
    run_filter: Option<&str>,
) -> Result<crate::ir::IrProgram, String> {
    lower_to_ir_impl(path, source_text, dep_paths, Some(run_filter))
}

fn lower_to_ir_impl(
    path: &str,
    source_text: &str,
    dep_paths: &[(crate::project::PkgId, std::path::PathBuf)],
    tests: Option<Option<&str>>,
) -> Result<crate::ir::IrProgram, String> {
    let program = parse_entry(path, source_text)?;
    let resolved = resolve_modules(path, &program, ModuleSource::Disk { dep_paths })?;
    lower_resolved(path, source_text, program, resolved, tests)
}

/// An explicitly imported stdlib module (`import bytes`) lowers its `= _`
/// intrinsic declarations as `Hole` bodies. Where the self-host registry
/// implements that surface, the declaration is dropped so the call links the
/// registry's body exactly as it does when the module is auto-imported and
/// never lowered (#2747: a since-removed `bytes` writer walled `expr:Hole` behind an
/// `import bytes` and lowered without one).
fn drop_registered_intrinsic_stubs(name: &str, module: &mut crate::ir::IrModule) {
    if !crate::stdlib::is_stdlib_module(name) && !crate::stdlib::is_bundled_module(name) {
        return;
    }
    let registered = |f: &crate::ir::IrFunction| {
        let surface = format!("{name}.{}", f.name.as_str());
        almide_types::self_host_registry::self_host_runtime()
            .iter()
            .any(|(_, maps)| maps.iter().any(|(_, s)| *s == surface))
    };
    module
        .functions
        .retain(|f| !(matches!(f.body.kind, crate::ir::IrExprKind::Hole) && registered(f)));
}

/// Where the modules an entry program imports come from (#2554).
///
/// The CLI reads them off disk through the project resolver; a consumer
/// that has no filesystem — the browser playground, whose tabs are the
/// project — hands the SAME list the incumbent renderer takes, already
/// parsed: `(module name, program, is_self_import)`, leaves first, with the
/// bundled stdlib modules it needs (`almide_mir::pipeline::bundled_self_modules`
/// resolves those for a single entry) — the resolver's auto-import step does
/// not run on this form.
#[derive(Clone, Copy)]
pub enum ModuleSource<'a> {
    /// Resolve `import` declarations relative to the entry file; external
    /// packages through `dep_paths` (`resolve_imports_with_deps`).
    Disk { dep_paths: &'a [(crate::project::PkgId, std::path::PathBuf)] },
    /// Pre-parsed modules; nothing is read from disk.
    Provided(&'a [(String, crate::ast::Program, bool)]),
    /// The resolution the caller already made for this entry from disk —
    /// [`ModuleSource::Disk`]'s answer, not computed twice (#4458).
    Resolved(&'a crate::resolve::ResolvedModules),
}

/// The front's parse step: the entry program, or the wall reason.
pub(crate) fn parse_entry(path: &str, source_text: &str) -> Result<crate::ast::Program, String> {
    let tokens = crate::lexer::Lexer::tokenize(source_text);
    let mut parser = crate::parser::Parser::new(tokens).with_file(path);
    let program = parser.parse().map_err(|e| format!("parse: {e}"))?;
    if !parser.errors.is_empty() {
        return Err(format!("parse errors: {}", parser.errors.len()));
    }
    Ok(program)
}

/// The front's module step: the resolved module list for `program`.
pub(crate) fn resolve_modules(
    path: &str,
    program: &crate::ast::Program,
    modules: ModuleSource<'_>,
) -> Result<crate::resolve::ResolvedModules, String> {
    match modules {
        ModuleSource::Disk { dep_paths } => crate::resolve::resolve_imports_with_deps(path, program, dep_paths)
            .map_err(|e| format!("resolve: {e}")),
        // No package id (a provided module is project-local by construction)
        // and no source map: the map only attributes a module's own type
        // errors to its file, and a provided module has no file.
        ModuleSource::Resolved(resolved) => Ok(resolved.clone()),
        ModuleSource::Provided(list) => Ok(crate::resolve::ResolvedModules {
            modules: list.iter().map(|(n, p, s)| (n.clone(), p.clone(), None, *s)).collect(),
            sources: std::collections::HashMap::new(),
            test_modules: Default::default(),
        }),
    }
}

/// The entry program checked and lowered — no module lowering, no test
/// synthesis, no linking. Every call the source spells (UFCS and pipes
/// included) is still the `CallTarget` the checker resolved it to, which is
/// what the stdlib test-coverage ledger reads (tests/stdlib_test_coverage_test.rs).
pub fn lower_entry_unlinked(path: &str, source_text: &str) -> Result<crate::ir::IrProgram, String> {
    let mut program = parse_entry(path, source_text)?;
    let resolved = resolve_modules(path, &program, ModuleSource::Disk { dep_paths: &[] })?;
    check_and_lower_entry(path, source_text, &mut program, &resolved).map(|(_, ir)| ir)
}

/// [`lower_resolved`]'s front: canonicalize → check → lower the entry.
fn check_and_lower_entry(
    path: &str,
    source_text: &str,
    program: &mut crate::ast::Program,
    resolved: &crate::resolve::ResolvedModules,
) -> Result<(crate::check::Checker, crate::ir::IrProgram), String> {
    let canon = crate::canonicalize::canonicalize_program(
        program,
        resolved.modules.iter().map(|(n, p, _, s)| (n.as_str(), p, *s)),
    );
    let mut checker = crate::check::Checker::from_env(canon.env);
    checker.set_source(path, source_text);
    checker.diagnostics = canon.diagnostics;
    crate::resolve::refresh_module_toplets(&mut checker, &resolved.modules);
    let diagnostics = checker.infer_program(program);
    let n_errors = diagnostics.iter().filter(|d| d.level == crate::diagnostic::Level::Error).count();
    if n_errors > 0 {
        return Err(format!("type errors: {n_errors}"));
    }

    // #3286: every module's versioned name is known BEFORE the entry lowers.
    // The entry's `lib.CENTER` use-site Var takes its `module_origin` from this
    // table; registered only inside the module loop below, a dependency's
    // name arrived after the entry had already spelled the bare `lib`, and
    // the use-site never met its `almide_rt_lib_v0` declaration (var:unmapped).
    register_versioned_module_names(&mut checker, &resolved.modules);
    let ir = crate::lower::lower_program(program, &checker.env, &checker.type_map);
    Ok((checker, ir))
}

/// The front's lowering half: canonicalize → check → lower → module
/// lowering → self-host linking → `link_ir` → the shared post-link rewrites.
pub(crate) fn lower_resolved(
    path: &str,
    source_text: &str,
    program: crate::ast::Program,
    resolved: crate::resolve::ResolvedModules,
    tests: Option<Option<&str>>,
) -> Result<crate::ir::IrProgram, String> {
    let tests = match tests {
        Some(filter) => Tests::File(filter),
        None => Tests::None,
    };
    lower_resolved_as(path, source_text, program, resolved, tests).map(|(ir, _)| ir)
}

/// What [`lower_resolved_as`] synthesizes the program's `main` for.
enum Tests<'a> {
    /// A program: its own `main`.
    None,
    /// One test file's tests (`__test_runner`), filtered by `--run`.
    File(Option<&'a str>),
    /// The test run's one program (#4441): every test file a module.
    Run(Vec<almide_driver::test_runner::TestRunFile>, Option<&'a str>),
}

/// The wasm test run's one program (#4441): `files` (path, source) each
/// become a module of an empty entry — every module they reach loaded,
/// checked and lowered once — and the runner `main` runs one file's tests per
/// instance (`almide_driver::test_runner::synthesize_test_run_main`).
/// Returns the linked IR and, per file, its lowered and selected test counts.
pub fn lower_test_run_to_ir(
    files: &[(String, String)],
    dep_paths: &[(crate::project::PkgId, std::path::PathBuf)],
    run_filter: Option<&str>,
) -> Result<(crate::ir::IrProgram, Vec<(usize, usize)>), String> {
    let first = files.first().map(|(p, _)| p.clone()).ok_or("no test file")?;
    let mut modules: Vec<(String, crate::ast::Program, Option<crate::project::PkgId>, bool)> = Vec::new();
    let mut loaded = std::collections::HashSet::new();
    let mut sources = std::collections::HashMap::new();
    let mut run_files = Vec::new();
    let mut test_programs = Vec::new();
    for (k, (path, source)) in files.iter().enumerate() {
        let program = parse_entry(path, source)?;
        let resolved = resolve_modules(path, &program, ModuleSource::Disk { dep_paths })?;
        let module = format!("almide_test_file_{k}");
        let reached = resolved.modules.iter().map(|(n, ..)| crate::intern::sym(n)).collect();
        for m in resolved.modules {
            if loaded.insert(m.0.clone()) {
                modules.push(m);
            }
        }
        sources.extend(resolved.sources);
        sources.insert(module.clone(), (path.clone(), source.clone()));
        run_files.push(almide_driver::test_runner::TestRunFile { module: crate::intern::sym(&module), reached });
        test_programs.push((module, program));
    }
    let mut test_modules = std::collections::HashSet::new();
    for (name, program) in test_programs {
        test_modules.insert(name.clone());
        modules.push((name, program, None, false));
    }
    let resolved = crate::resolve::ResolvedModules { modules, sources, test_modules };
    let entry = parse_entry(&first, "")?;
    lower_resolved_as(&first, "", entry, resolved, Tests::Run(run_files, run_filter))
}

fn lower_resolved_as(
    path: &str,
    source_text: &str,
    mut program: crate::ast::Program,
    mut resolved: crate::resolve::ResolvedModules,
    tests: Tests<'_>,
) -> Result<(crate::ir::IrProgram, Vec<(usize, usize)>), String> {
    let (mut checker, mut ir) = check_and_lower_entry(path, source_text, &mut program, &resolved)?;
    // Lower every resolved module into the program before linking — the
    // incumbent's lower_one_user_module loop (src/compile_driver.rs:172-222,
    // essential steps replicated with attribution; pkg versioning is inert
    // for stdlib-only entries). Without this, calls into bundled PURE-Almide
    // modules reach the interpreter as unresolved bridge lookups.
    let sources = std::mem::take(&mut resolved.sources);
    let mut module_diags = Vec::new();
    for (name, mod_prog, pkg_id, _) in &mut resolved.modules {
        if crate::stdlib::is_stdlib_module(name) && !crate::stdlib::is_bundled_module(name) {
            continue;
        }
        // Bridge-vs-link boundary: a module containing ANY bodyless decl
        // (`= _`) is a self-host SURFACE — its implementations live behind
        // the interpreter's registry bridge, and lowering the surface would
        // shadow the bridge with garbage stubs (found by probe: string.slice
        // inside a linked stub returned the codepoint). Only fully
        // self-contained modules (every fn has a real body — url, html) are
        // lowered and linked; everything else stays bridge-resolved.
        // A bodyless `@extern(...)` decl is not a surface (#2878): it is the
        // spec's other spelling of `fn f(...) -> T = _` with an extern
        // binding, lowered to the same Hole body, and the emitter turns it
        // into a declared import (or an `extern-native` wall). Skipping its
        // module sent every call into that module's ORDINARY fns to a
        // `call:` wall.
        let has_bridge_surface = mod_prog.decls.iter().any(|d| {
            matches!(d, crate::ast::Decl::Fn { body: None, extern_attrs, .. } if extern_attrs.is_empty())
        });
        if has_bridge_surface {
            continue;
        }
        let saved_self = checker.env.self_module_name;
        if let Some(pid) = pkg_id.as_ref() {
            checker.env.self_module_name = Some(crate::intern::sym(&pid.name));
        }
        infer_module_capturing(&mut checker, name, mod_prog, &sources, &mut module_diags);
        // Dependency modules lower under their VERSIONED name so two major
        // versions of one package coexist (incumbent's lower_one_user_module
        // versioning, verbatim). Project-local modules keep their bare name.
        // (registered for every module up front: `register_versioned_module_names`).
        let versioned = versioned_module_name(name, pkg_id.as_ref());
        let self_name = checker.env.self_module_name.map(|s| s.to_string());
        let import_table_name = self_name.as_deref().unwrap_or(name);
        let (mod_table, _) = crate::import_table::build_import_table(mod_prog, Some(import_table_name), &checker.env.user_modules);
        let saved_table = std::mem::replace(&mut checker.env.import_table, mod_table);
        let mut mod_ir_module = if resolved.test_modules.contains(name.as_str()) {
            crate::lower::lower_test_module(name, mod_prog, &checker.env, &checker.type_map)
        } else {
            crate::lower::lower_module(name, mod_prog, &checker.env, &checker.type_map, versioned)
        };
        checker.env.import_table = saved_table;
        checker.env.self_module_name = saved_self;
        drop_registered_intrinsic_stubs(name, &mut mod_ir_module);
        ir.modules.push(mod_ir_module);
    }
    // #2865: a module read off disk is the package's own, whatever its key
    // spells — a `src/prim.almd` must not reach the emitter's `prim.*` floor.
    // Bundled stdlib modules are absent from `sources`.
    let package: std::collections::HashSet<String> = resolved
        .modules
        .iter()
        .map(|(name, ..)| name.clone())
        .filter(|name| sources.contains_key(name))
        .collect();
    almide_wasm::package_keys::rekey_package_modules(&mut ir, &package);
    // E109 (#4151): the same gate every other route runs, before tests are
    // synthesized and before the program is linked or emitted.
    let refused = crate::effect_value_gate::effect_value_diagnostics(&ir, path);
    if !refused.is_empty() {
        let text: Vec<String> = refused.iter().map(|d| crate::diagnostic_render::display_with_source(d, source_text).to_string()).collect();
        return Err(text.join("\n"));
    }
    let mut counts = Vec::new();
    match tests {
        Tests::None => {}
        Tests::File(run_filter) => {
            // The structural leg's in-test assert lowering (the frontend's
            // non-test abort form), then the shared runner synthesis.
            almide_driver::test_runner::desugar_test_asserts(&mut ir);
            // Spaced (#2751): this leg keeps per-module VarTables, so a module's
            // re-inits run in a fn of its own space.
            almide_driver::test_runner::synthesize_test_runner_main_spaced(&mut ir, run_filter)
                .map_err(|e| format!("tests: {e}"))?;
        }
        Tests::Run(mut files, run_filter) => {
            almide_driver::test_runner::desugar_module_test_asserts(&mut ir);
            // A test file's module may have been re-keyed above (`prim` →
            // `prim#pkg`); the runner names it as the IR now does.
            for f in &mut files {
                if let Some(m) = ir.modules.iter().find(|m| almide_wasm::package_keys::display_key(m.name.as_str()) == f.module.as_str()) {
                    f.module = m.name;
                }
                f.reached = f
                    .reached
                    .iter()
                    .filter_map(|r| ir.modules.iter().find(|m| almide_wasm::package_keys::display_key(m.name.as_str()) == r.as_str()).map(|m| m.name))
                    .collect();
            }
            counts = almide_driver::test_runner::synthesize_test_run_main(&mut ir, &files, run_filter).map_err(|e| format!("tests: {e}"))?;
        }
    }
    link_self_host(&mut ir, &mut checker, &sources);
    almide_driver::link_ir(&mut ir);
    // E112 (ADR-0030): linking stopped at the `generic_instances` budget.
    if let Some(d) = almide_driver::budget_refusal() {
        return Err(crate::diagnostic_render::display_with_source(&d, source_text).to_string());
    }
    // R-map-09: the inline map arms load an entry's key and value into two
    // locals — the protocol the registered self-hosted bodies declare — so a
    // `((k, v)) => …` callback is adapted to it here (almide_ir::entry_split).
    crate::ir::entry_split::split_entry_callbacks(&mut ir);
    // #806 step 2, on this leg too (#2319): small pure-scalar fns inline as
    // reduced expressions at their call sites. The incumbent pipeline has run
    // this since #806 (almide-mir/pipeline.rs, post-link and pre-mut-param —
    // the same position it takes here); the structural leg never did, so an
    // inner-loop `eval_a(i, j)` stayed a call wasmtime does not inline across
    // (it inlines only under `-C inlining`, which no user passes).
    almide_mir::lower::inline_small_scalar_fns(&mut ir);
    // C-132 move-mode write-back: `mut` param fns return their mutated
    // buffer and call sites assign it back — the SAME shared-IR rewrite
    // the incumbent pipeline runs post-link (almide-mir/pipeline.rs).
    // Excluded shapes keep `mutated_params` and keep walling honestly.
    crate::ir::mut_param::lower_mut_params_move_mode(&mut ir);
    // #3154: a param a closure captures and the fn writes takes a var's cell.
    almide_wasm::cells::rebind_cell_params(&mut ir);
    Ok((ir, counts))
}

/// Self-host registry LOADING (wasm-leg resolution, interp-neutral):
/// registry modules are fully-bodied almide implementations of stdlib
/// surfaces (`float.to_string` → the Dragon4 in float_to_string.almd)
/// that are never imported, so resolve never sees them. This loads
/// exactly the ones the program (transitively) calls into `ir.modules`
/// and STOPS THERE: call sites keep their surface form
/// (`Module{float, to_string}`), which the interpreter resolves through
/// its native bridge exactly as before (rewriting them broke the interp
/// leg 468 → 254 — the bridge IS its execution path), while the wasm
/// emitter resolves the surface against the loaded implementation via
/// the same registry. One IR, two sound resolutions.
fn link_self_host(
    ir: &mut crate::ir::IrProgram,
    checker: &mut crate::check::Checker,
    sources: &std::collections::HashMap<String, (String, String)>,
) {
    use std::collections::{HashMap, HashSet};

    let mut registry: HashMap<String, &'static str> = HashMap::new();
    // Each surface's implementing fn, by the self-host source that holds it.
    let mut impls: HashMap<String, (&'static str, &'static str)> = HashMap::new();
    for (src, maps) in almide_types::self_host_registry::self_host_runtime() {
        for (impl_fn, surface) in *maps {
            registry.insert((*surface).to_string(), *src);
            impls.insert((*surface).to_string(), (*src, *impl_fn));
        }
    }

    /// A display of any type that could REACH a Float formats through
    /// the linked compound form at emission. Only the exactly-known
    /// float-free scalars (and their Applied closures) are exempt —
    /// Named/record types are opaque here, so they demand conservatively
    /// (an unused splice is reachability-pruned; a missed one was the
    /// ×4 "interp-part:Float-unlinked" wall: `${b}` over `Float?`).
    fn ty_float_free(t: &crate::types::Ty) -> bool {
        use crate::types::Ty;
        match t {
            Ty::Int | Ty::Bool | Ty::String | Ty::Unit => true,
            Ty::Applied(_, args) => args.iter().all(ty_float_free),
            _ => false,
        }
    }

    fn demand_declared_reprs(t: &crate::types::Ty, out: &mut HashSet<String>) {
        if let crate::types::Ty::Named(name, _) = t
            && let Some(surface) = almide_types::stdlib_info::runtime_backed_repr_surface(name.as_str())
        {
            out.insert(surface);
        }
        for c in t.children() {
            demand_declared_reprs(c, out);
        }
    }

    fn demand_requirement(key: &str, out: &mut HashSet<String>) {
        let Some((_, method)) = key.split_once('.') else { return };
        for r in crate::canonicalize::protocols::builtin_conformance_requirements() {
            if r.requirement == method {
                out.insert(format!("{}.{}", r.module, r.function));
            }
        }
    }

    fn scan_node(e: &crate::ir::IrExpr, out: &mut HashSet<String>) {
        // A value of a runtime-backed type displays through its declared repr
        // (#4204) wherever it meets a display — bare, nested, or in an error
        // message — so its mention is the demand (an unused splice is
        // reachability-pruned).
        demand_declared_reprs(&e.ty, out);
        match &e.kind {
            crate::ir::IrExprKind::Call { target, args, .. }
            | crate::ir::IrExprKind::TailCall { target, args, .. } => {
                match target {
                    crate::ir::CallTarget::Module { module, func, .. } => {
                        // The native JSON serializer formats floats through
                        // the linked float.to_string.
                        if (module.as_str() == "json" && func.as_str() == "stringify")
                            || (module.as_str() == "value" && func.as_str() == "__stringify")
                        {
                            out.insert("float.to_string".to_string());
                        }
                        // testing.assert_contains lowers as a native arm
                        // over the linked string.contains (#2743).
                        if module.as_str() == "testing" && func.as_str() == "assert_contains" {
                            out.insert("string.contains".to_string());
                        }
                        out.insert(format!("{}.{}", module.as_str(), func.as_str()));
                    }
                    // Bare println/print display their argument — the
                    // same implicit float demand as interpolation.
                    crate::ir::CallTarget::Named { name }
                        if matches!(name.as_str(), "println" | "print" | "eprintln")
                            && args.iter().any(|a| !ty_float_free(&a.ty)) =>
                    {
                        out.insert("float.to_string_compound".to_string());
                        out.insert("float32.to_string_compound".to_string());
                    }
                    // Codec splices call their registry helpers by BARE
                    // dunder name — the demand key IS the name.
                    crate::ir::CallTarget::Named { name } if name.as_str().starts_with("__") => {
                        out.insert(name.as_str().to_string());
                    }
                    // A protocol requirement reached through a type variable
                    // (`T.__join` in `list.join[T: Text]`) becomes a call to
                    // some conforming type's `<module>.<requirement>` only at
                    // monomorphization, after this scan: demand the
                    // requirement of every built-in conformer (an unused load
                    // is reachability-pruned).
                    crate::ir::CallTarget::Named { name } if name.as_str().contains('.') => {
                        demand_requirement(name.as_str(), out);
                    }
                    crate::ir::CallTarget::Method { method, .. } if method.as_str().contains('.') => {
                        demand_requirement(method.as_str(), out);
                    }
                    _ => {}
                }
            }
            // `**` on floats lowers to the LINKED vendored pow — the
            // demand is implicit in the operator.
            crate::ir::IrExprKind::BinOp { op: crate::ir::BinOp::PowFloat, .. } => {
                out.insert("math.__fpow".to_string());
            }
            // `%` on floats lowers to the LINKED exact remainder (wasm has no
            // float rem instruction, #3080).
            crate::ir::IrExprKind::BinOp { op: crate::ir::BinOp::ModFloat, .. } => {
                out.insert("float.fmod".to_string());
            }
            // A Float-reaching interpolation part formats through
            // float.to_string at emission — the demand is implicit.
            crate::ir::IrExprKind::StringInterp { parts } => {
                for p in parts {
                    if let crate::ir::IrStringPart::Expr { expr } = p
                        && !ty_float_free(&expr.ty)
                    {
                        out.insert("float.to_string_compound".to_string());
                        // A Float32, top-level or nested, prints through
                        // the f32 Schubfach (C-372).
                        out.insert("float32.to_string_compound".to_string());
                    }
                }
            }
            _ => {}
        }
    }
    /// One owned walk of a body: its demands (`scan_node`), whether an error
    /// type of it can reach a Float (`err_float`, below), and — for a module
    /// fn — its bare calls to its own module's fns as the surface
    /// `<module>.<fn>` (`__address(b)` inside stdlib/ffi.almd, a `local`
    /// intrinsic, demands `ffi.__address`, so a registered self-host body for
    /// it is loaded — ADR-0031). `refs` collects the fn VALUES and unresolved
    /// method names the body names, which reach fns without demanding a
    /// surface.
    ///
    /// The walk reads the body in place ([`BodyWalk`]): cloning at every
    /// level once copied each subtree once per ancestor — O(size x depth)
    /// (#4289) — and cloning each body once per round still cost a copy of
    /// the program per round (#4458).
    ///
    /// An error value whose type can reach a Float is displayed at emission
    /// wherever it meets a String channel: `main`'s abort renders it through
    /// its repr (#3470, `emit_abort_message`), and a `!` from a
    /// `Result[_, Float32]` into an effect fn's String error formats it. Any
    /// Result in a body with such an err type is therefore a display demand,
    /// the same as `${e}` (an unused splice is reachability-pruned). Before
    /// #4071 the demand was met by accident — every auto-imported module was
    /// lowered into every program, and their bodies demanded the formatter.
    struct BodyScan<'a> {
        module: Option<&'a str>,
        keys: HashSet<String>,
        refs: HashSet<String>,
        err_float: bool,
    }
    /// A body's walk for [`BodyScan`].
    struct BodyWalk<'s, 'a> {
        s: &'s mut BodyScan<'a>,
    }
    impl almide_ir::visit::IrVisitor for BodyWalk<'_, '_> {
        fn visit_expr(&mut self, e: &crate::ir::IrExpr) {
            scan_body_node(e, self.s);
            almide_ir::visit::walk_expr(self, e);
        }
    }
    fn scan_body_node(e: &crate::ir::IrExpr, s: &mut BodyScan<'_>) {
        use crate::ir::{CallTarget, IrExprKind};
        scan_node(e, &mut s.keys);
        s.err_float |= matches!(
            &e.ty,
            crate::types::Ty::Applied(crate::types::TypeConstructorId::Result, a)
                if a.len() == 2 && !ty_float_free(&a[1]) && !a[1].is_unresolved()
        );
        match &e.kind {
            IrExprKind::Call { target: CallTarget::Named { name }, .. }
            | IrExprKind::TailCall { target: CallTarget::Named { name }, .. }
                if !name.as_str().contains('.') =>
            {
                if let Some(m) = s.module {
                    s.keys.insert(format!("{m}.{}", name.as_str()));
                }
                s.refs.insert(name.as_str().to_string());
            }
            IrExprKind::Call { target: CallTarget::Method { method, .. }, .. }
            | IrExprKind::TailCall { target: CallTarget::Method { method, .. }, .. } => {
                s.refs.insert(method.as_str().to_string());
            }
            IrExprKind::FnRef { name } => {
                s.refs.insert(name.as_str().to_string());
                if let Some(m) = s.module.filter(|_| !name.as_str().contains('.')) {
                    s.refs.insert(format!("{m}.{}", name.as_str()));
                }
            }
            _ => {}
        }
    }
    /// The demands of every body the program can REACH (#4317). The roots
    /// are the entry program (its fns and top-level lets), every module's
    /// top-level lets, and every fn of a module that is neither a bundled
    /// stdlib module nor a linked self-host body. A library fn is scanned
    /// once something scanned names it: a call `<module>.<fn>`, a sibling's
    /// bare call, a fn value, an unresolved method of that name (any module
    /// declaring it), or a demanded surface whose loaded self-host body is
    /// it. Scanning every fn of every loaded module instead made a module's
    /// unused fns demand their callees' modules and self-host bodies, which
    /// demanded theirs: `int.to_string` linked 63 self-host bodies and the
    /// sized-int, list and string modules into a three-function program,
    /// tripling every wasm compile. An unscanned fn is never reached, so the
    /// emitter's reachability prunes it; a demand it alone would have made is
    /// never needed.
    fn scan_program(
        ir: &crate::ir::IrProgram,
        impls: &HashMap<String, (&'static str, &'static str)>,
        loaded: &HashMap<&'static str, String>,
        out: &mut HashSet<String>,
    ) {
        let library = |m: &str| m.starts_with("__selfhost_") || crate::stdlib::is_bundled_module(m);
        let mut by_key: HashMap<String, (usize, usize)> = HashMap::new();
        let mut by_name: HashMap<&str, Vec<(usize, usize)>> = HashMap::new();
        let mut work: Vec<(usize, usize)> = Vec::new();
        for (mi, m) in ir.modules.iter().enumerate() {
            for (fi, f) in m.functions.iter().enumerate() {
                by_key.insert(format!("{}.{}", m.name.as_str(), f.name.as_str()), (mi, fi));
                by_name.entry(f.name.as_str()).or_default().push((mi, fi));
                if !library(m.name.as_str()) {
                    work.push((mi, fi));
                }
            }
        }
        let mut err_float = false;
        let mut absorb = |s: BodyScan<'_>, work: &mut Vec<(usize, usize)>, out: &mut HashSet<String>| {
            err_float |= s.err_float;
            for k in s.keys.iter().chain(s.refs.iter()) {
                work.extend(by_key.get(k).copied());
                if let Some((src, impl_fn)) = impls.get(k)
                    && let Some(module) = loaded.get(src)
                {
                    work.extend(by_key.get(&format!("{module}.{impl_fn}")).copied());
                }
            }
            // A `value.*` surface the emitter lowers natively calls a
            // self-host helper no IR names (value.object boxes through
            // `__vu_fold_pairs`, C-401; value.merge merges through
            // `__vu_merge_pairs`): demand the helpers' source and, once it is
            // loaded, walk the helper's body like any reached fn.
            let mut helper_demands = Vec::new();
            for (surface, helper) in almide_wasm::VALUE_SELFHOST_HELPERS {
                if !s.keys.contains(*surface) {
                    continue;
                }
                helper_demands.push(almide_wasm::VALUE_HELPER_ANCHOR.to_string());
                if let Some((src, _)) = impls.get(almide_wasm::VALUE_HELPER_ANCHOR)
                    && let Some(module) = loaded.get(src)
                {
                    work.extend(by_key.get(&format!("{module}.{helper}")).copied());
                }
            }
            out.extend(helper_demands);
            for r in &s.refs {
                let bare = r.rsplit('.').next().unwrap_or(r);
                work.extend(by_name.get(bare).into_iter().flatten().copied());
            }
            for k in s.keys.iter().filter(|k| !k.contains('.')) {
                work.extend(by_name.get(k.as_str()).into_iter().flatten().copied());
            }
            out.extend(s.keys);
        };
        let scan = |e: &crate::ir::IrExpr, module: Option<&str>| {
            let mut s = BodyScan { module, keys: HashSet::new(), refs: HashSet::new(), err_float: false };
            almide_ir::visit::IrVisitor::visit_expr(&mut BodyWalk { s: &mut s }, e);
            BodyScan { module: None, keys: s.keys, refs: s.refs, err_float: s.err_float }
        };
        for f in &ir.functions {
            absorb(scan(&f.body, None), &mut work, out);
        }
        for tl in &ir.top_lets {
            absorb(scan(&tl.value, None), &mut work, out);
        }
        for m in &ir.modules {
            for tl in &m.top_lets {
                absorb(scan(&tl.value, Some(m.name.as_str())), &mut work, out);
            }
        }
        let mut seen: HashSet<(usize, usize)> = HashSet::new();
        while let Some((mi, fi)) = work.pop() {
            if !seen.insert((mi, fi)) {
                continue;
            }
            let m = &ir.modules[mi];
            absorb(scan(&m.functions[fi].body, Some(m.name.as_str())), &mut work, out);
        }
        if err_float {
            out.insert("float.to_string_compound".to_string());
            out.insert("float32.to_string_compound".to_string());
        }
    }

    /// Every surface a lowered body names, REACHED OR NOT (#4458): what
    /// [`scan_node`] demands of every expression of every function and
    /// top-level `let`, and a module fn's bare calls as `<module>.<fn>`.
    fn mentioned_surfaces(ir: &crate::ir::IrProgram, only: Option<&HashSet<String>>, out: &mut HashSet<String>) {
        use almide_ir::visit::{walk_expr, IrVisitor};
        struct Mentions<'a> {
            module: Option<&'a str>,
            out: &'a mut HashSet<String>,
        }
        impl IrVisitor for Mentions<'_> {
            fn visit_expr(&mut self, e: &crate::ir::IrExpr) {
                use crate::ir::{CallTarget, IrExprKind};
                scan_node(e, self.out);
                if let crate::types::Ty::Applied(crate::types::TypeConstructorId::Result, a) = &e.ty
                    && a.len() == 2
                    && !ty_float_free(&a[1])
                    && !a[1].is_unresolved()
                {
                    self.out.insert("float.to_string_compound".to_string());
                    self.out.insert("float32.to_string_compound".to_string());
                }
                if let (Some(m), IrExprKind::Call { target: CallTarget::Named { name }, .. } | IrExprKind::TailCall { target: CallTarget::Named { name }, .. }) =
                    (self.module, &e.kind)
                    && !name.as_str().contains('.')
                {
                    self.out.insert(format!("{m}.{}", name.as_str()));
                }
                walk_expr(self, e);
            }
        }
        let mut walk = |module: Option<&str>, e: &crate::ir::IrExpr| Mentions { module, out: &mut *out }.visit_expr(e);
        if only.is_none() {
            for f in &ir.functions {
                walk(None, &f.body);
            }
            for tl in &ir.top_lets {
                walk(None, &tl.value);
            }
        }
        for m in ir.modules.iter().filter(|m| only.is_none_or(|o| o.contains(m.name.as_str()))) {
            for f in &m.functions {
                walk(Some(m.name.as_str()), &f.body);
            }
            for tl in &m.top_lets {
                walk(Some(m.name.as_str()), &tl.value);
            }
        }
    }

    // Each self-host source's (surface, impl fn) rows.
    let mut by_src: HashMap<*const u8, Vec<(&String, &'static str)>> = HashMap::new();
    for (surface, (src, impl_fn)) in &impls {
        by_src.entry(src.as_ptr()).or_default().push((surface, impl_fn));
    }
    let mut loaded: HashMap<&'static str, String> = HashMap::new();
    // #4458: the functions each linked self-host source carries. A source
    // links only what the round's demands reach inside it (`reach_within`),
    // never its whole body: `value.__field` alone once checked and lowered
    // 139 functions of value_utils.almd for a program that calls three.
    let mut linked_fns: HashMap<&'static str, HashSet<String>> = HashMap::new();
    let mut module_diags = Vec::new();
    // Each missing bundled function is relowered for once (a surface the
    // module does not declare must not loop the link).
    let mut tried: HashSet<String> = HashSet::new();
    // The modules of the built-in types no literal produces (`int8`,
    // `bytes`, …): their requirement functions are the dispatch-only demands.
    let spelled_only_modules: HashSet<&'static str> = crate::canonicalize::protocols::BUILTIN_PROTOCOLS
        .iter()
        .flat_map(|p| p.conformers.iter())
        .filter(|ty| almide_frontend::bundled_reach::value_needs_spelling(ty))
        .filter_map(|ty| ty.stdlib_module())
        .collect();
    let requirement_keys: HashSet<String> = crate::canonicalize::protocols::builtin_conformance_requirements()
        .into_iter()
        .filter(|r| spelled_only_modules.contains(r.module))
        .map(|r| format!("{}.{}", r.module, r.function))
        .collect();
    // A natively lowered surface demands what the emitter resolves for it.
    let imply = |keys: &mut HashSet<String>| {
        for (lowered, resolved) in almide_wasm::EMITTER_IMPLIED_SURFACES {
            if keys.contains(*lowered) {
                keys.insert(resolved.to_string());
            }
        }
    };
    // What the lowered bodies mention only grows: a round walks just the
    // modules loaded or relowered since the last one (`None`: everything).
    let mut mentioned: HashSet<String> = HashSet::new();
    let mut fresh: Option<HashSet<String>> = None;
    loop {
        let mut needed = HashSet::new();
        scan_program(ir, &impls, &loaded, &mut needed);
        imply(&mut needed);
        // Deterministic load order: module indices, names, and layouts
        // must not depend on hash-seed iteration order.
        let mut needed: Vec<String> = needed.into_iter().collect();
        needed.sort();
        // ALMIDE_DBG_LINK=1: dump the demand set and what each key resolves
        // to — the probe that caught #1675's missed __err_at demand.
        if almide_base::env::flag("ALMIDE_DBG_LINK") {
            for n in &needed {
                eprintln!("[link] demand {} -> {}", n, registry.get(n).map(|_| "registered").unwrap_or("UNREGISTERED"));
            }
        }
        let mut grew = false;
        // A splice body links after module resolution, so a call it makes
        // into an Almide-bodied fn of a bundled module the entry never reached
        // (`bytes.__f16_bits` from matrix's f16 writer) has no lowered body:
        // load that module now, by the same steps `lower_resolved` takes. The
        // resolver keeps only the functions the program's sources reach
        // (#4328), so a module that IS loaded may still lack the one a splice
        // body or the emitter calls (`string.__err_text`'s body calls
        // `list.join`): such a module is lowered again with the missing
        // functions and what they reach inside it (#4458), and replaces its
        // pruned lowering.
        let mut missing: Vec<(String, HashSet<String>)> = Vec::new();
        for surface in &needed {
            if registry.contains_key(surface) {
                continue;
            }
            let Some((module, func)) = surface.split_once('.') else { continue };
            if !crate::stdlib::is_bundled_module(module) {
                continue;
            }
            let present = ir.modules.iter().find(|m| m.name.as_str() == module);
            let has_fn = present.is_some_and(|m| m.functions.iter().any(|f| f.name.as_str() == func));
            // A requirement a type variable dispatches to (`A.__sum` ->
            // `int8.__sum`) is demanded of every built-in conformer; the
            // dispatch lands in a module only when a value of its type exists.
            // No literal makes a sized number or `Bytes`, so a program that
            // never reached such a module holds no value of its type (#4458).
            let dispatch_only = present.is_none() && requirement_keys.contains(surface.as_str());
            if has_fn || dispatch_only || !tried.insert(surface.clone()) {
                continue;
            }
            match missing.iter_mut().find(|(m, _)| m == module) {
                Some((_, fs)) => {
                    fs.insert(func.to_string());
                }
                None => missing.push((module.to_string(), HashSet::from([func.to_string()]))),
            }
        }
        for (module, funcs) in missing {
            let module = module.as_str();
            let present = ir.modules.iter().position(|m| m.name.as_str() == module);
            let Some(src) = almide_types::stdlib_info::bundled_source(module) else { continue };
            let Some(whole) = almide_lang::parse_cached(src) else { continue };
            let has_bridge_surface = whole.decls.iter().any(|d| {
                matches!(d, crate::ast::Decl::Fn { body: None, extern_attrs, .. } if extern_attrs.is_empty())
            });
            if has_bridge_surface {
                continue;
            }
            let mut roots = funcs;
            match present {
                Some(i) => roots.extend(ir.modules[i].functions.iter().map(|f| f.name.to_string())),
                None => roots.extend(almide_frontend::bundled_reach::compiler_called_fns(module, whole)),
            }
            let keep = almide_frontend::bundled_reach::reach_within(module, whole, &roots);
            let mut mod_prog = whole.clone_retaining(|d| match d {
                crate::ast::Decl::Fn { name, .. } => keep.contains(name.as_str()),
                _ => true,
            });
            infer_module_capturing(checker, module, &mut mod_prog, sources, &mut module_diags);
            let (mod_table, _) =
                crate::import_table::build_import_table(&mod_prog, Some(module), &checker.env.user_modules);
            let saved_table = std::mem::replace(&mut checker.env.import_table, mod_table);
            let mut mod_ir = crate::lower::lower_module(module, &mod_prog, &checker.env, &checker.type_map, None);
            checker.env.import_table = saved_table;
            drop_registered_intrinsic_stubs(module, &mut mod_ir);
            if let Some(f) = fresh.as_mut() {
                f.insert(module.to_string());
            }
            match present {
                Some(i) => ir.modules[i] = mod_ir,
                None => ir.modules.push(mod_ir),
            }
            grew = true;
        }
        // The self-host sources this round links, in first-demand order
        // (their `__selfhost_N` names follow it), then every source already
        // linked (by name): a body lowered since may name more of it.
        mentioned_surfaces(ir, fresh.as_ref(), &mut mentioned);
        imply(&mut mentioned);
        fresh = Some(HashSet::new());
        let mut order: Vec<&'static str> = Vec::new();
        // A surface the emitter resolves on its own links its source as soon
        // as any lowered body, reached or not, names it (after the round's
        // demands, so their order still names the sources): the emitter's
        // reach is wider than the scan's.
        let mut emitter_own: Vec<String> = almide_wasm::EMITTER_IMPLIED_SURFACES
            .iter()
            .map(|(_, resolved)| resolved.to_string())
            .filter(|s| mentioned.contains(s) && !needed.contains(s))
            .collect();
        emitter_own.sort();
        emitter_own.dedup();
        for surface in needed.iter().chain(emitter_own.iter()) {
            if let Some(&(src, _)) = impls.get(surface)
                && !order.iter().any(|s| std::ptr::eq(*s, src))
            {
                order.push(src);
            }
        }
        let mut earlier: Vec<(&'static str, &String)> =
            loaded.iter().filter(|(s, _)| !order.iter().any(|o| std::ptr::eq(*o, **s))).map(|(s, n)| (*s, n)).collect();
        earlier.sort_by(|a, b| a.1.cmp(b.1));
        order.extend(earlier.into_iter().map(|(s, _)| s));
        let mut wanted: HashSet<String> = mentioned.clone();
        wanted.extend(needed.iter().cloned());
        // Each carries (#4458) the implementation of every surface the round
        // demands, every surface any lowered body names — reached or not:
        // the emitter's reach is wider than the scan's (a protocol
        // requirement it dispatches) — with what the emitter resolves on its
        // own for them (`EMITTER_IMPLIED_SURFACES`, `imply` above), the value
        // helpers it calls by name when their anchor is carried, and what
        // those reach inside the source (`reach_within`). It never carries a
        // function nothing can resolve, which loading the whole source did,
        // and a source none of whose demanded implementations the emitter
        // admits is not linked.
        let demanded: Vec<(&'static str, HashSet<String>)> = order
            .into_iter()
            .map(|src| {
                let mut roots = linked_fns.get(src).cloned().unwrap_or_default();
                for (surface, impl_fn) in by_src.get(&src.as_ptr()).into_iter().flatten() {
                    if !wanted.contains(*surface) {
                        continue;
                    }
                    // Only a whitelisted implementation is ever called through
                    // its surface (`registry_impl_admitted`): the emitter lowers
                    // every other one's surface itself, or walls.
                    if almide_wasm::registry_impl_admitted(impl_fn) {
                        roots.insert(impl_fn.to_string());
                    }
                    if surface.as_str() == almide_wasm::VALUE_HELPER_ANCHOR {
                        roots.extend(almide_wasm::VALUE_SELFHOST_HELPERS.iter().map(|(_, h)| h.to_string()));
                    }
                }
                (src, roots)
            })
            .collect();
        for (src, mut roots) in demanded {
            let present = loaded.get(src).cloned();
            if present.is_some() && roots.iter().all(|f| linked_fns.get(src).is_some_and(|l| l.contains(f))) {
                continue;
            }
            // Nothing in it can be called: it is not linked at all.
            if present.is_none() && roots.is_empty() {
                continue;
            }
            let name = present.unwrap_or_else(|| format!("__selfhost_{}", loaded.len() + 1));
            loaded.insert(src, name.clone());
            let Some(whole) = almide_lang::parse_cached(src) else { continue };
            // Only fully-bodied implementations may load — a bodyless
            // decl is a bridge surface, not an implementation.
            let bodyless = whole.decls.iter().any(|d| matches!(d, crate::ast::Decl::Fn { body: None, .. }));
            if bodyless {
                continue;
            }
            // Convention methods (`Type.method`) resolve by name suffix at
            // emission, never through a demand: they stay.
            roots.extend(whole.decls.iter().filter_map(|d| match d {
                crate::ast::Decl::Fn { name, .. } if name.as_str().contains('.') => Some(name.to_string()),
                _ => None,
            }));
            let keep = almide_frontend::bundled_reach::reach_within("", whole, &roots);
            // A demanded name the source does not declare reaches nothing new.
            if linked_fns.get(src) == Some(&keep) {
                continue;
            }
            let mut mod_prog = whole.clone_retaining(|d| match d {
                crate::ast::Decl::Fn { name, .. } => keep.contains(name.as_str()),
                _ => true,
            });
            if almide_base::env::flag("ALMIDE_DBG_LINK") {
                let mut fns: Vec<&String> = keep.iter().collect();
                fns.sort();
                eprintln!("[link] {name} carries {fns:?}");
            }
            linked_fns.insert(src, keep);
            infer_module_capturing(
                checker,
                &name,
                &mut mod_prog,
                sources,
                &mut module_diags,
            );
            // The same import-table dance the resolved-modules loop does:
            // lowering against the ENTRY's import table leaves the
            // module's own references as holes (found by the burn-up:
            // expr:Hole ×72 when this was skipped).
            let (mod_table, _) = crate::import_table::build_import_table(
                &mod_prog,
                Some(&name),
                &checker.env.user_modules,
            );
            let saved_table =
                std::mem::replace(&mut checker.env.import_table, mod_table);
            let mod_ir = crate::lower::lower_module(
                &name,
                &mod_prog,
                &checker.env,
                &checker.type_map,
                None,
            );
            checker.env.import_table = saved_table;
            if let Some(f) = fresh.as_mut() {
                f.insert(name.clone());
            }
            match ir.modules.iter().position(|m| m.name.as_str() == name) {
                Some(i) => ir.modules[i] = mod_ir,
                None => ir.modules.push(mod_ir),
            }
            grew = true;
        }
        if !grew {
            break;
        }
    }
}

/// The name a resolved module lowers under: a dependency module's
/// `pkg_id`-derived versioned name (`<pkg mod_name><suffix>` for a
/// submodule), `None` for a project-local module, which keeps its bare name.
pub fn versioned_module_name(name: &str, pkg_id: Option<&crate::project::PkgId>) -> Option<String> {
    pkg_id.map(|pid| {
        let base = pid.mod_name();
        match name.strip_prefix(&pid.name) {
            Some(suffix) => format!("{base}{suffix}"),
            None => base,
        }
    })
}

/// Register every resolved module's versioned name before ANY program is
/// lowered — the entry's (and each module's) `mod.NAME` use-site resolves its
/// origin through this table (`module_top_let_var`). The one copy every
/// driver calls: the native driver, both incumbent wasm paths and this leg.
pub fn register_versioned_module_names(
    checker: &mut crate::check::Checker,
    modules: &[(String, crate::ast::Program, Option<crate::project::PkgId>, bool)],
) {
    for (name, _, pkg_id, _) in modules {
        if let Some(v) = versioned_module_name(name, pkg_id.as_ref()) {
            checker.env.module_versioned_names.insert(crate::intern::sym(name), crate::intern::sym(&v));
        }
    }
}

/// One checker pass over a module with diagnostics captured against the
/// module's OWN source (not the entry file's). The single copy — the
/// compile driver and almide-spine's s3 both delegate here.
pub fn infer_module_capturing(
    checker: &mut crate::check::Checker,
    name: &str,
    mod_prog: &mut crate::ast::Program,
    sources: &std::collections::HashMap<String, (String, String)>,
    out: &mut Vec<(String, String, Vec<crate::diagnostic::Diagnostic>)>,
) {
    let Some((path, text)) = sources.get(name) else {
        // Bundled stdlib: compiled in, no user file. The flag is the module's
        // ORIGIN for E085 (its `@intrinsic`s are the runtime boundary, not a
        // user declaration) — the entry file's path is still in
        // `source_file` and must not be judged.
        //
        // Its E006s are reported, against the embedded source. Every
        // bundled diagnostic used to be dropped ("no user file to blame"),
        // and that hid a bundled module breaking the effect rule the checker
        // enforces on everyone (#848: an `args` plain fn read argv).
        // An E006 is context-free once the callee resolves (it reads only
        // the callee's `effect` marker), so a bundled one is a stdlib bug
        // and fails the build that reaches it; tests/bundled_module_effect_
        // isolation_test.rs fails CI before that. The OTHER codes stay
        // dropped: a bundled module inferred here is not in the context it
        // was written for, and measured on 2026-10-07 `http` reports 48
        // errors (its self-qualified `http.*` calls and types) and `json` 1
        // (a runtime-backed type) that are artifacts of that context, not defects. A
        // cross-module call the checker cannot resolve here (args.almd had no
        // `import env`, so `env.args` was E003, not E006) is caught by the
        // context-free rule in scripts/check-cap-effect-consistency.sh.
        let saved_bundled = checker.in_bundled_module;
        checker.in_bundled_module = true;
        let before = checker.diagnostics.len();
        checker.infer_module(mod_prog, name);
        checker.in_bundled_module = saved_bundled;
        let file = format!("<bundled stdlib>/{name}.almd");
        let e006: Vec<crate::diagnostic::Diagnostic> = checker.diagnostics[before..]
            .iter()
            .filter(|d| d.level == crate::diagnostic::Level::Error && d.code == Some("E006"))
            .map(|d| crate::diagnostic::Diagnostic { file: Some(file.clone()), ..d.clone() })
            .collect();
        if !e006.is_empty() {
            let text = crate::stdlib_info::bundled_source(name).unwrap_or("");
            out.push((file, text.to_string(), e006));
        }
        return;
    };
    let saved_file = checker.source_file.clone();
    let saved_text = checker.source_text.clone();
    let before = checker.diagnostics.len();
    checker.set_source(path, text);
    checker.infer_module(mod_prog, name);
    let produced: Vec<crate::diagnostic::Diagnostic> = checker.diagnostics[before..].to_vec();
    checker.source_file = saved_file;
    checker.source_text = saved_text;
    if !produced.is_empty() {
        out.push((path.clone(), text.clone(), produced));
    }
}
