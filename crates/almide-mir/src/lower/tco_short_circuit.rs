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
