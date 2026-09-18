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

/// Promote a test file's `test` fns to ordinary effect fns and synthesize the
/// runner `main` (the `__test_runner` protocol). `run_filter` is `almide test
/// --run <pattern>`; it selects with the SAME predicate the native leg's
/// emitted fn name comes from, so every leg runs the same set of tests.
pub fn synthesize_test_runner_main(
    ir: &mut IrProgram,
    run_filter: Option<&str>,
) -> Result<(), NotTestable> {
    use almide_ir::{CallTarget, IrExpr, IrExprKind, IrStmt, IrStmtKind};
    use almide_lang::intern::sym;
    use almide_lang::types::Ty;
    let has_tests = has_tests(ir);
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
    let mut mutable_tls: Vec<&almide_ir::IrTopLet> = ir
        .top_lets
        .iter()
        .chain(ir.modules.iter().flat_map(|m| m.top_lets.iter()))
        .filter(|tl| tl.mutable)
        .collect();
    mutable_tls.sort_by_key(|tl| tl.var.0);
    let reinit_stmts: Vec<IrStmt> = mutable_tls
        .iter()
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
    // `--run <pattern>` (#2085). Dropped here rather than before the wall checks
    // above so a filtered run walls exactly where an unfiltered one does —
    // otherwise `--run` would quietly change WHICH files reach the wasm leg.
    // The predicate is `almide_base::names`, the same one the native leg's
    // emitted fn name comes from: the two legs must select the same tests, and
    // when this leg selected all of them regardless, no gate could see it.
    if let Some(pattern) = run_filter {
        ir.functions.retain(|f| {
            !f.is_test
                || almide_lang::almide_base::names::test_name_matches_filter(f.name.as_str(), pattern)
        });
    }
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
        is_test: false,
        generics: None,
        extern_attrs: vec![],
        export_attrs: vec![],
        attrs: vec![],
        visibility: almide_ir::IrVisibility::Public,
        doc: None,
        blank_lines_before: 0,
        def_id: None,
        mutated_params: vec![], // fresh-fn: synthesized test-runner main, zero params
        module_origin: None,
    });
    Ok(())
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
    use almide_ir::visit_mut::{walk_expr_mut, IrMutVisitor};
    use almide_ir::{CallTarget, IrExpr, IrExprKind, VarTable};
    struct Desugar<'a> {
        vars: &'a mut VarTable,
    }
    impl IrMutVisitor for Desugar<'_> {
        fn visit_expr_mut(&mut self, expr: &mut IrExpr) {
            walk_expr_mut(self, expr);
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
    let IrProgram { functions, var_table, .. } = ir;
    for f in functions.iter_mut().filter(|f| f.is_test) {
        Desugar { vars: var_table }.visit_expr_mut(&mut f.body);
    }
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
    let parts: Vec<IrStringPart> = match name {
        "assert_eq" => vec![
            IrStringPart::Lit { value: format!("Error: assertion failed{at}\n  expected: ") },
            IrStringPart::Expr { expr: operands[1].clone() },
            IrStringPart::Lit { value: "\n  found: ".into() },
            IrStringPart::Expr { expr: operands[0].clone() },
        ],
        "assert_ne" => vec![
            IrStringPart::Lit { value: format!("Error: assertion failed{at}\n  expected: != ") },
            IrStringPart::Expr { expr: operands[0].clone() },
            IrStringPart::Lit { value: "\n  found: ".into() },
            IrStringPart::Expr { expr: operands[0].clone() },
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
