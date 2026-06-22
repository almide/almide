impl FuncCompiler<'_> {
    /// Emit `[data_ptr + pos]` loaded as the requested primitive type.
    /// `buf` is the bytes pointer (Bytes layout: [len:i32][data...]).
    /// `pos` is an Int (i64) byte offset into the data region.
    fn emit_typed_byte_read(&mut self, buf_expr: &IrExpr, pos_expr: &IrExpr, op: ByteReadOp) {
        // Compute address = buf + 4 + pos.
        self.emit_expr(buf_expr);
        wasm!(self.func, { i32_const(self.emitter.layout_reg.fixed_offset(super::engine::layout::STRING, super::engine::layout::string::DATA) as i32); i32_add; });
        self.emit_expr(pos_expr);
        wasm!(self.func, { i32_wrap_i64; i32_add; });

        match op {
            ByteReadOp::U8 => {
                wasm!(self.func, { i32_load8_u(0); i64_extend_i32_u; });
            }
            ByteReadOp::I32Le => {
                wasm!(self.func, { i32_load(0); i64_extend_i32_s; });
            }
            ByteReadOp::U32Le => {
                wasm!(self.func, { i32_load(0); i64_extend_i32_u; });
            }
            ByteReadOp::U16Le => {
                wasm!(self.func, { i32_load16_u(0); i64_extend_i32_u; });
            }
            ByteReadOp::I16Le => {
                wasm!(self.func, { i32_load16_s(0); i64_extend_i32_s; });
            }
            ByteReadOp::I64Le => {
                wasm!(self.func, { i64_load(0); });
            }
            ByteReadOp::F32Le => {
                wasm!(self.func, { f32_load(0); f64_promote_f32; });
            }
            ByteReadOp::F64Le => {
                wasm!(self.func, { f64_load(0); });
            }
            ByteReadOp::F16Le => {
                // F16 → F32 via runtime (no native WASM instruction).
                // Reserve a dedicated runtime helper.
                wasm!(self.func, { i32_load16_u(0); call(self.emitter.rt.bytes_f16_to_f64); });
            }
        }
    }

    /// Emit `bytes.append_<int_type>(b, val)` for integer-shaped values.
    /// `size_bytes`: 1 (u8) / 2 (u16) / 4 (u32, i32) / 8 (i64).
    /// Args: `b: Bytes`, `val: Int`. Returns Unit.
    pub(super) fn emit_bytes_append_i(&mut self, args: &[IrExpr], size_bytes: u32) {
        let buf = self.scratch.alloc_i32();
        let old_len = self.scratch.alloc_i32();
        let new_buf = self.scratch.alloc_i32();
        let val_i64 = self.scratch.alloc_i64();
        self.emit_expr(&args[0]);
        wasm!(self.func, { local_set(buf); });
        self.emit_expr(&args[1]);
        wasm!(self.func, { local_set(val_i64); });
        // old_len = buf[0]
        wasm!(self.func, {
            local_get(buf); i32_load(0); local_set(old_len);
        });
        // new_buf = alloc(hdr + old_len + size_bytes)
        let str_hdr = self.emitter.layout_reg.header_size(super::engine::layout::STRING) as i32;
        let str_cap_off = self.emitter.layout_reg.fixed_offset(super::engine::layout::STRING, super::engine::layout::string::CAP);
        wasm!(self.func, {
            local_get(old_len); i32_const(str_hdr + size_bytes as i32); i32_add;
            call(self.emitter.rt.alloc); local_set(new_buf);
            // new_buf.len = old_len + size_bytes, new_buf.cap = same
            local_get(new_buf); local_get(old_len); i32_const(size_bytes as i32); i32_add; i32_store(0);
            local_get(new_buf); local_get(old_len); i32_const(size_bytes as i32); i32_add; i32_store(self.emitter.layout_reg.fixed_offset(super::engine::layout::STRING, super::engine::layout::string::CAP));
            local_get(new_buf); local_get(old_len); i32_const(size_bytes as i32); i32_add; i32_store(str_cap_off);
            // memcpy old data
            local_get(new_buf); i32_const(self.emitter.layout_reg.fixed_offset(super::engine::layout::STRING, super::engine::layout::string::DATA) as i32); i32_add;
            local_get(buf); i32_const(self.emitter.layout_reg.fixed_offset(super::engine::layout::STRING, super::engine::layout::string::DATA) as i32); i32_add;
            local_get(old_len);
            memory_copy;
            // address = new_buf + 4 + old_len
            local_get(new_buf); i32_const(self.emitter.layout_reg.fixed_offset(super::engine::layout::STRING, super::engine::layout::string::DATA) as i32); i32_add; local_get(old_len); i32_add;
        });
        // Store with width-specific opcode. Almide Int is i64; narrow first.
        match size_bytes {
            1 => { wasm!(self.func, { local_get(val_i64); i32_wrap_i64; i32_store8(0); }); }
            2 => { wasm!(self.func, { local_get(val_i64); i32_wrap_i64; i32_store16(0); }); }
            4 => { wasm!(self.func, { local_get(val_i64); i32_wrap_i64; i32_store(0); }); }
            8 => { wasm!(self.func, { local_get(val_i64); i64_store(0); }); }
            _ => panic!("emit_bytes_append_i: unsupported size_bytes {size_bytes}"),
        }
        // Update the variable in-place when arg[0] is a Var.
        self.emit_mutator_writeback(&args[0], new_buf);
        self.scratch.free_i64(val_i64);
        self.scratch.free_i32(new_buf);
        self.scratch.free_i32(old_len);
        self.scratch.free_i32(buf);
    }

    /// Emit `bytes.append_<float_type>(b, val)`.
    /// `size_bytes`: 4 (f32, requires demote) or 8 (f64).
    pub(super) fn emit_bytes_append_f(&mut self, args: &[IrExpr], size_bytes: u32, as_f32: bool) {
        let buf = self.scratch.alloc_i32();
        let old_len = self.scratch.alloc_i32();
        let new_buf = self.scratch.alloc_i32();
        let fval = self.scratch.alloc_f64();
        self.emit_expr(&args[0]);
        wasm!(self.func, { local_set(buf); });
        self.emit_expr(&args[1]);
        wasm!(self.func, { local_set(fval); });
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
            local_get(new_buf); i32_const(self.emitter.layout_reg.fixed_offset(super::engine::layout::STRING, super::engine::layout::string::DATA) as i32); i32_add; local_get(old_len); i32_add;
        });
        if as_f32 {
            wasm!(self.func, { local_get(fval); f32_demote_f64; f32_store(0); });
        } else {
            wasm!(self.func, { local_get(fval); f64_store(0); });
        }
        let _ = as_f32; // satisfy unused-var lint when both branches identical
        self.emit_mutator_writeback(&args[0], new_buf);
        self.scratch.free_f64(fval);
        self.scratch.free_i32(new_buf);
        self.scratch.free_i32(old_len);
        self.scratch.free_i32(buf);
    }

    /// Emit `bytes.read_<int_type>_be(b, pos)` — single-value big-endian integer read.
    /// Pushes an i64 onto the WASM stack (the Almide `Int`).
    pub(super) fn emit_byte_read_be_int(&mut self, buf_expr: &IrExpr, pos_expr: &IrExpr, size_bytes: u32, signed: bool) {
        let buf = self.scratch.alloc_i32();
        let src = self.scratch.alloc_i32();
        let acc = self.scratch.alloc_i64();
        self.emit_expr(buf_expr);
        wasm!(self.func, { local_set(buf); });
        self.emit_expr(pos_expr);
        wasm!(self.func, {
            i32_wrap_i64;
            local_get(buf); i32_const(self.emitter.layout_reg.fixed_offset(super::engine::layout::STRING, super::engine::layout::string::DATA) as i32); i32_add; i32_add; local_set(src);
            i64_const(0); local_set(acc);
        });
        for i in 0..size_bytes {
            let shift = 8 * (size_bytes - 1 - i) as i64;
            wasm!(self.func, {
                local_get(acc);
                local_get(src);
                i32_load8_u(i as u64);
                i64_extend_i32_u;
                i64_const(shift); i64_shl;
                i64_or;
                local_set(acc);
            });
        }
        if signed && size_bytes < 8 {
            // Sign-extend a sub-64-bit value to i64. Shift left then arithmetic right.
            let pad = 64 - 8 * size_bytes as i64;
            wasm!(self.func, {
                local_get(acc); i64_const(pad); i64_shl;
                i64_const(pad); i64_shr_s;
            });
        } else {
            wasm!(self.func, { local_get(acc); });
        }
        self.scratch.free_i64(acc);
        self.scratch.free_i32(src);
        self.scratch.free_i32(buf);
    }

    /// Emit `bytes.read_<float_type>_be(b, pos)` — single-value BE float read.
    pub(super) fn emit_byte_read_be_float(&mut self, buf_expr: &IrExpr, pos_expr: &IrExpr, size_bytes: u32) {
        // Reuse the int reader to get the bit pattern, then reinterpret.
        self.emit_byte_read_be_int(buf_expr, pos_expr, size_bytes, /*signed=*/false);
        if size_bytes == 4 {
            wasm!(self.func, { i32_wrap_i64; f32_reinterpret_i32; f64_promote_f32; });
        } else {
            wasm!(self.func, { f64_reinterpret_i64; });
        }
    }

    /// Emit `bytes.set_<int_type>_le(b, pos, val)` — overwrite an integer in place.
    /// Args: `b: Bytes`, `pos: Int`, `val: Int`. Returns Unit.
    pub(super) fn emit_bytes_set_i(&mut self, args: &[IrExpr], size_bytes: u32) {
        let buf = self.scratch.alloc_i32();
        let pos = self.scratch.alloc_i32();
        let val_i64 = self.scratch.alloc_i64();
        self.emit_expr(&args[0]);
        wasm!(self.func, { local_set(buf); });
        self.emit_expr(&args[1]);
        wasm!(self.func, { i32_wrap_i64; local_set(pos); });
        self.emit_expr(&args[2]);
        wasm!(self.func, {
            local_set(val_i64);
            // address = buf + 4 + pos
            local_get(buf); i32_const(self.emitter.layout_reg.fixed_offset(super::engine::layout::STRING, super::engine::layout::string::DATA) as i32); i32_add; local_get(pos); i32_add;
        });
        match size_bytes {
            1 => { wasm!(self.func, { local_get(val_i64); i32_wrap_i64; i32_store8(0); }); }
            2 => { wasm!(self.func, { local_get(val_i64); i32_wrap_i64; i32_store16(0); }); }
            4 => { wasm!(self.func, { local_get(val_i64); i32_wrap_i64; i32_store(0); }); }
            8 => { wasm!(self.func, { local_get(val_i64); i64_store(0); }); }
            _ => panic!("emit_bytes_set_i: unsupported size_bytes {size_bytes}"),
        }
        self.scratch.free_i64(val_i64);
        self.scratch.free_i32(pos);
        self.scratch.free_i32(buf);
    }

    /// Emit `bytes.set_<float_type>_le(b, pos, val)`.
    pub(super) fn emit_bytes_set_f(&mut self, args: &[IrExpr], size_bytes: u32, as_f32: bool) {
        let buf = self.scratch.alloc_i32();
        let pos = self.scratch.alloc_i32();
        let fval = self.scratch.alloc_f64();
        self.emit_expr(&args[0]);
        wasm!(self.func, { local_set(buf); });
        self.emit_expr(&args[1]);
        wasm!(self.func, { i32_wrap_i64; local_set(pos); });
        self.emit_expr(&args[2]);
        wasm!(self.func, {
            local_set(fval);
            local_get(buf); i32_const(self.emitter.layout_reg.fixed_offset(super::engine::layout::STRING, super::engine::layout::string::DATA) as i32); i32_add; local_get(pos); i32_add;
        });
        if as_f32 {
            wasm!(self.func, { local_get(fval); f32_demote_f64; f32_store(0); });
        } else {
            wasm!(self.func, { local_get(fval); f64_store(0); });
        }
        let _ = size_bytes; // fixed by `as_f32` (4 vs 8); kept for parity with append helper
        self.scratch.free_f64(fval);
        self.scratch.free_i32(pos);
        self.scratch.free_i32(buf);
    }

    /// `bytes.set_<int>_be(b, pos, val)` — overwrite at position with BE bytes.
    pub(super) fn emit_bytes_set_i_be(&mut self, args: &[IrExpr], size_bytes: u32) {
        let buf = self.scratch.alloc_i32();
        let pos = self.scratch.alloc_i32();
        let val_i64 = self.scratch.alloc_i64();
        let dst = self.scratch.alloc_i32();
        self.emit_expr(&args[0]); wasm!(self.func, { local_set(buf); });
        self.emit_expr(&args[1]); wasm!(self.func, { i32_wrap_i64; local_set(pos); });
        self.emit_expr(&args[2]); wasm!(self.func, {
            local_set(val_i64);
            local_get(buf); i32_const(self.emitter.layout_reg.fixed_offset(super::engine::layout::STRING, super::engine::layout::string::DATA) as i32); i32_add; local_get(pos); i32_add; local_set(dst);
        });
        for i in 0..size_bytes {
            let shift = 8 * (size_bytes - 1 - i) as i64;
            wasm!(self.func, {
                local_get(dst);
                local_get(val_i64); i64_const(shift); i64_shr_u;
                i32_wrap_i64;
                i32_const(0xFF); i32_and;
                i32_store8(i as u64);
            });
        }
        self.scratch.free_i32(dst);
        self.scratch.free_i64(val_i64);
        self.scratch.free_i32(pos);
        self.scratch.free_i32(buf);
    }

    /// `bytes.set_<float>_be(b, pos, val)` — overwrite at position with BE bytes.
    pub(super) fn emit_bytes_set_f_be(&mut self, args: &[IrExpr], size_bytes: u32) {
        let buf = self.scratch.alloc_i32();
        let pos = self.scratch.alloc_i32();
        let bits = self.scratch.alloc_i64();
        let dst = self.scratch.alloc_i32();
        self.emit_expr(&args[0]); wasm!(self.func, { local_set(buf); });
        self.emit_expr(&args[1]); wasm!(self.func, { i32_wrap_i64; local_set(pos); });
        self.emit_expr(&args[2]);
        if size_bytes == 4 {
            wasm!(self.func, {
                f32_demote_f64; i32_reinterpret_f32; i64_extend_i32_u; local_set(bits);
            });
        } else {
            wasm!(self.func, { i64_reinterpret_f64; local_set(bits); });
        }
        wasm!(self.func, {
            local_get(buf); i32_const(self.emitter.layout_reg.fixed_offset(super::engine::layout::STRING, super::engine::layout::string::DATA) as i32); i32_add; local_get(pos); i32_add; local_set(dst);
        });
        for i in 0..size_bytes {
            let shift = 8 * (size_bytes - 1 - i) as i64;
            wasm!(self.func, {
                local_get(dst);
                local_get(bits); i64_const(shift); i64_shr_u;
                i32_wrap_i64;
                i32_const(0xFF); i32_and;
                i32_store8(i as u64);
            });
        }
        self.scratch.free_i32(dst);
        self.scratch.free_i64(bits);
        self.scratch.free_i32(pos);
        self.scratch.free_i32(buf);
    }

    /// Emit `bytes.append_<int_type>_be(b, val)`.
    /// WASM has no native big-endian store, so we write byte-by-byte from MSB to LSB.
    pub(super) fn emit_bytes_append_i_be(&mut self, args: &[IrExpr], size_bytes: u32) {
        let buf = self.scratch.alloc_i32();
        let old_len = self.scratch.alloc_i32();
        let new_buf = self.scratch.alloc_i32();
        let val_i64 = self.scratch.alloc_i64();
        let dst = self.scratch.alloc_i32();
        self.emit_expr(&args[0]);
        wasm!(self.func, { local_set(buf); });
        self.emit_expr(&args[1]);
        wasm!(self.func, {
            local_set(val_i64);
            local_get(buf); i32_load(0); local_set(old_len);
            local_get(old_len); i32_const(self.emitter.layout_reg.header_size(super::engine::layout::STRING) as i32 + size_bytes as i32); i32_add;
            call(self.emitter.rt.alloc); local_set(new_buf);
            local_get(new_buf); local_get(old_len); i32_const(size_bytes as i32); i32_add; i32_store(0);
            local_get(new_buf); local_get(old_len); i32_const(size_bytes as i32); i32_add; i32_store(self.emitter.layout_reg.fixed_offset(super::engine::layout::STRING, super::engine::layout::string::CAP));
            local_get(new_buf); i32_const(self.emitter.layout_reg.fixed_offset(super::engine::layout::STRING, super::engine::layout::string::DATA) as i32); i32_add;
            local_get(buf); i32_const(self.emitter.layout_reg.fixed_offset(super::engine::layout::STRING, super::engine::layout::string::DATA) as i32); i32_add;
            local_get(old_len);
            memory_copy;
            local_get(new_buf); i32_const(self.emitter.layout_reg.fixed_offset(super::engine::layout::STRING, super::engine::layout::string::DATA) as i32); i32_add; local_get(old_len); i32_add;
            local_set(dst);
        });
        // Write MSB-first: byte at offset i = (val >> (8*(size-1-i))) & 0xff
        for i in 0..size_bytes {
            let shift = 8 * (size_bytes - 1 - i) as i64;
            wasm!(self.func, {
                local_get(dst);
                local_get(val_i64); i64_const(shift); i64_shr_u;
                i32_wrap_i64;
                i32_const(0xFF); i32_and;
                i32_store8(i as u64);
            });
        }
        self.emit_mutator_writeback(&args[0], new_buf);
        self.scratch.free_i32(dst);
        self.scratch.free_i64(val_i64);
        self.scratch.free_i32(new_buf);
        self.scratch.free_i32(old_len);
        self.scratch.free_i32(buf);
    }

    /// Emit `bytes.append_<float_type>_be(b, val)` — reinterpret as int bits, then BE store.
    pub(super) fn emit_bytes_append_f_be(&mut self, args: &[IrExpr], size_bytes: u32) {
        let buf = self.scratch.alloc_i32();
        let old_len = self.scratch.alloc_i32();
        let new_buf = self.scratch.alloc_i32();
        let bits = self.scratch.alloc_i64();
        let dst = self.scratch.alloc_i32();
        self.emit_expr(&args[0]);
        wasm!(self.func, { local_set(buf); });
        self.emit_expr(&args[1]); // f64 on stack
        if size_bytes == 4 {
            // Demote to f32, reinterpret as i32 bits, extend to i64 for shifting.
            wasm!(self.func, {
                f32_demote_f64;
                i32_reinterpret_f32;
                i64_extend_i32_u;
                local_set(bits);
            });
        } else {
            wasm!(self.func, {
                i64_reinterpret_f64;
                local_set(bits);
            });
        }
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
            local_get(new_buf); i32_const(self.emitter.layout_reg.fixed_offset(super::engine::layout::STRING, super::engine::layout::string::DATA) as i32); i32_add; local_get(old_len); i32_add;
            local_set(dst);
        });
        for i in 0..size_bytes {
            let shift = 8 * (size_bytes - 1 - i) as i64;
            wasm!(self.func, {
                local_get(dst);
                local_get(bits); i64_const(shift); i64_shr_u;
                i32_wrap_i64;
                i32_const(0xFF); i32_and;
                i32_store8(i as u64);
            });
        }
        self.emit_mutator_writeback(&args[0], new_buf);
        self.scratch.free_i32(dst);
        self.scratch.free_i64(bits);
        self.scratch.free_i32(new_buf);
        self.scratch.free_i32(old_len);
        self.scratch.free_i32(buf);
    }

    // ── Cursor family helpers ──
    //
    // Tuple `(Int, Option[T])` layout: 12 bytes = `[i64 pos][i32 option_ptr]`.
    // Option payload is alloc'd as a separate cell:
    //   - Option[Int]   → 8-byte cell containing i64
    //   - Option[Float] → 8-byte cell containing f64
    //   - Option[Bytes] → cell pointer is the Bytes pointer itself (no extra alloc)
    // `0` represents `none`.

    /// Allocate a `(Int, Option[T])` tuple cell, populate with `(new_pos, opt_ptr)`,
    /// and leave the tuple pointer on the WASM stack. Caller has already pushed
    /// nothing; this method consumes the two scratch locals.
    fn emit_cursor_pack_tuple(&mut self, new_pos_local: u32, opt_ptr_local: u32) {
        let tuple = self.scratch.alloc_i32();
        wasm!(self.func, {
            i32_const(12); call(self.emitter.rt.alloc); local_set(tuple);
            // tuple[0..8] = new_pos (i64)
            local_get(tuple); local_get(new_pos_local); i64_store(0);
            // tuple[8..12] = opt_ptr (i32)
            local_get(tuple); local_get(opt_ptr_local); i32_store(8);
            local_get(tuple);
        });
        self.scratch.free_i32(tuple);
    }

    pub(super) fn emit_bytes_skip(&mut self, args: &[IrExpr]) {
        let buf = self.scratch.alloc_i32();
        let pos = self.scratch.alloc_i64();
        let n = self.scratch.alloc_i64();
        let len = self.scratch.alloc_i64();
        let np = self.scratch.alloc_i64();
        self.emit_expr(&args[0]); wasm!(self.func, { local_set(buf); });
        self.emit_expr(&args[1]); wasm!(self.func, { local_set(pos); });
        self.emit_expr(&args[2]); wasm!(self.func, {
            local_set(n);
            local_get(buf); i32_load(0); i64_extend_i32_u; local_set(len);
            local_get(pos); local_get(n); i64_add; local_set(np);
            // result = if np > len then len else np
            local_get(np); local_get(len); i64_gt_s;
            if_i64;
              local_get(len);
            else_;
              local_get(np);
            end;
        });
        self.scratch.free_i64(np);
        self.scratch.free_i64(len);
        self.scratch.free_i64(n);
        self.scratch.free_i64(pos);
        self.scratch.free_i32(buf);
    }

    pub(super) fn emit_bytes_eof(&mut self, args: &[IrExpr]) {
        let buf = self.scratch.alloc_i32();
        let pos = self.scratch.alloc_i32();
        self.emit_expr(&args[0]); wasm!(self.func, { local_set(buf); });
        self.emit_expr(&args[1]); wasm!(self.func, {
            i32_wrap_i64; local_set(pos);
            local_get(pos); local_get(buf); i32_load(0); i32_ge_u;
        });
        self.scratch.free_i32(pos);
        self.scratch.free_i32(buf);
    }

    /// `bytes.map_each(b, f) -> Bytes` — apply Int→Int closure to every byte.
    /// Closure layout: `[table_idx:i32][env_ptr:i32]`. Calling convention is
    /// `(env, arg) -> ret` resolved via `call_indirect`. The byte value is
    /// widened to i64 going in and truncated coming out.
    pub(super) fn emit_bytes_map_each(&mut self, args: &[IrExpr]) {
        let buf = self.scratch.alloc_i32();
        let closure = self.scratch.alloc_i32();
        let len = self.scratch.alloc_i32();
        let dst = self.scratch.alloc_i32();
        let i = self.scratch.alloc_i32();
        self.emit_expr(&args[0]); wasm!(self.func, { local_set(buf); });
        self.emit_expr(&args[1]); wasm!(self.func, {
            local_set(closure);
            local_get(buf); i32_load(0); local_set(len);
            local_get(len); call(self.emitter.rt.string_alloc); local_set(dst);
            local_get(dst); local_get(len); i32_store(0);
            i32_const(0); local_set(i);
            block_empty; loop_empty;
                local_get(i); local_get(len); i32_ge_u; br_if(1);
                // dst[i] = (i32) f((i64) b[i])
                local_get(dst); i32_const(self.emitter.layout_reg.fixed_offset(super::engine::layout::STRING, super::engine::layout::string::DATA) as i32); i32_add; local_get(i); i32_add;
                // closure call args: env, arg, table_idx
                local_get(closure); i32_load(4);
                local_get(buf); i32_const(self.emitter.layout_reg.fixed_offset(super::engine::layout::STRING, super::engine::layout::string::DATA) as i32); i32_add; local_get(i); i32_add;
                i32_load8_u(0); i64_extend_i32_u;
                local_get(closure); i32_load(0);
        });
        self.emit_closure_call(&almide_lang::types::Ty::Int, &almide_lang::types::Ty::Int);
        wasm!(self.func, {
                i32_wrap_i64; i32_store8(0);
                local_get(i); i32_const(1); i32_add; local_set(i);
                br(0);
            end; end;
            local_get(dst);
        });
        self.scratch.free_i32(i);
        self.scratch.free_i32(dst);
        self.scratch.free_i32(len);
        self.scratch.free_i32(closure);
        self.scratch.free_i32(buf);
    }

    /// `bytes.xor(a, b) -> Bytes`. Result length = `min(len(a), len(b))`.
    pub(super) fn emit_bytes_xor(&mut self, args: &[IrExpr]) {
        let a = self.scratch.alloc_i32();
        let b = self.scratch.alloc_i32();
        let alen = self.scratch.alloc_i32();
        let blen = self.scratch.alloc_i32();
        let n = self.scratch.alloc_i32();
        let dst = self.scratch.alloc_i32();
        let i = self.scratch.alloc_i32();
        self.emit_expr(&args[0]); wasm!(self.func, { local_set(a); });
        self.emit_expr(&args[1]); wasm!(self.func, {
            local_set(b);
            local_get(a); i32_load(0); local_set(alen);
            local_get(b); i32_load(0); local_set(blen);
            local_get(alen); local_get(blen); i32_lt_u;
            if_i32; local_get(alen); else_; local_get(blen); end;
            local_set(n);
            local_get(n); call(self.emitter.rt.string_alloc); local_set(dst);
            local_get(dst); local_get(n); i32_store(0);
            i32_const(0); local_set(i);
            block_empty; loop_empty;
                local_get(i); local_get(n); i32_ge_u; br_if(1);
                local_get(dst); i32_const(self.emitter.layout_reg.fixed_offset(super::engine::layout::STRING, super::engine::layout::string::DATA) as i32); i32_add; local_get(i); i32_add;
                local_get(a); i32_const(self.emitter.layout_reg.fixed_offset(super::engine::layout::STRING, super::engine::layout::string::DATA) as i32); i32_add; local_get(i); i32_add; i32_load8_u(0);
                local_get(b); i32_const(self.emitter.layout_reg.fixed_offset(super::engine::layout::STRING, super::engine::layout::string::DATA) as i32); i32_add; local_get(i); i32_add; i32_load8_u(0);
                i32_xor;
                i32_store8(0);
                local_get(i); i32_const(1); i32_add; local_set(i);
                br(0);
            end; end;
            local_get(dst);
        });
        self.scratch.free_i32(i);
        self.scratch.free_i32(dst);
        self.scratch.free_i32(n);
        self.scratch.free_i32(blen);
        self.scratch.free_i32(alen);
        self.scratch.free_i32(b);
        self.scratch.free_i32(a);
    }

    /// `bytes.pad_left` / `bytes.pad_right` — extend to target_len with val.
    pub(super) fn emit_bytes_pad(&mut self, args: &[IrExpr], left: bool) {
        let buf = self.scratch.alloc_i32();
        let target = self.scratch.alloc_i32();
        let val = self.scratch.alloc_i32();
        let blen = self.scratch.alloc_i32();
        let pad = self.scratch.alloc_i32();
        let dst = self.scratch.alloc_i32();
        let i = self.scratch.alloc_i32();
        self.emit_expr(&args[0]); wasm!(self.func, { local_set(buf); });
        self.emit_expr(&args[1]); wasm!(self.func, { i32_wrap_i64; local_set(target); });
        self.emit_expr(&args[2]); wasm!(self.func, {
            i32_wrap_i64; local_set(val);
            local_get(buf); i32_load(0); local_set(blen);
            // If blen >= target → clone unchanged
            local_get(blen); local_get(target); i32_ge_u;
            if_i32;
                local_get(blen); call(self.emitter.rt.string_alloc); local_set(dst);
                local_get(dst); local_get(buf); local_get(blen); i32_const(self.emitter.layout_reg.fixed_offset(super::engine::layout::STRING, super::engine::layout::string::DATA) as i32); i32_add; memory_copy;
                local_get(dst);
            else_;
                local_get(target); local_get(blen); i32_sub; local_set(pad);
                local_get(target); call(self.emitter.rt.string_alloc); local_set(dst);
                local_get(dst); local_get(target); i32_store(0);
        });
        if left {
            wasm!(self.func, {
                // Fill [0, pad) with val
                i32_const(0); local_set(i);
                block_empty; loop_empty;
                    local_get(i); local_get(pad); i32_ge_u; br_if(1);
                    local_get(dst); i32_const(self.emitter.layout_reg.fixed_offset(super::engine::layout::STRING, super::engine::layout::string::DATA) as i32); i32_add; local_get(i); i32_add;
                    local_get(val); i32_store8(0);
                    local_get(i); i32_const(1); i32_add; local_set(i);
                    br(0);
                end; end;
                // Copy original into [pad..target)
                local_get(dst); i32_const(self.emitter.layout_reg.fixed_offset(super::engine::layout::STRING, super::engine::layout::string::DATA) as i32); i32_add; local_get(pad); i32_add;
                local_get(buf); i32_const(self.emitter.layout_reg.fixed_offset(super::engine::layout::STRING, super::engine::layout::string::DATA) as i32); i32_add;
                local_get(blen);
                memory_copy;
            });
        } else {
            wasm!(self.func, {
                // Copy original into [0..blen)
                local_get(dst); i32_const(self.emitter.layout_reg.fixed_offset(super::engine::layout::STRING, super::engine::layout::string::DATA) as i32); i32_add;
                local_get(buf); i32_const(self.emitter.layout_reg.fixed_offset(super::engine::layout::STRING, super::engine::layout::string::DATA) as i32); i32_add;
                local_get(blen);
                memory_copy;
                // Fill [blen..target) with val
                i32_const(0); local_set(i);
                block_empty; loop_empty;
                    local_get(i); local_get(pad); i32_ge_u; br_if(1);
                    local_get(dst); i32_const(self.emitter.layout_reg.fixed_offset(super::engine::layout::STRING, super::engine::layout::string::DATA) as i32); i32_add; local_get(blen); i32_add; local_get(i); i32_add;
                    local_get(val); i32_store8(0);
                    local_get(i); i32_const(1); i32_add; local_set(i);
                    br(0);
                end; end;
            });
        }
        wasm!(self.func, {
                local_get(dst);
            end;
        });
        self.scratch.free_i32(i);
        self.scratch.free_i32(dst);
        self.scratch.free_i32(pad);
        self.scratch.free_i32(blen);
        self.scratch.free_i32(val);
        self.scratch.free_i32(target);
        self.scratch.free_i32(buf);
    }

    /// `bytes.copy_from(dst, src, dst_off, src_off, len)` — in-place memcpy.
    pub(super) fn emit_bytes_copy_from(&mut self, args: &[IrExpr]) {
        let dst = self.scratch.alloc_i32();
        let src = self.scratch.alloc_i32();
        let dst_off = self.scratch.alloc_i32();
        let src_off = self.scratch.alloc_i32();
        let len = self.scratch.alloc_i32();
        let dst_len = self.scratch.alloc_i32();
        let src_len = self.scratch.alloc_i32();
        let avail_dst = self.scratch.alloc_i32();
        let avail_src = self.scratch.alloc_i32();
        self.emit_expr(&args[0]); wasm!(self.func, { local_set(dst); });
        self.emit_expr(&args[1]); wasm!(self.func, { local_set(src); });
        self.emit_expr(&args[2]); wasm!(self.func, { i32_wrap_i64; local_set(dst_off); });
        self.emit_expr(&args[3]); wasm!(self.func, { i32_wrap_i64; local_set(src_off); });
        self.emit_expr(&args[4]); wasm!(self.func, {
            i32_wrap_i64; local_set(len);
            local_get(dst); i32_load(0); local_set(dst_len);
            local_get(src); i32_load(0); local_set(src_len);
            // If either offset out of range → no-op
            local_get(dst_off); local_get(dst_len); i32_ge_u;
            local_get(src_off); local_get(src_len); i32_ge_u; i32_or;
            if_empty;
                // skip
            else_;
                // Clamp len to min(len, dst_len - dst_off, src_len - src_off)
                local_get(dst_len); local_get(dst_off); i32_sub; local_set(avail_dst);
                local_get(src_len); local_get(src_off); i32_sub; local_set(avail_src);
                local_get(len); local_get(avail_dst); i32_lt_u;
                if_i32; local_get(len); else_; local_get(avail_dst); end;
                local_set(len);
                local_get(len); local_get(avail_src); i32_lt_u;
                if_i32; local_get(len); else_; local_get(avail_src); end;
                local_set(len);
                // memcpy: dst+4+dst_off ← src+4+src_off, len bytes
                local_get(dst); i32_const(self.emitter.layout_reg.fixed_offset(super::engine::layout::STRING, super::engine::layout::string::DATA) as i32); i32_add; local_get(dst_off); i32_add;
                local_get(src); i32_const(self.emitter.layout_reg.fixed_offset(super::engine::layout::STRING, super::engine::layout::string::DATA) as i32); i32_add; local_get(src_off); i32_add;
                local_get(len);
                memory_copy;
            end;
        });
        self.scratch.free_i32(avail_src);
        self.scratch.free_i32(avail_dst);
        self.scratch.free_i32(src_len);
        self.scratch.free_i32(dst_len);
        self.scratch.free_i32(len);
        self.scratch.free_i32(src_off);
        self.scratch.free_i32(dst_off);
        self.scratch.free_i32(src);
        self.scratch.free_i32(dst);
    }

}
