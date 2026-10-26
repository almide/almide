//! `bytes.split` — the List[Bytes] splitter (#1423 stage 4). The self-host
//! twin (`stdlib/bytes_core.almd`'s split) builds its
//! result through `prim.alloc_list_str` + `prim.store_str(lh + 12 + idx *
//! 8, ..)` — the incumbent's 8-byte slot stride written RAW, where this
//! layout packs a List[Bytes] as 4-byte handles — so it cannot link
//! (the coupled-type proxy is right about it). It is a native arm in
//! the `chunks` shape instead: two passes over the payload (count, then
//! fill), every piece a fresh block copied out of the source, handles
//! stored into a list block sized from the count. Semantics are
//! `runtime/rs/src/bytes.rs` verbatim: `split` on an EMPTY separator is
//! the one-element list holding a copy of the input, otherwise a
//! non-overlapping left-to-right scan (KMP searches, crate::kmp) whose
//! tail piece (possibly empty) is always pushed.

use almide_ir::IrExpr;
use wasm_encoder::BlockType;

use crate::bytes::BYTES;
use crate::emitter::Emitter;
use crate::*;

/// The piece sink: the source block `hb`, the list block `ho`, the
/// write cursor `hw` (a byte offset into the list payload) and a scratch
/// local `hp` for the freshly allocated piece.
struct PieceSink {
    hb: u32,
    ho: u32,
    hw: u32,
    hp: u32,
}

impl Emitter<'_> {
    /// Copy the payload window `[start, end)` of the sink's source block
    /// into a fresh block and store its handle at the sink's cursor, then
    /// advance the cursor by one 4-byte slot.
    fn emit_bytes_piece(&mut self, sink: &PieceSink, start: u32, end: u32) {
        let PieceSink { hb, ho, hw, hp } = *sink;
        let mut i = self.f.instructions();
        i.local_get(end).local_get(start).i32_sub().call(F_ALLOC).local_set(hp);
        i.local_get(hp).i32_const(almide_layout::PAYLOAD as i32).i32_add();
        i.local_get(hb).i32_const(almide_layout::PAYLOAD as i32).i32_add().local_get(start).i32_add();
        i.local_get(hp).i32_load(len_memarg());
        i.memory_copy(0, 0);
        i.local_get(ho).local_get(hw).i32_add().local_get(hp).i32_store(slot_memarg(0));
        i.local_get(hw).i32_const(4).i32_add().local_set(hw);
    }

    /// split: `b.split(sep)` — an empty separator yields `[copy of b]`;
    /// otherwise the non-overlapping left-to-right scan, the tail piece
    /// always pushed (native `out.push(b[start..].to_vec())`).
    pub(crate) fn lower_bytes_split(&mut self, b: &IrExpr, sep: &IrExpr) -> ArmResult {
        self.lower_arg(b, Some(BYTES), ArgMode::Borrow)?;
        let hb = self.hold_i32()?;
        self.f.instructions().local_set(hb);
        self.lower_arg(sep, Some(BYTES), ArgMode::Borrow)?;
        let hsep = self.hold_i32()?;
        let hn = self.hold_i32()?;
        let hm = self.hold_i32()?;
        let hc = self.hold_i32()?;
        let hi = self.hold_i32()?;
        let hj = self.hold_i32()?;
        let hf = self.hold_i32()?;
        let hs = self.hold_i32()?;
        let ho = self.hold_i32()?;
        let hw = self.hold_i32()?;
        let hp = self.hold_i32()?;
        let ht = self.hold_i32()?;
        let hk = self.hold_i32()?;
        let hx = self.hold_i32()?;
        let mut i = self.f.instructions();
        i.local_set(hsep);
        i.local_get(hb).i32_load(len_memarg()).local_set(hn);
        i.local_get(hsep).i32_load(len_memarg()).local_set(hm);
        i.local_get(hm).i32_eqz().if_(BlockType::Empty);
        // [copy of b]
        i.i32_const(4).call(F_ALLOC).local_set(ho);
        i.local_get(ho).local_get(hb).call(F_BLOCK_COPY).i32_store(slot_memarg(0));
        i.else_();
        // Both passes are forward walks of KMP searches, each resuming past
        // the last separator with one table (crate::kmp, #4206). `hf` holds
        // a match start, `hj` is the search cursor.
        let kmp = crate::kmp::Kmp { hay: hb, n: hn, pat: hsep, m: hm, tbl: ht, k: hk, c: hx, q: hj };
        kmp.emit_table(&mut i);
        // pass 1: count the non-overlapping matches
        i.i32_const(0).local_set(hc);
        i.i32_const(0).local_set(hi);
        i.block(BlockType::Empty).loop_(BlockType::Empty);
        i.local_get(hi).local_set(hj);
        kmp.emit_find(&mut i, hf);
        i.local_get(hf).i32_const(0).i32_lt_s().br_if(1);
        i.local_get(hc).i32_const(1).i32_add().local_set(hc);
        i.local_get(hf).local_get(hm).i32_add().local_set(hi);
        i.br(0).end().end();
        // the list block: (matches + 1) × 4-byte handles
        i.local_get(hc).i32_const(1).i32_add().i32_const(4).i32_mul().call(F_ALLOC).local_set(ho);
        i.i32_const(0).local_set(hw);
        i.i32_const(0).local_set(hs);
        // pass 2: a piece [start, match) per match, then the tail
        i.block(BlockType::Empty).loop_(BlockType::Empty);
        i.local_get(hs).local_set(hj);
        kmp.emit_find(&mut i, hf);
        i.local_get(hf).i32_const(0).i32_lt_s().br_if(1);
        let _ = i;
        let sink = PieceSink { hb, ho, hw, hp };
        self.emit_bytes_piece(&sink, hs, hf);
        let mut i = self.f.instructions();
        i.local_get(hf).local_get(hm).i32_add().local_set(hs);
        i.br(0).end().end();
        kmp.emit_free(&mut i);
        let _ = i;
        self.emit_bytes_piece(&sink, hs, hn);
        let mut i = self.f.instructions();
        i.end();
        i.local_get(ho);
        let _ = i;
        for _ in 0..15 {
            self.release_i32();
        }
        Ok(Some(Lowered::owned(SliceTy::List(self.types.intern(BYTES)))))
    }
}
