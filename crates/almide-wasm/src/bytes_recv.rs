//! The RECEIVER of an in-place `bytes` mutator — the Unit-returning
//! intrinsics (`append_*`, `write_*`, `fill`, `copy_from`) and the push
//! convention `append_u8` shares — split from bytes.rs for the file budget.
//!
//! Native mutates THROUGH the receiver: a var — a `var`, a cell, a
//! module-level `var` — sees the write. Since #2466 every writer declares
//! its receiver `mut`, so the checker rejects a `let`, a plain parameter and
//! a TEMPORARY (a call result, a fresh value) with E032, exactly as for
//! `push`; before #2466 the temporary was admitted (native mutated it and
//! dropped it), and the `Temp` arm below is what lowered it — kept so a
//! checked program that reaches this leg by another route (a fixture fed
//! straight to the emitter) still has a defined lowering. This leg lowers
//! every mutator functionally — a fresh block, or the push helper's
//! in-place window — and the receiver decides what happens to that block: a
//! var takes it back through its slot; a temporary has no slot, so the
//! block is RELEASED. Before #1849 the temporary walled here and the
//! reroute to the incumbent left the linked twin's result on the operand
//! stack — invalid wasm for a program native runs.
//!
//! Any other receiver — a field, an element, a control funnel — keeps its
//! honest wall: a write-back there needs an owner this leg does not model,
//! and dropping the block silently would be an aliasing miscompile.

use almide_ir::{IrExpr, IrExprKind, VarId};

use crate::bytes::BYTES;
use crate::emitter::Emitter;
use crate::rc_ownership::rc_certainly_fresh;
use crate::*;

pub(crate) enum BytesRecv {
    /// A var with a slot: the mutated block is written back through it.
    Var { id: VarId, idx: u32, ty: SliceTy, global: bool },
    /// A temporary: the mutated block is released after the call.
    Temp,
}

