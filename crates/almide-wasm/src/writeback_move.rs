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
//!
//! #3337: the same write-back also MOVES the var into the call. The site
//! judged the var unique (`emit_read_mut_var_cow`) and then shared it
//! (+1, the callee's credit), so the callee met its buffer at rc 2 and its
//! first `xs[i] = v` copied the whole list — 8 MB per call for a 1M-Float
//! accumulator, and a block that size is never reused by the allocator, so
//! 200 calls reached 2 GB. Where the statement is a bare call whose
//! returned buffer the very next statements write back into the var, the
//! var's own credit is the callee's: no share, and the var is emptied
//! right before the call (its write-back's release of the old occupant is
//! then a release of NULL). The callee meets rc 1 and writes in place; an
//! alias it takes itself (`let ys = xs`) still makes its judge copy.

use almide_ir::visit::{walk_expr, IrVisitor};
use almide_ir::{IrExpr, IrExprKind, IrStmt, IrStmtKind, VarId};

use crate::emitter::Emitter;

/// What a block statement may move: the temp an Assign moves out of
/// (#3104) and the vars a `mut` call site moves in (#3337), the latter
/// keyed by the call's argument slice so only that call takes them.
#[derive(Default)]
pub(crate) struct BlockMoves {
    pub(crate) temp: Option<VarId>,
    move_in: Option<MoveIn>,
}

/// #3337: the vars one call may move in — those its write-back rebinds,
/// or any at a tail site.
pub(crate) enum MoveSet {
    Vars(Vec<VarId>),
    Any,
}

#[path = "move_in_site.rs"]
mod move_in_site;
use move_in_site::{move_in_site, MoveIn};

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
pub(crate) fn reads_in_block(stmts: &[IrStmt], tail: Option<&IrExpr>, var: VarId) -> usize {
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
        let mut settle: Option<MoveIn> = None;
        let outer = self.moves.move_in.take();
        for (i, s) in stmts.iter().enumerate() {
            let site = move_in_site(stmts, tail, i);
            if let Some(m) = &site
                && m.subject.is_some()
            {
                settle = Some(m.clone());
            }
            self.moves = BlockMoves { temp: movable_temp(stmts, tail, i), move_in: MoveIn::joined(site, &outer) };
            self.lower_stmt(s)?;
            self.moves = BlockMoves::default();
            if settle.as_ref().is_some_and(|m| m.last == i) {
                self.settle_subject(settle.take().and_then(|m| m.subject));
            }
        }
        self.moves.move_in = outer;
        Ok(())
    }

    /// #3337: release the destructured call result right after its
    /// write-backs took their own credits on the buffers, so the next call
    /// on the same var meets the buffer unshared. Every OTHER slot must be a
    /// scalar — a droppable one (`__mp_res`) is a view the tail still reads.
    fn settle_subject(&mut self, subject: Option<(VarId, Vec<VarId>)>) {
        let Some((t, others)) = subject else { return };
        let Some(&(idx, ty)) = self.locals.get(&t) else { return };
        let other_droppable = others.iter().any(|b| self.locals.get(b).is_none_or(|&(_, bty)| self.rc_droppable(bty)));
        if other_droppable || !self.rc_owned.contains(&idx) || self.cells.contains(&t) {
            return;
        }
        let dec = self.dec_fn_of(ty);
        self.f.instructions().local_get(idx).call(dec);
        self.witness_dec(idx);
        self.empty_moved_temp(Some(idx));
    }

    /// #3337: the vars this call (by its argument slice) may take by move.
    /// `open`: the site may move at all (no region window). A `tail` site
    /// (not a self call, whose loop form rebinds the params) may move ANY
    /// var (#3342): nothing in this frame reads it after the call, and the
    /// exit plan's release of the emptied local is a release of NULL — the
    /// `if c then grow(xs, x) else ()` arm a write-back folds into the tail.
    pub(crate) fn take_move_in(&mut self, args: &[IrExpr], open: bool, tail: bool) -> MoveSet {
        match &self.moves.move_in {
            Some(m) if open && let Some(vars) = m.vars_at(args) => MoveSet::Vars(vars),
            _ if open && tail => MoveSet::Any,
            _ => MoveSet::Vars(Vec::new()),
        }
    }

    /// #3337: hand argument `k` of a call to table entry `i` to the callee
    /// by MOVE when its position is a declared `mut` param the callee owns
    /// and it is a var the write-back rebinds: read it
    /// through the site's judge, take no share, and return its local for
    /// [`Self::empty_moved_in`] to empty once every argument is lowered (a
    /// later argument may still read it). The var must be a plain local
    /// holding its own credit (an owned frame param counts), not a cell a
    /// closure the callee runs could read, and the only mention of it among
    /// the arguments. `None`: the share convention applies as before.
    pub(crate) fn try_move_in_arg(
        &mut self,
        move_in: &MoveSet,
        args: &[IrExpr],
        (i, k): (usize, usize),
        want: crate::SliceTy,
    ) -> Result<Option<u32>, crate::EmitError> {
        let info = &self.table.infos[i];
        if matches!(move_in, MoveSet::Vars(v) if v.is_empty()) || info.param_mut_decl.get(k) != Some(&true) || info.param_owned.get(k) != Some(&true) {
            return Ok(None);
        }
        let IrExprKind::Var { id } = &args[k].kind else { return Ok(None) };
        let Some(&(idx, _)) = self.locals.get(id) else { return Ok(None) };
        let holds_credit = if idx < self.rc_param_ceiling {
            self.rc_frame_params.contains(&idx)
        } else {
            self.rc_owned.contains(&idx)
        };
        if matches!(move_in, MoveSet::Vars(v) if !v.contains(id))
            || !holds_credit
            || !self.rc_droppable(want)
            || self.cells.contains(id)
            || args.iter().enumerate().any(|(j, o)| j != k && crate::rc_ownership::rc_mentions_var(o, *id))
        {
            return Ok(None);
        }
        if !self.lower_mut_param_arg(&args[k], true)? {
            self.lower(&args[k], Some(want))?;
        }
        self.modes_arg(want, true);
        if let Some(w) = self.witness.as_mut() {
            w.note_arg(&args[k] as *const IrExpr as usize);
            w.convention('m');
            if !w.move_local(idx) {
                w.poison();
            }
        }
        Ok(Some(idx))
    }

    /// The moved-in vars no longer hold their blocks: the callee does.
    pub(crate) fn empty_moved_in(&mut self, locals: &[u32]) {
        for &idx in locals {
            self.empty_moved_temp(Some(idx));
        }
    }

    /// The moved temp's local index, when the statement the block walk
    /// marked is this assignment of it and it is a plain owned local of a
    /// droppable type — anything else shares as before.
    pub(crate) fn take_moved_temp(&mut self, value: &IrExpr) -> Option<u32> {
        let t = self.moves.temp.take()?;
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
