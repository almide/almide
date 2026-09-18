//! The in-place `map.set` window (#1219 stage 1). `m[k] = v`,
//! `map.insert(m, k, v)` and the rebind `m = map.set(m, k, v)` all
//! funnel here when the receiver is a mutable var the frame owns: the
//! write lands IN the var's block when the block is the var's alone
//! (overwrite the entry, or append through `$map_reserve`'s list-push
//! growth), and takes the functional copy only when the block has been
//! shared. The old per-write copy made every build loop O(n²) in time
//! AND — Maps were never freed on this leg — O(n²) in retained bytes,
//! so a 20k-entry counter loop exhausted the heap where native ran in
//! milliseconds.
//!
//! # The judge
//!
//! Maps are droppable (#2010, Map stage b: the entries array AND every
//! handle key / value are credits the holder releases through
//! `DropEntries`), so the rc field is a LIVE count and `rc == 1` ⇒ the
//! var's block is its alone. Every other holder — a bind of the same
//! block, a record/tuple/variant/list/option slot, a map VALUE slot, a
//! closure env, a for-in cursor — takes the +1 through `rc_share_guard`
//! and releases it when it lets go. Params are excluded (a borrowed view
//! of the caller's block), and so are C-319 cells (captured + mutated
//! vars keep the functional rebind through the cell). A block below the
//! heap floor (a pool static) never mutates in place.
//!
//! # The entry credits
//!
//! The key and the value are lowered under `Retain`: a borrowed handle
//! takes +1, an owned one moves in. An overwrite releases the replaced
//! value's credit and the unstored key's; an append stores both. The
//! functional copy takes the credits of every entry it copied
//! (`$inc_entries`), and the shared original loses the var's credit.
//!
//! Order: key, then value, then the receiver read — a value that reads
//! the map (`m[k] = map.get_or(m, k, 0) + 1`, the counter idiom) sees
//! the pre-write block, exactly as the functional set evaluates, and a
//! var read has no effect so the reordering is unobservable.

use almide_ir::{IrExpr, VarId};
use wasm_encoder::{BlockType, MemArg, ValType};

use crate::emitter::Emitter;
use crate::work::Helper;
use crate::*;

/// The holds a map-set core works on: the receiver block, the key, the
/// scan's ABSOLUTE entry address (0 = absent) and the value.
pub(crate) struct MapSetHolds {
    pub(crate) mh: u32,
    pub(crate) kh: u32,
    pub(crate) eh: u32,
    pub(crate) vh: u32,
}

