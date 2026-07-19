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
    // Transparent-newtype erasure LAST (post-link, pre-lowering): `mod type X = String`
    // ctor calls/patterns/Ty tags become the inner type (see newtype_erase.rs).
    crate::lower::erase_transparent_newtypes(&mut ir);
    // Record default-field fill (Opts {} materializes its declared defaults) — the
    // SAME program-level pass classify runs (desugar-before-both).
    crate::lower::fill_record_defaults(&mut ir);
    // Pure call-bearing GLOBAL inits inline at their use sites (the lazy-static value
    // semantics — see inline_pure_call_globals; shared with classify: desugar-before-both).
    crate::lower::inline_pure_call_globals(&mut ir);
    // C-132 move-mode write-back: `mut` param fns return their mutated buffer and
    // call sites assign it back — the SAME rewrite the v0 wasm pipeline runs
    // (almide_ir::mut_param), applied pre-lowering so both v1 legs and the caps
    // counter see one tree. Rewritten fns drop `mutated_params` (the wall keys on
    // it); excluded shapes (multi-mut-param, same-name, non-Unit effect) keep it
    // and keep walling.
    almide_ir::mut_param::lower_mut_params_move_mode(&mut ir);
    // Guard → if restructure at the fn-body tail chain (conditional early return
    // expressed without early-return control flow — see desugar_guard.rs; shared
    // with classify: desugar-before-both).
    crate::lower::desugar_fn_body_guards(&mut ir);
    // Tail err-raise ifs normalize to the proven bind-position `!` shape (fed by the
    // guard restructure above; shared with classify: desugar-before-both).
    crate::lower::normalize_tail_err_raise_ifs(&mut ir);
    // Block call-arguments absorb their call (shared with classify: desugar-before-both).
    crate::lower::hoist_block_call_args(&mut ir);
    crate::lower::desugar_loop_early_returns(&mut ir);
    crate::lower::hoist_spread_call_bases(&mut ir);
    crate::lower::hoist_record_literal_args(&mut ir);
    // Debug aid: `ALMIDE_DUMP_IR=<substr>` dumps the post-chain body of matching fns.
    if let Ok(pat) = std::env::var("ALMIDE_DUMP_IR") {
        for f in ir.functions.iter().chain(ir.modules.iter().flat_map(|m| m.functions.iter())) {
            if f.name.as_str().contains(&pat) {
                eprintln!("=== ALMIDE_DUMP_IR {} ===\n{:#?}", f.name.as_str(), f.body);
            }
        }
    }
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
/// Promote a NO-`main` test file's `test` fns to ordinary effect fns and synthesize the
/// runner `main` (v0 `__test_runner` protocol). See [`try_render_wasm_source_tests`].
fn synthesize_test_runner_main(ir: &mut almide_ir::IrProgram) -> Result<(), LowerError> {
    use almide_ir::{CallTarget, IrExpr, IrExprKind, IrStmt, IrStmtKind};
    use almide_lang::intern::sym;
    use almide_lang::types::Ty;
    if ir.functions.iter().any(|f| !f.is_test && f.name.as_str() == "main") {
        // main-mode: both legs run main only (v0's `__main_runner` protocol); the
        // test fns stay `is_test` and the render loop skips them as before.
        return Ok(());
    }
    if ir.functions.iter().all(|f| !f.is_test) {
        return Err(LowerError::Unsupported(
            "test mode: no `main` and no test blocks — nothing to run".into(),
        ));
    }
    // v0's `__test_runner` re-initializes module globals before EVERY test (native
    // thread-isolation parity). The v1 `_start` runs `__global_init`/`__mg_init` ONCE —
    // so the runner main re-ASSIGNS every MUTABLE main-region top-let to its
    // initializer before each test (the ordinary `lower_mutable_global_assign` path:
    // take + drop-old + store — no leak, no new runtime). An IMMUTABLE top-let cannot
    // change between tests and needs no re-init. MODULE top-lets stay walled: their
    // VarIds live in a different numbering region than the main-region runner, so a
    // synthesized Assign could collide with an unrelated main-side id (the per-region
    // globals discipline) — that bridge is the remaining piece of this brick.
    if ir.modules.iter().any(|m| m.top_lets.iter().any(|tl| tl.mutable)) {
        // MUTABLE module top-lets stay walled in test mode: the per-test re-init
        // Assign would have to cross the VarId numbering-region bridge (a
        // synthesized main-region Assign could collide with an unrelated module
        // id — the per-region globals discipline). IMMUTABLE literal-init
        // top-lets cannot change between tests and need no re-init — they lower
        // through the const-bridge.
        return Err(LowerError::Unsupported(
            "test mode: MUTABLE module top-lets need the per-test region bridge, \
             not in this brick"
                .into(),
        ));
    }
    // A referenced IMPURE-call-initialized module top-let walls the file: the
    // const-bridge drops a call init (mod.rs's expr_has_call), a PURE one is
    // substituted into the reader bodies later (the ceangal/#785 substitution +
    // the record-field hoist), but an IMPURE one has no faithful route — and an
    // unbound reference TRAPS at runtime (index OOB) instead of walling.
    // Referenced = a frontend-synthesized cross-module ref entry names it.
    {
        use almide_ir::visit::{walk_expr, IrVisitor};
        struct C {
            has_call: bool,
            impure: bool,
        }
        impl IrVisitor for C {
            fn visit_expr(&mut self, e: &almide_ir::IrExpr) {
                match &e.kind {
                    almide_ir::IrExprKind::RuntimeCall { .. } => {
                        self.has_call = true;
                        self.impure = true;
                    }
                    almide_ir::IrExprKind::Call { target, .. } => {
                        self.has_call = true;
                        match target {
                            almide_ir::CallTarget::Module { module, func, .. } => {
                                if !crate::purity::is_pure(module.as_str(), func.as_str()) {
                                    self.impure = true;
                                }
                            }
                            almide_ir::CallTarget::Named { .. } => {}
                            _ => self.impure = true,
                        }
                    }
                    _ => {}
                }
                walk_expr(self, e);
            }
        }
        let effectish: std::collections::HashSet<&str> = ir
            .functions
            .iter()
            .chain(ir.modules.iter().flat_map(|m| m.functions.iter()))
            .filter(|f| f.is_effect)
            .map(|f| f.name.as_str())
            .collect();
        let impure_call_inits: std::collections::HashSet<(String, String)> = ir
            .modules
            .iter()
            .flat_map(|m| {
                let effectish = &effectish;
                m.top_lets.iter().filter_map(move |tl| {
                    let mut c = C { has_call: false, impure: false };
                    c.visit_expr(&tl.value);
                    // A Named callee that is an EFFECT fn is impure too.
                    let named_effect = {
                        use almide_ir::visit::{walk_expr, IrVisitor};
                        struct N<'a> {
                            hit: bool,
                            effectish: &'a std::collections::HashSet<&'a str>,
                        }
                        impl IrVisitor for N<'_> {
                            fn visit_expr(&mut self, e: &almide_ir::IrExpr) {
                                if let almide_ir::IrExprKind::Call {
                                    target: almide_ir::CallTarget::Named { name },
                                    ..
                                } = &e.kind
                                {
                                    if self.effectish.contains(name.as_str()) {
                                        self.hit = true;
                                    }
                                }
                                walk_expr(self, e);
                            }
                        }
                        let mut n = N { hit: false, effectish };
                        n.visit_expr(&tl.value);
                        n.hit
                    };
                    // PURE call inits PASS: the bind-form substitution places
                    // the init at the fn top, and repair_record_literal_field_tys
                    // heals the Unknown declared-field type the linked literal
                    // carried (#785) — the full single-file-proven form. IMPURE
                    // inits have no faithful route and stay walled.
                    if !(c.has_call && (c.impure || named_effect)) {
                        return None;
                    }
                    m.var_table
                        .entries
                        .get(tl.var.0 as usize)
                        .map(|e| (m.name.as_str().to_string(), e.name.as_str().to_uppercase()))
                })
            })
            .collect();
        if !impure_call_inits.is_empty()
            && ir.var_table.entries.iter().any(|e| {
                e.module_origin.as_ref().is_some_and(|mo| {
                    impure_call_inits.contains(&(mo.clone(), e.name.as_str().to_uppercase()))
                })
            })
        {
            return Err(LowerError::Unsupported(
                "test mode: a referenced impure-call-initialized module top-let \
                 needs the slot-routed bridge, not in this brick"
                    .into(),
            ));
        }
    }
    let reinit_stmts: Vec<IrStmt> = ir
        .top_lets
        .iter()
        .filter(|tl| tl.mutable)
        .map(|tl| IrStmt {
            kind: IrStmtKind::Assign { var: tl.var, value: tl.value.clone() },
            span: None,
        })
        .collect();
    let unit_expr =
        || IrExpr { kind: IrExprKind::Unit, ty: Ty::Unit, span: None, def_id: None };
    let println_stmt = |text: String| IrStmt {
        kind: IrStmtKind::Expr {
            expr: IrExpr {
                kind: IrExprKind::Call {
                    target: CallTarget::Named { name: sym("println") },
                    args: vec![IrExpr {
                        kind: IrExprKind::LitStr { value: text },
                        ty: Ty::String,
                        span: None,
                        def_id: None,
                    }],
                    type_args: Vec::new(),
                },
                ty: Ty::Unit,
                span: None,
                def_id: None,
            },
        },
        span: None,
    };
    let mut stmts: Vec<IrStmt> = Vec::new();
    let mut idx = 0usize;
    for f in ir.functions.iter_mut() {
        if !f.is_test {
            continue;
        }
        let display = f
            .name
            .as_str()
            .strip_prefix(almide_ir::TEST_NAME_PREFIX)
            .unwrap_or(f.name.as_str())
            .to_string();
        // Raw test names carry spaces/parens/unicode no WAT identifier admits — rename
        // to a mechanical id and drop `is_test` so the render loop lowers it like any
        // other effect fn (nothing else references a test fn by name).
        let mangled = format!("__almd_test_{idx}");
        idx += 1;
        f.name = sym(&mangled);
        f.is_test = false;
        // v0 isolation parity: reset every mutable top-let to its initializer
        // before the test body runs (see the reinit_stmts derivation above).
        stmts.extend(reinit_stmts.iter().cloned());
        stmts.push(println_stmt(format!("test: {display} ... ")));
        // The stmt-position effect call, in the SAME shape the frontend gives user
        // code: `Try { call }` with the LIFTED `Result[Unit, String]` call type — the
        // never-err strips / can-err propagation then classify it exactly like any
        // other caller (the C-135 def/callsite agreement).
        let call = IrExpr {
            kind: IrExprKind::Call {
                target: CallTarget::Named { name: sym(&mangled) },
                args: Vec::new(),
                type_args: Vec::new(),
            },
            ty: Ty::result(Ty::Unit, Ty::String),
            span: None,
            def_id: None,
        };
        stmts.push(IrStmt {
            kind: IrStmtKind::Expr {
                expr: IrExpr {
                    kind: IrExprKind::Try { expr: Box::new(call) },
                    ty: Ty::Unit,
                    span: None,
                    def_id: None,
                },
            },
            span: None,
        });
        stmts.push(println_stmt("ok".to_string()));
    }
    let body = IrExpr {
        kind: IrExprKind::Block { stmts, expr: Some(Box::new(unit_expr())) },
        ty: Ty::Unit,
        span: None,
        def_id: None,
    };
    ir.functions.push(almide_ir::IrFunction {
        name: sym("main"),
        params: vec![],
        ret_ty: Ty::Unit,
        body,
        is_effect: true,
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
        mutated_params: vec![],
        module_origin: None,
    });
    Ok(())
}

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
    try_render_wasm_source_impl(source, self_modules, verbose, false)
}

