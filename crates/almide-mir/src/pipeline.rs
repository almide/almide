//! v1 execution pipeline: a real `.almd` **source** program → a COMPLETE wasm module (WAT text)
//! via the v1 MIR renderer — the library form of the `render_program` example's `main()`, so the
//! `almide` CLI can drive the v1 path (opt-in `--verified` codegen) with a v0 fallback.
//!
//! The ONLY caller-supplied input beyond the source is `self_modules` — the resolved cross-module
//! `import self.<submodule>` siblings (the caller runs the canonical driver discovery, which uses
//! the `almide` crate and therefore cannot live in this library). Everything downstream of that
//! resolution lives here.
//!
//! Totality: every failure path returns `Err(LowerError::Unsupported(..))` (a clean WALL), NEVER a
//! process abort — so a caller can fall back to v0 codegen when v1 declines.

use crate::lower::LowerError;
use crate::render_wasm::try_render_wasm_program;
use crate::MirProgram;
use almide_frontend::canonicalize;
use almide_frontend::check::Checker;
use almide_frontend::ir_link;
use almide_frontend::lower::lower_program;
use almide_lang::lexer::Lexer;
use almide_lang::parser::Parser;
use almide_optimize::{mono, optimize};
use std::collections::HashMap;

/// The mangled flat name a user-module function gets when resolved to a user `CallFn`
/// (`bindgen` + `get_str` → `almide_rt_bindgen_get_str`) — the v1 analogue of v0's
/// `ir_link_flatten` module-fn renaming, and the call-site target this resolution emits.
fn user_module_fn_name(module: &str, func: &str) -> String {
    format!("almide_rt_{}_{}", module.replace('.', "_"), func.replace('.', "_"))
}

/// Resolve a USER-package/-module call (`bindgen.get_str(…)` via `import self as bindgen`,
/// `self.classifier.classify(…)`) to a real user `CallFn`. WITHOUT this, the MIR lowering
/// sees `CallTarget::Module { module: "bindgen", … }` and walls it as an "effectful/impure
/// stdlib Module call" — but `bindgen` is a USER module whose function is right here in
/// `ir.modules` (thanks to the sibling-link). This rewrites the CALL TARGET only (no IR-level
/// flatten — that would collide the per-module VarId regions; the sibling DEFINITIONS are
/// lowered separately to MIR with the same mangled name):
///   • a `CallTarget::Module { m, f }` where `m` is a user module that defines `f` becomes
///     `CallTarget::Named { name: "almide_rt_<m>_<f>" }` — an ORDINARY user call.
/// SOUNDNESS (caps): the resolved name carries NO dot, so the transitive caps gate treats it
/// as a user call (analyzed via the in-profile map / tainted if unknown), NOT as a pure
/// dotted stdlib call (`is_known_free`). A self-pkg call to an EFFECTFUL user fn therefore
/// surfaces its capability transitively, exactly like any direct user call. A STDLIB module
/// (`string`, bundled `json`, …) is NOT rewritten. No-op when there are no linked user modules.
fn resolve_user_module_calls(ir: &mut almide_ir::IrProgram) {
    use almide_ir::{walk_expr_mut, CallTarget, IrExprKind, IrMutVisitor};
    use almide_lang::intern::sym;
    let user_mods: std::collections::HashMap<String, std::collections::HashSet<String>> = ir
        .modules
        .iter()
        .filter(|m| !almide_lang::stdlib_info::is_any_stdlib(m.name.as_str()))
        .map(|m| {
            (
                m.name.as_str().to_string(),
                m.functions.iter().map(|f| f.name.as_str().to_string()).collect(),
            )
        })
        .collect();
    if user_mods.is_empty() {
        return; // single-file / stdlib-only — strict no-op.
    }
    struct Rw<'a> {
        user_mods: &'a std::collections::HashMap<String, std::collections::HashSet<String>>,
        root_fns: std::collections::HashSet<String>,
    }
    impl IrMutVisitor for Rw<'_> {
        fn visit_expr_mut(&mut self, e: &mut almide_ir::IrExpr) {
            walk_expr_mut(self, e);
            if let IrExprKind::Call { target, .. } = &mut e.kind {
                if let CallTarget::Module { module, func, .. } = target {
                    let (m, f) = (module.as_str(), func.as_str());
                    if self.user_mods.get(m).is_some_and(|fs| fs.contains(f)) {
                        *target = CallTarget::Named { name: sym(&user_module_fn_name(m, f)) };
                    }
                } else if let CallTarget::Named { name } = target {
                    // A BARE Named call to a fn that lives in exactly ONE linked user module: the
                    // frontend resolves an `import self as g` call to the bare name when the target is
                    // the package's own module — rewrite to the module fn's mangled def name. Ambiguity
                    // (two modules defining the name, or a root fn shadowing it) leaves the call
                    // untouched — the unlinked gate then walls it honestly instead of guessing.
                    let f = name.as_str();
                    if !self.root_fns.contains(f) {
                        let mut owners = self.user_mods.iter().filter(|(_, fs)| fs.contains(f));
                        if let (Some((m, _)), None) = (owners.next(), owners.next()) {
                            *target = CallTarget::Named { name: sym(&user_module_fn_name(m, f)) };
                        }
                    }
                }
            }
        }
    }
    let root_fns: std::collections::HashSet<String> =
        ir.functions.iter().map(|f| f.name.as_str().to_string()).collect();
    let mut rw = Rw { user_mods: &user_mods, root_fns };
    for func in &mut ir.functions {
        rw.visit_expr_mut(&mut func.body);
    }
    for tl in &mut ir.top_lets {
        rw.visit_expr_mut(&mut tl.value);
    }
    for m in &mut ir.modules {
        for func in &mut m.functions {
            rw.visit_expr_mut(&mut func.body);
        }
        for tl in &mut m.top_lets {
            rw.visit_expr_mut(&mut tl.value);
        }
    }
}

