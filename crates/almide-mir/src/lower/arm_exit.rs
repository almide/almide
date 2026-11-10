//! An early exit from a branch arm of a STATEMENT, on the certified native
//! render (#4640).
//!
//! The MIR has no mid-function return. A `guard c else E` at the fn-body tail
//! chain is expressible without one (`desugar_fn_body_guards`): the code after
//! the guard is exactly the fn's tail, so `if c then { rest } else E` is the
//! same control flow. A guard in an arm of an `if` / `match` STATEMENT is not:
//! the arm's value is dropped and the statements after the branch run, so the
//! guard stayed a `Guard` and the fn was refused. The front end's `guard let`
//! arrives the same way — a statement `match` whose failing arm is a bare
//! `err(e)` or `{ guard false else E; () }`.
//!
//! [`lift_stmt_arm_exits`] makes the branch the tail: the rest of the block
//! moves into every arm that carries on, and an arm that leaves keeps only its
//! exit.
//!
//! ```text
//! { pre; if c then { guard g else E; a } else (); rest; tail }
//!   ≡  { pre; if c then { guard g else E; a; rest; tail } else { rest; tail } }
//! ```
//!
//! The guard now stands where the block's value is the fn's value, the position
//! the guard restructure already rewrites. Only the fn's own tail chain is
//! walked — a block's tail, the arms of a tail branch — so the lifted branch is
//! always the fn's value: never a loop body, a lambda body or a bound value,
//! where an arm's value is not what the fn returns.
//!
//! A loop body is not on that chain, and an exit in one leaves through the
//! loop forms instead: one that can only raise is spelled as the propagation
//! the loop error flag carries (`LoopRaises`), and one that yields a value
//! stays a `Guard` for the loop early-return form
//! (`desugar_loop_early_returns`), which reads a loop of the tail chain — so a
//! branch statement whose arm holds such a loop is lifted as well.
//!
//! The rest of the block is duplicated once per continuing arm. A body that
//! would grow past [`MAX_LIFTED_NODES`] is left as it was and refused as before.

use almide_ir::visit_mut::{walk_expr_mut, IrMutVisitor};
use almide_ir::{IrExpr, IrExprKind, IrMatchArm, IrStmt, IrStmtKind};
use almide_lang::types::Ty;

/// Node ceiling of a lifted body: each lift copies the rest of the block into
/// every continuing arm, so a chain of N exiting branches is 2^N copies.
const MAX_LIFTED_NODES: usize = 200_000;

/// Run the lift over every fn of the program (the certified native render
/// only: the other render's lowering and its ledgers are unchanged).
pub fn lift_stmt_arm_exits(program: &mut almide_ir::IrProgram) {
    if !super::owned_tco_slots() {
        return;
    }
    let fns = program.functions.iter_mut().chain(program.modules.iter_mut().flat_map(|m| m.functions.iter_mut()));
    for func in fns {
        LoopRaises.visit_expr_mut(&mut func.body);
        if let Some(body) = lift_tail(&func.body).filter(|b| node_count(b) <= MAX_LIFTED_NODES) {
            func.body = body;
        }
    }
}

/// In a loop body an exit that can only raise takes the propagation spelling:
/// `guard c else err(e)` becomes `if c then () else { err(e)! }`, and a bare
/// `err(e)` arm of a branch statement — what the front end leaves of `guard
/// let … else err(e)` in an arm there — becomes `{ err(e)! }`. The meaning is
/// the same (the fn returns that err), and a `!` in a loop is the form the
/// loop error flag (`desugar_loop_unwrap`) already carries out of the loop and
/// past the statements after it. An else that can also yield a value keeps its
/// `Guard`. Lambda bodies are not entered: their exits leave the lambda.
struct LoopRaises;

impl IrMutVisitor for LoopRaises {
    fn visit_expr_mut(&mut self, e: &mut IrExpr) {
        match &mut e.kind {
            IrExprKind::Lambda { .. } => return,
            IrExprKind::ForIn { body, .. } | IrExprKind::While { body, .. } => loop_stmts_raise(body),
            _ => {}
        }
        walk_expr_mut(self, e);
    }
}

