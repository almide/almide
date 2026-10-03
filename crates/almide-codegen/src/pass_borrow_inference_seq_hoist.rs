// ── Phase 3b: operand sequences with a NESTED `&mut` (#3230) ──────────
//
// The direct rule (`hoist_call_if_needed`) sees a `&mut x` that IS an
// argument. A `&mut x` can also sit INSIDE a later operand — the argument of
// a nested call (`three(s, grow(s), s)` renders `three(&*s, &*grow(&mut s),
// &*s)`), or a string-interpolation part (`"${c} ${bump(c)}"` renders
// `format!("{} {}", c, bump(&mut c))`, and `format!` borrows every argument
// for the whole statement). An EARLIER operand that reads `x` then holds a
// shared borrow across the `&mut`, and rustc refuses the program (E0502).
//
// The fix binds those earlier operands to `let __hoist` temps before the
// sequence, so they are values by the time the `&mut` is taken. Order is the
// language's left-to-right: the hoisted set is a PREFIX of the sequence —
// every operand up to the last conflicting one that reads `x` or can run user
// code — so no effect moves past another, and an operand AFTER the `&mut`
// is never hoisted (it must see the write-back).

/// Every root variable a `&mut` place anywhere inside `e` borrows.
fn nested_mut_roots(e: &IrExpr) -> Vec<VarId> {
    struct Roots(Vec<VarId>);
    impl IrVisitor for Roots {
        fn visit_expr(&mut self, e: &IrExpr) {
            if let Some(v) = find_mut_borrow_var(e) {
                self.0.push(v);
            }
            walk_expr(self, e);
        }
    }
    let mut r = Roots(Vec::new());
    r.visit_expr(e);
    r.0
}

/// Hoist the operands of `items` that precede a nested `&mut` of a variable
/// they read (plus, for order, every effectful operand before the last such
/// one). A direct `&mut` operand is never hoisted itself.
fn hoist_before_nested_mut(items: Vec<IrExpr>, hoisted: &mut Vec<IrStmt>, cx: &mut HoistCx<'_>) -> Vec<IrExpr> {
    let roots: Vec<Vec<VarId>> = items.iter().map(nested_mut_roots).collect();
    let conflicts: Vec<bool> = (0..items.len())
        .map(|i| {
            find_mut_borrow_var(&items[i]).is_none()
                && roots[i + 1..].iter().flatten().any(|r| cx.must_hoist(&items[i], *r))
        })
        .collect();
    let Some(last) = conflicts.iter().rposition(|c| *c) else { return items };
    items
        .into_iter()
        .enumerate()
        .map(|(i, it)| {
            let in_prefix = i <= last && find_mut_borrow_var(&it).is_none();
            if in_prefix && (conflicts[i] || may_run_user_code(&it)) {
                hoist_one_arg(it, hoisted, cx)
            } else {
                it
            }
        })
        .collect()
}

/// The direct rule: a call with `&mut x` as one argument hoists every other
/// argument that reads `x` (see [`hoist_conflicting_reads`]).
fn hoist_direct_mut_conflicts(args: Vec<IrExpr>, hoisted: &mut Vec<IrStmt>, cx: &mut HoistCx<'_>) -> Vec<IrExpr> {
    let Some(mut_id) = args.iter().find_map(find_mut_borrow_var) else { return args };
    args.into_iter()
        .map(|arg| {
            if find_mut_borrow_var(&arg).is_none() && cx.must_hoist(&arg, mut_id) {
                hoist_one_arg(arg, hoisted, cx)
            } else {
                arg
            }
        })
        .collect()
}

/// A string interpolation is an operand sequence: apply
/// [`hoist_before_nested_mut`] to its expression parts. Any other node is
/// returned as is.
fn hoist_interp_parts(node: IrExpr, cx: &mut HoistCx<'_>) -> IrExpr {
    let IrExpr { kind: IrExprKind::StringInterp { parts }, ty, span, def_id } = node else { return node };
    let mut exprs = Vec::new();
    let mut skeleton = Vec::with_capacity(parts.len());
    for part in parts {
        match part {
            IrStringPart::Expr { expr } => {
                exprs.push(expr);
                skeleton.push(None);
            }
            lit => skeleton.push(Some(lit)),
        }
    }
    let mut hoisted = Vec::new();
    let mut exprs = hoist_before_nested_mut(exprs, &mut hoisted, cx).into_iter();
    let parts = skeleton
        .into_iter()
        .map(|slot| slot.unwrap_or_else(|| IrStringPart::Expr { expr: exprs.next().expect("one expr per slot") }))
        .collect();
    let interp = IrExpr { kind: IrExprKind::StringInterp { parts }, ty, span, def_id };
    wrap_hoisted(hoisted, interp)
}

/// `{ hoisted…; value }`, or `value` itself when nothing was hoisted.
fn wrap_hoisted(hoisted: Vec<IrStmt>, value: IrExpr) -> IrExpr {
    if hoisted.is_empty() {
        return value;
    }
    let (ty, span) = (value.ty.clone(), value.span);
    IrExpr { kind: IrExprKind::Block { stmts: hoisted, expr: Some(Box::new(value)) }, ty, span, def_id: None }
}