/// Lower `.almd` source to a linked `IrProgram` (`parse → check → lower → optimize → mono →
/// ir_link`) — the SAME frontend cut point emit_cert_from_source uses. `modules` are the resolved
/// cross-module siblings (empty ⇒ the single-file path); each is inferred + `lower_module`d into
/// `ir.modules` so a cross-module record/variant type reaches `build_record_layouts`. A parse or
/// type error is a clean WALL (`Err`), never an abort.
fn source_to_ir_with(
    source: &str,
    modules: &[(String, almide_lang::ast::Program, bool)],
) -> Result<almide_ir::IrProgram, LowerError> {
    let tokens = Lexer::tokenize(source);
    let mut prog = Parser::new(tokens)
        .parse()
        .map_err(|e| LowerError::Unsupported(format!("parse error: {e:?}")))?;
    let canon = canonicalize::canonicalize_program(
        &prog,
        modules.iter().map(|(n, p, s)| (n.as_str(), p, *s)),
    );
    let mut checker = Checker::from_env(canon.env);
    let diags = checker.infer_program(&mut prog);
    let errors: Vec<_> = diags
        .iter()
        .filter(|d| d.level == almide_frontend::diagnostic::Level::Error)
        .map(|d| d.message.clone())
        .collect();
    if !errors.is_empty() {
        return Err(LowerError::Unsupported(format!("type errors: {errors:?}")));
    }
    let mut ir = lower_program(&prog, &checker.env, &checker.type_map);

    // Lower each resolved sibling MODULE into `ir.modules` — the SAME sequence the real driver runs
    // after `lower_program` (infer_module → per-module import table → lower_module → push). Bundled
    // stdlib modules carried by `resolve` are skipped (their defs come from the runtime/self-host
    // registry); only real user siblings contribute their type_decls + fns.
    for (name, mod_prog, is_self) in modules {
        if almide_lang::stdlib_info::is_stdlib_module(name)
            && !almide_lang::stdlib_info::is_bundled_module(name)
        {
            continue;
        }
        let mut mod_prog = mod_prog.clone();
        let _ = is_self;
        checker.infer_module(&mut mod_prog, name);
        let self_name = checker.env.self_module_name.map(|s| s.to_string());
        let import_table_name = self_name.as_deref().unwrap_or(name.as_str());
        let (mod_table, _) = almide_frontend::import_table::build_import_table(
            &mod_prog,
            Some(import_table_name),
            &checker.env.user_modules,
        );
        let saved_table = std::mem::replace(&mut checker.env.import_table, mod_table);
        let mod_ir = almide_frontend::lower::lower_module(
            name,
            &mod_prog,
            &checker.env,
            &checker.type_map,
            None,
        );
        checker.env.import_table = saved_table;
        ir.modules.push(mod_ir);
    }

    resolve_user_module_calls(&mut ir);

    optimize::optimize_program(&mut ir);
    mono::monomorphize(&mut ir);
    ir_link::ir_link(&mut ir);
    Ok(ir)
}

