//! #3104: a block temp whose ONLY use is the value of the next assignment
//! MOVES its credit into the assigned place instead of sharing it.
//!
//! The C-132 write-back binds the callee's returned buffer and stores it
//! back — `{ let __mp_buf = f(xs, v); xs = __mp_buf }`. As a share, the
//! store gave the block a second credit, and the temp kept its own until
//! the next rebind or the frame's exit released it. So at the next call on
//! the same place the call-site copy-on-write judge (#2503) saw rc 2 and
//! copied the whole buffer: a loop of `mut`-param pushes was O(n²) in
//! bytes (1000 pushes of an Int: 4 MB). Moving the credit keeps the count
//! at 1 and the push in place.
//!
//! The move is only taken where it cannot be observed: the temp is bound
//! in the same block, before the assignment, and the block reads it
//! nowhere else (lambdas included) — a nested place's `var` holder
//! (`var t1 = o.a; t1.xs = buf; o.a = t1`) moves the same way. The emitted store skips the share,
//! the temp's local is emptied (a release of it is then a release of NULL,
//! a no-op), and the ownership witness records the credit's transfer.

use almide_ir::visit::{walk_expr, IrVisitor};
use almide_ir::{IrExpr, IrExprKind, IrStmt, IrStmtKind, VarId};

use crate::emitter::Emitter;

/// The temp statement `i` of a block may move out of, if any.
pub(crate) fn movable_temp(stmts: &[IrStmt], tail: Option<&IrExpr>, i: usize) -> Option<VarId> {
    let value = match &stmts.get(i)?.kind {
        IrStmtKind::Assign { value, .. } | IrStmtKind::FieldAssign { value, .. } => value,
        _ => return None,
    };
    let IrExprKind::Var { id } = &value.kind else { return None };
    let bound_before = stmts[..i]
        .iter()
        .any(|s| matches!(&s.kind, IrStmtKind::Bind { var, .. } if var == id));
    (bound_before && reads_in_block(stmts, tail, *id) == 1).then_some(*id)
}

/// How many times `var` is read in the block (statements and tail).
fn reads_in_block(stmts: &[IrStmt], tail: Option<&IrExpr>, var: VarId) -> usize {
    struct Count(VarId, usize);
    impl IrVisitor for Count {
        fn visit_expr(&mut self, e: &IrExpr) {
            if matches!(&e.kind, IrExprKind::Var { id } if *id == self.0) {
                self.1 += 1;
            }
            walk_expr(self, e);
        }
    }
    let mut c = Count(var, 0);
    for s in stmts {
        c.visit_stmt(s);
    }
    if let Some(t) = tail {
        c.visit_expr(t);
    }
    c.1
}

impl Emitter<'_> {
    /// A block's statements, each told which temp it may move out of.
    pub(crate) fn lower_block_stmts(&mut self, stmts: &[IrStmt], tail: Option<&IrExpr>) -> Result<(), crate::EmitError> {
        for (i, s) in stmts.iter().enumerate() {
            self.moved_temp = movable_temp(stmts, tail, i);
            self.lower_stmt(s)?;
            self.moved_temp = None;
        }
        Ok(())
    }

    /// The moved temp's local index, when the statement the block walk
    /// marked is this assignment of it and it is a plain owned local of a
    /// droppable type — anything else shares as before.
    pub(crate) fn take_moved_temp(&mut self, value: &IrExpr) -> Option<u32> {
        let t = self.moved_temp.take()?;
        if !matches!(&value.kind, IrExprKind::Var { id } if *id == t) || self.cells.contains(&t) {
            return None;
        }
        let &(idx, ty) = self.locals.get(&t)?;
        (idx >= self.rc_param_ceiling && self.rc_droppable(ty) && self.rc_owned.contains(&idx)).then_some(idx)
    }

    /// After the store: the temp no longer holds the block.
    pub(crate) fn empty_moved_temp(&mut self, moved: Option<u32>) {
        let Some(idx) = moved else { return };
        self.f.instructions().i32_const(0).local_set(idx);
        if let Some(w) = self.witness.as_mut() {
            w.empty_local(idx);
        }
    }

    /// The witness side of a moved assignment: `dst` now owns the object
    /// `src` held, with no share and no release on it.
    pub(crate) fn witness_transfer(&mut self, dst: u32, released_old: bool, src: u32) {
        if let Some(w) = self.witness.as_mut()
            && !w.transfer(dst, released_old, src)
        {
            w.poison();
        }
    }
}
