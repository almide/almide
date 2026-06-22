//! List stdlib call dispatch for WASM codegen (non-closure functions).

use super::FuncCompiler;
use super::values;
use almide_ir::IrExpr;
use almide_lang::types::Ty;
use wasm_encoder::ValType;

impl FuncCompiler<'_> {
    /// Dispatch a list stdlib method call (non-closure). Returns true if handled.
    pub(super) fn emit_list_call(&mut self, method: &str, args: &[IrExpr]) -> bool {
        use super::engine::layout::{LIST, STRING, list as ll, string as ls};
        let list_data_off = self.emitter.layout_reg.fixed_offset(LIST, ll::DATA) as i32;
        let list_hdr = self.emitter.layout_reg.header_size(LIST) as i32;
        let str_data_off = self.emitter.layout_reg.fixed_offset(STRING, ls::DATA) as i32;
        let str_hdr = self.emitter.layout_reg.header_size(STRING) as i32;
        let str_cap_off = self.emitter.layout_reg.fixed_offset(STRING, ls::CAP) as i32;
        // Disjoint sub-dispatch groups (split for file-size; chain order is
        // irrelevant since every method-string matches exactly one group).
        if self.emit_list_call_g2(method, args) {
            return true;
        }
        if self.emit_list_call_g3(method, args) {
            return true;
        }
        match method {
            "len" | "length" => {
                self.emit_expr(&args[0]);
                wasm!(self.func, { i32_load(0); i64_extend_i32_u; });
            }
            "get_or" => {
                // get_or(xs, i, default) → A
                let elem_ty = self.resolve_list_elem(&args[0], None);
                let elem_size = values::byte_size(&elem_ty);
                let vt = values::ty_to_valtype(&elem_ty).unwrap_or(ValType::I32);
                let xs = self.scratch.alloc_i32();
                let idx = self.scratch.alloc_i32();
                let in_bounds = self.scratch.alloc_i32();
                let len = self.scratch.alloc_i32();
                // Store xs, len, i
                self.emit_expr(&args[0]);
                wasm!(self.func, { local_set(xs); local_get(xs); i32_load(0); local_set(len); });
                self.emit_expr(&args[1]); // i: i64
                // idx = min_u(i, len); in_bounds = (i_u < len) on the full i64,
                // so a negative OR a >= 2^32 index returns the default rather
                // than wrapping to a small in-range slot (C-054). The unsigned
                // test folds both `i < 0` and `i >= len` into one comparison.
                self.emit_checked_index_i32(len, in_bounds);
                wasm!(self.func, { local_set(idx); });
                // in_bounds ? xs[idx] : default
                let load_at = |this: &mut Self| {
                    wasm!(this.func, {
                        local_get(xs); i32_const(list_data_off); i32_add;
                        local_get(idx); i32_const(elem_size as i32); i32_mul; i32_add;
                    });
                };
                match vt {
                    ValType::I64 => {
                        wasm!(self.func, { local_get(in_bounds); if_i64; });
                        load_at(self);
                        wasm!(self.func, { i64_load(0); else_; });
                        self.emit_expr(&args[2]);
                        wasm!(self.func, { end; });
                    }
                    ValType::F64 => {
                        wasm!(self.func, { local_get(in_bounds); if_f64; });
                        load_at(self);
                        wasm!(self.func, { f64_load(0); else_; });
                        self.emit_expr(&args[2]);
                        wasm!(self.func, { end; });
                    }
                    _ => {
                        wasm!(self.func, { local_get(in_bounds); if_i32; });
                        load_at(self);
                        wasm!(self.func, { i32_load(0); else_; });
                        self.emit_expr(&args[2]);
                        wasm!(self.func, { end; });
                    }
                }
                self.scratch.free_i32(len);
                self.scratch.free_i32(in_bounds);
                self.scratch.free_i32(idx);
                self.scratch.free_i32(xs);
            }
            "take" => {
                // take(xs, n) → List[A]: first min(n, len) elements
                let elem_ty = self.resolve_list_elem(&args[0], None);
                let es = values::byte_size(&elem_ty) as i32;
                let xs = self.scratch.alloc_i32();
                let len = self.scratch.alloc_i32();
                let new_len = self.scratch.alloc_i32();
                let dst = self.scratch.alloc_i32();
                let i = self.scratch.alloc_i32();
                self.emit_expr(&args[0]);
                wasm!(self.func, { local_set(xs); local_get(xs); i32_load(0); local_set(len); });
                self.emit_expr(&args[1]);
                // new_len = min_u(n, len): take's UNSIGNED `min(n as usize, len)`
                // IS the clamp, computed on the i64 count before narrowing
                // (C-054). A negative `n` (huge as usize) saturates to len →
                // whole list, matching native `take(n as usize)`.
                self.emit_clamp_count_to_i32(super::calls_list_helpers::ClampHi::LenLocal(len));
                wasm!(self.func, {
                    local_set(new_len);
                    i32_const(list_hdr); local_get(new_len); i32_const(es); i32_mul; i32_add;
                    call(self.emitter.rt.alloc); local_set(dst);
                    local_get(dst); local_get(new_len); i32_store(0);
                    i32_const(0); local_set(i);
                    block_empty; loop_empty;
                      local_get(i); local_get(new_len); i32_ge_u; br_if(1);
                      local_get(dst); i32_const(list_data_off); i32_add;
                      local_get(i); i32_const(es); i32_mul; i32_add;
                      local_get(xs); i32_const(list_data_off); i32_add;
                      local_get(i); i32_const(es); i32_mul; i32_add;
                });
                self.emit_elem_copy_owned(&elem_ty);
                wasm!(self.func, {
                      local_get(i); i32_const(1); i32_add; local_set(i);
                      br(0);
                    end; end;
                    local_get(dst);
                });
                self.scratch.free_i32(i);
                self.scratch.free_i32(dst);
                self.scratch.free_i32(new_len);
                self.scratch.free_i32(len);
                self.scratch.free_i32(xs);
            }
            "drop" => {
                // drop(xs, n): skip first n
                let elem_ty = self.resolve_list_elem(&args[0], None);
                let es = values::byte_size(&elem_ty) as i32;
                let xs = self.scratch.alloc_i32();
                let len = self.scratch.alloc_i32();
                let start = self.scratch.alloc_i32();
                let new_len = self.scratch.alloc_i32();
                let dst = self.scratch.alloc_i32();
                let i = self.scratch.alloc_i32();
                self.emit_expr(&args[0]);
                wasm!(self.func, { local_set(xs); local_get(xs); i32_load(0); local_set(len); });
                self.emit_expr(&args[1]);
                // start = min_u(n, len) on the i64 count (C-054); then
                // new_len = len - start can never underflow. A negative `n`
                // (huge as usize) saturates to len → drops everything (empty),
                // matching native `skip(n as usize)`.
                self.emit_clamp_count_to_i32(super::calls_list_helpers::ClampHi::LenLocal(len));
                wasm!(self.func, {
                    local_set(start);
                    // new_len = len - start
                    local_get(len); local_get(start); i32_sub;
                    local_set(new_len);
                    i32_const(list_hdr); local_get(new_len); i32_const(es); i32_mul; i32_add;
                    call(self.emitter.rt.alloc); local_set(dst);
                    local_get(dst); local_get(new_len); i32_store(0);
                    i32_const(0); local_set(i);
                    block_empty; loop_empty;
                      local_get(i); local_get(new_len); i32_ge_u; br_if(1);
                      local_get(dst); i32_const(list_data_off); i32_add;
                      local_get(i); i32_const(es); i32_mul; i32_add;
                      local_get(xs); i32_const(list_data_off); i32_add;
                      local_get(start); local_get(i); i32_add;
                      i32_const(es); i32_mul; i32_add;
                });
                self.emit_elem_copy_owned(&elem_ty);
                wasm!(self.func, {
                      local_get(i); i32_const(1); i32_add; local_set(i);
                      br(0);
                    end; end;
                    local_get(dst);
                });
                self.scratch.free_i32(i);
                self.scratch.free_i32(dst);
                self.scratch.free_i32(new_len);
                self.scratch.free_i32(start);
                self.scratch.free_i32(len);
                self.scratch.free_i32(xs);
            }
            "slice" => {
                // slice(xs, start, end) → List[A]
                let elem_ty = self.resolve_list_elem(&args[0], None);
                let es = values::byte_size(&elem_ty) as i32;
                let xs = self.scratch.alloc_i32();
                let start = self.scratch.alloc_i32();
                let end = self.scratch.alloc_i32();
                let new_len = self.scratch.alloc_i32();
                let dst = self.scratch.alloc_i32();
                let len = self.scratch.alloc_i32();
                self.emit_expr(&args[0]);
                wasm!(self.func, {
                    local_set(xs);
                    local_get(xs); i32_load(0); local_set(len);
                });
                // Saturate start/end to [0, len] on the i64 index BEFORE
                // narrowing (C-054), UNSIGNED — native `list.slice` is
                // `s = start as usize; e = (end as usize).min(len)`, so a
                // negative/huge index is enormous as usize and `min_u(_, len)`
                // sends it to len (then the `if end > start` guard empties). A
                // bare i32_wrap_i64 was wrong for indices >= 2^32 (they wrap to a
                // small IN-range index). NOTE this is UNSIGNED, unlike
                // `string.slice` whose oracle clamps SIGNED (`start.max(0)`).
                //   s = min_u(start,len); e = min_u(end,len);
                //   if s >= e { [] } else { xs[s..e] }
                self.emit_expr(&args[1]); // start (i64)
                self.emit_clamp_count_to_i32(super::calls_list_helpers::ClampHi::LenLocal(len));
                wasm!(self.func, { local_set(start); });
                self.emit_expr(&args[2]); // end (i64)
                self.emit_clamp_count_to_i32(super::calls_list_helpers::ClampHi::LenLocal(len));
                wasm!(self.func, {
                    local_set(end);
                    // new_len = (end > start) ? end - start : 0
                    local_get(end); local_get(start); i32_sub;
                      i32_const(0);
                      local_get(end); local_get(start); i32_gt_u; select;
                    local_set(new_len);
                    i32_const(list_hdr); local_get(new_len); i32_const(es); i32_mul; i32_add;
                    call(self.emitter.rt.alloc); local_set(dst);
                    local_get(dst); local_get(new_len); i32_store(0);
                    // copy loop: dst[i] = xs[start + i] for i in 0..new_len (len reused as i)
                    i32_const(0); local_set(len);
                    block_empty; loop_empty;
                      local_get(len); local_get(new_len); i32_ge_u; br_if(1);
                      local_get(dst); i32_const(list_data_off); i32_add;
                      local_get(len); i32_const(es); i32_mul; i32_add;
                      local_get(xs); i32_const(list_data_off); i32_add;
                      local_get(start); local_get(len); i32_add;
                      i32_const(es); i32_mul; i32_add;
                });
                self.emit_elem_copy_owned(&elem_ty);
                wasm!(self.func, {
                      local_get(len); i32_const(1); i32_add; local_set(len);
                      br(0);
                    end; end;
                    local_get(dst);
                });
                self.scratch.free_i32(len);
                self.scratch.free_i32(dst);
                self.scratch.free_i32(new_len);
                self.scratch.free_i32(end);
                self.scratch.free_i32(start);
                self.scratch.free_i32(xs);
            }
            "reverse" => {
                let elem_ty = self.resolve_list_elem(&args[0], None);
                let elem_size = values::byte_size(&elem_ty);
                let xs = self.scratch.alloc_i32();
                let len = self.scratch.alloc_i32();
                let dst = self.scratch.alloc_i32();
                let i = self.scratch.alloc_i32();
                self.emit_expr(&args[0]);
                wasm!(self.func, {
                    local_set(xs);
                    local_get(xs); i32_load(0); local_set(len);
                    // alloc dst
                    i32_const(list_hdr); local_get(len); i32_const(elem_size as i32); i32_mul; i32_add;
                    call(self.emitter.rt.alloc); local_set(dst);
                    local_get(dst); local_get(len); i32_store(0);
                    // loop: dst[i] = src[len-1-i]
                    i32_const(0); local_set(i);
                    block_empty; loop_empty;
                      local_get(i); local_get(len); i32_ge_u; br_if(1);
                      // dst addr
                      local_get(dst); i32_const(list_data_off); i32_add;
                      local_get(i); i32_const(elem_size as i32); i32_mul; i32_add;
                      // src addr
                      local_get(xs); i32_const(list_data_off); i32_add;
                      local_get(len); i32_const(1); i32_sub; local_get(i); i32_sub;
                      i32_const(elem_size as i32); i32_mul; i32_add;
                });
                // Copy elem_size bytes
                self.emit_elem_copy_owned(&elem_ty);
                wasm!(self.func, {
                      local_get(i); i32_const(1); i32_add; local_set(i);
                      br(0);
                    end; end;
                    local_get(dst);
                });
                self.scratch.free_i32(i);
                self.scratch.free_i32(dst);
                self.scratch.free_i32(len);
                self.scratch.free_i32(xs);
            }
            "range" => {
                // range(start, end) → List[Int]
                let start_val = self.scratch.alloc_i32();
                let len = self.scratch.alloc_i32();
                let dst = self.scratch.alloc_i32();
                let i = self.scratch.alloc_i32();
                self.emit_expr(&args[0]);
                wasm!(self.func, { i32_wrap_i64; local_set(start_val); });
                self.emit_expr(&args[1]);
                wasm!(self.func, {
                    i32_wrap_i64;
                    local_get(start_val); i32_sub; // len = end - start
                    local_set(len);
                    local_get(len); i32_const(0); i32_lt_s;
                    if_empty; i32_const(0); local_set(len); end; // clamp to 0
                    // alloc
                    i32_const(list_hdr); local_get(len); i32_const(8); i32_mul; i32_add;
                    call(self.emitter.rt.alloc); local_set(dst);
                    local_get(dst); local_get(len); i32_store(0); // dst.len
                    i32_const(0); local_set(i);
                    block_empty; loop_empty;
                      local_get(i); local_get(len); i32_ge_u; br_if(1);
                      local_get(dst); i32_const(list_data_off); i32_add;
                      local_get(i); i32_const(8); i32_mul; i32_add;
                      local_get(start_val); local_get(i); i32_add;
                      i64_extend_i32_s;
                      i64_store(0);
                      local_get(i); i32_const(1); i32_add; local_set(i);
                      br(0);
                    end; end;
                    local_get(dst);
                });
                self.scratch.free_i32(i);
                self.scratch.free_i32(dst);
                self.scratch.free_i32(len);
                self.scratch.free_i32(start_val);
            }
            _ => return self.emit_list_closure_call(method, args),
        }
        true
    }
}

// Disjoint sub-dispatch groups of `emit_list_call`, split out to keep each file
// under the line-count ceiling. Chained from `emit_list_call`; arm patterns are
// disjoint so order is irrelevant and behavior is identical.
include!("calls_list_p2.rs");
include!("calls_list_p3.rs");
