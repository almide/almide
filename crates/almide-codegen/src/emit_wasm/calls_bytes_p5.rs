impl FuncCompiler<'_> {
    /// `bytes.read_<int>_<endian>_at(b, pos) -> (Int, Option[Int])`.
    pub(super) fn emit_cursor_read_int(&mut self, args: &[IrExpr], width: u32, signed: bool, big_endian: bool) {
        let buf = self.scratch.alloc_i32();
        let pos = self.scratch.alloc_i64();
        let pos_i32 = self.scratch.alloc_i32();
        let new_pos = self.scratch.alloc_i64();
        let opt_ptr = self.scratch.alloc_i32();
        let payload = self.scratch.alloc_i32();
        let val = self.scratch.alloc_i64();
        self.emit_expr(&args[0]); wasm!(self.func, { local_set(buf); });
        self.emit_expr(&args[1]); wasm!(self.func, {
            local_set(pos);
            local_get(pos); i32_wrap_i64; local_set(pos_i32);
            // bounds: pos + width <= len?
            local_get(pos_i32); i32_const(width as i32); i32_add;
            local_get(buf); i32_load(0);
            i32_le_u;
            if_empty;
              // in-bounds: read value
        });
        // Push value as i64 (for storing in the option payload).
        if big_endian {
            // BE: byte-by-byte
            wasm!(self.func, { i64_const(0); local_set(val); });
            for i in 0..width {
                let shift = 8 * (width - 1 - i) as i64;
                wasm!(self.func, {
                    local_get(val);
                    local_get(buf); i32_const(self.emitter.layout_reg.fixed_offset(super::engine::layout::STRING, super::engine::layout::string::DATA) as i32); i32_add; local_get(pos_i32); i32_add;
                    i32_load8_u(i as u64);
                    i64_extend_i32_u;
                    i64_const(shift); i64_shl;
                    i64_or;
                    local_set(val);
                });
            }
            // Sign-extend if signed and width < 8
            if signed && width < 8 {
                let pad = 64 - 8 * width as i64;
                wasm!(self.func, {
                    local_get(val); i64_const(pad); i64_shl;
                    i64_const(pad); i64_shr_s;
                    local_set(val);
                });
            }
        } else {
            // LE: native loads
            wasm!(self.func, {
                local_get(buf); i32_const(self.emitter.layout_reg.fixed_offset(super::engine::layout::STRING, super::engine::layout::string::DATA) as i32); i32_add; local_get(pos_i32); i32_add;
            });
            match (width, signed) {
                (1, _) => { wasm!(self.func, { i32_load8_u(0); i64_extend_i32_u; }); }
                (2, false) => { wasm!(self.func, { i32_load16_u(0); i64_extend_i32_u; }); }
                (2, true) => { wasm!(self.func, { i32_load16_s(0); i64_extend_i32_s; }); }
                (4, false) => { wasm!(self.func, { i32_load(0); i64_extend_i32_u; }); }
                (4, true) => { wasm!(self.func, { i32_load(0); i64_extend_i32_s; }); }
                (8, _) => { wasm!(self.func, { i64_load(0); }); }
                _ => panic!("unsupported width {width}"),
            }
            wasm!(self.func, { local_set(val); });
        }
        // alloc 8-byte payload, store val, set opt_ptr
        wasm!(self.func, {
            i32_const(8); call(self.emitter.rt.alloc); local_set(payload);
            local_get(payload); local_get(val); i64_store(0);
            local_get(payload); local_set(opt_ptr);
            local_get(pos); i64_const(width as i64); i64_add; local_set(new_pos);
            else_;
              // out-of-bounds: opt_ptr=0, new_pos=pos
              i32_const(0); local_set(opt_ptr);
              local_get(pos); local_set(new_pos);
            end;
        });
        self.emit_cursor_pack_tuple(new_pos, opt_ptr);
        self.scratch.free_i64(val);
        self.scratch.free_i32(payload);
        self.scratch.free_i32(opt_ptr);
        self.scratch.free_i64(new_pos);
        self.scratch.free_i32(pos_i32);
        self.scratch.free_i64(pos);
        self.scratch.free_i32(buf);
    }

    /// `bytes.read_<float>_<endian>_at(b, pos) -> (Int, Option[Float])`.
    /// Implementation = read_int + reinterpret on the way to the option cell.
    pub(super) fn emit_cursor_read_float(&mut self, args: &[IrExpr], width: u32, big_endian: bool) {
        let buf = self.scratch.alloc_i32();
        let pos = self.scratch.alloc_i64();
        let pos_i32 = self.scratch.alloc_i32();
        let new_pos = self.scratch.alloc_i64();
        let opt_ptr = self.scratch.alloc_i32();
        let payload = self.scratch.alloc_i32();
        let fval = self.scratch.alloc_f64();
        self.emit_expr(&args[0]); wasm!(self.func, { local_set(buf); });
        self.emit_expr(&args[1]); wasm!(self.func, {
            local_set(pos);
            local_get(pos); i32_wrap_i64; local_set(pos_i32);
            local_get(pos_i32); i32_const(width as i32); i32_add;
            local_get(buf); i32_load(0);
            i32_le_u;
            if_empty;
        });
        if big_endian {
            // Build i64 bits BE, then reinterpret to float.
            let bits = self.scratch.alloc_i64();
            wasm!(self.func, { i64_const(0); local_set(bits); });
            for i in 0..width {
                let shift = 8 * (width - 1 - i) as i64;
                wasm!(self.func, {
                    local_get(bits);
                    local_get(buf); i32_const(self.emitter.layout_reg.fixed_offset(super::engine::layout::STRING, super::engine::layout::string::DATA) as i32); i32_add; local_get(pos_i32); i32_add;
                    i32_load8_u(i as u64);
                    i64_extend_i32_u;
                    i64_const(shift); i64_shl;
                    i64_or;
                    local_set(bits);
                });
            }
            if width == 4 {
                wasm!(self.func, {
                    local_get(bits); i32_wrap_i64; f32_reinterpret_i32; f64_promote_f32;
                    local_set(fval);
                });
            } else {
                wasm!(self.func, { local_get(bits); f64_reinterpret_i64; local_set(fval); });
            }
            self.scratch.free_i64(bits);
        } else {
            wasm!(self.func, {
                local_get(buf); i32_const(self.emitter.layout_reg.fixed_offset(super::engine::layout::STRING, super::engine::layout::string::DATA) as i32); i32_add; local_get(pos_i32); i32_add;
            });
            if width == 4 {
                wasm!(self.func, { f32_load(0); f64_promote_f32; local_set(fval); });
            } else {
                wasm!(self.func, { f64_load(0); local_set(fval); });
            }
        }
        wasm!(self.func, {
            i32_const(8); call(self.emitter.rt.alloc); local_set(payload);
            local_get(payload); local_get(fval); f64_store(0);
            local_get(payload); local_set(opt_ptr);
            local_get(pos); i64_const(width as i64); i64_add; local_set(new_pos);
            else_;
              i32_const(0); local_set(opt_ptr);
              local_get(pos); local_set(new_pos);
            end;
        });
        self.emit_cursor_pack_tuple(new_pos, opt_ptr);
        self.scratch.free_f64(fval);
        self.scratch.free_i32(payload);
        self.scratch.free_i32(opt_ptr);
        self.scratch.free_i64(new_pos);
        self.scratch.free_i32(pos_i32);
        self.scratch.free_i64(pos);
        self.scratch.free_i32(buf);
    }

    /// `bytes.take_at(b, pos, n) -> (Int, Option[Bytes])`.
    /// Copies `n` bytes into a fresh Bytes; returns none if `pos + n > len`.
    pub(super) fn emit_cursor_take(&mut self, args: &[IrExpr]) {
        let buf = self.scratch.alloc_i32();
        let pos = self.scratch.alloc_i64();
        let pos_i32 = self.scratch.alloc_i32();
        let n_i32 = self.scratch.alloc_i32();
        let new_pos = self.scratch.alloc_i64();
        let opt_ptr = self.scratch.alloc_i32();
        let dst = self.scratch.alloc_i32();
        self.emit_expr(&args[0]); wasm!(self.func, { local_set(buf); });
        self.emit_expr(&args[1]); wasm!(self.func, {
            local_set(pos);
            local_get(pos); i32_wrap_i64; local_set(pos_i32);
        });
        self.emit_expr(&args[2]); wasm!(self.func, {
            i32_wrap_i64; local_set(n_i32);
            local_get(pos_i32); local_get(n_i32); i32_add;
            local_get(buf); i32_load(0);
            i32_le_u;
            if_empty;
              // alloc Bytes: 4 + n bytes
              local_get(n_i32); i32_const(self.emitter.layout_reg.fixed_offset(super::engine::layout::STRING, super::engine::layout::string::DATA) as i32); i32_add;
              call(self.emitter.rt.alloc); local_set(dst);
              local_get(dst); local_get(n_i32); i32_store(0);
              // memcpy data
              local_get(dst); i32_const(self.emitter.layout_reg.fixed_offset(super::engine::layout::STRING, super::engine::layout::string::DATA) as i32); i32_add;
              local_get(buf); i32_const(self.emitter.layout_reg.fixed_offset(super::engine::layout::STRING, super::engine::layout::string::DATA) as i32); i32_add; local_get(pos_i32); i32_add;
              local_get(n_i32);
              memory_copy;
              // Wrap the Bytes pointer in an Option cell (4 bytes).
              i32_const(4); call(self.emitter.rt.alloc); local_set(opt_ptr);
              local_get(opt_ptr); local_get(dst); i32_store(0);
              local_get(pos); local_get(n_i32); i64_extend_i32_u; i64_add; local_set(new_pos);
            else_;
              i32_const(0); local_set(opt_ptr);
              local_get(pos); local_set(new_pos);
            end;
        });
        self.emit_cursor_pack_tuple(new_pos, opt_ptr);
        self.scratch.free_i32(dst);
        self.scratch.free_i32(opt_ptr);
        self.scratch.free_i64(new_pos);
        self.scratch.free_i32(n_i32);
        self.scratch.free_i32(pos_i32);
        self.scratch.free_i64(pos);
        self.scratch.free_i32(buf);
    }

    /// `bytes.read_bool_at(b, pos) -> (Int, Option[Bool])`.
    /// Option[Bool] payload is a 4-byte i32 cell (0 or 1).
    pub(super) fn emit_cursor_read_bool(&mut self, args: &[IrExpr]) {
        let buf = self.scratch.alloc_i32();
        let pos = self.scratch.alloc_i64();
        let pos_i32 = self.scratch.alloc_i32();
        let new_pos = self.scratch.alloc_i64();
        let opt_ptr = self.scratch.alloc_i32();
        let payload = self.scratch.alloc_i32();
        self.emit_expr(&args[0]); wasm!(self.func, { local_set(buf); });
        self.emit_expr(&args[1]); wasm!(self.func, {
            local_set(pos);
            local_get(pos); i32_wrap_i64; local_set(pos_i32);
            local_get(pos_i32); i32_const(1); i32_add;
            local_get(buf); i32_load(0);
            i32_le_u;
            if_empty;
              i32_const(4); call(self.emitter.rt.alloc); local_set(payload);
              local_get(payload);
              local_get(buf); i32_const(self.emitter.layout_reg.fixed_offset(super::engine::layout::STRING, super::engine::layout::string::DATA) as i32); i32_add; local_get(pos_i32); i32_add;
              i32_load8_u(0); i32_const(0); i32_ne;
              i32_store(0);
              local_get(payload); local_set(opt_ptr);
              local_get(pos); i64_const(1); i64_add; local_set(new_pos);
            else_;
              i32_const(0); local_set(opt_ptr);
              local_get(pos); local_set(new_pos);
            end;
        });
        self.emit_cursor_pack_tuple(new_pos, opt_ptr);
        self.scratch.free_i32(payload);
        self.scratch.free_i32(opt_ptr);
        self.scratch.free_i64(new_pos);
        self.scratch.free_i32(pos_i32);
        self.scratch.free_i64(pos);
        self.scratch.free_i32(buf);
    }

    /// `bytes.read_f16_le_at(b, pos) -> (Int, Option[Float])`.
    /// Reads 2 bytes LE, expands half → f64 via the `__bytes_f16_to_f64`
    /// runtime helper, stores in an 8-byte payload cell.
    pub(super) fn emit_cursor_read_f16_le(&mut self, args: &[IrExpr]) {
        let buf = self.scratch.alloc_i32();
        let pos = self.scratch.alloc_i64();
        let pos_i32 = self.scratch.alloc_i32();
        let new_pos = self.scratch.alloc_i64();
        let opt_ptr = self.scratch.alloc_i32();
        let payload = self.scratch.alloc_i32();
        let fval = self.scratch.alloc_f64();
        self.emit_expr(&args[0]); wasm!(self.func, { local_set(buf); });
        self.emit_expr(&args[1]); wasm!(self.func, {
            local_set(pos);
            local_get(pos); i32_wrap_i64; local_set(pos_i32);
            local_get(pos_i32); i32_const(2); i32_add;
            local_get(buf); i32_load(0);
            i32_le_u;
            if_empty;
              local_get(buf); i32_const(self.emitter.layout_reg.fixed_offset(super::engine::layout::STRING, super::engine::layout::string::DATA) as i32); i32_add; local_get(pos_i32); i32_add;
              i32_load16_u(0);
              call(self.emitter.rt.bytes_f16_to_f64);
              local_set(fval);
              i32_const(8); call(self.emitter.rt.alloc); local_set(payload);
              local_get(payload); local_get(fval); f64_store(0);
              local_get(payload); local_set(opt_ptr);
              local_get(pos); i64_const(2); i64_add; local_set(new_pos);
            else_;
              i32_const(0); local_set(opt_ptr);
              local_get(pos); local_set(new_pos);
            end;
        });
        self.emit_cursor_pack_tuple(new_pos, opt_ptr);
        self.scratch.free_f64(fval);
        self.scratch.free_i32(payload);
        self.scratch.free_i32(opt_ptr);
        self.scratch.free_i64(new_pos);
        self.scratch.free_i32(pos_i32);
        self.scratch.free_i64(pos);
        self.scratch.free_i32(buf);
    }

    /// `bytes.read_string_be_at(b, pos) -> (Int, Option[String])`.
    /// u32 big-endian length prefix, then UTF-8 body. Returns
    /// `(pos, None)` without advancing when either the prefix or the body
    /// runs off the end.
    pub(super) fn emit_cursor_read_string_be(&mut self, args: &[IrExpr]) {
        let buf = self.scratch.alloc_i32();
        let pos = self.scratch.alloc_i64();
        let pos_i32 = self.scratch.alloc_i32();
        let new_pos = self.scratch.alloc_i64();
        let opt_ptr = self.scratch.alloc_i32();
        let slen = self.scratch.alloc_i32();
        let str_ptr = self.scratch.alloc_i32();
        let buf_len = self.scratch.alloc_i32();
        self.emit_expr(&args[0]); wasm!(self.func, { local_set(buf); });
        self.emit_expr(&args[1]); wasm!(self.func, {
            local_set(pos);
            local_get(pos); i32_wrap_i64; local_set(pos_i32);
            local_get(buf); i32_load(0); local_set(buf_len);
            // Prefix bounds: pos + 4 (u32 prefix size) <= len?
            local_get(pos_i32); i32_const(4); i32_add;
            local_get(buf_len); i32_le_u;
            if_empty;
              // Read u32 BE length (4 bytes, big-endian).
              i32_const(0); local_set(slen);
              local_get(slen);
              local_get(buf); i32_const(self.emitter.layout_reg.fixed_offset(super::engine::layout::STRING, super::engine::layout::string::DATA) as i32); i32_add; local_get(pos_i32); i32_add;
              i32_load8_u(0); i32_const(24); i32_shl; i32_or;
              local_get(buf); i32_const(self.emitter.layout_reg.fixed_offset(super::engine::layout::STRING, super::engine::layout::string::DATA) as i32); i32_add; local_get(pos_i32); i32_add;
              i32_load8_u(1); i32_const(16); i32_shl; i32_or;
              local_get(buf); i32_const(self.emitter.layout_reg.fixed_offset(super::engine::layout::STRING, super::engine::layout::string::DATA) as i32); i32_add; local_get(pos_i32); i32_add;
              i32_load8_u(2); i32_const(8); i32_shl; i32_or;
              local_get(buf); i32_const(self.emitter.layout_reg.fixed_offset(super::engine::layout::STRING, super::engine::layout::string::DATA) as i32); i32_add; local_get(pos_i32); i32_add;
              i32_load8_u(3); i32_or;
              local_set(slen);
              // Body bounds: pos + 4 + slen <= len?
              local_get(pos_i32); i32_const(4); i32_add; local_get(slen); i32_add;
              local_get(buf_len); i32_le_u;
              if_empty;
                // Alloc String: [len:i32][utf8...]
                local_get(slen); i32_const(self.emitter.layout_reg.fixed_offset(super::engine::layout::STRING, super::engine::layout::string::DATA) as i32); i32_add;
                call(self.emitter.rt.alloc); local_set(str_ptr);
                local_get(str_ptr); local_get(slen); i32_store(0); // len
                local_get(str_ptr); local_get(slen); i32_store(self.emitter.layout_reg.fixed_offset(super::engine::layout::STRING, super::engine::layout::string::CAP)); // cap
                local_get(str_ptr); i32_const(self.emitter.layout_reg.fixed_offset(super::engine::layout::STRING, super::engine::layout::string::DATA) as i32); i32_add;
                local_get(buf); i32_const(self.emitter.layout_reg.fixed_offset(super::engine::layout::STRING, super::engine::layout::string::DATA) as i32); i32_add; local_get(pos_i32); i32_add; i32_const(4); i32_add;
                local_get(slen);
                memory_copy;
                // Option[String] cell is a 4-byte pointer wrapper.
                i32_const(4); call(self.emitter.rt.alloc); local_set(opt_ptr);
                local_get(opt_ptr); local_get(str_ptr); i32_store(0);
                // new_pos = pos + 4 + slen
                local_get(pos); i64_const(4); i64_add;
                local_get(slen); i64_extend_i32_u; i64_add;
                local_set(new_pos);
              else_;
                i32_const(0); local_set(opt_ptr);
                local_get(pos); local_set(new_pos);
              end;
            else_;
              i32_const(0); local_set(opt_ptr);
              local_get(pos); local_set(new_pos);
            end;
        });
        self.emit_cursor_pack_tuple(new_pos, opt_ptr);
        self.scratch.free_i32(buf_len);
        self.scratch.free_i32(str_ptr);
        self.scratch.free_i32(slen);
        self.scratch.free_i32(opt_ptr);
        self.scratch.free_i64(new_pos);
        self.scratch.free_i32(pos_i32);
        self.scratch.free_i64(pos);
        self.scratch.free_i32(buf);
    }

    // ── Typed byte IO with runtime Endian dispatch ─────────────────
    // Args are (b, offset_or_value, endian). `endian` is a bare Endian
    // variant tag (i32): 0 = LittleEndian, 1 = BigEndian. The emitter
    // evaluates the tag once, branches on it, and reuses the existing
    // `_le` / `_be` low-level emitters inside each arm. `b` and the
    // second arg are re-emitted per branch; user test cases pass Var /
    // literal here, so the double emit is free.

    /// `read_uintN` / `read_intN(b, offset, endian) -> UIntN / IntN`.
    /// The inner LE/BE emitters produce i64 (Almide's canonical integer
    /// width for bytes APIs); for the typed form the return is a sized
    /// numeric (UInt16 / UInt32 / Int32) which maps to WASM `i32`, so
    /// we `i32_wrap_i64` after the branch joins.
    pub(super) fn emit_bytes_read_typed_int(&mut self, args: &[IrExpr], size_bytes: u32, signed: bool) {
        self.emit_expr(&args[2]);
        // Endian is a nullary variant — tag at [ptr + 0]. 0 = LittleEndian.
        wasm!(self.func, { i32_load(0); i32_eqz; if_i64; });
        // LE branch — reuse the LE path via typed_byte_read.
        let op = match (size_bytes, signed) {
            (2, false) => ByteReadOp::U16Le,
            (2, true) => ByteReadOp::I16Le,
            (4, false) => ByteReadOp::U32Le,
            (4, true) => ByteReadOp::I32Le,
            _ => unreachable!("unsupported typed int read size {}", size_bytes),
        };
        self.emit_typed_byte_read(&args[0], &args[1], op);
        wasm!(self.func, { else_; });
        self.emit_byte_read_be_int(&args[0], &args[1], size_bytes, signed);
        wasm!(self.func, { end; i32_wrap_i64; });
    }

    /// `read_float32(b, offset, endian) -> Float32`. Inner emitters
    /// produce f64 (canonical Almide float width); the typed form
    /// demotes to f32 at the join.
    pub(super) fn emit_bytes_read_typed_float(&mut self, args: &[IrExpr], size_bytes: u32) {
        self.emit_expr(&args[2]);
        wasm!(self.func, { i32_load(0); });
        wasm!(self.func, { i32_eqz; if_f64; });
        let op = match size_bytes {
            4 => ByteReadOp::F32Le,
            8 => ByteReadOp::F64Le,
            _ => unreachable!("unsupported typed float read size {}", size_bytes),
        };
        self.emit_typed_byte_read(&args[0], &args[1], op);
        wasm!(self.func, { else_; });
        self.emit_byte_read_be_float(&args[0], &args[1], size_bytes);
        wasm!(self.func, { end; f32_demote_f64; });
    }

    /// `write_uintN / write_intN(b, value, endian) -> Unit`.
    /// The value arg arrives as `i32` (sized numeric). The untyped
    /// `emit_bytes_append_i` expects `i64` (Almide canonical width),
    /// so we synthesise a widened IR expr before delegating.
    pub(super) fn emit_bytes_write_typed_int(&mut self, args: &[IrExpr], size_bytes: u32) {
        self.emit_bytes_typed_append_inline(&args[0], &args[1], &args[2], size_bytes, /*is_float=*/ false);
    }

    /// `write_float32(b, value, endian) -> Unit`. The value is `f32`
    /// at the typed surface; inner emitters take canonical `f64`.
    pub(super) fn emit_bytes_write_typed_float(&mut self, args: &[IrExpr], size_bytes: u32) {
        self.emit_bytes_typed_append_inline(&args[0], &args[1], &args[2], size_bytes, /*is_float=*/ true);
    }

    /// `set_uintN / set_intN(b, offset, value, endian) -> Unit`.
    pub(super) fn emit_bytes_set_typed_int(&mut self, args: &[IrExpr], size_bytes: u32) {
        self.emit_bytes_typed_set_inline(&args[0], &args[1], &args[2], &args[3], size_bytes, /*is_float=*/ false);
    }

    /// `set_float32(b, offset, value, endian) -> Unit`.
    pub(super) fn emit_bytes_set_typed_float(&mut self, args: &[IrExpr], size_bytes: u32) {
        self.emit_bytes_typed_set_inline(&args[0], &args[1], &args[2], &args[3], size_bytes, /*is_float=*/ true);
    }

    /// Inline typed `bytes.write_<T>` emission — handles f32/i32 value
    /// widths and Endian variant tag dispatch in a single pass. No
    /// delegation to the untyped `emit_bytes_append_i` helpers because
    /// those assume an i64 value slot that we'd need to synthesise.
    fn emit_bytes_typed_append_inline(
        &mut self,
        buf_expr: &IrExpr,
        val_expr: &IrExpr,
        endian_expr: &IrExpr,
        size_bytes: u32,
        is_float: bool,
    ) {
        let buf = self.scratch.alloc_i32();
        let old_len = self.scratch.alloc_i32();
        let new_buf = self.scratch.alloc_i32();
        let endian_tag = self.scratch.alloc_i32();
        let val_i64 = self.scratch.alloc_i64();
        let val_f64 = self.scratch.alloc_f64();

        self.emit_expr(buf_expr);
        wasm!(self.func, { local_set(buf); });

        // Normalise value to canonical width (i64 for int, f64 for float).
        self.emit_expr(val_expr);
        if is_float {
            if is_sized_f32_val(val_expr) { wasm!(self.func, { f64_promote_f32; }); }
            wasm!(self.func, { local_set(val_f64); });
        } else {
            if is_sized_i32_val(val_expr) { wasm!(self.func, { i64_extend_i32_u; }); }
            wasm!(self.func, { local_set(val_i64); });
        }

        self.emit_expr(endian_expr);
        wasm!(self.func, { i32_load(0); local_set(endian_tag); });

        // Alloc fresh buffer wider by `size_bytes`, memcpy old data.
        wasm!(self.func, {
            local_get(buf); i32_load(0); local_set(old_len);
            local_get(old_len); i32_const(self.emitter.layout_reg.header_size(super::engine::layout::STRING) as i32 + size_bytes as i32); i32_add;
            call(self.emitter.rt.alloc); local_set(new_buf);
            local_get(new_buf); local_get(old_len); i32_const(size_bytes as i32); i32_add; i32_store(0);
            local_get(new_buf); local_get(old_len); i32_const(size_bytes as i32); i32_add; i32_store(self.emitter.layout_reg.fixed_offset(super::engine::layout::STRING, super::engine::layout::string::CAP));
            local_get(new_buf); i32_const(self.emitter.layout_reg.fixed_offset(super::engine::layout::STRING, super::engine::layout::string::DATA) as i32); i32_add;
            local_get(buf); i32_const(self.emitter.layout_reg.fixed_offset(super::engine::layout::STRING, super::engine::layout::string::DATA) as i32); i32_add;
            local_get(old_len);
            memory_copy;
        });

        // Store destination = new_buf + 4 + old_len.
        wasm!(self.func, { local_get(endian_tag); i32_eqz; if_empty; });
        self.emit_typed_append_store(new_buf, old_len, val_i64, val_f64, size_bytes, is_float, /*be=*/ false);
        wasm!(self.func, { else_; });
        self.emit_typed_append_store(new_buf, old_len, val_i64, val_f64, size_bytes, is_float, /*be=*/ true);
        wasm!(self.func, { end; });

        // #525 (A8): route through the SHARED write-back — the hand-rolled
        // var_map-only form silently lost the realloc'd buffer for a
        // module-global Bytes var and stored the buffer pointer OVER a
        // shared-cell capture's cell pointer (the Closure-v2 P6 corruption).
        self.emit_mutator_writeback(buf_expr, new_buf);

        self.scratch.free_f64(val_f64);
        self.scratch.free_i64(val_i64);
        self.scratch.free_i32(endian_tag);
        self.scratch.free_i32(new_buf);
        self.scratch.free_i32(old_len);
        self.scratch.free_i32(buf);
    }

    /// Inline typed `bytes.set_<T>` — mutates the buffer at `offset`
    /// in-place. No allocation, no length change, no `var` rebind.
    fn emit_bytes_typed_set_inline(
        &mut self,
        buf_expr: &IrExpr,
        offset_expr: &IrExpr,
        val_expr: &IrExpr,
        endian_expr: &IrExpr,
        size_bytes: u32,
        is_float: bool,
    ) {
        let buf = self.scratch.alloc_i32();
        let offset = self.scratch.alloc_i32();
        let endian_tag = self.scratch.alloc_i32();
        let val_i64 = self.scratch.alloc_i64();
        let val_f64 = self.scratch.alloc_f64();

        self.emit_expr(buf_expr);
        wasm!(self.func, { local_set(buf); });
        self.emit_expr(offset_expr);
        wasm!(self.func, { i32_wrap_i64; local_set(offset); });
        self.emit_expr(val_expr);
        if is_float {
            if is_sized_f32_val(val_expr) { wasm!(self.func, { f64_promote_f32; }); }
            wasm!(self.func, { local_set(val_f64); });
        } else {
            if is_sized_i32_val(val_expr) { wasm!(self.func, { i64_extend_i32_u; }); }
            wasm!(self.func, { local_set(val_i64); });
        }
        self.emit_expr(endian_expr);
        wasm!(self.func, { i32_load(0); local_set(endian_tag); });

        wasm!(self.func, { local_get(endian_tag); i32_eqz; if_empty; });
        self.emit_typed_set_store(buf, offset, val_i64, val_f64, size_bytes, is_float, /*be=*/ false);
        wasm!(self.func, { else_; });
        self.emit_typed_set_store(buf, offset, val_i64, val_f64, size_bytes, is_float, /*be=*/ true);
        wasm!(self.func, { end; });

        self.scratch.free_f64(val_f64);
        self.scratch.free_i64(val_i64);
        self.scratch.free_i32(endian_tag);
        self.scratch.free_i32(offset);
        self.scratch.free_i32(buf);
    }

    /// Shared store body for typed append: address is `new_buf + 4 + old_len`.
    fn emit_typed_append_store(
        &mut self,
        new_buf: u32,
        old_len: u32,
        val_i64: u32,
        val_f64: u32,
        size_bytes: u32,
        is_float: bool,
        be: bool,
    ) {
        wasm!(self.func, {
            local_get(new_buf); i32_const(self.emitter.layout_reg.fixed_offset(super::engine::layout::STRING, super::engine::layout::string::DATA) as i32); i32_add; local_get(old_len); i32_add;
        });
        self.emit_typed_store_body(val_i64, val_f64, size_bytes, is_float, be);
    }

    /// Shared store body for typed set: address is `buf + 4 + offset`.
    fn emit_typed_set_store(
        &mut self,
        buf: u32,
        offset: u32,
        val_i64: u32,
        val_f64: u32,
        size_bytes: u32,
        is_float: bool,
        be: bool,
    ) {
        wasm!(self.func, {
            local_get(buf); i32_const(self.emitter.layout_reg.fixed_offset(super::engine::layout::STRING, super::engine::layout::string::DATA) as i32); i32_add; local_get(offset); i32_add;
        });
        self.emit_typed_store_body(val_i64, val_f64, size_bytes, is_float, be);
    }

    /// Emit the width+endian specific store instructions. Address is
    /// already on the stack; this finishes the memory write.
    fn emit_typed_store_body(
        &mut self,
        val_i64: u32,
        val_f64: u32,
        size_bytes: u32,
        is_float: bool,
        be: bool,
    ) {
        if is_float {
            match (size_bytes, be) {
                (4, false) => { wasm!(self.func, { local_get(val_f64); f32_demote_f64; f32_store(0); }); }
                (8, false) => { wasm!(self.func, { local_get(val_f64); f64_store(0); }); }
                (4, true) => {
                    wasm!(self.func, { local_get(val_f64); f32_demote_f64; i32_reinterpret_f32; });
                    self.emit_bswap32_on_stack();
                    wasm!(self.func, { i32_store(0); });
                }
                (8, true) => {
                    wasm!(self.func, { local_get(val_f64); i64_reinterpret_f64; });
                    self.emit_bswap64_on_stack();
                    wasm!(self.func, { i64_store(0); });
                }
                _ => panic!("typed float store: unsupported size {}", size_bytes),
            }
        } else {
            match (size_bytes, be) {
                (1, _) => { wasm!(self.func, { local_get(val_i64); i32_wrap_i64; i32_store8(0); }); }
                (2, false) => { wasm!(self.func, { local_get(val_i64); i32_wrap_i64; i32_store16(0); }); }
                (4, false) => { wasm!(self.func, { local_get(val_i64); i32_wrap_i64; i32_store(0); }); }
                (8, false) => { wasm!(self.func, { local_get(val_i64); i64_store(0); }); }
                (2, true) => {
                    wasm!(self.func, { local_get(val_i64); i32_wrap_i64; });
                    self.emit_bswap16_on_stack();
                    wasm!(self.func, { i32_store16(0); });
                }
                (4, true) => {
                    wasm!(self.func, { local_get(val_i64); i32_wrap_i64; });
                    self.emit_bswap32_on_stack();
                    wasm!(self.func, { i32_store(0); });
                }
                (8, true) => {
                    wasm!(self.func, { local_get(val_i64); });
                    self.emit_bswap64_on_stack();
                    wasm!(self.func, { i64_store(0); });
                }
                _ => panic!("typed int store: unsupported size {}", size_bytes),
            }
        }
    }

    /// Reverse the low 16 bits of the i32 on top of the stack.
    fn emit_bswap16_on_stack(&mut self) {
        let v = self.scratch.alloc_i32();
        wasm!(self.func, {
            local_set(v);
            local_get(v); i32_const(8); i32_shr_u;
            local_get(v); i32_const(0xFF); i32_and; i32_const(8); i32_shl;
            i32_or;
        });
        self.scratch.free_i32(v);
    }

    /// Reverse the four bytes of the i32 on top of the stack.
    fn emit_bswap32_on_stack(&mut self) {
        let v = self.scratch.alloc_i32();
        wasm!(self.func, {
            local_set(v);
            // byte3 → byte0
            local_get(v); i32_const(24); i32_shr_u;
            // byte2 → byte1
            local_get(v); i32_const(8); i32_shr_u; i32_const(0xFF00); i32_and;
            i32_or;
            // byte1 → byte2
            local_get(v); i32_const(8); i32_shl;
            i32_const(0x00FF0000_u32 as i32); i32_and;
            i32_or;
            // byte0 → byte3
            local_get(v); i32_const(24); i32_shl;
            i32_or;
        });
        self.scratch.free_i32(v);
    }

    /// Reverse the eight bytes of the i64 on top of the stack.
    fn emit_bswap64_on_stack(&mut self) {
        let lo = self.scratch.alloc_i32();
        let hi = self.scratch.alloc_i32();
        let v = self.scratch.alloc_i64();
        wasm!(self.func, {
            local_set(v);
            local_get(v); i32_wrap_i64; local_set(lo);
            local_get(v); i64_const(32); i64_shr_u; i32_wrap_i64; local_set(hi);
        });
        wasm!(self.func, { local_get(lo); });
        self.emit_bswap32_on_stack();
        wasm!(self.func, { i64_extend_i32_u; i64_const(32); i64_shl; });
        wasm!(self.func, { local_get(hi); });
        self.emit_bswap32_on_stack();
        wasm!(self.func, { i64_extend_i32_u; i64_or; });
        self.scratch.free_i64(v);
        self.scratch.free_i32(hi);
        self.scratch.free_i32(lo);
    }
}
