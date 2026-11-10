//! The `__test_runner` protocol: promote a test file's `test` fns to ordinary
//! effect fns and synthesize the runner `main` that calls each in declaration
//! order, printing `test: <name> ... ` / `ok` per test.
//!
//! # Why it is HERE and not in a wasm leg
//!
//! It is the same reason [`link_ir`](crate::link_ir) is here. This transform
//! decides which programs `almide test --target wasm` can run at all, and it
//! used to live inside ONE of the two wasm legs — so the test runner rendered
//! through the incumbent while `build`/`run`/`check --target wasm` rendered
//! through the structural leg. The two disagreed in the direction that hides:
//! a file the DEFAULT build emits and runs correctly was reported
//! `SKIP (v1 wall — no verified wasm rendering)` and its tests never ran, out
//! of a command that then exited 0 (#2121).
//!
//! The transform itself is leg-independent — it is IR in, IR out, and names no
//! backend. A leg's own limits stay with that leg: `almide-mir` runs its
//! incumbent-brick refusal before calling this, and neither leg's refusal can
//! silently become the other's.

use almide_ir::IrProgram;

/// Why a program cannot be rendered as a test module. Leg-independent: this is
/// a property of the PROGRAM, and a caller maps it onto its own error type.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NotTestable(pub String);

impl std::fmt::Display for NotTestable {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}

/// Does this program declare any `test` block? The leg-specific refusals run
/// only for a program that reaches test synthesis at all, so a caller that has
/// one asks this first.
pub fn has_tests(ir: &IrProgram) -> bool {
    ir.functions.iter().any(|f| f.is_test)
}

/// Does the runner run `f` under `--run <run_filter>`? The ONE selection
/// predicate: the synthesis below keeps exactly these tests, and the harness
/// counts a passing run's tests from it (#3489) instead of from anything the
/// run printed — a program can print the runner's `ok` itself.
pub fn runs_test(f: &almide_ir::IrFunction, run_filter: Option<&str>) -> bool {
    f.is_test
        && run_filter.is_none_or(|pattern| almide_lang::almide_base::names::test_name_matches_filter(f.display_name(), pattern))
}

/// Promote a test file's `test` fns to ordinary effect fns and synthesize the
/// runner `main` (the `__test_runner` protocol). `run_filter` is `almide test
/// --run <pattern>`; it selects with the SAME predicate the native leg's
/// emitted fn name comes from, so every leg runs the same set of tests.
pub fn synthesize_test_runner_main(
    ir: &mut IrProgram,
    run_filter: Option<&str>,
) -> Result<(), NotTestable> {
    synthesize(ir, run_filter, ModuleReinit::ProgramUniqueIds)
}

/// The same synthesis for a leg whose module VarIds are NOT program-unique —
/// the structural wasm leg keeps each module's own VarTable and names a
/// global by (space, VarId) (#1596). A module's mutable top-lets cannot be
/// re-assigned from the runner `main`: main's space is the entry program's,
/// so a module id there is unmapped (`assign:unmapped`, #2751) or — worse —
/// equal to an entry global's id, which the re-init then overwrites with the
/// module's initializer. Each such module instead gets a synthesized
/// `__almd_test_reinit` fn holding its re-assigns, lowered in the module's
/// own space like any module fn, and the runner calls it before every test.
pub fn synthesize_test_runner_main_spaced(
    ir: &mut IrProgram,
    run_filter: Option<&str>,
) -> Result<(), NotTestable> {
    synthesize(ir, run_filter, ModuleReinit::PerModuleFn)
}

/// How the runner re-initializes a MODULE's mutable top-lets before a test.
#[derive(Clone, Copy, PartialEq, Eq)]
enum ModuleReinit {
    /// Inline `Assign`s in the runner main (the incumbent pipeline, whose
    /// `disambiguate_module_global_regions` made every module id unique).
    ProgramUniqueIds,
    /// One `__almd_test_reinit` fn per module, called from the runner main.
    PerModuleFn,
}

