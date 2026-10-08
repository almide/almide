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
/// never lowered (#2747: `bytes.write_uint16` walled `expr:Hole` behind an
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
        ModuleSource::Provided(list) => Ok(crate::resolve::ResolvedModules {
            modules: list.iter().map(|(n, p, s)| (n.clone(), p.clone(), None, *s)).collect(),
            sources: std::collections::HashMap::new(),
        }),
    }
}

/// The front's lowering half: canonicalize → check → lower → module
/// lowering → self-host linking → `link_ir` → the shared post-link rewrites.
pub(crate) fn lower_resolved(
    path: &str,
    source_text: &str,
    mut program: crate::ast::Program,
    mut resolved: crate::resolve::ResolvedModules,
    tests: Option<Option<&str>>,
) -> Result<crate::ir::IrProgram, String> {
    let canon = crate::canonicalize::canonicalize_program(
        &program,
        resolved.modules.iter().map(|(n, p, _, s)| (n.as_str(), p, *s)),
    );
    let mut checker = crate::check::Checker::from_env(canon.env);
    checker.set_source(path, source_text);
    checker.diagnostics = canon.diagnostics;
    crate::resolve::refresh_module_toplets(&mut checker, &resolved.modules);
    let diagnostics = checker.infer_program(&mut program);
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
    let mut ir = crate::lower::lower_program(&program, &checker.env, &checker.type_map);
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
        let mut mod_ir_module = crate::lower::lower_module(name, mod_prog, &checker.env, &checker.type_map, versioned);
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
    if let Some(run_filter) = tests {
        // The structural leg's in-test assert lowering (the frontend's
        // non-test abort form), then the shared runner synthesis.
        almide_driver::test_runner::desugar_test_asserts(&mut ir);
        // Spaced (#2751): this leg keeps per-module VarTables, so a module's
        // re-inits run in a fn of its own space.
        almide_driver::test_runner::synthesize_test_runner_main_spaced(&mut ir, run_filter)
            .map_err(|e| format!("tests: {e}"))?;
    }
    link_self_host(&mut ir, &mut checker, &sources);
    almide_driver::link_ir(&mut ir);
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
    Ok(ir)
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
    for (src, maps) in almide_types::self_host_registry::self_host_runtime() {
        for (_impl_fn, surface) in *maps {
            registry.insert((*surface).to_string(), *src);
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

    fn scan_expr(e: &crate::ir::IrExpr, out: &mut HashSet<String>) {
        match &e.kind {
            crate::ir::IrExprKind::Call { target, args, .. }
            | crate::ir::IrExprKind::TailCall { target, args, .. } => {
                match target {
                    crate::ir::CallTarget::Module { module, func, .. } => {
                        // The native JSON serializer formats floats through
                        // the linked float.to_string.
                        if (module.as_str() == "json" || module.as_str() == "value")
                            && func.as_str() == "stringify"
                        {
                            out.insert("float.to_string".to_string());
                        }
                        // string.from_bytes lowers as from_list ∘ the
                        // linked lossy decoder (same WHATWG algorithm).
                        if module.as_str() == "string" && func.as_str() == "from_bytes" {
                            out.insert("bytes.to_string_lossy".to_string());
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
                    _ => {}
                }
            }
            // `**` on floats lowers to the LINKED vendored pow — the
            // demand is implicit in the operator.
            crate::ir::IrExprKind::BinOp { op: crate::ir::BinOp::PowFloat, .. } => {
                out.insert("math.fpow".to_string());
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
        e.clone().map_children(&mut |c| {
            scan_expr(&c, out);
            c
        });
    }
    fn scan_program(ir: &crate::ir::IrProgram, out: &mut HashSet<String>) {
        for f in &ir.functions {
            scan_expr(&f.body, out);
        }
        for tl in &ir.top_lets {
            scan_expr(&tl.value, out);
        }
        for m in &ir.modules {
            for f in &m.functions {
                scan_expr(&f.body, out);
            }
            for tl in &m.top_lets {
                scan_expr(&tl.value, out);
            }
        }
    }

    let mut loaded: HashSet<&'static str> = HashSet::new();
    let mut module_diags = Vec::new();
    loop {
        let mut needed = HashSet::new();
        scan_program(ir, &mut needed);
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
        for surface in needed {
            let Some(&src) = registry.get(&surface) else { continue };
            if loaded.contains(&src) {
                continue;
            }
            loaded.insert(src);
            let name = format!("__selfhost_{}", loaded.len());
            let tokens = crate::lexer::Lexer::tokenize(src);
            let mut parser = crate::parser::Parser::new(tokens).with_file(&name);
            let Ok(mut mod_prog) = parser.parse() else { continue };
            if !parser.errors.is_empty() {
                continue;
            }
            // Only fully-bodied implementations may load — a bodyless
            // decl is a bridge surface, not an implementation.
            let bodyless = mod_prog
                .decls
                .iter()
                .any(|d| matches!(d, crate::ast::Decl::Fn { body: None, .. }));
            if bodyless {
                continue;
            }
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
            ir.modules.push(mod_ir);
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
        // enforces on everyone (#848: `args.flag`, a plain fn, read argv).
        // An E006 is context-free once the callee resolves (it reads only
        // the callee's `effect` marker), so a bundled one is a stdlib bug
        // and fails the build that reaches it; tests/bundled_module_effect_
        // isolation_test.rs fails CI before that. The OTHER codes stay
        // dropped: a bundled module inferred here is not in the context it
        // was written for, and measured on 2026-10-07 `http` reports 48
        // errors (its self-qualified `http.*` calls and types) and `json` 1
        // (`JsonPath`) that are artifacts of that context, not defects. A
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
