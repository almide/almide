//! Bytes stdlib call dispatch for WASM codegen.
//!
//! Memory layout: [len:i32][data:u8...]  (same as String)

use super::FuncCompiler;
use almide_ir::IrExpr;

/// Requested primitive load for the typed byte-read family.
#[derive(Clone, Copy)]
enum ByteReadOp {
    U8,
    I16Le,
    I32Le,
    U32Le,
    U16Le,
    I64Le,
    F32Le,
    F64Le,
    F16Le,
}

impl FuncCompiler<'_> {
    /// Dispatch a bytes stdlib method call. Returns true if handled.
    pub(super) fn emit_bytes_call(&mut self, method: &str, args: &[IrExpr]) -> bool {
        match method {
            "len" => {
                // bytes.len(b) → Int (i64)
                self.emit_expr(&args[0]);
                wasm!(self.func, { i32_load(0); i64_extend_i32_u; });
            }
            "is_empty" => {
                // bytes.is_empty(b) → Bool (i32)
                self.emit_expr(&args[0]);
                wasm!(self.func, { i32_load(0); i32_eqz; });
            }
            "get" => {
                // bytes.get(b, i) → Option[Int]
                // none = null_ptr (0), some = alloc [value:i64]
                let buf = self.scratch.alloc_i32();
                let idx = self.scratch.alloc_i32();
                let result = self.scratch.alloc_i32();
                self.emit_expr(&args[0]);
                wasm!(self.func, { local_set(buf); });
                self.emit_expr(&args[1]);
                wasm!(self.func, {
                    i32_wrap_i64; local_set(idx);
                    // bounds check: idx < 0 || idx >= len → none (0)
                    local_get(idx);
                    local_get(buf); i32_load(0);
                    i32_ge_u;
                    local_get(idx); i32_const(0); i32_lt_s;
                    i32_or;
                    if_i32;
                      i32_const(0); // none
                    else_;
                      // alloc 8 bytes for i64 value
                      i32_const(8);
                      call(self.emitter.rt.alloc);
                      local_set(result);
                      local_get(result);
                      // load byte as u8 → i64
                      local_get(buf); i32_const(4); i32_add;
                      local_get(idx); i32_add;
                      i32_load8_u(0);
                      i64_extend_i32_u;
                      i64_store(0);
                      local_get(result);
                    end;
                });
                self.scratch.free_i32(result);
                self.scratch.free_i32(idx);
                self.scratch.free_i32(buf);
            }
            "get_or" => {
                // bytes.get_or(b, i, default) → Int (i64)
                let buf = self.scratch.alloc_i32();
                let idx = self.scratch.alloc_i32();
                self.emit_expr(&args[0]);
                wasm!(self.func, { local_set(buf); });
                self.emit_expr(&args[1]);
                wasm!(self.func, {
                    i32_wrap_i64; local_set(idx);
                    local_get(idx);
                    local_get(buf); i32_load(0);
                    i32_ge_u;
                    local_get(idx); i32_const(0); i32_lt_s;
                    i32_or;
                    if_i64;
                });
                self.emit_expr(&args[2]); // default
                wasm!(self.func, {
                    else_;
                      local_get(buf); i32_const(4); i32_add;
                      local_get(idx); i32_add;
                      i32_load8_u(0);
                      i64_extend_i32_u;
                    end;
                });
                self.scratch.free_i32(idx);
                self.scratch.free_i32(buf);
            }
            "set" => {
                // bytes.set(b, i, val) → Bytes (mutate in place, return same pointer)
                let buf = self.scratch.alloc_i32();
                let idx = self.scratch.alloc_i32();
                let val = self.scratch.alloc_i32();
                self.emit_expr(&args[0]);
                wasm!(self.func, { local_set(buf); });
                self.emit_expr(&args[1]);
                wasm!(self.func, { i32_wrap_i64; local_set(idx); });
                self.emit_expr(&args[2]);
                wasm!(self.func, {
                    i32_wrap_i64; local_set(val);
                    // bounds check: idx < len
                    local_get(idx); local_get(buf); i32_load(0); i32_lt_u;
                    if_empty;
                      // store byte: mem[buf + 4 + idx] = val
                      local_get(buf); i32_const(4); i32_add; local_get(idx); i32_add;
                      local_get(val);
                      i32_store8(0);
                    end;
                    // return buf pointer
                    local_get(buf);
                });
                self.scratch.free_i32(val);
                self.scratch.free_i32(idx);
                self.scratch.free_i32(buf);
            }
            "new" => {
                // bytes.new(len) → Bytes: alloc [len:i32][zeroed data]
                let n = self.scratch.alloc_i32();
                let ptr = self.scratch.alloc_i32();
                self.emit_expr(&args[0]);
                wasm!(self.func, {
                    i32_wrap_i64; local_set(n);
                    // alloc 4 + n bytes
                    local_get(n); i32_const(4); i32_add;
                    call(self.emitter.rt.alloc);
                    local_set(ptr);
                    // store length
                    local_get(ptr); local_get(n); i32_store(0);
                    // zero the data region: memory.fill(ptr+4, 0, n)
                    local_get(ptr); i32_const(4); i32_add;
                    i32_const(0);
                    local_get(n);
                    memory_fill;
                    local_get(ptr);
                });
                self.scratch.free_i32(ptr);
                self.scratch.free_i32(n);
            }
            "from_list" => {
                // bytes.from_list(xs: List[Int]) → Bytes
                // List layout: [len:i32][elem0:i64][elem1:i64]...
                // Bytes layout: [len:i32][byte0:u8][byte1:u8]...
                let xs = self.scratch.alloc_i32();
                let dst = self.scratch.alloc_i32();
                let len = self.scratch.alloc_i32();
                let i = self.scratch.alloc_i32();
                self.emit_expr(&args[0]);
                wasm!(self.func, {
                    local_set(xs);
                    local_get(xs); i32_load(0); local_set(len);
                    // alloc 4 + len
                    local_get(len); i32_const(4); i32_add;
                    call(self.emitter.rt.alloc);
                    local_set(dst);
                    local_get(dst); local_get(len); i32_store(0);
                    // loop: copy each i64 as u8
                    i32_const(0); local_set(i);
                    block_empty; loop_empty;
                      local_get(i); local_get(len); i32_ge_u; br_if(1);
                      // dst_byte_addr = dst + 4 + i
                      local_get(dst); i32_const(4); i32_add; local_get(i); i32_add;
                      // src_elem = xs + 4 + i*8 → load i64, wrap to i32, store as u8
                      local_get(xs); i32_const(4); i32_add;
                      local_get(i); i32_const(8); i32_mul; i32_add;
                      i64_load(0);
                      i32_wrap_i64;
                      i32_store8(0);
                      local_get(i); i32_const(1); i32_add; local_set(i);
                      br(0);
                    end; end;
                    local_get(dst);
                });
                self.scratch.free_i32(i);
                self.scratch.free_i32(len);
                self.scratch.free_i32(dst);
                self.scratch.free_i32(xs);
            }
            "to_list" => {
                // bytes.to_list(b) → List[Int]
                // Bytes: [len:i32][u8...]  →  List: [len:i32][i64...]
                let src = self.scratch.alloc_i32();
                let dst = self.scratch.alloc_i32();
                let len = self.scratch.alloc_i32();
                let i = self.scratch.alloc_i32();
                self.emit_expr(&args[0]);
                wasm!(self.func, {
                    local_set(src);
                    local_get(src); i32_load(0); local_set(len);
                    // alloc 4 + len*8
                    local_get(len); i32_const(8); i32_mul; i32_const(4); i32_add;
                    call(self.emitter.rt.alloc);
                    local_set(dst);
                    local_get(dst); local_get(len); i32_store(0);
                    // loop
                    i32_const(0); local_set(i);
                    block_empty; loop_empty;
                      local_get(i); local_get(len); i32_ge_u; br_if(1);
                      // dst + 4 + i*8
                      local_get(dst); i32_const(4); i32_add;
                      local_get(i); i32_const(8); i32_mul; i32_add;
                      // load u8 from src + 4 + i, extend to i64
                      local_get(src); i32_const(4); i32_add;
                      local_get(i); i32_add;
                      i32_load8_u(0);
                      i64_extend_i32_u;
                      i64_store(0);
                      local_get(i); i32_const(1); i32_add; local_set(i);
                      br(0);
                    end; end;
                    local_get(dst);
                });
                self.scratch.free_i32(i);
                self.scratch.free_i32(len);
                self.scratch.free_i32(dst);
                self.scratch.free_i32(src);
            }
            "slice" => {
                // bytes.slice(b, start, end) → Bytes
                let src = self.scratch.alloc_i32();
                let s = self.scratch.alloc_i32();
                let e = self.scratch.alloc_i32();
                let new_len = self.scratch.alloc_i32();
                let dst = self.scratch.alloc_i32();
                self.emit_expr(&args[0]);
                wasm!(self.func, { local_set(src); });
                self.emit_expr(&args[1]);
                wasm!(self.func, { i32_wrap_i64; local_set(s); });
                self.emit_expr(&args[2]);
                wasm!(self.func, {
                    i32_wrap_i64; local_set(e);
                    // clamp start to [0, len]
                    local_get(s); i32_const(0); i32_lt_s;
                    if_empty; i32_const(0); local_set(s); end;
                    local_get(s); local_get(src); i32_load(0); i32_gt_u;
                    if_empty; local_get(src); i32_load(0); local_set(s); end;
                    // clamp end to [start, len]
                    local_get(e); local_get(s); i32_lt_s;
                    if_empty; local_get(s); local_set(e); end;
                    local_get(e); local_get(src); i32_load(0); i32_gt_u;
                    if_empty; local_get(src); i32_load(0); local_set(e); end;
                    // new_len = e - s
                    local_get(e); local_get(s); i32_sub; local_set(new_len);
                    // alloc
                    local_get(new_len); i32_const(4); i32_add;
                    call(self.emitter.rt.alloc);
                    local_set(dst);
                    local_get(dst); local_get(new_len); i32_store(0);
                    // memory.copy(dst+4, src+4+s, new_len)
                    local_get(dst); i32_const(4); i32_add;
                    local_get(src); i32_const(4); i32_add; local_get(s); i32_add;
                    local_get(new_len);
                    memory_copy;
                    local_get(dst);
                });
                self.scratch.free_i32(dst);
                self.scratch.free_i32(new_len);
                self.scratch.free_i32(e);
                self.scratch.free_i32(s);
                self.scratch.free_i32(src);
            }
            "concat" => {
                // bytes.concat(a, b) → Bytes
                let a = self.scratch.alloc_i32();
                let b = self.scratch.alloc_i32();
                let dst = self.scratch.alloc_i32();
                let len_a = self.scratch.alloc_i32();
                let len_b = self.scratch.alloc_i32();
                self.emit_expr(&args[0]);
                wasm!(self.func, { local_set(a); });
                self.emit_expr(&args[1]);
                wasm!(self.func, {
                    local_set(b);
                    local_get(a); i32_load(0); local_set(len_a);
                    local_get(b); i32_load(0); local_set(len_b);
                    // alloc 4 + len_a + len_b
                    local_get(len_a); local_get(len_b); i32_add; i32_const(4); i32_add;
                    call(self.emitter.rt.alloc);
                    local_set(dst);
                    // store total length
                    local_get(dst);
                    local_get(len_a); local_get(len_b); i32_add;
                    i32_store(0);
                    // copy a data
                    local_get(dst); i32_const(4); i32_add;
                    local_get(a); i32_const(4); i32_add;
                    local_get(len_a);
                    memory_copy;
                    // copy b data
                    local_get(dst); i32_const(4); i32_add; local_get(len_a); i32_add;
                    local_get(b); i32_const(4); i32_add;
                    local_get(len_b);
                    memory_copy;
                    local_get(dst);
                });
                self.scratch.free_i32(len_b);
                self.scratch.free_i32(len_a);
                self.scratch.free_i32(dst);
                self.scratch.free_i32(b);
                self.scratch.free_i32(a);
            }
            "repeat" => {
                // bytes.repeat(b, n) → Bytes
                let src = self.scratch.alloc_i32();
                let n = self.scratch.alloc_i32();
                let src_len = self.scratch.alloc_i32();
                let dst = self.scratch.alloc_i32();
                let total = self.scratch.alloc_i32();
                let i = self.scratch.alloc_i32();
                self.emit_expr(&args[0]);
                wasm!(self.func, { local_set(src); });
                self.emit_expr(&args[1]);
                wasm!(self.func, {
                    i32_wrap_i64; local_set(n);
                    // clamp n to >= 0
                    local_get(n); i32_const(0); i32_lt_s;
                    if_empty; i32_const(0); local_set(n); end;
                    local_get(src); i32_load(0); local_set(src_len);
                    local_get(src_len); local_get(n); i32_mul; local_set(total);
                    // alloc 4 + total
                    local_get(total); i32_const(4); i32_add;
                    call(self.emitter.rt.alloc);
                    local_set(dst);
                    local_get(dst); local_get(total); i32_store(0);
                    // loop: copy src data n times
                    i32_const(0); local_set(i);
                    block_empty; loop_empty;
                      local_get(i); local_get(n); i32_ge_u; br_if(1);
                      // dst + 4 + i*src_len
                      local_get(dst); i32_const(4); i32_add;
                      local_get(i); local_get(src_len); i32_mul; i32_add;
                      local_get(src); i32_const(4); i32_add;
                      local_get(src_len);
                      memory_copy;
                      local_get(i); i32_const(1); i32_add; local_set(i);
                      br(0);
                    end; end;
                    local_get(dst);
                });
                self.scratch.free_i32(i);
                self.scratch.free_i32(total);
                self.scratch.free_i32(dst);
                self.scratch.free_i32(src_len);
                self.scratch.free_i32(n);
                self.scratch.free_i32(src);
            }
            "push" => {
                // bytes.push(b, val): append 1 byte to buf
                // Layout: [len:i32][data...] → store val at buf+4+len, len++
                // NOTE: this mutates in-place. For simplicity, realloc to len+1.
                let buf = self.scratch.alloc_i32();
                let old_len = self.scratch.alloc_i32();
                let new_buf = self.scratch.alloc_i32();
                self.emit_expr(&args[0]); // buf ptr
                wasm!(self.func, { local_set(buf); });
                self.emit_expr(&args[1]); // val (i64)
                wasm!(self.func, { i32_wrap_i64; }); // val as i32
                let val = self.scratch.alloc_i32();
                wasm!(self.func, { local_set(val); });
                wasm!(self.func, {
                    // old_len = buf[0]
                    local_get(buf); i32_load(0); local_set(old_len);
                    // new_buf = alloc(4 + old_len + 1)
                    local_get(old_len); i32_const(5); i32_add;
                    call(self.emitter.rt.alloc);
                    local_set(new_buf);
                    // new_buf[0] = old_len + 1
                    local_get(new_buf); local_get(old_len); i32_const(1); i32_add; i32_store(0);
                    // copy old data: new_buf+4 <- buf+4, old_len bytes
                    local_get(new_buf); i32_const(4); i32_add;
                    local_get(buf); i32_const(4); i32_add;
                    local_get(old_len);
                    memory_copy;
                    // new_buf[4 + old_len] = val
                    local_get(new_buf); i32_const(4); i32_add; local_get(old_len); i32_add;
                    local_get(val); i32_store8(0);
                });
                // Update the variable: need to store new_buf back
                // The buf variable is the first arg — if it's a Var, update the local
                if let almide_ir::IrExprKind::Var { id } = &args[0].kind {
                    if let Some(&local_idx) = self.var_map.get(&id.0) {
                        wasm!(self.func, { local_get(new_buf); local_set(local_idx); });
                    }
                }
                self.scratch.free_i32(val);
                self.scratch.free_i32(new_buf);
                self.scratch.free_i32(old_len);
                self.scratch.free_i32(buf);
            }
            "clear" => {
                // bytes.clear(b): set len to 0
                self.emit_expr(&args[0]);
                wasm!(self.func, { i32_const(0); i32_store(0); });
            }
            "append_f64_le" => {
                // bytes.append_f64_le(b, val): append 8 bytes (f64 little-endian).
                // Like `push` but for an f64 — realloc to len+8 and store.
                // Mutates the variable in-place when arg[0] is a Var.
                let buf = self.scratch.alloc_i32();
                let old_len = self.scratch.alloc_i32();
                let new_buf = self.scratch.alloc_i32();
                let fval = self.scratch.alloc_f64();
                self.emit_expr(&args[0]); // buf ptr
                wasm!(self.func, { local_set(buf); });
                self.emit_expr(&args[1]); // val: f64 on stack
                wasm!(self.func, {
                    local_set(fval);
                    // old_len = buf[0]
                    local_get(buf); i32_load(0); local_set(old_len);
                    // new_buf = alloc(4 + old_len + 8)
                    local_get(old_len); i32_const(12); i32_add;
                    call(self.emitter.rt.alloc);
                    local_set(new_buf);
                    // new_buf[0] = old_len + 8
                    local_get(new_buf); local_get(old_len); i32_const(8); i32_add; i32_store(0);
                    // copy old data: new_buf+4 <- buf+4, old_len bytes
                    local_get(new_buf); i32_const(4); i32_add;
                    local_get(buf); i32_const(4); i32_add;
                    local_get(old_len);
                    memory_copy;
                    // *(new_buf + 4 + old_len) = fval (f64 LE)
                    local_get(new_buf); i32_const(4); i32_add; local_get(old_len); i32_add;
                    local_get(fval);
                    f64_store(0);
                });
                if let almide_ir::IrExprKind::Var { id } = &args[0].kind {
                    if let Some(&local_idx) = self.var_map.get(&id.0) {
                        wasm!(self.func, { local_get(new_buf); local_set(local_idx); });
                    }
                }
                self.scratch.free_f64(fval);
                self.scratch.free_i32(new_buf);
                self.scratch.free_i32(old_len);
                self.scratch.free_i32(buf);
            }
            "append_f32_le" => self.emit_bytes_append_f(args, /*size_bytes=*/4, /*as_f32=*/true),
            "append_u8" => self.emit_bytes_append_i(args, 1),
            "append_u16_le" => self.emit_bytes_append_i(args, 2),
            "append_u32_le" => self.emit_bytes_append_i(args, 4),
            "append_i32_le" => self.emit_bytes_append_i(args, 4),
            "append_i64_le" => self.emit_bytes_append_i(args, 8),
            "map_each" => self.emit_bytes_map_each(args),
            "xor" => self.emit_bytes_xor(args),
            "pad_left" => self.emit_bytes_pad(args, /*left=*/true),
            "pad_right" => self.emit_bytes_pad(args, /*left=*/false),
            "copy_from" => self.emit_bytes_copy_from(args),
            "reverse" => self.emit_bytes_reverse(args),
            "fill" => self.emit_bytes_fill(args),
            "insert" => self.emit_bytes_insert(args),
            "remove_at" => self.emit_bytes_remove_at(args),
            "chunks" => self.emit_bytes_chunks(args),
            "split" => self.emit_bytes_split(args, /*single_byte=*/false, /*lf=*/false),
            "lines" => self.emit_bytes_split(args, /*single_byte=*/true, /*lf=*/true),
            "starts_with" => self.emit_bytes_prefix_match(args, /*at_end=*/false),
            "ends_with" => self.emit_bytes_prefix_match(args, /*at_end=*/true),
            "contains" => {
                self.emit_bytes_index_of_inner(args);
                wasm!(self.func, { i32_const(-1); i64_extend_i32_s; i64_ne; });
            }
            "index_of" => {
                self.emit_bytes_index_of_inner(args);
                // Wrap result: -1 → none, else some(pos).
                let pos_i64 = self.scratch.alloc_i64();
                let opt_ptr = self.scratch.alloc_i32();
                wasm!(self.func, {
                    local_set(pos_i64);
                    local_get(pos_i64); i64_const(0); i64_lt_s;
                    if_i32;
                        i32_const(0);
                    else_;
                        i32_const(8); call(self.emitter.rt.alloc); local_set(opt_ptr);
                        local_get(opt_ptr); local_get(pos_i64); i64_store(0);
                        local_get(opt_ptr);
                    end;
                });
                self.scratch.free_i32(opt_ptr);
                self.scratch.free_i64(pos_i64);
            }
            "cmp" => self.emit_bytes_cmp(args),
            "from_string" => {
                // bytes.from_string(s): String and Bytes have same layout [len:i32][data:u8...]
                // Just return the string pointer (effectively a cast)
                self.emit_expr(&args[0]);
            }
            "to_string_lossy" => {
                // Same layout as String. WASM target does not yet validate UTF-8;
                // invalid sequences pass through unchanged (the JS host will see
                // garbage but no panic). Real lossy substitution lives in the
                // Rust runtime.
                self.emit_expr(&args[0]);
            }
            "is_valid_utf8" => self.emit_bytes_is_valid_utf8(args),
            "to_string" => {
                // Validate UTF-8 first; on success wrap as ok(b), else err.
                let buf = self.scratch.alloc_i32();
                let res = self.scratch.alloc_i32();
                self.emit_expr(&args[0]);
                wasm!(self.func, { local_set(buf); });
                // Push buf, validate, branch on result
                let dummy_arg = IrExpr {
                    kind: args[0].kind.clone(),
                    ty: args[0].ty.clone(),
                    span: args[0].span,
                };
                self.emit_bytes_is_valid_utf8(std::slice::from_ref(&dummy_arg));
                let err_str = self.emitter.intern_string("invalid UTF-8");
                wasm!(self.func, {
                    if_i32;
                        i32_const(8); call(self.emitter.rt.alloc); local_set(res);
                        local_get(res); i32_const(0); i32_store(0);
                        local_get(res); local_get(buf); i32_store(4);
                        local_get(res);
                    else_;
                        i32_const(8); call(self.emitter.rt.alloc); local_set(res);
                        local_get(res); i32_const(1); i32_store(0);
                        local_get(res); i32_const(err_str as i32); i32_store(4);
                        local_get(res);
                    end;
                });
                self.scratch.free_i32(res);
                self.scratch.free_i32(buf);
            }
            "set_f32_le" => {
                // bytes.set_f32_le(b, pos, val) → Unit
                // f32.store [addr, f32_val]: addr = buf + 4 + pos
                let buf = self.scratch.alloc_i32();
                let addr = self.scratch.alloc_i32();
                let fval = self.scratch.alloc_f64();
                self.emit_expr(&args[0]);
                wasm!(self.func, { local_set(buf); });
                self.emit_expr(&args[1]);
                wasm!(self.func, {
                    i32_wrap_i64;
                    local_get(buf); i32_const(4); i32_add; i32_add;
                    local_set(addr);
                });
                self.emit_expr(&args[2]); // val: f64 on stack
                wasm!(self.func, {
                    local_set(fval);
                    // push addr, then demoted val, then store
                    local_get(addr);
                    local_get(fval);
                    f32_demote_f64;
                    f32_store(0);
                });
                self.scratch.free_f64(fval);
                self.scratch.free_i32(addr);
                self.scratch.free_i32(buf);
            }
            "set_u16_le" => {
                // bytes.set_u16_le(b, pos, val) → Unit
                // Store u16 little-endian at buf + 4 + pos
                let buf = self.scratch.alloc_i32();
                let pos = self.scratch.alloc_i32();
                let val = self.scratch.alloc_i32();
                self.emit_expr(&args[0]);
                wasm!(self.func, { local_set(buf); });
                self.emit_expr(&args[1]);
                wasm!(self.func, { i32_wrap_i64; local_set(pos); });
                self.emit_expr(&args[2]);
                wasm!(self.func, {
                    i32_wrap_i64; local_set(val);
                    // address = buf + 4 + pos
                    local_get(buf); i32_const(4); i32_add; local_get(pos); i32_add;
                    local_get(val);
                    i32_store16(0);
                });
                self.scratch.free_i32(val);
                self.scratch.free_i32(pos);
                self.scratch.free_i32(buf);
            }
            "set_u8" => self.emit_bytes_set_i(args, 1),
            "set_i16_le" => self.emit_bytes_set_i(args, 2),
            "set_u32_le" => self.emit_bytes_set_i(args, 4),
            "set_i32_le" => self.emit_bytes_set_i(args, 4),
            "set_i64_le" => self.emit_bytes_set_i(args, 8),
            "set_f64_le" => self.emit_bytes_set_f(args, /*size_bytes=*/8, /*as_f32=*/false),
            "set_u16_be" => self.emit_bytes_set_i_be(args, 2),
            "set_i16_be" => self.emit_bytes_set_i_be(args, 2),
            "set_u32_be" => self.emit_bytes_set_i_be(args, 4),
            "set_i32_be" => self.emit_bytes_set_i_be(args, 4),
            "set_i64_be" => self.emit_bytes_set_i_be(args, 8),
            "set_f32_be" => self.emit_bytes_set_f_be(args, 4),
            "set_f64_be" => self.emit_bytes_set_f_be(args, 8),
            "append_i16_le" => self.emit_bytes_append_i(args, 2),
            "append_i16_be" => self.emit_bytes_append_i_be(args, 2),
            "append_u16_be" => self.emit_bytes_append_i_be(args, 2),
            "append_u32_be" => self.emit_bytes_append_i_be(args, 4),
            "append_i32_be" => self.emit_bytes_append_i_be(args, 4),
            "append_i64_be" => self.emit_bytes_append_i_be(args, 8),
            "append_f32_be" => self.emit_bytes_append_f_be(args, 4),
            "append_f64_be" => self.emit_bytes_append_f_be(args, 8),
            "data_ptr" => {
                // bytes.data_ptr(b) → Int (i64)
                // Return pointer to data region: buf + 4
                self.emit_expr(&args[0]);
                wasm!(self.func, { i32_const(4); i32_add; i64_extend_i32_u; });
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
            "read_f32_le_at" => self.emit_cursor_read_float(args, 4, false),
            "read_f64_le_at" => self.emit_cursor_read_float(args, 8, false),
            "read_f32_be_at" => self.emit_cursor_read_float(args, 4, true),
            "read_f64_be_at" => self.emit_cursor_read_float(args, 8, true),
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
                    local_get(buf); i32_const(4); i32_add; i32_add; local_set(src);
                });
                self.emit_expr(&args[2]);
                wasm!(self.func, {
                    i32_wrap_i64; local_set(len);
                    // alloc 4 + len
                    local_get(len); i32_const(4); i32_add;
                    call(self.emitter.rt.alloc); local_set(dst);
                    local_get(dst); local_get(len); i32_store(0);
                    local_get(dst); i32_const(4); i32_add;
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
                      local_get(buf); i32_const(4); i32_add; local_get(pos); i32_add;
                      i32_load(0); local_set(lval);
                      // pos += 4 + len
                      local_get(pos); i32_const(4); i32_add; local_get(lval); i32_add;
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
                    local_get(n); i32_const(4); i32_mul; i32_const(4); i32_add;
                    call(self.emitter.rt.alloc); local_set(result);
                    local_get(result); local_get(n); i32_store(0);
                    i32_const(0); local_set(i);
                    block_empty; loop_empty;
                      local_get(i); local_get(n); i32_ge_u; br_if(1);
                      // len at [buf+4+pos]
                      local_get(buf); i32_const(4); i32_add; local_get(pos); i32_add;
                      i32_load(0); local_set(lval);
                      // alloc string: [len][bytes]
                      local_get(lval); i32_const(4); i32_add;
                      call(self.emitter.rt.alloc); local_set(s);
                      local_get(s); local_get(lval); i32_store(0);
                      // memcpy bytes: dst = s+4, src = buf+4+pos+4, n = lval
                      local_get(s); i32_const(4); i32_add;
                      local_get(buf); i32_const(4); i32_add;
                      local_get(pos); i32_add; i32_const(4); i32_add;
                      local_get(lval);
                      memory_copy;
                      // result[i] = s
                      local_get(result); i32_const(4); i32_add;
                      local_get(i); i32_const(4); i32_mul; i32_add;
                      local_get(s); i32_store(0);
                      // pos += 4 + len
                      local_get(pos); i32_const(4); i32_add; local_get(lval); i32_add;
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
                    local_get(n); i32_const(out_bytes); i32_mul; i32_const(4); i32_add;
                    call(self.emitter.rt.alloc); local_set(result);
                    local_get(result); local_get(n); i32_store(0);
                    i32_const(0); local_set(i);
                    block_empty; loop_empty;
                      local_get(i); local_get(n); i32_ge_u; br_if(1);
                      // dst = result + 4 + i * out_bytes
                      local_get(result); i32_const(4); i32_add;
                      local_get(i); i32_const(out_bytes); i32_mul; i32_add;
                      // src addr = buf + 4 + pos + i * elem_bytes
                      local_get(buf); i32_const(4); i32_add; local_get(pos); i32_add;
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

    /// Emit `[data_ptr + pos]` loaded as the requested primitive type.
    /// `buf` is the bytes pointer (Bytes layout: [len:i32][data...]).
    /// `pos` is an Int (i64) byte offset into the data region.
    fn emit_typed_byte_read(&mut self, buf_expr: &IrExpr, pos_expr: &IrExpr, op: ByteReadOp) {
        // Compute address = buf + 4 + pos.
        self.emit_expr(buf_expr);
        wasm!(self.func, { i32_const(4); i32_add; });
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
        // new_buf = alloc(4 + old_len + size_bytes)
        wasm!(self.func, {
            local_get(old_len); i32_const(4 + size_bytes as i32); i32_add;
            call(self.emitter.rt.alloc); local_set(new_buf);
            // new_buf[0] = old_len + size_bytes
            local_get(new_buf); local_get(old_len); i32_const(size_bytes as i32); i32_add; i32_store(0);
            // memcpy old data
            local_get(new_buf); i32_const(4); i32_add;
            local_get(buf); i32_const(4); i32_add;
            local_get(old_len);
            memory_copy;
            // address = new_buf + 4 + old_len
            local_get(new_buf); i32_const(4); i32_add; local_get(old_len); i32_add;
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
        if let almide_ir::IrExprKind::Var { id } = &args[0].kind {
            if let Some(&local_idx) = self.var_map.get(&id.0) {
                wasm!(self.func, { local_get(new_buf); local_set(local_idx); });
            }
        }
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
            local_get(old_len); i32_const(4 + size_bytes as i32); i32_add;
            call(self.emitter.rt.alloc); local_set(new_buf);
            local_get(new_buf); local_get(old_len); i32_const(size_bytes as i32); i32_add; i32_store(0);
            local_get(new_buf); i32_const(4); i32_add;
            local_get(buf); i32_const(4); i32_add;
            local_get(old_len);
            memory_copy;
            local_get(new_buf); i32_const(4); i32_add; local_get(old_len); i32_add;
        });
        if as_f32 {
            wasm!(self.func, { local_get(fval); f32_demote_f64; f32_store(0); });
        } else {
            wasm!(self.func, { local_get(fval); f64_store(0); });
        }
        let _ = as_f32; // satisfy unused-var lint when both branches identical
        if let almide_ir::IrExprKind::Var { id } = &args[0].kind {
            if let Some(&local_idx) = self.var_map.get(&id.0) {
                wasm!(self.func, { local_get(new_buf); local_set(local_idx); });
            }
        }
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
            local_get(buf); i32_const(4); i32_add; i32_add; local_set(src);
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
            local_get(buf); i32_const(4); i32_add; local_get(pos); i32_add;
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
            local_get(buf); i32_const(4); i32_add; local_get(pos); i32_add;
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
            local_get(buf); i32_const(4); i32_add; local_get(pos); i32_add; local_set(dst);
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
            local_get(buf); i32_const(4); i32_add; local_get(pos); i32_add; local_set(dst);
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
            local_get(old_len); i32_const(4 + size_bytes as i32); i32_add;
            call(self.emitter.rt.alloc); local_set(new_buf);
            local_get(new_buf); local_get(old_len); i32_const(size_bytes as i32); i32_add; i32_store(0);
            local_get(new_buf); i32_const(4); i32_add;
            local_get(buf); i32_const(4); i32_add;
            local_get(old_len);
            memory_copy;
            local_get(new_buf); i32_const(4); i32_add; local_get(old_len); i32_add;
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
        if let almide_ir::IrExprKind::Var { id } = &args[0].kind {
            if let Some(&local_idx) = self.var_map.get(&id.0) {
                wasm!(self.func, { local_get(new_buf); local_set(local_idx); });
            }
        }
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
            local_get(old_len); i32_const(4 + size_bytes as i32); i32_add;
            call(self.emitter.rt.alloc); local_set(new_buf);
            local_get(new_buf); local_get(old_len); i32_const(size_bytes as i32); i32_add; i32_store(0);
            local_get(new_buf); i32_const(4); i32_add;
            local_get(buf); i32_const(4); i32_add;
            local_get(old_len);
            memory_copy;
            local_get(new_buf); i32_const(4); i32_add; local_get(old_len); i32_add;
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
        if let almide_ir::IrExprKind::Var { id } = &args[0].kind {
            if let Some(&local_idx) = self.var_map.get(&id.0) {
                wasm!(self.func, { local_get(new_buf); local_set(local_idx); });
            }
        }
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
            local_get(len); i32_const(4); i32_add; call(self.emitter.rt.alloc); local_set(dst);
            local_get(dst); local_get(len); i32_store(0);
            i32_const(0); local_set(i);
            block_empty; loop_empty;
                local_get(i); local_get(len); i32_ge_u; br_if(1);
                // dst[i] = (i32) f((i64) b[i])
                local_get(dst); i32_const(4); i32_add; local_get(i); i32_add;
                // closure call args: env, arg, table_idx
                local_get(closure); i32_load(4);
                local_get(buf); i32_const(4); i32_add; local_get(i); i32_add;
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
            local_get(n); i32_const(4); i32_add; call(self.emitter.rt.alloc); local_set(dst);
            local_get(dst); local_get(n); i32_store(0);
            i32_const(0); local_set(i);
            block_empty; loop_empty;
                local_get(i); local_get(n); i32_ge_u; br_if(1);
                local_get(dst); i32_const(4); i32_add; local_get(i); i32_add;
                local_get(a); i32_const(4); i32_add; local_get(i); i32_add; i32_load8_u(0);
                local_get(b); i32_const(4); i32_add; local_get(i); i32_add; i32_load8_u(0);
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
                local_get(blen); i32_const(4); i32_add; call(self.emitter.rt.alloc); local_set(dst);
                local_get(dst); local_get(buf); local_get(blen); i32_const(4); i32_add; memory_copy;
                local_get(dst);
            else_;
                local_get(target); local_get(blen); i32_sub; local_set(pad);
                local_get(target); i32_const(4); i32_add; call(self.emitter.rt.alloc); local_set(dst);
                local_get(dst); local_get(target); i32_store(0);
        });
        if left {
            wasm!(self.func, {
                // Fill [0, pad) with val
                i32_const(0); local_set(i);
                block_empty; loop_empty;
                    local_get(i); local_get(pad); i32_ge_u; br_if(1);
                    local_get(dst); i32_const(4); i32_add; local_get(i); i32_add;
                    local_get(val); i32_store8(0);
                    local_get(i); i32_const(1); i32_add; local_set(i);
                    br(0);
                end; end;
                // Copy original into [pad..target)
                local_get(dst); i32_const(4); i32_add; local_get(pad); i32_add;
                local_get(buf); i32_const(4); i32_add;
                local_get(blen);
                memory_copy;
            });
        } else {
            wasm!(self.func, {
                // Copy original into [0..blen)
                local_get(dst); i32_const(4); i32_add;
                local_get(buf); i32_const(4); i32_add;
                local_get(blen);
                memory_copy;
                // Fill [blen..target) with val
                i32_const(0); local_set(i);
                block_empty; loop_empty;
                    local_get(i); local_get(pad); i32_ge_u; br_if(1);
                    local_get(dst); i32_const(4); i32_add; local_get(blen); i32_add; local_get(i); i32_add;
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
                local_get(dst); i32_const(4); i32_add; local_get(dst_off); i32_add;
                local_get(src); i32_const(4); i32_add; local_get(src_off); i32_add;
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

    /// `bytes.reverse(b) -> Bytes`. Allocates a fresh buffer.
    pub(super) fn emit_bytes_reverse(&mut self, args: &[IrExpr]) {
        let buf = self.scratch.alloc_i32();
        let len = self.scratch.alloc_i32();
        let dst = self.scratch.alloc_i32();
        let i = self.scratch.alloc_i32();
        self.emit_expr(&args[0]);
        wasm!(self.func, {
            local_set(buf);
            local_get(buf); i32_load(0); local_set(len);
            local_get(len); i32_const(4); i32_add; call(self.emitter.rt.alloc); local_set(dst);
            local_get(dst); local_get(len); i32_store(0);
            i32_const(0); local_set(i);
            block_empty; loop_empty;
                local_get(i); local_get(len); i32_ge_u; br_if(1);
                // dst[4 + i] = buf[4 + (len - 1 - i)]
                local_get(dst); i32_const(4); i32_add; local_get(i); i32_add;
                local_get(buf); i32_const(4); i32_add;
                local_get(len); i32_const(1); i32_sub; local_get(i); i32_sub; i32_add;
                i32_load8_u(0);
                i32_store8(0);
                local_get(i); i32_const(1); i32_add; local_set(i);
                br(0);
            end; end;
            local_get(dst);
        });
        self.scratch.free_i32(i);
        self.scratch.free_i32(dst);
        self.scratch.free_i32(len);
        self.scratch.free_i32(buf);
    }

    /// `bytes.fill(b, val)` — overwrite all bytes in place.
    pub(super) fn emit_bytes_fill(&mut self, args: &[IrExpr]) {
        let buf = self.scratch.alloc_i32();
        let val = self.scratch.alloc_i32();
        let len = self.scratch.alloc_i32();
        let i = self.scratch.alloc_i32();
        self.emit_expr(&args[0]);
        wasm!(self.func, { local_set(buf); });
        self.emit_expr(&args[1]);
        wasm!(self.func, {
            i32_wrap_i64; local_set(val);
            local_get(buf); i32_load(0); local_set(len);
            i32_const(0); local_set(i);
            block_empty; loop_empty;
                local_get(i); local_get(len); i32_ge_u; br_if(1);
                local_get(buf); i32_const(4); i32_add; local_get(i); i32_add;
                local_get(val); i32_store8(0);
                local_get(i); i32_const(1); i32_add; local_set(i);
                br(0);
            end; end;
        });
        self.scratch.free_i32(i);
        self.scratch.free_i32(len);
        self.scratch.free_i32(val);
        self.scratch.free_i32(buf);
    }

    /// `bytes.insert(b, pos, val) -> Bytes`. Returns a fresh buffer of length
    /// `len(b) + 1`. `pos` clamps to `[0, len(b)]`.
    pub(super) fn emit_bytes_insert(&mut self, args: &[IrExpr]) {
        let buf = self.scratch.alloc_i32();
        let pos = self.scratch.alloc_i32();
        let val = self.scratch.alloc_i32();
        let len = self.scratch.alloc_i32();
        let dst = self.scratch.alloc_i32();
        self.emit_expr(&args[0]); wasm!(self.func, { local_set(buf); });
        self.emit_expr(&args[1]); wasm!(self.func, { i32_wrap_i64; local_set(pos); });
        self.emit_expr(&args[2]); wasm!(self.func, {
            i32_wrap_i64; local_set(val);
            local_get(buf); i32_load(0); local_set(len);
            // Clamp pos to [0, len]
            local_get(pos); i32_const(0); i32_lt_s;
            if_empty; i32_const(0); local_set(pos); end;
            local_get(pos); local_get(len); i32_gt_u;
            if_empty; local_get(len); local_set(pos); end;
            // alloc len + 5
            local_get(len); i32_const(5); i32_add; call(self.emitter.rt.alloc); local_set(dst);
            local_get(dst); local_get(len); i32_const(1); i32_add; i32_store(0);
            // memcpy [0, pos)
            local_get(dst); i32_const(4); i32_add;
            local_get(buf); i32_const(4); i32_add;
            local_get(pos);
            memory_copy;
            // store val at dst+4+pos
            local_get(dst); i32_const(4); i32_add; local_get(pos); i32_add;
            local_get(val); i32_store8(0);
            // memcpy [pos, len)
            local_get(dst); i32_const(5); i32_add; local_get(pos); i32_add;
            local_get(buf); i32_const(4); i32_add; local_get(pos); i32_add;
            local_get(len); local_get(pos); i32_sub;
            memory_copy;
            local_get(dst);
        });
        self.scratch.free_i32(dst);
        self.scratch.free_i32(len);
        self.scratch.free_i32(val);
        self.scratch.free_i32(pos);
        self.scratch.free_i32(buf);
    }

    /// `bytes.remove_at(b, pos) -> Bytes`. Out-of-range returns clone.
    pub(super) fn emit_bytes_remove_at(&mut self, args: &[IrExpr]) {
        let buf = self.scratch.alloc_i32();
        let pos = self.scratch.alloc_i32();
        let len = self.scratch.alloc_i32();
        let dst = self.scratch.alloc_i32();
        self.emit_expr(&args[0]); wasm!(self.func, { local_set(buf); });
        self.emit_expr(&args[1]); wasm!(self.func, {
            i32_wrap_i64; local_set(pos);
            local_get(buf); i32_load(0); local_set(len);
            // If pos out of range → clone len+4 bytes
            local_get(pos); local_get(len); i32_ge_u;
            if_i32;
                local_get(len); i32_const(4); i32_add; call(self.emitter.rt.alloc); local_set(dst);
                local_get(dst); local_get(buf); local_get(len); i32_const(4); i32_add; memory_copy;
                local_get(dst);
            else_;
                local_get(len); i32_const(3); i32_add; call(self.emitter.rt.alloc); local_set(dst);
                local_get(dst); local_get(len); i32_const(1); i32_sub; i32_store(0);
                // memcpy [0, pos)
                local_get(dst); i32_const(4); i32_add;
                local_get(buf); i32_const(4); i32_add;
                local_get(pos);
                memory_copy;
                // memcpy [pos+1, len)
                local_get(dst); i32_const(4); i32_add; local_get(pos); i32_add;
                local_get(buf); i32_const(5); i32_add; local_get(pos); i32_add;
                local_get(len); local_get(pos); i32_sub; i32_const(1); i32_sub;
                memory_copy;
                local_get(dst);
            end;
        });
        self.scratch.free_i32(dst);
        self.scratch.free_i32(len);
        self.scratch.free_i32(pos);
        self.scratch.free_i32(buf);
    }

    /// `bytes.chunks(b, size) -> List[Bytes]`. Builds a fresh List with one
    /// fresh Bytes per chunk (last may be shorter).
    pub(super) fn emit_bytes_chunks(&mut self, args: &[IrExpr]) {
        let buf = self.scratch.alloc_i32();
        let size = self.scratch.alloc_i32();
        let len = self.scratch.alloc_i32();
        let n_chunks = self.scratch.alloc_i32();
        let result = self.scratch.alloc_i32();
        let i = self.scratch.alloc_i32();
        let off = self.scratch.alloc_i32();
        let chunk_len = self.scratch.alloc_i32();
        let chunk = self.scratch.alloc_i32();
        self.emit_expr(&args[0]); wasm!(self.func, { local_set(buf); });
        self.emit_expr(&args[1]); wasm!(self.func, {
            i32_wrap_i64; local_set(size);
            local_get(buf); i32_load(0); local_set(len);
            // n_chunks = ceil(len / size); if size == 0 → 0
            local_get(size); i32_eqz;
            if_i32; i32_const(0);
            else_;
                local_get(len); local_get(size); i32_add; i32_const(1); i32_sub;
                local_get(size); i32_div_u;
            end;
            local_set(n_chunks);
            // alloc List header: 4 + n_chunks*4
            local_get(n_chunks); i32_const(4); i32_mul; i32_const(4); i32_add;
            call(self.emitter.rt.alloc); local_set(result);
            local_get(result); local_get(n_chunks); i32_store(0);
            i32_const(0); local_set(i);
            i32_const(0); local_set(off);
            block_empty; loop_empty;
                local_get(i); local_get(n_chunks); i32_ge_u; br_if(1);
                // chunk_len = min(size, len - off)
                local_get(len); local_get(off); i32_sub;
                local_get(size); local_get(len); local_get(off); i32_sub; i32_lt_u;
                if_i32; local_get(size); else_; local_get(len); local_get(off); i32_sub; end;
                local_set(chunk_len);
                // alloc chunk: 4 + chunk_len
                local_get(chunk_len); i32_const(4); i32_add;
                call(self.emitter.rt.alloc); local_set(chunk);
                local_get(chunk); local_get(chunk_len); i32_store(0);
                local_get(chunk); i32_const(4); i32_add;
                local_get(buf); i32_const(4); i32_add; local_get(off); i32_add;
                local_get(chunk_len);
                memory_copy;
                // result.elems[i] = chunk
                local_get(result); i32_const(4); i32_add; local_get(i); i32_const(4); i32_mul; i32_add;
                local_get(chunk); i32_store(0);
                local_get(off); local_get(size); i32_add; local_set(off);
                local_get(i); i32_const(1); i32_add; local_set(i);
                br(0);
            end; end;
            local_get(result);
        });
        self.scratch.free_i32(chunk);
        self.scratch.free_i32(chunk_len);
        self.scratch.free_i32(off);
        self.scratch.free_i32(i);
        self.scratch.free_i32(result);
        self.scratch.free_i32(n_chunks);
        self.scratch.free_i32(len);
        self.scratch.free_i32(size);
        self.scratch.free_i32(buf);
    }

    /// `bytes.split(b, sep) -> List[Bytes]` and `bytes.lines(b) -> List[Bytes]`.
    /// Two-pass implementation: first count parts, then alloc List + chunks.
    /// `lf=true` uses a hardcoded `'\n'` separator (and ignores `sep` arg).
    pub(super) fn emit_bytes_split(&mut self, args: &[IrExpr], _single_byte: bool, lf: bool) {
        let buf = self.scratch.alloc_i32();
        let sep = self.scratch.alloc_i32();
        let blen = self.scratch.alloc_i32();
        let plen = self.scratch.alloc_i32();
        let count = self.scratch.alloc_i32();
        let i = self.scratch.alloc_i32();
        let j = self.scratch.alloc_i32();
        let start = self.scratch.alloc_i32();
        let result = self.scratch.alloc_i32();
        let chunk = self.scratch.alloc_i32();
        let chunk_len = self.scratch.alloc_i32();
        let out_idx = self.scratch.alloc_i32();
        self.emit_expr(&args[0]); wasm!(self.func, { local_set(buf); });
        if lf {
            // sep is implicit "\n" — alloc a 1-byte sep buffer at runtime.
            wasm!(self.func, {
                i32_const(5); call(self.emitter.rt.alloc); local_set(sep);
                local_get(sep); i32_const(1); i32_store(0);
                local_get(sep); i32_const(10); i32_store8(4);
            });
        } else {
            self.emit_expr(&args[1]);
            wasm!(self.func, { local_set(sep); });
        }
        wasm!(self.func, {
            local_get(buf); i32_load(0); local_set(blen);
            local_get(sep); i32_load(0); local_set(plen);
        });
        if lf {
            // For lines: count = number of '\n' bytes; trailing '\n' adds nothing.
            wasm!(self.func, {
                i32_const(0); local_set(count);
                i32_const(0); local_set(i);
                block_empty; loop_empty;
                    local_get(i); local_get(blen); i32_ge_u; br_if(1);
                    local_get(buf); i32_const(4); i32_add; local_get(i); i32_add;
                    i32_load8_u(0); i32_const(10); i32_eq;
                    if_empty; local_get(count); i32_const(1); i32_add; local_set(count); end;
                    local_get(i); i32_const(1); i32_add; local_set(i);
                    br(0);
                end; end;
                // If buffer doesn't end with newline, add 1 for the final line.
                local_get(blen); i32_eqz;
                if_empty;
                    // empty buffer → count stays 0
                else_;
                    local_get(buf); i32_const(4); i32_add; local_get(blen); i32_const(1); i32_sub; i32_add;
                    i32_load8_u(0); i32_const(10); i32_ne;
                    if_empty; local_get(count); i32_const(1); i32_add; local_set(count); end;
                end;
            });
        } else {
            // Generic split. Empty sep → 1 part (whole buffer).
            wasm!(self.func, {
                i32_const(1); local_set(count);
                local_get(plen); i32_eqz;
                if_empty;
                    // empty sep — count stays 1, skip scan
                else_;
                    i32_const(0); local_set(i);
                    block_empty; loop_empty;
                        local_get(i); local_get(plen); i32_add; local_get(blen); i32_gt_u; br_if(1);
                        // compare buf[i..i+plen] == sep[0..plen]; out_idx = match flag
                        i32_const(0); local_set(j);
                        i32_const(1); local_set(out_idx);
                        block_empty; loop_empty;
                            local_get(j); local_get(plen); i32_ge_u; br_if(1);
                            local_get(buf); i32_const(4); i32_add; local_get(i); i32_add; local_get(j); i32_add;
                            i32_load8_u(0);
                            local_get(sep); i32_const(4); i32_add; local_get(j); i32_add;
                            i32_load8_u(0);
                            i32_ne;
                            if_empty;
                                i32_const(0); local_set(out_idx); br(2);
                            end;
                            local_get(j); i32_const(1); i32_add; local_set(j);
                            br(0);
                        end; end;
                        local_get(out_idx);
                        if_empty;
                            local_get(count); i32_const(1); i32_add; local_set(count);
                            local_get(i); local_get(plen); i32_add; local_set(i);
                        else_;
                            local_get(i); i32_const(1); i32_add; local_set(i);
                        end;
                        br(0);
                    end; end;
                end;
            });
        }
        // Second pass: build the actual list using count chunks.
        wasm!(self.func, {
            local_get(count); i32_const(4); i32_mul; i32_const(4); i32_add;
            call(self.emitter.rt.alloc); local_set(result);
            local_get(result); local_get(count); i32_store(0);
            i32_const(0); local_set(start);
            i32_const(0); local_set(out_idx);
            i32_const(0); local_set(i);
            block_empty; loop_empty;
                local_get(out_idx); local_get(count); i32_ge_u; br_if(1);
                // Find next sep starting at i (or end).
                block_empty; loop_empty;
                    local_get(i); local_get(plen); i32_add; local_get(blen); i32_gt_u; br_if(1);
                    // compare buf[i..i+plen] == sep
                    i32_const(0); local_set(j);
                    i32_const(1); local_set(chunk_len); // reuse: match flag
                    block_empty; loop_empty;
                        local_get(j); local_get(plen); i32_ge_u; br_if(1);
                        local_get(buf); i32_const(4); i32_add; local_get(i); i32_add; local_get(j); i32_add;
                        i32_load8_u(0);
                        local_get(sep); i32_const(4); i32_add; local_get(j); i32_add;
                        i32_load8_u(0);
                        i32_ne; if_empty; i32_const(0); local_set(chunk_len); br(2); end;
                        local_get(j); i32_const(1); i32_add; local_set(j);
                        br(0);
                    end; end;
                    local_get(chunk_len); br_if(1); // matched → break inner
                    local_get(i); i32_const(1); i32_add; local_set(i);
                    br(0);
                end; end;
                // chunk = buf[start..i] (or buf[start..blen] when no further match)
                local_get(i); local_get(plen); i32_add; local_get(blen); i32_gt_u;
                if_empty; local_get(blen); local_set(i); end;
                local_get(i); local_get(start); i32_sub; local_set(chunk_len);
                local_get(chunk_len); i32_const(4); i32_add; call(self.emitter.rt.alloc); local_set(chunk);
                local_get(chunk); local_get(chunk_len); i32_store(0);
                local_get(chunk); i32_const(4); i32_add;
                local_get(buf); i32_const(4); i32_add; local_get(start); i32_add;
                local_get(chunk_len);
                memory_copy;
                local_get(result); i32_const(4); i32_add; local_get(out_idx); i32_const(4); i32_mul; i32_add;
                local_get(chunk); i32_store(0);
                local_get(out_idx); i32_const(1); i32_add; local_set(out_idx);
                local_get(i); local_get(plen); i32_add; local_set(i);
                local_get(i); local_set(start);
                br(0);
            end; end;
            local_get(result);
        });
        self.scratch.free_i32(out_idx);
        self.scratch.free_i32(chunk_len);
        self.scratch.free_i32(chunk);
        self.scratch.free_i32(result);
        self.scratch.free_i32(start);
        self.scratch.free_i32(j);
        self.scratch.free_i32(i);
        self.scratch.free_i32(count);
        self.scratch.free_i32(plen);
        self.scratch.free_i32(blen);
        self.scratch.free_i32(sep);
        self.scratch.free_i32(buf);
    }

    /// `bytes.starts_with` / `bytes.ends_with`. Both compare `pat` against a
    /// fixed-position window in `b` and return Bool. Result accumulated into
    /// a local to avoid block-result-type bookkeeping.
    pub(super) fn emit_bytes_prefix_match(&mut self, args: &[IrExpr], at_end: bool) {
        let b = self.scratch.alloc_i32();
        let pat = self.scratch.alloc_i32();
        let blen = self.scratch.alloc_i32();
        let plen = self.scratch.alloc_i32();
        let i = self.scratch.alloc_i32();
        let off = self.scratch.alloc_i32();
        let result = self.scratch.alloc_i32();
        self.emit_expr(&args[0]); wasm!(self.func, { local_set(b); });
        self.emit_expr(&args[1]); wasm!(self.func, {
            local_set(pat);
            local_get(b); i32_load(0); local_set(blen);
            local_get(pat); i32_load(0); local_set(plen);
            i32_const(1); local_set(result);
            // If pat longer than b → false and skip loop.
            local_get(plen); local_get(blen); i32_gt_u;
            if_empty;
                i32_const(0); local_set(result);
            else_;
        });
        if at_end {
            wasm!(self.func, {
                local_get(blen); local_get(plen); i32_sub; local_set(off);
            });
        } else {
            wasm!(self.func, { i32_const(0); local_set(off); });
        }
        wasm!(self.func, {
                i32_const(0); local_set(i);
                block_empty; loop_empty;
                    local_get(i); local_get(plen); i32_ge_u; br_if(1);
                    local_get(b); i32_const(4); i32_add; local_get(off); i32_add; local_get(i); i32_add;
                    i32_load8_u(0);
                    local_get(pat); i32_const(4); i32_add; local_get(i); i32_add;
                    i32_load8_u(0);
                    i32_ne;
                    if_empty;
                        i32_const(0); local_set(result); br(2);
                    end;
                    local_get(i); i32_const(1); i32_add; local_set(i);
                    br(0);
                end; end;
            end;
            local_get(result);
        });
        self.scratch.free_i32(result);
        self.scratch.free_i32(off);
        self.scratch.free_i32(i);
        self.scratch.free_i32(plen);
        self.scratch.free_i32(blen);
        self.scratch.free_i32(pat);
        self.scratch.free_i32(b);
    }

    /// Shared core for `contains` / `index_of`: returns i64 position
    /// (or -1 sentinel if not found) on the WASM stack.
    pub(super) fn emit_bytes_index_of_inner(&mut self, args: &[IrExpr]) {
        let b = self.scratch.alloc_i32();
        let pat = self.scratch.alloc_i32();
        let blen = self.scratch.alloc_i32();
        let plen = self.scratch.alloc_i32();
        let limit = self.scratch.alloc_i32(); // last valid start = blen - plen
        let i = self.scratch.alloc_i32();
        let j = self.scratch.alloc_i32();
        let result = self.scratch.alloc_i32();
        self.emit_expr(&args[0]); wasm!(self.func, { local_set(b); });
        self.emit_expr(&args[1]); wasm!(self.func, {
            local_set(pat);
            local_get(b); i32_load(0); local_set(blen);
            local_get(pat); i32_load(0); local_set(plen);
            i32_const(-1); local_set(result);
            // Empty pattern → 0
            local_get(plen); i32_eqz;
            if_empty;
                i32_const(0); local_set(result);
            else_;
                local_get(plen); local_get(blen); i32_gt_u;
                if_empty;
                    // result stays -1
                else_;
                    local_get(blen); local_get(plen); i32_sub; local_set(limit);
                    i32_const(0); local_set(i);
                    block_empty; loop_empty;
                        local_get(i); local_get(limit); i32_gt_u; br_if(1);
                        i32_const(0); local_set(j);
                        block_empty; loop_empty;
                            local_get(j); local_get(plen); i32_ge_u; br_if(1);
                            local_get(b); i32_const(4); i32_add; local_get(i); i32_add; local_get(j); i32_add;
                            i32_load8_u(0);
                            local_get(pat); i32_const(4); i32_add; local_get(j); i32_add;
                            i32_load8_u(0);
                            i32_ne; br_if(1);
                            local_get(j); i32_const(1); i32_add; local_set(j);
                            br(0);
                        end; end;
                        // If we reached j == plen, full match.
                        local_get(j); local_get(plen); i32_eq;
                        if_empty;
                            local_get(i); local_set(result);
                            br(3);
                        end;
                        local_get(i); i32_const(1); i32_add; local_set(i);
                        br(0);
                    end; end;
                end;
            end;
            local_get(result); i64_extend_i32_s;
        });
        self.scratch.free_i32(result);
        self.scratch.free_i32(j);
        self.scratch.free_i32(i);
        self.scratch.free_i32(limit);
        self.scratch.free_i32(plen);
        self.scratch.free_i32(blen);
        self.scratch.free_i32(pat);
        self.scratch.free_i32(b);
    }

    /// `bytes.cmp(a, b) -> Int` — byte-wise lexicographic comparison.
    pub(super) fn emit_bytes_cmp(&mut self, args: &[IrExpr]) {
        let a = self.scratch.alloc_i32();
        let b = self.scratch.alloc_i32();
        let alen = self.scratch.alloc_i32();
        let blen = self.scratch.alloc_i32();
        let minlen = self.scratch.alloc_i32();
        let i = self.scratch.alloc_i32();
        let av = self.scratch.alloc_i32();
        let bv = self.scratch.alloc_i32();
        let result = self.scratch.alloc_i32();
        self.emit_expr(&args[0]); wasm!(self.func, { local_set(a); });
        self.emit_expr(&args[1]); wasm!(self.func, {
            local_set(b);
            local_get(a); i32_load(0); local_set(alen);
            local_get(b); i32_load(0); local_set(blen);
            // minlen = min(alen, blen)
            local_get(alen); local_get(blen); i32_lt_u;
            if_i32; local_get(alen); else_; local_get(blen); end;
            local_set(minlen);
            i32_const(0); local_set(result);
            i32_const(0); local_set(i);
            block_empty; loop_empty;
                local_get(i); local_get(minlen); i32_ge_u; br_if(1);
                local_get(a); i32_const(4); i32_add; local_get(i); i32_add; i32_load8_u(0); local_set(av);
                local_get(b); i32_const(4); i32_add; local_get(i); i32_add; i32_load8_u(0); local_set(bv);
                local_get(av); local_get(bv); i32_lt_u;
                if_empty; i32_const(-1); local_set(result); br(2); end;
                local_get(av); local_get(bv); i32_gt_u;
                if_empty; i32_const(1); local_set(result); br(2); end;
                local_get(i); i32_const(1); i32_add; local_set(i);
                br(0);
            end; end;
            // All shared bytes equal → shorter is less.
            local_get(result); i32_eqz;
            if_empty;
                local_get(alen); local_get(blen); i32_lt_u;
                if_empty; i32_const(-1); local_set(result); end;
                local_get(alen); local_get(blen); i32_gt_u;
                if_empty; i32_const(1); local_set(result); end;
            end;
            local_get(result); i64_extend_i32_s;
        });
        self.scratch.free_i32(result);
        self.scratch.free_i32(bv);
        self.scratch.free_i32(av);
        self.scratch.free_i32(i);
        self.scratch.free_i32(minlen);
        self.scratch.free_i32(blen);
        self.scratch.free_i32(alen);
        self.scratch.free_i32(b);
        self.scratch.free_i32(a);
    }

    /// `bytes.is_valid_utf8(b) -> Bool`. Shape-validates UTF-8 (catches invalid
    /// lead/follow bytes and short sequences; does not flag overlong forms or
    /// surrogates). Sufficient to reject obvious garbage like `0xFF` and to
    /// accept all well-formed Unicode strings.
    pub(super) fn emit_bytes_is_valid_utf8(&mut self, args: &[IrExpr]) {
        let buf = self.scratch.alloc_i32();
        let len = self.scratch.alloc_i32();
        let i = self.scratch.alloc_i32();
        let b = self.scratch.alloc_i32();
        let need = self.scratch.alloc_i32();
        let valid = self.scratch.alloc_i32();
        let k = self.scratch.alloc_i32();
        let fb = self.scratch.alloc_i32();
        self.emit_expr(&args[0]);
        wasm!(self.func, {
            local_set(buf);
            local_get(buf); i32_load(0); local_set(len);
            i32_const(0); local_set(i);
            i32_const(1); local_set(valid);
            block_empty; loop_empty;
                local_get(i); local_get(len); i32_ge_u; br_if(1);
                local_get(buf); i32_const(4); i32_add; local_get(i); i32_add; i32_load8_u(0); local_set(b);
                // ASCII fast path
                local_get(b); i32_const(128); i32_lt_u;
                if_empty;
                    local_get(i); i32_const(1); i32_add; local_set(i);
                    br(2); // continue outer loop
                end;
                // Determine number of follow-bytes
                local_get(b); i32_const(0xC2); i32_lt_u;
                if_empty;
                    i32_const(0); local_set(valid); br(2);
                end;
                local_get(b); i32_const(0xE0); i32_lt_u;
                if_i32; i32_const(1); else_;
                  local_get(b); i32_const(0xF0); i32_lt_u;
                  if_i32; i32_const(2); else_;
                    local_get(b); i32_const(0xF5); i32_lt_u;
                    if_i32; i32_const(3); else_; i32_const(-1); end;
                  end;
                end;
                local_set(need);
                // need == -1 → invalid
                local_get(need); i32_const(-1); i32_eq;
                if_empty;
                    i32_const(0); local_set(valid); br(2);
                end;
                // Bounds: i + 1 + need > len → invalid
                local_get(i); i32_const(1); i32_add; local_get(need); i32_add; local_get(len); i32_gt_u;
                if_empty;
                    i32_const(0); local_set(valid); br(2);
                end;
                // Walk follow-bytes
                i32_const(0); local_set(k);
                block_empty; loop_empty;
                    local_get(k); local_get(need); i32_ge_u; br_if(1);
                    local_get(buf); i32_const(5); i32_add;
                    local_get(i); i32_add; local_get(k); i32_add;
                    i32_load8_u(0); local_set(fb);
                    local_get(fb); i32_const(0x80); i32_lt_u;
                    local_get(fb); i32_const(0xC0); i32_ge_u; i32_or;
                    if_empty;
                        i32_const(0); local_set(valid); br(4);
                    end;
                    local_get(k); i32_const(1); i32_add; local_set(k);
                    br(0);
                end; end;
                local_get(i); i32_const(1); i32_add; local_get(need); i32_add; local_set(i);
                br(0);
            end; end;
            local_get(valid);
        });
        self.scratch.free_i32(fb);
        self.scratch.free_i32(k);
        self.scratch.free_i32(valid);
        self.scratch.free_i32(need);
        self.scratch.free_i32(b);
        self.scratch.free_i32(i);
        self.scratch.free_i32(len);
        self.scratch.free_i32(buf);
    }

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
                    local_get(buf); i32_const(4); i32_add; local_get(pos_i32); i32_add;
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
                local_get(buf); i32_const(4); i32_add; local_get(pos_i32); i32_add;
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
                    local_get(buf); i32_const(4); i32_add; local_get(pos_i32); i32_add;
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
                local_get(buf); i32_const(4); i32_add; local_get(pos_i32); i32_add;
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
              local_get(n_i32); i32_const(4); i32_add;
              call(self.emitter.rt.alloc); local_set(dst);
              local_get(dst); local_get(n_i32); i32_store(0);
              // memcpy data
              local_get(dst); i32_const(4); i32_add;
              local_get(buf); i32_const(4); i32_add; local_get(pos_i32); i32_add;
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
            local_get(old_len); i32_const(4 + size_bytes as i32); i32_add;
            call(self.emitter.rt.alloc); local_set(new_buf);
            local_get(new_buf); local_get(old_len); i32_const(size_bytes as i32); i32_add; i32_store(0);
            local_get(new_buf); i32_const(4); i32_add;
            local_get(buf); i32_const(4); i32_add;
            local_get(old_len);
            memory_copy;
        });

        // Store destination = new_buf + 4 + old_len.
        wasm!(self.func, { local_get(endian_tag); i32_eqz; if_empty; });
        self.emit_typed_append_store(new_buf, old_len, val_i64, val_f64, size_bytes, is_float, /*be=*/ false);
        wasm!(self.func, { else_; });
        self.emit_typed_append_store(new_buf, old_len, val_i64, val_f64, size_bytes, is_float, /*be=*/ true);
        wasm!(self.func, { end; });

        if let almide_ir::IrExprKind::Var { id } = &buf_expr.kind {
            if let Some(&local_idx) = self.var_map.get(&id.0) {
                wasm!(self.func, { local_get(new_buf); local_set(local_idx); });
            }
        }

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
            local_get(new_buf); i32_const(4); i32_add; local_get(old_len); i32_add;
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
            local_get(buf); i32_const(4); i32_add; local_get(offset); i32_add;
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

/// `true` when the typed byte-IO value arg carries a WASM `i32` runtime
/// representation (Almide `Int8` / `Int16` / `Int32` / `UInt8` /
/// `UInt16` / `UInt32`). The inner append/set emitters evaluate the
/// value as a canonical-width `Int` (`i64`), so callers of this helper
/// insert an `i64_extend_i32_u` / `_s` after `emit_expr` to bridge the
/// width.
fn is_sized_i32_val(expr: &IrExpr) -> bool {
    use almide_lang::types::Ty;
    matches!(expr.ty, Ty::Int8 | Ty::Int16 | Ty::Int32
        | Ty::UInt8 | Ty::UInt16 | Ty::UInt32)
}

/// `true` when the typed byte-IO value arg carries a WASM `f32` runtime
/// representation (Almide `Float32`). Inner emitters expect `f64`, so
/// callers insert `f64_promote_f32` after `emit_expr`.
fn is_sized_f32_val(expr: &IrExpr) -> bool {
    use almide_lang::types::Ty;
    matches!(expr.ty, Ty::Float32)
}
