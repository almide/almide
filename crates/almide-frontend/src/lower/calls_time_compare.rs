// `compute.compare` / `duration.compare` (R-duration-03): the one clock
// function that is not a unit constructor. `include!`d by calls.rs; kept out
// of calls_time.rs, whose text fingerprints the inspected unit constructors.

/// The `compare` call on a clock module, or None for every other call.
fn lower_time_compare_call(
    ctx: &mut LowerCtx,
    callee: &ast::Expr,
    args: &[ast::Expr],
    span: Option<ast::Span>,
) -> Option<IrExpr> {
    let ast::ExprKind::Member { object, field } = &callee.kind else { return None };
    let ast::ExprKind::Ident { name: module, .. } = &object.kind else { return None };
    if almide_lang::time_units::clock_type_of_module(module.as_str()).is_none() || field.as_str() != "compare" {
        return None;
    }
    lower_time_compare(ctx, args, span)
}

/// `compute.compare(a, b)` / `duration.compare(a, b)` erase to a three-way
/// compare of the two nanosecond Ints: `if a < b then -1 else if a > b then 1
/// else 0`, each operand evaluated once, left to right.
fn lower_time_compare(ctx: &mut LowerCtx, args: &[ast::Expr], span: Option<ast::Span>) -> Option<IrExpr> {
    let [a, b] = args else { return None };
    let la = lower_expr(ctx, a);
    let lb = lower_expr(ctx, b);
    let va = ctx.define_var("__time_cmp_a", Ty::Int, Mutability::Let, None);
    let vb = ctx.define_var("__time_cmp_b", Ty::Int, Mutability::Let, None);
    let bind = |var, value| IrStmt {
        kind: IrStmtKind::Bind { var, mutability: Mutability::Let, ty: Ty::Int, value },
        span: None,
    };
    let stmts = vec![bind(va, la), bind(vb, lb)];
    let var = |ctx: &mut LowerCtx, id| ctx.mk(IrExprKind::Var { id }, Ty::Int, span);
    let lit = |ctx: &mut LowerCtx, v: i64| ctx.mk(IrExprKind::LitInt { value: v }, Ty::Int, span);
    let cmp = |ctx: &mut LowerCtx, op| {
        let l = var(ctx, va);
        let r = var(ctx, vb);
        ctx.mk(IrExprKind::BinOp { op, left: Box::new(l), right: Box::new(r) }, Ty::Bool, span)
    };
    let gt = cmp(ctx, BinOp::Gt);
    let one = lit(ctx, 1);
    let zero = lit(ctx, 0);
    let inner = ctx.mk(
        IrExprKind::If { cond: Box::new(gt), then: Box::new(one), else_: Box::new(zero) },
        Ty::Int,
        span,
    );
    let lt = cmp(ctx, BinOp::Lt);
    let minus_one = lit(ctx, -1);
    let value = ctx.mk(
        IrExprKind::If { cond: Box::new(lt), then: Box::new(minus_one), else_: Box::new(inner) },
        Ty::Int,
        span,
    );
    Some(ctx.mk(IrExprKind::Block { stmts, expr: Some(Box::new(value)) }, Ty::Int, span))
}
