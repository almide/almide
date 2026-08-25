//! Bytes — the byte-packed buffer surface (String's layout twin). The
//! oracle allows in-place `set_*` (a `mut`/buffer API); under the
//! bind-deep-copy doctrine a local's block is uniquely its own, so the
//! stores are unobservable through aliases. `bytes.new` relies on the
//! bump allocator's zero guarantee (fresh pages are zero and the bump
//! head never reuses).

use almide_ir::{IrExpr, IrExprKind};
use wasm_encoder::{BlockType, MemArg, ValType};

use crate::emitter::Emitter;
use crate::*;

const BYTES: SliceTy = SliceTy::Scalar(Scalar::Bytes);

fn byte_at() -> MemArg {
    MemArg { offset: u64::from(almide_layout::PAYLOAD), align: 0, memory_index: 0 }
}

/// Payload-relative byte address: byte k of the current window.
impl Emitter<'_> {
    /// Append `k` big-endian bytes of the value (LSB-only when k = 1 —
    /// `val as u8`); `float` reinterprets an f64 to its bit pattern
    /// first (write_f64_be).
    fn lower_bytes_write_be(
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
    fn lower_bytes_chunks(&mut self, b: &IrExpr, size: &IrExpr) -> Result<Option<SliceTy>, EmitError> {
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
}

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
            // some(byte) / none (native b.get — usize-wrap: negative i
            // is huge and misses). Its default is NONE, not 0, so it
            // takes its own guard instead of the bits path.
            ("get", [b, idx]) => {
                self.lower(b, Some(BYTES))?;
                let bh = self.hold_i32()?;
                self.f.instructions().local_set(bh);
                self.lower(idx, Some(INT))?;
                let ih = self.hold_i64()?;
                let hr = self.hold_i32()?;
                self.f.instructions().local_set(ih);
                self.bytes_room(bh, ih, 1);
                let mut i = self.f.instructions();
                i.if_(BlockType::Result(ValType::I32));
                i.i32_const(8).call(F_ALLOC).local_tee(hr);
                i.local_get(bh).local_get(ih).i32_wrap_i64().i32_add();
                i.i64_load8_u(byte_k(0));
                i.i64_store(slot_memarg(almide_layout::OPTION_FIELD));
                i.local_get(hr);
                i.else_();
                i.i32_const(0);
                i.end();
                let _ = i;
                self.release_i32();
                self.release_i64();
                self.release_i32();
                Ok(Some(SliceTy::Option(self.types.intern(INT))))
            }
            // Functional set: a fresh copy, one in-range byte replaced
            // (native clone + guarded store).
            ("set", [b, idx, v]) => {
                self.lower(b, Some(BYTES))?;
                self.f.instructions().call(F_BLOCK_COPY);
                let bh = self.hold_i32()?;
                self.f.instructions().local_set(bh);
                self.lower(idx, Some(INT))?;
                let ih = self.hold_i64()?;
                self.f.instructions().local_set(ih);
                self.lower(v, Some(INT))?;
                let hv = self.hold_i64()?;
                self.f.instructions().local_set(hv);
                self.bytes_room(bh, ih, 1);
                let mut i = self.f.instructions();
                i.if_(BlockType::Empty);
                i.local_get(bh).local_get(ih).i32_wrap_i64().i32_add();
                i.local_get(hv).i64_store8(byte_k(0));
                i.end();
                i.local_get(bh);
                let _ = i;
                self.release_i64();
                self.release_i64();
                self.release_i32();
                Ok(Some(BYTES))
            }
            // MUT push (native b.push): copy-grow 1, store, write back.
            ("push", [b, v]) | ("append_u8", [b, v]) => {
                let IrExprKind::Var { id } = &b.kind else {
                    return unsup("bytes-push-nonvar");
                };
                let Some(&(var_idx, var_ty)) = self.locals.get(id) else {
                    return unsup("var:unmapped");
                };
                self.f.instructions().local_get(var_idx);
                if self.cells.contains(id) {
                    self.load_ty_slot(var_ty, 0);
                }
                let bh = self.hold_i32()?;
                self.f.instructions().local_set(bh);
                self.lower(v, Some(INT))?;
                let hv = self.hold_i64()?;
                self.f.instructions().local_set(hv);
                let (len_h, rh) = self.emit_copy_grow(bh, 1)?;
                self.f
                    .instructions()
                    .local_get(rh)
                    .i32_const(almide_layout::PAYLOAD as i32)
                    .i32_add()
                    .local_get(len_h)
                    .i32_add()
                    .local_get(hv)
                    .i64_store8(MemArg { offset: 0, align: 0, memory_index: 0 });
                self.f.instructions().local_get(rh);
                self.emit_store_var(*id, var_idx, var_ty)?;
                self.release_i32();
                self.release_i32();
                self.release_i64();
                self.release_i32();
                Ok(None)
            }
            // pad to target with `val` on the chosen side; target <= len
            // (negative INCLUDED — the signed read, both legs) is a copy.
            ("pad_left" | "pad_right", [b, target, v]) => {
                let left = func == "pad_left";
                self.lower(b, Some(BYTES))?;
                let bh = self.hold_i32()?;
                self.f.instructions().local_set(bh);
                self.lower(target, Some(INT))?;
                let ht = self.hold_i64()?;
                self.f.instructions().local_set(ht);
                self.lower(v, Some(INT))?;
                let hv = self.hold_i64()?;
                let ho = self.hold_i32()?;
                let hp = self.hold_i32()?;
                let mut i = self.f.instructions();
                i.local_set(hv);
                i.local_get(ht);
                i.local_get(bh).i32_load(len_memarg()).i64_extend_i32_u();
                i.i64_le_s().if_(BlockType::Result(ValType::I32));
                i.local_get(bh).call(F_BLOCK_COPY);
                i.else_();
                i.local_get(ht).i32_wrap_i64().call(F_ALLOC).local_set(ho);
                // pad = target - len bytes of val
                i.local_get(ht)
                    .i32_wrap_i64()
                    .local_get(bh)
                    .i32_load(len_memarg())
                    .i32_sub()
                    .local_set(hp);
                // fill zone start: left → payload; right → payload + len
                i.local_get(ho).i32_const(almide_layout::PAYLOAD as i32).i32_add();
                if !left {
                    i.local_get(bh).i32_load(len_memarg()).i32_add();
                }
                i.local_get(hv).i32_wrap_i64();
                i.local_get(hp);
                i.memory_fill(0);
                // the source bytes: left → after the pad; right → front
                i.local_get(ho).i32_const(almide_layout::PAYLOAD as i32).i32_add();
                if left {
                    i.local_get(hp).i32_add();
                }
                i.local_get(bh).i32_const(almide_layout::PAYLOAD as i32).i32_add();
                i.local_get(bh).i32_load(len_memarg());
                i.memory_copy(0, 0);
                i.local_get(ho);
                i.end();
                let _ = i;
                self.release_i32();
                self.release_i32();
                self.release_i64();
                self.release_i64();
                self.release_i32();
                Ok(Some(BYTES))
            }
            // MUT window copy (native copy_from): either offset past its
            // buffer is a no-op; len clamps to both remainders.
            ("copy_from", [dst, src, doff, soff, n]) => {
                let IrExprKind::Var { id } = &dst.kind else {
                    return unsup("bytes-copy-from-nonvar");
                };
                let Some(&(var_idx, var_ty)) = self.locals.get(id) else {
                    return unsup("var:unmapped");
                };
                self.f.instructions().local_get(var_idx);
                if self.cells.contains(id) {
                    self.load_ty_slot(var_ty, 0);
                }
                self.f.instructions().call(F_BLOCK_COPY);
                let dh = self.hold_i32()?;
                self.f.instructions().local_set(dh);
                self.lower(src, Some(BYTES))?;
                let sh = self.hold_i32()?;
                self.f.instructions().local_set(sh);
                self.lower(doff, Some(INT))?;
                let hdo = self.hold_i64()?;
                self.f.instructions().local_set(hdo);
                self.lower(soff, Some(INT))?;
                let hso = self.hold_i64()?;
                self.f.instructions().local_set(hso);
                self.lower(n, Some(INT))?;
                let hn = self.hold_i64()?;
                let hl = self.hold_i32()?;
                let mut i = self.f.instructions();
                i.local_set(hn);
                // in-range offsets? (usize-wrap: negative = huge = miss)
                i.local_get(hdo).i64_const(0).i64_ge_s();
                i.local_get(hdo);
                i.local_get(dh).i32_load(len_memarg()).i64_extend_i32_u();
                i.i64_lt_s().i32_and();
                i.local_get(hso).i64_const(0).i64_ge_s().i32_and();
                i.local_get(hso);
                i.local_get(sh).i32_load(len_memarg()).i64_extend_i32_u();
                i.i64_lt_s().i32_and();
                i.local_get(hn).i64_const(0).i64_ge_s().i32_and();
                i.if_(BlockType::Empty);
                // len = min(n, dst_rem, src_rem) — select(v1,v2,cond) =
                // cond ? v1 : v2, so the REMAINDER is v1 under n > rem.
                i.local_get(dh)
                    .i32_load(len_memarg())
                    .local_get(hdo)
                    .i32_wrap_i64()
                    .i32_sub();
                i.local_get(hn).i32_wrap_i64();
                i.local_get(hn).i32_wrap_i64();
                i.local_get(dh)
                    .i32_load(len_memarg())
                    .local_get(hdo)
                    .i32_wrap_i64()
                    .i32_sub();
                i.i32_gt_u().select();
                i.local_set(hl);
                i.local_get(sh)
                    .i32_load(len_memarg())
                    .local_get(hso)
                    .i32_wrap_i64()
                    .i32_sub();
                i.local_get(hl);
                i.local_get(hl);
                i.local_get(sh)
                    .i32_load(len_memarg())
                    .local_get(hso)
                    .i32_wrap_i64()
                    .i32_sub();
                i.i32_gt_u().select();
                i.local_set(hl);
                i.local_get(dh)
                    .i32_const(almide_layout::PAYLOAD as i32)
                    .i32_add()
                    .local_get(hdo)
                    .i32_wrap_i64()
                    .i32_add();
                i.local_get(sh)
                    .i32_const(almide_layout::PAYLOAD as i32)
                    .i32_add()
                    .local_get(hso)
                    .i32_wrap_i64()
                    .i32_add();
                i.local_get(hl);
                i.memory_copy(0, 0);
                i.end();
                i.local_get(dh);
                let _ = i;
                self.emit_store_var(*id, var_idx, var_ty)?;
                self.release_i32();
                self.release_i64();
                self.release_i64();
                self.release_i64();
                self.release_i32();
                self.release_i32();
                Ok(None)
            }
            // slice: native usize-min clamps — a NEGATIVE bound casts
            // huge and saturates to len (s >= e is the empty buffer).
            ("slice", [b, s, e]) => {
                self.lower(b, Some(BYTES))?;
                let hb = self.hold_i32()?;
                self.f.instructions().local_set(hb);
                self.lower(s, Some(INT))?;
                let hs = self.hold_i64()?;
                self.f.instructions().local_set(hs);
                self.lower(e, Some(INT))?;
                let he = self.hold_i64()?;
                let ho = self.hold_i32()?;
                let mut i = self.f.instructions();
                i.local_set(he);
                for h in [hs, he] {
                    i.local_get(h);
                    i.local_get(hb).i32_load(len_memarg()).i64_extend_i32_u();
                    i.local_get(h)
                        .local_get(hb)
                        .i32_load(len_memarg())
                        .i64_extend_i32_u()
                        .i64_lt_u();
                    i.select().local_set(h);
                }
                i.local_get(hs).local_get(he).i64_ge_u().if_(BlockType::Result(ValType::I32));
                i.i32_const(0).call(F_ALLOC);
                i.else_();
                i.local_get(he).local_get(hs).i64_sub().i32_wrap_i64().call(F_ALLOC).local_set(ho);
                i.local_get(ho).i32_const(almide_layout::PAYLOAD as i32).i32_add();
                i.local_get(hb)
                    .i32_const(almide_layout::PAYLOAD as i32)
                    .i32_add()
                    .local_get(hs)
                    .i32_wrap_i64()
                    .i32_add();
                i.local_get(he).local_get(hs).i64_sub().i32_wrap_i64();
                i.memory_copy(0, 0);
                i.local_get(ho);
                i.end();
                let _ = i;
                self.release_i32();
                self.release_i64();
                self.release_i64();
                self.release_i32();
                Ok(Some(BYTES))
            }
            // The BE serialization cursor (#1099 intrinsics): append k
            // big-endian bytes of the value, mut on the native surface —
            // the push convention (var write-back, no value).
            (
                "write_u8" | "write_u16_be" | "write_u32_be" | "write_i64_be" | "write_f64_be",
                [b, v],
            ) => {
                let IrExprKind::Var { id } = &b.kind else {
                    return unsup("bytes-write-nonvar");
                };
                let Some(&(var_idx, var_ty)) = self.locals.get(id) else {
                    return unsup("var:unmapped");
                };
                let (k, float) = match func {
                    "write_u8" => (1, false),
                    "write_u16_be" => (2, false),
                    "write_u32_be" => (4, false),
                    "write_i64_be" => (8, false),
                    _ => (8, true),
                };
                self.lower_bytes_write_be(b, v, k, float)?;
                self.emit_store_var(*id, var_idx, var_ty)?;
                Ok(None)
            }
            // copy_within: memmove inside the buffer, NO-OP when the
            // range is empty or the destination does not fit (native
            // guard verbatim — the adds wrap in i64 exactly like the
            // release-mode usize arithmetic they mirror).
            ("copy_within", [b, s, e, d]) => {
                let IrExprKind::Var { id } = &b.kind else {
                    return unsup("bytes-copy-within-nonvar");
                };
                let Some(&(var_idx, var_ty)) = self.locals.get(id) else {
                    return unsup("var:unmapped");
                };
                self.lower(b, Some(BYTES))?;
                let hb = self.hold_i32()?;
                self.f.instructions().local_set(hb);
                self.lower(s, Some(INT))?;
                let hs = self.hold_i64()?;
                self.f.instructions().local_set(hs);
                self.lower(e, Some(INT))?;
                let he = self.hold_i64()?;
                self.f.instructions().local_set(he);
                self.lower(d, Some(INT))?;
                let hd = self.hold_i64()?;
                let ho = self.hold_i32()?;
                let mut i = self.f.instructions();
                i.local_set(hd);
                i.local_get(hb).i32_load(len_memarg()).call(F_ALLOC).local_set(ho);
                i.local_get(ho).i32_const(almide_layout::PAYLOAD as i32).i32_add();
                i.local_get(hb).i32_const(almide_layout::PAYLOAD as i32).i32_add();
                i.local_get(hb).i32_load(len_memarg());
                i.memory_copy(0, 0);
                // e = min_u(e, len)
                i.local_get(he);
                i.local_get(hb).i32_load(len_memarg()).i64_extend_i32_u();
                i.local_get(he)
                    .local_get(hb)
                    .i32_load(len_memarg())
                    .i64_extend_i32_u()
                    .i64_lt_u();
                i.select().local_set(he);
                // s < e  &&  d + (e - s) <= len
                i.local_get(hs).local_get(he).i64_lt_u();
                i.local_get(hd).local_get(he).i64_add().local_get(hs).i64_sub();
                i.local_get(hb).i32_load(len_memarg()).i64_extend_i32_u();
                i.i64_le_u();
                i.i32_and().if_(BlockType::Empty);
                i.local_get(ho)
                    .i32_const(almide_layout::PAYLOAD as i32)
                    .i32_add()
                    .local_get(hd)
                    .i32_wrap_i64()
                    .i32_add();
                i.local_get(ho)
                    .i32_const(almide_layout::PAYLOAD as i32)
                    .i32_add()
                    .local_get(hs)
                    .i32_wrap_i64()
                    .i32_add();
                i.local_get(he).local_get(hs).i64_sub().i32_wrap_i64();
                i.memory_copy(0, 0);
                i.end();
                i.local_get(ho);
                let _ = i;
                self.emit_store_var(*id, var_idx, var_ty)?;
                self.release_i32();
                self.release_i64();
                self.release_i64();
                self.release_i64();
                self.release_i32();
                Ok(None)
            }
            // fill: every byte becomes `val as u8` — mut on the native
            // surface (same convention).
            ("fill", [b, v]) => {
                let IrExprKind::Var { id } = &b.kind else {
                    return unsup("bytes-fill-nonvar");
                };
                let Some(&(var_idx, var_ty)) = self.locals.get(id) else {
                    return unsup("var:unmapped");
                };
                self.lower(b, Some(BYTES))?;
                let hb = self.hold_i32()?;
                self.f.instructions().local_set(hb);
                self.lower(v, Some(INT))?;
                let hv = self.hold_i64()?;
                let ho = self.hold_i32()?;
                let mut i = self.f.instructions();
                i.local_set(hv);
                i.local_get(hb).i32_load(len_memarg()).call(F_ALLOC).local_set(ho);
                i.local_get(ho).i32_const(almide_layout::PAYLOAD as i32).i32_add();
                i.local_get(hv).i32_wrap_i64();
                i.local_get(hb).i32_load(len_memarg());
                i.memory_fill(0);
                i.local_get(ho);
                let _ = i;
                self.emit_store_var(*id, var_idx, var_ty)?;
                self.release_i32();
                self.release_i64();
                self.release_i32();
                Ok(None)
            }
            // Concatenation: blocks share the string layout (len = bytes),
            // so the string concat helper applies verbatim.
            ("concat", [a, b]) => {
                self.lower(a, Some(BYTES))?;
                self.lower(b, Some(BYTES))?;
                self.f.instructions().call(F_CONCAT);
                Ok(Some(BYTES))
            }
            // chunks: size <= 0 is the empty list; the last chunk may be
            // short. size clamps through i64 BEFORE any i32 narrowing so
            // a huge size is ONE chunk, never a wrapped count.
            ("chunks", [b, size]) => self.lower_bytes_chunks(b, size),
            // to_string: std::str::from_utf8 verbatim — ok shares the
            // block; err carries the Utf8Error Display line.
            ("to_string", [b]) => {
                let inv_pre = self.pool.intern("invalid UTF-8: invalid utf-8 sequence of ");
                let inv_mid = self.pool.intern(" bytes from index ");
                let inc_pre = self.pool.intern("invalid UTF-8: incomplete utf-8 byte sequence from index ");
                let h = self.work.helper(Helper::BytesToString { inv_pre, inv_mid, inc_pre });
                self.lower(b, Some(BYTES))?;
                self.f.instructions().call(h);
                Ok(Some(SliceTy::Result(self.types.intern(STR), self.types.intern(STR))))
            }
            // One i64 slot per byte (native to_list).
            ("to_list", [b]) => {
                self.lower(b, Some(BYTES))?;
                let bh = self.hold_i32()?;
                let hc = self.hold_i32()?;
                let ho = self.hold_i32()?;
                let mut i = self.f.instructions();
                i.local_set(bh);
                i.local_get(bh).i32_load(len_memarg()).i32_const(8).i32_mul();
                i.call(F_ALLOC).local_set(ho);
                i.i32_const(0).local_set(hc);
                i.block(BlockType::Empty).loop_(BlockType::Empty);
                i.local_get(hc).local_get(bh).i32_load(len_memarg()).i32_ge_u().br_if(1);
                i.local_get(ho).local_get(hc).i32_const(8).i32_mul().i32_add();
                i.local_get(bh).local_get(hc).i32_add().i64_load8_u(byte_k(0));
                i.i64_store(slot_memarg(0));
                i.local_get(hc).i32_const(1).i32_add().local_set(hc);
                i.br(0).end().end();
                i.local_get(ho);
                let _ = i;
                for _ in 0..3 {
                    self.release_i32();
                }
                Ok(Some(SliceTy::List(self.types.intern(INT))))
            }
            // n copies (native n.max(0); the C-197 structural bound dies
            // as OOM — no chosen ceiling, ratified A 2026-08-17).
            ("repeat", [b, n]) => {
                self.lower(b, Some(BYTES))?;
                let bh = self.hold_i32()?;
                self.f.instructions().local_set(bh);
                self.lower(n, Some(INT))?;
                let hn = self.hold_i64()?;
                let ho = self.hold_i32()?;
                let hw = self.hold_i32()?;
                let oom = self.pool.intern("Error: out of memory");
                let mut i = self.f.instructions();
                i.local_set(hn);
                // n = max(n, 0)  (select: v1 first)
                i.local_get(hn).i64_const(0);
                i.local_get(hn).i64_const(0).i64_gt_s();
                i.select().local_set(hn);
                // total = len * n, judged in i64 BEFORE the i32 wrap —
                // past the structural bound is the C-197 die.
                i.local_get(bh).i32_load(len_memarg()).i64_extend_i32_u();
                i.local_get(hn).i64_mul();
                i.i64_const(0x7FFF_0000).i64_gt_s().if_(BlockType::Empty);
                i.i32_const(oom as i32).call(F_EPRINTLN_BLOCK);
                i.i32_const(1).call(F_EXIT_IMPORT).unreachable();
                i.end();
                i.local_get(bh).i32_load(len_memarg()).i64_extend_i32_u();
                i.local_get(hn).i64_mul().i32_wrap_i64();
                i.call(F_ALLOC).local_set(ho);
                i.local_get(ho).i32_const(almide_layout::PAYLOAD as i32).i32_add().local_set(hw);
                i.block(BlockType::Empty).loop_(BlockType::Empty);
                i.local_get(hn).i64_eqz().br_if(1);
                i.local_get(hw);
                i.local_get(bh).i32_const(almide_layout::PAYLOAD as i32).i32_add();
                i.local_get(bh).i32_load(len_memarg());
                i.memory_copy(0, 0);
                i.local_get(hw).local_get(bh).i32_load(len_memarg()).i32_add().local_set(hw);
                i.local_get(hn).i64_const(1).i64_sub().local_set(hn);
                i.br(0).end().end();
                i.local_get(ho);
                let _ = i;
                self.release_i32();
                self.release_i32();
                self.release_i64();
                self.release_i32();
                Ok(Some(BYTES))
            }
            // Native from_utf8_lossy (the WHATWG helper) — the self-host
            // impl is a raw copy and must not shadow this.
            ("to_string_lossy", [b]) => {
                self.lower(b, Some(BYTES))?;
                let lossy = self.work.helper(Helper::Utf8Lossy);
                self.f.instructions().call(lossy);
                Ok(Some(STR))
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
            // The linked append/write family is FUNCTIONAL in the
            // self-host but MUT on the native surface — a statement call
            // on a var writes the fresh result back (the list.push
            // convention).
            (f, [b, ..]) if f.starts_with("append_") || f.starts_with("write_") => {
                let IrExprKind::Var { id } = &b.kind else {
                    return unsup("bytes-append-nonvar");
                };
                let Some(&(var_idx, var_ty)) = self.locals.get(id) else {
                    return unsup("var:unmapped");
                };
                match self.lower_linked_call("bytes", func, args, false)? {
                    Some(SliceTy::Scalar(Scalar::Bytes)) => {}
                    other => return unsup(&format!("bytes-append-ret:{other:?}")),
                }
                self.emit_store_var(*id, var_idx, var_ty)?;
                Ok(None)
            }
            // Not a native arm: the audited linked path before the wall.
            _ => self.lower_linked_call("bytes", func, args, false),
        }
    }
}
