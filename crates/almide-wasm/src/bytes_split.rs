//! `bytes.split` / `bytes.lines` — the List[Bytes] splitters (#1423
//! stage 4). The self-host twins in `stdlib/bytes_split.almd` build their
//! result through `prim.alloc_list_str` + `prim.store_str(lh + 12 + idx *
//! 8, ..)` — the incumbent's 8-byte slot stride written RAW, where this
//! layout packs a List[Bytes] as 4-byte handles — so they cannot link
//! (the coupled-type proxy is right about them). Both are native arms in
//! the `chunks` shape instead: two passes over the payload (count, then
//! fill), every piece a fresh block copied out of the source, handles
//! stored into a list block sized from the count. Semantics are
//! `runtime/rs/src/bytes.rs` verbatim: `split` on an EMPTY separator is
//! the one-element list holding a copy of the input, otherwise a
//! non-overlapping left-to-right scan whose tail piece (possibly empty)
//! is always pushed; `lines` splits on `\n` (no `\r` handling) and pushes
//! the final tail only when it is non-empty.

use almide_ir::IrExpr;
use wasm_encoder::BlockType;

use crate::bytes::{byte_at, BYTES};
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

/// The separator scan: haystack `hb`, needle `hsep` of length `hm`, and
/// two scratch locals (`hj` the byte index, `hf` the match flag).
struct SepScan {
    hb: u32,
    hsep: u32,
    hm: u32,
    hj: u32,
    hf: u32,
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

    /// lines: `b.lines()` — pieces between `\n` bytes, the trailing
    /// piece only when non-empty (native `if start < b.len()`).
    pub(crate) fn lower_bytes_lines(&mut self, b: &IrExpr) -> Result<Option<SliceTy>, EmitError> {
        self.lower(b, Some(BYTES))?;
        let hb = self.hold_i32()?;
        let hn = self.hold_i32()?;
        let hc = self.hold_i32()?;
        let hs = self.hold_i32()?;
        let he = self.hold_i32()?;
        let ho = self.hold_i32()?;
        let hw = self.hold_i32()?;
        let hp = self.hold_i32()?;
        let mut i = self.f.instructions();
        i.local_set(hb);
        i.local_get(hb).i32_load(len_memarg()).local_set(hn);
        // pass 1: count = #'\n' + (n > 0 && last != '\n')
        i.i32_const(0).local_set(hc);
        i.i32_const(0).local_set(hs);
        i.block(BlockType::Empty).loop_(BlockType::Empty);
        i.local_get(hs).local_get(hn).i32_ge_u().br_if(1);
        i.local_get(hb).local_get(hs).i32_add().i32_load8_u(byte_at()).i32_const(10).i32_eq();
        i.local_get(hc).i32_add().local_set(hc);
        i.local_get(hs).i32_const(1).i32_add().local_set(hs);
        i.br(0).end().end();
        i.local_get(hn).i32_const(0).i32_gt_u();
        i.local_get(hb).local_get(hn).i32_add().i32_const(1).i32_sub().i32_load8_u(byte_at());
        i.i32_const(10).i32_ne();
        i.i32_and().local_get(hc).i32_add().local_set(hc);
        // the list block: count × 4-byte handles
        i.local_get(hc).i32_const(4).i32_mul().call(F_ALLOC).local_set(ho);
        i.i32_const(0).local_set(hw);
        i.i32_const(0).local_set(hs);
        // pass 2: each piece [start, next '\n' or n)
        i.block(BlockType::Empty).loop_(BlockType::Empty);
        i.local_get(hs).local_get(hn).i32_ge_u().br_if(1);
        i.local_get(hs).local_set(he);
        i.block(BlockType::Empty).loop_(BlockType::Empty);
        i.local_get(he).local_get(hn).i32_ge_u().br_if(1);
        i.local_get(hb).local_get(he).i32_add().i32_load8_u(byte_at()).i32_const(10).i32_eq().br_if(1);
        i.local_get(he).i32_const(1).i32_add().local_set(he);
        i.br(0).end().end();
        let _ = i;
        self.emit_bytes_piece(&PieceSink { hb, ho, hw, hp }, hs, he);
        let mut i = self.f.instructions();
        i.local_get(he).i32_const(1).i32_add().local_set(hs);
        i.br(0).end().end();
        i.local_get(ho);
        let _ = i;
        for _ in 0..8 {
            self.release_i32();
        }
        Ok(Some(SliceTy::List(self.types.intern(BYTES))))
    }

