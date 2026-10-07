//! #3170: a `mut` parameter overwritten from a call over its own old value.
//!
//! `ws = list.filter(ws, f)` on `mut ws: List[T]` reads the param — a
//! `&mut Vec<T>` — as the owned value the call consumes. The generic rule
//! ([`Lower::own_consumed_ref_mut`]) clones it. But the assign overwrites the
//! param, so the old value is dead the moment the write lands: the consuming
//! read MOVES it out instead (`std::mem::take(ws)`), when nothing can observe
//! the empty value the take leaves behind until then.
//!
//! #3454 extends the field form to a plain local: `b.text = b.text + "ab"` on
//! a `var b` copied the whole accumulated field on every step (quadratic in a
//! loop); it now takes it, under the same soundness scan.
use super::*;
use almide_ir::visit::{walk_expr, walk_stmt, IrVisitor};

impl Lower<'_> {
    /// Spell the consuming read of an overwritten `mut` param as a take,
    /// when that is provably invisible ([`take_is_sound`]).
    pub(super) fn take_overwritten_ref_mut(&self, stmt: &mut IrStmt) {
        // `ws = f(ws)` takes the param; `b.xs = f(b.xs)` takes the one field
        // of a `mut` record param (`std::mem::take(&mut b.xs)`).
        let (id, field, value) = match &mut stmt.kind {
            IrStmtKind::Assign { var, value } => (*var, None, value),
            IrStmtKind::FieldAssign { target, field, value } => (*target, Some(*field), value),
            _ => return,
        };
        let place_ok = match self.params.iter().find(|p| p.var == id) {
            Some(param) => param.borrow == ParamBorrow::RefMut,
            None => field.is_some() && self.is_plain_local(id),
        };
        if !place_ok || !takeable(&value.ty) || !take_is_sound(value, id) {
            return;
        }
        TakeAt { id, field, done: false }.visit_expr_mut(value);
    }

    /// #3454: `b.text = b.text + "ab"` on a function-local `var b` held as a
    /// plain `let mut` — no global, no shared cell, no copy-on-write `Rc` (a
    /// closure captures it), no clone-always class. Its field is a place
    /// `&mut b.text` reaches directly, so the one read the overwrite kills
    /// takes it the way a `mut` param's does. (A plain `s = s + …` on a local
    /// is the clone pass's move, #3404.)
    fn is_plain_local(&self, id: VarId) -> bool {
        self.ann.global(id).is_none() && !self.ann.is_shared_mut(&id)
            && !matches!(self.ann.get_var_storage(&id), almide_ir::annotations::VarStorage::RcCow)
            && !self.ann.always_clone_vars.contains(&id)
    }
}

/// A type whose Rust form is `Default` for every element type, so
/// `std::mem::take` applies: `Vec<T>` and `String`. A map or set keeps the
/// clone (its derived `Default` bounds the element types).
fn takeable(ty: &Ty) -> bool {
    matches!(ty, Ty::String | Ty::Bytes | Ty::Applied(TypeConstructorId::List, _))
}

/// The take leaves the param empty until the assign writes it, so the RHS
/// must name the param exactly ONCE (no other read sees the hole), outside
/// any closure, loop or fan arm (the read runs once), and have no early exit
/// — a `!`/`?` propagation, an `err` return, a `guard`, a `break`/`continue` — that
/// could leave the fn or the loop with the hole still in place, where the
/// caller (C-132) or the next iteration would read it.
fn take_is_sound(value: &IrExpr, id: VarId) -> bool {
    let mut scan = Scan { id, reads: 0, depth: 0, bad: false };
    scan.visit_expr(value);
    scan.reads == 1 && !scan.bad
}

struct Scan {
    id: VarId,
    reads: usize,
    depth: usize,
    bad: bool,
}

impl IrVisitor for Scan {
    fn visit_expr(&mut self, e: &IrExpr) {
        let (exits, nests) = match &e.kind {
            IrExprKind::Var { id } if *id == self.id => {
                self.reads += 1;
                (self.depth > 0, false)
            }
            IrExprKind::Try { .. } | IrExprKind::Unwrap { .. } | IrExprKind::ResultErr { .. }
            | IrExprKind::Break | IrExprKind::Continue => (true, false),
            // A pre-rendered template may carry its own `?` or `return`.
            IrExprKind::InlineRust { template: code, .. } | IrExprKind::RenderedCall { code } =>
                (code.contains('?') || code.contains("return"), false),
            IrExprKind::Lambda { .. } | IrExprKind::ForIn { .. }
            | IrExprKind::While { .. } | IrExprKind::Fan { .. } => (false, true),
            _ => (false, false),
        };
        self.bad |= exits;
        self.depth += usize::from(nests);
        walk_expr(self, e);
        self.depth -= usize::from(nests);
    }

