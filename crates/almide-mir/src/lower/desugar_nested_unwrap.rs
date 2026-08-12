// #1183: ANF-hoist an expression-NESTED scalar `!` out of a Bind/Assign value
// to its own bind-position statement — `out = out + [conv(s)!]` becomes
// `let $t = conv(s)!; out = out + [$t]` — so the PROVEN bind-position
// machinery handles it (the effect-unwrap continuation match in straight-line
// code, the loop flag rewrite inside a `for`/`while` body). Before this pass
// the nested unwrap SURVIVED every desugar and fell to the scalar-operand
// Ok-payload read, which had no tag dispatch: the wasm leg silently read the
// err block's payload slot as the element value while native (`?`) propagated
// — the accept-and-wrong divergence #1183 pins.
//
// Runs as a BRANCH_PASSES row (shared by the lowering ladder AND the classify
// count side via `desugar_heap_branches` — desugar-before-both by
// construction, the #1176 lesson). The rewrite adds one Bind and moves the
// Unwrap subtree intact, so the counted call set is unchanged.
//
// SUBSET: the innermost nested `Unwrap` whose operand is a Result/Option and
// whose payload type is SCALAR (the class the silent read admitted). Root
// position is EXCLUDED for Bind (already the proven let-unwrap shape) and
// INCLUDED for Assign (`out = f()!` has no bind-position route of its own).
// In the TEST world `!` is a plain unwrap (L9) — hoisting to a bind preserves
// that semantics unchanged, so no ABI gate is needed.

/// One hoist step: rewrite the FIRST qualifying Bind/Assign statement found
/// (pre-order over blocks, `if`/`match` arms, and loop bodies). The
/// enclosing fixpoint re-runs until no statement qualifies.
pub(crate) fn desugar_stmt_value_nested_unwrap(
    body: &IrExpr,
    next_var: &mut u32,
) -> Option<IrExpr> {
    let mut out = body.clone();
    if nested_unwrap_rewrite_expr(&mut out, next_var) {
        Some(out)
    } else {
        None
    }
}

/// Depth-first search for a statement list containing a qualifying stmt;
/// rewrites in place and returns true on the first hit.
fn nested_unwrap_rewrite_expr(e: &mut IrExpr, next_var: &mut u32) -> bool {
    use almide_ir::IrExprKind;
    match &mut e.kind {
        IrExprKind::Block { stmts, expr } => {
            if nested_unwrap_rewrite_stmts(stmts, next_var) {
                return true;
            }
            if let Some(t) = expr {
                return nested_unwrap_rewrite_expr(t, next_var);
            }
            false
        }
        IrExprKind::ForIn { body, .. } | IrExprKind::While { body, .. } => {
            nested_unwrap_rewrite_stmts(body, next_var)
        }
        IrExprKind::If { cond, then, else_ } => {
            nested_unwrap_rewrite_expr(cond, next_var)
                || nested_unwrap_rewrite_expr(then, next_var)
                || nested_unwrap_rewrite_expr(else_, next_var)
        }
        IrExprKind::Match { subject, arms } => {
            if nested_unwrap_rewrite_expr(subject, next_var) {
                return true;
            }
            for a in arms.iter_mut() {
                if nested_unwrap_rewrite_expr(&mut a.body, next_var) {
                    return true;
                }
            }
            false
        }
        _ => false,
    }
}

/// Rewrite the first qualifying Bind/Assign in this statement list (or recurse
/// into nested statement expressions).
fn nested_unwrap_rewrite_stmts(
    stmts: &mut Vec<almide_ir::IrStmt>,
    next_var: &mut u32,
) -> bool {
    use almide_ir::{IrExpr, IrExprKind, IrStmt, IrStmtKind, Mutability, VarId};
    for i in 0..stmts.len() {
        let (root_ok, value) = match &mut stmts[i].kind {
            // A Bind whose WHOLE value is the unwrap is the proven let-unwrap
            // shape — only a NESTED unwrap qualifies.
            IrStmtKind::Bind { value, .. } => (false, Some(value)),
            // An Assign has no bind-position route of its own — root included.
            IrStmtKind::Assign { value, .. } => (true, Some(value)),
            IrStmtKind::Expr { expr } => {
                if nested_unwrap_rewrite_expr(expr, next_var) {
                    return true;
                }
                (false, None)
            }
            _ => (false, None),
        };
        let Some(value) = value else { continue };
        let at_root = matches!(&value.kind, IrExprKind::Unwrap { .. });
        if at_root && !root_ok {
            // Still recurse INSIDE the root unwrap's operand for a deeper one.
            let IrExprKind::Unwrap { expr: inner } = &mut value.kind else { unreachable!() };
            if let Some(u) = take_innermost_scalar_unwrap(inner, next_var) {
                let (bind, span) = u;
                stmts.insert(i, IrStmt { kind: bind, span });
                return true;
            }
            continue;
        }
        if let Some((bind, span)) = take_innermost_scalar_unwrap(value, next_var) {
            stmts.insert(i, IrStmt { kind: bind, span });
            return true;
        }
        // Root-position Assign unwrap with no deeper nested one: hoist the
        // root itself so the assign's value becomes a plain var.
        if at_root && root_ok {
            let ty = value.ty.clone();
            let span = value.span.clone();
            let var = VarId(*next_var);
            *next_var += 1;
            let unwrap_expr = std::mem::replace(
                value,
                IrExpr { kind: IrExprKind::Var { id: var }, ty: ty.clone(), span: span.clone(), def_id: None },
            );
            stmts.insert(
                i,
                IrStmt {
                    kind: IrStmtKind::Bind { var, mutability: Mutability::Let, ty, value: unwrap_expr },
                    span,
                },
            );
            return true;
        }
    }
    false
}

