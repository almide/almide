//! The in-place map delete (#3640), `map.set`'s window (map_inplace.rs)
//! applied to removal. The functional `map.remove(m, k)` over a receiver
//! whose credit the op holds (an owned temporary, or a dying var: the rebind `m = map.remove(m, k)`
//! in a loop, a self tail call, a fold callback — `@reuse(m)` on the
//! declaration), used to copy the whole map on every call, an absent key
//! included. Now the same judge decides:
//!
//! * **absent key** — nothing to do: the receiver IS the result, shared or
//!   not (removing nothing cannot be observed), so the call costs the
//!   keyed lookup alone.
//! * **present key, the credit is the block's only one** (a heap block at
//!   rc 1) — the entry's key and value credits are released, the later
//!   entries shift down one (insertion order kept, as native's
//!   `Vec::remove`), `len` drops by one entry and an indexed block's index
//!   is retired (map_index.rs invariant 3). That shift is O(n), so the
//!   remove on a TOMBSTONE var (below) leaves a hole instead.
//! * **present key, shared** (an alias, a holder, a pool static) — the
//!   functional copy without the entry, and the op's credit on the shared
//!   original goes: every other holder keeps the pre-delete map (C-033).
//!
//! A borrowed receiver keeps the functional copy, as `map.set` does.
//!
//! # Tombstone vars (#3691)
//!
//! The `m = map.remove(m, k)` of a local `m` with an indexed key class (Int or
//! String) past the index threshold tombstones the entry instead of
//! shifting (map_index.rs "Holes": `$mapidx_kill`, then `$mapidx_settle`
//! compacts once the holes outnumber the live entries) — native's
//! `AlmideMap::remove` holes the same way, so a delete is amortized O(1) on
//! both legs. The holes must not reach anything that walks the block, so
//! they stay inside the var: `tomb_vars_of` names, per frame, every var a
//! tail remove rebinds, and each read of one SETTLES the block (compacts
//! it, a no-op without holes) before the value goes anywhere — the read
//! hook in emitter.rs and the `mut`-param pass (`emit_read_mut_var_cow`).
//! Only the reads that look through the index read it raw: `get`,
//! `get_or`, `contains`, `len` / `is_empty` (which subtract the holes) and
//! the in-place set and delete windows. Holes only ever sit in a block at
//! rc 1 — every holder takes its credit through a settling read — so a
//! shared block's functional copy never copies one. A param, a global and
//! a C-319 cell keep the shift.
//!
//! #4208: the FUNCTIONAL remove holes too when its result is the var's own
//! next value — `m = map.remove(m, k)` / `s = set.remove(s, x)` and a
//! `list.fold` body ending in `map.remove(acc, k)` (`tail_removes_of`: the
//! remove in tail position of the rebind's rhs or the fold body). Those
//! vars are tombstone vars, the remove reads its dying receiver raw, and
//! the plain fold settles its accumulator once as it hands the result on.
//! Sets hole the same way through the same helpers (stride = the member
//! slot, the key at offset 0); `set.contains` / `len` / `is_empty` read a
//! tombstone set raw. A remove anywhere else still shifts.

use std::collections::HashSet;

use almide_ir::visit::{walk_expr, walk_stmt, IrVisitor};
use almide_ir::{CallTarget, IrExpr, IrExprKind, IrStmt, IrStmtKind, VarId};
use wasm_encoder::{BlockType, MemArg};

use crate::arm::{ArgMode, Lowered};
use crate::emitter::Emitter;
use crate::*;

/// The helpers a judged delete calls, resolved before its operands lower.
/// `tomb` = `($mapidx_kill, $mapidx_settle)` for a tombstone var's delete.
#[derive(Clone, Copy)]
struct RemoveFns {
    scan: u32,
    forget: Option<u32>,
    drop_map: u32,
    tomb: Option<(u32, u32)>,
}

