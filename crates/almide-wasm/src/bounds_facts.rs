//! #3345: a bounds check that an earlier check in the same straight-line
//! stretch already decided is not emitted again.
//!
//! fannkuchredux's flip loop reads `perm[lo]` and `perm[hi]` and then stores
//! to both: four checks per iteration where two decide everything (native's
//! LLVM drops the repeats the same way).
//!
//! A FACT `(xs, i, c)` says: "`i + c <u count(xs)` was checked and passed,
//! and neither side of that comparison has changed since". It is recorded
//! right after an emitted check of `xs[i + c]` (`c` a small literal, 0 for a
//! plain `xs[i]` — payload_ptr.rs `affine_index`), where `i` is a variable and the
//! count is the loop's HOISTED count (len_hoist.rs) — and a later `xs[i]`,
//! read or store, under the same fact skips its check.
//!
//! WHY IT HOLDS — each half of the comparison, across everything that can
//! run between the check and the reuse:
//! - **The count** is the hoisted local: len_hoist.rs proves the loop never
//!   rebinds `xs`, never hands it to a call and holds no lambda, so the
//!   local is the list's element count for the whole loop. An element store
//!   or a copy-on-write copy changes the block ADDRESS, never the count, and
//!   the address is re-read at every access.
//! - **The index** is a variable, and between the two accesses only
//!   FACT-PRESERVING statements run ([`fact_preserving`]): `let` / assign /
//!   element store whose expressions are variables, literals, element reads
//!   and scalar arithmetic or comparison — no call (nothing can write a
//!   cell or a global behind the code's back), no `and` / `or` (their right
//!   side is conditional, so a fact recorded there would not hold after),
//!   no control flow. A `let` or assign of a variable drops every fact that
//!   names it. Any other statement — `if`, a loop, a match, a guard, an RC
//!   statement, a call — runs with recording OFF and leaves the facts EMPTY
//!   after it, so nothing flows across a branch, a join or a nested loop.
//! - **Exits**: facts live only from a loop body's start forward, in
//!   emission order; every iteration starts with none (the back edge carries
//!   nothing), and a `break` / `continue` / trap sits inside a statement that
//!   is not fact-preserving. A trap between the check and the reuse ends the
//!   run, so no later state is observable.
//! - **Aliases and captures**: the fact is about a count and an `i64` in a
//!   local, not about memory, so another handle to the same block cannot
//!   invalidate it; a closure that captured `i` could change it only by being
//!   called, and no call runs under a fact.

use almide_ir::{BinOp, IrExpr, IrExprKind, IrStmt, IrStmtKind, VarId};
use almide_types::types::Ty;

use crate::emitter::Emitter;

fn scalar(t: &Ty) -> bool {
    matches!(t, Ty::Int | Ty::Float | Ty::Bool)
}

/// An expression that runs no call and no control flow, and assigns nothing.
fn plain(e: &IrExpr) -> bool {
    match &e.kind {
        IrExprKind::Var { .. } | IrExprKind::LitInt { .. } | IrExprKind::LitFloat { .. } | IrExprKind::LitBool { .. } => true,
        IrExprKind::IndexAccess { object, index } => matches!(object.kind, IrExprKind::Var { .. }) && plain(index),
        IrExprKind::UnOp { operand, .. } => scalar(&operand.ty) && plain(operand),
        IrExprKind::BinOp { op, left, right } => {
            !matches!(op, BinOp::And | BinOp::Or) && scalar(&left.ty) && scalar(&right.ty) && plain(left) && plain(right)
        }
        _ => false,
    }
}

/// A statement the facts survive (module header).
pub(crate) fn fact_preserving(st: &IrStmt) -> bool {
    match &st.kind {
        IrStmtKind::Bind { value, .. } | IrStmtKind::Assign { value, .. } => plain(value),
        IrStmtKind::IndexAssign { index, value, .. } => plain(index) && plain(value),
        IrStmtKind::Comment { .. } => true,
        _ => false,
    }
}

impl Emitter<'_> {
    /// Lower one loop-body statement under the fact discipline (module header).
    pub(crate) fn lower_stmt_with_facts(&mut self, st: &IrStmt) -> Result<(), crate::EmitError> {
        if !fact_preserving(st) {
            let on = self.bounds_facts.take().is_some();
            self.lower_stmt(st)?;
            self.bounds_facts = on.then(Vec::new);
            return Ok(());
        }
        self.lower_stmt(st)?;
        if let IrStmtKind::Bind { var, .. } | IrStmtKind::Assign { var, .. } = &st.kind
            && let Some(facts) = self.bounds_facts.as_mut()
        {
            facts.retain(|&(xs, i, _)| xs != *var && i != *var);
        }
        Ok(())
    }

    /// `(xs, v, c)` when `xs[index]` is a fact candidate here: recording is
    /// on, the index is `v` or `v + c` (payload_ptr.rs `affine_index`) and
    /// `xs`'s count is hoisted.
    fn fact_key(&self, xs: VarId, index: &IrExpr) -> Option<(VarId, VarId, i64)> {
        self.bounds_facts.as_ref()?;
        self.hoisted_counts.get(&xs)?;
        let (v, c) = super::payload_ptr::affine_index(index)?;
        Some((xs, v, c))
    }

    /// Is `xs[index]` already known in bounds (its check can be skipped)?
    pub(crate) fn bounds_known(&self, xs: VarId, index: &IrExpr) -> bool {
        self.fact_key(xs, index).is_some_and(|k| self.bounds_facts.as_ref().is_some_and(|f| f.contains(&k)))
    }

    /// Record that `xs[index]`'s check was just emitted and passed.
    pub(crate) fn bounds_record(&mut self, xs: VarId, index: &IrExpr) {
        if let Some(k) = self.fact_key(xs, index)
            && let Some(f) = self.bounds_facts.as_mut()
            && !f.contains(&k)
        {
            f.push(k);
        }
    }
}
