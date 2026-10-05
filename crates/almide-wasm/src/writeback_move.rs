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

/// Does another argument mention `var` in a way that could hold or change
/// its block? A scalar-typed argument that only READS it by index or field
/// (`xs[0]`, `h.n`) is computed before the var is emptied, carries no
/// handle and writes nothing, so it cannot.
fn mentions_beyond_scalar(a: &IrExpr, var: VarId) -> bool {
    use almide_types::types::Ty;
    struct Reads {
        var: VarId,
        other: bool,
    }
    impl IrVisitor for Reads {
        fn visit_expr(&mut self, e: &IrExpr) {
            match &e.kind {
                IrExprKind::IndexAccess { object, index } if matches!(&object.kind, IrExprKind::Var { id } if *id == self.var) => {
                    self.visit_expr(index)
                }
                IrExprKind::Member { object, .. } | IrExprKind::TupleIndex { object, .. }
                    if matches!(&object.kind, IrExprKind::Var { id } if *id == self.var) => {}
                IrExprKind::Var { id } if *id == self.var => self.other = true,
                IrExprKind::Block { .. } | IrExprKind::Lambda { .. } => self.other |= crate::rc_ownership::rc_mentions_var(e, self.var),
                _ => walk_expr(self, e),
            }
        }
    }
    let scalar = matches!(
        a.ty,
        Ty::Int | Ty::Float | Ty::Bool | Ty::Int8 | Ty::Int16 | Ty::Int32 | Ty::Int64
            | Ty::UInt8 | Ty::UInt16 | Ty::UInt32 | Ty::UInt64 | Ty::Float32 | Ty::Float64
    );
    if !scalar {
        return crate::rc_ownership::rc_mentions_var(a, var);
    }
    let mut r = Reads { var, other: false };
    r.visit_expr(a);
    r.other
}

/// A type no closure can hide in: scalars, text, bytes, and the builtin
/// containers of such — never a function, a record or a variant, which
/// might carry one.
fn inert_ty(t: &almide_types::types::Ty) -> bool {
    use almide_types::types::Ty;
    match t {
        Ty::Applied(_, args) | Ty::Tuple(args) => args.iter().all(inert_ty),
        Ty::Fn { .. } | Ty::Named(..) | Ty::Record { .. } | Ty::OpenRecord { .. } | Ty::Variant { .. } => false,
        Ty::TypeVar(_) | Ty::Union(_) | Ty::Unknown | Ty::Never => false,
        _ => true,
    }
}

/// What a block statement may move: the temp an Assign moves out of
/// (#3104) and the vars a `mut` call site moves in (#3337), the latter
/// keyed by the call's argument slice so only that call takes them.
#[derive(Default)]
pub(crate) struct BlockMoves {
    pub(crate) temp: Option<VarId>,
    move_in: Option<MoveIn>,
}

/// #3337: the places one call may move in — those its write-back
/// rebinds, or any plain local at a tail site.
pub(crate) enum MoveSet {
    Vars(Vec<Place>),
    Any,
}

/// Where a moved-in argument came from, emptied once every argument is
/// lowered: a var (a local, or a C-319 cell through its address) or a
/// record var's field slot (#3343).
pub(crate) enum Emptied {
    Var(VarId, u32, crate::SliceTy),
    /// The record var's local, the slot's offset, and whether the local
    /// holds a C-319 cell (the record one load deeper).
    Slot(u32, u32, bool),
    /// A top-let global the callee cannot reach.
    Global(VarId, u32),
}

