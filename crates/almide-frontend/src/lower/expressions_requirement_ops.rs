// Continuation of expressions.rs — operators on a declared type lowered to
// the protocol requirement that defines them (R-bigint-04). Split out of
// expressions_access.rs to keep it under the 800-line codopsy max-lines
// threshold; same file scope via `include!` (inherits expressions.rs's `use`
// imports).

/// An operator on a declared type that conforms to a protocol requiring the
/// operator's method (R-bigint-04): `a + b` on `BigInt` is `bigint.add(a, b)`,
/// `a < b` is `bigint.compare(a, b) < 0`. The pairing of operator and method is
/// `protocols::OPERATOR_REQUIREMENTS` / `ORDER_REQUIREMENT`; which function is
/// the type's own is `requirement_call_target`. `None` for every primitive and
/// for a type that does not conform, which lower to the `BinOp` as before.
fn lower_requirement_operator(
    ctx: &mut LowerCtx, op: &str, left_ty: &Ty, (l, r): (IrExpr, IrExpr), ty: Ty, span: Option<crate::ast::Span>,
) -> Option<IrExpr> {
    use crate::canonicalize::protocols::{OPERATOR_REQUIREMENTS, ORDER_REQUIREMENT};
    if let Some((_, method)) = OPERATOR_REQUIREMENTS.iter().find(|(o, _)| *o == op) {
        let target = requirement_call_target(ctx, left_ty, method)?;
        return Some(ctx.mk(IrExprKind::Call { target, args: vec![l, r], type_args: vec![] }, ty, span));
    }
    if matches!(op, "<" | "<=" | ">" | ">=") {
        let target = requirement_call_target(ctx, left_ty, ORDER_REQUIREMENT)?;
        let cmp = ctx.mk(IrExprKind::Call { target, args: vec![l, r], type_args: vec![] }, Ty::Int, span);
        let zero = ctx.mk(IrExprKind::LitInt { value: 0 }, Ty::Int, span);
        let bin_op = binop_for(op, &Ty::Int, &Ty::Int);
        return Some(ctx.mk(IrExprKind::BinOp { op: bin_op, left: Box::new(cmp), right: Box::new(zero) }, ty, span));
    }
    None
}

/// The function that is declared type `ty`'s requirement `method`, when one
/// of its protocols requires it: its convention method (`fn V.add`), else the
/// declaring module's function of that name (`bigint.add`, which may be
/// module-local: the desugaring is the module's own operator).
pub(super) fn requirement_call_target(ctx: &mut LowerCtx, ty: &Ty, method: &str) -> Option<CallTarget> {
    let Ty::Named(name, _) = ty else { return None };
    if !crate::canonicalize::protocols::type_requires(ctx.env, name.as_str(), method) {
        return None;
    }
    if let Some(key) = ctx.find_convention_fn(ty, method) {
        return Some(CallTarget::Named { name: key });
    }
    let module = crate::canonicalize::protocols::declaring_module(ctx.env, name.as_str())?;
    let key = almide_base::intern::sym(&format!("{}.{}", module, method));
    ctx.env.functions.contains_key(&key).then(|| CallTarget::Module {
        module: almide_base::intern::sym(&module),
        func: almide_base::intern::sym(method),
        def_id: None,
    })
}