/// The per-module re-init fn's name (`ModuleReinit::PerModuleFn`).
pub const MODULE_REINIT_FN: &str = "__almd_test_reinit";

fn synthesize(ir: &mut IrProgram, run_filter: Option<&str>, module_reinit: ModuleReinit) -> Result<(), NotTestable> {
    use almide_ir::{CallTarget, IrExpr, IrExprKind, IrStmt, IrStmtKind};
    use almide_lang::intern::sym;
    use almide_lang::types::Ty;
    let has_tests = has_tests(ir);
    let unit = || IrExpr { kind: IrExprKind::Unit, ty: Ty::Unit, span: None, def_id: None };
    if let Some(main_idx) =
        ir.functions.iter().position(|f| !f.is_test && f.name.as_str() == "main")
    {
        if !has_tests {
            // main-mode: both legs run main only (v0's `__main_runner` protocol).
            return Ok(());
        }
        // main + test blocks. NATIVE test mode compiles `main` but never calls it —
        // cargo's harness runs the `#[test]` fns alone. Mirror that: drop the user
        // `main` so the synthesized runner is the entry and the TESTS run. The old
        // behaviour kept `main` as the entry and left the tests unlowered, so the
        // harness skipped the file to native ("wasm test-mode runs main only") —
        // 17 of the 32 fallbacks in `almide test` were this one harness gap, not a
        // v1 subset wall (#813). These fixtures' `main` is separately exercised in
        // MAIN mode by the cross-target parity gate, so nothing loses coverage.
        ir.functions.remove(main_idx);
    }
    if !has_tests {
        return Err(NotTestable(
            "test mode: no `main` and no test blocks — nothing to run".into(),
        ));
    }
    // v0's `__test_runner` re-initializes module globals before EVERY test (native
    // thread-isolation parity: each `#[test]` gets a fresh thread, so a native test
    // never sees a sibling's writes). The v1 `_start` runs `__global_init`/`__mg_init`
    // ONCE — so the runner main re-ASSIGNS every MUTABLE top-let to its initializer
    // before each test (the ordinary `lower_mutable_global_assign` path: take +
    // drop-old + store — no leak, no new runtime). An IMMUTABLE top-let cannot change
    // between tests and needs no re-init.
    //
    // MODULE top-lets are included (#1233 — the per-test region bridge): the
    // `disambiguate_module_global_regions` pass now runs BEFORE this synthesis, so
    // every module id is program-unique and a main-region `Assign` naming one cannot
    // collide with an unrelated main-side id. `assign_mutable_global_slots` unions the
    // same two sources and sorts by raw id, so ordering the re-inits the same way
    // replays declaration order slot-for-slot, exactly as `__mg_init` does.
    let reassign = |tl: &almide_ir::IrTopLet| IrStmt {
        kind: IrStmtKind::Assign { var: tl.var, value: tl.value.clone() },
        span: None,
    };
    let inline_stmts: Vec<IrStmt> = {
        let module_tls = ir.modules.iter().flat_map(|m| m.top_lets.iter());
        let mut mutable_tls: Vec<&almide_ir::IrTopLet> = match module_reinit {
            ModuleReinit::ProgramUniqueIds => ir.top_lets.iter().chain(module_tls).filter(|tl| tl.mutable).collect(),
            ModuleReinit::PerModuleFn => ir.top_lets.iter().filter(|tl| tl.mutable).collect(),
        };
        mutable_tls.sort_by_key(|tl| tl.var.0);
        mutable_tls.iter().map(|tl| reassign(tl)).collect()
    };
    let mut reinit_stmts: Vec<IrStmt> = Vec::new();
    if module_reinit == ModuleReinit::PerModuleFn {
        // Modules first, in `ir.modules` order (the resolver's, leaves first),
        // then the entry's own top-lets below.
        for m in ir.modules.iter_mut() {
            let mut tls: Vec<&almide_ir::IrTopLet> = m.top_lets.iter().filter(|tl| tl.mutable).collect();
            if tls.is_empty() {
                continue;
            }
            tls.sort_by_key(|tl| tl.var.0);
            let body = IrExpr {
                kind: IrExprKind::Block { stmts: tls.iter().map(|tl| reassign(tl)).collect(), expr: Some(Box::new(unit())) },
                ty: Ty::Unit,
                span: None,
                def_id: None,
            };
            m.functions.push(runner_fn(sym(MODULE_REINIT_FN), body, false));
            reinit_stmts.push(IrStmt {
                kind: IrStmtKind::Expr {
                    expr: IrExpr {
                        kind: IrExprKind::Call {
                            target: CallTarget::Module { module: m.name, func: sym(MODULE_REINIT_FN), def_id: None },
                            args: Vec::new(),
                            type_args: Vec::new(),
                        },
                        ty: Ty::Unit,
                        span: None,
                        def_id: None,
                    },
                },
                span: None,
            });
        }
    }
    reinit_stmts.extend(inline_stmts);
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
    // `--run <pattern>` (#2085). Dropped here rather than before the wall checks
    // above so a filtered run walls exactly where an unfiltered one does —
    // otherwise `--run` would quietly change WHICH files reach the wasm leg.
    // The predicate is `almide_base::names`, the same one the native leg's
    // emitted fn name comes from: the two legs must select the same tests, and
    // when this leg selected all of them regardless, no gate could see it.
    if let Some(pattern) = run_filter {
        ir.functions.retain(|f| !f.is_test || runs_test(f, Some(pattern)));
    }
    let mut stmts: Vec<IrStmt> = Vec::new();
    let mut idx = 0usize;
    for f in ir.functions.iter_mut() {
        if !f.is_test {
            continue;
        }
        let display = f.display_name().to_string();
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
        kind: IrExprKind::Block { stmts, expr: Some(Box::new(unit())) },
        ty: Ty::Unit,
        span: None,
        def_id: None,
    };
    let body = if ir.functions.iter().any(|f| f.name.as_str() == almide_ir::ABORT_DISPATCH_FN) {
        abort_site_selector(body)
    } else {
        body
    };
    ir.functions.push(runner_fn(sym("main"), body, true));
    Ok(())
}

