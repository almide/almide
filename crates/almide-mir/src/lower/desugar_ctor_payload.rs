// #1564: ANF-hoist a heap CALL payload out of a Result/Option constructor in
// RETURNED position — `ok(list.get(xs, 0))` becomes
// `{ let $t = list.get(xs, 0); ok($t) }` — so the proven bind-position call
// machinery materializes the payload and the ctor wraps a bound Var (the
// spelling that already lowers). Before this pass the inline spelling fell to
// the deferred-Opaque tail wall for ANY payload kind — record and variant
// alike — while the hand-written bind-then-wrap spelling ran: the same value
// walled or lowered by WHO produced it (producer-keyed admission, the
// reference survey's diagnosed disease). One structural rewrite deletes the
// spelling axis instead of admitting producers point-wise.
//
// Runs as a BRANCH_PASSES row (shared by the lowering ladder AND the count
// side). Moves the call subtree intact — the counted call set is unchanged.
//
// SUBSET: a ResultOk/ResultErr/OptionSome whose payload is a HEAP-typed CALL
// (a scalar payload already lowers — no churn), at a RETURNED position only:
// the fn-body tail, an arm tail of a returned `if`/`match`, or a nested block
// tail — exactly where the tail walls fired. An Unwrap payload (`ok(f(x)!)`)
// is EXCLUDED — `desugar_unwrap_rewrap_identity` owns that shape. The walk
// never enters a Lambda body (its params are not in scope at the hoist
// point), a loop, or a statement position (a discarded ctor never walled).

/// One hoist step: rewrite the FIRST qualifying ctor found in returned
/// position (pre-order: block tail, then `if`/`match` arm tails). The
/// enclosing fixpoint re-runs until no position qualifies.
pub(crate) fn desugar_returned_ctor_call_payload(
    body: &IrExpr,
    next_var: &mut u32,
) -> Option<IrExpr> {
    let mut out = body.clone();
    ctor_payload_rewrite_returned(&mut out, next_var).then_some(out)
}

/// Walk ONLY the returned-expression tree; hoist in place, true on first hit.
fn ctor_payload_rewrite_returned(e: &mut IrExpr, next_var: &mut u32) -> bool {
    use almide_ir::{IrExprKind, IrStmt};
    match &mut e.kind {
        IrExprKind::Block { stmts, expr: Some(t) } => {
            if let Some((bind, span)) = take_ctor_call_payload(t, next_var) {
                stmts.push(IrStmt { kind: bind, span });
                return true;
            }
            ctor_payload_rewrite_returned(t, next_var)
        }
        IrExprKind::If { then, else_, .. } => {
            ctor_payload_rewrite_returned(then, next_var)
                || ctor_payload_rewrite_returned(else_, next_var)
        }
        IrExprKind::Match { arms, .. } => {
            for a in arms.iter_mut() {
                if ctor_payload_rewrite_returned(&mut a.body, next_var) {
                    return true;
                }
            }
            false
        }
        // A bare returned position that IS the ctor (an `if` arm, a match arm,
        // or a blockless fn body): wrap it into a Block so the hoisted bind
        // has a statement list to land in.
        _ => {
            let Some((bind, span)) = take_ctor_call_payload(e, next_var) else {
                return false;
            };
            let ty = e.ty.clone();
            let espan = e.span.clone();
            let ctor = std::mem::replace(
                e,
                IrExpr {
                    kind: IrExprKind::OptionNone,
                    ty: ty.clone(),
                    span: espan.clone(),
                    def_id: None,
                },
            );
            e.kind = IrExprKind::Block {
                stmts: vec![IrStmt { kind: bind, span }],
                expr: Some(Box::new(ctor)),
            };
            true
        }
    }
}

/// If `e` is a Result/Option ctor whose payload is a heap-typed Call, hoist
/// the payload into a fresh Bind (payload replaced by the bound Var) and
/// return the Bind stmt kind. The ctor node itself is left in place.
fn take_ctor_call_payload(
    e: &mut IrExpr,
    next_var: &mut u32,
) -> Option<(almide_ir::IrStmtKind, Option<almide_ir::Span>)> {
    use almide_ir::{IrExpr, IrExprKind, IrStmtKind, Mutability, VarId};
    let inner = match &mut e.kind {
        IrExprKind::ResultOk { expr }
        | IrExprKind::ResultErr { expr }
        | IrExprKind::OptionSome { expr } => expr,
        _ => return None,
    };
    if !matches!(inner.kind, IrExprKind::Call { .. }) || !crate::lower::is_heap_ty(&inner.ty) {
        return None;
    }
    let ty = inner.ty.clone();
    let span = inner.span.clone();
    let var = VarId(*next_var);
    *next_var += 1;
    let call = std::mem::replace(
        inner.as_mut(),
        IrExpr { kind: IrExprKind::Var { id: var }, ty: ty.clone(), span: span.clone(), def_id: None },
    );
    Some((IrStmtKind::Bind { var, mutability: Mutability::Let, ty, value: call }, span))
}