#[path = "move_in_site.rs"]
mod move_in_site;
use move_in_site::{move_in_site, MoveIn, Place};

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
        self.lower_stmts_moving(stmts, tail, Self::lower_stmt)
    }

    /// [`Self::lower_block_stmts`] with the statement lowering given — a
    /// loop body lowers each statement with its bounds facts (#3406: a
    /// loop body's `b = add(b, i)` moves `b` like a block's does).
    pub(crate) fn lower_stmts_moving(
        &mut self,
        stmts: &[IrStmt],
        tail: Option<&IrExpr>,
        mut lower: impl FnMut(&mut Self, &IrStmt) -> Result<(), crate::EmitError>,
    ) -> Result<(), crate::EmitError> {
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
            lower(self, s)?;
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
    /// and it names a place the write-back rebinds: read it through the
    /// site's judge, take no share, and return where it lives for
    /// [`Self::empty_moved_in`] to empty once every argument is lowered (a
    /// later argument may still read it). It must be the only mention of
    /// its var among the arguments. `None`: the share convention applies.
    pub(crate) fn try_move_in_arg(
        &mut self,
        move_in: &MoveSet,
        args: &[IrExpr],
        (i, k): (usize, usize),
        want: crate::SliceTy,
    ) -> Result<Option<Emptied>, crate::EmitError> {
        let info = &self.table.infos[i];
        // #3406: an OWNED non-`mut` param takes the var too where the
        // statement itself rebinds it (`b = add(b, i)`: a reassignment
        // moves) — never at a bare tail site, which only `mut` widens.
        let is_mut = info.param_mut_decl.get(k) == Some(&true);
        if matches!(move_in, MoveSet::Vars(v) if v.is_empty())
            || (!is_mut && !matches!(move_in, MoveSet::Vars(_)))
            || info.param_owned.get(k) != Some(&true)
            || !self.rc_droppable(want)
        {
            return Ok(None);
        }
        let Some(place) = Place::of(&args[k]) else { return Ok(None) };
        let root = match place {
            Place::Var(v) | Place::Field(v, _) => v,
        };
        let Some(&(idx, root_ty)) = self.locals.get(&root) else {
            return self.try_move_in_global(move_in, args, (i, k), place, want);
        };
        let listed = match move_in {
            MoveSet::Vars(v) => v.contains(&place),
            MoveSet::Any => matches!(place, Place::Var(_)) && !self.cells.contains(&root),
        };
        if !listed || args.iter().enumerate().any(|(j, o)| j != k && mentions_beyond_scalar(o, root)) {
            return Ok(None);
        }
        // A C-319 cell (#3343) is shared with every closure that captured
        // it, so a closure the callee runs could read it while it is empty —
        // admitted only when no argument can carry one (a closure value, or
        // a record that might hold one).
        if self.cells.contains(&root) && !args.iter().all(|a| inert_ty(&a.ty)) {
            return Ok(None);
        }
        let Some(emptied) = self.emptied_place(place, idx, root_ty, want)? else { return Ok(None) };
        self.hand_over_moved(&emptied, &args[k], want, is_mut)?;
        Ok(Some(emptied))
    }

    /// Does the local at `idx` hold its own credit (an owned frame param,
    /// or an owned local)?
    pub(crate) fn holds_credit(&self, idx: u32) -> bool {
        if idx < self.rc_param_ceiling { self.rc_frame_params.contains(&idx) } else { self.rc_owned.contains(&idx) }
    }

    /// Where a moved place in local `idx` lives, if it can move: a var (a
    /// cell holds its occupant's credit), or a handle field of a record var.
    fn emptied_place(&self, place: Place, idx: u32, root_ty: crate::SliceTy, want: crate::SliceTy) -> Result<Option<Emptied>, crate::EmitError> {
        Ok(match place {
            Place::Var(id) if self.cells.contains(&id) || self.holds_credit(idx) => Some(Emptied::Var(id, idx, root_ty)),
            Place::Var(_) => None,
            Place::Field(h, field) => {
                let in_cell = self.cells.contains(&h);
                if !(in_cell || self.holds_credit(idx)) || !matches!(root_ty, crate::SliceTy::Named(_)) {
                    return Ok(None);
                }
                let (fty, off) = self.record_field_slot(root_ty, &field)?;
                (fty == want && self.elem_is_handle(fty)).then_some(Emptied::Slot(idx, off, in_cell))
            }
        })
    }

    /// Push the moved place's block (judged unique first, no share) and note
    /// the hand-over convention; the credit itself moves at
    /// [`Self::empty_moved_in`].
    fn hand_over_moved(&mut self, emptied: &Emptied, arg: &IrExpr, want: crate::SliceTy, is_mut: bool) -> Result<(), crate::EmitError> {
        match *emptied {
            Emptied::Var(..) | Emptied::Global(..) => {
                if !self.lower_mut_param_arg(arg, is_mut)? {
                    self.lower(arg, Some(want))?;
                }
            }
            // Unshare the path (the record, then the leaf), then read the
            // slot's block with the slot's own credit.
            Emptied::Slot(rec, off, in_cell) => {
                self.make_mut_place_unique(arg)?;
                self.f.instructions().local_get(rec);
                if in_cell {
                    self.f.instructions().i32_load(crate::slot_memarg(0));
                }
                self.f.instructions().i32_load(crate::slot_memarg(off));
            }
        }
        self.modes_arg(want, true);
        // The place's credit moves at [`Self::empty_moved_in`], not here: a
        // later argument may still read the block through the place (a
        // scalar `w[1]`, `mentions_beyond_scalar`), and that read is the
        // frame's — its line must show the read before the hand-over.
        if let Some(w) = self.witness.as_mut() {
            w.note_arg(arg as *const IrExpr as usize);
            w.convention('m');
        }
        Ok(())
    }

    /// #3343: a top-let global the write-back rebinds moves in like a local
    /// when the callee cannot reach it (global_reach.rs) — the one reader
    /// that could see it empty.
    fn try_move_in_global(
        &mut self,
        move_in: &MoveSet,
        args: &[IrExpr],
        (i, k): (usize, usize),
        place: Place,
        want: crate::SliceTy,
    ) -> Result<Option<Emptied>, crate::EmitError> {
        let Place::Var(id) = place else { return Ok(None) };
        let g = (self.var_space, id);
        let Some(&(gidx, _)) = self.globals.get(&g) else { return Ok(None) };
        let reaches = !matches!(self.work.global_reach.borrow().get(i), Some(Some(set)) if !set.contains(&g));
        if reaches
            || !matches!(move_in, MoveSet::Vars(v) if v.contains(&place))
            || args.iter().enumerate().any(|(j, o)| j != k && mentions_beyond_scalar(o, id))
        {
            return Ok(None);
        }
        let emptied = Emptied::Global(id, gidx);
        let is_mut = self.table.infos[i].param_mut_decl.get(k) == Some(&true);
        self.hand_over_moved(&emptied, &args[k], want, is_mut)?;
        Ok(Some(emptied))
    }

    /// The moved-in places no longer hold their blocks: the callee does.
    pub(crate) fn empty_moved_in(&mut self, emptied: &[Emptied]) -> Result<(), crate::EmitError> {
        for e in emptied {
            match *e {
                Emptied::Var(id, idx, ty) => {
                    self.f.instructions().i32_const(0);
                    self.emit_store_var(id, idx, ty)?;
                    self.witness_move_and_empty(id, false);
                }
                Emptied::Global(id, gidx) => {
                    self.f.instructions().i32_const(0).global_set(gidx);
                    self.witness_move_and_empty(id, true);
                }
                Emptied::Slot(rec, off, in_cell) => {
                    self.f.instructions().local_get(rec);
                    if in_cell {
                        self.f.instructions().i32_load(crate::slot_memarg(0));
                    }
                    self.f.instructions().i32_const(0).i32_store(crate::slot_memarg(off));
                }
            }
        }
        Ok(())
    }

    /// The moved place's credit goes to the callee — every argument has been
    /// lowered, so no later read precedes it — and its holder is now empty.
    pub(crate) fn witness_move_and_empty(&mut self, id: VarId, global: bool) {
        let Some(l) = self.witness_holder(id, global) else { return };
        let Some(w) = self.witness.as_mut() else { return };
        if !w.move_local(l) {
            w.poison();
        }
        w.empty_local(l);
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