/// `testing.assert_aborts` (ADR-0029 D3): the runner `main` of a program with
/// abort sites runs ONE site when the host started this instance for it, and
/// the tests otherwise: `if testing.__abort_site_selected() >= 0 then
/// __almd_abort_dispatch(testing.__abort_site_selected())! else <tests>`.
fn abort_site_selector(tests: almide_ir::IrExpr) -> almide_ir::IrExpr {
    use almide_ir::{BinOp, CallTarget, IrExpr, IrExprKind};
    use almide_lang::intern::sym;
    use almide_lang::types::Ty;
    let mk = |kind: IrExprKind, ty: Ty| IrExpr { kind, ty, span: None, def_id: None };
    let selected = || {
        let target = CallTarget::Module { module: sym("testing"), func: sym("__abort_site_selected"), def_id: None };
        mk(IrExprKind::Call { target, args: Vec::new(), type_args: Vec::new() }, Ty::Int)
    };
    let zero = mk(IrExprKind::LitInt { value: 0 }, Ty::Int);
    let cond = mk(IrExprKind::BinOp { op: BinOp::Gte, left: Box::new(selected()), right: Box::new(zero) }, Ty::Bool);
    let target = CallTarget::Named { name: sym(almide_ir::ABORT_DISPATCH_FN) };
    let dispatch = mk(IrExprKind::Call { target, args: vec![selected()], type_args: Vec::new() }, Ty::result(Ty::Unit, Ty::String));
    let run = mk(IrExprKind::Try { expr: Box::new(dispatch) }, Ty::Unit);
    mk(IrExprKind::If { cond: Box::new(cond), then: Box::new(run), else_: Box::new(tests) }, Ty::Unit)
}

