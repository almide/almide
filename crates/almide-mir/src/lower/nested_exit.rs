//! An exit nested in a value the fn goes on to use, on the certified native
//! render (#4678).
//!
//! A failing `guard let` returns from the fn wherever its block stands. The
//! front end lowers one in a block whose value an enclosing expression uses
//! (a call argument, an operand, an element, a condition, an interpolation
//! part, a range end) to a match whose failing arm is `{ guard false else E;
//! <unreachable> }`. The MIR has no mid-function return, so that guard was
//! refused there.
//!
//! [`hoist_nested_exit`] moves such a value out of its expression into a bind
//! ahead of the statement, with every operand evaluated before it bound first,
//! in order:
//!
//! ```text
//! { pre; let k = f(a(), { …; match s { ok(v) => v, _ => { guard false else E; ⊥ } } }); rest }
//!   ≡  { pre; let t1 = a(); …; let t2 = match s { … }; let k = f(t1, t2); rest }
//! ```
//!
//! The block's own statements join the enclosing block (its names are unique
//! VarIds). The exit then stands in a bound branch on the fn's tail chain,
//! which [`super::arm_exit`] lifts: the bind and the rest of the block move
//! into every arm, and the exiting arm ends with `E`. Only operands evaluated
//! UNCONDITIONALLY are walked — never an arm, the right of `and` / `or`, a
//! `??` fallback, a loop body or condition, or a lambda body — so the
//! evaluation order is the source order.
//!
//! A `while` condition runs before every turn, so it cannot leave the
//! statement. [`while_exit`] moves it into the loop body instead, where the
//! loop early-return form carries an exit out:
//!
//! ```text
//! while C { body }  ≡  var go = true; while go { let c = C; if c then { body } else { go = false } }
//! ```

use almide_ir::{IrExpr, IrExprKind, IrStmt, IrStmtKind, IrStringPart, Mutability, VarTable};
use almide_lang::types::Ty;

/// Does an exit from the fn stand on `e`'s value spine: a `Guard` statement
/// of a block, a block's tail, or an arm of an `if` / `match`?
pub(super) fn spine_exits(e: &IrExpr) -> bool {
    match &e.kind {
        IrExprKind::Block { stmts, expr } => {
            stmts.iter().any(|s| matches!(s.kind, IrStmtKind::Guard { .. })) || expr.as_deref().is_some_and(spine_exits)
        }
        IrExprKind::If { then, else_, .. } => spine_exits(then) || spine_exits(else_),
        IrExprKind::Match { arms, .. } => arms.iter().any(|a| spine_exits(&a.body)),
        _ => false,
    }
}

/// The operands of `e` that are evaluated unconditionally, in evaluation
/// order. A kind not listed has none walked.
fn strict_operands(e: &mut IrExpr) -> Vec<&mut IrExpr> {
    use almide_ir::BinOp;
    match &mut e.kind {
        IrExprKind::Call { args, .. } | IrExprKind::TailCall { args, .. } | IrExprKind::RuntimeCall { args, .. } => {
            args.iter_mut().collect()
        }
        IrExprKind::BinOp { op: BinOp::And | BinOp::Or, left, .. } => vec![&mut **left],
        IrExprKind::BinOp { left, right, .. } => vec![&mut **left, &mut **right],
        IrExprKind::List { elements } | IrExprKind::Tuple { elements } => elements.iter_mut().collect(),
        IrExprKind::Record { fields, .. } => fields.iter_mut().map(|(_, v)| v).collect(),
        IrExprKind::SpreadRecord { base, fields } => {
            std::iter::once(&mut **base).chain(fields.iter_mut().map(|(_, v)| v)).collect()
        }
        IrExprKind::Range { start, end, .. } => vec![&mut **start, &mut **end],
        IrExprKind::IndexAccess { object, index } | IrExprKind::MapAccess { object, key: index } => {
            vec![&mut **object, &mut **index]
        }
        IrExprKind::StringInterp { parts } => parts
            .iter_mut()
            .filter_map(|p| match p {
                IrStringPart::Expr { expr } => Some(expr),
                IrStringPart::Lit { .. } => None,
            })
            .collect(),
        IrExprKind::If { cond: x, .. }
        | IrExprKind::Match { subject: x, .. }
        | IrExprKind::ForIn { iterable: x, .. }
        | IrExprKind::UnOp { operand: x, .. }
        | IrExprKind::Member { object: x, .. }
        | IrExprKind::TupleIndex { object: x, .. }
        | IrExprKind::ResultOk { expr: x }
        | IrExprKind::ResultErr { expr: x }
        | IrExprKind::OptionSome { expr: x }
        | IrExprKind::UnwrapOr { expr: x, .. }
        | IrExprKind::Unwrap { expr: x }
        | IrExprKind::Try { expr: x }
        | IrExprKind::ToOption { expr: x } => vec![&mut **x],
        _ => Vec::new(),
    }
}