/// Every var rebound to a functional remove of itself (`m = map.remove(m, k)`, `s = set.remove(s,
/// x)`), and every `list.fold` accumulator whose callback body ends in one
/// (`(m, i) => map.remove(m, i)`, #4208) — the frame's candidate tombstone
/// vars (an over-approximation is only a no-op settle per read).
pub(crate) fn tomb_vars_of(body: &IrExpr) -> HashSet<VarId> {
    struct Scan(HashSet<VarId>);
    impl IrVisitor for Scan {
        fn visit_expr(&mut self, e: &IrExpr) {
            if let IrExprKind::Call { target: CallTarget::Module { module, func, .. }, args, .. } = &e.kind {
                if module.as_str() == "list"
                    && func.as_str() == "fold"
                    && let Some(IrExprKind::Lambda { params, body, .. }) = args.get(2).map(|a| &a.kind)
                    && let Some(&(acc, _)) = params.first()
                    && !tail_removes_of(body, acc).is_empty()
                {
                    self.0.insert(acc);
                }
            }
            walk_expr(self, e);
        }
        fn visit_stmt(&mut self, s: &IrStmt) {
            if let IrStmtKind::Assign { var, value } = &s.kind
                && !tail_removes_of(value, *var).is_empty()
            {
                self.0.insert(*var);
            }
            walk_stmt(self, s);
        }
    }
    let mut s = Scan(HashSet::new());
    s.visit_expr(body);
    s.0
}

/// The receiver nodes of the `map.remove(var, _)` / `set.remove(var, _)`
/// calls in TAIL position of `e` (through block tails, `if` branches and
/// `match` arms) — the removes whose result IS the next value of `var`
/// (the rhs of `var = …`, or a fold body whose value rebinds the
/// accumulator). Only those may leave holes (#4208): a remove anywhere else
/// hands its result to a holder that does not settle.
/// A receiver of the keyed-block representation: a Map or a Set block,
/// the two kinds the key index (map_index.rs) and its holes cover. The
/// property is read off the receiver's TYPE, so the scan names no module:
/// a remove over any other collection keeps its own lowering.
fn is_keyed_block(ty: &almide_types::types::Ty) -> bool {
    use almide_types::types::TypeConstructorId as C;
    matches!(ty, almide_types::types::Ty::Applied(C::Map | C::Set, _))
}

pub(crate) fn tail_removes_of(e: &IrExpr, var: VarId) -> Vec<*const IrExpr> {
    fn walk(e: &IrExpr, var: VarId, out: &mut Vec<*const IrExpr>) {
        match &e.kind {
            IrExprKind::Call { target: CallTarget::Module { func, .. }, args, .. }
                if func.as_str() == "remove" && args.len() == 2 && is_keyed_block(&args[0].ty) =>
            {
                if matches!(args[0].kind, IrExprKind::Var { id } if id == var) {
                    out.push(&args[0] as *const IrExpr);
                }
            }
            IrExprKind::Block { expr: Some(tail), .. } => walk(tail, var, out),
            IrExprKind::If { then, else_, .. } => {
                walk(then, var, out);
                walk(else_, var, out);
            }
            IrExprKind::Match { arms, .. } => arms.iter().for_each(|a| walk(&a.body, var, out)),
            _ => {}
        }
    }
    let mut out = Vec::new();
    walk(e, var, &mut out);
    out
}

/// The holds a judged delete works on: the receiver block (its result,
/// too) and the scan's ABSOLUTE entry address (0 = absent).
#[derive(Clone, Copy)]
pub(crate) struct RemoveHolds {
    pub(crate) mh: u32,
    pub(crate) eh: u32,
}