/// A synthesized zero-param Unit fn of the runner protocol (the runner `main`,
/// a module's `__almd_test_reinit`).
fn runner_fn(name: almide_lang::intern::Sym, body: almide_ir::IrExpr, is_effect: bool) -> almide_ir::IrFunction {
    almide_ir::IrFunction {
        name,
        params: vec![],
        ret_ty: almide_lang::types::Ty::Unit,
        body,
        is_effect,
        is_test: false,
        generics: None,
        extern_attrs: vec![],
        export_attrs: vec![],
        attrs: vec![],
        visibility: almide_ir::IrVisibility::Public,
        doc: None,
        blank_lines_before: 0,
        def_id: None,
        mutated_params: vec![], // fresh-fn: synthesized runner fn, zero params
        module_origin: None,
    }
}

/// The in-test assert lowering of the structural leg (#2179). The frontend
/// leaves `assert` / `assert_eq` / `assert_ne` RAW inside `test` blocks —
/// native's cargo harness wants the Rust macros there, per-test failure and
/// all — so a wasm leg has to lower them itself. The structural leg had
/// nothing (`Unsupported("call:assert_eq")`, the wall that kept the test lane
/// on the incumbent). This rewrites every raw in-test assert into the abort
/// form the frontend already gives a NON-test assert (`desugar_assert_abort`):
/// bind the operands, `eprintln` the `Error: assertion failed … at: line N`
/// block, `process.exit(1)` — the same bytes a failing non-test assert prints
/// on every target, and a leg that needs to know nothing about asserts.
///
/// Leg-independent IR in, IR out, and it lives here for the same reason the
/// runner synthesis does. It is NOT applied inside [`synthesize_test_runner_main`]
/// yet: the incumbent brick cannot lower the `if` this produces over two
/// Result- or Option-typed operands with a call-bearing else arm ("unresolvable
/// condition … Var vs Var"), so it keeps its own `hoist_assert` +
/// `assert_die_expr` trap form until it retires (#1584). The structural
/// front (`wasm_leg::lower_to_ir_tests_with_deps`) runs this before the
/// synthesis.
pub fn desugar_test_asserts(ir: &mut IrProgram) {
    use almide_ir::visit_mut::IrMutVisitor;
    let IrProgram { functions, var_table, .. } = ir;
    for f in functions.iter_mut().filter(|f| f.is_test) {
        Desugar { vars: var_table }.visit_expr_mut(&mut f.body);
    }
}

/// [`desugar_test_asserts`] for the test files of the wasm test run's one
/// program (#4441), whose tests are module fns — each module's own var table.
pub fn desugar_module_test_asserts(ir: &mut IrProgram) {
    use almide_ir::visit_mut::IrMutVisitor;
    for m in ir.modules.iter_mut() {
        let almide_ir::IrModule { functions, var_table, .. } = m;
        for f in functions.iter_mut().filter(|f| f.is_test) {
            Desugar { vars: var_table }.visit_expr_mut(&mut f.body);
        }
    }
}

struct Desugar<'a> {
    vars: &'a mut almide_ir::VarTable,
}

impl almide_ir::visit_mut::IrMutVisitor for Desugar<'_> {
    fn visit_expr_mut(&mut self, expr: &mut almide_ir::IrExpr) {
        use almide_ir::{CallTarget, IrExprKind};
        almide_ir::visit_mut::walk_expr_mut(self, expr);
        let IrExprKind::Call { target: CallTarget::Named { name }, args, .. } = &expr.kind else {
            return;
        };
        let n = name.as_str();
        let is_assert = (matches!(n, "assert_eq" | "assert_ne") && args.len() == 2)
            || (n == "assert" && !args.is_empty());
        if !is_assert {
            return;
        }
        let n = n.to_string();
        let span = expr.span;
        let IrExprKind::Call { args, .. } = std::mem::replace(&mut expr.kind, IrExprKind::Unit) else {
            unreachable!()
        };
        *expr = assert_abort(self.vars, &n, args, span);
    }
}

/// One test file of the wasm test run's one program (#4441): its module, and
/// the modules its own program would have loaded (their mutable top-lets are
/// what its runner re-initializes before each test, as its own runner did).
pub struct TestRunFile {
    pub module: almide_lang::intern::Sym,
    pub reached: std::collections::HashSet<almide_lang::intern::Sym>,
}