    /// Leaves `1` on the stack when the needle's bytes equal the haystack
    /// payload at offset `hi`, else `0`.
    fn emit_bytes_sep_match(&mut self, scan: &SepScan, hi: u32) {
        let SepScan { hb, hsep, hm, hj, hf } = *scan;
        let mut i = self.f.instructions();
        i.i32_const(1).local_set(hf);
        i.i32_const(0).local_set(hj);
        i.block(BlockType::Empty).loop_(BlockType::Empty);
        i.local_get(hj).local_get(hm).i32_ge_u().br_if(1);
        i.local_get(hb).local_get(hi).i32_add().local_get(hj).i32_add().i32_load8_u(byte_at());
        i.local_get(hsep).local_get(hj).i32_add().i32_load8_u(byte_at());
        i.i32_ne().if_(BlockType::Empty);
        i.i32_const(0).local_set(hf);
        i.br(2);
        i.end();
        i.local_get(hj).i32_const(1).i32_add().local_set(hj);
        i.br(0).end().end();
        i.local_get(hf);
    }

    /// split: `b.split(sep)` — an empty separator yields `[copy of b]`;
    /// otherwise the non-overlapping left-to-right scan, the tail piece
    /// always pushed (native `out.push(b[start..].to_vec())`).
    pub(crate) fn lower_bytes_split(&mut self, b: &IrExpr, sep: &IrExpr) -> Result<Option<SliceTy>, EmitError> {
        self.lower(b, Some(BYTES))?;
        let hb = self.hold_i32()?;
        self.f.instructions().local_set(hb);
        self.lower(sep, Some(BYTES))?;
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
        let mut i = self.f.instructions();
        i.local_set(hsep);
        i.local_get(hb).i32_load(len_memarg()).local_set(hn);
        i.local_get(hsep).i32_load(len_memarg()).local_set(hm);
        i.local_get(hm).i32_eqz().if_(BlockType::Empty);
        // [copy of b]
        i.i32_const(4).call(F_ALLOC).local_set(ho);
        i.local_get(ho).local_get(hb).call(F_BLOCK_COPY).i32_store(slot_memarg(0));
        i.else_();
        // pass 1: count the non-overlapping matches
        i.i32_const(0).local_set(hc);
        i.i32_const(0).local_set(hi);
        i.block(BlockType::Empty).loop_(BlockType::Empty);
        i.local_get(hi).local_get(hm).i32_add().local_get(hn).i32_gt_u().br_if(1);
        let _ = i;
        let scan = SepScan { hb, hsep, hm, hj, hf };
        self.emit_bytes_sep_match(&scan, hi);
        let mut i = self.f.instructions();
        i.if_(BlockType::Empty);
        i.local_get(hc).i32_const(1).i32_add().local_set(hc);
        i.local_get(hi).local_get(hm).i32_add().local_set(hi);
        i.else_();
        i.local_get(hi).i32_const(1).i32_add().local_set(hi);
        i.end();
        i.br(0).end().end();
        // the list block: (matches + 1) × 4-byte handles
        i.local_get(hc).i32_const(1).i32_add().i32_const(4).i32_mul().call(F_ALLOC).local_set(ho);
        i.i32_const(0).local_set(hw);
        i.i32_const(0).local_set(hs);
        i.i32_const(0).local_set(hi);
        // pass 2: a piece [start, match) per match, then the tail
        i.block(BlockType::Empty).loop_(BlockType::Empty);
        i.local_get(hi).local_get(hm).i32_add().local_get(hn).i32_gt_u().br_if(1);
        let _ = i;
        self.emit_bytes_sep_match(&scan, hi);
        let sink = PieceSink { hb, ho, hw, hp };
        let mut i = self.f.instructions();
        i.if_(BlockType::Empty);
        let _ = i;
        self.emit_bytes_piece(&sink, hs, hi);
        let mut i = self.f.instructions();
        i.local_get(hi).local_get(hm).i32_add().local_set(hi);
        i.local_get(hi).local_set(hs);
        i.else_();
        i.local_get(hi).i32_const(1).i32_add().local_set(hi);
        i.end();
        i.br(0).end().end();
        let _ = i;
        self.emit_bytes_piece(&sink, hs, hn);
        let mut i = self.f.instructions();
        i.end();
        i.local_get(ho);
        let _ = i;
        for _ in 0..12 {
            self.release_i32();
        }
        Ok(Some(SliceTy::List(self.types.intern(BYTES))))
    }
}
