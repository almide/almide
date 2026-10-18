// The scoping of a fan arm's `!` (#3462, #3464, #3467) and the
// `fan.settle { }` block's slots. `include!`d by expressions.rs (the
// 800-line file budget); it shares that module's scope and imports.

/// `fan.settle { arms }` — SEQUENTIAL settle (T2-4): each arm evaluates in
/// arm order into its own Result slot (a plain arm wraps in Ok, a Result arm
/// passes through — its Err is CAPTURED, never propagated), and the value is
/// the tuple of the slots. A tuple literal evaluates its elements in exactly
/// arm order (the guarantee the fan{} desugar rides), so the settle IS the
/// literal — and a destructuring bind then splits it into DIRECT per-arm
/// binds the downstream match tracking understands.
fn lower_fan_settle(ctx: &mut LowerCtx, arms: &[ast::Expr], ty: Ty, span: Option<ast::Span>) -> IrExpr {
    let slots: &[Ty] = match &ty { Ty::Tuple(ts) => ts, _ => &[] };
    let elems: Vec<IrExpr> = arms
        .iter()
        .enumerate()
        .map(|(i, arm)| {
            let a = lower_expr(ctx, arm);
            let a = settle_arm_scope(ctx, a, slots.get(i));
            if a.ty.result_err_ty().is_some() {
                return a;
            }
            let rt = Ty::result(a.ty.clone(), Ty::String);
            ctx.mk(IrExprKind::ResultOk { expr: Box::new(a) }, rt, span)
        })
        .collect();
    ctx.mk(IrExprKind::Tuple { elements: elems }, ty, span)
}

/// #3462: a `fan { … }` arm whose body propagates with `!` is a propagation
/// SCOPE of its own (C-199, ADR-0024 D1): the `!` ends the ARM with its Err,
/// every sibling still runs, and the join reports the lowest-index Err. The
/// arm is lowered as the zero-arg thunk `() => ok(body)` called in place —
/// the shape a `fan.map` callback's `!` already rides on every consumer — so
/// no backend reads that `!` as an exit from the enclosing fn. An arm whose
/// only marker is its own top-level `!` already is a Result arm and is left
/// alone.
///
/// #3467: the thunk fails with what its `!`s fail with — a callback's channel
/// rule (ADR-0021 D1): one shared `E` when every `!` fails with `E`, else
/// `String`. In a `-> Result[_, E]` fn the checker holds every arm `!` to `E`.
fn fan_arm_scope(ctx: &mut LowerCtx, arm: IrExpr) -> IrExpr {
    let inner = match &arm.kind { IrExprKind::Unwrap { expr } => &**expr, _ => &arm };
    if bang_channel_in_scope(inner).is_none() {
        return arm;
    }
    let err_ty = bang_channel_in_scope(&arm).unwrap_or(Ty::String);
    arm_thunk(ctx, arm, &err_ty)
}

/// #3464: a `fan.settle { }` arm with a `!` of its own is scoped the same
/// way — its Err is that arm's slot — and the thunk takes the slot's error
/// type, which the checker decided as a callback's channel.
fn settle_arm_scope(ctx: &mut LowerCtx, arm: IrExpr, slot: Option<&Ty>) -> IrExpr {
    let Some(bang_err) = bang_channel_in_scope(&arm) else { return arm };
    let err_ty = slot.and_then(Ty::result_err_ty).unwrap_or(bang_err);
    arm_thunk(ctx, arm, &err_ty)
}

/// The arm as the zero-arg thunk `() => ok(body)` called in place, failing
/// with `err_ty`.
fn arm_thunk(ctx: &mut LowerCtx, arm: IrExpr, err_ty: &Ty) -> IrExpr {
    let span = arm.span;
    let mut body = arm;
    convert_option_unwraps_to_result(&mut body);
    if !body.ty.is_result() {
        body = crate::lower::wrap_fallible_value_tail(body, err_ty);
    }
    let ret = body.ty.clone();
    let lambda_ty = Ty::Fn { params: Vec::new(), ret: Box::new(ret.clone()), is_effect: false };
    let lambda_id = Some(ctx.next_lambda_id());
    let thunk = ctx.mk(IrExprKind::Lambda { params: Vec::new(), body: Box::new(body), lambda_id }, lambda_ty, span);
    let target = CallTarget::Computed { callee: Box::new(thunk) };
    ctx.mk(IrExprKind::Call { target, args: Vec::new(), type_args: Vec::new() }, ret, span)
}