/// Is `e` a value whose evaluation the hoisted value cannot change or observe:
/// a literal, or a variable that is never reassigned?
fn is_stable(e: &IrExpr, vt: &VarTable) -> bool {
    match &e.kind {
        IrExprKind::LitInt { .. } | IrExprKind::LitFloat { .. } | IrExprKind::LitStr { .. } | IrExprKind::LitBool { .. } => true,
        IrExprKind::Unit | IrExprKind::OptionNone | IrExprKind::FnRef { .. } => true,
        IrExprKind::Var { id } => vt.get(*id).mutability == Mutability::Let,
        _ => false,
    }
}

/// `let <fresh> = <e>` ahead of the statement; `e` becomes the fresh variable.
fn bind_out(e: &mut IrExpr, out: &mut Vec<IrStmt>, vt: &mut VarTable) {
    let ty = e.ty.clone();
    let var = vt.alloc(almide_base::intern::sym("__exit_v"), ty.clone(), Mutability::Let, e.span);
    let read = IrExpr { kind: IrExprKind::Var { id: var }, ty: ty.clone(), span: e.span, def_id: None };
    let value = std::mem::replace(e, read);
    flatten_bind(var, ty, value, out);
}

/// `let var = value`, a block value's statements first.
fn flatten_bind(var: almide_ir::VarId, ty: Ty, value: IrExpr, out: &mut Vec<IrStmt>) {
    let span = value.span;
    match value.kind {
        IrExprKind::Block { stmts, expr: Some(tail) } => {
            out.extend(stmts);
            flatten_bind(var, ty, *tail, out);
        }
        kind => {
            let value = IrExpr { kind, ty: ty.clone(), span, def_id: value.def_id };
            out.push(IrStmt { kind: IrStmtKind::Bind { var, mutability: Mutability::Let, ty, value }, span });
        }
    }
}

/// Hoist the first exiting value among `e`'s unconditional operands (or
/// deeper in them) into `out`, the operands before it bound first. `true`
/// when one was hoisted.
fn hoist_in(e: &mut IrExpr, out: &mut Vec<IrStmt>, vt: &mut VarTable) -> bool {
    let mut ops = strict_operands(e);
    let Some(at) = ops.iter().position(|o| spine_exits(o) || contains_nested(o)) else { return false };
    for o in ops.iter_mut().take(at) {
        if !is_stable(o, vt) {
            bind_out(o, out, vt);
        }
    }
    let target = &mut *ops[at];
    if spine_exits(target) {
        bind_out(target, out, vt);
        return true;
    }
    hoist_in(target, out, vt)
}

/// Does an exiting value stand among `e`'s unconditional operands, at any depth?
fn contains_nested(e: &IrExpr) -> bool {
    let mut copy = e.clone();
    strict_operands(&mut copy).iter().any(|o| spine_exits(o) || contains_nested(o))
}

/// Does a `Guard` statement stand anywhere in `s` / `e`?
struct GuardScan(bool);

impl almide_ir::visit::IrVisitor for GuardScan {
    fn visit_expr(&mut self, e: &IrExpr) {
        if !self.0 {
            almide_ir::visit::walk_expr(self, e);
        }
    }
    fn visit_stmt(&mut self, s: &IrStmt) {
        self.0 |= matches!(s.kind, IrStmtKind::Guard { .. });
        almide_ir::visit::walk_stmt(self, s);
    }
}

fn stmt_has_guard(s: &IrStmt) -> bool {
    use almide_ir::visit::IrVisitor;
    let mut scan = GuardScan(false);
    scan.visit_stmt(s);
    scan.0
}

fn has_guard(e: &IrExpr) -> bool {
    use almide_ir::visit::IrVisitor;
    let mut scan = GuardScan(false);
    scan.visit_expr(e);
    scan.0
}

/// The statements with every `let x = { stmts; tail }` that holds a guard
/// spliced into them as `stmts; let x = tail` (VarIds are unique, so the
/// block's names may join the enclosing block). An earlier pass binds a
/// literal's elements this way. `None` when there is none.
fn flatten_bound_blocks(stmts: &[IrStmt]) -> Option<Vec<IrStmt>> {
    let at = stmts.iter().position(|s| {
        matches!(&s.kind, IrStmtKind::Bind { value, .. }
            if matches!(value.kind, IrExprKind::Block { expr: Some(_), .. }) && has_guard(value))
    })?;
    let IrStmtKind::Bind { var, ty, value, mutability } = &stmts[at].kind else { return None };
    let mut out = stmts[..at].to_vec();
    flatten_bind(*var, ty.clone(), value.clone(), &mut out);
    if let Some(IrStmt { kind: IrStmtKind::Bind { mutability: m, .. }, .. }) = out.last_mut() {
        *m = *mutability;
    }
    out.extend(stmts[at + 1..].iter().cloned());
    Some(out)
}