/// `E` as a Unit block that raises on every path, or `None` when a path of it
/// yields anything but a bare `err(e)`.
fn as_raise(e: &IrExpr) -> Option<IrExpr> {
    use almide_lang::types::constructor::TypeConstructorId;
    match &e.kind {
        IrExprKind::ResultErr { .. } => {
            let Ty::Applied(TypeConstructorId::Result, args) = &e.ty else { return None };
            let bang = IrExpr { kind: IrExprKind::Unwrap { expr: Box::new(e.clone()) }, ty: args.first()?.clone(), span: e.span, def_id: None };
            Some(block(vec![IrStmt { kind: IrStmtKind::Expr { expr: bang }, span: e.span }], None, &Ty::Unit, e))
        }
        IrExprKind::Block { stmts, expr: Some(tail) } => {
            let mut all = stmts.clone();
            all.push(IrStmt { kind: IrStmtKind::Expr { expr: as_raise(tail)? }, span: tail.span });
            Some(block(all, None, &Ty::Unit, e))
        }
        IrExprKind::If { cond, then, else_ } => {
            let kind = IrExprKind::If { cond: cond.clone(), then: Box::new(as_raise(then)?), else_: Box::new(as_raise(else_)?) };
            Some(IrExpr { kind, ty: Ty::Unit, span: e.span, def_id: None })
        }
        _ => None,
    }
}

fn loop_stmts_raise(stmts: &mut [IrStmt]) {
    for s in stmts {
        let raised = match &mut s.kind {
            IrStmtKind::Guard { cond, else_ } => as_raise(else_).map(|raise| match cond.kind {
                IrExprKind::LitBool { value: false } => raise,
                _ => {
                    let unit = IrExpr { kind: IrExprKind::Unit, ty: Ty::Unit, span: cond.span, def_id: None };
                    let kind = IrExprKind::If { cond: Box::new(cond.clone()), then: Box::new(unit), else_: Box::new(raise) };
                    IrExpr { kind, ty: Ty::Unit, span: cond.span, def_id: None }
                }
            }),
            IrStmtKind::Expr { expr } => {
                loop_spine_raise(expr, false);
                None
            }
            _ => None,
        };
        if let Some(expr) = raised {
            s.kind = IrStmtKind::Expr { expr };
        }
    }
}

fn loop_spine_raise(e: &mut IrExpr, is_arm: bool) {
    let unit = e.ty == Ty::Unit;
    match &mut e.kind {
        IrExprKind::ResultErr { .. } if is_arm => {
            if let Some(raise) = as_raise(e) {
                *e = raise;
            }
        }
        IrExprKind::Block { stmts, expr } => {
            loop_stmts_raise(stmts);
            if let Some(t) = expr.as_deref_mut() {
                loop_spine_raise(t, false);
            }
        }
        IrExprKind::If { then, else_, .. } if unit => {
            loop_spine_raise(then, true);
            loop_spine_raise(else_, true);
        }
        IrExprKind::Match { arms, .. } if unit => arms.iter_mut().for_each(|a| loop_spine_raise(&mut a.body, true)),
        _ => {}
    }
}

fn node_count(e: &IrExpr) -> usize {
    use almide_ir::visit::{walk_expr, IrVisitor};
    struct C(usize);
    impl IrVisitor for C {
        fn visit_expr(&mut self, e: &IrExpr) {
            self.0 += 1;
            walk_expr(self, e);
        }
    }
    let mut c = C(0);
    c.visit_expr(e);
    c.0
}

/// Is `e` a branch whose value a statement drops?
fn is_unit_branch(e: &IrExpr) -> bool {
    e.ty == Ty::Unit && matches!(e.kind, IrExprKind::If { .. } | IrExprKind::Match { .. } | IrExprKind::Block { .. })
}

/// Does an exit from the fn stand on `e`'s statement spine: a `Guard`
/// statement, an arm that is a bare `err(e)`, or a loop that exits by value?
/// Through arm blocks and the branch statements nested in them; never into a
/// lambda body or a bound value.
fn spine_leaves_fn(e: &IrExpr, is_arm: bool) -> bool {
    match &e.kind {
        IrExprKind::ResultErr { .. } => is_arm,
        IrExprKind::Block { stmts, expr } => {
            stmts.iter().any(|s| match &s.kind {
                IrStmtKind::Guard { .. } => true,
                IrStmtKind::Expr { expr } => spine_leaves_fn(expr, false),
                _ => false,
            }) || expr.as_deref().is_some_and(|t| spine_leaves_fn(t, false))
        }
        IrExprKind::If { then, else_, .. } => spine_leaves_fn(then, true) || spine_leaves_fn(else_, true),
        IrExprKind::Match { arms, .. } => arms.iter().any(|a| spine_leaves_fn(&a.body, true)),
        // A loop whose body exits by value: once the loop is a statement of the
        // fn's tail chain, the loop early-return form carries the value out.
        IrExprKind::ForIn { body, .. } | IrExprKind::While { body, .. } => body.iter().any(loop_stmt_exits),
        _ => false,
    }
}