/// The runner `main` of the wasm test run's one program (#4441): every test
/// file is a module, and the instance's selection
/// (`almide_base::names::test_run_selection`, op 118) picks what runs —
/// test file `k`'s tests, in order, exactly as file `k`'s own runner ran them
/// (`test: <name> ... ` / `ok`, its modules' mutable top-lets re-initialized
/// before each), or an abort-site child of file `k`. Returns, per file, the
/// lowered test count and how many `run_filter` selects.
pub fn synthesize_test_run_main(
    ir: &mut IrProgram,
    files: &[TestRunFile],
    run_filter: Option<&str>,
) -> Result<Vec<(usize, usize)>, NotTestable> {
    use almide_base::names::test_run_selection::{self as sel, TEST_RUN_SITE_STRIDE};
    use almide_ir::{BinOp, CallTarget, IrExpr, IrExprKind, IrStmt, IrStmtKind};
    use almide_lang::intern::sym;
    use almide_lang::types::Ty;
    let mk = |kind: IrExprKind, ty: Ty| IrExpr { kind, ty, span: None, def_id: None };
    let unit = || IrExpr { kind: IrExprKind::Unit, ty: Ty::Unit, span: None, def_id: None };
    let stmt = |expr: IrExpr| IrStmt { kind: IrStmtKind::Expr { expr }, span: None };
    let println_stmt = |text: String| {
        let arg = IrExpr { kind: IrExprKind::LitStr { value: text }, ty: Ty::String, span: None, def_id: None };
        stmt(IrExpr {
            kind: IrExprKind::Call { target: CallTarget::Named { name: sym("println") }, args: vec![arg], type_args: Vec::new() },
            ty: Ty::Unit,
            span: None,
            def_id: None,
        })
    };
    let module_call = |module: almide_lang::intern::Sym, func: &str, args: Vec<IrExpr>, ty: Ty| {
        mk(IrExprKind::Call { target: CallTarget::Module { module, func: sym(func), def_id: None }, args, type_args: Vec::new() }, ty)
    };
    // Each module's re-init fn, as the spaced single-file runner makes it.
    let mut reinit: Vec<(almide_lang::intern::Sym, IrStmt)> = Vec::new();
    for m in ir.modules.iter_mut() {
        let mut tls: Vec<&almide_ir::IrTopLet> = m.top_lets.iter().filter(|tl| tl.mutable).collect();
        if tls.is_empty() {
            continue;
        }
        tls.sort_by_key(|tl| tl.var.0);
        let stmts = tls.iter().map(|tl| IrStmt { kind: IrStmtKind::Assign { var: tl.var, value: tl.value.clone() }, span: None }).collect();
        let body = mk(IrExprKind::Block { stmts, expr: Some(Box::new(unit())) }, Ty::Unit);
        m.functions.push(runner_fn(sym(MODULE_REINIT_FN), body, false));
        reinit.push((m.name, stmt(module_call(m.name, MODULE_REINIT_FN, Vec::new(), Ty::Unit))));
    }
    let selected_now = || module_call(sym("testing"), "__abort_site_selected", Vec::new(), Ty::Int);
    let int = |v: i64| mk(IrExprKind::LitInt { value: v }, Ty::Int);
    let binop = |op: BinOp, l: IrExpr, r: IrExpr, ty: Ty| mk(IrExprKind::BinOp { op, left: Box::new(l), right: Box::new(r) }, ty);
    let mut counts = Vec::new();
    let mut run_chain = unit();
    let mut site_chain = unit();
    let mut any_test = false;
    for (k, file) in files.iter().enumerate().rev() {
        let Some(m) = ir.modules.iter_mut().find(|m| m.name == file.module) else {
            return Err(NotTestable(format!("test run: no module {}", file.module)));
        };
        let lowered = m.functions.iter().filter(|f| f.is_test).count();
        if let Some(pattern) = run_filter {
            m.functions.retain(|f| !f.is_test || runs_test(f, Some(pattern)));
        }
        let mut stmts = Vec::new();
        let mut selected = 0usize;
        let module = m.name;
        let has_dispatch = m.functions.iter().any(|f| f.name.as_str() == almide_ir::ABORT_DISPATCH_FN);
        for (idx, f) in m.functions.iter_mut().filter(|f| f.is_test).enumerate() {
            let display = f.display_name().to_string();
            let mangled = format!("__almd_test_{idx}");
            selected = idx + 1;
            f.name = sym(&mangled);
            f.is_test = false;
            stmts.extend(reinit.iter().filter(|(name, _)| file.reached.contains(name) || *name == module).map(|(_, s)| s.clone()));
            stmts.push(println_stmt(format!("test: {display} ... ")));
            let call = module_call(module, &mangled, Vec::new(), Ty::result(Ty::Unit, Ty::String));
            stmts.push(stmt(mk(IrExprKind::Try { expr: Box::new(call) }, Ty::Unit)));
            stmts.push(println_stmt("ok".to_string()));
        }
        any_test |= lowered > 0;
        counts.push((lowered, selected));
        let block = mk(IrExprKind::Block { stmts, expr: Some(Box::new(unit())) }, Ty::Unit);
        let is_file = binop(BinOp::Eq, selected_now(), int(sel::file(k)), Ty::Bool);
        run_chain = mk(IrExprKind::If { cond: Box::new(is_file), then: Box::new(block), else_: Box::new(run_chain) }, Ty::Unit);
        if has_dispatch {
            let base = k as i64 * TEST_RUN_SITE_STRIDE;
            let lo = binop(BinOp::Gte, selected_now(), int(base), Ty::Bool);
            let hi = binop(BinOp::Lt, selected_now(), int(base + TEST_RUN_SITE_STRIDE), Ty::Bool);
            let site = binop(BinOp::SubInt, selected_now(), int(base), Ty::Int);
            let dispatch = module_call(module, almide_ir::ABORT_DISPATCH_FN, vec![site], Ty::result(Ty::Unit, Ty::String));
            let run = mk(IrExprKind::Try { expr: Box::new(dispatch) }, Ty::Unit);
            let cond = binop(BinOp::And, lo, hi, Ty::Bool);
            site_chain = mk(IrExprKind::If { cond: Box::new(cond), then: Box::new(run), else_: Box::new(site_chain) }, Ty::Unit);
        }
    }
    counts.reverse();
    if !any_test {
        return Err(NotTestable("test run: no test blocks — nothing to run".into()));
    }
    let is_site = binop(BinOp::Gte, selected_now(), int(0), Ty::Bool);
    let body = mk(IrExprKind::If { cond: Box::new(is_site), then: Box::new(site_chain), else_: Box::new(run_chain) }, Ty::Unit);
    ir.functions.push(runner_fn(sym("main"), body, true));
    Ok(counts)
}

