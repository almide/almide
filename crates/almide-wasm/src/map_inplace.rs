//! The in-place `map.set` window (#1219 stage 1). `m[k] = v`,
//! `map.insert(m, k, v)` and the rebind `m = map.set(m, k, v)` all
//! funnel here when the receiver is a mutable var the frame owns: the
//! write lands IN the var's block when the block is the var's alone
//! (overwrite the entry, or append through `$map_reserve`'s list-push
//! growth), and takes the functional copy only when the block has been
//! shared. The old per-write copy made every build loop O(n²) in time
//! AND — Maps are never freed on this leg — O(n²) in retained bytes,
//! so a 20k-entry counter loop exhausted the heap where native ran in
//! milliseconds.
//!
//! # The judge
//!
//! Maps stay OFF the droppable set (no drop glue, never dec'd), so the
//! rc field is a MONOTONE sharing witness rather than a live count: it
//! only ever says "a second holder existed at some point". That is
//! exactly the question the window asks — `rc == 1` ⇒ the var's block
//! is its alone. Binds and assigns COPY a map (so two vars never share
//! a block), and every other holder — a record/tuple/variant/list/
//! option slot, a map VALUE slot, a closure env, a for-in cursor —
//! takes the +1 through `rc_share_guard` / `rc_map_value_share` / the
//! for-in subject inc. Params are excluded (a borrowed view of the
//! caller's block), and so are C-319 cells (captured + mutated vars
//! keep the functional rebind through the cell). A block below the heap
//! floor (a pool static) never mutates in place.
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
    /// A Map handle stored into a block slot witnesses a second holder
    /// (see the module doc). Scoped to Map-typed slots so every other
    /// element class keeps its exact pre-#1219 emission.
    pub(crate) fn rc_map_value_share(&mut self, e: &IrExpr, ty: SliceTy) {
        if matches!(ty, SliceTy::Map(..)) {
            self.rc_share_guard(e, ty);
        }
    }

    /// A DROPPABLE key or value (Str / Bytes / List-of-scalar) written
    /// into a map entry is co-owned by the map: +1 on the stored handle.
    /// Before this the entry held a BORROWED handle — a `let`-bound key
    /// (`let k = "k" + int.to_string(i)`) was released at the next loop
    /// rebind and the map kept a dangling pointer that the next
    /// allocation of the same size class overwrote, so `map.keys` read
    /// back the digit strings the loop built next (native: `k0,k1,…`;
    /// the 0.61.1 default leg: `2,3,0,1,…`). Runs on the ALLOCATED
    /// branch only for keys (an overwrite stores no key) and on both for
    /// values; `$inc` no-ops below the heap floor, so literal keys pay
    /// nothing. The overwritten value keeps today's leak (it may still be
    /// co-owned by the tuple `map.from_list` copied it from).
    fn rc_entry_coown(&mut self, hold: u32, ty: SliceTy) {
        if self.rc_droppable(ty) {
            self.f.instructions().local_get(hold).call(F_INC);
        }
    }

    /// The functional `map.set` core: the result block (a copy of the
    /// receiver with the found entry overwritten, or grown by one entry
    /// appended) is left on the stack.
    pub(crate) fn emit_map_set_copy(
        &mut self,
        h: MapSetHolds,
        k: SliceTy,
        v: SliceTy,
        lay: (u32, u32, u32),
    ) -> Result<(), EmitError> {
        let MapSetHolds { mh, kh, eh, vh } = h;
        self.f
            .instructions()
            .local_get(eh)
            .i32_const(0)
            .i32_ne()
            .if_(BlockType::Result(ValType::I32));
        // overwrite in a copy: dest = r + (e - m) + voff
        let (len_h, rh) = self.emit_copy_grow(mh, 0)?;
        self.f
            .instructions()
            .local_get(rh)
            .local_get(eh)
            .i32_add()
            .local_get(mh)
            .i32_sub()
            .i32_const(lay.1 as i32)
            .i32_add()
            .local_get(vh);
        self.store_ty_slot_raw(v);
        self.rc_entry_coown(vh, v);
        self.f.instructions().local_get(rh);
        let _ = len_h;
        self.release_i32();
        self.release_i32();
        self.f.instructions().else_();
        // append a fresh entry at the old end
        let (len_h2, rh2) = self.emit_copy_grow(mh, lay.2)?;
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
        self.rc_entry_coown(kh, k);
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
        self.rc_entry_coown(vh, v);
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
        let scan = self.scan_helper(k)?;
        let reserve = self.work.helper(Helper::MapReserve);
        let kh = self.hold_for(k)?;
        self.lower(key, Some(k))?;
        self.f.instructions().local_set(kh);
        let vh = self.hold_for(v)?;
        self.lower(value, Some(v))?;
        self.rc_map_value_share(value, v);
        self.f.instructions().local_set(vh);
        let mh = self.hold_i32()?;
        self.emit_read_mut_var(id, idx, ty, global);
        self.f.instructions().local_set(mh);
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
            // present: overwrite the value slot in place
            i.local_get(eh).i32_const(voff).i32_add().local_get(vh);
        }
        self.store_ty_slot_raw(v);
        self.rc_entry_coown(vh, v);
        {
            let mut i = self.f.instructions();
            i.else_();
            // absent: room for one entry (in place under class slack,
            // else the doubled block), then the pair at the old end.
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
        self.rc_entry_coown(kh, k);
        self.f.instructions().local_get(eh).i32_const(voff).i32_add().local_get(vh);
        self.store_ty_slot_raw(v);
        self.rc_entry_coown(vh, v);
        {
            let mut i = self.f.instructions();
            i.local_get(mh)
                .local_get(mh)
                .i32_load(len_memarg())
                .i32_const(esz)
                .i32_add()
                .i32_store(len_memarg());
            i.end();
            i.else_();
        }
        // shared (or a static): the functional copy, as before
        self.emit_map_set_copy(MapSetHolds { mh, kh, eh, vh }, k, v, lay)?;
        self.f.instructions().local_set(mh);
        self.f.instructions().end();
        self.f.instructions().local_get(mh);
        self.emit_store_mut_var(*id, idx, ty, global)?;
        self.release_i32(); // eh
        self.release_i32(); // mh
        self.release_for(v);
        self.release_for(k);
        Ok(true)
    }
}
