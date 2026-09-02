//! The `StringInterp` arm of the clone pass: the E0505 guard for a
//! `format!` argument list (#1829).
//!
//! `"${m.size} ${m.is_file} ${kind(m)}"` renders as ONE `format!` call, and
//! `format_args!` takes a shared borrow of every argument for the duration of
//! the call — a part that is a place expression (`m`, `m.size`, `&m`) keeps
//! its root variable borrowed while the sibling parts evaluate. A sibling that
//! MOVES that variable (`kind(m)` with a by-value parameter, a `move` closure
//! capture) is rustc E0505: the flat last-use count handed `kind(m)` the move
//! because it was the variable's final occurrence, and the wasm leg — no
//! borrow live-range — accepted the program. Same rule as the call guard
//! (`pass_clone::call_borrowed_vars`, #809): INSIDE one interpolation the
//! borrow's live range, not the last-use count, is the authority.
//!
//! The guard fires only on an actual conflict (a place part's root moved by a
//! value part), and then only the value parts walk under the forced-clone
//! set: a place part renders as the borrow itself and keeps the ordinary walk,
//! so an interpolation without the conflict emits byte-for-byte what it did.

use std::collections::HashSet;
use almide_ir::*;
use almide_ir::visit::{IrVisitor, walk_expr};
use super::pass_clone::{CloneCtx, insert_clones_live};

/// The root variable of a place-expression part — a `Var`, a field/deref
/// chain on one, or a `Borrow` of either — which `format_args!` borrows in
/// place for the whole call. `None` for a value expression (a call, a
/// literal, a `Clone`): that part evaluates to a temporary and holds no
/// borrow of its own past its evaluation.
fn place_root(expr: &IrExpr) -> Option<VarId> {
    match &expr.kind {
        IrExprKind::Var { id } => Some(*id),
        IrExprKind::Member { object, .. }
        | IrExprKind::TupleIndex { object, .. }
        | IrExprKind::Borrow { expr: object, .. }
        | IrExprKind::Deref { expr: object } => place_root(object),
        _ => None,
    }
}

/// Finds an occurrence of `var` that reads it BY VALUE: a `Var` node that is
/// not the object of a field/index access nor the operand of a borrow or
/// deref. Such an occurrence renders as a move when it is the variable's
/// last use — the conflicting half of the E0505.
struct ByValueUse {
    var: VarId,
    found: bool,
}

impl IrVisitor for ByValueUse {
    fn visit_expr(&mut self, expr: &IrExpr) {
        if self.found {
            return;
        }
        match &expr.kind {
            IrExprKind::Var { id } => self.found = *id == self.var,
            // The object of an access, the operand of a borrow/deref: a read
            // through the place, never a move of the variable itself.
            IrExprKind::Member { object, .. }
            | IrExprKind::TupleIndex { object, .. }
            | IrExprKind::IndexAccess { object, .. }
            | IrExprKind::MapAccess { object, .. }
            | IrExprKind::Borrow { expr: object, .. }
            | IrExprKind::Deref { expr: object }
                if matches!(object.kind, IrExprKind::Var { .. }) =>
            {
                match &expr.kind {
                    IrExprKind::IndexAccess { index: rest, .. } | IrExprKind::MapAccess { key: rest, .. } => self.visit_expr(rest),
                    _ => {}
                }
            }
            _ => walk_expr(self, expr),
        }
    }
}

fn moves_var(expr: &IrExpr, var: VarId) -> bool {
    let mut scan = ByValueUse { var, found: false };
    scan.visit_expr(expr);
    scan.found
}

fn part_expr(part: &IrStringPart) -> Option<&IrExpr> {
    match part {
        IrStringPart::Expr { expr } => Some(expr),
        IrStringPart::Lit { .. } => None,
    }
}

/// `StringInterp { parts }` arm of [`insert_clones_live`].
pub(crate) fn insert_clones_string_interp(parts: Vec<IrStringPart>, ctx: &mut CloneCtx) -> IrExprKind {
    // Only a last-use-tracked (`eligible`) var can render as a move: an
    // `always` var is cloned at every occurrence, and a Copy scalar is never
    // cloned nor moved — forcing either would only change what it emits.
    let roots: HashSet<VarId> = parts.iter().filter_map(part_expr).filter_map(place_root)
        .filter(|v| ctx.eligible.contains(v))
        .collect();
    let conflicted: HashSet<VarId> = roots.into_iter()
        .filter(|v| parts.iter().filter_map(part_expr).any(|e| place_root(e) != Some(*v) && moves_var(e, *v)))
        .collect();
    let merged: HashSet<VarId> = ctx.always.union(&conflicted).copied().collect();
    let parts = parts.into_iter().map(|p| match p {
        IrStringPart::Expr { expr } => {
            let expr = if conflicted.is_empty() || place_root(&expr).is_some() {
                insert_clones_live(expr, ctx)
            } else {
                let mut guard = CloneCtx {
                    always: &merged,
                    eligible: ctx.eligible,
                    remaining: ctx.remaining,
                    in_loop: ctx.in_loop,
                    memo: ctx.memo,
                    fresh: ctx.fresh,
                };
                insert_clones_live(expr, &mut guard)
            };
            IrStringPart::Expr { expr }
        }
        lit => lit,
    }).collect();
    IrExprKind::StringInterp { parts }
}
