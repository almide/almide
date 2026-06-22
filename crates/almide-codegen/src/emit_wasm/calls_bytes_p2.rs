impl FuncCompiler<'_> {
    /// Group 2 of the `emit_bytes_call` dispatch (see `calls_bytes.rs`).
    /// Disjoint from groups 1/3; returns `true` if a method matched.
    pub(super) fn emit_bytes_call_g2(&mut self, method: &str, args: &[IrExpr]) -> bool {
        match method {
            "data_ptr" => {
                // bytes.data_ptr(b) → Int (i64)
                // Return pointer to data region: buf + 4
                self.emit_expr(&args[0]);
                wasm!(self.func, { i32_const(self.emitter.layout_reg.fixed_offset(super::engine::layout::STRING, super::engine::layout::string::DATA) as i32); i32_add; i64_extend_i32_u; });
            }
            // ── RawPtr / linear-memory bridge (#440) ──
            // A RawPtr is an i32 linear-memory byte offset. `as_ptr`/`as_mut_ptr`
            // return the offset of the data region (`b + DATA`); the copying ops
            // move bytes between that region and a raw offset via `memory.copy`.
            "as_ptr" | "as_mut_ptr" => {
                // bytes.as_ptr(b) / as_mut_ptr(b) → RawPtr (i32 data offset)
                self.emit_expr(&args[0]);
                wasm!(self.func, {
                    i32_const(self.emitter.layout_reg.fixed_offset(super::engine::layout::STRING, super::engine::layout::string::DATA) as i32);
                    i32_add;
                });
            }
            "copy_to_ptr" => {
                // bytes.copy_to_ptr(b, ptr, cap) → Int (bytes copied)
                // n = clamp(cap, 0, len(b)); memory.copy(ptr, b+DATA, n); return n.
                let src = self.scratch.alloc_i32();
                let dst = self.scratch.alloc_i32();
                let n = self.scratch.alloc_i32();
                self.emit_expr(&args[0]); wasm!(self.func, { local_set(src); });            // src bytes ptr
                self.emit_expr(&args[1]); wasm!(self.func, { local_set(dst); });            // dst raw offset (i32 RawPtr)
                self.emit_expr(&args[2]); wasm!(self.func, { i32_wrap_i64; local_set(n); }); // cap (Int i64 → i32)
                wasm!(self.func, {
                    local_get(n); i32_const(0); i32_lt_s;
                    if_empty; i32_const(0); local_set(n); end;
                    local_get(n); local_get(src); i32_load(0); i32_gt_u;
                    if_empty; local_get(src); i32_load(0); local_set(n); end;
                    local_get(dst);
                    local_get(src); i32_const(self.emitter.layout_reg.fixed_offset(super::engine::layout::STRING, super::engine::layout::string::DATA) as i32); i32_add;
                    local_get(n);
                    memory_copy;
                    local_get(n); i64_extend_i32_u;
                });
                self.scratch.free_i32(n);
                self.scratch.free_i32(dst);
                self.scratch.free_i32(src);
            }
            "from_raw_ptr" => {
                // bytes.from_raw_ptr(ptr, len) → Bytes (copying: alloc + memory.copy)
                let src = self.scratch.alloc_i32();
                let len = self.scratch.alloc_i32();
                let dst = self.scratch.alloc_i32();
                self.emit_expr(&args[0]); wasm!(self.func, { local_set(src); });              // ptr (i32 RawPtr)
                self.emit_expr(&args[1]); wasm!(self.func, { i32_wrap_i64; local_set(len); }); // len (Int i64 → i32)
                wasm!(self.func, {
                    local_get(len); i32_const(0); i32_lt_s;
                    if_empty; i32_const(0); local_set(len); end;
                    local_get(len); i32_const(self.emitter.layout_reg.header_size(super::engine::layout::STRING) as i32); i32_add;
                    call(self.emitter.rt.alloc);
                    local_set(dst);
                    local_get(dst); local_get(len); i32_store(0);
                    local_get(dst); i32_const(self.emitter.layout_reg.fixed_offset(super::engine::layout::STRING, super::engine::layout::string::DATA) as i32); i32_add;
                    local_get(src);
                    local_get(len);
                    memory_copy;
                    local_get(dst);
                });
                self.scratch.free_i32(dst);
                self.scratch.free_i32(len);
                self.scratch.free_i32(src);
            }
            // ── Little-endian reads (native WASM loads) ──
            "read_u8" => {
                self.emit_typed_byte_read(&args[0], &args[1], ByteReadOp::U8);
            }
            "read_i32_le" => {
                self.emit_typed_byte_read(&args[0], &args[1], ByteReadOp::I32Le);
            }
            "read_u32_le" => {
                self.emit_typed_byte_read(&args[0], &args[1], ByteReadOp::U32Le);
            }
            "read_u16_le" => {
                self.emit_typed_byte_read(&args[0], &args[1], ByteReadOp::U16Le);
            }
            "read_i64_le" => {
                self.emit_typed_byte_read(&args[0], &args[1], ByteReadOp::I64Le);
            }
            "read_f32_le" => {
                self.emit_typed_byte_read(&args[0], &args[1], ByteReadOp::F32Le);
            }
            "read_f64_le" => {
                self.emit_typed_byte_read(&args[0], &args[1], ByteReadOp::F64Le);
            }
            "read_f16_le" => {
                self.emit_typed_byte_read(&args[0], &args[1], ByteReadOp::F16Le);
            }
            "skip" => self.emit_bytes_skip(args),
            "eof" => self.emit_bytes_eof(args),
            "read_u8_at" => self.emit_cursor_read_int(args, 1, /*signed=*/false, /*be=*/false),
            "read_u16_le_at" => self.emit_cursor_read_int(args, 2, false, false),
            "read_u16_be_at" => self.emit_cursor_read_int(args, 2, false, true),
            "read_i16_le_at" => self.emit_cursor_read_int(args, 2, true, false),
            "read_i16_be_at" => self.emit_cursor_read_int(args, 2, true, true),
            "read_u32_le_at" => self.emit_cursor_read_int(args, 4, false, false),
            "read_i32_le_at" => self.emit_cursor_read_int(args, 4, true, false),
            "read_i64_le_at" => self.emit_cursor_read_int(args, 8, true, false),
            "read_u32_be_at" => self.emit_cursor_read_int(args, 4, false, true),
            "read_i32_be_at" => self.emit_cursor_read_int(args, 4, true, true),
            "read_i64_be_at" => self.emit_cursor_read_int(args, 8, true, true),
            "read_f16_le_at" => self.emit_cursor_read_f16_le(args),
            "read_f32_le_at" => self.emit_cursor_read_float(args, 4, false),
            "read_f64_le_at" => self.emit_cursor_read_float(args, 8, false),
            "read_f32_be_at" => self.emit_cursor_read_float(args, 4, true),
            "read_f64_be_at" => self.emit_cursor_read_float(args, 8, true),
            "read_bool_at" => self.emit_cursor_read_bool(args),
            "read_string_be_at" => self.emit_cursor_read_string_be(args),
            "take_at" => self.emit_cursor_take(args),
            "read_i16_le" => self.emit_typed_byte_read(&args[0], &args[1], ByteReadOp::I16Le),
            "read_u16_be" => self.emit_byte_read_be_int(&args[0], &args[1], 2, /*signed=*/false),
            "read_i16_be" => self.emit_byte_read_be_int(&args[0], &args[1], 2, true),
            "read_u32_be" => self.emit_byte_read_be_int(&args[0], &args[1], 4, /*signed=*/false),
            "read_i32_be" => self.emit_byte_read_be_int(&args[0], &args[1], 4, true),
            "read_i64_be" => self.emit_byte_read_be_int(&args[0], &args[1], 8, true),
            "read_f32_be" => self.emit_byte_read_be_float(&args[0], &args[1], 4),
            "read_f64_be" => self.emit_byte_read_be_float(&args[0], &args[1], 8),
            // ── Typed read/write/set with runtime Endian dispatch ──
            // Stage 4a/4b typed API: args are (b, offset/value, endian).
            // `endian` is a bare Endian variant (tag 0 = LittleEndian,
            // tag 1 = BigEndian), emitted as i32. The runtime branch
            // picks the matching `_le` / `_be` existing emitter.
            "read_uint16" => self.emit_bytes_read_typed_int(args, 2, /*signed=*/false),
            "read_uint32" => self.emit_bytes_read_typed_int(args, 4, false),
            "read_int32" => self.emit_bytes_read_typed_int(args, 4, true),
            "read_float32" => self.emit_bytes_read_typed_float(args, 4),
            "write_uint16" => self.emit_bytes_write_typed_int(args, 2),
            "write_uint32" => self.emit_bytes_write_typed_int(args, 4),
            "write_int32" => self.emit_bytes_write_typed_int(args, 4),
            "write_float32" => self.emit_bytes_write_typed_float(args, 4),
            "set_uint16" => self.emit_bytes_set_typed_int(args, 2),
            "set_uint32" => self.emit_bytes_set_typed_int(args, 4),
            "set_int32" => self.emit_bytes_set_typed_int(args, 4),
            "set_float32" => self.emit_bytes_set_typed_float(args, 4),
            _ => return false,
        }
        true
    }

    /// Group 3 of the `emit_bytes_call` dispatch (see `calls_bytes.rs`).
    /// Disjoint from groups 1/2; returns `true` if a method matched.
    pub(super) fn emit_bytes_call_g3(&mut self, method: &str, args: &[IrExpr]) -> bool {
        match method {
            "read_string_at" => {
                // bytes.read_string_at(b, pos, len) → String
                // Copy `len` bytes from [data + pos] into a newly allocated
                // String buffer `[len:i32][bytes]`.
                let buf = self.scratch.alloc_i32();
                let src = self.scratch.alloc_i32();
                let len = self.scratch.alloc_i32();
                let dst = self.scratch.alloc_i32();
                self.emit_expr(&args[0]);
                wasm!(self.func, { local_set(buf); });
                self.emit_expr(&args[1]);
                wasm!(self.func, {
                    i32_wrap_i64;
                    local_get(buf); i32_const(self.emitter.layout_reg.fixed_offset(super::engine::layout::STRING, super::engine::layout::string::DATA) as i32); i32_add; i32_add; local_set(src);
                });
                self.emit_expr(&args[2]);
                wasm!(self.func, {
                    i32_wrap_i64; local_set(len);
                    // alloc 4 + len
                    local_get(len); i32_const(self.emitter.layout_reg.header_size(super::engine::layout::STRING) as i32); i32_add;
                    call(self.emitter.rt.alloc); local_set(dst);
                    local_get(dst); local_get(len); i32_store(0);
                    local_get(dst); i32_const(self.emitter.layout_reg.fixed_offset(super::engine::layout::STRING, super::engine::layout::string::DATA) as i32); i32_add;
                    local_get(src);
                    local_get(len);
                    memory_copy;
                    local_get(dst);
                });
                self.scratch.free_i32(dst);
                self.scratch.free_i32(len);
                self.scratch.free_i32(src);
                self.scratch.free_i32(buf);
            }
            "skip_length_prefixed_le" => {
                // bytes.skip_length_prefixed_le(b, pos, count) → Int
                // Skip `count` entries of [u32 len][len bytes] starting at pos.
                let buf = self.scratch.alloc_i32();
                let pos = self.scratch.alloc_i32();
                let n = self.scratch.alloc_i32();
                let i = self.scratch.alloc_i32();
                let lval = self.scratch.alloc_i32();
                self.emit_expr(&args[0]);
                wasm!(self.func, { local_set(buf); });
                self.emit_expr(&args[1]);
                wasm!(self.func, { i32_wrap_i64; local_set(pos); });
                self.emit_expr(&args[2]);
                wasm!(self.func, {
                    i32_wrap_i64; local_set(n);
                    i32_const(0); local_set(i);
                    block_empty; loop_empty;
                      local_get(i); local_get(n); i32_ge_u; br_if(1);
                      // Load u32 len from buf + 4 + pos
                      local_get(buf); i32_const(self.emitter.layout_reg.fixed_offset(super::engine::layout::STRING, super::engine::layout::string::DATA) as i32); i32_add; local_get(pos); i32_add;
                      i32_load(0); local_set(lval);
                      // pos += 4 + len
                      local_get(pos); i32_const(self.emitter.layout_reg.fixed_offset(super::engine::layout::STRING, super::engine::layout::string::DATA) as i32); i32_add; local_get(lval); i32_add;
                      local_set(pos);
                      local_get(i); i32_const(1); i32_add; local_set(i);
                      br(0);
                    end; end;
                    local_get(pos); i64_extend_i32_u;
                });
                self.scratch.free_i32(lval);
                self.scratch.free_i32(i);
                self.scratch.free_i32(n);
                self.scratch.free_i32(pos);
                self.scratch.free_i32(buf);
            }
            "read_length_prefixed_strings_le" => {
                // bytes.read_length_prefixed_strings_le(b, pos, count) → List[String]
                let buf = self.scratch.alloc_i32();
                let pos = self.scratch.alloc_i32();
                let n = self.scratch.alloc_i32();
                let result = self.scratch.alloc_i32();
                let i = self.scratch.alloc_i32();
                let lval = self.scratch.alloc_i32();
                let s = self.scratch.alloc_i32();
                self.emit_expr(&args[0]);
                wasm!(self.func, { local_set(buf); });
                self.emit_expr(&args[1]);
                wasm!(self.func, { i32_wrap_i64; local_set(pos); });
                self.emit_expr(&args[2]);
                wasm!(self.func, {
                    i32_wrap_i64; local_set(n);
                    // alloc list: 4 + n*4
                    local_get(n); i32_const(4); i32_mul; i32_const(self.emitter.layout_reg.header_size(super::engine::layout::STRING) as i32); i32_add;
                    call(self.emitter.rt.alloc); local_set(result);
                    local_get(result); local_get(n); i32_store(0);
                    i32_const(0); local_set(i);
                    block_empty; loop_empty;
                      local_get(i); local_get(n); i32_ge_u; br_if(1);
                      // len at [buf+4+pos]
                      local_get(buf); i32_const(self.emitter.layout_reg.fixed_offset(super::engine::layout::STRING, super::engine::layout::string::DATA) as i32); i32_add; local_get(pos); i32_add;
                      i32_load(0); local_set(lval);
                      // alloc string: [len][bytes]
                      local_get(lval); i32_const(self.emitter.layout_reg.fixed_offset(super::engine::layout::STRING, super::engine::layout::string::DATA) as i32); i32_add;
                      call(self.emitter.rt.alloc); local_set(s);
                      local_get(s); local_get(lval); i32_store(0);
                      // memcpy bytes: dst = s+4, src = buf+4+pos+4, n = lval
                      local_get(s); i32_const(self.emitter.layout_reg.fixed_offset(super::engine::layout::STRING, super::engine::layout::string::DATA) as i32); i32_add;
                      local_get(buf); i32_const(self.emitter.layout_reg.fixed_offset(super::engine::layout::STRING, super::engine::layout::string::DATA) as i32); i32_add;
                      local_get(pos); i32_add; i32_const(self.emitter.layout_reg.fixed_offset(super::engine::layout::STRING, super::engine::layout::string::DATA) as i32); i32_add;
                      local_get(lval);
                      memory_copy;
                      // result[i] = s
                      local_get(result); i32_const(self.emitter.layout_reg.fixed_offset(super::engine::layout::STRING, super::engine::layout::string::DATA) as i32); i32_add;
                      local_get(i); i32_const(4); i32_mul; i32_add;
                      local_get(s); i32_store(0);
                      // pos += 4 + len
                      local_get(pos); i32_const(self.emitter.layout_reg.fixed_offset(super::engine::layout::STRING, super::engine::layout::string::DATA) as i32); i32_add; local_get(lval); i32_add;
                      local_set(pos);
                      local_get(i); i32_const(1); i32_add; local_set(i);
                      br(0);
                    end; end;
                    local_get(result);
                });
                self.scratch.free_i32(s);
                self.scratch.free_i32(lval);
                self.scratch.free_i32(i);
                self.scratch.free_i32(result);
                self.scratch.free_i32(n);
                self.scratch.free_i32(pos);
                self.scratch.free_i32(buf);
            }
            "read_i16_le_array" | "read_u16_le_array"
            | "read_i16_be_array" | "read_u16_be_array"
            | "read_i32_le_array" | "read_u32_le_array" | "read_i64_le_array"
            | "read_f32_le_array" | "read_f64_le_array" | "read_f16_le_array"
            | "read_i32_be_array" | "read_u32_be_array" | "read_i64_be_array"
            | "read_f32_be_array" | "read_f64_be_array" => {
                // bytes.read_XX_<endian>_array(b, pos, count) → List[T]
                // Element width in source bytes; output cell is always 8 bytes
                // (Almide Int = i64, Float = f64).
                let is_be = method.contains("_be_");
                let buf = self.scratch.alloc_i32();
                let pos = self.scratch.alloc_i32();
                let n = self.scratch.alloc_i32();
                let result = self.scratch.alloc_i32();
                let i = self.scratch.alloc_i32();
                let elem_bytes: i32 = match method {
                    "read_f16_le_array"
                    | "read_i16_le_array" | "read_u16_le_array"
                    | "read_i16_be_array" | "read_u16_be_array" => 2,
                    "read_i64_le_array" | "read_f64_le_array" | "read_i64_be_array" | "read_f64_be_array" => 8,
                    _ => 4, // i32 / u32 / f32 (LE or BE)
                };
                let out_bytes: i32 = 8;  // list elem size (i64 or f64)
                self.emit_expr(&args[0]);
                wasm!(self.func, { local_set(buf); });
                self.emit_expr(&args[1]);
                wasm!(self.func, { i32_wrap_i64; local_set(pos); });
                self.emit_expr(&args[2]);
                wasm!(self.func, {
                    i32_wrap_i64; local_set(n);
                    // alloc list: 4 + n * out_bytes
                    local_get(n); i32_const(out_bytes); i32_mul; i32_const(self.emitter.layout_reg.header_size(super::engine::layout::STRING) as i32); i32_add;
                    call(self.emitter.rt.alloc); local_set(result);
                    local_get(result); local_get(n); i32_store(0);
                    i32_const(0); local_set(i);
                    block_empty; loop_empty;
                      local_get(i); local_get(n); i32_ge_u; br_if(1);
                      // dst = result + 4 + i * out_bytes
                      local_get(result); i32_const(self.emitter.layout_reg.fixed_offset(super::engine::layout::STRING, super::engine::layout::string::DATA) as i32); i32_add;
                      local_get(i); i32_const(out_bytes); i32_mul; i32_add;
                      // src addr = buf + 4 + pos + i * elem_bytes
                      local_get(buf); i32_const(self.emitter.layout_reg.fixed_offset(super::engine::layout::STRING, super::engine::layout::string::DATA) as i32); i32_add; local_get(pos); i32_add;
                      local_get(i); i32_const(elem_bytes); i32_mul; i32_add;
                });
                // i16/u16 LE: native load, sign/zero extend
                if !is_be && (method == "read_i16_le_array" || method == "read_u16_le_array") {
                    if method == "read_i16_le_array" {
                        wasm!(self.func, { i32_load16_s(0); i64_extend_i32_s; i64_store(0); });
                    } else {
                        wasm!(self.func, { i32_load16_u(0); i64_extend_i32_u; i64_store(0); });
                    }
                } else if is_be {
                    // BE path: load each byte and reassemble manually.
                    // Stack already has dst address. Save it, then build value.
                    let dst_addr = self.scratch.alloc_i32();
                    let src_addr = self.scratch.alloc_i32();
                    let acc = self.scratch.alloc_i64();
                    wasm!(self.func, { local_set(src_addr); local_set(dst_addr); });
                    // Build acc = (b[0] << (8*(n-1))) | (b[1] << (8*(n-2))) | ... | b[n-1]
                    wasm!(self.func, { i64_const(0); local_set(acc); });
                    for i in 0..(elem_bytes as u32) {
                        let shift = 8 * ((elem_bytes as u32) - 1 - i) as i64;
                        wasm!(self.func, {
                            local_get(acc);
                            local_get(src_addr);
                            i32_load8_u(i as u64);
                            i64_extend_i32_u;
                            i64_const(shift); i64_shl;
                            i64_or;
                            local_set(acc);
                        });
                    }
                    // Now write into dst_addr based on method
                    match method {
                        "read_i16_be_array" => {
                            // sign-extend 16-bit
                            wasm!(self.func, {
                                local_get(dst_addr);
                                local_get(acc); i32_wrap_i64; i32_const(16); i32_shl;
                                i32_const(16); i32_shr_s;
                                i64_extend_i32_s;
                                i64_store(0);
                            });
                        }
                        "read_u16_be_array" => {
                            wasm!(self.func, { local_get(dst_addr); local_get(acc); i64_store(0); });
                        }
                        "read_i32_be_array" => {
                            // sign-extend 32-bit value
                            wasm!(self.func, {
                                local_get(dst_addr);
                                local_get(acc); i32_wrap_i64; i64_extend_i32_s;
                                i64_store(0);
                            });
                        }
                        "read_u32_be_array" => {
                            wasm!(self.func, { local_get(dst_addr); local_get(acc); i64_store(0); });
                        }
                        "read_i64_be_array" => {
                            wasm!(self.func, { local_get(dst_addr); local_get(acc); i64_store(0); });
                        }
                        "read_f32_be_array" => {
                            wasm!(self.func, {
                                local_get(dst_addr);
                                local_get(acc); i32_wrap_i64; f32_reinterpret_i32; f64_promote_f32;
                                f64_store(0);
                            });
                        }
                        "read_f64_be_array" => {
                            wasm!(self.func, {
                                local_get(dst_addr);
                                local_get(acc); f64_reinterpret_i64;
                                f64_store(0);
                            });
                        }
                        _ => {}
                    }
                    self.scratch.free_i64(acc);
                    self.scratch.free_i32(src_addr);
                    self.scratch.free_i32(dst_addr);
                } else {
                    match method {
                        "read_i32_le_array" => {
                            wasm!(self.func, { i32_load(0); i64_extend_i32_s; i64_store(0); });
                        }
                        "read_u32_le_array" => {
                            wasm!(self.func, { i32_load(0); i64_extend_i32_u; i64_store(0); });
                        }
                        "read_i64_le_array" => {
                            wasm!(self.func, { i64_load(0); i64_store(0); });
                        }
                        "read_f32_le_array" => {
                            wasm!(self.func, { f32_load(0); f64_promote_f32; f64_store(0); });
                        }
                        "read_f64_le_array" => {
                            wasm!(self.func, { f64_load(0); f64_store(0); });
                        }
                        "read_f16_le_array" => {
                            // f16 bits → f64 via runtime
                            wasm!(self.func, {
                                i32_load16_u(0);
                                call(self.emitter.rt.bytes_f16_to_f64);
                                f64_store(0);
                            });
                        }
                        _ => {}
                    }
                }
                wasm!(self.func, {
                      local_get(i); i32_const(1); i32_add; local_set(i);
                      br(0);
                    end; end;
                    local_get(result);
                });
                self.scratch.free_i32(i);
                self.scratch.free_i32(result);
                self.scratch.free_i32(n);
                self.scratch.free_i32(pos);
                self.scratch.free_i32(buf);
            }
            _ => return false,
        }
        true
    }
}
