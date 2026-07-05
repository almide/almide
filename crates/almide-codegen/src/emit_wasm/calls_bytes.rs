//! Bytes stdlib call dispatch for WASM codegen.
//!
//! Memory layout: [len:i32][data:u8...]  (same as String)

use super::FuncCompiler;
use almide_ir::{IrExpr, IrExprKind};

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
    ///
    /// The arm set is partitioned into three disjoint groups (every method
    /// string matches at most one). Group 1 lives here; groups 2 and 3 are
    /// chained sub-matches in `calls_bytes_p2.rs`. Chain order is irrelevant
    /// because the groups never overlap.
    pub(super) fn emit_bytes_call(&mut self, method: &str, args: &[IrExpr]) -> bool {
        if self.emit_bytes_call_g2(method, args) {
            return true;
        }
        if self.emit_bytes_call_g3(method, args) {
            return true;
        }
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
                      local_get(buf); i32_const(self.emitter.layout_reg.fixed_offset(super::engine::layout::STRING, super::engine::layout::string::DATA) as i32); i32_add;
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
                      local_get(buf); i32_const(self.emitter.layout_reg.fixed_offset(super::engine::layout::STRING, super::engine::layout::string::DATA) as i32); i32_add;
                      local_get(idx); i32_add;
                      i32_load8_u(0);
                      i64_extend_i32_u;
                    end;
                });
                self.scratch.free_i32(idx);
                self.scratch.free_i32(buf);
            }
            "set" => {
                // bytes.set(b, i, val) → Bytes. ORACLE-pure: the native runtime
                // CLONES and returns a new Vec (value semantics), so a shared
                // input — a named Var (locals may alias, params alias the
                // caller's value) or an alias-shaped expr — must be cloned
                // before the store, or the mutation is observable through the
                // other binding (aes cfb8: `encrypt_block(iv, …)` clobbered the
                // caller's loop iv). The in-place fast path lives ONLY in the
                // `x = bytes.set(x, …)` Assign peephole (statements.rs) for
                // provably-unaliased targets. A non-Var, non-alias arg is a
                // fresh temp this call uniquely owns — no clone needed.
                let buf = self.scratch.alloc_i32();
                let idx = self.scratch.alloc_i32();
                let val = self.scratch.alloc_i32();
                self.emit_expr(&args[0]);
                if matches!(&args[0].kind, IrExprKind::Var { .. })
                    || crate::pass_perceus::yields_borrowed_alias(&args[0])
                {
                    wasm!(self.func, { call(self.emitter.rt.cow_check); });
                }
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
                      local_get(buf); i32_const(self.emitter.layout_reg.fixed_offset(super::engine::layout::STRING, super::engine::layout::string::DATA) as i32); i32_add; local_get(idx); i32_add;
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
            "set_at" => {
                // bytes.set_at(b, i, val) -> Unit: in-place index write, no realloc.
                // Same store as `set`, but returns Unit so nothing is left on the stack.
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
                      local_get(buf); i32_const(self.emitter.layout_reg.fixed_offset(super::engine::layout::STRING, super::engine::layout::string::DATA) as i32); i32_add; local_get(idx); i32_add;
                      local_get(val);
                      i32_store8(0);
                    end;
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
                    local_get(n); i32_const(self.emitter.layout_reg.header_size(super::engine::layout::STRING) as i32); i32_add;
                    call(self.emitter.rt.alloc);
                    local_set(ptr);
                    // store length + cap
                    local_get(ptr); local_get(n); i32_store(0);
                    local_get(ptr); local_get(n); i32_store(self.emitter.layout_reg.fixed_offset(super::engine::layout::STRING, super::engine::layout::string::CAP));
                    // zero the data region
                    local_get(ptr); i32_const(self.emitter.layout_reg.fixed_offset(super::engine::layout::STRING, super::engine::layout::string::DATA) as i32); i32_add;
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
                    local_get(len); i32_const(self.emitter.layout_reg.header_size(super::engine::layout::STRING) as i32); i32_add;
                    call(self.emitter.rt.alloc);
                    local_set(dst);
                    local_get(dst); local_get(len); i32_store(0);
                    // loop: copy each i64 as u8
                    i32_const(0); local_set(i);
                    block_empty; loop_empty;
                      local_get(i); local_get(len); i32_ge_u; br_if(1);
                      // dst_byte_addr = dst + 4 + i
                      local_get(dst); i32_const(self.emitter.layout_reg.fixed_offset(super::engine::layout::STRING, super::engine::layout::string::DATA) as i32); i32_add; local_get(i); i32_add;
                      // src_elem = xs + 4 + i*8 → load i64, wrap to i32, store as u8
                      local_get(xs); i32_const(self.emitter.layout_reg.fixed_offset(super::engine::layout::STRING, super::engine::layout::string::DATA) as i32); i32_add;
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
                    local_get(len); i32_const(8); i32_mul; i32_const(self.emitter.layout_reg.header_size(super::engine::layout::STRING) as i32); i32_add;
                    call(self.emitter.rt.alloc);
                    local_set(dst);
                    local_get(dst); local_get(len); i32_store(0);
                    // loop
                    i32_const(0); local_set(i);
                    block_empty; loop_empty;
                      local_get(i); local_get(len); i32_ge_u; br_if(1);
                      // dst + 4 + i*8
                      local_get(dst); i32_const(self.emitter.layout_reg.fixed_offset(super::engine::layout::STRING, super::engine::layout::string::DATA) as i32); i32_add;
                      local_get(i); i32_const(8); i32_mul; i32_add;
                      // load u8 from src + 4 + i, extend to i64
                      local_get(src); i32_const(self.emitter.layout_reg.fixed_offset(super::engine::layout::STRING, super::engine::layout::string::DATA) as i32); i32_add;
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
                    local_get(new_len); i32_const(self.emitter.layout_reg.header_size(super::engine::layout::STRING) as i32); i32_add;
                    call(self.emitter.rt.alloc);
                    local_set(dst);
                    local_get(dst); local_get(new_len); i32_store(0);
                    // memory.copy(dst+4, src+4+s, new_len)
                    local_get(dst); i32_const(self.emitter.layout_reg.fixed_offset(super::engine::layout::STRING, super::engine::layout::string::DATA) as i32); i32_add;
                    local_get(src); i32_const(self.emitter.layout_reg.fixed_offset(super::engine::layout::STRING, super::engine::layout::string::DATA) as i32); i32_add; local_get(s); i32_add;
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
                    local_get(len_a); local_get(len_b); i32_add; i32_const(self.emitter.layout_reg.header_size(super::engine::layout::STRING) as i32); i32_add;
                    call(self.emitter.rt.alloc);
                    local_set(dst);
                    // store total length
                    local_get(dst);
                    local_get(len_a); local_get(len_b); i32_add;
                    i32_store(0);
                    // copy a data
                    local_get(dst); i32_const(self.emitter.layout_reg.fixed_offset(super::engine::layout::STRING, super::engine::layout::string::DATA) as i32); i32_add;
                    local_get(a); i32_const(self.emitter.layout_reg.fixed_offset(super::engine::layout::STRING, super::engine::layout::string::DATA) as i32); i32_add;
                    local_get(len_a);
                    memory_copy;
                    // copy b data
                    local_get(dst); i32_const(self.emitter.layout_reg.fixed_offset(super::engine::layout::STRING, super::engine::layout::string::DATA) as i32); i32_add; local_get(len_a); i32_add;
                    local_get(b); i32_const(self.emitter.layout_reg.fixed_offset(super::engine::layout::STRING, super::engine::layout::string::DATA) as i32); i32_add;
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
                    local_get(total); i32_const(self.emitter.layout_reg.header_size(super::engine::layout::STRING) as i32); i32_add;
                    call(self.emitter.rt.alloc);
                    local_set(dst);
                    local_get(dst); local_get(total); i32_store(0);
                    // loop: copy src data n times
                    i32_const(0); local_set(i);
                    block_empty; loop_empty;
                      local_get(i); local_get(n); i32_ge_u; br_if(1);
                      // dst + 4 + i*src_len
                      local_get(dst); i32_const(self.emitter.layout_reg.fixed_offset(super::engine::layout::STRING, super::engine::layout::string::DATA) as i32); i32_add;
                      local_get(i); local_get(src_len); i32_mul; i32_add;
                      local_get(src); i32_const(self.emitter.layout_reg.fixed_offset(super::engine::layout::STRING, super::engine::layout::string::DATA) as i32); i32_add;
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
                    // new_buf = alloc(hdr + old_len + 1)
                    local_get(old_len); i32_const(self.emitter.layout_reg.header_size(super::engine::layout::STRING) as i32 + 1); i32_add;
                    call(self.emitter.rt.alloc);
                    local_set(new_buf);
                    // new_buf.len = old_len + 1, new_buf.cap = same
                    local_get(new_buf); local_get(old_len); i32_const(1); i32_add; i32_store(0);
                    local_get(new_buf); local_get(old_len); i32_const(1); i32_add; i32_store(self.emitter.layout_reg.fixed_offset(super::engine::layout::STRING, super::engine::layout::string::CAP));
                    // copy old data: new_buf+4 <- buf+4, old_len bytes
                    local_get(new_buf); i32_const(self.emitter.layout_reg.fixed_offset(super::engine::layout::STRING, super::engine::layout::string::DATA) as i32); i32_add;
                    local_get(buf); i32_const(self.emitter.layout_reg.fixed_offset(super::engine::layout::STRING, super::engine::layout::string::DATA) as i32); i32_add;
                    local_get(old_len);
                    memory_copy;
                    // new_buf[4 + old_len] = val
                    local_get(new_buf); i32_const(self.emitter.layout_reg.fixed_offset(super::engine::layout::STRING, super::engine::layout::string::DATA) as i32); i32_add; local_get(old_len); i32_add;
                    local_get(val); i32_store8(0);
                });
                // Update the variable: need to store new_buf back
                // The buf variable is the first arg — if it's a Var, update the local
                self.emit_mutator_writeback(&args[0], new_buf);
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
                    local_get(new_buf); i32_const(self.emitter.layout_reg.fixed_offset(super::engine::layout::STRING, super::engine::layout::string::DATA) as i32); i32_add;
                    local_get(buf); i32_const(self.emitter.layout_reg.fixed_offset(super::engine::layout::STRING, super::engine::layout::string::DATA) as i32); i32_add;
                    local_get(old_len);
                    memory_copy;
                    // *(new_buf + 4 + old_len) = fval (f64 LE)
                    local_get(new_buf); i32_const(self.emitter.layout_reg.fixed_offset(super::engine::layout::STRING, super::engine::layout::string::DATA) as i32); i32_add; local_get(old_len); i32_add;
                    local_get(fval);
                    f64_store(0);
                });
                self.emit_mutator_writeback(&args[0], new_buf);
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
            "heap_save" => {
                // bytes.heap_save() -> Int: call __heap_save, extend i32→i64
                wasm!(self.func, { call(self.emitter.rt.heap_save); i64_extend_i32_u; });
            }
            "heap_restore" => {
                // bytes.heap_restore(checkpoint: Int): wrap i64→i32, call __heap_restore
                self.emit_expr(&args[0]);
                wasm!(self.func, { i32_wrap_i64; call(self.emitter.rt.heap_restore); });
            }
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
                // bytes.from_string(s): COPY into an independent Bytes buffer (#690).
                // A zero-copy cast (returning the String pointer) aliases the source
                // String's RC-managed buffer — the String's scope-end Dec then frees
                // a buffer the Bytes still points at (later bytes.len/get reads freed
                // memory). Bytes/String share the [len][cap][data@8] layout; copy len.
                let src = self.scratch.alloc_i32();
                let len = self.scratch.alloc_i32();
                let dst = self.scratch.alloc_i32();
                self.emit_expr(&args[0]);
                let data_off = self.emitter.layout_reg.fixed_offset(super::engine::layout::STRING, super::engine::layout::string::DATA) as i32;
                let cap_off = self.emitter.layout_reg.fixed_offset(super::engine::layout::STRING, super::engine::layout::string::CAP);
                let hdr = self.emitter.layout_reg.header_size(super::engine::layout::STRING) as i32;
                wasm!(self.func, {
                    local_set(src);
                    local_get(src); i32_load(0); local_set(len);
                    local_get(len); i32_const(hdr); i32_add;
                    call(self.emitter.rt.alloc);
                    local_set(dst);
                    local_get(dst); local_get(len); i32_store(0);
                    local_get(dst); local_get(len); i32_store(cap_off);
                    local_get(dst); i32_const(data_off); i32_add;
                    local_get(src); i32_const(data_off); i32_add;
                    local_get(len);
                    memory_copy;
                    local_get(dst);
                });
                self.scratch.free_i32(src);
                self.scratch.free_i32(len);
                self.scratch.free_i32(dst);
            }
            "to_string_lossy" => {
                // COPY into an independent String buffer (not a cast). A zero-copy cast
                // aliases the source Bytes' buffer; the result String's RC dec then frees
                // a buffer the Bytes still points at (#690, reverse direction). Copy len.
                // (WASM does not validate UTF-8; invalid sequences pass through unchanged.)
                let src = self.scratch.alloc_i32();
                let len = self.scratch.alloc_i32();
                let dst = self.scratch.alloc_i32();
                self.emit_expr(&args[0]);
                let data_off = self.emitter.layout_reg.fixed_offset(super::engine::layout::STRING, super::engine::layout::string::DATA) as i32;
                let cap_off = self.emitter.layout_reg.fixed_offset(super::engine::layout::STRING, super::engine::layout::string::CAP);
                let hdr = self.emitter.layout_reg.header_size(super::engine::layout::STRING) as i32;
                wasm!(self.func, {
                    local_set(src);
                    local_get(src); i32_load(0); local_set(len);
                    local_get(len); i32_const(hdr); i32_add;
                    call(self.emitter.rt.alloc);
                    local_set(dst);
                    local_get(dst); local_get(len); i32_store(0);
                    local_get(dst); local_get(len); i32_store(cap_off);
                    local_get(dst); i32_const(data_off); i32_add;
                    local_get(src); i32_const(data_off); i32_add;
                    local_get(len);
                    memory_copy;
                    local_get(dst);
                });
                self.scratch.free_i32(src);
                self.scratch.free_i32(len);
                self.scratch.free_i32(dst);
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
                    span: args[0].span, def_id: None,
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
                    local_get(buf); i32_const(self.emitter.layout_reg.fixed_offset(super::engine::layout::STRING, super::engine::layout::string::DATA) as i32); i32_add; i32_add;
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
                    local_get(buf); i32_const(self.emitter.layout_reg.fixed_offset(super::engine::layout::STRING, super::engine::layout::string::DATA) as i32); i32_add; local_get(pos); i32_add;
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
            _ => return false,
        }
        true
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

// `emit_bytes_call` groups 2/3 (chained sub-matches) + the ~50 sibling
// byte-IO emitters, split out to keep every file under the 1000-line limit.
// Each part re-opens `impl FuncCompiler<'_>` and shares this module's scope
// (FuncCompiler, IrExpr, ByteReadOp, the `wasm!` macro).
include!("calls_bytes_p2.rs");
include!("calls_bytes_p3.rs");
include!("calls_bytes_p4.rs");
include!("calls_bytes_p5.rs");
