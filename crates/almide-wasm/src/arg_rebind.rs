//! #3815 — an operand that reads a variable a LATER operand rebinds.
//!
//! Operands are values evaluated left to right, once (ALS-E26, C-258), so in
//! `list.contains(xs, { xs = ["z"]; "a" })` the first argument is the list
//! `xs` held before the block ran. The emitter lowers a plain read — a
//! variable, or a field / position of one — as a view of the variable's
//! block, with no credit of its own: it pushes the address, the later
//! operand's assignment releases that block, and the call reads freed
//! memory (`false`, `none`, garbage, or a trap). The fold arms already
//! evaluate their source first (#2045); every other operand sequence did
//! not.
//!
//! The fix is at the IR, before the argument-temporary binder: in every
//! operand sequence — a call's arguments, a binary op, a list / tuple /
//! record / map literal, an interpolation, an index — a plain read whose
//! root variable a later operand assigns is bound first,
//! `{ let t = xs; list.contains(t, { xs = …; "a" }) }`. The Bind route takes
//! the read's `+1`, so the value survives the rebind and the frame's exit
//! plan releases it. Order is kept: every earlier operand that is not a
//! plain read, a literal or a lambda is bound with it, in source order
//! (the #3139 rule), so no observable operand changes places with the read.
//! The rule names no function: it is the operand sequence's shape alone.

use almide_base::intern::sym;
use almide_ir::visit_mut::{walk_expr_mut, IrMutVisitor};
use almide_ir::{CallTarget, IrExpr, IrExprKind, IrProgram, IrStmt, IrStmtKind, IrStringPart, Mutability, VarId, VarTable};
use std::collections::HashSet;

/// Rewrite every function body and top-level initializer; `None` when no
/// operand sequence reads a variable a later operand rebinds.
pub(crate) fn bind_reads_before_rebinds(ir: &IrProgram) -> Option<IrProgram> {
    if !any_assignment(ir) {
        return None;
    }
    let mut out = ir.clone();
    let mut changed = false;
    {
        let mut v = Orderer { vars: &mut out.var_table, changed: &mut changed };
        for f in out.functions.iter_mut() {
            v.visit_expr_mut(&mut f.body);
        }
        for tl in out.top_lets.iter_mut() {
            v.visit_expr_mut(&mut tl.value);
        }
    }
    for m in out.modules.iter_mut() {
        let mut v = Orderer { vars: &mut m.var_table, changed: &mut changed };
        for f in m.functions.iter_mut() {
            v.visit_expr_mut(&mut f.body);
        }
        for tl in m.top_lets.iter_mut() {
            v.visit_expr_mut(&mut tl.value);
        }
    }
    changed.then_some(out)
}

/// A program with no assignment at all has nothing to reorder; skip the clone.
fn any_assignment(ir: &IrProgram) -> bool {
    let mut seen = HashSet::new();
    let bodies = ir.functions.iter().map(|f| &f.body).chain(ir.top_lets.iter().map(|t| &t.value)).chain(
        ir.modules.iter().flat_map(|m| m.functions.iter().map(|f| &f.body).chain(m.top_lets.iter().map(|t| &t.value))),
    );
    for body in bodies {
        almide_ir::collect_assigned_vars(body, &mut seen);
        if !seen.is_empty() {
            return true;
        }
    }
    false
}

/// The variable a plain read views — `x`, `x.f`, `x.0.f` — or `None`.
fn plain_read_root(e: &IrExpr) -> Option<VarId> {
    match &e.kind {
        IrExprKind::Var { id } => Some(*id),
        IrExprKind::Member { object, .. } | IrExprKind::TupleIndex { object, .. } => plain_read_root(object),
        _ => None,
    }
}

/// An operand that may stay in place while operands before it are bound:
/// evaluating it has no effect and nothing can change what it reads.
fn stays_in_place(e: &IrExpr) -> bool {
    plain_read_root(e).is_some()
        || matches!(
            e.kind,
            IrExprKind::LitInt { .. }
                | IrExprKind::LitFloat { .. }
                | IrExprKind::LitBool { .. }
                | IrExprKind::LitStr { .. }
                | IrExprKind::Unit
                | IrExprKind::OptionNone
                | IrExprKind::EmptyMap
                | IrExprKind::FnRef { .. }
                | IrExprKind::Lambda { .. }
                | IrExprKind::ClosureCreate { .. }
        )
}

