//! #2114: stdin lines grow through the existing owned-string accumulator.
//! Stop only at LF/EOF; never consume a byte belonging to the next line.
//! #2539: `read_line_opt` is the same walk, answering `none` at end of input.
use wasm_encoder::BlockType;

use crate::emitter::Emitter;
use crate::*;

impl Emitter<'_> {
    pub(crate) fn io_read_line(&mut self) -> Result<(), EmitError> {
        self.io_read_line_into(None)
    }

    /// #2539: the same line read, but `none` when not a single byte was
    /// left — an empty line (just LF) is `some("")`, which `read_line`'s
    /// bare String cannot say. `got` flips on the first byte taken.
    pub(crate) fn io_read_line_opt(&mut self) -> Result<(), EmitError> {
        let got = self.hold_i32()?;
        self.f.instructions().i32_const(0).local_set(got);
        self.io_read_line_into(Some(got))?;
        let line = self.hold_i32()?;
        let cell = self.tmp_i32_local;
        {
            let mut i = self.f.instructions();
            i.local_set(line);
            i.local_get(got).if_(BlockType::Result(wasm_encoder::ValType::I32));
            // some(line): a 4-byte option cell holding the handle (env.get's shape).
            i.i32_const(4).call(F_ALLOC).local_set(cell);
            i.local_get(cell).local_get(line).i32_store(slot_memarg(almide_layout::OPTION_FIELD));
            i.local_get(cell);
            i.else_();
            i.local_get(line).call(F_DEC_FLAT);
            i.i32_const(almide_layout::NULL_ADDR as i32);
            i.end();
        }
        self.release_i32();
        self.release_i32();
        Ok(())
    }

    fn io_read_line_into(&mut self, got: Option<u32>) -> Result<(), EmitError> {
        let line = self.hold_i32()?;
        let byte = self.hold_i32()?;
        let len = self.hold_i32()?;
        {
            let mut i = self.f.instructions();
            i.i32_const(0).call(F_ALLOC).local_set(line);
            i.i32_const(1).call(F_ALLOC).local_set(byte);
            i.block(BlockType::Empty).loop_(BlockType::Empty);
        }
        self.fs_call_stdin_take(1)?;
        {
            let mut i = self.f.instructions();
            i.i64_const(0xFFFF_FFFF).i64_and().i64_eqz().br_if(1);
            if let Some(g) = got {
                i.i32_const(1).local_set(g);
            }
            i.local_get(byte).i32_const(almide_layout::PAYLOAD as i32).i32_add().call(F_HOST_READ);
            i.local_get(byte).i64_load8_u(crate::bytes::byte_k(0));
            i.i64_const(10).i64_eq().br_if(1);
            // Owned dst, borrowed one-byte src. The helper grows geometrically
            // and releases each outgrown block; allocation failure is C-197.
            i.local_get(line).local_get(byte).call(F_STR_APPEND).local_set(line);
            i.br(0).end().end();
            i.local_get(byte).call(F_DEC_FLAT);
            // Match native trim_end_matches('\r'), including repeated CRs
            // and a final EOF-terminated line. Interior CR bytes are retained.
            i.local_get(line).i32_load(len_memarg()).local_set(len);
            i.block(BlockType::Empty).loop_(BlockType::Empty);
            i.local_get(len).i32_eqz().br_if(1);
            i.local_get(line).local_get(len).i32_add().i32_const(1).i32_sub();
            i.i64_load8_u(crate::bytes::byte_k(0)).i64_const(13).i64_ne().br_if(1);
            i.local_get(len).i32_const(1).i32_sub().local_set(len);
            i.br(0).end().end();
            i.local_get(line).local_get(len).i32_store(len_memarg());
            i.local_get(line);
        }
        self.release_i32();
        self.release_i32();
        self.release_i32();
        Ok(())
    }
}
