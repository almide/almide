//! Bytes stdlib call dispatch for WASM codegen.
//!
//! Memory layout: [len:i32][data:u8...]  (same as String)

use super::FuncCompiler;
use almide_ir::IrExpr;

/// Requested primitive load for the typed byte-read family.
#[derive(Clone, Copy)]
enum ByteReadOp {
    U8,
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
            "from_string" => {
                // bytes.from_string(s): String and Bytes have same layout [len:i32][data:u8...]
                // Just return the string pointer (effectively a cast)
                self.emit_expr(&args[0]);
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
            "set_u32_le" => self.emit_bytes_set_i(args, 4),
            "set_i32_le" => self.emit_bytes_set_i(args, 4),
            "set_i64_le" => self.emit_bytes_set_i(args, 8),
            "set_f64_le" => self.emit_bytes_set_f(args, /*size_bytes=*/8, /*as_f32=*/false),
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
            "read_u32_be" => self.emit_byte_read_be_int(&args[0], &args[1], 4, /*signed=*/false),
            "read_i32_be" => self.emit_byte_read_be_int(&args[0], &args[1], 4, true),
            "read_i64_be" => self.emit_byte_read_be_int(&args[0], &args[1], 8, true),
            "read_f32_be" => self.emit_byte_read_be_float(&args[0], &args[1], 4),
            "read_f64_be" => self.emit_byte_read_be_float(&args[0], &args[1], 8),
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
            "read_i32_le_array" | "read_u32_le_array" | "read_i64_le_array"
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
                    "read_f16_le_array" => 2,
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
                if is_be {
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
}
