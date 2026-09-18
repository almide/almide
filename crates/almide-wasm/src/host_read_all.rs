//! #2116: stdin read-to-end grows on the guest heap, never in the park.
//!
//! Op 31 drained the stream into the host's fixed park span, so the span
//! — not the heap — was the ceiling: 257,024 bytes on stock WASI (where
//! the overflow wore the unsupported-op message) and 326,657 on a p2
//! component (where it produced no diagnostic at all), against a native
//! leg with no bound. Taking the stream in chunks off the shared cursor
//! (op 35, served by all three shims) and appending into the owned-string
//! accumulator removes the ceiling from every leg at once, the way #2114
//! removed the 4096-byte line limit.
use wasm_encoder::BlockType;

use crate::emitter::Emitter;
use crate::*;

/// Bytes per take. Every shim clamps its own read to 4096, so a larger
/// ask buys nothing; a short read is the contract, and only a zero-byte
/// answer means EOF.
const CHUNK: i32 = 4096;

impl Emitter<'_> {
    pub(crate) fn io_read_all(&mut self) -> Result<(), EmitError> {
        let acc = self.hold_i32()?;
        let chunk = self.hold_i32()?;
        let got = self.hold_i32()?;
        {
            let mut i = self.f.instructions();
            i.i32_const(0).call(F_ALLOC).local_set(acc);
            i.i32_const(CHUNK).call(F_ALLOC).local_set(chunk);
            i.block(BlockType::Empty).loop_(BlockType::Empty);
        }
        self.fs_call_stdin_take(CHUNK)?;
        {
            let mut i = self.f.instructions();
            i.i64_const(0xFFFF_FFFF).i64_and().i32_wrap_i64().local_set(got);
            i.local_get(got).i32_eqz().br_if(1);
            // Narrow the staging block to the bytes this take answered:
            // $str_append copies `src`'s LEN. The release below reads CAP,
            // a separate header field, so the whole allocation still goes
            // back to its class.
            i.local_get(chunk).local_get(got).i32_store(len_memarg());
            i.local_get(chunk).i32_const(almide_layout::PAYLOAD as i32).i32_add().call(F_HOST_READ);
            // Owned dst, borrowed src — the helper grows geometrically and
            // releases each outgrown block; allocation failure is C-197.
            i.local_get(acc).local_get(chunk).call(F_STR_APPEND).local_set(acc);
            i.br(0).end().end();
            i.local_get(chunk).call(F_DEC_FLAT);
            i.local_get(acc);
        }
        self.release_i32();
        self.release_i32();
        self.release_i32();
        Ok(())
    }
}
