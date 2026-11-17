//! The in-place `bytes` mutator arms (#3641): `fill`, `clear` and
//! `copy_within` change a unique receiver in place, through the receiver
//! protocol `push` and `copy_from` already use (bytes_recv.rs: classify,
//! read through the COW gate, mutate, write back). Split from bytes_recv.rs
//! so the arm chain (`-> ArmResult`, scripts/check-arm-args.sh) stays apart
//! from the receiver helper it calls. `drop_end` (#4756) truncates a dying
//! source in place by rewriting its length.

use almide_ir::IrExpr;
use wasm_encoder::BlockType;

use crate::bytes::BYTES;
use crate::emitter::Emitter;
use crate::*;

impl Emitter<'_> {
    /// The in-place mutator protocol (#3641) — `push`'s and `copy_from`'s,
    /// shared: classify the receiver, read it through the COW gate (a var's
    /// shared block copies once and the var is repointed at the copy, so an
    /// alias keeps its bytes; a unique one passes through untouched), let
    /// `body` mutate the block held in the local it is handed, then write
    /// it back. A unique receiver therefore costs what the mutation touches
    /// — O(range) — where the functional lowering allocated and copied the
    /// whole buffer on every call, even for an empty range.
    fn bytes_in_place(
        &mut self,
        what: &str,
        b: &IrExpr,
        body: impl FnOnce(&mut Self, u32) -> Result<(), EmitError>,
    ) -> ArmResult {
        let recv = self.bytes_recv(what, b)?;
        self.emit_read_bytes_recv(&recv, b)?;
        let hb = self.hold_i32()?;
        self.f.instructions().local_set(hb);
        body(self, hb)?;
        self.f.instructions().local_get(hb);
        self.emit_bytes_writeback(&recv)?;
        self.release_i32();
        Ok(None)
    }

    /// fill: every byte becomes `val as u8` (native `b.fill`).
    pub(crate) fn lower_bytes_fill(&mut self, b: &IrExpr, v: &IrExpr) -> ArmResult {
        self.bytes_in_place("fill", b, |e, hb| {
            e.lower_arg(v, Some(INT), ArgMode::Borrow)?;
            let hv = e.hold_i64()?;
            let mut i = e.f.instructions();
            i.local_set(hv);
            i.local_get(hb).i32_const(almide_layout::PAYLOAD as i32).i32_add();
            i.local_get(hv).i32_wrap_i64();
            i.local_get(hb).i32_load(len_memarg());
            i.memory_fill(0);
            e.release_i64();
            Ok(())
        })
    }

    /// clear (native `Vec::clear`, #1423 stage 4): length 0, capacity kept,
    /// so a refill reuses the block. A record field receiver is rebound to
    /// an empty buffer (`lower_field_clear`).
    pub(crate) fn lower_bytes_clear(&mut self, b: &IrExpr) -> ArmResult {
        if self.lower_field_clear(b)? {
            return Ok(None);
        }
        self.bytes_in_place("clear", b, |e, hb| {
            e.f.instructions().local_get(hb).i32_const(0).i32_store(len_memarg());
            Ok(())
        })
    }

    /// copy_within: memmove inside the buffer, NO-OP when the range is
    /// empty or the destination does not fit (C-213). The fit test is
    /// SUBTRACTIVE and signed — `d >= 0 && d <= len - (e - s)` — like
    /// `bytes_room`. The first cut copied native's `d + (e - s) <= len`
    /// "verbatim", and that sum wraps in i64 exactly as native's release
    /// usize arithmetic did: `-1 + 2 = 1 <= 6` passed, and where native's
    /// `Vec::copy_within` still had a slice bounds check behind the guard,
    /// this leg ran `memory.copy(payload - 1, ..)` and stored into the
    /// block header (#2474 — `[2, 2, 3, 4, 5, 6]` for `copy_within(b, 0, 2,
    /// -1)`, and `dst = -len` rewrote the length field). With `e <= len`
    /// (the clamp below) and `s < e`, `e - s` is in `[1, len]` and `len - (e
    /// - s)` in `[0, len)`, so a `d` that passes puts `[d, d + (e - s))`
    /// inside the payload — no offset can reach memory.copy otherwise.
    /// `memory.copy` is overlap-safe, so the move is done in place.
    pub(crate) fn lower_bytes_copy_within(&mut self, b: &IrExpr, s: &IrExpr, e: &IrExpr, d: &IrExpr) -> ArmResult {
        self.bytes_in_place("copy-within", b, |em, hb| {
            let mut held = [0u32; 3];
            for (slot, arg) in held.iter_mut().zip([s, e, d]) {
                em.lower_arg(arg, Some(INT), ArgMode::Borrow)?;
                *slot = em.hold_i64()?;
                em.f.instructions().local_set(*slot);
            }
            let [hs, he, hd] = held;
            let payload = |i: &mut wasm_encoder::InstructionSink<'_>, off: u32| {
                i.local_get(hb).i32_const(almide_layout::PAYLOAD as i32).i32_add();
                i.local_get(off).i32_wrap_i64().i32_add();
            };
            let len = |i: &mut wasm_encoder::InstructionSink<'_>| {
                i.local_get(hb).i32_load(len_memarg()).i64_extend_i32_u();
            };
            let mut i = em.f.instructions();
            // e = min_u(e, len)
            i.local_get(he);
            len(&mut i);
            i.local_get(he);
            len(&mut i);
            i.i64_lt_u().select().local_set(he);
            // s < e (unsigned: a negative s is enormous and fails)
            //   && d >= 0 && d <= len - (e - s)
            i.local_get(hs).local_get(he).i64_lt_u();
            i.local_get(hd).i64_const(0).i64_ge_s().i32_and();
            i.local_get(hd);
            len(&mut i);
            i.local_get(he).local_get(hs).i64_sub().i64_sub();
            i.i64_le_s();
            i.i32_and().if_(wasm_encoder::BlockType::Empty);
            payload(&mut i, hd);
            payload(&mut i, hs);
            i.local_get(he).local_get(hs).i64_sub().i32_wrap_i64();
            i.memory_copy(0, 0);
            i.end();
            for _ in held {
                em.release_i64();
            }
            Ok(())
        })
    }

    /// `bytes.drop_end(b, n)` (#4756), `@reuse(b)`: a dying source is
    /// truncated in place by rewriting its length (as `list.drop_end`'s
    /// in-place arm), so a trim-one-byte rebind loop is O(1) per step; a live
    /// source is copied first. A negative n is 0, an n past the end empties.
    pub(crate) fn lower_bytes_drop_end(&mut self, b: &IrExpr, n: &IrExpr) -> ArmResult {
        if self.owned_reuse_source(b, &[n], Some(BYTES))?.is_none() {
            self.lower_arg(b, Some(BYTES), ArgMode::Borrow)?;
            self.f.instructions().call(F_BLOCK_COPY);
        }
        let bh = self.hold_i32()?;
        self.f.instructions().local_set(bh);
        // `$cow` hands a static block back as it is (a literal's data):
        // the truncate writes its length, so a static is copied first.
        let mut i = self.f.instructions();
        i.local_get(bh).global_get(G_LINE_END).i32_lt_u().if_(BlockType::Empty);
        i.local_get(bh).call(F_BLOCK_COPY).local_set(bh);
        i.end();
        let _ = i;
        self.lower_arg(n, Some(INT), ArgMode::Borrow)?;
        let hn = self.hold_i64()?;
        let hl = self.hold_i64()?;
        let mut i = self.f.instructions();
        i.local_set(hn);
        i.local_get(hn).i64_const(0).local_get(hn).i64_const(0).i64_gt_s().select().local_set(hn); // #4147: a negative count is 0
        i.local_get(bh).i32_load(len_memarg()).i64_extend_i32_u().local_set(hl);
        // len' = len - min(n, len)
        i.local_get(bh);
        i.local_get(hl);
        i.local_get(hn).local_get(hl).local_get(hn).local_get(hl).i64_lt_s().select();
        i.i64_sub().i32_wrap_i64();
        i.i32_store(len_memarg());
        i.local_get(bh);
        let _ = i;
        self.release_i64();
        self.release_i64();
        self.release_i32();
        Ok(Some(Lowered::owned(BYTES)))
    }
}
