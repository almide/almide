//! Bytes stdlib call dispatch for WASM codegen.
//!
//! Memory layout: [len:i32][data:u8...]  (same as String)

use super::FuncCompiler;
use crate::ir::IrExpr;

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
            _ => return false,
        }
        true
    }
}