/// Find a hoistable nested scalar Result/Option `Unwrap` strictly BELOW the
/// given expression's root; replace it with a fresh Var and return the Bind
/// stmt kind that hoists it.
///
/// DESCENT IS DELIBERATELY NARROW — only positions where lifting the unwrap
/// BEFORE the whole statement preserves scope and evaluation order:
/// - a non-short-circuit `BinOp`'s left operand, and its right operand only
///   when the left is CALL-FREE (else the hoist would reorder side effects);
/// - `List`/`Tuple` literal elements, each only while every EARLIER element
///   is call-free (same reordering rule).
/// NEVER into: a Lambda body (different scope — hoisting captures the lambda's
/// params in the outer bind), `if`/`match` arms or `??` fallbacks (conditional
/// evaluation — the hoisted unwrap would run and propagate unconditionally),
/// `and`/`or` right operands (short-circuit), call ARGUMENTS (the call-arg
/// unwrap ANF already owns that position), or nested blocks/loops. The first
/// draft used the generic visitor and walled five fallible_lambda_test fns by
/// hoisting a LAMBDA-body `!` into the enclosing bind — the ratchet caught it.
fn take_innermost_scalar_unwrap(
    value: &mut almide_ir::IrExpr,
    next_var: &mut u32,
) -> Option<(almide_ir::IrStmtKind, Option<almide_ir::Span>)> {
    use almide_ir::{IrExpr, IrExprKind, IrStmtKind, Mutability, VarId};

    fn is_hoistable_unwrap(e: &IrExpr) -> bool {
        matches!(
            &e.kind,
            IrExprKind::Unwrap { expr: inner }
                if !crate::lower::is_heap_ty(&e.ty)
                    && matches!(
                        &inner.ty,
                        almide_lang::types::Ty::Applied(
                            almide_lang::types::constructor::TypeConstructorId::Result
                                | almide_lang::types::constructor::TypeConstructorId::Option,
                            _
                        )
                    )
        )
    }

    fn hoist_here(e: &mut IrExpr, next_var: &mut u32) -> (IrStmtKind, Option<almide_ir::Span>) {
        let ty = e.ty.clone();
        let span = e.span.clone();
        let var = VarId(*next_var);
        *next_var += 1;
        let unwrap_expr = std::mem::replace(
            e,
            IrExpr { kind: IrExprKind::Var { id: var }, ty: ty.clone(), span: span.clone(), def_id: None },
        );
        (IrStmtKind::Bind { var, mutability: Mutability::Let, ty, value: unwrap_expr }, span)
    }

    /// Recurse through the narrow admissible positions. `at_root` excludes the
    /// value's own root node (the caller decides root policy).
    fn search(
        e: &mut IrExpr,
        next_var: &mut u32,
        at_root: bool,
    ) -> Option<(IrStmtKind, Option<almide_ir::Span>)> {
        if !at_root && is_hoistable_unwrap(e) {
            return Some(hoist_here(e, next_var));
        }
        match &mut e.kind {
            IrExprKind::BinOp { op, left, right } => {
                if matches!(op, almide_ir::BinOp::And | almide_ir::BinOp::Or) {
                    return None;
                }
                if let Some(found) = search(left, next_var, false) {
                    return Some(found);
                }
                if !crate::lower::expr_contains_call(left) {
                    return search(right, next_var, false);
                }
                None
            }
            IrExprKind::List { elements } | IrExprKind::Tuple { elements } => {
                for el in elements.iter_mut() {
                    if let Some(found) = search(el, next_var, false) {
                        return Some(found);
                    }
                    if crate::lower::expr_contains_call(el) {
                        return None;
                    }
                }
                None
            }
            _ => None,
        }
    }

    search(value, next_var, true)
}