/// The value expression of a statement the hoist may rewrite.
fn stmt_value(s: &mut IrStmt) -> Option<&mut IrExpr> {
    match &mut s.kind {
        IrStmtKind::Bind { value, .. }
        | IrStmtKind::BindDestructure { value, .. }
        | IrStmtKind::Assign { value, .. }
        | IrStmtKind::Expr { expr: value } => Some(value),
        _ => None,
    }
}

/// `while C { body }` whose condition `C` holds an exit on its value spine,
/// as the two statements of the module doc. The condition's bind is lifted
/// in the loop body the way a bound branch is on the tail chain, its exiting
/// arm ending in the `Guard`. `None` for any other statement.
fn while_exit(s: &IrStmt, vt: &mut VarTable) -> Option<Vec<IrStmt>> {
    let IrStmtKind::Expr { expr } = &s.kind else { return None };
    let IrExprKind::While { cond, body } = &expr.kind else { return None };
    if !spine_exits(cond) {
        return None;
    }
    let span = cond.span;
    let var = |kind| IrExpr { kind, ty: Ty::Bool, span, def_id: None };
    let go = vt.alloc(almide_base::intern::sym("__while_go"), Ty::Bool, Mutability::Var, span);
    let c = vt.alloc(almide_base::intern::sym("__while_c"), Ty::Bool, Mutability::Let, span);
    let mut turn = Vec::new();
    flatten_bind(c, Ty::Bool, (**cond).clone(), &mut turn);
    let unit_block = |stmts| IrExpr { kind: IrExprKind::Block { stmts, expr: None }, ty: Ty::Unit, span, def_id: None };
    let stop = IrStmt { kind: IrStmtKind::Assign { var: go, value: var(IrExprKind::LitBool { value: false }) }, span };
    let step = IrExprKind::If {
        cond: Box::new(var(IrExprKind::Var { id: c })),
        then: Box::new(unit_block(body.clone())),
        else_: Box::new(unit_block(vec![stop])),
    };
    let step = IrStmt { kind: IrStmtKind::Expr { expr: IrExpr { kind: step, ty: Ty::Unit, span, def_id: None } }, span };
    let bind = turn.pop()?;
    let IrStmtKind::Bind { value, .. } = &bind.kind else { return None };
    if !super::arm_exit::bound_branch_exits(value) {
        return None;
    }
    let lifted = super::arm_exit::bound_branch_with_rest(value, &bind, &[step], None, &Ty::Unit)?;
    turn.push(IrStmt { kind: IrStmtKind::Expr { expr: lifted }, span });
    let looped = IrExprKind::While { cond: Box::new(var(IrExprKind::Var { id: go })), body: turn };
    let init = IrStmtKind::Bind { var: go, mutability: Mutability::Var, ty: Ty::Bool, value: var(IrExprKind::LitBool { value: true }) };
    Some(vec![
        IrStmt { kind: init, span },
        IrStmt { kind: IrStmtKind::Expr { expr: IrExpr { kind: looped, ty: expr.ty.clone(), span: expr.span, def_id: None } }, span: s.span },
    ])
}

/// Rewrite the first statement (or the tail) of a tail-chain block whose
/// value holds an exit nested in an operand. `None` when there is none.
pub(super) fn hoist_nested_exit(stmts: &[IrStmt], tail: Option<&IrExpr>, vt: &mut VarTable) -> Option<(Vec<IrStmt>, Option<IrExpr>)> {
    if !stmts.iter().any(|s| stmt_has_guard(s)) && !tail.is_some_and(has_guard) {
        return None;
    }
    if let Some(flat) = flatten_bound_blocks(stmts) {
        return Some((flat, tail.cloned()));
    }
    for (i, s) in stmts.iter().enumerate() {
        if let Some(rewritten) = while_exit(s, vt) {
            let mut all = stmts[..i].to_vec();
            all.extend(rewritten);
            all.extend(stmts[i + 1..].iter().cloned());
            return Some((all, tail.cloned()));
        }
        let mut s = s.clone();
        let mut out = Vec::new();
        if stmt_value(&mut s).is_some_and(|v| hoist_in(v, &mut out, vt)) {
            let mut all = stmts[..i].to_vec();
            all.extend(out);
            all.push(s);
            all.extend(stmts[i + 1..].iter().cloned());
            return Some((all, tail.cloned()));
        }
    }
    let mut t = tail?.clone();
    let mut out = Vec::new();
    if !hoist_in(&mut t, &mut out, vt) {
        return None;
    }
    let mut all = stmts.to_vec();
    all.extend(out);
    Some((all, Some(t)))
}
