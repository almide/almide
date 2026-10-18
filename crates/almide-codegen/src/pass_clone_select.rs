//! Read-only selections borrow (#3532).
//!
//! Problem: `if c then x else x2` over two live heap locals is an owned
//! value, so the clone walk copies the chosen one (`x.clone()`) every time
//! it runs — O(len) per evaluation inside a loop, where wasm shares the
//! block. When the selection is only read, a reference to the chosen local
//! is the same value: `if c { &x } else { &x2 }`.
//!
//! Approach: the rule the head reads use (`pass_clone_head`): rewrite the
//! selection only where every use reads it through a site the walker spells
//! identically for a `&T`, and the locals it chooses between stay put
//! (no move, write or capture) for as long as the reference lives.
//!
//! - `let y = if c then x else x2; rest` → `let y: _ = if c { &x } else { &x2 }`
//!   when `rest` only reads `y` and leaves `x` and `x2` in place;
//! - `f(&(if c then x else x2))` (a borrowed argument) → `f(if c { &x } else { &x2 })`
//!   when no sibling argument names `x` or `x2`.
//!
//! Nested `if`s whose every arm is a variable select too. The chosen
//! variables must be plain `let` locals of the fn that render as values, so
//! every arm's `&` has the same type: not a param (it may already be a
//! reference), a loop or pattern binder (the clone walk may bind it `&T`), a
//! binder another pass already bound by reference (`by_ref`: head reads,
//! TCO temporaries, an earlier selection), or a top-level let (a global the
//! walker spells differently).
//!
//! Compatibility: none observable — the same value is read on every path;
//! only the native copy goes. Wasm does not run this pass.
use std::collections::HashSet;
use almide_ir::*;
use almide_ir::visit_mut::{walk_expr_mut, IrMutVisitor};
use almide_lang::types::Ty;
use super::pass_clone_projection::{mentions, reads_binding};

/// Rewrite every read-only selection of `func`'s body; returns the `let`
/// binders now bound by reference. `by_ref` holds the binders already bound
/// by reference.
pub(super) fn rewrite(func: &mut IrFunction, vt: &VarTable, top_lets: &HashSet<VarId>, by_ref: &HashSet<VarId>) -> HashSet<VarId> {
    let lets = plain_lets(&func.body).into_iter().filter(|v| !top_lets.contains(v) && !by_ref.contains(v)).collect();
    let mut sel = Selections { vt, lets, ref_lets: HashSet::new() };
    sel.visit_expr_mut(&mut func.body);
    sel.ref_lets
}

struct Selections<'a> {
    vt: &'a VarTable,
    /// The plain `let` binders a selection may choose between.
    lets: HashSet<VarId>,
    ref_lets: HashSet<VarId>,
}

/// Every variable a plain `let` statement binds in `body`.
fn plain_lets(body: &IrExpr) -> HashSet<VarId> {
    struct Lets(HashSet<VarId>);
    impl almide_ir::visit::IrVisitor for Lets {
        fn visit_stmt(&mut self, s: &IrStmt) {
            if let IrStmtKind::Bind { var, mutability: Mutability::Let, .. } = &s.kind {
                self.0.insert(*var);
            }
            almide_ir::visit::walk_stmt(self, s);
        }
    }
    let mut lets = Lets(HashSet::new());
    almide_ir::visit::IrVisitor::visit_expr(&mut lets, body);
    lets.0
}

/// The variables the selection `e` chooses between, when every arm (through
/// nested `if`s) is a variable read.
fn selected(e: &IrExpr) -> Option<Vec<VarId>> {
    match &e.kind {
        IrExprKind::Var { id } => Some(vec![*id]),
        IrExprKind::If { then, else_, .. } => {
            let mut vars = selected(then)?;
            vars.extend(selected(else_)?);
            Some(vars)
        }
        _ => None,
    }
}

