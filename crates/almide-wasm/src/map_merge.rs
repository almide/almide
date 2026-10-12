//! The in-place `map.merge` (#3643): `map.set`'s judged write
//! (map_inplace.rs) applied once per entry of the right operand. A left
//! operand whose credit the op holds — an owned temporary, or a dying var:
//! the rebind `m = map.merge(m, b)` in a loop, a self tail call, a fold
//! callback (`@reuse(a)` on the declaration) — used to be copied whole on
//! every call. Now each of b's entries is upserted into it under the judge:
//!
//! * **the credit is the block's only one** (a heap block at rc 1) — a key
//!   the map has takes b's value at its position (the replaced value's
//!   credit released), a new key appends through `$map_reserve`'s geometric
//!   growth and the index hook, exactly native's `AlmideMap::insert` order.
//! * **shared** (an alias, a holder, a capture, a pool static) — the first
//!   entry takes the functional copy and the op's credit on the shared
//!   original goes, so every other holder keeps the pre-merge map (C-033);
//!   the copy is the op's alone, so the remaining entries land in it.
//!
//! The right operand is borrowed and only read: each entry's key and value
//! take their own credit before the judged write stores or releases them.
//! It cannot be the left block at rc 1 — a dying var the right operand
//! reads is not moved (`owned_receiver`), and any other holder of the same
//! block makes it shared. A borrowed left operand keeps the copying merge
//! (collections_hof.rs).

use almide_ir::IrExpr;
use wasm_encoder::BlockType;

use crate::arm::{ArgMode, Lowered};
use crate::emitter::Emitter;
use crate::*;

impl Emitter<'_> {
    /// `map.merge(a, b)` over a left operand whose credit the op may take
    /// (`owned_receiver`); `None` keeps the borrowed-operand copy.
    pub(crate) fn try_map_merge_owned(&mut self, a: &IrExpr, b: &IrExpr) -> Result<Option<Lowered>, EmitError> {
        let Some(dying) = self.owned_receiver(a, &[b]) else { return Ok(None) };
        self.owned_call_marks.set_moving(a, dying.is_some());
        let got = self.lower_arg(a, None, ArgMode::Retain)?;
        self.owned_call_marks.set_moving(a, false);
        let SliceTy::Map(kt, vt) = got else {
            return unsup(&format!("map-op-of:{got:?}"));
        };
        let (k, v) = (self.types.el(kt), self.types.el(vt));
        let mh = self.hold_i32()?;
        self.f.instructions().local_set(mh);
        if let Some((id, idx, _)) = dying {
            self.empty_dying(id, idx);
        }
        let fns = self.map_set_fns(k, v)?;
        if self.lower_arg(b, None, ArgMode::Borrow)? != got {
            return unsup("map-merge-ty");
        }
        let bh = self.hold_i32()?;
        self.f.instructions().local_set(bh);
        self.emit_map_merge_into(mh, bh, (k, v), fns)?;
        self.release_i32(); // bh
        self.f.instructions().local_get(mh);
        self.release_i32(); // mh
        Ok(Some(Lowered::owned(got)))
    }

    /// Upsert every entry of `bh` into `mh` through the judged set; `mh`
    /// holds the result (the same block, grown, or the copy).
    fn emit_map_merge_into(
        &mut self,
        mh: u32,
        bh: u32,
        (k, v): (SliceTy, SliceTy),
        fns: crate::map_inplace::MapSetFns,
    ) -> Result<(), EmitError> {
        let (koff, voff, esz) = crate::collections::entry_layout(k, v);
        let (cur, end) = (self.hold_i32()?, self.hold_i32()?);
        let (kh, vh) = (self.hold_for(k)?, self.hold_for(v)?);
        {
            let mut i = self.f.instructions();
            i.local_get(bh).i32_const(almide_layout::PAYLOAD as i32).i32_add().local_tee(cur);
            i.local_get(bh).i32_load(len_memarg()).i32_add().local_set(end);
            i.block(BlockType::Empty).loop_(BlockType::Empty);
            i.local_get(cur).local_get(end).i32_ge_u().br_if(1);
        }
        // the entry's key and value, each with its own credit
        for (hold, ty, off) in [(kh, k, koff), (vh, v, voff)] {
            self.f.instructions().local_get(cur).i32_const(off as i32).i32_add();
            self.load_ty_slot_at(ty);
            self.share_handle_top(ty);
            self.f.instructions().local_set(hold);
        }
        self.emit_map_set_judged(mh, kh, vh, (k, v), fns)?;
        let mut i = self.f.instructions();
        i.local_get(cur).i32_const(esz as i32).i32_add().local_set(cur);
        i.br(0).end().end();
        self.release_for(v);
        self.release_for(k);
        self.release_i32(); // end
        self.release_i32(); // cur
        Ok(())
    }
}
