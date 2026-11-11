/// A short-circuit RHS is a tail position. Expose it to the existing loop
/// conversion without moving or duplicating the LHS: its call, if recursive,
/// remains an ordinary non-tail call evaluated before the branch. Only walk
/// function-tail positions, never a condition, operand, statement or lambda.
/// This tree is used only if the complete TCO rewrite succeeds.
fn expose_short_circuit_tails(e: &mut IrExpr) {
    use almide_ir::BinOp;
    match &mut e.kind {
        IrExprKind::If { then, else_, .. } => {
            expose_short_circuit_tails(then);
            expose_short_circuit_tails(else_);
        }
        IrExprKind::Match { arms, .. } => {
            for arm in arms { expose_short_circuit_tails(&mut arm.body); }
        }
        IrExprKind::Block { expr: Some(tail), .. } => expose_short_circuit_tails(tail),
        IrExprKind::BinOp { op: BinOp::And | BinOp::Or, .. } => {
            let IrExprKind::BinOp { op, left, mut right } = e.kind.clone() else { unreachable!() };
            expose_short_circuit_tails(&mut right);
            let short = Box::new(tco_ir(IrExprKind::LitBool { value: op == BinOp::Or }, Ty::Bool));
            let (then, else_) = if op == BinOp::And { (right, short) } else { (short, right) };
            e.kind = IrExprKind::If { cond: left, then, else_ };
        }
        _ => {}
    }
}

/// A `guard c else E` among the statements of a function-tail block is a
/// branch of the tail: `{ pre; guard c else E; rest }` is `{ pre; if c then
/// { rest } else E }`, the else being the function's result there. Spell it
/// so before the loop conversion, which reads a tail `if` arm by arm and left
/// a guard statement in the loop body with no way to deliver its else
/// (#4640). Only where the else has the block's own type — a value, never a
/// diverging else — and only on function-tail positions.
fn expose_guard_tails(e: &mut IrExpr) {
    match &mut e.kind {
        IrExprKind::If { then, else_, .. } => {
            expose_guard_tails(then);
            expose_guard_tails(else_);
        }
        IrExprKind::Match { arms, .. } => {
            for arm in arms { expose_guard_tails(&mut arm.body); }
        }
        IrExprKind::Block { stmts, expr: Some(tail) } => {
            let ty = tail.ty.clone();
            let at = stmts.iter().position(|s| matches!(&s.kind, IrStmtKind::Guard { else_, .. } if else_.ty == ty));
            let Some(at) = at else { return expose_guard_tails(tail) };
            let rest: Vec<IrStmt> = stmts.drain(at + 1..).collect();
            let Some(IrStmt { kind: IrStmtKind::Guard { cond, else_ }, .. }) = stmts.pop() else { unreachable!() };
            let old_tail = std::mem::replace(&mut **tail, tco_ir(IrExprKind::Unit, Ty::Unit));
            let mut then = if rest.is_empty() {
                old_tail
            } else {
                tco_ir(IrExprKind::Block { stmts: rest, expr: Some(Box::new(old_tail)) }, ty.clone())
            };
            expose_guard_tails(&mut then);
            **tail = tco_ir(IrExprKind::If { cond: Box::new(cond), then: Box::new(then), else_: Box::new(else_) }, ty);
        }
        _ => {}
    }
}