impl Emitter<'_> {
    fn map_remove_fns(&mut self, k: SliceTy, v: SliceTy) -> Result<RemoveFns, EmitError> {
        let scan = self.keyed_find(k)?;
        let forget = self.keyed_remove(k);
        let drop_map = self.dec_fn_of(SliceTy::Map(self.types.intern(k), self.types.intern(v)));
        Ok(RemoveFns { scan, forget, drop_map, tomb: None })
    }

    /// A read of tombstone var `id` (type `ty`, its value on the stack)
    /// settles the block first — unless a keyed read asked for it raw.
    pub(crate) fn settle_tomb_read(&mut self, id: &VarId, ty: SliceTy) {
        if self.raw_map_read || !self.tomb_vars.contains(id) || self.cells.contains(id) {
            return;
        }
        let (k, koff, esz) = match ty {
            SliceTy::Map(kt, vt) => {
                let (k, v) = (self.types.el(kt), self.types.el(vt));
                let (koff, _, esz) = crate::collections::entry_layout(k, v);
                (k, koff, esz)
            }
            // a Set block is its members, one stride each, the key at 0
            SliceTy::Set(h) => {
                let e = self.types.el(h);
                (e, 0, e.slot_size())
            }
            _ => return,
        };
        let Some(settle) = self.keyed_settle(k) else { return };
        self.f.instructions().i32_const(esz as i32).i32_const(koff as i32).i32_const(1).call(settle);
    }

    /// Lower with the functional removes of `var` in tail position of `e`
    /// allowed to leave holes (#4208): `e` is the rhs of `var = e`, or a
    /// fold body whose value rebinds the accumulator `var`, so their result
    /// is `var`'s next value and the holes stay inside the tombstone var.
    pub(crate) fn with_tomb_tail<T>(&mut self, var: VarId, e: &IrExpr, f: impl FnOnce(&mut Self) -> T) -> T {
        let marks: Vec<usize> = if self.tomb_vars.contains(&var) && !self.cells.contains(&var) {
            tail_removes_of(e, var).into_iter().map(|p| p as usize).collect()
        } else {
            Vec::new()
        };
        for m in &marks {
            self.tomb_tail.insert(*m);
        }
        let got = f(self);
        for m in &marks {
            self.tomb_tail.remove(m);
        }
        got
    }

    /// The functional remove over receiver `m` leaves a hole: `m` is a
    /// dying tombstone var (a frame local, not a param) and the call is a
    /// tail remove of its rebind ([`Self::with_tomb_tail`]).
    fn hole_lane(&self, m: &IrExpr, dying: Option<(VarId, u32, SliceTy)>) -> bool {
        dying.is_some_and(|(id, idx, _)| self.tomb_vars.contains(&id) && idx >= self.rc_param_ceiling)
            && self.tomb_tail.contains(&(m as *const IrExpr as usize))
    }

    /// Lower a remove's receiver, retained; a hole lane's read is raw (the
    /// block may hold holes, and the remove steps over them).
    fn lower_remove_receiver(&mut self, m: &IrExpr, dying: Option<(VarId, u32, SliceTy)>, hole: bool) -> Result<SliceTy, EmitError> {
        self.owned_call_marks.set_moving(m, dying.is_some());
        self.raw_map_read = hole;
        let got = self.lower_arg(m, None, ArgMode::Retain);
        self.raw_map_read = false;
        self.owned_call_marks.set_moving(m, false);
        got
    }

    /// Lower a keyed read's receiver `m`: a plain var is read raw (no
    /// settle) — the index lane steps over holes. Anything else lowers as
    /// usual.
    pub(crate) fn lower_map_receiver(&mut self, m: &IrExpr, raw: bool) -> Result<SliceTy, EmitError> {
        self.raw_map_read = raw && matches!(m.kind, IrExprKind::Var { .. });
        let got = self.lower_arg(m, None, ArgMode::Borrow);
        self.raw_map_read = false;
        got
    }

    /// `m` is a plain read of a tombstone var: its block may hold holes.
    pub(crate) fn tomb_receiver(&self, m: &IrExpr) -> bool {
        matches!(&m.kind, IrExprKind::Var { id } if self.tomb_vars.contains(id) && !self.cells.contains(id))
    }

    /// The live entry count (i32) of the map block on the stack, `stride`
    /// bytes per entry; `holed` subtracts a tombstone var's holes.
    pub(crate) fn emit_map_live_count(&mut self, stride: u32, holed: bool) -> Result<(), EmitError> {
        if !holed {
            self.f.instructions().i32_load(len_memarg()).i32_const(stride as i32).i32_div_u();
            return Ok(());
        }
        let mh = self.hold_i32()?;
        let dead = self.map_dead_fn();
        let mut i = self.f.instructions();
        i.local_tee(mh).i32_load(len_memarg()).i32_const(stride as i32).i32_div_u();
        i.local_get(mh).call(dead).i32_sub();
        self.release_i32();
        Ok(())
    }

    /// Release the handle in the slot `off` bytes into entry `eh` and leave
    /// NULL there, so a later walk of the hole releases nothing.
    fn emit_release_and_clear(&mut self, eh: u32, off: u32, ty: SliceTy) {
        if !self.elem_is_handle(ty) {
            return;
        }
        self.f.instructions().local_get(eh).i32_const(off as i32).i32_add();
        self.emit_release_slot_at(ty);
        let at = MemArg { offset: u64::from(off), align: 2, memory_index: 0 };
        self.f.instructions().local_get(eh).i32_const(almide_layout::NULL_ADDR as i32).i32_store(at);
    }

    /// The in-place delete of a tombstone var: `$mapidx_kill` holes the
    /// entry (its value's — and a String key's — credit released, the slot
    /// cleared; an Int key stays for the liveness probe), then
    /// `$mapidx_settle` compacts once the holes outnumber the live entries.
    /// Under the threshold the kill declines and the shift runs.
    fn emit_map_remove_hole(&mut self, h: RemoveHolds, k: SliceTy, v: SliceTy, fns: RemoveFns, (kill, settle): (u32, u32)) {
        let RemoveHolds { mh, eh } = h;
        let (koff, voff, esz) = crate::collections::entry_layout(k, v);
        let mut i = self.f.instructions();
        i.local_get(mh).i32_const(esz as i32).i32_const(koff as i32).local_get(eh).call(kill);
        i.if_(BlockType::Empty);
        self.emit_release_and_clear(eh, voff, v);
        self.emit_release_and_clear(eh, koff, k);
        let mut i = self.f.instructions();
        i.local_get(mh).i32_const(esz as i32).i32_const(koff as i32).i32_const(0).call(settle).drop();
        i.else_();
        self.emit_map_remove_shift(h, k, v, fns.forget);
        self.f.instructions().end();
    }

    /// The functional remove's result, left on the stack: a copy of `mh`
    /// without the entry at `eh` (a whole copy when `eh` is 0), holding
    /// its own entry credits — the removed entry's stay with the input.
    pub(crate) fn emit_map_remove_copy(&mut self, h: RemoveHolds, k: SliceTy, v: SliceTy) -> Result<(), EmitError> {
        let RemoveHolds { mh, eh } = h;
        let esz = crate::collections::entry_layout(k, v).2 as i32;
        let payload = almide_layout::PAYLOAD as i32;
        let ho = self.hold_i32()?;
        let hp = self.hold_i32()?;
        let mut i = self.f.instructions();
        i.local_get(eh).i32_eqz().if_(BlockType::Empty);
        // absent: whole copy
        i.local_get(mh).i32_load(len_memarg()).call(F_ALLOC).local_set(ho);
        i.local_get(ho).i32_const(payload).i32_add();
        i.local_get(mh).i32_const(payload).i32_add();
        i.local_get(mh).i32_load(len_memarg());
        i.memory_copy(0, 0);
        i.else_();
        // pre = eh - (mh + PAYLOAD): the entries before the removed one
        i.local_get(eh).local_get(mh).i32_const(payload).i32_add().i32_sub().local_set(hp);
        i.local_get(mh).i32_load(len_memarg()).i32_const(esz).i32_sub();
        i.call(F_ALLOC).local_set(ho);
        i.local_get(ho).i32_const(payload).i32_add();
        i.local_get(mh).i32_const(payload).i32_add();
        i.local_get(hp);
        i.memory_copy(0, 0);
        // the entries after it, one entry earlier
        i.local_get(ho).i32_const(payload).i32_add().local_get(hp).i32_add();
        i.local_get(eh).i32_const(esz).i32_add();
        i.local_get(mh).i32_load(len_memarg()).local_get(hp).i32_sub().i32_const(esz).i32_sub();
        i.memory_copy(0, 0);
        i.end();
        let mt = SliceTy::Map(self.types.intern(k), self.types.intern(v));
        self.emit_inc_entries(ho, mt, None);
        self.f.instructions().local_get(ho);
        self.release_i32(); // hp
        self.release_i32(); // ho
        Ok(())
    }

    /// Remove the entry at `eh` from `mh` IN PLACE: its credits released,
    /// the later entries moved down one, `len` shrunk, the index retired.
    fn emit_map_remove_shift(&mut self, RemoveHolds { mh, eh }: RemoveHolds, k: SliceTy, v: SliceTy, forget: Option<u32>) {
        let (koff, voff, esz) = crate::collections::entry_layout(k, v);
        let (koff, voff, esz) = (koff as i32, voff as i32, esz as i32);
        self.f.instructions().local_get(eh).i32_const(koff).i32_add();
        self.emit_release_slot_at(k);
        self.f.instructions().local_get(eh).i32_const(voff).i32_add();
        self.emit_release_slot_at(v);
        let mut i = self.f.instructions();
        // memmove(eh, eh + esz, end - (eh + esz)), end = mh + PAYLOAD + len
        i.local_get(eh).local_get(eh).i32_const(esz).i32_add();
        i.local_get(mh).i32_const(almide_layout::PAYLOAD as i32).i32_add().local_get(mh).i32_load(len_memarg()).i32_add();
        i.local_get(eh).i32_sub().i32_const(esz).i32_sub();
        i.memory_copy(0, 0);
        i.local_get(mh).local_get(mh).i32_load(len_memarg()).i32_const(esz).i32_sub().i32_store(len_memarg());
        if let Some(forget) = forget {
            i.local_get(mh).i32_const(0).call(forget).drop();
        }
    }

    /// The judged delete over a receiver `mh` whose ONE credit the caller
    /// holds (see the module doc); `mh` holds the result.
    fn emit_map_remove_judged(&mut self, h: RemoveHolds, (k, v): (SliceTy, SliceTy), fns: RemoveFns) -> Result<(), EmitError> {
        let RemoveHolds { mh, eh } = h;
        let rc = MemArg { offset: u64::from(almide_layout::RC.offset), align: 2, memory_index: 0 };
        let oh = self.hold_i32()?;
        {
            let mut i = self.f.instructions();
            i.local_get(eh).if_(BlockType::Empty);
            i.local_get(mh).global_get(G_LINE_END).i32_ge_u();
            i.local_get(mh).i32_load(rc).i32_const(1).i32_eq();
            i.i32_and().if_(BlockType::Empty);
        }
        match fns.tomb {
            Some(tomb) => self.emit_map_remove_hole(h, k, v, fns, tomb),
            None => self.emit_map_remove_shift(h, k, v, fns.forget),
        }
        self.f.instructions().else_();
        self.emit_map_remove_copy(RemoveHolds { mh, eh }, k, v)?;
        let mut i = self.f.instructions();
        i.local_set(oh).local_get(mh).call(fns.drop_map).local_get(oh).local_set(mh);
        i.end().end();
        self.release_i32(); // oh
        Ok(())
    }

    /// Scan `mh` for the key in `kh` into `eh`, then the judged delete.
    fn emit_map_remove_scanned(&mut self, mh: u32, kh: u32, kv: (SliceTy, SliceTy), fns: RemoveFns) -> Result<(), EmitError> {
        let (koff, _, esz) = crate::collections::entry_layout(kv.0, kv.1);
        let eh = self.hold_i32()?;
        self.f
            .instructions()
            .local_get(mh)
            .i32_const(esz as i32)
            .i32_const(koff as i32)
            .local_get(kh)
            .call(fns.scan)
            .local_set(eh);
        self.emit_map_remove_judged(RemoveHolds { mh, eh }, kv, fns)?;
        self.release_i32(); // eh
        Ok(())
    }

    /// `map.remove(m, k)` over a receiver whose credit the op may take
    /// (`owned_receiver`); `None` keeps the borrowed-receiver copy.
    pub(crate) fn try_map_remove_owned(&mut self, m: &IrExpr, key: &IrExpr) -> Result<Option<Lowered>, EmitError> {
        let Some(dying) = self.owned_receiver(m, &[key]) else { return Ok(None) };
        let hole = self.hole_lane(m, dying);
        let got = self.lower_remove_receiver(m, dying, hole)?;
        let SliceTy::Map(kt, vt) = got else {
            return unsup(&format!("map-op-of:{got:?}"));
        };
        let (k, v) = (self.types.el(kt), self.types.el(vt));
        let mh = self.hold_i32()?;
        self.f.instructions().local_set(mh);
        if let Some((id, idx, _)) = dying {
            self.empty_dying(id, idx);
        }
        let mut fns = self.map_remove_fns(k, v)?;
        if hole {
            fns.tomb = self.keyed_kill(k);
        }
        let kh = self.hold_for(k)?;
        self.lower_arg(key, Some(k), ArgMode::Borrow)?;
        self.f.instructions().local_set(kh);
        self.emit_map_remove_scanned(mh, kh, (k, v), fns)?;
        self.f.instructions().local_get(mh);
        self.release_for(k);
        self.release_i32(); // mh
        Ok(Some(Lowered::owned(got)))
    }

    /// `set.remove(s, x)` over a receiver whose credit the op holds — the
    /// map judge above with members for entries (`@reuse(s)`; native's
    /// `almide_rt_set_remove_owned`). An absent member returns the receiver
    /// itself; a present one at rc 1 shifts out in place and retires the
    /// index; a shared block takes the functional copy and drops the op's
    /// credit on the original. `None` keeps the borrowed-receiver copy.
    pub(crate) fn try_set_remove_owned(&mut self, s: &IrExpr, x: &IrExpr) -> Result<Option<Lowered>, EmitError> {
        let Some(dying) = self.owned_receiver(s, &[x]) else { return Ok(None) };
        let hole = self.hole_lane(s, dying);
        let got = self.lower_remove_receiver(s, dying, hole)?;
        let SliceTy::Set(h) = got else {
            return unsup(&format!("set-op-of:{got:?}"));
        };
        let e = self.types.el(h);
        let sh = self.hold_i32()?;
        self.f.instructions().local_set(sh);
        if let Some((id, idx, _)) = dying {
            self.empty_dying(id, idx);
        }
        let scan = self.keyed_find(e)?;
        let forget = self.keyed_remove(e);
        let tomb = if hole { self.keyed_kill(e) } else { None };
        let drop_set = self.dec_fn_of(got);
        let xh = self.hold_for(e)?;
        self.lower_arg(x, Some(e), ArgMode::Borrow)?;
        self.f.instructions().local_set(xh);
        let eh = self.hold_i32()?;
        let oh = self.hold_i32()?;
        let rc = MemArg { offset: u64::from(almide_layout::RC.offset), align: 2, memory_index: 0 };
        let stride = e.slot_size() as i32;
        {
            let mut i = self.f.instructions();
            i.local_get(sh).i32_const(stride).i32_const(0).local_get(xh).call(scan).local_tee(eh);
            i.if_(BlockType::Empty);
            i.local_get(sh).global_get(G_LINE_END).i32_ge_u();
            i.local_get(sh).i32_load(rc).i32_const(1).i32_eq();
            i.i32_and().if_(BlockType::Empty);
        }
        match tomb {
            // #4208: a tombstone var's tail remove holes the member, as the
            // map's does (stride = the member slot, the key at offset 0)
            Some((kill, settle)) => {
                let mut i = self.f.instructions();
                i.local_get(sh).i32_const(stride).i32_const(0).local_get(eh).call(kill);
                i.if_(BlockType::Empty);
                self.emit_release_and_clear(eh, 0, e);
                let mut i = self.f.instructions();
                i.local_get(sh).i32_const(stride).i32_const(0).i32_const(0).call(settle).drop();
                i.else_();
                self.emit_set_remove_shift(sh, eh, e, forget);
                self.f.instructions().end();
            }
            None => self.emit_set_remove_shift(sh, eh, e, forget),
        }
        self.f.instructions().else_();
        self.emit_set_remove_copy(sh, eh, e)?;
        let mut i = self.f.instructions();
        i.local_set(oh).local_get(sh).call(drop_set).local_get(oh).local_set(sh);
        i.end().end();
        i.local_get(sh);
        self.release_i32(); // oh
        self.release_i32(); // eh
        self.release_for(e);
        self.release_i32(); // sh
        Ok(Some(Lowered::owned(got)))
    }

    /// Remove the member at `eh` from set `sh` IN PLACE: its credit
    /// released, the later members moved down one, `len` shrunk, the index
    /// retired.
    fn emit_set_remove_shift(&mut self, sh: u32, eh: u32, e: SliceTy, forget: Option<u32>) {
        let stride = e.slot_size() as i32;
        self.f.instructions().local_get(eh);
        self.emit_release_slot_at(e);
        let mut i = self.f.instructions();
        // memmove(eh, eh + stride, end - (eh + stride)), end = sh + PAYLOAD + len
        i.local_get(eh).local_get(eh).i32_const(stride).i32_add();
        i.local_get(sh).i32_const(almide_layout::PAYLOAD as i32).i32_add().local_get(sh).i32_load(len_memarg()).i32_add();
        i.local_get(eh).i32_sub().i32_const(stride).i32_sub();
        i.memory_copy(0, 0);
        i.local_get(sh).local_get(sh).i32_load(len_memarg()).i32_const(stride).i32_sub().i32_store(len_memarg());
        if let Some(forget) = forget {
            i.local_get(sh).i32_const(0).call(forget).drop();
        }
    }
}
