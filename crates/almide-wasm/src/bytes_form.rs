//! The value forms of the bytes producers (#4612) — split from bytes.rs for
//! the file budget.

use almide_ir::IrExpr;

use super::{byte_form_of, byte_k};

/// What a bytes producer's VALUE is over the byte block it reads (#4612).
/// An intrinsic has no body for a pass to read, so its value semantics live
/// in its lowering arm; this is that arm's declaration, and
/// [`Emitter::lower_bytes_call`] dispatches the two arms THROUGH it, so the
/// form a consumer reads and the code a materialized call runs cannot drift
/// apart. A consumer asks for a form, never for a function.
#[derive(Clone, Copy)]
pub(crate) enum ByteForm<'e> {
    /// A fresh block holding the same bytes as its argument, a layout twin
    /// (String and Bytes share the packed layout): lowered as `$block_copy`.
    Copy(&'e IrExpr),
    /// A `List[Int]` whose element `k` is byte `k` zero-extended (0..=255),
    /// one element per byte, in order.
    Widen(&'e IrExpr),
}

impl ByteForm<'_> {
    /// Both forms read their argument and write only their own fresh
    /// result: a pipeline stage that is pure and cannot abort, in the shared
    /// vocabulary the fusion judgment reads (`almide_ir::fusion`).
    pub(crate) fn stage(self) -> almide_ir::fusion::Stage {
        almide_ir::fusion::Stage { pure: true, total: true, short_circuits: false }
    }
}

/// The form of `e`, when it is a call of a bytes producer that has one.
pub(crate) fn byte_form(e: &IrExpr) -> Option<ByteForm<'_>> {
    let almide_ir::IrExprKind::Call { target: almide_ir::CallTarget::Module { module, func, .. }, args, .. } = &e.kind
    else {
        return None;
    };
    (module.as_str() == "bytes").then(|| byte_form_of(func.as_str(), args)).flatten()
}

/// The list `e` as a WALK over a byte block read in place: a widen, through
/// any number of copies of its block (a copy holds the same bytes, and a
/// walk only reads). Returns the block expression and the producer nodes the
/// walk consumes without lowering them, outermost first, for the witness
/// audit. A walk elides allocations only; whether the consumer may run it
/// element by element is the consumer's stage judgment, with [`ByteForm::stage`]
/// as the producer's verdict.
pub(crate) fn widened_byte_walk(e: &IrExpr) -> Option<(&IrExpr, Vec<&IrExpr>)> {
    let ByteForm::Widen(mut block) = byte_form(e)? else { return None };
    let mut consumed = vec![e];
    while let Some(ByteForm::Copy(src)) = byte_form(block) {
        consumed.push(block);
        block = src;
    }
    Some((block, consumed))
}

/// Push byte `idx` of the block in `base`, zero-extended to i64: the element
/// a [`ByteForm::Widen`] holds, whether it is materialized or walked.
pub(crate) fn emit_widened_byte(i: &mut wasm_encoder::InstructionSink<'_>, base: u32, idx: u32) {
    i.local_get(base).local_get(idx).i32_add().i64_load8_u(byte_k(0));
}