/// The abort form of one assert — the frontend's `desugar_assert_abort`, over
/// IR that has already left the lowering context (fresh vars come from the
/// program's own table).
fn assert_abort(
    vars: &mut almide_ir::VarTable,
    name: &str,
    args: Vec<almide_ir::IrExpr>,
    span: Option<almide_ir::Span>,
) -> almide_ir::IrExpr {
    use almide_ir::{BinOp, CallTarget, IrExpr, IrExprKind, IrStmt, IrStmtKind, IrStringPart, Mutability};
    use almide_lang::intern::sym;
    use almide_lang::types::Ty;
    let mk = |kind: IrExprKind, ty: Ty| IrExpr { kind, ty, span, def_id: None };
    let mut stmts: Vec<IrStmt> = Vec::new();
    let mut operands: Vec<IrExpr> = Vec::new();
    for (i, a) in args.into_iter().enumerate() {
        // The message belongs to the failure arm. Binding it here would
        // evaluate its effects (and `!`) even when the assertion succeeds.
        if name == "assert" && i == 1 {
            operands.push(a);
            continue;
        }
        let ty = a.ty.clone();
        let v = vars.alloc(sym(&format!("__assert_{i}")), ty.clone(), Mutability::Let, span);
        stmts.push(IrStmt {
            kind: IrStmtKind::Bind { var: v, mutability: Mutability::Let, ty: ty.clone(), value: a },
            span: None,
        });
        operands.push(mk(IrExprKind::Var { id: v }, ty));
    }
    let cond = match name {
        "assert_eq" | "assert_ne" => mk(
            IrExprKind::BinOp {
                op: if name == "assert_eq" { BinOp::Eq } else { BinOp::Neq },
                left: Box::new(operands[0].clone()),
                right: Box::new(operands[1].clone()),
            },
            Ty::Bool,
        ),
        _ => operands[0].clone(),
    };
    let at = span.map(|s| format!("\n  at: line {}", s.line)).unwrap_or_default();
    // An operand as the frontend's `assert_shown` shows it (#4262): its
    // rendering through C-418's escaper, so the program's control and bidi
    // characters never reach the block raw. (The frontend already lowers an
    // in-test `assert_eq` / `assert_ne` to the harness `assert(cond, msg)`;
    // this arm is for IR that did not come through it.)
    let shown = |e: &IrExpr| {
        let text = if matches!(e.ty, Ty::String) {
            e.clone()
        } else {
            mk(IrExprKind::StringInterp { parts: vec![IrStringPart::Expr { expr: e.clone() }] }, Ty::String)
        };
        mk(
            IrExprKind::Call {
                target: CallTarget::Module { module: sym("string"), func: sym("__err_text"), def_id: None },
                args: vec![text],
                type_args: vec![],
            },
            Ty::String,
        )
    };
    let parts: Vec<IrStringPart> = match name {
        "assert_eq" => vec![
            IrStringPart::Lit { value: format!("Error: assertion failed{at}\n  expected: ") },
            IrStringPart::Expr { expr: shown(&operands[1]) },
            IrStringPart::Lit { value: "\n  found: ".into() },
            IrStringPart::Expr { expr: shown(&operands[0]) },
        ],
        "assert_ne" => vec![
            IrStringPart::Lit { value: format!("Error: assertion failed{at}\n  expected: != ") },
            IrStringPart::Expr { expr: shown(&operands[0]) },
            IrStringPart::Lit { value: "\n  found: ".into() },
            IrStringPart::Expr { expr: shown(&operands[0]) },
        ],
        _ if operands.len() >= 2 => {
            let mut p = vec![
                IrStringPart::Lit { value: "Error: assertion failed: ".into() },
                IrStringPart::Expr { expr: operands[1].clone() },
            ];
            if !at.is_empty() {
                p.push(IrStringPart::Lit { value: at.clone() });
            }
            p
        }
        _ => vec![IrStringPart::Lit { value: format!("Error: assertion failed{at}") }],
    };
    let msg = mk(IrExprKind::StringInterp { parts }, Ty::String);
    let eprint = mk(
        IrExprKind::Call {
            target: CallTarget::Named { name: sym("eprintln") },
            args: vec![msg],
            type_args: vec![],
        },
        Ty::Unit,
    );
    let one = mk(IrExprKind::LitInt { value: 1 }, Ty::Int);
    let exit = mk(
        IrExprKind::Call {
            target: CallTarget::Module { module: sym("process"), func: sym("exit"), def_id: None },
            args: vec![one],
            type_args: vec![],
        },
        Ty::Unit,
    );
    let fail = mk(
        IrExprKind::Block {
            stmts: vec![
                IrStmt { kind: IrStmtKind::Expr { expr: eprint }, span: None },
                IrStmt { kind: IrStmtKind::Expr { expr: exit }, span: None },
            ],
            expr: None,
        },
        Ty::Unit,
    );
    let ok = mk(IrExprKind::Block { stmts: vec![], expr: None }, Ty::Unit);
    let guard = mk(
        IrExprKind::If { cond: Box::new(cond), then: Box::new(ok), else_: Box::new(fail) },
        Ty::Unit,
    );
    stmts.push(IrStmt { kind: IrStmtKind::Expr { expr: guard }, span: None });
    mk(IrExprKind::Block { stmts, expr: None }, Ty::Unit)
}
