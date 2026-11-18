//! A `guard let` exit nested in an operand is bound ahead of its statement: a
//! target-independent IR rewrite (#4678).
//!
//! A failing `guard let` returns from the fn wherever its block stands. The
//! front end lowers one whose block's value an enclosing expression uses (a
//! call argument, an operand, an element, a condition, an interpolation part,
//! a range end) to a match whose failing arm is the guard's return channel,
//! `{ guard false else E; <a Never value> }`. Every leg runs that channel, but
//! a leg that builds the enclosing expression piece by piece (a literal's
//! allocation and stores, an argument list) then holds a half-built value
//! across a return.
//!
//! [`hoist_value_exits`] moves such a value out of its expression into a bind
//! ahead of the statement, every operand evaluated before it bound first, in
//! order, so the source's evaluation order is unchanged:
//!
//! ```text
//! let k = f(a(), { …; match s { ok(v) => v, _ => { guard false else E; ⊥ } } })
//!   ≡  let t1 = a(); …; let t2 = match s { … }; let k = f(t1, t2)
//! ```
//!
//! The block's own statements join the enclosing block (its names are unique
//! VarIds). Only operands evaluated UNCONDITIONALLY are walked — never an arm,
//! the right of `and` / `or`, a `??` fallback, a loop body or condition, or a
//! lambda body.

use crate::visit_mut::{walk_expr_mut, IrMutVisitor};
use crate::{BinOp, IrExpr, IrExprKind, IrProgram, IrStmt, IrStmtKind, IrStringPart, Mutability, VarId, VarTable};
use almide_lang::types::Ty;

/// Does an exit from the fn stand on `e`'s value spine: a `Guard` statement
/// of a block, a block's tail, or an arm of an `if` / `match`?
pub fn spine_exits(e: &IrExpr) -> bool {
    match &e.kind {
        IrExprKind::Block { stmts, expr } => {
            stmts.iter().any(|s| matches!(s.kind, IrStmtKind::Guard { .. })) || expr.as_deref().is_some_and(spine_exits)
        }
        IrExprKind::If { then, else_, .. } => spine_exits(then) || spine_exits(else_),
        IrExprKind::Match { arms, .. } => arms.iter().any(|a| spine_exits(&a.body)),
        _ => false,
    }
}

/// Is `e` the return channel the front end gives a `guard let` in an operand:
/// a block `{ …; guard false else E; <a Never value> }`?
pub fn is_value_exit(e: &IrExpr) -> bool {
    let IrExprKind::Block { stmts, expr: Some(tail) } = &e.kind else { return false };
    let exit = |s: &IrStmt| matches!(&s.kind, IrStmtKind::Guard { cond, .. } if matches!(cond.kind, IrExprKind::LitBool { value: false }));
    tail.ty == Ty::Never && stmts.last().is_some_and(exit)
}

/// Does that return channel stand on `e`'s value spine?
fn spine_value_exit(e: &IrExpr) -> bool {
    match &e.kind {
        _ if is_value_exit(e) => true,
        IrExprKind::Block { expr: Some(t), .. } => spine_value_exit(t),
        IrExprKind::If { then, else_, .. } => spine_value_exit(then) || spine_value_exit(else_),
        IrExprKind::Match { arms, .. } => arms.iter().any(|a| spine_value_exit(&a.body)),
        _ => false,
    }
}

