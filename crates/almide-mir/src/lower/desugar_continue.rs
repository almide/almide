// ── a nested `continue` moved to the end of its iteration (#4386) ──
//
// The loop lowering runs a `continue` in two positions: a statement of the loop
// body itself (`if c then continue else ()`, the guarded rest), and the LAST
// thing an iteration does, where it is `()`. A `continue` inside an `if`/`match`
// arm that has more loop-body statements after it is neither:
//
//   for x in xs { if x > 1 then { if x != 3 then { out = out + l } else continue } else (); out = out + "." }
//
// so it walled — and before #4386 it was rewritten to `()`, which ran the `.` on
// the skipped element. This pass SINKS the statements after such a construct
// into each of its arms (`… else { continue; out = out + "." }` is the same
// iteration as before, in every path), after which every `continue` in it is
// the last thing its path runs and becomes `()`. Only a construct that holds a
// `continue` of THIS loop is touched (not one inside a nested loop, which is
// that loop's own), so a loop body without one keeps its shape.
//
// The sunk statements are DUPLICATED, one copy per arm, like the tail
// duplication of `let s = <branch>; rest` (desugar_branch.rs): every path still
// runs each statement at most once, and the caps / interp counts read this
// desugared tree. Each nested construct doubles what follows it, so the copies
// are bounded; past the bound the body is left as it was and walls honestly.

/// IR nodes the sinking may add to one loop body before it gives up.
const MAX_SUNK_CONTINUE_NODES: i64 = 4096;

/// The loop body with every this-loop `continue` moved to the end of its path, or
/// the body unchanged when it holds none in a reachable shape (or the copies would
/// pass the bound).
fn tail_loop_continues(body: Vec<almide_ir::IrStmt>, changed: &mut bool) -> Vec<almide_ir::IrStmt> {
    if !body.iter().any(stmt_has_loop_continue) {
        return body;
    }
    let mut budget = MAX_SUNK_CONTINUE_NODES;
    let mut moved = false;
    match continue_tail_loop_stmts(body.clone(), &mut budget, &mut moved) {
        Some(new_body) if moved => {
            *changed = true;
            new_body
        }
        _ => body,
    }
}

/// A LOOP-BODY statement list (or the rest after one of its statements): the
/// shapes the lowering already runs there — a bare `continue`, and
/// `if c then continue else ()` with its guarded rest — are kept, and the rest
/// after them is handled the same way; the first other construct holding a
/// `continue` takes [`continue_tail_stmts`].
fn continue_tail_loop_stmts(
    mut stmts: Vec<almide_ir::IrStmt>,
    budget: &mut i64,
    moved: &mut bool,
) -> Option<Vec<almide_ir::IrStmt>> {
    let Some(i) = stmts.iter().position(stmt_has_loop_continue) else { return Some(stmts) };
    let almide_ir::IrStmtKind::Expr { expr } = &stmts[i].kind else { return Some(stmts) };
    let kept = match &expr.kind {
        almide_ir::IrExprKind::If { then, else_, .. } => is_bare_continue(then) && is_unit_arm(else_),
        _ => is_bare_continue(expr),
    };
    if !kept {
        return continue_tail_stmts(stmts, budget, moved);
    }
    let rest = stmts.split_off(i + 1);
    stmts.extend(continue_tail_loop_stmts(rest, budget, moved)?);
    Some(stmts)
}

/// `continue`, alone or as a one-statement block (the lowering's own test).
fn is_bare_continue(e: &almide_ir::IrExpr) -> bool {
    use almide_ir::{IrExprKind, IrStmtKind};
    match &e.kind {
        IrExprKind::Continue => true,
        IrExprKind::Block { stmts, expr } => match (stmts.as_slice(), expr) {
            ([], Some(tail)) => is_bare_continue(tail),
            ([only], None) => matches!(&only.kind, IrStmtKind::Expr { expr } if is_bare_continue(expr)),
            _ => false,
        },
        _ => false,
    }
}

fn is_unit_arm(e: &almide_ir::IrExpr) -> bool {
    match &e.kind {
        almide_ir::IrExprKind::Unit => true,
        almide_ir::IrExprKind::Block { stmts, expr } => stmts.is_empty() && expr.as_deref().is_none_or(is_unit_arm),
        _ => false,
    }
}

/// `stmts` (the rest of an iteration) with the first construct holding a
/// `continue` absorbing the statements after it, the statements after a bare
/// `continue` (never reached) dropped, and the last statement's `continue`s turned
/// into `()`. `None` = the bound was passed, or a `continue` sits in a shape this
/// pass does not move.
fn continue_tail_stmts(
    mut stmts: Vec<almide_ir::IrStmt>,
    budget: &mut i64,
    moved: &mut bool,
) -> Option<Vec<almide_ir::IrStmt>> {
    use almide_ir::IrStmtKind;
    let stop = |s: &almide_ir::IrStmt| match &s.kind {
        IrStmtKind::Expr { expr } => expr_has_loop_continue(expr),
        _ => false,
    };
    let Some(i) = stmts.iter().position(stop) else { return Some(stmts) };
    let mut rest = stmts.split_off(i + 1);
    let IrStmtKind::Expr { expr } = stmts.pop()?.kind else { unreachable!("stop() matched an Expr statement") };
    if matches!(expr.kind, almide_ir::IrExprKind::Continue) {
        rest.clear();
    }
    charge(budget, &rest)?;
    let span = expr.span;
    let expr = continue_tail_expr(expr, rest, budget, moved)?;
    stmts.push(almide_ir::IrStmt { span, kind: IrStmtKind::Expr { expr } });
    Some(stmts)
}