/// Does a `Guard` stand on this loop-body statement's spine (the nested loops
/// included)?
fn loop_stmt_exits(s: &IrStmt) -> bool {
    fn spine(e: &IrExpr) -> bool {
        match &e.kind {
            IrExprKind::Block { stmts, expr } => stmts.iter().any(loop_stmt_exits) || expr.as_deref().is_some_and(spine),
            IrExprKind::If { then, else_, .. } => spine(then) || spine(else_),
            IrExprKind::Match { arms, .. } => arms.iter().any(|a| spine(&a.body)),
            IrExprKind::ForIn { body, .. } | IrExprKind::While { body, .. } => body.iter().any(loop_stmt_exits),
            _ => false,
        }
    }
    match &s.kind {
        IrStmtKind::Guard { .. } => true,
        IrStmtKind::Expr { expr } => spine(expr),
        _ => false,
    }
}

/// Does `e`'s value come only from spelled `ok(..)` constructors — the else
/// the front end keeps as a bare arm at a Unit tail?
fn yields_only_ok(e: &IrExpr) -> bool {
    match &e.kind {
        IrExprKind::ResultOk { .. } => true,
        IrExprKind::Block { expr: Some(tail), .. } => yields_only_ok(tail),
        IrExprKind::If { then, else_, .. } => yields_only_ok(then) && yields_only_ok(else_),
        IrExprKind::Match { arms, .. } => arms.iter().any(|_| true) && arms.iter().all(|a| yields_only_ok(&a.body)),
        _ => false,
    }
}

/// `E` with each spelled `ok(x)` it yields replaced by `x`, where the body's
/// type is `x`'s and not the Result: an effect fn that declares the bare
/// payload returns `x` as its ok, so `ok(x)` and `x` are the same early result
/// — and `x` is the leaf the arms beside it already yield. `None` when nothing
/// changed.
fn raw_ok_value(e: &IrExpr, ty: &Ty) -> Option<IrExpr> {
    let rebuilt = |kind: IrExprKind| IrExpr { kind, ty: ty.clone(), span: e.span, def_id: e.def_id };
    match &e.kind {
        IrExprKind::ResultOk { expr } if e.ty != *ty && expr.ty == *ty => Some((**expr).clone()),
        IrExprKind::Block { stmts, expr: Some(tail) } => {
            let tail = raw_ok_value(tail, ty)?;
            Some(rebuilt(IrExprKind::Block { stmts: stmts.clone(), expr: Some(Box::new(tail)) }))
        }
        IrExprKind::If { cond, then, else_ } => {
            let (t, f) = (raw_ok_value(then, ty), raw_ok_value(else_, ty));
            if t.is_none() && f.is_none() {
                return None;
            }
            Some(rebuilt(IrExprKind::If {
                cond: cond.clone(),
                then: Box::new(t.unwrap_or_else(|| (**then).clone())),
                else_: Box::new(f.unwrap_or_else(|| (**else_).clone())),
            }))
        }
        _ => None,
    }
}

fn block(stmts: Vec<IrStmt>, expr: Option<IrExpr>, ty: &Ty, like: &IrExpr) -> IrExpr {
    IrExpr { kind: IrExprKind::Block { stmts, expr: expr.map(Box::new) }, ty: ty.clone(), span: like.span, def_id: None }
}

/// The arm once an unconditional exit `guard false else E` ends it: `pre` are
/// the arm's statements before the exit, everything after it never runs.
///
/// The shape is the one the front end gives the same else at the fn's tail,
/// which every later rewrite already reads. In a Unit body an else that is not
/// itself Unit, a bare `err(e)` or a spelled `ok(..)` stays the `Guard` (the
/// Unit-tail ok-wrap would drop its value as a statement); every other else is
/// the arm's value, its own statements joining `pre`.
fn exit_arm(pre: Vec<IrStmt>, guard: &IrStmt, else_: &IrExpr, ty: &Ty, like: &IrExpr) -> IrExpr {
    let as_value = *ty != Ty::Unit
        || else_.ty == Ty::Unit
        || matches!(else_.kind, IrExprKind::ResultErr { .. })
        || yields_only_ok(else_);
    if !as_value {
        let mut stmts = pre;
        stmts.push(guard.clone());
        return block(stmts, None, ty, like);
    }
    let stripped = raw_ok_value(else_, ty);
    let else_ = stripped.as_ref().unwrap_or(else_);
    if pre.is_empty() {
        return else_.clone();
    }
    match &else_.kind {
        IrExprKind::Block { stmts, expr } => {
            let mut all = pre;
            all.extend(stmts.iter().cloned());
            block(all, expr.as_deref().cloned(), ty, like)
        }
        _ => block(pre, Some(else_.clone()), ty, like),
    }
}

