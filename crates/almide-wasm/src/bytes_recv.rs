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
    /// any other receiver, and for a PARAMETER's field, whose writes stay
    /// caller-visible exactly as the var arm exempts a parameter. No block
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
        if !global && idx < self.rc_param_ceiling {
            return Ok(false);
        }
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
        match recv {
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
