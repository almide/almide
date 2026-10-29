// The T18 abort form, shared by the prelude asserts (C-153), the snapshot
// assert (C-339) and the `testing.assert_*` library asserts (C-381).
// `include!`d by calls.rs (the 800-line file budget); it shares that
// module's scope and imports.

/// Bind every operand to a `__assert_<i>` temp, in order, so each is
/// evaluated exactly once: the failure block re-references the temps and
/// never re-runs the operand expressions.
///
/// A literal is evaluated in place: reading it twice reads the same value.
/// A variable operand is read in place too: a temp would be a second owner,
/// and the native leg cloned the whole value into it on every call. It is
/// read in place only when no operand can write it between the reads — the
/// binding is a `let`, or every operand is a variable or a literal.
fn bind_assert_operands(
    ctx: &mut LowerCtx,
    ir_args: Vec<IrExpr>,
    span: Option<ast::Span>,
) -> (Vec<IrStmt>, Vec<IrExpr>) {
    let mut stmts: Vec<IrStmt> = Vec::new();
    let mut vars: Vec<IrExpr> = Vec::new();
    let literal = |a: &IrExpr| {
        matches!(
            a.kind,
            IrExprKind::LitInt { .. } | IrExprKind::LitFloat { .. } | IrExprKind::LitStr { .. } | IrExprKind::LitBool { .. } | IrExprKind::Unit
        )
    };
    let inert = ir_args.iter().all(|a| literal(a) || matches!(a.kind, IrExprKind::Var { .. }));
    for (i, a) in ir_args.into_iter().enumerate() {
        let in_place = match a.kind {
            IrExprKind::Var { id } => inert || ctx.var_table.get(id).mutability == Mutability::Let,
            _ => literal(&a),
        };
        if in_place {
            vars.push(a);
            continue;
        }
        let a_ty = a.ty.clone();
        let v = ctx.define_var(&format!("__assert_{i}"), a_ty.clone(), Mutability::Let, None);
        stmts.push(IrStmt {
            kind: IrStmtKind::Bind { var: v, mutability: Mutability::Let, ty: a_ty.clone(), value: a },
            span: None,
        });
        vars.push(ctx.mk(IrExprKind::Var { id: v }, a_ty, span));
    }
    (stmts, vars)
}

/// An assert operand as its failure text shows it (#4262): the ALS-R2
/// rendering (through a declared repr where one exists) passed through C-418's
/// escaper `string.__err_text`, so a control or bidi character of the
/// program's data reads `\n` / `\u{1b}` / `\u{202e}` and a newline cannot
/// forge a line of the block. One rule on every lane and route: the program's
/// own abort, both test lanes, both legs.
fn assert_shown(ctx: &mut LowerCtx, operand: IrExpr, span: Option<ast::Span>) -> IrExpr {
    let rendered = ctx.render_declared(operand);
    let text = if matches!(rendered.ty, Ty::String) {
        rendered
    } else {
        ctx.mk(IrExprKind::StringInterp { parts: vec![IrStringPart::Expr { expr: rendered }] }, Ty::String, span)
    };
    let call = IrExprKind::Call {
        target: CallTarget::Module {
            module: sym("string"),
            func: sym("__err_text"),
            def_id: ctx.def_map.get(&sym("string.__err_text")).copied(),
        },
        args: vec![text],
        type_args: vec![],
    };
    ctx.mk(call, Ty::String, span)
}

/// `{ eprintln("<parts>"); process.exit(1) }` — the failure arm every T18
/// desugar shares, so every consumer (native, both wasm legs, the interp)
/// inherits the same bytes.
fn t18_abort_block(ctx: &mut LowerCtx, parts: Vec<IrStringPart>, span: Option<ast::Span>) -> IrExpr {
    let msg = ctx.mk(IrExprKind::StringInterp { parts }, Ty::String, span);
    let eprint = ctx.mk(
        IrExprKind::Call {
            target: CallTarget::Named { name: sym("eprintln") },
            args: vec![msg],
            type_args: vec![],
        },
        Ty::Unit,
        span,
    );
    let one = ctx.mk(IrExprKind::LitInt { value: 1 }, Ty::Int, span);
    let exit = ctx.mk(
        IrExprKind::Call {
            target: CallTarget::Module {
                module: sym("process"),
                func: sym("exit"),
                def_id: ctx.def_map.get(&sym("process.exit")).copied(),
            },
            args: vec![one],
            type_args: vec![],
        },
        Ty::Unit,
        span,
    );
    ctx.mk(
        IrExprKind::Block {
            stmts: vec![
                IrStmt { kind: IrStmtKind::Expr { expr: eprint }, span: None },
                IrStmt { kind: IrStmtKind::Expr { expr: exit }, span: None },
            ],
            expr: None,
        },
        Ty::Unit,
        span,
    )
}

/// How a `testing.assert_*` decides pass.
enum TestingVerdict {
    /// A Bool condition over the bound operands.
    Cond(IrExpr),
    /// The first operand matches this pattern.
    Arm(IrPattern),
}

/// The arity of a `testing` assert this desugar owns, or None. One name
/// per arm: this is each function's own lowering, chosen by its name.
fn testing_assert_arity(func: &str) -> Option<usize> {
    match func {
        "assert_some" => Some(1),
        "assert_none" => Some(1),
        "assert_ok" => Some(1),
        "assert_err" => Some(1),
        "assert_gt" => Some(2),
        "assert_lt" => Some(2),
        "assert_contains" => Some(2),
        "assert_approx" => Some(3),
        _ => None,
    }
}

