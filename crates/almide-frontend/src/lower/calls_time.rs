// Time-value lowering: the `Duration`/`Instant` binops and constructor
// with their abort guards. `include!`d by calls.rs (the 800-line file
// budget); it shares that module's scope and imports.

/// The §13 abort half of the time guards:
/// `{ eprintln("<prefix>${val}<suffix>"); process.exit(1) }` — the exact
/// assert-desugar abort tail, shared by the constructor's negative-argument
/// trap and the `T * Int` negative-scale trap.
fn time_abort_expr(
    ctx: &mut LowerCtx,
    prefix: String,
    val: IrExpr,
    suffix: &str,
    span: Option<ast::Span>,
) -> IrExpr {
    let mut parts = vec![
        IrStringPart::Lit { value: prefix },
        IrStringPart::Expr { expr: val },
    ];
    if !suffix.is_empty() {
        parts.push(IrStringPart::Lit { value: suffix.into() });
    }
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

/// ADR-0001 S3 erasure for the time-type operator algebra: `T + T` and
/// `T * Int` saturate at i64::MAX, `T - T` saturates at 0 (the ≥0 invariant),
/// and a NEGATIVE scale factor is the same deterministic §13 abort as a
/// negative constructor argument. Comparisons need no interception — they
/// erase to plain Int compares downstream. Returns `None` for any other op
/// (the checker has already rejected them; generic lowering recovers).
pub(super) fn lower_time_binop(
    ctx: &mut LowerCtx,
    op: &str,
    l: IrExpr,
    r: IrExpr,
    l_time: bool,
    span: Option<ast::Span>,
) -> Option<IrExpr> {
    if !matches!(op, "+" | "-" | "*") {
        return None;
    }
    let bind = |ctx: &mut LowerCtx, name: &str, e: IrExpr| {
        let v = ctx.define_var(name, Ty::Int, Mutability::Let, None);
        let stmt = IrStmt {
            kind: IrStmtKind::Bind { var: v, mutability: Mutability::Let, ty: Ty::Int, value: e },
            span: None,
        };
        (stmt, v)
    };
    let var = |ctx: &mut LowerCtx, v| ctx.mk(IrExprKind::Var { id: v }, Ty::Int, span);
    let lit = |ctx: &mut LowerCtx, n: i64| ctx.mk(IrExprKind::LitInt { value: n }, Ty::Int, span);
    let binop = |ctx: &mut LowerCtx, o, a: IrExpr, b: IrExpr, ty: Ty| {
        ctx.mk(IrExprKind::BinOp { op: o, left: Box::new(a), right: Box::new(b) }, ty, span)
    };
    match op {
        // T + T: both operands are ≥0 by the constructor invariant, so a
        // wrapped sum is always negative — saturate on sign.
        "+" => {
            let sum = binop(ctx, BinOp::AddInt, l, r, Ty::Int);
            let (bind_s, s) = bind(ctx, "__time_sum", sum);
            let sv = var(ctx, s);
            let zero = lit(ctx, 0);
            let neg = binop(ctx, BinOp::Lt, sv, zero, Ty::Bool);
            let max = lit(ctx, i64::MAX);
            let sv2 = var(ctx, s);
            let sat = ctx.mk(
                IrExprKind::If { cond: Box::new(neg), then: Box::new(max), else_: Box::new(sv2) },
                Ty::Int,
                span,
            );
            Some(ctx.mk(
                IrExprKind::Block { stmts: vec![bind_s], expr: Some(Box::new(sat)) },
                Ty::Int,
                span,
            ))
        }
        // T - T: floor at 0 (the remaining-budget invariant is never negative).
        "-" => {
            let diff = binop(ctx, BinOp::SubInt, l, r, Ty::Int);
            let (bind_d, d) = bind(ctx, "__time_diff", diff);
            let dv = var(ctx, d);
            let zero = lit(ctx, 0);
            let neg = binop(ctx, BinOp::Lt, dv, zero, Ty::Bool);
            let zero2 = lit(ctx, 0);
            let dv2 = var(ctx, d);
            let sat = ctx.mk(
                IrExprKind::If { cond: Box::new(neg), then: Box::new(zero2), else_: Box::new(dv2) },
                Ty::Int,
                span,
            );
            Some(ctx.mk(
                IrExprKind::Block { stmts: vec![bind_d], expr: Some(Box::new(sat)) },
                Ty::Int,
                span,
            ))
        }
        // T * Int (either order): trap on a negative factor, then
        // `n == 0 → 0`, `t > MAX/n → MAX`, else `t * n` (n ≥ 1 in the divide).
        "*" => {
            let (t_expr, n_expr) = if l_time { (l, r) } else { (r, l) };
            let (bind_t, t) = bind(ctx, "__time_val", t_expr);
            let (bind_n, n) = bind(ctx, "__time_scale", n_expr);
            let nv = var(ctx, n);
            let zero = lit(ctx, 0);
            let neg = binop(ctx, BinOp::Lt, nv, zero, Ty::Bool);
            let nv1 = var(ctx, n);
            let abort =
                time_abort_expr(ctx, "Error: negative time scale: ".into(), nv1, "", span);
            let ok = ctx.mk(IrExprKind::Block { stmts: vec![], expr: None }, Ty::Unit, span);
            let guard = ctx.mk(
                IrExprKind::If { cond: Box::new(neg), then: Box::new(abort), else_: Box::new(ok) },
                Ty::Unit,
                span,
            );
            let nv2 = var(ctx, n);
            let zero2 = lit(ctx, 0);
            let is_zero = binop(ctx, BinOp::Eq, nv2, zero2, Ty::Bool);
            let max = lit(ctx, i64::MAX);
            let nv3 = var(ctx, n);
            let limit = binop(ctx, BinOp::DivInt, max, nv3, Ty::Int);
            let tv = var(ctx, t);
            let over = binop(ctx, BinOp::Gt, tv, limit, Ty::Bool);
            let max2 = lit(ctx, i64::MAX);
            let tv2 = var(ctx, t);
            let nv4 = var(ctx, n);
            let scaled = binop(ctx, BinOp::MulInt, tv2, nv4, Ty::Int);
            let sat = ctx.mk(
                IrExprKind::If { cond: Box::new(over), then: Box::new(max2), else_: Box::new(scaled) },
                Ty::Int,
                span,
            );
            let zero3 = lit(ctx, 0);
            let value = ctx.mk(
                IrExprKind::If { cond: Box::new(is_zero), then: Box::new(zero3), else_: Box::new(sat) },
                Ty::Int,
                span,
            );
            Some(ctx.mk(
                IrExprKind::Block {
                    stmts: vec![
                        bind_t,
                        bind_n,
                        IrStmt { kind: IrStmtKind::Expr { expr: guard }, span: None },
                    ],
                    expr: Some(Box::new(value)),
                },
                Ty::Int,
                span,
            ))
        }
        _ => unreachable!(),
    }
}

/// The `compute.*` / `duration.*` unit-constructor erasure (ADR-0001 S2/S3).
/// Returns None for every other call. The unit set is closed; the checker has
/// already diagnosed unknown units, so an unknown unit here just declines.
fn lower_time_ctor(
    ctx: &mut LowerCtx,
    callee: &ast::Expr,
    args: &[ast::Expr],
    span: Option<ast::Span>,
) -> Option<IrExpr> {
    let ast::ExprKind::Member { object, field } = &callee.kind else { return None };
    let ast::ExprKind::Ident { name: module, .. } = &object.kind else { return None };
    if almide_lang::time_units::clock_type_of_module(module.as_str()).is_none() {
        return None;
    }
    let factor: i64 = almide_lang::time_units::unit_factor(field.as_str())?;
    let [arg] = args else { return None };
    let n = lower_expr(ctx, arg);
    // ADR-0001 S3: the erased value is ALWAYS in [0, i64::MAX] — a negative
    // argument is a deterministic abort (§13 convention: stderr + exit 1,
    // identical on both targets), an overflowing construction saturates to
    // i64::MAX (never wraps). `sat_limit` is the largest argument that scales
    // without overflow.
    let sat_limit = i64::MAX / factor;
    // Non-negative literal: fold the whole guard at compile time.
    if let IrExprKind::LitInt { value } = n.kind {
        if value >= 0 {
            let scaled = if value > sat_limit { i64::MAX } else { value * factor };
            return Some(ctx.mk(IrExprKind::LitInt { value: scaled }, Ty::Int, span));
        }
    }
    let t = ctx.define_var("__time_arg", Ty::Int, Mutability::Let, None);
    let bind = IrStmt {
        kind: IrStmtKind::Bind { var: t, mutability: Mutability::Let, ty: Ty::Int, value: n },
        span: None,
    };
    let tv = |ctx: &mut LowerCtx| ctx.mk(IrExprKind::Var { id: t }, Ty::Int, span);
    let lit = |ctx: &mut LowerCtx, v: i64| ctx.mk(IrExprKind::LitInt { value: v }, Ty::Int, span);
    // if t < 0 then { eprintln("Error: negative time: module.unit(t)"); process.exit(1) }
    let zero = lit(ctx, 0);
    let t0 = tv(ctx);
    let neg = ctx.mk(
        IrExprKind::BinOp { op: BinOp::Lt, left: Box::new(t0), right: Box::new(zero) },
        Ty::Bool,
        span,
    );
    let t1 = tv(ctx);
    let abort = time_abort_expr(
        ctx,
        format!("Error: negative time: {}.{}(", module.as_str(), field.as_str()),
        t1,
        ")",
        span,
    );
    let ok = ctx.mk(IrExprKind::Block { stmts: vec![], expr: None }, Ty::Unit, span);
    let trap_guard = ctx.mk(
        IrExprKind::If { cond: Box::new(neg), then: Box::new(abort), else_: Box::new(ok) },
        Ty::Unit,
        span,
    );
    // if t > sat_limit then i64::MAX else t * factor
    let t2 = tv(ctx);
    let limit = lit(ctx, sat_limit);
    let over = ctx.mk(
        IrExprKind::BinOp { op: BinOp::Gt, left: Box::new(t2), right: Box::new(limit) },
        Ty::Bool,
        span,
    );
    let max = lit(ctx, i64::MAX);
    let t3 = tv(ctx);
    let f = lit(ctx, factor);
    let scaled = ctx.mk(
        IrExprKind::BinOp { op: BinOp::MulInt, left: Box::new(t3), right: Box::new(f) },
        Ty::Int,
        span,
    );
    let value = ctx.mk(
        IrExprKind::If { cond: Box::new(over), then: Box::new(max), else_: Box::new(scaled) },
        Ty::Int,
        span,
    );
    Some(ctx.mk(
        IrExprKind::Block {
            stmts: vec![bind, IrStmt { kind: IrStmtKind::Expr { expr: trap_guard }, span: None }],
            expr: Some(Box::new(value)),
        },
        Ty::Int,
        span,
    ))
}