/// The error the `!`s in `e` fail with, when one propagates out of `e`
/// itself — not inside a lambda (its own channel) or a nested `fan` (its arms
/// are scoped in turn): their shared `E`, or `String` when they differ (an
/// Option's `none` and a plain effect call fail with `String`).
fn bang_channel_in_scope(e: &IrExpr) -> Option<Ty> {
    use almide_ir::visit::{walk_expr, IrVisitor};
    struct Scan(Option<Ty>);
    impl IrVisitor for Scan {
        fn visit_expr(&mut self, e: &IrExpr) {
            match &e.kind {
                IrExprKind::Lambda { .. } => return,
                // A nested fan's arms are scoped in turn, but its JOIN
                // propagates the first failing arm's Err out of it — a `!` of
                // this scope (#1388: a fan nested in a fan arm was a `?` inside
                // the outer arm's non-Result thread closure, natively).
                IrExprKind::Fan { exprs } => {
                    for arm in exprs {
                        let err = match &arm.kind {
                            IrExprKind::Unwrap { expr } => Some(expr.ty.result_err_ty().unwrap_or(Ty::String)),
                            _ => arm.ty.result_err_ty(),
                        };
                        if let Some(err) = err {
                            self.join(err);
                        }
                    }
                    return;
                }
                IrExprKind::Unwrap { expr } => self.join(expr.ty.result_err_ty().unwrap_or(Ty::String)),
                _ => {}
            }
            walk_expr(self, e);
        }
    }
    impl Scan {
        fn join(&mut self, err: Ty) {
            self.0 = Some(match self.0.take() {
                Some(seen) if seen != err => Ty::String,
                _ => err,
            });
        }
    }
    let mut s = Scan(None);
    s.visit_expr(e);
    s.0
}

/// The record form `fan { x: a, y: b }` (#1388): the tuple-form fan of the
/// same arms, destructured into one fresh binding per field, then the record
/// literal over them — `{ let (x', y') = fan { a, b }; { x: x', y: y' } }`.
/// Every leg (native threads + ADR-0013 share marks, wasm, interp) sees the
/// one `Fan` node it already runs, so the arms' concurrency, `!` scoping and
/// first-Err order are the tuple form's by construction. An arm's slot type
/// is its value type: a scoped `!` arm is a Result the join unwraps (the
/// checker refused a field whose own value is a Result).
fn lower_fan_record(ctx: &mut LowerCtx, arms: Vec<IrExpr>, names: &[almide_base::intern::Sym], ty: Ty, span: Option<ast::Span>) -> IrExpr {
    let slot_tys: Vec<Ty> = arms.iter().map(|a| match &a.ty {
        Ty::Applied(TypeConstructorId::Result, args) if args.len() == 2 => args[0].clone(),
        t => t.clone(),
    }).collect();
    let fan_ty = match slot_tys.as_slice() {
        [one] => one.clone(),
        many => Ty::Tuple(many.to_vec()),
    };
    let vars: Vec<VarId> = names.iter().zip(&slot_tys)
        .map(|(n, t)| ctx.var_table.alloc(sym(&format!("__fan_{}", n.as_str())), t.clone(), Mutability::Let, span))
        .collect();
    let fan = ctx.mk(IrExprKind::Fan { exprs: arms }, fan_ty.clone(), span);
    let bind = match vars.as_slice() {
        [one] => IrStmtKind::Bind { var: *one, mutability: Mutability::Let, ty: fan_ty, value: fan },
        many => IrStmtKind::BindDestructure {
            pattern: IrPattern::Tuple {
                elements: many.iter().zip(&slot_tys).map(|(v, t)| IrPattern::Bind { var: *v, ty: t.clone() }).collect(),
            },
            value: fan,
        },
    };
    let fields: Vec<(almide_base::intern::Sym, IrExpr)> = names.iter().zip(vars.iter().zip(&slot_tys))
        .map(|(n, (v, t))| (*n, ctx.mk(IrExprKind::Var { id: *v }, t.clone(), span)))
        .collect();
    let record = ctx.mk(IrExprKind::Record { name: None, fields }, ty.clone(), span);
    ctx.mk(IrExprKind::Block { stmts: vec![IrStmt { kind: bind, span }], expr: Some(Box::new(record)) }, ty, span)
}
