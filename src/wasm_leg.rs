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
    let tokens = crate::lexer::Lexer::tokenize(source_text);
    let mut parser = crate::parser::Parser::new(tokens).with_file(path);
    let mut program = parser.parse().map_err(|e| format!("parse: {e}"))?;
    if !parser.errors.is_empty() {
        return Err(format!("parse errors: {}", parser.errors.len()));
    }

    let mut resolved = crate::resolve::resolve_imports_with_deps(path, &program, dep_paths)
        .map_err(|e| format!("resolve: {e}"))?;

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
        let has_bodyless = mod_prog.decls.iter().any(|d| matches!(d, crate::ast::Decl::Fn { body: None, .. }));
        if has_bodyless {
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
        let versioned = pkg_id.as_ref().map(|pid| {
            let base = pid.mod_name();
            match name.strip_prefix(&pid.name) {
                Some(suffix) => format!("{}{}", base, suffix),
                None => base,
            }
        });
        if let Some(ref v) = versioned {
            checker.env.module_versioned_names.insert(crate::intern::sym(name), crate::intern::sym(v));
        }
        let self_name = checker.env.self_module_name.map(|s| s.to_string());
        let import_table_name = self_name.as_deref().unwrap_or(name);
        let (mod_table, _) = crate::import_table::build_import_table(mod_prog, Some(import_table_name), &checker.env.user_modules);
        let saved_table = std::mem::replace(&mut checker.env.import_table, mod_table);
        let mod_ir_module = crate::lower::lower_module(name, mod_prog, &checker.env, &checker.type_map, versioned);
        checker.env.import_table = saved_table;
        checker.env.self_module_name = saved_self;
        ir.modules.push(mod_ir_module);
    }
    link_self_host(&mut ir, &mut checker, &sources);
    almide_driver::link_ir(&mut ir);
    // C-132 move-mode write-back: `mut` param fns return their mutated
    // buffer and call sites assign it back — the SAME shared-IR rewrite
    // the incumbent pipeline runs post-link (almide-mir/pipeline.rs).
    // Excluded shapes keep `mutated_params` and keep walling honestly.
    crate::ir::mut_param::lower_mut_params_move_mode(&mut ir);
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
                        out.insert(format!("{}.{}", module.as_str(), func.as_str()));
                    }
                    // Bare println/print display their argument — the
                    // same implicit float demand as interpolation.
                    crate::ir::CallTarget::Named { name }
                        if matches!(name.as_str(), "println" | "print" | "eprintln")
                            && args.iter().any(|a| !ty_float_free(&a.ty)) =>
                    {
                        out.insert("float.to_string_compound".to_string());
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
            // A Float-reaching interpolation part formats through
            // float.to_string at emission — the demand is implicit.
            crate::ir::IrExprKind::StringInterp { parts } => {
                for p in parts {
                    if let crate::ir::IrStringPart::Expr { expr } = p
                        && !ty_float_free(&expr.ty)
                    {
                        out.insert("float.to_string_compound".to_string());
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
        // Bundled stdlib: compiled in and CI-gated, no user file to blame.
        checker.infer_module(mod_prog, name);
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