impl Emitter<'_> {
    /// Release the credit a HOLD carries when its value is a handle the
    /// holder owns (a Retain-lowered key that was not stored, an unused
    /// init) — nothing for a flat value.
    pub(crate) fn emit_release_hold(&mut self, hold: u32, ty: SliceTy) {
        if self.elem_is_handle(ty) {
            let dec = self.dec_fn_of(ty);
            self.f.instructions().local_get(hold).call(dec);
        }
    }

    /// Release the handle stored in the slot whose ABSOLUTE address is on
    /// the stack (the entry value an overwrite replaces); consumes the
    /// address either way.
    pub(crate) fn emit_release_slot_at(&mut self, ty: SliceTy) {
        if self.elem_is_handle(ty) {
            let dec = self.dec_fn_of(ty);
            self.f.instructions().i32_load(MemArg { offset: 0, align: 2, memory_index: 0 }).call(dec);
        } else {
            self.f.instructions().drop();
        }
    }

    /// The functional `map.set` core: the result block (a copy of the
    /// receiver with the found entry overwritten, or grown by one entry
    /// appended) is left on the stack. The key and value holds carry
    /// Retain credits.
    pub(crate) fn emit_map_set_copy(
        &mut self,
        h: MapSetHolds,
        k: SliceTy,
        v: SliceTy,
        lay: (u32, u32, u32),
    ) -> Result<(), EmitError> {
        let MapSetHolds { mh, kh, eh, vh } = h;
        let mt = SliceTy::Map(self.types.intern(k), self.types.intern(v));
        self.f
            .instructions()
            .local_get(eh)
            .i32_const(0)
            .i32_ne()
            .if_(BlockType::Result(ValType::I32));
        // overwrite in a copy: dest = r + (e - m) + voff
        let (len_h, rh) = self.emit_copy_grow(mh, 0)?;
        self.emit_inc_entries(rh, mt, None);
        for _ in 0..2 {
            self.f
                .instructions()
                .local_get(rh)
                .local_get(eh)
                .i32_add()
                .local_get(mh)
                .i32_sub()
                .i32_const(lay.1 as i32)
                .i32_add();
        }
        self.emit_release_slot_at(v);
        self.f.instructions().local_get(vh);
        self.store_ty_slot_raw(v);
        self.emit_release_hold(kh, k);
        self.f.instructions().local_get(rh);
        let _ = len_h;
        self.release_i32();
        self.release_i32();
        self.f.instructions().else_();
        // append a fresh entry at the old end
        let (len_h2, rh2) = self.emit_copy_grow(mh, lay.2)?;
        self.emit_inc_entries(rh2, mt, Some(len_h2));
        self.f
            .instructions()
            .local_get(rh2)
            .i32_const(almide_layout::PAYLOAD as i32)
            .i32_add()
            .local_get(len_h2)
            .i32_add()
            .i32_const(lay.0 as i32)
            .i32_add()
            .local_get(kh);
        self.store_ty_slot_raw(k);
        self.f
            .instructions()
            .local_get(rh2)
            .i32_const(almide_layout::PAYLOAD as i32)
            .i32_add()
            .local_get(len_h2)
            .i32_add()
            .i32_const(lay.1 as i32)
            .i32_add()
            .local_get(vh);
        self.store_ty_slot_raw(v);
        self.f.instructions().local_get(rh2);
        self.release_i32();
        self.release_i32();
        self.f.instructions().end();
        Ok(())
    }

    /// The window. `Ok(false)` when the receiver does not qualify — the
    /// caller keeps its functional set + var write-back.
    pub(crate) fn try_map_set_in_place(
        &mut self,
        id: &VarId,
        key: &IrExpr,
        value: &IrExpr,
    ) -> Result<bool, EmitError> {
        if self.metered || self.cells.contains(id) {
            return Ok(false);
        }
        let Some((idx, ty, global)) = self.mut_var(id) else {
            return Ok(false);
        };
        if !global && idx < self.rc_param_ceiling {
            return Ok(false);
        }
        let SliceTy::Map(kt, vt) = ty else {
            return Ok(false);
        };
        let (k, v) = (self.types.el(kt), self.types.el(vt));
        let lay = crate::collections::entry_layout(k, v);
        let (koff, voff, esz) = (lay.0 as i32, lay.1 as i32, lay.2 as i32);
        // the var's block is stable across the loop: the index lane, and
        // its append hook after the store (#1219 stage 2)
        let scan = self.keyed_find(k)?;
        let append = self.keyed_append(k);
        let reserve = self.work.helper(Helper::MapReserve);
        let drop_map = self.dec_fn_of(ty);
        let kh = self.hold_for(k)?;
        self.lower_arg(key, Some(k), ArgMode::Retain)?;
        self.f.instructions().local_set(kh);
        let vh = self.hold_for(v)?;
        self.lower_arg(value, Some(v), ArgMode::Retain)?;
        self.f.instructions().local_set(vh);
        let mh = self.hold_i32()?;
        self.emit_read_mut_var(id, idx, ty, global);
        self.f.instructions().local_set(mh);
        let oh = self.hold_i32()?;
        let eh = self.hold_i32()?;
        self.f
            .instructions()
            .local_get(mh)
            .i32_const(esz)
            .i32_const(koff)
            .local_get(kh)
            .call(scan)
            .local_set(eh);
        let rc = MemArg { offset: u64::from(almide_layout::RC.offset), align: 2, memory_index: 0 };
        {
            let mut i = self.f.instructions();
            // the judge: a heap block whose only holder is this var
            i.local_get(mh).global_get(G_LINE_END).i32_ge_u();
            i.local_get(mh).i32_load(rc).i32_const(1).i32_eq();
            i.i32_and().if_(BlockType::Empty);
            i.local_get(eh).if_(BlockType::Empty);
            // present: release the replaced value, overwrite the slot in
            // place; the key was not stored, its credit goes back
            i.local_get(eh).i32_const(voff).i32_add();
        }
        self.emit_release_slot_at(v);
        self.f.instructions().local_get(eh).i32_const(voff).i32_add().local_get(vh);
        self.store_ty_slot_raw(v);
        self.emit_release_hold(kh, k);
        {
            let mut i = self.f.instructions();
            i.else_();
            // absent: room for one entry (in place under class slack,
            // else the doubled block), then the pair at the old end.
            i.local_get(mh).local_set(oh);
            i.local_get(mh).i32_const(esz).call(reserve).local_set(mh);
            i.local_get(mh)
                .i32_const(almide_layout::PAYLOAD as i32)
                .i32_add()
                .local_get(mh)
                .i32_load(len_memarg())
                .i32_add()
                .local_set(eh);
            i.local_get(eh).i32_const(koff).i32_add().local_get(kh);
        }
        self.store_ty_slot_raw(k);
        self.f.instructions().local_get(eh).i32_const(voff).i32_add().local_get(vh);
        self.store_ty_slot_raw(v);
        {
            let mut i = self.f.instructions();
            i.local_get(mh)
                .local_get(mh)
                .i32_load(len_memarg())
                .i32_const(esz)
                .i32_add()
                .i32_store(len_memarg());
            // the index follows the entry (and the block, if it moved)
            if let Some(append) = append {
                i.local_get(oh).local_get(mh).i32_const(esz).i32_const(koff).call(append).drop();
            }
            i.end();
            i.else_();
        }
        // shared (or a static): the functional copy, and the var's credit
        // on the shared original goes with the rebind
        self.emit_map_set_copy(MapSetHolds { mh, kh, eh, vh }, k, v, lay)?;
        self.f.instructions().local_set(oh);
        self.f.instructions().local_get(mh).call(drop_map);
        self.f.instructions().local_get(oh).local_set(mh);
        self.f.instructions().end();
        self.f.instructions().local_get(mh);
        self.emit_store_mut_var(*id, idx, ty, global)?;
        self.release_i32(); // eh
        self.release_i32(); // oh
        self.release_i32(); // mh
        self.release_for(v);
        self.release_for(k);
        Ok(true)
    }
}