impl Emitter<'_> {
    /// Classify the receiver of the `what` arm (the wall names it).
    pub(crate) fn bytes_recv(&self, what: &str, b: &IrExpr) -> Result<BytesRecv, EmitError> {
        match &b.kind {
            IrExprKind::Var { id } => match self.mut_var(id) {
                Some((idx, ty, global)) => Ok(BytesRecv::Var { id: *id, idx, ty, global }),
                None => unsup("var:unmapped"),
            },
            IrExprKind::Call { .. } => Ok(BytesRecv::Temp),
            k if rc_certainly_fresh(k) => Ok(BytesRecv::Temp),
            _ => unsup(&format!("bytes-{what}-nonvar")),
        }
    }

    /// Push the receiver's block for an IN-PLACE helper. A var reads
    /// through the COW gate (RC-5: a shared block copies first and the
    /// var is repointed at the unique copy). A temporary's call result
    /// may BORROW a live holder's block — a fn returning its parameter,
    /// an element read inside a native arm — so it is materialized as a
    /// copy unless certainly fresh: the mutation lands on a block nobody
    /// else holds (value semantics), and the original keeps its count.
    pub(crate) fn emit_read_bytes_recv(
        &mut self,
        recv: &BytesRecv,
        b: &IrExpr,
    ) -> Result<(), EmitError> {
        match recv {
            BytesRecv::Var { id, idx, ty, global } => {
                self.emit_read_mut_var_cow(id, *idx, *ty, *global)
            }
            BytesRecv::Temp => {
                self.lower(b, Some(BYTES))?;
                if !rc_certainly_fresh(&b.kind) {
                    self.f.instructions().call(F_BLOCK_COPY);
                }
                Ok(())
            }
        }
    }

    /// `bytes.set_*(h.f, ..)` on a record var's Bytes FIELD — the set
    /// family writes IN PLACE with no write-back, so both levels are made
    /// unique first (#794's two-level COW, on this leg): the record through
    /// its COW judge (a shared record copies with its slot credits and the
    /// var is repointed at the copy), then the field's block through `$cow`
    /// (a shared block copies, the record's credit moves to the copy, and
    /// the slot is repointed). Without it `var p2 = p1` shares one block
    /// and the write shows through `p1.f` — native's RcCow never lets it.
    /// Pushes the now-unique field block and answers `true`; `false` for
    /// any other receiver. A PARAMETER's field is judged too (#3342): the
    /// write-back hands the copy back to the caller. No block
    /// reached here is pooled: the pool holds strings, nullary variant
    /// cases and closure blocks, never a record with a field or a Bytes.
    pub(crate) fn emit_read_field_bytes_cow(&mut self, b: &IrExpr) -> Result<bool, EmitError> {
        let IrExprKind::Member { object, field } = &b.kind else {
            return Ok(false);
        };
        let IrExprKind::Var { id } = &object.kind else {
            return Ok(false);
        };
        let Some((idx, rty, global)) = self.mut_var(id) else {
            return Ok(false);
        };
        let SliceTy::Named(ti) = rty else {
            return Ok(false);
        };
        let off = {
            let crate::types_table::NamedDef::Record(r) = self.types.def(ti) else {
                return Ok(false);
            };
            match r.fields.iter().find(|f| f.name == field.as_str()) {
                Some(fi) if fi.ty == BYTES => fi.offset,
                _ => return Ok(false),
            }
        };
        let (rcow, bcow) = (self.cow_fn_of(rty), self.cow_fn_of(BYTES));
        let hr = self.hold_i32()?;
        self.emit_read_mut_var(id, idx, rty, global);
        self.f.instructions().call(rcow).local_tee(hr);
        self.emit_store_mut_var(*id, idx, rty, global)?;
        // #2755: the record's copy-on-write rebinds the var; the field's own
        // `$cow` below moves a credit the record holds, not this frame.
        self.witness_mut_rebind(*id, global);
        let scr = self.scr_i32_local;
        {
            let mut i = self.f.instructions();
            i.local_get(hr).i32_load(slot_memarg(off)).call(bcow).local_set(scr);
            i.local_get(hr).local_get(scr);
        }
        self.store_ty_slot(BYTES, off);
        self.f.instructions().local_get(scr);
        self.release_i32();
        Ok(true)
    }

    /// The mutated block is on the stack: a var takes it back through
    /// its slot; a temporary's is released. Every arm's block is uniquely
    /// held — the native arms allocate it, the push helper answers with
    /// the materialized copy or its grown successor, and the linked twins
    /// (`__bam`, `__bt_append`, `bytes_write_string_be`) build theirs with
    /// `prim.alloc_bytes` — so `$dec_flat` frees it, and no-ops a static.
    pub(crate) fn emit_bytes_writeback(&mut self, recv: &BytesRecv) -> Result<(), EmitError> {
        self.bytes_writeback(recv, false)
    }

    /// `emit_bytes_writeback` for an arm whose block is FRESH (never the
    /// receiver's own): a var's credit on the block it replaces goes with
    /// the rebind (`emit_rebind_mut_var_fresh`) — the linked append family
    /// (#2968).
    pub(crate) fn emit_bytes_writeback_fresh(&mut self, recv: &BytesRecv) -> Result<(), EmitError> {
        self.bytes_writeback(recv, true)
    }

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

    fn bytes_writeback(&mut self, recv: &BytesRecv, fresh: bool) -> Result<(), EmitError> {
        match recv {
            BytesRecv::Var { id, idx, ty, global } if fresh => {
                self.emit_rebind_mut_var_fresh(*id, *idx, *ty, *global)
            }
            BytesRecv::Var { id, idx, ty, global } => {
                self.emit_store_mut_var(*id, *idx, *ty, *global)
            }
            BytesRecv::Temp => {
                self.f.instructions().call(F_DEC_FLAT);
                Ok(())
            }
        }
    }
}