/// Single-file convenience (no cross-module siblings) — the bundled-runtime / drop-source
/// re-lowering paths, which never carry `import self.*`.
fn source_to_ir(source: &str) -> Result<almide_ir::IrProgram, LowerError> {
    source_to_ir_with(source, &[])
}

/// Render a `.almd` **source** program to a COMPLETE wasm module (WAT text) via the v1 MIR renderer.
///
/// `self_modules` are the caller-resolved `import self.<submodule>` siblings (empty ⇒ single file).
/// `verbose` gates the honest per-function "outside the lowering subset" diagnostics to stderr.
///
/// Returns `Ok(wat)` when the WHOLE program lowers (every function in-subset, `main` present, no
/// unlinked call), else `Err(LowerError::Unsupported(..))` — a clean WALL the caller can fall back
/// from (v0 codegen). NEVER a wrong module: honest-wall.
pub fn try_render_wasm_source(
    source: &str,
    self_modules: &[(String, almide_lang::ast::Program, bool)],
    verbose: bool,
) -> Result<String, LowerError> {
    // STRICT VALUE MODE: this is an OUTPUT path — a deferred Const-0 must never be executable
    // (flight-evidence-gaps F2, the prim.handle literal address-0 class).
    crate::lower::STRICT_VALUES.store(true, std::sync::atomic::Ordering::Relaxed);

    let ir = source_to_ir_with(source, self_modules)?;
    // ADT brick 5b: GENERATE the recursive-drop fns (`__drop_<T>`) for nested-variant types and
    // re-lower with them in scope. v1-trust-spine-only — v0 manages its own memory. Two-pass.
    let anon_recs = crate::lower::collect_recursive_anon_records(&ir);
    let mut all_type_decls = ir.type_decls.clone();
    for m in &ir.modules {
        all_type_decls.extend(m.type_decls.iter().cloned());
    }
    let uses_result_opt_str = crate::lower::program_uses_result_option_str(&ir);
    // First-class function values need the UNIFORM closure-block release
    // (`$__drop_closure` — self-describing recursive drop, DropVariant "closure").
    let closure_drop =
        if crate::lower::program_uses_closures(&ir) { crate::lower::CLOSURE_DROP_SRC } else { "" };
    let drops = format!(
        "{}{}{}{}",
        crate::lower::generate_variant_drop_sources(&all_type_decls),
        crate::lower::generate_record_drop_sources(&all_type_decls, &anon_recs, uses_result_opt_str),
        crate::lower::generate_variant_repr_sources(&all_type_decls),
        closure_drop,
    );
    // The generated drops free a `Value` field via value_core's INTERNAL `__drop_value` — bring
    // value_core's source into scope for the re-lower's type check; the auto-link dedups it.
    let needs_value_core = drops.contains("__drop_value") || drops.contains("__drop_list_value");
    let value_core_src: &str = if needs_value_core {
        include_str!("../../../stdlib/value_core.almd")
    } else {
        ""
    };
    let ir = if drops.trim().is_empty() {
        ir
    } else {
        source_to_ir_with(&format!("{source}\n{value_core_src}\n{drops}"), self_modules)?
    };

    // Top-level `let` globals (VarId -> Ty) + their INITIALIZER exprs, union of program + modules.
    let mut globals: HashMap<almide_ir::VarId, almide_lang::types::Ty> = HashMap::new();
    let mut global_inits: HashMap<almide_ir::VarId, almide_ir::IrExpr> = HashMap::new();
    for tl in &ir.top_lets {
        globals.insert(tl.var, tl.ty.clone());
        global_inits.insert(tl.var, tl.value.clone());
    }
    for m in &ir.modules {
        for tl in &m.top_lets {
            globals.insert(tl.var, tl.ty.clone());
            global_inits.insert(tl.var, tl.value.clone());
        }
    }

    // Record-layout registry (type name → fields) for the VALUE MODEL.
    let mut record_layouts = crate::lower::build_record_layouts(&ir.type_decls);
    for m in &ir.modules {
        record_layouts.extend(crate::lower::build_record_layouts(&m.type_decls));
    }
    // Alias each UNIQUELY-owned base name to its qualified layout (a bare `Named` reference to a
    // module record must resolve its field layout); an ambiguous base stays qualified-only.
    {
        let mut owners: std::collections::HashMap<String, Vec<String>> = Default::default();
        for k in record_layouts.keys() {
            if let Some((_, base)) = k.rsplit_once('.') {
                owners.entry(base.to_string()).or_default().push(k.clone());
            }
        }
        for (base, ks) in owners {
            if ks.len() == 1 && !record_layouts.contains_key(&base) {
                let v = record_layouts.get(&ks[0]).cloned().unwrap();
                record_layouts.insert(base, v);
            }
        }
    }

    // Variant-layout registry (type name → tag + per-constructor fields) for custom ADTs.
    let mut variant_layouts = crate::lower::build_variant_layouts(&ir.type_decls);
    for m in &ir.modules {
        let m_vl = crate::lower::build_variant_layouts(&m.type_decls);
        variant_layouts.by_type.extend(m_vl.by_type);
        variant_layouts.ctor_to_type.extend(m_vl.ctor_to_type);
    }
    {
        let mut owners: std::collections::HashMap<String, Vec<String>> = Default::default();
        for k in variant_layouts.by_type.keys() {
            if let Some((_, base)) = k.rsplit_once('.') {
                owners.entry(base.to_string()).or_default().push(k.clone());
            }
        }
        for (base, ks) in owners {
            if ks.len() == 1 && !variant_layouts.by_type.contains_key(&base) {
                let v = variant_layouts.by_type.get(&ks[0]).cloned().unwrap();
                variant_layouts.by_type.insert(base, v);
            }
        }
    }

    // PROGRAM pre-pass: inline mutual-recursive tail siblings (semantics-preserving TCO exposure).
    let inlined_fns =
        crate::lower::inline_mutual_tail_recursion(&ir.functions, &globals, &record_layouts);

    let mut functions = Vec::new();
    let mut walled = Vec::new();
    for func in &inlined_fns {
        // `test "…"` blocks lower to fns calling the test harness (no wasm def) — never reachable
        // from `_start`/`main`, so skip them (rendering one would pull a dangling `(call $assert_eq)`).
        if func.is_test {
            continue;
        }
        match crate::lower::lower_function_all_with_globals(
            func,
            &globals,
            &global_inits,
            &record_layouts,
            &variant_layouts,
        ) {
            Ok(mirs) => functions.extend(mirs),
            Err(e) => walled.push(format!("{}: {e:?}", func.name.as_str())),
        }
    }
    if !walled.is_empty() && verbose {
        eprintln!(
            "[render_program] {} of {} function(s) outside the lowering subset (NOT rendered):",
            walled.len(),
            ir.functions.len()
        );
        for w in &walled {
            eprintln!("  {w}");
        }
    }

    // Lower the linked USER-module functions the target's resolved `almide_rt_<m>_<f>` references,
    // renamed to the SAME mangled name. Each lowered SEPARATELY (its own VarId region + shared
    // globals). A sibling that itself WALLS is silently skipped (the target then fails the
    // unlinked-call render wall if it truly needed it). Stdlib modules stay out (self-host below).
    let already: std::collections::HashSet<String> =
        functions.iter().map(|f| f.name.clone()).collect();
    for m in &ir.modules {
        if almide_lang::stdlib_info::is_any_stdlib(m.name.as_str()) {
            continue;
        }
        let mname = m.name.as_str().to_string();
        for func in &m.functions {
            if func.is_test {
                continue;
            }
            let mangled = user_module_fn_name(&mname, func.name.as_str());
            if already.contains(&mangled) {
                continue;
            }
            if let Ok(mirs) = crate::lower::lower_function_all_with_globals(
                func,
                &globals,
                &global_inits,
                &record_layouts,
                &variant_layouts,
            ) {
                for (i, mut mir) in mirs.into_iter().enumerate() {
                    if i == 0 {
                        mir.name = mangled.clone();
                    }
                    functions.push(mir);
                }
            }
        }
    }

    // Auto-link the self-hosted stdlib runtime (int.to_string, string.concat, …) when an entry is
    // called but not defined, renaming its impl fn to the call name. A linked impl may call ANOTHER
    // registry entry, so iterate to a FIXPOINT.
    loop {
        let before = functions.len();
        for (rt_source, entries) in crate::render_wasm::self_host_runtime() {
            let mut any_called = entries.iter().any(|(_, call)| {
                functions.iter().any(|f| {
                    f.ops.iter().any(|op| matches!(op, crate::Op::CallFn { name, .. } if name == call))
                })
            });
            // A Value drop renders `(call $__drop_value …)` — a value_core helper that is NOT a
            // registered call_name, so force value_core when ANY Value-drop op is present.
            if entries.iter().any(|(_, c)| *c == "value.null") {
                any_called = any_called
                    || functions.iter().any(|f| {
                        f.ops.iter().any(|op| {
                            matches!(
                                op,
                                crate::Op::DropValue { .. }
                                    | crate::Op::DropListValue { .. }
                                    | crate::Op::DropListStrValue { .. }
                                    | crate::Op::DropListStrStr { .. }
                                    | crate::Op::DropResultValue { .. }
                                    | crate::Op::DropResultListValue { .. }
                            )
                        })
                    });
            }
            let any_defined =
                entries.iter().any(|(_, call)| functions.iter().any(|f| &f.name == call));
            if any_called && !any_defined {
                let rt = source_to_ir(rt_source)?;
                for f in &rt.functions {
                    let lowered = crate::lower::lower_function(f, &globals);
                    if let Err(e) = &lowered {
                        if verbose
                            && (entries.iter().any(|(impl_fn, _)| f.name.as_str() == *impl_fn)
                                || f.name.as_str().starts_with("__"))
                        {
                            eprintln!("[self-host] {} failed to lower: {:?}", f.name.as_str(), e);
                        }
                    }
                    if let Ok(mut mir) = lowered {
                        if let Some((_, call)) =
                            entries.iter().find(|(impl_fn, _)| &mir.name == impl_fn)
                        {
                            mir.name = call.to_string();
                        }
                        functions.push(mir);
                    }
                }
            }
        }
        // Dedup by name (identical source ⇒ no-op merge).
        let mut seen = std::collections::HashSet::new();
        functions.retain(|f| seen.insert(f.name.clone()));
        if functions.len() == before {
            break;
        }
    }

    // A self-hosted runtime fn may call ANOTHER registered impl by its IMPL name, but the auto-link
    // RENAMED that def to its call_name. Rewrite those call sites to the call_name.
    let impl_to_call: std::collections::HashMap<&str, &str> = crate::render_wasm::self_host_runtime()
        .iter()
        .flat_map(|(_, es)| es.iter().map(|(i, c)| (*i, *c)))
        .collect();
    for f in &mut functions {
        for op in &mut f.ops {
            if let crate::Op::CallFn { name, .. } = op {
                if let Some(&c) = impl_to_call.get(name.as_str()) {
                    *name = c.to_string();
                }
            }
        }
    }

    // Auto-link the self-hosted runtime `print_str` (`println` → `PrintStr` → `(call $print_str)`).
    if !functions.iter().any(|f| f.name == "print_str") {
        let rt = source_to_ir(include_str!("../../../stdlib/print_str.almd"))?;
        for f in &rt.functions {
            if let Ok(mir) = crate::lower::lower_function(f, &globals) {
                functions.push(mir);
            }
        }
    }

    // EAGER GLOBAL-INIT semantics (C-007): v0 evaluates every ABORTABLE top-let initializer at
    // startup. Synthesize `__global_init` binding each CALL-FREE SCALAR initializer and have
    // `_start` call it before `$main`. Call-bearing/heap inits keep per-use/wall handling.
    {
        fn has_call(e: &almide_ir::IrExpr) -> bool {
            use almide_ir::visit::{walk_expr, IrVisitor};
            struct C(bool);
            impl IrVisitor for C {
                fn visit_expr(&mut self, e: &almide_ir::IrExpr) {
                    if matches!(
                        e.kind,
                        almide_ir::IrExprKind::Call { .. } | almide_ir::IrExprKind::RuntimeCall { .. }
                    ) {
                        self.0 = true;
                    }
                    walk_expr(self, e);
                }
            }
            let mut c = C(false);
            c.visit_expr(e);
            c.0
        }
        let mut max_var = 0u32;
        for (v, _) in &globals {
            max_var = max_var.max(v.0);
        }
        {
            use almide_ir::visit::{walk_expr, IrVisitor};
            struct M(u32);
            impl IrVisitor for M {
                fn visit_expr(&mut self, e: &almide_ir::IrExpr) {
                    if let almide_ir::IrExprKind::Var { id } = &e.kind {
                        self.0 = self.0.max(id.0);
                    }
                    walk_expr(self, e);
                }
            }
            let mut m = M(max_var);
            for f in &ir.functions {
                m.visit_expr(&f.body);
            }
            max_var = m.0;
        }
        let mut stmts: Vec<almide_ir::IrStmt> = Vec::new();
        let mut ordered: Vec<_> = ir
            .top_lets
            .iter()
            .chain(ir.modules.iter().flat_map(|m| m.top_lets.iter()))
            .collect();
        ordered.sort_by_key(|tl| tl.var.0);
        // A later initializer may READ an earlier global — INLINE each processed init into its
        // dependents (declaration order; all call-free ⇒ pure, finite substitution).
        let mut subst: std::collections::HashMap<almide_ir::VarId, almide_ir::IrExpr> =
            std::collections::HashMap::new();
        for tl in ordered {
            let scalar = !crate::lower::is_heap_ty(&tl.ty);
            if scalar && !has_call(&tl.value) {
                let mut value = tl.value.clone();
                for (gv, ge) in &subst {
                    value = almide_ir::substitute::substitute_var_in_expr(&value, *gv, ge);
                }
                subst.insert(tl.var, value.clone());
                max_var += 1;
                stmts.push(almide_ir::IrStmt {
                    kind: almide_ir::IrStmtKind::Bind {
                        var: almide_ir::VarId(max_var),
                        mutability: almide_ir::Mutability::Let,
                        ty: tl.ty.clone(),
                        value,
                    },
                    span: None,
                });
            }
        }
        if !stmts.is_empty() {
            let body = almide_ir::IrExpr {
                kind: almide_ir::IrExprKind::Block { stmts, expr: None },
                ty: almide_lang::types::Ty::Unit,
                span: Default::default(),
                def_id: None,
            };
            let init_fn = almide_ir::IrFunction {
                name: almide_lang::intern::sym("__global_init"),
                params: vec![],
                ret_ty: almide_lang::types::Ty::Unit,
                body,
                is_effect: false,
                is_async: false,
                is_test: false,
                generics: None,
                extern_attrs: vec![],
                export_attrs: vec![],
                attrs: vec![],
                visibility: almide_ir::IrVisibility::Public,
                doc: None,
                blank_lines_before: 0,
                def_id: None,
                module_origin: None,
                mutated_params: vec![],
            };
            if let Ok(mir) = crate::lower::lower_function(&init_fn, &globals) {
                functions.push(mir);
            }
        }
    }

    // If `main` itself was WALLED, there is no `$main` — yet the renderer emits
    // `(func (export "_start") (call $main))`. Wall the WHOLE program cleanly instead of a
    // main-less (invalid) module.
    if !functions.iter().any(|f| f.name == "main") {
        return Err(LowerError::Unsupported(
            "main is outside the MIR-lowering subset".into(),
        ));
    }

    // Any UNLINKED stdlib/runtime call would render a dangling `(call $name)` (invalid wasm) — the
    // renderer rejects it cleanly. Returns the WAT on success.
    try_render_wasm_program(&MirProgram { functions })
}