/// One arm of the lifted branch: the arm's own statements, then the rest of
/// the enclosing block. A bare `err(e)` arm IS the exit and stays as it is; an
/// arm that reaches `guard false else E` ends there.
fn arm_with_rest(arm: &IrExpr, rest: &[IrStmt], tail: Option<&IrExpr>, ty: &Ty) -> IrExpr {
    if matches!(arm.kind, IrExprKind::ResultErr { .. }) {
        return arm.clone();
    }
    let mut stmts: Vec<IrStmt> = Vec::new();
    match &arm.kind {
        IrExprKind::Block { stmts: own, expr } => {
            for (i, s) in own.iter().enumerate() {
                if let IrStmtKind::Guard { cond, else_ } = &s.kind {
                    if matches!(cond.kind, IrExprKind::LitBool { value: false }) {
                        return exit_arm(own[..i].to_vec(), s, else_, ty, arm);
                    }
                }
            }
            stmts.extend(own.iter().cloned());
            if let Some(t) = expr.as_deref().filter(|t| !matches!(t.kind, IrExprKind::Unit)) {
                stmts.push(IrStmt { kind: IrStmtKind::Expr { expr: t.clone() }, span: t.span });
            }
        }
        IrExprKind::Unit => {}
        _ => stmts.push(IrStmt { kind: IrStmtKind::Expr { expr: arm.clone() }, span: arm.span }),
    }
    stmts.extend(rest.iter().cloned());
    block(stmts, tail.cloned(), ty, arm)
}

/// The branch statement `s` as the block's tail, the rest of the block in
/// every arm.
fn branch_with_rest(s: &IrExpr, rest: &[IrStmt], tail: Option<&IrExpr>, ty: &Ty) -> IrExpr {
    let kind = match &s.kind {
        IrExprKind::If { cond, then, else_ } => IrExprKind::If {
            cond: cond.clone(),
            then: Box::new(arm_with_rest(then, rest, tail, ty)),
            else_: Box::new(arm_with_rest(else_, rest, tail, ty)),
        },
        IrExprKind::Match { subject, arms } => IrExprKind::Match {
            subject: subject.clone(),
            arms: arms
                .iter()
                .map(|a| IrMatchArm {
                    pattern: a.pattern.clone(),
                    guard: a.guard.clone(),
                    body: arm_with_rest(&a.body, rest, tail, ty),
                })
                .collect(),
        },
        // A bare block statement is the one-armed branch.
        _ => return arm_with_rest(s, rest, tail, ty),
    };
    IrExpr { kind, ty: ty.clone(), span: s.span, def_id: s.def_id }
}

/// Lift every exiting branch statement on `e`'s tail chain; `e` stands where
/// its value is the fn's value. `None` when nothing on the chain exits.
fn lift_tail(e: &IrExpr) -> Option<IrExpr> {
    let rebuilt = |kind: IrExprKind| IrExpr { kind, ty: e.ty.clone(), span: e.span, def_id: e.def_id };
    match &e.kind {
        IrExprKind::Block { stmts, expr } => {
            let exiting = stmts.iter().position(|s| {
                matches!(&s.kind, IrStmtKind::Expr { expr } if is_unit_branch(expr) && spine_leaves_fn(expr, false))
            });
            if let Some(i) = exiting {
                let IrStmtKind::Expr { expr: s } = &stmts[i].kind else { return None };
                let lifted = branch_with_rest(s, &stmts[i + 1..], expr.as_deref(), &e.ty);
                let lifted = lift_tail(&lifted).unwrap_or(lifted);
                return Some(rebuilt(IrExprKind::Block { stmts: stmts[..i].to_vec(), expr: Some(Box::new(lifted)) }));
            }
            let tail = lift_tail(expr.as_deref()?)?;
            Some(rebuilt(IrExprKind::Block { stmts: stmts.clone(), expr: Some(Box::new(tail)) }))
        }
        IrExprKind::If { cond, then, else_ } => {
            let (t, f) = (lift_tail(then), lift_tail(else_));
            if t.is_none() && f.is_none() {
                return None;
            }
            Some(rebuilt(IrExprKind::If {
                cond: cond.clone(),
                then: Box::new(t.unwrap_or_else(|| (**then).clone())),
                else_: Box::new(f.unwrap_or_else(|| (**else_).clone())),
            }))
        }
        IrExprKind::Match { subject, arms } => {
            let bodies: Vec<Option<IrExpr>> = arms.iter().map(|a| lift_tail(&a.body)).collect();
            if bodies.iter().all(Option::is_none) {
                return None;
            }
            let arms = arms
                .iter()
                .zip(bodies)
                .map(|(a, b)| IrMatchArm { pattern: a.pattern.clone(), guard: a.guard.clone(), body: b.unwrap_or_else(|| a.body.clone()) })
                .collect();
            Some(rebuilt(IrExprKind::Match { subject: subject.clone(), arms }))
        }
        _ => None,
    }
}