/// `testing.assert_gt` / `assert_lt` / `assert_approx` / `assert_contains` /
/// `assert_some` / `assert_none` / `assert_ok` / `assert_err` OUTSIDE a test
/// block (#3747) desugar to the T18 abort
/// form, the same way `assert_eq` does (C-153), so every target prints
/// one block and exits 1:
/// ```text
/// Error: assertion failed
///   at: line <N>
///   expected: <the relation, e.g. `> 2`, `contains "x"`, `ok(_)`>
///   found: <the operand>
/// ```
/// Inside a test block the call stays a call: the harness reports it.
fn desugar_testing_assert_outside_test(
    ctx: &mut LowerCtx,
    target: &CallTarget,
    ir_args: &[IrExpr],
    span: Option<ast::Span>,
) -> Option<IrExpr> {
    if ctx.in_test {
        return None;
    }
    let CallTarget::Module { module, func, .. } = target else { return None };
    let f = func.as_str();
    if module.as_str() != "testing" || testing_assert_arity(f) != Some(ir_args.len()) {
        return None;
    }
    let (mut stmts, v) = bind_assert_operands(ctx, ir_args.to_vec(), span);
    let at = span.as_ref().map(|s| format!("\n  at: line {}", s.line)).unwrap_or_default();
    let head = |rel: &str| IrStringPart::Lit { value: format!("Error: assertion failed{at}\n  expected: {rel}") };
    let lit = |s: &str| IrStringPart::Lit { value: s.to_string() };
    let shown: Vec<IrExpr> = v.iter().map(|e| assert_shown(ctx, e.clone(), span)).collect();
    let val = |i: usize| IrStringPart::Expr { expr: shown[i].clone() };
    let found = || lit("\n  found: ");
    let (parts, verdict) = match f {
        "assert_gt" => {
            let cond = testing_binop(ctx, BinOp::Gt, &v[0], &v[1], span);
            (vec![head("> "), val(1), found(), val(0)], TestingVerdict::Cond(cond))
        }
        "assert_lt" => {
            let cond = testing_binop(ctx, BinOp::Lt, &v[0], &v[1], span);
            (vec![head("< "), val(1), found(), val(0)], TestingVerdict::Cond(cond))
        }
        // `abs(a - b) < tolerance`, strict, as `a - b < t and b - a < t`:
        // IEEE subtraction negates exactly, and a NaN fails both halves.
        "assert_approx" => {
            let ab = testing_binop(ctx, BinOp::SubFloat, &v[0], &v[1], span);
            let ba = testing_binop(ctx, BinOp::SubFloat, &v[1], &v[0], span);
            let lo = testing_binop(ctx, BinOp::Lt, &ab, &v[2], span);
            let hi = testing_binop(ctx, BinOp::Lt, &ba, &v[2], span);
            let cond = testing_binop(ctx, BinOp::And, &lo, &hi, span);
            let parts = vec![head("within "), val(2), lit(" of "), val(1), found(), val(0)];
            (parts, TestingVerdict::Cond(cond))
        }
        "assert_contains" => {
            let call = IrExprKind::Call {
                target: CallTarget::Module {
                    module: sym("string"),
                    func: sym("contains"),
                    def_id: ctx.def_map.get(&sym("string.contains")).copied(),
                },
                args: vec![v[0].clone(), v[1].clone()],
                type_args: vec![],
            };
            let cond = ctx.mk(call, Ty::Bool, span);
            (vec![head("contains "), val(1), found(), val(0)], TestingVerdict::Cond(cond))
        }
        "assert_some" => {
            let pat = IrPattern::Some { inner: Box::new(IrPattern::Wildcard) };
            (vec![head("some(_)"), found(), lit("none")], TestingVerdict::Arm(pat))
        }
        "assert_none" => (vec![head("none"), found(), val(0)], TestingVerdict::Arm(IrPattern::None)),
        "assert_ok" => {
            let pat = IrPattern::Ok { inner: Box::new(IrPattern::Wildcard) };
            (vec![head("ok(_)"), found(), val(0)], TestingVerdict::Arm(pat))
        }
        "assert_err" => {
            let pat = IrPattern::Err { inner: Box::new(IrPattern::Wildcard) };
            (vec![head("err(_)"), found(), val(0)], TestingVerdict::Arm(pat))
        }
        _ => return None,
    };
    let fail = t18_abort_block(ctx, parts, span);
    let ok = ctx.mk(IrExprKind::Block { stmts: vec![], expr: None }, Ty::Unit, span);
    let guard = match verdict {
        TestingVerdict::Cond(cond) => IrExprKind::If { cond: Box::new(cond), then: Box::new(ok), else_: Box::new(fail) },
        TestingVerdict::Arm(pattern) => IrExprKind::Match {
            subject: Box::new(v[0].clone()),
            arms: vec![
                IrMatchArm { pattern, guard: None, body: ok },
                IrMatchArm { pattern: IrPattern::Wildcard, guard: None, body: fail },
            ],
        },
    };
    let guard = ctx.mk(guard, Ty::Unit, span);
    stmts.push(IrStmt { kind: IrStmtKind::Expr { expr: guard }, span: None });
    // A Unit tail, as `desugar_assert_abort`'s: the call may be a `let` value.
    let unit = ctx.mk(IrExprKind::Unit, Ty::Unit, span);
    Some(ctx.mk(IrExprKind::Block { stmts, expr: Some(Box::new(unit)) }, Ty::Unit, span))
}

/// `l <op> r` typed by the operator (Bool for comparisons, Float for `SubFloat`).
fn testing_binop(ctx: &LowerCtx, op: BinOp, l: &IrExpr, r: &IrExpr, span: Option<ast::Span>) -> IrExpr {
    let ty = op.result_ty().unwrap_or(Ty::Bool);
    ctx.mk(IrExprKind::BinOp { op, left: Box::new(l.clone()), right: Box::new(r.clone()) }, ty, span)
}