/// `e` (an iteration's last statement, followed by `rest`) with `rest` sunk into
/// each arm and its tail `continue`s turned into `()`.
fn continue_tail_expr(
    e: almide_ir::IrExpr,
    rest: Vec<almide_ir::IrStmt>,
    budget: &mut i64,
    moved: &mut bool,
) -> Option<almide_ir::IrExpr> {
    use almide_ir::{IrExpr, IrExprKind};
    let kind = match e.kind {
        IrExprKind::Continue => {
            *moved = true;
            return Some(arm_block(Vec::new(), e.span, e.def_id));
        }
        IrExprKind::If { cond, then, else_ } => {
            charge(budget, &rest)?; // the second copy (the caller paid for the first)
            let then = continue_tail_arm(*then, rest.clone(), budget, moved)?;
            let else_ = continue_tail_arm(*else_, rest, budget, moved)?;
            IrExprKind::If { cond, then: Box::new(then), else_: Box::new(else_) }
        }
        IrExprKind::Match { subject, arms } => {
            let mut out = Vec::with_capacity(arms.len());
            for arm in arms {
                charge(budget, &rest)?;
                let body = continue_tail_arm(arm.body, rest.clone(), budget, moved)?;
                out.push(almide_ir::IrMatchArm { body, ..arm });
            }
            IrExprKind::Match { subject, arms: out }
        }
        IrExprKind::Block { .. } => return continue_tail_arm(e, rest, budget, moved),
        _ => return None,
    };
    *moved = true;
    Some(IrExpr { kind, ty: Ty::Unit, span: e.span, def_id: e.def_id })
}

/// One arm run as statements, then `rest`, as a Unit block.
fn continue_tail_arm(
    arm: almide_ir::IrExpr,
    rest: Vec<almide_ir::IrStmt>,
    budget: &mut i64,
    moved: &mut bool,
) -> Option<almide_ir::IrExpr> {
    use almide_ir::{IrExprKind, IrStmt, IrStmtKind};
    let (span, def_id) = (arm.span, arm.def_id);
    let mut stmts = match arm.kind {
        IrExprKind::Unit => Vec::new(),
        IrExprKind::Block { stmts, expr } => {
            let mut stmts = stmts;
            if let Some(tail) = expr {
                stmts.push(IrStmt { span: tail.span, kind: IrStmtKind::Expr { expr: *tail } });
            }
            stmts
        }
        kind => vec![IrStmt { span, kind: IrStmtKind::Expr { expr: almide_ir::IrExpr { kind, ty: arm.ty, span, def_id } } }],
    };
    stmts.extend(rest);
    Some(arm_block(continue_tail_stmts(stmts, budget, moved)?, span, def_id))
}

fn arm_block(stmts: Vec<almide_ir::IrStmt>, span: Option<almide_ir::Span>, def_id: Option<almide_ir::DefId>) -> almide_ir::IrExpr {
    almide_ir::IrExpr { kind: almide_ir::IrExprKind::Block { stmts, expr: None }, ty: Ty::Unit, span, def_id }
}

fn stmt_has_loop_continue(s: &almide_ir::IrStmt) -> bool {
    match &s.kind {
        almide_ir::IrStmtKind::Expr { expr } => expr_has_loop_continue(expr),
        _ => false,
    }
}

/// Does `e` hold a `continue` of the loop it sits in (not of a nested loop, and not
/// inside a lambda)?
fn expr_has_loop_continue(e: &almide_ir::IrExpr) -> bool {
    use almide_ir::visit::{walk_expr, IrVisitor};
    use almide_ir::IrExprKind;
    struct Scan(bool);
    impl IrVisitor for Scan {
        fn visit_expr(&mut self, e: &almide_ir::IrExpr) {
            match &e.kind {
                IrExprKind::Continue => self.0 = true,
                IrExprKind::ForIn { .. } | IrExprKind::While { .. } | IrExprKind::Lambda { .. } => {}
                _ => walk_expr(self, e),
            }
        }
    }
    let mut s = Scan(false);
    s.visit_expr(e);
    s.0
}

/// Pay for one more copy of `rest`; `None` once the bound is passed.
fn charge(budget: &mut i64, rest: &[almide_ir::IrStmt]) -> Option<()> {
    *budget -= count_stmt_nodes(rest);
    (*budget >= 0).then_some(())
}

fn count_stmt_nodes(stmts: &[almide_ir::IrStmt]) -> i64 {
    use almide_ir::visit::{walk_expr, IrVisitor};
    struct C(i64);
    impl IrVisitor for C {
        fn visit_expr(&mut self, e: &almide_ir::IrExpr) {
            self.0 += 1;
            walk_expr(self, e);
        }
    }
    let mut c = C(0);
    for s in stmts {
        c.visit_stmt(s);
    }
    c.0
}
