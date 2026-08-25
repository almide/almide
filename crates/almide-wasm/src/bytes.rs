//! Bytes — the byte-packed buffer surface (String's layout twin). The
//! oracle allows in-place `set_*` (a `mut`/buffer API); under the
//! bind-deep-copy doctrine a local's block is uniquely its own, so the
//! stores are unobservable through aliases. `bytes.new` relies on the
//! bump allocator's zero guarantee (fresh pages are zero and the bump
//! head never reuses).

use almide_ir::IrExpr;
use wasm_encoder::{BlockType, MemArg, ValType};

use crate::emitter::Emitter;
use crate::*;

const BYTES: SliceTy = SliceTy::Scalar(Scalar::Bytes);

fn byte_at() -> MemArg {
    MemArg { offset: u64::from(almide_layout::PAYLOAD), align: 0, memory_index: 0 }
}

impl Emitter<'_> {
    /// Bounds guard: absolute index (i64) already wrapped on the stack is
    /// NOT the shape here — this takes (block hold, index hold i64) and
    /// traps unless 0 <= i < len (the oracle aborts out of bounds).
    fn bytes_bounds(&mut self, bh: u32, ih: u32) {
        let mut i = self.f.instructions();
        i.local_get(ih).i64_const(0).i64_lt_s();
        i.local_get(ih);
        i.local_get(bh).i32_load(len_memarg()).i64_extend_i32_u();
        i.i64_ge_s().i32_or().if_(BlockType::Empty).unreachable().end();
    }

    pub(crate) fn lower_bytes_call(
        &mut self,
        func: &str,
        args: &[IrExpr],
    ) -> Result<Option<SliceTy>, EmitError> {
        match (func, args) {
            ("new", [n]) => {
                self.lower(n, Some(INT))?;
                self.f.instructions().i32_wrap_i64().call(F_ALLOC);
                Ok(Some(BYTES))
            }
            ("from_string", [s]) => {
                self.lower(s, Some(STR))?;
                self.f.instructions().call(F_BLOCK_COPY);
                Ok(Some(BYTES))
            }
            ("len", [b]) => {
                self.lower(b, Some(BYTES))?;
                self.f.instructions().i32_load(len_memarg()).i64_extend_i32_u();
                Ok(Some(INT))
            }
            ("from_list", [xs]) => {
                match self.lower(xs, None)? {
                    SliceTy::List(h) if self.types.el(h) == INT => {}
                    other => return unsup(&format!("bytes-from-of:{other:?}")),
                }
                let bh = self.hold_i32()?;
                let ch = self.hold_i32()?;
                let ih = self.hold_i32()?;
                let rh = self.hold_i32()?;
                self.f.instructions().local_tee(bh);
                self.f
                    .instructions()
                    .i32_load(len_memarg())
                    .i32_const(8)
                    .i32_div_u()
                    .local_tee(ch)
                    .call(F_ALLOC)
                    .local_set(rh)
                    .i32_const(0)
                    .local_set(ih);
                self.f.instructions().block(BlockType::Empty).loop_(BlockType::Empty);
                self.f.instructions().local_get(ih).local_get(ch).i32_ge_u().br_if(1);
                self.f.instructions().local_get(rh).local_get(ih).i32_add();
                self.f
                    .instructions()
                    .local_get(bh)
                    .local_get(ih)
                    .i32_const(8)
                    .i32_mul()
                    .i32_add()
                    .i64_load(slot_memarg(0))
                    .i32_wrap_i64()
                    .i32_store8(byte_at());
                self.f
                    .instructions()
                    .local_get(ih)
                    .i32_const(1)
                    .i32_add()
                    .local_set(ih)
                    .br(0)
                    .end()
                    .end();
                self.f.instructions().local_get(rh);
                self.release_i32();
                self.release_i32();
                self.release_i32();
                self.release_i32();
                Ok(Some(BYTES))
            }
            ("get_or", [b, i, d]) => {
                self.lower(b, Some(BYTES))?;
                let bh = self.hold_i32()?;
                self.f.instructions().local_set(bh);
                self.lower(i, Some(INT))?;
                let ih = self.hold_i64()?;
                self.f.instructions().local_set(ih);
                let mut ins = self.f.instructions();
                ins.local_get(ih).i64_const(0).i64_lt_s();
                ins.local_get(ih);
                ins.local_get(bh).i32_load(len_memarg()).i64_extend_i32_u();
                ins.i64_ge_s().i32_or().if_(BlockType::Result(ValType::I64));
                self.lower(d, Some(INT))?;
                self.f.instructions().else_();
                self.f
                    .instructions()
                    .local_get(bh)
                    .local_get(ih)
                    .i32_wrap_i64()
                    .i32_add()
                    .i32_load8_u(byte_at())
                    .i64_extend_i32_u()
                    .end();
                self.release_i64();
                self.release_i32();
                Ok(Some(INT))
            }
            ("read_u8", [b, i]) | ("get", [b, i]) if func == "read_u8" => {
                self.lower(b, Some(BYTES))?;
                let bh = self.hold_i32()?;
                self.f.instructions().local_set(bh);
                self.lower(i, Some(INT))?;
                let ih = self.hold_i64()?;
                self.f.instructions().local_set(ih);
                self.bytes_bounds(bh, ih);
                self.f
                    .instructions()
                    .local_get(bh)
                    .local_get(ih)
                    .i32_wrap_i64()
                    .i32_add()
                    .i32_load8_u(byte_at())
                    .i64_extend_i32_u();
                self.release_i64();
                self.release_i32();
                Ok(Some(INT))
            }
            ("set_at", [b, i, v]) | ("set_u8", [b, i, v]) => {
                self.lower(b, Some(BYTES))?;
                let bh = self.hold_i32()?;
                self.f.instructions().local_set(bh);
                self.lower(i, Some(INT))?;
                let ih = self.hold_i64()?;
                self.f.instructions().local_set(ih);
                self.bytes_bounds(bh, ih);
                self.f.instructions().local_get(bh).local_get(ih).i32_wrap_i64().i32_add();
                self.lower(v, Some(INT))?;
                self.f.instructions().i32_wrap_i64().i32_store8(byte_at());
                self.release_i64();
                self.release_i32();
                Ok(None)
            }
            ("set_f32_le", [b, i, v]) => {
                self.lower(b, Some(BYTES))?;
                let bh = self.hold_i32()?;
                self.f.instructions().local_set(bh);
                self.lower(i, Some(INT))?;
                let ih = self.hold_i64()?;
                self.f.instructions().local_set(ih);
                // bounds for i .. i+3
                let mut ins = self.f.instructions();
                ins.local_get(ih).i64_const(0).i64_lt_s();
                ins.local_get(ih).i64_const(3).i64_add();
                ins.local_get(bh).i32_load(len_memarg()).i64_extend_i32_u();
                ins.i64_ge_s().i32_or().if_(BlockType::Empty).unreachable().end();
                self.f.instructions().local_get(bh).local_get(ih).i32_wrap_i64().i32_add();
                self.lower(v, Some(FLOAT))?;
                // f64 → f32 bits, little-endian store (wasm stores are LE)
                self.f
                    .instructions()
                    .f32_demote_f64()
                    .i32_reinterpret_f32()
                    .i32_store(MemArg {
                        offset: u64::from(almide_layout::PAYLOAD),
                        align: 0,
                        memory_index: 0,
                    });
                self.release_i64();
                self.release_i32();
                Ok(None)
            }
            // OOB (or a negative pos) is DEFINED 0.0 for the f32 reader
            // (native: checked_add + len test), unlike f16's trap form.
            ("read_f32_le", [b, i]) => {
                self.lower(b, Some(BYTES))?;
                let bh = self.hold_i32()?;
                self.f.instructions().local_set(bh);
                self.lower(i, Some(INT))?;
                let ih = self.hold_i64()?;
                let mut ins = self.f.instructions();
                ins.local_set(ih);
                ins.local_get(ih).i64_const(0).i64_lt_s();
                ins.local_get(ih).i64_const(4).i64_add();
                ins.local_get(bh).i32_load(len_memarg()).i64_extend_i32_u();
                ins.i64_gt_s().i32_or();
                ins.if_(BlockType::Result(ValType::F64));
                ins.f64_const(0.0.into());
                ins.else_();
                ins.local_get(bh).local_get(ih).i32_wrap_i64().i32_add();
                ins.f32_load(MemArg {
                    offset: u64::from(almide_layout::PAYLOAD),
                    align: 0,
                    memory_index: 0,
                });
                ins.f64_promote_f32();
                ins.end();
                let _ = ins;
                self.release_i64();
                self.release_i32();
                Ok(Some(FLOAT))
            }
            ("read_f16_le", [b, i]) => {
                self.lower(b, Some(BYTES))?;
                let bh = self.hold_i32()?;
                self.f.instructions().local_set(bh);
                self.lower(i, Some(INT))?;
                let ih = self.hold_i64()?;
                self.f.instructions().local_set(ih);
                let mut ins = self.f.instructions();
                ins.local_get(ih).i64_const(0).i64_lt_s();
                ins.local_get(ih).i64_const(1).i64_add();
                ins.local_get(bh).i32_load(len_memarg()).i64_extend_i32_u();
                ins.i64_ge_s().i32_or().if_(BlockType::Empty).unreachable().end();
                self.f
                    .instructions()
                    .local_get(bh)
                    .local_get(ih)
                    .i32_wrap_i64()
                    .i32_add()
                    .i32_load16_u(MemArg {
                        offset: u64::from(almide_layout::PAYLOAD),
                        align: 0,
                        memory_index: 0,
                    })
                    .call(F_F16_TO_F64);
                self.release_i64();
                self.release_i32();
                Ok(Some(FLOAT))
            }
            _ => unsup(&format!("call:bytes.{func}")),
        }
    }
}