    fn visit_stmt(&mut self, s: &IrStmt) {
        let target = match &s.kind {
            IrStmtKind::Assign { var: t, .. } | IrStmtKind::IndexAssign { target: t, .. }
            | IrStmtKind::MapInsert { target: t, .. } | IrStmtKind::FieldAssign { target: t, .. }
            | IrStmtKind::ListSwap { target: t, .. } | IrStmtKind::ListReverse { target: t, .. }
            | IrStmtKind::ListRotateLeft { target: t, .. } => Some(*t),
            IrStmtKind::ListCopySlice { dst, .. } => Some(*dst),
            _ => None,
        };
        // A `guard` leaves the fn (or the loop) from inside the RHS.
        self.bad |= target == Some(self.id) || matches!(s.kind, IrStmtKind::Guard { .. });
        walk_stmt(self, s);
    }
}

/// Replace the param's one read, when it sits in a by-value slot — the
/// positions [`Lower::lower_consumers`] owns — with `std::mem::take(p)`. A
/// read anywhere else (a borrow, a member) is not a consumption and stays.
struct TakeAt {
    id: VarId,
    /// The field of the param the assign writes, if it writes one.
    field: Option<Sym>,
    done: bool,
}

impl TakeAt {
    /// Take at `e` when it reads the param — bare, or through the `Clone`
    /// clone insertion put there because it saw a later read (it does not
    /// know the assign kills the old value). True when it took.
    fn slot(&mut self, e: &mut IrExpr) -> bool {
        if self.done {
            return false;
        }
        if let IrExprKind::Clone { expr: inner } = &e.kind
            && self.is_place(inner)
        {
            let IrExprKind::Clone { expr: inner } = std::mem::replace(&mut e.kind, IrExprKind::Unit) else { unreachable!() };
            *e = *inner;
        }
        if !self.is_place(e) {
            return false;
        }
        let place = std::mem::replace(e, mk(IrExprKind::Unit, Ty::Unit, None));
        let (ty, span) = (place.ty.clone(), place.span);
        let template = if self.field.is_some() { "std::mem::take(&mut {p})" } else { "std::mem::take({p})" };
        *e = mk(IrExprKind::InlineRust { template: template.to_string(), args: vec![(sym("p"), place)] }, ty, span);
        self.done = true;
        true
    }

    /// Is `e` the place the assign overwrites: the param, or its field?
    fn is_place(&self, e: &IrExpr) -> bool {
        match (&e.kind, self.field) {
            (IrExprKind::Var { id }, None) => *id == self.id,
            (IrExprKind::Member { object, field }, Some(f)) => *field == f
                && matches!(object.kind, IrExprKind::Var { id } if id == self.id),
            _ => false,
        }
    }
}

impl IrMutVisitor for TakeAt {
    fn visit_expr_mut(&mut self, expr: &mut IrExpr) {
        match &mut expr.kind {
            IrExprKind::Call { args: xs, .. } | IrExprKind::TailCall { args: xs, .. }
            | IrExprKind::RuntimeCall { args: xs, .. } | IrExprKind::List { elements: xs }
            | IrExprKind::Tuple { elements: xs } => xs.iter_mut().for_each(|a| { self.slot(a); }),
            IrExprKind::Record { fields, .. } => fields.iter_mut().for_each(|(_, f)| { self.slot(f); }),
            IrExprKind::OptionSome { expr: e } | IrExprKind::ResultOk { expr: e } => { self.slot(e); }
            IrExprKind::BinOp { op: BinOp::ConcatStr | BinOp::ConcatList, left, right } => {
                self.slot(left);
                self.slot(right);
            }
            // A borrowed source (`.iter().cloned()`) becomes the taken,
            // owned one: the chain moves its elements instead of cloning.
            IrExprKind::IterChain { source, consume, .. } => {
                if self.slot(source) { *consume = true; }
            }
            _ => {}
        }
        if !self.done {
            walk_expr_mut(self, expr);
        }
    }
}