/// The selection with each arm's variable read replaced by a shared borrow.
fn borrow_arms(e: IrExpr, as_str: bool) -> IrExpr {
    let IrExpr { kind, ty, span, def_id } = e;
    let kind = match kind {
        IrExprKind::If { cond, then, else_ } => IrExprKind::If {
            cond,
            then: Box::new(borrow_arms(*then, as_str)),
            else_: Box::new(borrow_arms(*else_, as_str)),
        },
        var => IrExprKind::Borrow { expr: Box::new(IrExpr { kind: var, ty: ty.clone(), span, def_id }), as_str, mutable: false },
    };
    IrExpr { kind, ty, span, def_id: None }
}

/// A selection's type worth borrowing: a heap value the clone walk copies.
fn heap_value(ty: &Ty) -> bool {
    super::pass_clone::needs_clone(ty)
}

impl Selections<'_> {
    /// The variables of a borrowable selection: an `if` over plain `let`
    /// locals of this fn that are still bound by value.
    fn locals(&self, e: &IrExpr) -> Option<Vec<VarId>> {
        if !matches!(e.kind, IrExprKind::If { .. }) || !heap_value(&e.ty) {
            return None;
        }
        let vars = selected(e)?;
        vars.iter()
            .all(|v| self.lets.contains(v) && !self.ref_lets.contains(v) && self.vt.get(*v).mutability == Mutability::Let)
            .then_some(vars)
    }

    /// A borrowed call argument that is a selection: its arms borrow, so
    /// the argument is the chosen reference. No sibling argument may name a
    /// chosen local: the reference lives through the call.
    fn borrow_args(&self, e: &mut IrExpr) {
        let (IrExprKind::Call { args, .. } | IrExprKind::RuntimeCall { args, .. }) = &mut e.kind else { return };
        for i in 0..args.len() {
            let IrExprKind::Borrow { expr: sel, as_str, mutable: false } = &args[i].kind else { continue };
            let Some(vars) = self.locals(sel) else { continue };
            if args.iter().enumerate().any(|(j, a)| j != i && vars.iter().any(|&v| mentions(a, v))) {
                continue;
            }
            let as_str = *as_str;
            let IrExprKind::Borrow { expr: sel, .. } = std::mem::replace(&mut args[i].kind, IrExprKind::Unit) else { unreachable!() };
            let ty = args[i].ty.clone();
            args[i] = IrExpr { ty, ..borrow_arms(*sel, as_str) };
        }
    }

    /// `let h = <selection>` whose binder the rest of the block only reads,
    /// over locals the rest leaves in place: bind `h` by reference.
    fn bind_ref(&mut self, stmts: &mut [IrStmt], tail: Option<&IrExpr>) {
        for i in 0..stmts.len() {
            let IrStmtKind::Bind { var, mutability: Mutability::Let, value, .. } = &stmts[i].kind else { continue };
            let h = *var;
            let Some(vars) = self.locals(value) else { continue };
            if vars.contains(&h) {
                continue;
            }
            let rest = IrExpr {
                kind: IrExprKind::Block { stmts: stmts[i + 1..].to_vec(), expr: tail.map(|t| Box::new(t.clone())) },
                ty: Ty::Unit,
                span: None,
                def_id: None,
            };
            if !mentions(&rest, h) || !reads_binding(&rest, h) || !vars.iter().all(|&v| super::pass_clone_head::stays_put(&rest, v)) {
                continue;
            }
            let IrStmtKind::Bind { value, .. } = &mut stmts[i].kind else { unreachable!() };
            let sel = std::mem::replace(value, IrExpr { kind: IrExprKind::Unit, ty: Ty::Unit, span: None, def_id: None });
            *value = borrow_arms(sel, false);
            self.ref_lets.insert(h);
        }
    }
}

impl IrMutVisitor for Selections<'_> {
    fn visit_expr_mut(&mut self, e: &mut IrExpr) {
        self.borrow_args(e);
        walk_expr_mut(self, e);
        match &mut e.kind {
            IrExprKind::Block { stmts, expr } => self.bind_ref(stmts, expr.as_deref()),
            IrExprKind::ForIn { body, .. } | IrExprKind::While { body, .. } => self.bind_ref(body, None),
            _ => {}
        }
    }
}
