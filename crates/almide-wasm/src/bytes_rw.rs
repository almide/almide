//! Bytes read/set matrix (C-229 totality) + the append-family
//! builders — split from bytes.rs for the file budget. The layout
//! doctrine lives in bytes.rs's module doc.

use almide_ir::IrExpr;
use wasm_encoder::{BlockType, ValType};

use crate::bytes::{byte_k, BYTES};
use crate::emitter::Emitter;
use crate::*;

impl Emitter<'_> {
    /// `bytes.read_length_prefixed_strings_le(b, pos, count)` — up to
    /// count [u32-LE length][bytes] entries decoded LOSSILY; a truncated
    /// prefix or body STOPS the scan; negative pos reads nothing. Both
    /// bounds SUBTRACTIVE (the additive forms wrap — the self-host's
    /// own doctrine). Two passes: count, then fill 4-byte slots.
    pub(crate) fn lower_bytes_lenprefix(
        &mut self,
        b: &IrExpr,
        pos: &IrExpr,
        count: &IrExpr,
    ) -> Result<Option<SliceTy>, EmitError> {
        let lossy = self.work.helper(crate::work::Helper::Utf8Lossy);
        self.lower(b, Some(BYTES))?;
        let hb = self.hold_i32()?;
        self.f.instructions().local_set(hb);
        self.lower(pos, Some(INT))?;
        let hp0 = self.hold_i64()?;
        self.f.instructions().local_set(hp0);
        self.lower(count, Some(INT))?;
        let hrem = self.hold_i64()?;
        let hp = self.hold_i64()?;
        let hsl = self.hold_i64()?;
        let hn = self.hold_i32()?;
        let hout = self.hold_i32()?;
        let hi = self.hold_i32()?;
        let hs = self.hold_i32()?;
        let mut i = self.f.instructions();
        // rem = max(count, 0)
        i.local_set(hrem);
        i.i64_const(0).local_get(hrem).local_get(hrem).i64_const(0).i64_lt_s().select();
        i.local_set(hrem);
        // pass 1: count
        i.i32_const(0).local_set(hn);
        i.local_get(hp0).local_set(hp);
        i.block(BlockType::Empty).loop_(BlockType::Empty);
        i.local_get(hrem).i64_eqz().br_if(1);
        i.local_get(hp).i64_const(0).i64_lt_s().br_if(1);
        i.local_get(hp);
        i.local_get(hb).i32_load(len_memarg()).i64_extend_i32_u().i64_const(4).i64_sub();
        i.i64_gt_s().br_if(1);
        i.local_get(hb).local_get(hp).i32_wrap_i64().i32_add();
        i.i32_load(wasm_encoder::MemArg {
            offset: u64::from(almide_layout::PAYLOAD),
            align: 0,
            memory_index: 0,
        });
        i.i64_extend_i32_u().local_set(hsl);
        i.local_get(hsl);
        i.local_get(hb)
            .i32_load(len_memarg())
            .i64_extend_i32_u()
            .i64_const(4)
            .i64_sub()
            .local_get(hp)
            .i64_sub();
        i.i64_gt_s().br_if(1);
        i.local_get(hp).i64_const(4).i64_add().local_get(hsl).i64_add().local_set(hp);
        i.local_get(hrem).i64_const(1).i64_sub().local_set(hrem);
        i.local_get(hn).i32_const(1).i32_add().local_set(hn);
        i.br(0).end().end();
        // pass 2: fill
        i.local_get(hn).i32_const(4).i32_mul().call(F_ALLOC).local_set(hout);
        i.local_get(hp0).local_set(hp);
        i.i32_const(0).local_set(hi);
        i.block(BlockType::Empty).loop_(BlockType::Empty);
        i.local_get(hi).local_get(hn).i32_ge_u().br_if(1);
        i.local_get(hb).local_get(hp).i32_wrap_i64().i32_add();
        i.i32_load(wasm_encoder::MemArg {
            offset: u64::from(almide_layout::PAYLOAD),
            align: 0,
            memory_index: 0,
        });
        i.i64_extend_i32_u().local_set(hsl);
        // slice = fresh block of sl bytes at p+4, then the lossy decode
        i.local_get(hsl).i32_wrap_i64().call(F_ALLOC).local_set(hs);
        i.local_get(hs).i32_const(almide_layout::PAYLOAD as i32).i32_add();
        i.local_get(hb)
            .i32_const(almide_layout::PAYLOAD as i32)
            .i32_add()
            .local_get(hp)
            .i32_wrap_i64()
            .i32_add()
            .i32_const(4)
            .i32_add();
        i.local_get(hsl).i32_wrap_i64();
        i.memory_copy(0, 0);
        i.local_get(hout).local_get(hi).i32_const(4).i32_mul().i32_add();
        i.local_get(hs).call(lossy);
        i.i32_store(slot_memarg(0));
        i.local_get(hp).i64_const(4).i64_add().local_get(hsl).i64_add().local_set(hp);
        i.local_get(hi).i32_const(1).i32_add().local_set(hi);
        i.br(0).end().end();
        i.local_get(hout);
        let _ = i;
        for _ in 0..4 {
            self.release_i32();
        }
        for _ in 0..3 {
            self.release_i64();
        }
        self.release_i64();
        self.release_i32();
        Ok(Some(SliceTy::List(self.types.intern(STR))))
    }

    /// Append `k` big-endian bytes of the value (LSB-only when k = 1 —
    /// `val as u8`); `float` reinterprets an f64 to its bit pattern
    /// first (write_f64_be).
    pub(crate) fn lower_bytes_write_be(
        &mut self,
        b: &IrExpr,
        v: &IrExpr,
        k: i32,
        float: bool,
    ) -> Result<Option<SliceTy>, EmitError> {
        self.lower(b, Some(BYTES))?;
        let hb = self.hold_i32()?;
        self.f.instructions().local_set(hb);
        self.lower(v, Some(if float { FLOAT } else { INT }))?;
        let hv = self.hold_i64()?;
        let ho = self.hold_i32()?;
        let mut i = self.f.instructions();
        if float {
            i.i64_reinterpret_f64();
        }
        i.local_set(hv);
        i.local_get(hb).i32_load(len_memarg()).i32_const(k).i32_add().call(F_ALLOC);
        i.local_set(ho);
        i.local_get(ho).i32_const(almide_layout::PAYLOAD as i32).i32_add();
        i.local_get(hb).i32_const(almide_layout::PAYLOAD as i32).i32_add();
        i.local_get(hb).i32_load(len_memarg());
        i.memory_copy(0, 0);
        for j in 0..k {
            // byte_k already carries the PAYLOAD offset — the base here
            // is handle + byte index only (the double-add once landed
            // every cursor byte in the next block's zero header).
            i.local_get(ho).local_get(hb).i32_load(len_memarg()).i32_add();
            i.local_get(hv).i64_const(i64::from((k - 1 - j) * 8)).i64_shr_u().i32_wrap_i64();
            i.i32_store8(byte_k(j as u8));
        }
        i.local_get(ho);
        let _ = i;
        self.release_i32();
        self.release_i64();
        self.release_i32();
        Ok(Some(BYTES))
    }
    /// chunks: `b.chunks(size)` — size <= 0 yields the empty list; a
    /// size past the buffer is one whole chunk (the i64 clamp precedes
    /// every i32 narrowing).
    pub(crate) fn lower_bytes_chunks(&mut self, b: &IrExpr, size: &IrExpr) -> Result<Option<SliceTy>, EmitError> {
        self.lower(b, Some(BYTES))?;
        let hb = self.hold_i32()?;
        self.f.instructions().local_set(hb);
        self.lower(size, Some(INT))?;
        let hs64 = self.hold_i64()?;
        let hs = self.hold_i32()?;
        let ho = self.hold_i32()?;
        let hw = self.hold_i32()?;
        let hoff = self.hold_i32()?;
        let hn = self.hold_i32()?;
        let hc = self.hold_i32()?;
        let mut i = self.f.instructions();
        i.local_set(hs64);
        i.local_get(hb).i32_load(len_memarg()).local_set(hn);
        i.local_get(hs64).i64_const(0).i64_le_s().if_(BlockType::Result(ValType::I32));
        i.i32_const(0).call(F_ALLOC);
        i.else_();
        // s = min(size, max(n, 1)) — an i32-safe stride that still means
        // "one chunk" for any size >= n
        i.local_get(hs64);
        i.local_get(hn).i32_const(1).local_get(hn).i32_const(0).i32_gt_u().select();
        i.i64_extend_i32_u();
        i.local_get(hs64);
        i.local_get(hn).i32_const(1).local_get(hn).i32_const(0).i32_gt_u().select();
        i.i64_extend_i32_u();
        i.i64_lt_s().select();
        i.i32_wrap_i64().local_set(hs);
        // count = ceil(n / s); out = List[Bytes] (4-byte handles)
        i.local_get(hn).local_get(hs).i32_add().i32_const(1).i32_sub();
        i.local_get(hs).i32_div_u();
        i.i32_const(4).i32_mul().call(F_ALLOC).local_set(ho);
        i.i32_const(0).local_set(hw);
        i.i32_const(0).local_set(hoff);
        i.block(BlockType::Empty).loop_(BlockType::Empty);
        i.local_get(hoff).local_get(hn).i32_ge_u().br_if(1);
        // c = min(s, n - off)
        i.local_get(hs);
        i.local_get(hn).local_get(hoff).i32_sub();
        i.local_get(hs).local_get(hn).local_get(hoff).i32_sub().i32_lt_u();
        i.select().local_set(hc);
        i.local_get(hc).call(F_ALLOC);
        i.local_tee(hc); // reuse: chunk handle (byte count now spent)
        i.i32_const(almide_layout::PAYLOAD as i32).i32_add();
        i.local_get(hb).i32_const(almide_layout::PAYLOAD as i32).i32_add().local_get(hoff).i32_add();
        i.local_get(hc).i32_load(len_memarg());
        i.memory_copy(0, 0);
        i.local_get(ho).local_get(hw).i32_add().local_get(hc).i32_store(slot_memarg(0));
        i.local_get(hw).i32_const(4).i32_add().local_set(hw);
        i.local_get(hoff).local_get(hc).i32_load(len_memarg()).i32_add().local_set(hoff);
        i.br(0).end().end();
        i.local_get(ho);
        i.end();
        let _ = i;
        for _ in 0..7 {
            self.release_i32();
        }
        self.release_i64();
        Ok(Some(SliceTy::List(self.types.intern(BYTES))))
    }
    /// C-229 totality: a read DEFAULTS and a write NO-OPS when the window
    /// [pos, pos+width) leaves the buffer — negative pos included, never a
    /// trap. The room test SUBTRACTS (`pos <= len - width`): the
    /// `pos + width` sum wraps for a pos near the top of i64 and once let
    /// a store land back inside the buffer (fuzz seed 510754018593).
    /// Leaves the in-room boolean (i32) on the stack.
    pub(crate) fn bytes_room(&mut self, bh: u32, ih: u32, width: u8) {
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
    pub(crate) fn lower_bytes_read_bits(
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
    pub(crate) fn lower_bytes_set(
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
    pub(crate) fn lower_bytes_rw(
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
}
