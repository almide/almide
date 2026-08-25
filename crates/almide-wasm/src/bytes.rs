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

/// Payload-relative byte address: byte k of the current window.
fn byte_k(k: u8) -> MemArg {
    MemArg {
        offset: u64::from(almide_layout::PAYLOAD) + u64::from(k),
        align: 0,
        memory_index: 0,
    }
}

impl Emitter<'_> {
    /// C-229 totality: a read DEFAULTS and a write NO-OPS when the window
    /// [pos, pos+width) leaves the buffer — negative pos included, never a
    /// trap. The room test SUBTRACTS (`pos <= len - width`): the
    /// `pos + width` sum wraps for a pos near the top of i64 and once let
    /// a store land back inside the buffer (fuzz seed 510754018593).
    /// Leaves the in-room boolean (i32) on the stack.
    fn bytes_room(&mut self, bh: u32, ih: u32, width: u8) {
        let mut i = self.f.instructions();
        i.local_get(ih).i64_const(0).i64_ge_s();
        i.local_get(ih);
        i.local_get(bh).i32_load(len_memarg()).i64_extend_i32_u();
        i.i64_const(i64::from(width)).i64_sub();
        i.i64_le_s().i32_and();
    }

    /// The whole scalar-read family as ONE shape: guard → window bits as
    /// i64 (sign-extended per the surface), out of room → 0. Floats, bool
    /// and f16 convert the bits afterwards — 0 bits IS each type's
    /// native default (0.0 / false), so the default needs no second path.
    fn lower_bytes_read_bits(
        &mut self,
        b: &IrExpr,
        pos: &IrExpr,
        width: u8,
        signed: bool,
        be: bool,
    ) -> Result<(), EmitError> {
        self.lower(b, Some(BYTES))?;
        let bh = self.hold_i32()?;
        self.f.instructions().local_set(bh);
        self.lower(pos, Some(INT))?;
        let ih = self.hold_i64()?;
        let ha = self.hold_i32()?;
        self.f.instructions().local_set(ih);
        self.bytes_room(bh, ih, width);
        let mut i = self.f.instructions();
        i.if_(BlockType::Result(ValType::I64));
        i.local_get(bh).local_get(ih).i32_wrap_i64().i32_add();
        if be {
            // compose MSB-first: acc = (acc << 8) | byte[k]
            i.local_set(ha);
            i.local_get(ha).i64_load8_u(byte_k(0));
            for k in 1..width {
                i.i64_const(8).i64_shl();
                i.local_get(ha).i64_load8_u(byte_k(k)).i64_or();
            }
            if signed {
                match width {
                    2 => {
                        i.i64_extend16_s();
                    }
                    4 => {
                        i.i64_extend32_s();
                    }
                    _ => {}
                }
            }
        } else {
            match (width, signed) {
                (1, true) => i.i64_load8_s(byte_k(0)),
                (1, false) => i.i64_load8_u(byte_k(0)),
                (2, true) => i.i64_load16_s(byte_k(0)),
                (2, false) => i.i64_load16_u(byte_k(0)),
                (4, true) => i.i64_load32_s(byte_k(0)),
                (4, false) => i.i64_load32_u(byte_k(0)),
                _ => i.i64_load(byte_k(0)),
            };
        }
        i.else_().i64_const(0).end();
        let _ = i;
        self.release_i32();
        self.release_i64();
        self.release_i32();
        Ok(())
    }

    /// The scalar-set family: value bits ALWAYS evaluate (argument order
    /// is unconditional), then the guarded store — or nothing.
    fn lower_bytes_set(
        &mut self,
        b: &IrExpr,
        pos: &IrExpr,
        v: &IrExpr,
        width: u8,
        be: bool,
        float: bool,
    ) -> Result<(), EmitError> {
        self.lower(b, Some(BYTES))?;
        let bh = self.hold_i32()?;
        self.f.instructions().local_set(bh);
        self.lower(pos, Some(INT))?;
        let ih = self.hold_i64()?;
        self.f.instructions().local_set(ih);
        if float {
            self.lower(v, Some(FLOAT))?;
            let mut i = self.f.instructions();
            if width == 4 {
                i.f32_demote_f64().i32_reinterpret_f32().i64_extend_i32_u();
            } else {
                i.i64_reinterpret_f64();
            }
        } else {
            self.lower(v, Some(INT))?;
        }
        let hv = self.hold_i64()?;
        let ha = self.hold_i32()?;
        self.f.instructions().local_set(hv);
        self.bytes_room(bh, ih, width);
        let mut i = self.f.instructions();
        i.if_(BlockType::Empty);
        i.local_get(bh).local_get(ih).i32_wrap_i64().i32_add();
        if be {
            i.local_set(ha);
            for k in 0..width {
                i.local_get(ha).local_get(hv);
                let sh = i64::from(width - 1 - k) * 8;
                if sh > 0 {
                    i.i64_const(sh).i64_shr_u();
                }
                i.i64_store8(byte_k(k));
            }
        } else {
            i.local_get(hv);
            match width {
                1 => i.i64_store8(byte_k(0)),
                2 => i.i64_store16(byte_k(0)),
                4 => i.i64_store32(byte_k(0)),
                _ => i.i64_store(byte_k(0)),
            };
        }
        i.end();
        let _ = i;
        self.release_i32();
        self.release_i64();
        self.release_i64();
        self.release_i32();
        Ok(())
    }

    /// The C-229 scalar read/set matrix — every width, both
    /// endiannesses, TOTAL: an out-of-room read is the type's default,
    /// an out-of-room set is a no-op (negative and top-of-i64 positions
    /// included). Err(..) inside; Ok(None) = not a matrix surface.
    fn lower_bytes_rw(
        &mut self,
        func: &str,
        args: &[IrExpr],
    ) -> Result<Option<Option<SliceTy>>, EmitError> {
        let width_of = |f: &str| {
            if f.contains("16") {
                2
            } else if f.contains("32") {
                4
            } else {
                8
            }
        };
        match (func, args) {
            ("read_u8" | "read_bool", [b, i]) => {
                self.lower_bytes_read_bits(b, i, 1, false, false)?;
                if func == "read_bool" {
                    self.f.instructions().i64_const(0).i64_ne();
                    return Ok(Some(Some(BOOL)));
                }
                Ok(Some(Some(INT)))
            }
            ("read_u16_le" | "read_u16_be" | "read_i16_le" | "read_i16_be" | "read_u32_le"
            | "read_u32_be" | "read_i32_le" | "read_i32_be" | "read_i64_le" | "read_i64_be", [b, i]) => {
                self.lower_bytes_read_bits(
                    b,
                    i,
                    width_of(func),
                    func.starts_with("read_i"),
                    func.ends_with("_be"),
                )?;
                Ok(Some(Some(INT)))
            }
            ("set_at" | "set_u8", [b, i, v]) => {
                self.lower_bytes_set(b, i, v, 1, false, false)?;
                Ok(Some(None))
            }
            ("set_u16_le" | "set_u16_be" | "set_i16_le" | "set_i16_be" | "set_u32_le"
            | "set_u32_be" | "set_i32_le" | "set_i32_be" | "set_i64_le" | "set_i64_be", [b, i, v]) => {
                self.lower_bytes_set(b, i, v, width_of(func), func.ends_with("_be"), false)?;
                Ok(Some(None))
            }
            ("set_f32_le" | "set_f32_be" | "set_f64_le" | "set_f64_be", [b, i, v]) => {
                self.lower_bytes_set(b, i, v, width_of(func), func.ends_with("_be"), true)?;
                Ok(Some(None))
            }
            ("read_f32_le" | "read_f32_be" | "read_f64_le" | "read_f64_be", [b, i]) => {
                let width = width_of(func);
                self.lower_bytes_read_bits(b, i, width, false, func.ends_with("_be"))?;
                if width == 4 {
                    self.f.instructions().i32_wrap_i64().f32_reinterpret_i32().f64_promote_f32();
                } else {
                    self.f.instructions().f64_reinterpret_i64();
                }
                Ok(Some(Some(FLOAT)))
            }
            // f16 bits through the same total window; 0 bits = 0.0.
            ("read_f16_le", [b, i]) => {
                self.lower_bytes_read_bits(b, i, 2, false, false)?;
                self.f.instructions().i32_wrap_i64().call(F_F16_TO_F64);
                Ok(Some(Some(FLOAT)))
            }
            _ => Ok(None),
        }
    }

    pub(crate) fn lower_bytes_call(
        &mut self,
        func: &str,
        args: &[IrExpr],
    ) -> Result<Option<SliceTy>, EmitError> {
        if let Some(out) = self.lower_bytes_rw(func, args)? {
            return Ok(out);
        }
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
                // the default ALWAYS evaluates (native argument order) —
                // in-branch lowering would skip its effects in-bounds
                self.lower(d, Some(INT))?;
                let hd = self.hold_i64()?;
                self.f.instructions().local_set(hd);
                self.bytes_room(bh, ih, 1);
                let mut ins = self.f.instructions();
                ins.if_(BlockType::Result(ValType::I64));
                ins.local_get(bh)
                    .local_get(ih)
                    .i32_wrap_i64()
                    .i32_add()
                    .i32_load8_u(byte_at())
                    .i64_extend_i32_u();
                ins.else_().local_get(hd).end();
                let _ = ins;
                self.release_i64();
                self.release_i64();
                self.release_i32();
                Ok(Some(INT))
            }
            _ => unsup(&format!("call:bytes.{func}")),
        }
    }
}