/// The variables an operand assigns when it is evaluated. A lambda is not
/// run by being passed, so it assigns nothing here.
fn assigned_by(e: &IrExpr) -> HashSet<u32> {
    let mut out = HashSet::new();
    if !matches!(e.kind, IrExprKind::Lambda { .. }) {
        almide_ir::collect_assigned_vars(e, &mut out);
    }
    out
}

/// The operands of `e`, in evaluation order, when `e` evaluates them all
/// left to right before it acts. `None` for any other node.
fn operands(e: &mut IrExpr) -> Option<Vec<&mut IrExpr>> {
    let ops: Vec<&mut IrExpr> = match &mut e.kind {
        IrExprKind::Call { target, args, .. } | IrExprKind::TailCall { target, args } => {
            let head: Option<&mut IrExpr> = match target {
                CallTarget::Computed { callee } => Some(callee.as_mut()),
                CallTarget::Method { object, .. } => Some(object.as_mut()),
                CallTarget::Named { .. } | CallTarget::Module { .. } => None,
            };
            head.into_iter().chain(args.iter_mut()).collect()
        }
        IrExprKind::RuntimeCall { args, .. } => args.iter_mut().collect(),
        // `and` / `or` evaluate the left operand first as well; the right
        // one may not run, which changes nothing about binding the left.
        IrExprKind::BinOp { left, right, .. } => vec![left.as_mut(), right.as_mut()],
        IrExprKind::List { elements } | IrExprKind::Tuple { elements } => elements.iter_mut().collect(),
        IrExprKind::Record { fields, .. } => fields.iter_mut().map(|(_, v)| v).collect(),
        IrExprKind::SpreadRecord { base, fields } => {
            std::iter::once(base.as_mut()).chain(fields.iter_mut().map(|(_, v)| v)).collect()
        }
        IrExprKind::MapLiteral { entries } => entries.iter_mut().flat_map(|(k, v)| [k, v]).collect(),
        IrExprKind::StringInterp { parts } => parts
            .iter_mut()
            .filter_map(|p| match p {
                IrStringPart::Expr { expr } => Some(expr),
                IrStringPart::Lit { .. } => None,
            })
            .collect(),
        IrExprKind::IndexAccess { object, index } => vec![object.as_mut(), index.as_mut()],
        IrExprKind::MapAccess { object, key } => vec![object.as_mut(), key.as_mut()],
        IrExprKind::Range { start, end, .. } => vec![start.as_mut(), end.as_mut()],
        _ => return None,
    };
    Some(ops)
}

struct Orderer<'a> {
    vars: &'a mut VarTable,
    changed: &'a mut bool,
}

impl IrMutVisitor for Orderer<'_> {
    fn visit_expr_mut(&mut self, e: &mut IrExpr) {
        walk_expr_mut(self, e);
        let Some(ops) = operands(e) else { return };
        if ops.len() < 2 {
            return;
        }
        // The variables each operand's SUCCESSORS assign.
        let mut later: Vec<HashSet<u32>> = vec![HashSet::new(); ops.len()];
        for i in (0..ops.len() - 1).rev() {
            let mut s = later[i + 1].clone();
            s.extend(assigned_by(ops[i + 1]));
            later[i] = s;
        }
        let rebound: Vec<bool> = ops
            .iter()
            .zip(&later)
            .map(|(a, l)| plain_read_root(a).is_some_and(|v| l.contains(&v.0)))
            .collect();
        let Some(last) = rebound.iter().rposition(|r| *r) else { return };
        let mut binds = Vec::new();
        for (i, a) in ops.into_iter().enumerate().take(last + 1) {
            if !(rebound[i] || !stays_in_place(a)) {
                continue;
            }
            let (ty, span) = (a.ty.clone(), a.span);
            let id = self.vars.alloc(sym("__arg_before_rebind"), ty.clone(), Mutability::Let, span);
            let value = std::mem::replace(a, IrExpr { kind: IrExprKind::Var { id }, ty: ty.clone(), span, def_id: None });
            binds.push(IrStmt { kind: IrStmtKind::Bind { var: id, mutability: Mutability::Let, ty, value }, span });
        }
        *self.changed = true;
        let (ty, span) = (e.ty.clone(), e.span);
        let node = std::mem::take(e);
        *e = IrExpr { kind: IrExprKind::Block { stmts: binds, expr: Some(Box::new(node)) }, ty, span, def_id: None };
    }
}
