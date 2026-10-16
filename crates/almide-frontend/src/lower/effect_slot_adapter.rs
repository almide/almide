// The effect-slot ok(...) adapter (#1148). `include!`d by
// expressions_access.rs (the 800-line file budget); it shares that file's
// scope and imports.

/// A PURE fn value flowing into an `effect (A) -> B` slot must become a
/// carrier-returning closure — the slot's runtime shape is
/// `(A) -> Result[B, String]` (ALS-M15, #1148). Wraps the value as
/// `(a0..an) => ok(value(a0..an))`. No-op when the slot is not an effect
/// slot (read through its alias: `HttpHandler`) or the value already returns
/// the carrier (a named effect fn's value, which the checker bakes —
/// `fn_value_ty`). A value that is an `if` / `match` / block is adapted ARM by
/// arm: one arm may already be an effect handler while another is a pure
/// lambda, and the branch as a whole then types as the effect handler, so a
/// whole-value check would leave the pure arm unwrapped.
pub(super) fn adapt_fn_value_to_effect_slot(ctx: &mut LowerCtx, val: IrExpr, slot: &Ty) -> IrExpr {
    let slot = ctx.env.resolve_named(slot);
    if !matches!(slot, Ty::Fn { is_effect: true, .. }) {
        return val;
    }
    adapt_arms_to_effect_slot(ctx, val)
}

/// A pure fn type that does not already return the carrier.
fn needs_carrier(ty: &Ty) -> bool {
    matches!(ty, Ty::Fn { ret, is_effect: false, .. } if !ret.is_result())
}

fn adapt_arms_to_effect_slot(ctx: &mut LowerCtx, mut val: IrExpr) -> IrExpr {
    let reached = match &mut val.kind {
        IrExprKind::If { then, else_, .. } => {
            adapt_boxed(ctx, then);
            adapt_boxed(ctx, else_);
            true
        }
        IrExprKind::Match { arms, .. } => {
            for arm in arms.iter_mut() {
                arm.body = adapt_arms_to_effect_slot(ctx, arm.body.clone());
            }
            true
        }
        IrExprKind::Block { expr: Some(tail), .. } => {
            adapt_boxed(ctx, tail);
            true
        }
        _ => false,
    };
    if !reached {
        return wrap_in_carrier(ctx, val);
    }
    if let Ty::Fn { params, ret, is_effect: false } = &val.ty
        && !ret.is_result()
    {
        val.ty = Ty::Fn { params: params.clone(), ret: Box::new(Ty::result(ret.as_ref().clone(), Ty::String)), is_effect: false };
    }
    val
}

fn adapt_boxed(ctx: &mut LowerCtx, slot: &mut Box<IrExpr>) {
    **slot = adapt_arms_to_effect_slot(ctx, slot.as_ref().clone());
}

/// `(a0..an) => ok(val(a0..an))` for a pure fn value; anything else unchanged.
fn wrap_in_carrier(ctx: &mut LowerCtx, val: IrExpr) -> IrExpr {
    if !needs_carrier(&val.ty) {
        return val;
    }
    let Ty::Fn { params: vparams, ret: vret, .. } = &val.ty else { return val };
    let span = val.span;
    let vparams = vparams.clone();
    let inner_ret = vret.as_ref().clone();
    let params: Vec<(VarId, Ty)> = vparams.iter().enumerate().map(|(i, pt)| {
        let vid = ctx.var_table.alloc(sym(&format!("__eff_wrap{}", i)), pt.clone(), Mutability::Let, None);
        (vid, pt.clone())
    }).collect();
    let args: Vec<IrExpr> = params.iter()
        .map(|(vid, pt)| ctx.mk(IrExprKind::Var { id: *vid }, pt.clone(), span))
        .collect();
    let call = ctx.mk(IrExprKind::Call {
        target: CallTarget::Computed { callee: Box::new(val) },
        args, type_args: vec![],
    }, inner_ret.clone(), span);
    let carrier = Ty::result(inner_ret, Ty::String);
    let body = ctx.mk(IrExprKind::ResultOk { expr: Box::new(call) }, carrier.clone(), span);
    let lambda_ty = Ty::Fn { params: vparams, ret: Box::new(carrier), is_effect: false };
    let lambda_id = Some(ctx.next_lambda_id());
    ctx.mk(IrExprKind::Lambda { params, body: Box::new(body), lambda_id }, lambda_ty, span)
}
