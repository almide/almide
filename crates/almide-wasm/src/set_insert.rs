//! The in-place `set.insert` (#3552), `map.set`'s window (map_inplace.rs)
//! applied to a set. Every insert used to copy the whole set, so building one
//! member at a time — `s = set.insert(s, x)` in a loop, a fold seeded with a
//! set — was O(n²) on wasm. `set.insert` is declared `@reuse(s)`; when the op
//! holds the receiver's credit (a dying var, or any non-var receiver lowered
//! retained, `judged_receiver`), the judge decides:
//!
//! * **present** — nothing changes: the receiver IS the result, and the
//!   unstored member's credit goes back.
//! * **absent, the credit is the block's only one** (a heap block at rc 1) —
//!   room for one member through `$map_reserve` (class slack, else the
//!   doubled block), the member at the old end, and the key index follows
//!   the block (`keyed_append`), as `set.from_list` builds it.
//! * **absent, shared** (an alias, a holder, a pool static) — the functional
//!   copy with the member, and the op's credit on the shared original goes:
//!   every other holder keeps the set it had.
//!
//! A borrowed receiver (a live var) keeps the functional copy in
//! collections_set.rs.

use almide_ir::IrExpr;
use wasm_encoder::{BlockType, MemArg};

use crate::arm::{ArgMode, Lowered};
use crate::emitter::Emitter;
use crate::*;

impl Emitter<'_> {
    pub(crate) fn try_set_insert_owned(&mut self, s: &IrExpr, x: &IrExpr) -> Result<Option<Lowered>, EmitError> {
        let Some(dying) = self.judged_receiver(s, &[x]) else { return Ok(None) };
        self.owned_call_marks.set_moving(s, dying.is_some());
        let got = self.lower_arg(s, None, ArgMode::Retain)?;
        self.owned_call_marks.set_moving(s, false);
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
        let append = self.keyed_append(e);
        let reserve = self.work.helper(crate::work::Helper::MapReserve);
        let drop_set = self.dec_fn_of(got);
        // the member's credit moves into its slot when it is stored
        let xh = self.hold_for(e)?;
        self.lower_arg(x, Some(e), ArgMode::Retain)?;
        self.f.instructions().local_set(xh);
        let stride = e.slot_size() as i32;
        let rc = MemArg { offset: u64::from(almide_layout::RC.offset), align: 2, memory_index: 0 };
        let oh = self.hold_i32()?;
        {
            let mut i = self.f.instructions();
            i.local_get(sh).i32_const(stride).i32_const(0).local_get(xh).call(scan).i32_const(0).i32_ne();
            i.if_(BlockType::Empty);
        }
        // present: the set is the result as it came
        self.emit_release_hold(xh, e);
        {
            let mut i = self.f.instructions();
            i.else_();
            i.local_get(sh).global_get(G_LINE_END).i32_ge_u();
            i.local_get(sh).i32_load(rc).i32_const(1).i32_eq();
            i.i32_and().if_(BlockType::Empty);
            // ours alone: room for one member, then the member at the old end
            i.local_get(sh).local_set(oh);
            i.local_get(sh).i32_const(stride).call(reserve).local_tee(sh);
            i.i32_const(almide_layout::PAYLOAD as i32).i32_add().local_get(sh).i32_load(len_memarg()).i32_add();
            i.local_get(xh);
        }
        self.store_ty_slot_raw(e);
        {
            let mut i = self.f.instructions();
            i.local_get(sh).local_get(sh).i32_load(len_memarg()).i32_const(stride).i32_add().i32_store(len_memarg());
            // the index follows the member (and the block, if it moved)
            if let Some(append) = append {
                i.local_get(oh).local_get(sh).i32_const(stride).i32_const(0).call(append).drop();
            }
            i.else_();
        }
        // shared: the functional copy with the member; the op's credit on
        // the shared original goes with it
        let (len_h, rh) = self.emit_copy_grow(sh, stride as u32)?;
        self.emit_inc_entries(rh, got, Some(len_h));
        self.f
            .instructions()
            .local_get(rh)
            .i32_const(almide_layout::PAYLOAD as i32)
            .i32_add()
            .local_get(len_h)
            .i32_add()
            .local_get(xh);
        self.store_ty_slot_raw(e);
        {
            let mut i = self.f.instructions();
            i.local_get(sh).call(drop_set);
            i.local_get(rh).local_set(sh);
        }
        self.release_i32(); // rh
        self.release_i32(); // len_h
        self.f.instructions().end().end();
        self.f.instructions().local_get(sh);
        self.release_i32(); // oh
        self.release_for(e);
        self.release_i32(); // sh
        Ok(Some(Lowered::owned(got)))
    }
}