/// The operands of `e` that are evaluated unconditionally, in evaluation
/// order. A kind not listed has none walked.
fn strict_operands(e: &mut IrExpr) -> Vec<&mut IrExpr> {
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
pub fn flatten_bind(var: VarId, ty: Ty, value: IrExpr, out: &mut Vec<IrStmt>) {
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

/// Hoist into `out` the first value among `e`'s unconditional operands (or
/// deeper in them) that `exits` names, the operands before it bound first.
/// `true` when one was hoisted.
pub fn hoist_in(e: &mut IrExpr, out: &mut Vec<IrStmt>, vt: &mut VarTable, exits: fn(&IrExpr) -> bool) -> bool {
    let mut ops = strict_operands(e);
    let Some(at) = ops.iter().position(|o| exits(o) || contains_nested(o, exits)) else { return false };
    for o in ops.iter_mut().take(at) {
        if !is_stable(o, vt) {
            bind_out(o, out, vt);
        }
    }
    let target = &mut *ops[at];
    if exits(target) {
        bind_out(target, out, vt);
        return true;
    }
    hoist_in(target, out, vt, exits)
}

/// Does a value `exits` names stand among `e`'s unconditional operands, at
/// any depth?
fn contains_nested(e: &IrExpr, exits: fn(&IrExpr) -> bool) -> bool {
    let mut copy = e.clone();
    strict_operands(&mut copy).iter().any(|o| exits(o) || contains_nested(o, exits))
}

/// The value expression of a statement the hoist may rewrite.
pub fn stmt_value(s: &mut IrStmt) -> Option<&mut IrExpr> {
    match &mut s.kind {
        IrStmtKind::Bind { value, .. }
        | IrStmtKind::BindDestructure { value, .. }
        | IrStmtKind::Assign { value, .. }
        | IrStmtKind::Expr { expr: value } => Some(value),
        _ => None,
    }
}

/// Does the return channel stand anywhere in `body`?
pub fn holds_value_exit(body: &IrExpr) -> bool {
    struct Scan(bool);
    impl crate::visit::IrVisitor for Scan {
        fn visit_expr(&mut self, e: &IrExpr) {
            self.0 |= is_value_exit(e);
            if !self.0 {
                crate::visit::walk_expr(self, e);
            }
        }
    }
    let mut scan = Scan(false);
    crate::visit::IrVisitor::visit_expr(&mut scan, body);
    scan.0
}

/// Hoist every return channel out of the operands of a block's statements and
/// tail, until none is left.
fn hoist_block(stmts: &mut Vec<IrStmt>, tail: &mut Option<Box<IrExpr>>, vt: &mut VarTable) {
    let mut i = 0;
    while i < stmts.len() {
        let mut out = Vec::new();
        if stmt_value(&mut stmts[i]).is_some_and(|v| hoist_in(v, &mut out, vt, spine_value_exit)) {
            let n = out.len();
            stmts.splice(i..i, out);
            i += n;
            continue;
        }
        i += 1;
    }
    if let Some(t) = tail.as_deref_mut() {
        let mut out = Vec::new();
        while hoist_in(t, &mut out, vt, spine_value_exit) {}
        stmts.extend(out);
    }
}

struct Hoist<'a>(&'a mut VarTable);

impl IrMutVisitor for Hoist<'_> {
    fn visit_expr_mut(&mut self, e: &mut IrExpr) {
        walk_expr_mut(self, e);
        if let IrExprKind::Block { stmts, expr } = &mut e.kind {
            hoist_block(stmts, expr, self.0);
        }
    }
}

/// Run the hoist over every fn of the program that holds a return channel (a
/// body that is not a block becomes the tail of one). Returns the number of
/// fns rewritten.
pub fn hoist_value_exits(program: &mut IrProgram) -> usize {
    let IrProgram { functions, modules, var_table, .. } = program;
    let mut n = 0;
    for f in functions.iter_mut().chain(modules.iter_mut().flat_map(|m| m.functions.iter_mut())) {
        if !holds_value_exit(&f.body) {
            continue;
        }
        if !matches!(f.body.kind, IrExprKind::Block { .. }) {
            let (ty, span) = (f.body.ty.clone(), f.body.span);
            let body = std::mem::replace(&mut f.body, IrExpr { kind: IrExprKind::Unit, ty: ty.clone(), span, def_id: None });
            f.body = IrExpr { kind: IrExprKind::Block { stmts: Vec::new(), expr: Some(Box::new(body)) }, ty, span, def_id: None };
        }
        Hoist(var_table).visit_expr_mut(&mut f.body);
        n += 1;
    }
    n
}
