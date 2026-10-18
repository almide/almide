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
use almide_ir::visit::IrVisitor;
use almide_ir::visit_mut::{walk_expr_mut, IrMutVisitor};
use almide_lang::types::Ty;
use super::pass_clone_projection::{mentions, reads_binding};

/// Rewrite every read-only selection of `func`'s body; returns the `let`
/// binders now bound by reference. `by_ref` holds the binders already bound
/// by reference.
///
/// Every arm must spell its binder the same way (`&x` of a value), so a
/// binder that any selection names in an arm is never itself bound by
/// reference — otherwise its arm would be `&y` over a `y` that is already a
/// `&T`, and which one the walk met first would decide the arm types. With
/// that set fixed up front the order is free of such races: the argument
/// selections are rewritten first (their arms now read as borrows), then the
/// `let`s that bind by reference are decided over that body, then rewritten.
pub(super) fn rewrite(func: &mut IrFunction, vt: &VarTable, top_lets: &HashSet<VarId>, by_ref: &HashSet<VarId>) -> HashSet<VarId> {
    let lets = plain_lets(&func.body).into_iter().filter(|v| !top_lets.contains(v) && !by_ref.contains(v)).collect();
    let sel = Selections { vt, lets, arms: arm_vars(&func.body), ref_lets: HashSet::new() };
    Args(&sel).visit_expr_mut(&mut func.body);
    let mut decide = Decide { sel };
    decide.visit_expr(&func.body);
    let mut sel = decide.sel;
    sel.visit_expr_mut(&mut func.body);
    sel.ref_lets
}

/// Every variable a selection names in an arm, anywhere in `body`.
fn arm_vars(body: &IrExpr) -> HashSet<VarId> {
    struct Arms(HashSet<VarId>);
    impl IrVisitor for Arms {
        fn visit_expr(&mut self, e: &IrExpr) {
            if matches!(e.kind, IrExprKind::If { .. }) && heap_value(&e.ty) {
                self.0.extend(selected(e).into_iter().flatten());
            }
            almide_ir::visit::walk_expr(self, e);
        }
    }
    let mut arms = Arms(HashSet::new());
    arms.visit_expr(body);
    arms.0
}

struct Selections<'a> {
    vt: &'a VarTable,
    /// The plain `let` binders a selection may choose between.
    lets: HashSet<VarId>,
    /// The binders some selection names in an arm: never bound by reference.
    arms: HashSet<VarId>,
    /// The `let` binders bound by reference.
    ref_lets: HashSet<VarId>,
}

/// Every variable a plain `let` statement binds in `body`.
fn plain_lets(body: &IrExpr) -> HashSet<VarId> {
    struct Lets(HashSet<VarId>);
    impl IrVisitor for Lets {
        fn visit_stmt(&mut self, s: &IrStmt) {
            if let IrStmtKind::Bind { var, mutability: Mutability::Let, .. } = &s.kind {
                self.0.insert(*var);
            }
            almide_ir::visit::walk_stmt(self, s);
        }
    }
    let mut lets = Lets(HashSet::new());
    lets.visit_expr(body);
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

/// The block that follows `stmts[i]`, ending in `tail`.
fn rest_after(stmts: &[IrStmt], i: usize, tail: Option<&IrExpr>) -> IrExpr {
    IrExpr {
        kind: IrExprKind::Block { stmts: stmts[i + 1..].to_vec(), expr: tail.map(|t| Box::new(t.clone())) },
        ty: Ty::Unit,
        span: None,
        def_id: None,
    }
}

impl Selections<'_> {
    /// The variables of a borrowable selection: an `if` over plain `let`
    /// locals of this fn.
    fn locals(&self, e: &IrExpr) -> Option<Vec<VarId>> {
        if !matches!(e.kind, IrExprKind::If { .. }) || !heap_value(&e.ty) {
            return None;
        }
        let vars = selected(e)?;
        vars.iter()
            .all(|v| self.lets.contains(v) && self.vt.get(*v).mutability == Mutability::Let)
            .then_some(vars)
    }

    /// Does `let h = <stmts[i]'s selection>` qualify: the rest of the block
    /// only reads `h` and leaves the chosen locals in place?
    fn binds_by_ref(&self, stmts: &[IrStmt], i: usize, tail: Option<&IrExpr>) -> Option<VarId> {
        let IrStmtKind::Bind { var, mutability: Mutability::Let, value, .. } = &stmts[i].kind else { return None };
        let vars = self.locals(value)?;
        if vars.contains(var) || self.arms.contains(var) {
            return None;
        }
        let rest = rest_after(stmts, i, tail);
        (mentions(&rest, *var) && reads_binding(&rest, *var) && vars.iter().all(|&v| super::pass_clone_head::stays_put(&rest, v)))
            .then_some(*var)
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

    /// Rewrite the decided `let`s of a block to bind by reference.
    fn bind_ref(&self, stmts: &mut [IrStmt]) {
        for stmt in stmts.iter_mut() {
            let IrStmtKind::Bind { var, value, .. } = &mut stmt.kind else { continue };
            if !self.ref_lets.contains(var) {
                continue;
            }
            let sel = std::mem::replace(value, IrExpr { kind: IrExprKind::Unit, ty: Ty::Unit, span: None, def_id: None });
            *value = borrow_arms(sel, false);
        }
    }
}

/// The first pass: the borrowed call arguments that are selections.
struct Args<'s, 'a>(&'s Selections<'a>);

impl IrMutVisitor for Args<'_, '_> {
    fn visit_expr_mut(&mut self, e: &mut IrExpr) {
        self.0.borrow_args(e);
        walk_expr_mut(self, e);
    }
}

/// The second pass: which `let` selections bind by reference, judged on the
/// body with its argument selections already borrowing.
struct Decide<'a> {
    sel: Selections<'a>,
}

impl Decide<'_> {
    fn block(&mut self, stmts: &[IrStmt], tail: Option<&IrExpr>) {
        for i in 0..stmts.len() {
            if let Some(h) = self.sel.binds_by_ref(stmts, i, tail) {
                self.sel.ref_lets.insert(h);
            }
        }
    }
}

impl IrVisitor for Decide<'_> {
    fn visit_expr(&mut self, e: &IrExpr) {
        match &e.kind {
            IrExprKind::Block { stmts, expr } => self.block(stmts, expr.as_deref()),
            IrExprKind::ForIn { body, .. } | IrExprKind::While { body, .. } => self.block(body, None),
            _ => {}
        }
        almide_ir::visit::walk_expr(self, e);
    }
}

/// The third pass: the decided `let`s bind by reference.
impl IrMutVisitor for Selections<'_> {
    fn visit_expr_mut(&mut self, e: &mut IrExpr) {
        walk_expr_mut(self, e);
        match &mut e.kind {
            IrExprKind::Block { stmts, .. } => self.bind_ref(stmts),
            IrExprKind::ForIn { body, .. } | IrExprKind::While { body, .. } => self.bind_ref(body),
            _ => {}
        }
    }
}
