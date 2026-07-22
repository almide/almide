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
    let mut parser = Parser::new(tokens);
    let mut prog = parser
        .parse()
        .map_err(|e| LowerError::Unsupported(format!("parse error: {e:?}")))?;
    // `Parser::parse()` is a recovery parser: it can return `Ok` with a
    // partial `Program` (unparseable top-level items dropped) while still
    // recording the failures in `.errors` — the CLI's own `parse_file`
    // checks this separately (main.rs). Skipping this check here would
    // silently compile a truncated program instead of walling honestly.
    if !parser.errors.is_empty() {
        let messages: Vec<String> = parser.errors.iter().map(|d| d.display()).collect();
        return Err(LowerError::Unsupported(format!(
            "parse error: {}",
            messages.join("\n")
        )));
    }
    let canon = canonicalize::canonicalize_program(
        &prog,
        modules.iter().map(|(n, p, s)| (n.as_str(), p, *s)),
    );
    let mut checker = Checker::from_env(canon.env);
    // #785 parity with the CLI drivers: module top-let types must be fully
    // inferred BEFORE the entry program reads them. Without this pre-pass a
    // cross-module reader of a generic-ctor top-let (`let MAYBE = some(Cfg
    // {…})`) sees the registration seed `Option[Unknown]`, the match payload
    // binding stays Unknown, and the whole program walls.
    for (name, mod_prog, _) in modules {
        if almide_lang::stdlib_info::is_stdlib_module(name)
            && !almide_lang::stdlib_info::is_bundled_module(name)
        {
            continue;
        }
        checker.refresh_module_top_lets(mod_prog, name);
    }
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
    // #806 step 2: small pure-scalar fns inline as reduced expressions at their
    // call sites (shared with classify: desugar-before-both).
    crate::lower::inline_small_scalar_fns(&mut ir);
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
    let mut ir = build_ir_with_drops(source, self_modules, test_mode)?;
    try_render_wasm_source_impl_rest(&mut ir, verbose)
}

/// Phase 1: synthesize the recursive-drop / repr source text this program's linked
/// types need, splice it into the source, and re-lower (v1-trust-spine-only — v0
/// manages its own memory). In `test_mode`, promote `test "…"` fns to a synthesized
/// runner `main`. Returns the FINAL linked `IrProgram` the rest of the pipeline
/// (globals, layouts, MIR lowering) continues from.
fn build_ir_with_drops(
    source: &str,
    self_modules: &[(String, almide_lang::ast::Program, bool)],
    test_mode: bool,
) -> Result<almide_ir::IrProgram, LowerError> {
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
    // A `List[(String, <Fn>)]` pairs literal (the closure-valued map's from_list
    // input) routes its scope-end drop to `$__drop_list_str_clo` (per-tuple: key
    // rc_dec + `$__drop_closure` on the value slot).
    let list_str_clo_drop = if crate::lower::program_uses_str_clo_pairs(&ir) {
        crate::lower::LIST_STR_CLO_DROP_SRC
    } else {
        ""
    };
    // An `Option[(String, String)]` (the if-merged `some((s1, s2))` ctor) routes
    // its scope-end drop to `$__drop_opt_str_str`.
    let opt_str_str_drop = if crate::lower::program_uses_opt_str_str(&ir) {
        crate::lower::OPT_STR_STR_DROP_SRC
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
        "{}{}{}{}{}{}{}{}{}{}{}{}{}{}",
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
        list_str_clo_drop,
        opt_str_str_drop,
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
    Ok(ir)
}

include!("pipeline_b.rs");
include!("pipeline_c.rs");
