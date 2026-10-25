//! `json.to_map` on this backend's native Value layout (value.rs): an
//! Object Value's pairs as a `Map[String, String]`.

use almide_ir::IrExpr;
use wasm_encoder::BlockType;

use crate::emitter::Emitter;
use crate::value::{value_tags, VT_STR};
use crate::*;

impl Emitter<'_> {
    /// #1423 bucket A — `json.to_map`: an OBJECT Value's pairs become a
    /// `Map[String, String]` (a Str value's payload verbatim, any other
    /// value stringified — semantics verbatim from
    /// `runtime/rs/src/json.rs::almide_json_to_map`); a non-object is
    /// `none`. Each pair is ONE judged upsert (C-380): an object that
    /// repeats a key yields that key once, at its first position, holding
    /// its last value — the Map invariant native's `collect` keeps. Writing
    /// one entry per pair put the key in the map twice (#3700).
    #[allow(clippy::too_many_lines)]
    pub(crate) fn lower_json_to_map(
        &mut self,
        j: &IrExpr,
    ) -> ArmResult {
        self.lower_arg(j, Some(SliceTy::Value), ArgMode::Borrow)?;
        let ti = self.types.tuple(vec![STR, SliceTy::Value]);
        let def = self.types.tuple_def(ti);
        let (key_off, val_off) = (def.fields[0].1, def.fields[1].1);
        let fns = self.map_set_fns(STR, STR)?;
        let hv = self.hold_i32()?;
        let hp = self.hold_i32()?;
        let hend = self.hold_i32()?;
        let hout = self.hold_i32()?;
        let hpair = self.hold_i32()?;
        let kh = self.hold_for(STR)?;
        let vh = self.hold_for(STR)?;
        {
            let mut i = self.f.instructions();
            i.local_set(hv);
            i.local_get(hv)
                .i32_load(slot_memarg(almide_layout::SUM_TAG))
                .i32_const(value_tags::VT_OBJECT)
                .i32_ne()
                .if_(BlockType::Result(wasm_encoder::ValType::I32));
            i.i32_const(almide_layout::NULL_ADDR as i32);
            i.else_();
            // pairs list: n 4-byte slots, each a (key, value) pair block.
            i.local_get(hv).i32_load(slot_memarg(almide_layout::SUM_FIELD)).local_set(hv);
            i.local_get(hv).i32_const(almide_layout::PAYLOAD as i32).i32_add().local_set(hp);
            i.local_get(hp).local_get(hv).i32_load(len_memarg()).i32_add().local_set(hend);
            // out map: empty, grown by the judged set's `$map_reserve`.
            i.i32_const(0).call(F_ALLOC).local_set(hout);
            i.block(BlockType::Empty).loop_(BlockType::Empty);
            i.local_get(hp).local_get(hend).i32_ge_u().br_if(1);
            i.local_get(hp).i32_load(raw_mem()).local_set(hpair);
            i.local_get(hpair).i32_load(slot_memarg(key_off));
        }
        // the map co-owns the key string it copied out of the pair (its
        // typed drop releases it; the Value keeps its own credit). On an
        // overwrite the judged set gives back the unstored key's share.
        self.rc_inc_top();
        {
            let mut i = self.f.instructions();
            i.local_set(kh);
            // value: Str payload verbatim, else the canonical stringify.
            i.local_get(hpair).i32_load(slot_memarg(val_off)).local_set(hpair);
            i.local_get(hpair)
                .i32_load(slot_memarg(almide_layout::SUM_TAG))
                .i32_const(VT_STR)
                .i32_eq()
                .if_(BlockType::Result(wasm_encoder::ValType::I32));
            i.local_get(hpair).i32_load(slot_memarg(almide_layout::SUM_FIELD));
        }
        // a Str payload is shared with the Value: the map takes +1
        self.rc_inc_top();
        {
            let mut i = self.f.instructions();
            i.else_();
            i.local_get(hpair);
        }
        self.emit_value_stringify()?;
        // the replaced value's share goes back on an overwrite, too
        self.f.instructions().end().local_set(vh);
        self.emit_map_set_judged(hout, kh, vh, (STR, STR), fns)?;
        {
            let mut i = self.f.instructions();
            i.local_get(hp).i32_const(4).i32_add().local_set(hp);
            i.br(0).end().end();
            // some(map): the Option block holding the map.
            i.i32_const(4)
                .call(F_ALLOC)
                .local_tee(hv)
                .local_get(hout)
                .i32_store(slot_memarg(almide_layout::OPTION_FIELD));
            i.local_get(hv);
            i.end();
        }
        self.release_for(STR); // vh
        self.release_for(STR); // kh
        for _ in 0..5 {
            self.release_i32();
        }
        let sh = self.types.intern(STR);
        let mh = self.types.intern(SliceTy::Map(sh, sh));
        Ok(Some(Lowered::owned(SliceTy::Option(mh))))
    }
}

/// Raw 4-byte addressing for pointers that already carry the payload
/// offset (the pairs-list walk) — `slot_memarg` would double-shift.
fn raw_mem() -> wasm_encoder::MemArg {
    wasm_encoder::MemArg { offset: 0, align: 2, memory_index: 0 }
}
