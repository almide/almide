//! The RECEIVER of an in-place `bytes` mutator — the non-`mut`
//! Unit-returning intrinsics (`append_*`, `write_*`, `fill`, `copy_from`)
//! and the push convention `append_u8` shares — split from bytes.rs for
//! the file budget.
//!
//! Native mutates THROUGH the receiver: a var — `let` or `var`, a cell, a
//! module-level `var` — sees the write, and a TEMPORARY (a call result, a
//! fresh value) is mutated and dropped. The checker admits the temporary
//! because the receiver is not `mut` (`push`'s `mut b` makes it E032), so
//! the statement is legal and observes nothing but its own argument
//! evaluation. This leg lowers every mutator functionally — a fresh block,
//! or the push helper's in-place window — and the receiver decides what
//! happens to that block: a var takes it back through its slot; a
//! temporary has no slot, so the block is RELEASED. Before #1849 the
//! temporary walled here and the reroute to the incumbent left the linked
//! twin's result on the operand stack — invalid wasm for a program native
//! runs.
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
