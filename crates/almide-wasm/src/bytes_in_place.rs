//! The in-place `bytes` mutator arms (#3641): `fill`, `clear` and
//! `copy_within` change a unique receiver in place, through the receiver
//! protocol `push` and `copy_from` already use (bytes_recv.rs: classify,
//! read through the COW gate, mutate, write back). Split from bytes_recv.rs
//! so the arm chain (`-> ArmResult`, scripts/check-arm-args.sh) stays apart
//! from the receiver helper it calls.

use almide_ir::IrExpr;

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
}