/// TEST-mode variant for the `almide test` wasm harness: when the file has NO `main`,
/// its `test "…"` fns are promoted to ordinary effect fns (renamed `__almd_test_<i>` —
/// the raw names carry spaces/unicode no WAT identifier admits) and a runner `main` is
/// synthesized with v0's `__test_runner` protocol (`test: <name> ... ` / `ok` per test,
/// assert failure = controlled halt with a non-zero exit). A file WITH `main` renders
/// exactly like [`try_render_wasm_source`] — both legs run main only, the v0 protocol.
/// Programs with top-let globals WALL in test mode (v0 re-inits globals before EVERY
/// test; the v1 `_start` inits once — shipping that silently would leak one test's
/// mutations into the next).
pub fn try_render_wasm_source_tests(
    source: &str,
    self_modules: &[(String, almide_lang::ast::Program, bool)],
    verbose: bool,
) -> Result<String, LowerError> {
    try_render_wasm_source_impl(source, self_modules, verbose, true)
}

fn try_render_wasm_source_impl(
    source: &str,
    self_modules: &[(String, almide_lang::ast::Program, bool)],
    verbose: bool,
    test_mode: bool,
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
    // A GENERIC user variant instantiated with concrete args as a `List[<...>]` LITERAL element
    // type (`List[Either[Int,String]]`) needs a SHADOW `type <inst>` + `$__drop_<inst>`/
    // `$__drop_list_<inst>` generated for THIS SPECIFIC instantiation — the raw declaration
    // (`Either[L,R]`) carries unresolved type-parameter placeholders the drop generator can't
    // classify (see `is_rich_variant_ty`'s doc comment, mod_p2.rs). Built from a PRE-relower
    // `VariantLayouts` (this program's OWN declared generics/cases, before the drops text is
    // appended) so discovery sees the ORIGINAL `Left(1)`/`Right("y")` list literal. The shadow
    // type declaration text is prepended to `drops` below; the shadow `IrTypeDecl` is spliced
    // into `all_type_decls` so the SAME `generate_variant_drop_sources` call already below
    // covers it too (no separate/duplicate drop-generation call).
    let pre_relower_variant_layouts = crate::lower::build_variant_layouts(&all_type_decls);
    let generic_variant_list_insts =
        crate::lower::discover_generic_variant_list_instantiations(&ir, &pre_relower_variant_layouts);
    let (generic_variant_type_decl_src, generic_variant_synthetic_decls) =
        crate::lower::generate_generic_variant_instantiation_type_decls(
            &generic_variant_list_insts,
            &pre_relower_variant_layouts,
        );
    // The SHADOW decls exist only to hang `$__drop_<inst>` on — they must NOT reach the
    // REPR generator: their synthetic ctor names (`C__Either_Int_String_0`) would render
    // into `${…}` output (native prints the REAL `Left(…)`), and their emitted
    // `__repr_<inst>` would collide with the instantiation-keyed repr of the same name
    // that the interp section generates from the REAL generic decl. Snapshot the
    // shadow-free list for the repr call below.
    let repr_type_decls = all_type_decls.clone();
    all_type_decls.extend(generic_variant_synthetic_decls);
    let uses_result_opt_str = crate::lower::program_uses_result_option_str(&ir);
    // First-class function values need the UNIFORM closure-block release
    // (`$__drop_closure` — self-describing recursive drop, DropVariant "closure").
    let closure_drop =
        if crate::lower::program_uses_closures(&ir) { crate::lower::CLOSURE_DROP_SRC } else { "" };
    // A `List[<Fn>]` LITERAL (`[(x)=>x+1, (x)=>x*2]`) routes its scope-end drop to the
    // generated `$__drop_list_closure` (per-element `$__drop_closure` — required, not a
    // blind rc_dec, since a captured heap slot would otherwise leak). Needs
    // `CLOSURE_DROP_SRC` in scope, which `program_uses_closures` already guarantees
    // whenever a closure LIST exists (the list's elements are Lambda exprs).
    let list_closure_drop = if crate::lower::program_uses_closure_list(&ir) {
        crate::lower::LIST_CLOSURE_DROP_SRC
    } else {
        ""
    };
    // A `Map[String, <Fn>]` (the closure-valued map — mclo class) routes its scope-end
    // drop to `$__drop_map_mclo` (per-value `$__drop_closure` over the split layout).
    // Needs `CLOSURE_DROP_SRC` in scope, which `program_uses_closures` guarantees
    // whenever a closure-valued map exists (its values are Fn-typed exprs).
    let map_mclo_drop = if crate::lower::program_uses_map_closure(&ir) {
        crate::lower::MAP_MCLO_DROP_SRC
    } else {
        ""
    };
    // A `List[Option/Result]` literal with owned-handle-slot elements routes its drop to the
    // generated `$__drop_list_lenlist` (the shared `lenlist_elem_class` decides both sides).
    let lenlist_drop = if crate::lower::program_uses_lenlist_elem_lists(&ir) {
        crate::lower::LENLIST_DROP_SRC
    } else {
        ""
    };
    // `__drop_list_str` (a `List[String]` record OR variant ctor field, OR a closure's
    // nested-heap capture — `CLOSURE_DROP_SRC`'s `__drop_closure_loop` unconditionally
    // references it once ANY closure exists, since a capture's concrete type isn't known
    // at this gate without re-running `lift_lambda`'s own free-vars scan; conservatively
    // widened on `program_uses_closures` rather than precisely detecting a List[String]
    // capture — always correct, occasionally includes an unused routine) — SHARED between
    // the record and variant drop generators, so it is emitted ONCE here rather than by
    // either generator inline (two independent copies would be a duplicate-fn compile
    // error).
    let list_str_drop = if crate::lower::program_uses_list_str_drop_field(&all_type_decls)
        || crate::lower::program_uses_anon_list_str_record(&ir, &all_type_decls)
        || crate::lower::program_uses_closures(&ir)
    {
        crate::lower::LIST_STR_DROP_SRC
    } else {
        ""
    };
    // `Result[List[Int], List[String]]` (result.collect) routes its drop to the
    // TAG-AWARE `$__drop_res_ilsl` (Err → recursive string free; Ok → flat).
    let res_ilsl_drop = if crate::lower::program_uses_res_intlist_strlist(&ir) {
        crate::lower::RES_ILSL_DROP_SRC
    } else {
        ""
    };
    // `map.find`'s `Option[(String, <scalar>)]` result routes its drop to the TAG-AWARE
    // `$__drop_opt_str_int` (Some → recursive String-slot free; None → nothing) — a blind
    // flat `rc_dec` of the Option's payload slot would only free the TUPLE's own refcount,
    // leaking its String.
    let opt_str_int_drop = if crate::lower::program_calls_map_find(&ir) {
        crate::lower::OPT_STR_INT_DROP_SRC
    } else {
        ""
    };
    let drops = format!(
        "{}{}{}{}{}{}{}{}{}{}{}{}",
        generic_variant_type_decl_src,
        crate::lower::generate_variant_drop_sources(&all_type_decls),
        crate::lower::generate_record_drop_sources(&all_type_decls, &anon_recs, uses_result_opt_str),
        crate::lower::generate_variant_repr_sources(&repr_type_decls, &crate::lower::collect_interp_anon_records(&ir), &crate::lower::collect_interp_repr_containers(&ir)),
        crate::lower::generate_krec_sources(&ir, &all_type_decls),
        closure_drop,
        res_ilsl_drop,
        lenlist_drop,
        list_str_drop,
        list_closure_drop,
        map_mclo_drop,
        opt_str_int_drop,
    );
    // The generated drops free a `Value` field via value_core's INTERNAL `__drop_value` — bring
    // value_core's source into scope for the re-lower's type check; the auto-link dedups it.
    let needs_value_core = drops.contains("__drop_value")
        || drops.contains("__drop_list_value")
        // A generated repr calls value_core's JSON serializer by its IMPL name
        // (`value_stringify` — the `${Value}` / Value-field C-060 reprs).
        || drops.contains("value_stringify");
    let value_core_src: &str = if needs_value_core {
        include_str!("../../../stdlib/value_core.almd")
    } else {
        ""
    };
    if std::env::var("ALMIDE_DUMP_DROPS").is_ok() {
        eprintln!("=== ALMIDE_DUMP_DROPS ===\n{drops}\n=== end ===");
    }
    let mut ir = if drops.trim().is_empty() {
        ir
    } else {
        source_to_ir_with(&format!("{source}\n{value_core_src}\n{drops}"), self_modules)?
    };
    if test_mode {
        synthesize_test_runner_main(&mut ir)?;
    }

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
    // PER-REGION globals: the shared union above keys BOTH the main program's and each
    // module's top-let VarIds — two PRIVATE numbering regions that can COLLIDE (main-side
    // VarId(2) vs a module's VarId(2) are unrelated). MAIN functions must resolve through
    // main's own entries first (re-inserted last, winning collisions) plus the cross-module
    // NAME bridge (`toplib.SYSTEM` referenced through a main-side id); MODULE functions keep
    // the module-entries-win union (their region, as today).
    let mut main_globals = globals.clone();
    let mut main_global_inits = global_inits.clone();
    let mut mutable_toplet_aliases: std::collections::HashMap<almide_ir::VarId, almide_ir::VarId> =
        std::collections::HashMap::new();
    crate::lower::bridge_cross_module_toplets(&ir, &mut main_globals, &mut main_global_inits, &mut mutable_toplet_aliases);
    for tl in &ir.top_lets {
        main_globals.insert(tl.var, tl.ty.clone());
        main_global_inits.insert(tl.var, tl.value.clone());
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
        variant_layouts.ctor_field_defaults.extend(m_vl.ctor_field_defaults);
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
    // The input is the WHOLE program — main's functions PLUS every linked user-module sibling
    // under its MANGLED `almide_rt_<m>_<f>` name (bodies already reference siblings by that
    // name, post-`resolve_user_module_calls`). Without the siblings, the never-err/auto-wrap
    // ABI registries were populated from MAIN's functions only: a cross-module effect callee
    // (`m.estep`) was UNCLASSIFIED, so the caller kept its auto-`?` Try (expecting a heap
    // Result handle) while the separately-lowered callee returned its raw scalar — the
    // crossmod_shape_matrix i64/i32 invalid-wasm class. One combined classification makes
    // caller and callee agree by construction; the returned rewritten bodies are then split
    // back into the main / module lowering regions (each keeps its own globals union).
    let mut module_fn_sibs: Vec<almide_ir::IrFunction> = ir
        .modules
        .iter()
        .filter(|m| !almide_lang::stdlib_info::is_any_stdlib(m.name.as_str()))
        .flat_map(|m| {
            let mname = m.name.as_str().to_string();
            // INTRA-MODULE bare sibling calls resolve MODULE-LOCALLY (the #692 rule:
            // current-module qualified > bare > any-module) — a clone body left with a
            // bare `route(x, 100)` linked MAIN's same-named 0-arg `route` and shipped
            // invalid wasm as "v1-verified" (values remaining on stack at the callee's
            // arity mismatch — wasm_same_name_crossmod_test).
            let sibs: std::collections::HashSet<String> =
                m.functions.iter().map(|f| f.name.as_str().to_string()).collect();
            m.functions.iter().filter(|f| !f.is_test).map(move |f| {
                let mut nf = f.clone();
                nf.name = almide_lang::intern::sym(&user_module_fn_name(&mname, f.name.as_str()));
                struct Rw<'a> {
                    mname: &'a str,
                    sibs: &'a std::collections::HashSet<String>,
                }
                impl almide_ir::IrMutVisitor for Rw<'_> {
                    fn visit_expr_mut(&mut self, e: &mut almide_ir::IrExpr) {
                        almide_ir::walk_expr_mut(self, e);
                        if let almide_ir::IrExprKind::Call { target, .. } = &mut e.kind {
                            if let almide_ir::CallTarget::Named { name } = target {
                                let f = name.as_str();
                                if !f.starts_with("almide_rt_") && self.sibs.contains(f) {
                                    *target = almide_ir::CallTarget::Named {
                                        name: almide_lang::intern::sym(&user_module_fn_name(
                                            self.mname, f,
                                        )),
                                    };
                                }
                            }
                        }
                    }
                }
                let mut rw = Rw { mname: &mname, sibs: &sibs };
                almide_ir::IrMutVisitor::visit_expr_mut(&mut rw, &mut nf.body);
                nf
            })
        })
        .collect();
    let mut all_fns: Vec<almide_ir::IrFunction> = ir.functions.clone();
    all_fns.extend(module_fn_sibs.iter().cloned());
    // The combined run POPULATES the name-keyed ABI registries over the WHOLE program
    // (that is the crossmod fix — caller and callee classify identically); only MAIN's
    // rewritten bodies are kept. The module siblings lower below from their ORIGINAL
    // bodies: feeding the pre-pass's REWRITTEN module bodies through the module loop
    // regressed the intra-module tail-call shape (`route(x, 100)` left values on the
    // wasm stack — wasm_same_name_crossmod_test), and the registries alone are what the
    // module-side lowering consults by MANGLED name.
    let mut inlined_fns =
        crate::lower::inline_mutual_tail_recursion(&ir.functions, &main_globals, &record_layouts);
    // WIDEN the ABI registries over the whole program AFTER the main pre-pass (whose own
    // population is main-only, the pre-batch behavior its rewrites were verified under):
    // every LOWERING-time keyed lookup (never-err strip exclusions, AUTO_WRAP body.ty
    // override, `ret_is_result_abi`) then sees module callees by their mangled names —
    // the crossmod caller/callee ABI agreement — without the pre-pass rewrites ever
    // touching module bodies.
    crate::lower::populate_abi_registries(&all_fns, &record_layouts);
    // The registries above are program-wide, but the never-err REWRITES ran with MAIN-ONLY
    // sets (inside the pre-pass) and never touched module bodies at all. So a MAIN caller of
    // a cross-module never-err callee kept its lifted `Try`/Result-typed call, and a MODULE
    // sibling caller kept its own — both then lower Result-handle reads (`local.set`) over
    // the raw/void ABI the callee's def actually has: the #786 invalid-wasm class. Re-run
    // the never-err rewrite family with the PROGRAM-WIDE sets over BOTH regions, so every
    // caller agrees with the combined classification by construction (idempotent where the
    // main-only pass already fired; call TARGETS are never renamed, so the module-side name
    // resolution the original-bodies rule protects is untouched).
    //
    // FIXPOINT (the #485 effect_assign shape): the strips consult AUTO_WRAP (an
    // auto-wrapped callee's `!`/`??` must NOT strip) while AUTO_WRAP itself is derived
    // from the bodies (has a stmt-position propagating unwrap). A strip can remove a
    // callee's LAST propagating unwrap (`plain_assign`'s `x = step(x)` Try), after which
    // its REAL lowered ABI is bare — but the stale registry still said "wrapped", so its
    // own def lowered bare while every `plain_assign()!` site kept the Result-handle
    // read: invalid wasm (def/callsite ABI split). Iterate populate → rewrite until the
    // registries describe the rewritten bodies verbatim (monotone — strips only remove
    // nodes, AUTO_WRAP only shrinks — so this terminates; the cap is a safety net).
    let mut prev_auto_wrap: Option<std::collections::HashSet<String>> = None;
    for _ in 0..8 {
        let mut all_rewritten: Vec<almide_ir::IrFunction> = inlined_fns.clone();
        all_rewritten.extend(module_fn_sibs.iter().cloned());
        crate::lower::populate_abi_registries(&all_rewritten, &record_layouts);
        let cur = crate::lower::auto_wrap_abi_snapshot();
        if prev_auto_wrap.as_ref() == Some(&cur) {
            break;
        }
        prev_auto_wrap = Some(cur);
        let wide_can_err = crate::lower::compute_can_err(&all_rewritten);
        let wide_lifted = crate::lower::lifted_effect_fn_names(&all_rewritten);
        for f in inlined_fns.iter_mut().chain(module_fn_sibs.iter_mut()) {
            let self_name = f.name.as_str().to_string();
            crate::lower::strip_never_err_unwraps(
                &mut f.body,
                &wide_can_err,
                &wide_lifted,
                &self_name,
            );
            crate::lower::rewrite_never_err_effect_match(&mut f.body, &wide_can_err, &wide_lifted);
            crate::lower::unwrap_never_err_call_types(&mut f.body, &wide_can_err, &wide_lifted);
            crate::lower::rewrap_never_err_into_result_targets(
                &mut f.body,
                &wide_can_err,
                &wide_lifted,
                &record_layouts,
            );
        }
    }
    // Cross-module DERIVED-METHOD name bridge (#790 codec row, piece 2 of the pinned
    // design): a MAIN-region `T.encode` / `T.decode` reference whose type `T` is
    // declared by exactly ONE linked module (and not by main) resolves to that
    // module's MANGLED derived fn (`almide_rt_<m>_T.encode`) — the same unique-owner
    // rule the variant-layout bridging above uses. Without this the reference stays
    // unlinked and the whole program walls (honest, but the direct-method shapes are
    // fully lowerable). Container helpers (`__encode_list_<m>.T`) stay walled — their
    // v1 lowering is the recorded remainder of the bridge design.
    {
        let main_types: std::collections::HashSet<&str> =
            ir.type_decls.iter().map(|td| td.name.as_str()).collect();
        let mut owners: std::collections::HashMap<&str, Vec<&str>> =
            std::collections::HashMap::new();
        for m in &ir.modules {
            if almide_lang::stdlib_info::is_any_stdlib(m.name.as_str()) {
                continue;
            }
            for td in &m.type_decls {
                // Module type names may arrive QUALIFIED (`varlib.Pigment`) — key the
                // owner map by the BASE name (the same normalization the variant-layout
                // bridging above applies).
                let base = td.name.as_str().rsplit('.').next().unwrap_or(td.name.as_str());
                owners.entry(base).or_default().push(m.name.as_str());
            }
        }
        struct Rw<'a> {
            main_types: &'a std::collections::HashSet<&'a str>,
            owners: &'a std::collections::HashMap<&'a str, Vec<&'a str>>,
        }
        impl almide_ir::IrMutVisitor for Rw<'_> {
            fn visit_expr_mut(&mut self, e: &mut almide_ir::IrExpr) {
                almide_ir::walk_expr_mut(self, e);
                if let almide_ir::IrExprKind::Call {
                    target: almide_ir::CallTarget::Named { name },
                    ..
                } = &mut e.kind
                {
                    let n = name.as_str();
                    if n.starts_with("almide_rt_") || n.starts_with("__") {
                        return;
                    }
                    let Some((ty_name, method)) = n.rsplit_once('.') else { return };
                    if method != "encode" && method != "decode" {
                        return;
                    }
                    // `varlib.Pigment.decode` → qualifier "varlib" + base "Pigment";
                    // `Pigment.decode` → base only. A qualified ref must match the
                    // owner; a bare ref must not shadow a MAIN type of the same name.
                    let (qualifier, base) = match ty_name.rsplit_once('.') {
                        Some((q, b)) => (Some(q), b),
                        None => (None, ty_name),
                    };
                    if qualifier.is_none() && self.main_types.contains(base) {
                        return;
                    }
                    if let Some(ms) = self.owners.get(base) {
                        if let [only] = ms.as_slice() {
                            if qualifier.is_none() || qualifier == Some(only) {
                                *name = almide_lang::intern::sym(&user_module_fn_name(
                                    only,
                                    &format!("{base}.{method}"),
                                ));
                            }
                        }
                    }
                }
            }
        }
        let mut rw = Rw { main_types: &main_types, owners: &owners };
        // BOTH regions: main's derived fns reference the imported payload type's codec
        // methods, and the OWNING module's own derived fns reference their sibling
        // types' methods by the same bare `T.method` names (the derive emits Named
        // targets directly — no Method desugar ever re-forms them).
        for f in inlined_fns.iter_mut().chain(module_fn_sibs.iter_mut()) {
            almide_ir::IrMutVisitor::visit_expr_mut(&mut rw, &mut f.body);
        }
        // …and publish the unique-owner map for the DESUGAR-time resolution: the
        // `T.method` Named names are FORMED inside the per-fn lowering (from Method
        // targets), after this pipeline pass — the registry is how they see it.
        let derived_owners: std::collections::HashMap<String, String> = owners
            .iter()
            .filter(|(t, ms)| ms.len() == 1 && !main_types.contains(*t))
            .map(|(t, ms)| (t.to_string(), ms[0].to_string()))
            .collect();
        crate::lower::set_derived_type_owners(derived_owners);
    }
    // MUTABLE module-level `var`s (program + modules): assign each a linear-memory
    // storage slot (declaration order = VarId order, the same ordering `__global_init`
    // uses) and publish the VarId → (slot, Ty) map — reads/assigns then route through the
    // slot (`Load`/`$__mg_get`/`$__mg_take`+`Store`). A VarId collision across regions or
    // an over-cap count WALLS the program (honest, never a mis-routed slot).
    let mut mutable_tls: Vec<_> = ir
        .top_lets
        .iter()
        .chain(ir.modules.iter().flat_map(|m| m.top_lets.iter()))
        .filter(|tl| tl.mutable)
        .collect();
    mutable_tls.sort_by_key(|tl| tl.var.0);
    if mutable_tls.len() > 64 {
        return Err(LowerError::Unsupported(format!(
            "{} mutable module-level vars exceed the 64-slot global region",
            mutable_tls.len()
        )));
    }
    {
        let mut seen = std::collections::HashSet::new();
        for tl in &mutable_tls {
            if !seen.insert(tl.var.0) {
                return Err(LowerError::Unsupported(format!(
                    "mutable module-level var id collision across regions ({:?})",
                    tl.var
                )));
            }
        }
    }
    let mutable_global_count = mutable_tls.len() as u32;
    let mut mutable_global_map: std::collections::HashMap<u32, (u32, almide_lang::types::Ty)> = mutable_tls
        .iter()
        .enumerate()
        .map(|(i, tl)| (tl.var.0, (i as u32, tl.ty.clone())))
        .collect();
    // #782: alias each main-side synthesized ref onto its module var's slot —
    // the retired v0 fallback used to absorb these as walls; now `m.count`
    // reads and assigns route through the SAME storage the owning module uses.
    for (main_id, mod_id) in &mutable_toplet_aliases {
        if let Some(entry) = mutable_global_map.get(&mod_id.0).cloned() {
            mutable_global_map.insert(main_id.0, entry);
        }
    }
    crate::lower::set_mutable_global_vars(mutable_global_map);

    // CROSS-MODULE global refs carry UNKNOWN expr types the frontend never infers
    // (`v.white` — the ceangal theme class): repair them from the bridged globals
    // maps BEFORE lowering, or the AllTypesConcrete precondition walls the whole fn.
    for f in inlined_fns.iter_mut() {
        crate::lower::repair_unknown_global_ref_tys(f, &main_globals);
        crate::lower::repair_member_field_tys(f, &record_layouts);
    }
    for f in module_fn_sibs.iter_mut() {
        crate::lower::repair_unknown_global_ref_tys(f, &globals);
        crate::lower::repair_member_field_tys(f, &record_layouts);
    }

    // A BRIDGED cross-module ref whose module-side init is a PURE CALL (`v.black` →
    // view's `let black = rgb(0,0,0)`, the ceangal theme class) cannot materialize in
    // `value_or_global` (a lowering-time CallFn would break the classify `mir == ir`
    // count). SUBSTITUTE the init into the fn bodies instead — the call then exists in
    // the IR itself, so every count and cap sees it (the exact discipline
    // `inline_pure_call_globals` applies within one region, extended to the refs the
    // bridge resolves across regions). Purity gate: every call inside the init is a
    // pure stdlib call or a RESOLVED user fn that is itself call-transitively clean
    // (effect fns were mangled through the resolver and appear in no pure set).
    {
        use almide_ir::visit::{walk_expr, IrVisitor};
        struct HasImpure<'a> {
            impure: bool,
            effectish: &'a std::collections::HashSet<String>,
        }
        impl IrVisitor for HasImpure<'_> {
            fn visit_expr(&mut self, e: &almide_ir::IrExpr) {
                match &e.kind {
                    almide_ir::IrExprKind::RuntimeCall { .. } => self.impure = true,
                    almide_ir::IrExprKind::Call { target, .. } => match target {
                        almide_ir::CallTarget::Module { module, func, .. } => {
                            if !crate::purity::is_pure(module.as_str(), func.as_str()) {
                                self.impure = true;
                            }
                        }
                        almide_ir::CallTarget::Named { name } => {
                            if self.effectish.contains(name.as_str()) {
                                self.impure = true;
                            }
                        }
                        _ => self.impure = true,
                    },
                    _ => {}
                }
                walk_expr(self, e);
            }
        }
        let effectish: std::collections::HashSet<String> = all_fns
            .iter()
            .filter(|f| f.is_effect)
            .map(|f| f.name.as_str().to_string())
            .collect();
        let mut subs: Vec<(almide_ir::VarId, almide_ir::IrExpr)> = Vec::new();
        for (i, info) in ir.var_table.entries.iter().enumerate() {
            if info.module_origin.is_none() {
                continue;
            }
            let id = almide_ir::VarId(i as u32);
            let Some(init) = main_global_inits.get(&id) else { continue };
            // #782: with the v0 fallback retired, a HEAP toplet whose init is a
            // CTOR form (tuple/record/variant/some/ok — `let PAIR = ("a", 1)`,
            // `let MOOD = Happy`) must ALSO substitute: value_or_global's CONST
            // path only materializes flat literals (LitStr / all-literal List),
            // and the old "computed init" wall used to fall back to v0. The
            // bind-form substitution evaluates the pure ctor at fn-top — same
            // discipline as the pure-call inits below.
            let heap_ctor_init = crate::lower::is_heap_ty(&init.ty)
                && !matches!(
                    &init.kind,
                    almide_ir::IrExprKind::LitStr { .. } | almide_ir::IrExprKind::List { .. }
                );
            if !crate::lower::expr_contains_call(init) && !heap_ctor_init {
                continue;
            }
            let mut h = HasImpure { impure: false, effectish: &effectish };
            h.visit_expr(init);
            if !h.impure {
                subs.push((id, init.clone()));
            }
        }
        for (id, init) in &subs {
            // MAIN-region readers take the BIND form instead of the raw expression
            // substitution: `let __g_init = default_gap(); …Var(__g_init)…` — a call
            // spliced into an arbitrary position (a record FIELD — the #785 shape)
            // rendered as a dst-less bare call (invalid wasm), while a call in BIND
            // position plus a Var reference is the proven single-file form. The bind
            // goes at the fn-body top (the init is pure, so hoisting its evaluation
            // is unobservable); fns that never reference the global are untouched.
            for f in inlined_fns.iter_mut() {
                fn references(e: &almide_ir::IrExpr, id: almide_ir::VarId) -> bool {
                    use almide_ir::visit::{walk_expr, IrVisitor};
                    struct V(almide_ir::VarId, bool);
                    impl IrVisitor for V {
                        fn visit_expr(&mut self, e: &almide_ir::IrExpr) {
                            if matches!(&e.kind, almide_ir::IrExprKind::Var { id } if *id == self.0)
                            {
                                self.1 = true;
                            }
                            walk_expr(self, e);
                        }
                    }
                    let mut v = V(id, false);
                    v.visit_expr(e);
                    v.1
                }
                if !references(&f.body, *id) {
                    continue;
                }
                let nv = ir.var_table.alloc(
                    almide_lang::intern::sym("__g_init"),
                    init.ty.clone(),
                    almide_ir::Mutability::Let,
                    None,
                );
                let nv_ref = almide_ir::IrExpr {
                    kind: almide_ir::IrExprKind::Var { id: nv },
                    ty: init.ty.clone(),
                    span: None,
                    def_id: None,
                };
                f.body = almide_ir::substitute::substitute_var_in_expr(&f.body, *id, &nv_ref);
                let bind_stmt = almide_ir::IrStmt {
                    kind: almide_ir::IrStmtKind::Bind {
                        var: nv,
                        mutability: almide_ir::Mutability::Let,
                        ty: init.ty.clone(),
                        value: init.clone(),
                    },
                    span: None,
                };
                if let almide_ir::IrExprKind::Block { stmts, .. } = &mut f.body.kind {
                    stmts.insert(0, bind_stmt);
                } else {
                    // An EXPRESSION-form body (`effect fn main() -> Unit =
                    // println(m.CFG.name)`) has no statement list — wrap it in a
                    // Block so the fn-top bind exists. Without this the
                    // substitution left `Var(__g_init)` UNBOUND (#782, the
                    // record/variant toplet matrix cells).
                    let old_ty = f.body.ty.clone();
                    let old_span = f.body.span.clone();
                    let old = std::mem::replace(
                        &mut f.body,
                        almide_ir::IrExpr {
                            kind: almide_ir::IrExprKind::Unit,
                            ty: almide_lang::types::Ty::Unit,
                            span: None,
                            def_id: None,
                        },
                    );
                    f.body = almide_ir::IrExpr {
                        kind: almide_ir::IrExprKind::Block {
                            stmts: vec![bind_stmt],
                            expr: Some(Box::new(old)),
                        },
                        ty: old_ty,
                        span: old_span,
                        def_id: None,
                    };
                }
            }
            // Module siblings keep the raw substitution (their separate VarId
            // numbering region cannot carry a main-region bind id) — the ceangal
            // in-module reader class this path has always served.
            for f in module_fn_sibs.iter_mut() {
                f.body = almide_ir::substitute::substitute_var_in_expr(&f.body, *id, init);
            }
        }
        if !subs.is_empty() {
            for f in inlined_fns.iter_mut() {
                crate::lower::repair_record_literal_field_tys(f);
            }
        }
    }

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
            &main_globals,
            &main_global_inits,
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
    for func in &module_fn_sibs {
        // ORIGINAL bodies under the mangled name — every keyed lookup (never-err strip,
        // AUTO_WRAP ABI, `ret_is_result_abi`) sees the SAME name callers use via the
        // combined registry population above.
        if already.contains(func.name.as_str()) {
            continue;
        }
        if let Ok(mirs) = crate::lower::lower_function_all_with_globals(
            func,
            &globals,
            &global_inits,
            &record_layouts,
            &variant_layouts,
        ) {
            functions.extend(mirs);
        }
    }

    // MUTABLE-GLOBAL INITIALIZATION: synthesize `__mg_init` assigning each mutable
    // top-let its declared initializer (declaration order), lowered through the SAME
    // slot-routed Assign path user code uses (minus the old-value take/drop — the slots
    // start zeroed). `_start` calls it before `__global_init`/`main`. A non-lowerable
    // initializer WALLS the whole program: shipping zeroed globals would be a silent
    // miscompile, and the v0 fallback initializes them correctly.
    if !mutable_tls.is_empty() {
        let stmts: Vec<almide_ir::IrStmt> = mutable_tls
            .iter()
            .map(|tl| almide_ir::IrStmt {
                kind: almide_ir::IrStmtKind::Assign { var: tl.var, value: tl.value.clone() },
                span: None,
            })
            .collect();
        let body = almide_ir::IrExpr {
            kind: almide_ir::IrExprKind::Block { stmts, expr: None },
            ty: almide_lang::types::Ty::Unit,
            span: Default::default(),
            def_id: None,
        };
        let init_fn = almide_ir::IrFunction {
            name: almide_lang::intern::sym("__mg_init"),
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
        match crate::lower::lower_function_all_with_globals(
            &init_fn,
            &main_globals,
            &main_global_inits,
            &record_layouts,
            &variant_layouts,
        ) {
            Ok(mirs) => functions.extend(mirs),
            Err(e) => {
                return Err(LowerError::Unsupported(format!(
                    "mutable module-level var initializer outside the executable subset: {e:?}"
                )))
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
            // A `Map[String, <flat heap>]` drop renders `(call $__drop_map_hval …)` — a
            // map_hval helper that is NOT a registered call name, so force map_hval when
            // the DropVariant is present (the C-039 typechange twins build hval-flavor
            // maps without calling any registered map_hval fn — the value_core pattern).
            if entries.iter().any(|(impl_fn, _)| *impl_fn == "map_new_hval") {
                any_called = any_called
                    || functions.iter().any(|f| {
                        f.ops.iter().any(|op| matches!(op,
                            crate::Op::DropVariant { ty, .. } if ty == "map_hval"))
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

    // `pub fn` EXPORT roots (#457): a Public non-test MAIN-program fn must be a named wasm
    // export (host-invocable, the v0 emitter's export contract). One that LOWERED gets an
    // `(export …)` directive; one that WALLED cannot be exported — decline the WHOLE module
    // so the `--verified` pipeline falls back to v0 (which exports it) rather than shipping
    // an artifact silently missing a public entry point.
    let mut exports: Vec<(String, String, Vec<bool>, Option<bool>)> = Vec::new();
    let is_float_ty = |t: &almide_lang::types::Ty| {
        matches!(
            t,
            almide_lang::types::Ty::Float
                | almide_lang::types::Ty::Float32
                | almide_lang::types::Ty::Float64
        )
    };
    for func in &ir.functions {
        if !func.is_test
            && func.name.as_str() != "main"
            && !func.generics.as_ref().map_or(false, |g| !g.is_empty())
            && matches!(func.visibility, almide_ir::IrVisibility::Public)
        {
            let n = func.name.as_str();
            if functions.iter().any(|f| f.name == n) {
                // `@export(wasm, "sym")` overrides the export name (v0's criterion,
                // mod_p3.rs). v1's internal value model carries a Float as raw i64 BITS;
                // the renderer emits a reinterpret wrapper for Float-bearing signatures
                // so the public ABI presents real f64s (v0 parity).
                let export_name = func
                    .export_attrs
                    .iter()
                    .find(|a| a.target.as_str() == "wasm")
                    .map(|a| a.symbol.to_string())
                    .unwrap_or_else(|| n.to_string());
                let param_floats: Vec<bool> =
                    func.params.iter().map(|p| is_float_ty(&p.ty)).collect();
                let ret_float = match &func.ret_ty {
                    almide_lang::types::Ty::Unit => None,
                    t => Some(is_float_ty(t)),
                };
                exports.push((export_name, n.to_string(), param_floats, ret_float));
            } else {
                return Err(LowerError::Unsupported(format!(
                    "exported `pub fn {n}` is outside the MIR-lowering subset (the wasm module \
                     must carry its export)"
                )));
            }
        }
    }

    // Any UNLINKED stdlib/runtime call would render a dangling `(call $name)` (invalid wasm) — the
    // renderer rejects it cleanly. Returns the WAT on success.
    try_render_wasm_program(&MirProgram { functions, exports, mutable_global_count })
}

/// NATIVE leg of the trust spine (#764, rung 1): lower `.almd` source through the
/// SAME Perceus MIR the wasm leg uses and render it to native Rust — `Dup` as
/// `.clone()`, `Drop` erased to Rust's scope-end drop, the runtime boundary mapped
/// to a closed native shim floor. `verify_ownership` certifies the Perceus balance
/// on the same ops before the erasure. WALLS (`Err`) on anything outside the
/// rung-1 subset — the CLI falls back to v0, so a rendered program is never wrong.
/// Debug probe: dump the lowered MIR ops of every non-test fn (walls listed
/// per fn). Used by examples/probe_native.rs during rung development.
pub fn debug_dump_mir(source: &str) -> Result<String, LowerError> {
    crate::lower::STRICT_VALUES.store(true, std::sync::atomic::Ordering::Relaxed);
    let ir = source_to_ir_with(source, &[])?;
    let globals = std::collections::HashMap::new();
    let global_inits = std::collections::HashMap::new();
    // The REAL pipeline's layout registries — without them a record literal
    // lowers as an Opaque skeleton and a field read walls (the very first
    // rung-5 records probe misread that as a lowering gap).
    let record_layouts = crate::lower::build_record_layouts(&ir.type_decls);
    let variant_layouts = crate::lower::build_variant_layouts(&ir.type_decls);
    let mut out = String::new();
    for func in &ir.functions {
        if func.is_test {
            continue;
        }
        match crate::lower::lower_function_all_with_globals(func, &globals, &global_inits, &record_layouts, &variant_layouts) {
            Ok(all) => {
                for f in all {
                    out.push_str(&format!("== fn {} ==\n", f.name));
                    for op in &f.ops {
                        out.push_str(&format!("  {op:?}\n"));
                    }
                }
            }
            Err(e) => out.push_str(&format!("== fn {} LOWER-WALL: {e:?}\n", func.name)),
        }
    }
    Ok(out)
}

pub fn try_render_rust_source(source: &str) -> Result<String, LowerError> {
    crate::lower::STRICT_VALUES.store(true, std::sync::atomic::Ordering::Relaxed);
    let ir = source_to_ir_with(source, &[])?;
    // Rung-5 records slab: the layout registries the wasm leg threads — without
    // them a record literal lowers as an Opaque skeleton and every field read
    // strict-walls (the probe_native trap recorded in the trust-spine ledger).
    let record_layouts = crate::lower::build_record_layouts(&ir.type_decls);
    let variant_layouts = crate::lower::build_variant_layouts(&ir.type_decls);
    if !ir.modules.is_empty() {
        return Err(LowerError::Unsupported(
            "native: multi-module program — outside rung 1".into(),
        ));
    }
    if !ir.top_lets.is_empty() {
        return Err(LowerError::Unsupported(
            "native: top-level lets — outside rung 1".into(),
        ));
    }
    let globals = std::collections::HashMap::new();
    let mut functions = Vec::new();
    for func in &ir.functions {
        if func.is_test {
            continue;
        }
        // PRECISION WALL (rung 2): the native renderer types a heap `Repr::Ptr`
        // param/result as a STRING — only sound when the DECLARED type says so.
        // Any signature outside {Int, Bool, String, Unit-ret} walls here, where
        // the Almide-level `Ty` is still visible (`MirParam` carries only reprs).
        use almide_lang::types::Ty;
        use almide_lang::types::constructor::TypeConstructorId;
        let is_scalar_list = |t: &Ty| {
            matches!(t, Ty::Applied(TypeConstructorId::List, a)
                if a.len() == 1 && matches!(a[0], Ty::Int | Ty::Bool))
        };
        // Rung-5 records slab: an ALL-SCALAR record is layout-identical to a
        // scalar list (the DynList block), so its params/returns ride the same
        // `&[i64]`/`Vec<i64>` convention on native.
        let is_scalar_record = |t: &Ty| -> bool {
            let Ty::Named(n, args) = t else { return false };
            if !args.is_empty() { return false; }
            record_layouts
                .get(n.as_str())
                .is_some_and(|(_, ftys)| ftys.iter().all(|(_, ft)| !crate::lower::is_heap_ty(ft)))
        };
        // A FLAT variant (every ctor scalar-only) is likewise one slot block
        // (tag@0, payload@1+) — same `&[i64]` convention (rung-5 variants slab).
        let is_flat_variant = |t: &Ty| -> bool {
            let Ty::Named(n, args) = t else { return false };
            if !args.is_empty() { return false; }
            variant_layouts.by_type.get(n.as_str()).is_some_and(|layout| {
                layout.cases.iter().all(|c| c.fields.iter().all(|(_, ft)| !crate::lower::is_heap_ty(ft)))
            })
        };
        // Rung-5 closures slab: a SCALAR closure type (`(Int) -> Int` — every param
        // and the return scalar) travels as its env block (`Vec<i64>`: [fnidx,
        // drop-header, captures…]); invocation dispatches through the generated
        // `__almd_ci_*` tables. Heap-param/-return closures stay wasm-only.
        let is_scalar_fn = |t: &Ty| {
            matches!(t, Ty::Fn { params, ret }
                if params.iter().all(|p| matches!(p, Ty::Int | Ty::Bool))
                    && matches!(**ret, Ty::Int | Ty::Bool))
        };
        let sig_ok = |t: &Ty| {
            matches!(t, Ty::Int | Ty::Bool | Ty::Float | Ty::String)
                || is_scalar_list(t)
                || is_scalar_record(t)
                || is_flat_variant(t)
                || is_scalar_fn(t)
        };
        for p in &func.params {
            if !sig_ok(&p.ty) {
                return Err(LowerError::Unsupported(format!(
                    "native: fn `{}` param `{:?}` type — outside the native rung subset",
                    func.name, p.ty
                )));
            }
        }
        if !matches!(func.ret_ty, Ty::Unit) && !sig_ok(&func.ret_ty) {
            return Err(LowerError::Unsupported(format!(
                "native: fn `{}` return type {:?} — outside the native rung subset",
                func.name, func.ret_ty
            )));
        }
        // ALL-OR-NOTHING: any unlowerable fn walls the program (the native rungs
        // have no per-fn fallback — a partial native binary cannot call into v0).
        let all = crate::lower::lower_function_all_with_globals(
            func,
            &globals,
            &std::collections::HashMap::new(),
            &record_layouts,
            &variant_layouts,
        )
        .map_err(|e| {
            LowerError::Unsupported(format!("native: fn `{}`: {e:?}", func.name))
        })?;
        functions.extend(all);
    }
    if !functions.iter().any(|f| f.name == "main") {
        return Err(LowerError::Unsupported(
            "native: main is outside the MIR-lowering subset".into(),
        ));
    }
    // The SIG-KIND table (rung 4): the declared param/return kinds, computed here
    // where the Almide-level `Ty` is visible, so the native render can type a heap
    // `Repr::Ptr` as `&str` vs `&[i64]` per the declaration.
    let mut sigs: crate::render_native::NativeSigs = Default::default();
    {
        use almide_lang::types::constructor::TypeConstructorId;
        use almide_lang::types::Ty;
        use crate::render_native::NativeSigKind;
        let kind = |t: &Ty| -> Option<NativeSigKind> {
            match t {
                Ty::Int | Ty::Bool => Some(NativeSigKind::I64),
                Ty::Float => Some(NativeSigKind::F64),
                Ty::String => Some(NativeSigKind::Str),
                Ty::Applied(TypeConstructorId::List, a)
                    if a.len() == 1 && matches!(a[0], Ty::Int | Ty::Bool) =>
                {
                    Some(NativeSigKind::ListI64)
                }
                // An all-scalar record travels as its slot block (see sig_ok).
                Ty::Named(n, args)
                    if args.is_empty()
                        && record_layouts
                            .get(n.as_str())
                            .is_some_and(|(_, ftys)| ftys.iter().all(|(_, ft)| !crate::lower::is_heap_ty(ft))) =>
                {
                    Some(NativeSigKind::ListI64)
                }
                // A flat variant travels as its tag+payload slot block.
                Ty::Named(n, args)
                    if args.is_empty()
                        && variant_layouts.by_type.get(n.as_str()).is_some_and(|layout| {
                            layout.cases.iter().all(|c| c.fields.iter().all(|(_, ft)| !crate::lower::is_heap_ty(ft)))
                        }) =>
                {
                    Some(NativeSigKind::ListI64)
                }
                // A scalar closure travels as its env block (rung-5 closures slab).
                Ty::Fn { params, ret }
                    if params.iter().all(|p| matches!(p, Ty::Int | Ty::Bool))
                        && matches!(**ret, Ty::Int | Ty::Bool) =>
                {
                    Some(NativeSigKind::ListI64)
                }
                _ => None,
            }
        };
        for func in &ir.functions {
            if func.is_test {
                continue;
            }
            let params: Option<Vec<_>> = func.params.iter().map(|p| kind(&p.ty)).collect();
            let ret = if matches!(func.ret_ty, Ty::Unit) {
                Some(None)
            } else {
                kind(&func.ret_ty).map(Some)
            };
            if let (Some(ps), Some(r)) = (params, ret) {
                sigs.insert(func.name.as_str().to_string(), (ps, r));
            }
        }
        // LIFTED lambdas exist only as MirFunctions (no IR sig): param 0 is the env
        // block (`&[i64]`), the rest are the lambda's own params — SCALAR reprs only
        // in this slab (a heap-param lambda walls the program: all-or-nothing, and
        // its dispatch arm could not type). The return kind is body-derived by the
        // renderer, so it is not declared here.
        for f in &functions {
            if !f.name.starts_with("__lambda_") {
                continue;
            }
            let mut ps = vec![crate::render_native::NativeSigKind::ListI64];
            for p in &f.params[1..] {
                match p.repr {
                    crate::Repr::Scalar { .. } => {
                        ps.push(crate::render_native::NativeSigKind::I64)
                    }
                    _ => {
                        return Err(LowerError::Unsupported(format!(
                            "native: lambda `{}` heap param — outside the closures slab",
                            f.name
                        )))
                    }
                }
            }
            sigs.insert(f.name.clone(), (ps, Some(crate::render_native::NativeSigKind::I64)));
        }
    }
    crate::render_native::try_render_native_program(
        &MirProgram {
            functions,
            exports: Vec::new(),
            // Rung 1 walls every top-let above, so there are no mutable-global slots.
            mutable_global_count: 0,
        },
        &sigs,
    )
}
