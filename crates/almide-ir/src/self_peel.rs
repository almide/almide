//! One level of self-inlining for the accumulator loop (the wasm legs).
//!
//! `accum_tre` leaves one self-call per loop round:
//! ```text
//! fn fib(n) = { var m = n; var acc = 0
//!   while m >= 2 { acc = acc + fib(m - 1); m = m - 2 }
//!   acc + m }
//! ```
//! This replaces that call, ONCE, by a copy of the rewritten body with the
//! argument bound to a fresh `let`; the call inside the copy stays a call.
//! Half the calls of the loop form remain (a round's `fib(m - 1)` is now a
//! loop of its own, calling only for its odd steps), and a wasm engine that
//! does not inline recursive edges keeps the frame setup it saves.
//!
//! It is beta-reduction with the argument bound first — evaluated once,
//! before the copied body, exactly as the call evaluated it — over a body
//! `accum_tre` already proved pure, total and Int-only, so no value, effect,
//! allocation or trap moves. Recursion depth only shrinks. The copy costs
//! the body's bytes once per rewritten function.
use std::collections::HashMap;

use crate::visit::{walk_expr, IrVisitor};
use crate::visit_mut::{walk_expr_mut, walk_stmt_mut, IrMutVisitor};
use crate::*;

fn is_self_call(e: &IrExpr, name: almide_base::intern::Sym) -> bool {
    matches!(&e.kind, IrExprKind::Call { target: CallTarget::Named { name: n }, .. } if *n == name)
}

struct Shape {
    name: almide_base::intern::Sym,
    self_calls: usize,
    plain: bool,
}

impl IrVisitor for Shape {
    fn visit_expr(&mut self, e: &IrExpr) {
        if is_self_call(e, self.name) {
            self.self_calls += 1;
        }
        if matches!(e.kind, IrExprKind::Lambda { .. } | IrExprKind::ForIn { .. } | IrExprKind::Match { .. }) {
            self.plain = false;
        }
        walk_expr(self, e);
    }
    fn visit_stmt(&mut self, s: &IrStmt) {
        if !matches!(s.kind, IrStmtKind::Bind { .. } | IrStmtKind::Assign { .. } | IrStmtKind::Expr { .. }) {
            self.plain = false;
        }
        crate::visit::walk_stmt(self, s);
    }
}

/// Fresh ids for everything the copy binds (`let`/`var`), parameters seeded.
struct Rename<'v> {
    vt: &'v mut VarTable,
    map: HashMap<VarId, VarId>,
}

impl Rename<'_> {
    fn fresh(&mut self, id: VarId) -> VarId {
        let info = self.vt.get(id);
        let (ty, m) = (info.ty.clone(), info.mutability);
        let fresh = self.vt.alloc_like(id, ty, m);
        self.map.insert(id, fresh);
        fresh
    }
}

impl IrMutVisitor for Rename<'_> {
    fn visit_expr_mut(&mut self, e: &mut IrExpr) {
        if let IrExprKind::Var { id } = &mut e.kind {
            if let Some(n) = self.map.get(id) {
                *id = *n;
            }
        }
        walk_expr_mut(self, e);
    }
    fn visit_stmt_mut(&mut self, s: &mut IrStmt) {
        // The value first: it cannot read the variable it binds.
        walk_stmt_mut(self, s);
        match &mut s.kind {
            IrStmtKind::Bind { var, .. } => *var = self.fresh(*var),
            IrStmtKind::Assign { var, .. } => {
                if let Some(n) = self.map.get(var) {
                    *var = *n;
                }
            }
            _ => {}
        }
    }
}

struct Peel<'a> {
    name: almide_base::intern::Sym,
    params: &'a [IrParam],
    body: &'a IrExpr,
    vt: &'a mut VarTable,
    done: bool,
}

impl IrMutVisitor for Peel<'_> {
    fn visit_expr_mut(&mut self, e: &mut IrExpr) {
        if self.done {
            return;
        }
        if !is_self_call(e, self.name) {
            walk_expr_mut(self, e);
            return;
        }
        let IrExprKind::Call { args, .. } = std::mem::replace(&mut e.kind, IrExprKind::Unit) else { unreachable!() };
        let mut rn = Rename { vt: self.vt, map: HashMap::new() };
        let mut stmts = Vec::with_capacity(args.len());
        for (p, arg) in self.params.iter().zip(args) {
            let var = rn.fresh(p.var);
            stmts.push(IrStmt {
                kind: IrStmtKind::Bind { var, mutability: Mutability::Let, ty: p.ty.clone(), value: arg },
                span: e.span,
            });
        }
        let mut copy = self.body.clone();
        rn.visit_expr_mut(&mut copy);
        e.kind = IrExprKind::Block { stmts, expr: Some(Box::new(copy)) };
        self.done = true;
    }
}

/// Inline the one self-call of an accumulator-loop body once. `func` must be
/// a function `accum_tre::rewrite` just rewrote. Returns whether it did.
pub fn peel(func: &mut IrFunction, vars: &mut VarTable) -> bool {
    let mut shape = Shape { name: func.name, self_calls: 0, plain: true };
    shape.visit_expr(&func.body);
    if shape.self_calls != 1 || !shape.plain || func.params.iter().any(|p| p.is_mut) {
        return false;
    }
    let original = func.body.clone();
    let mut peel = Peel { name: func.name, params: &func.params, body: &original, vt: vars, done: false };
    peel.visit_expr_mut(&mut func.body);
    peel.done
}
