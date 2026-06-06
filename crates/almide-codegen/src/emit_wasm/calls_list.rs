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
                // Store xs, i
                self.emit_expr(&args[0]);
                wasm!(self.func, { local_set(xs); });
                self.emit_expr(&args[1]); // i: i64
                wasm!(self.func, { i32_wrap_i64; local_set(idx); });
                // bounds check: i < 0 || i >= len → default
                match vt {
                    ValType::I64 => {
                        wasm!(self.func, {
                            local_get(idx); // i
                            local_get(xs); i32_load(0); // len
                            i32_ge_u;
                            local_get(idx); i32_const(0); i32_lt_s;
                            i32_or;
                            if_i64;
                        });
                        self.emit_expr(&args[2]); // default
                        wasm!(self.func, {
                            else_;
                              local_get(xs); i32_const(list_data_off); i32_add;
                              local_get(idx); i32_const(elem_size as i32); i32_mul; i32_add;
                              i64_load(0);
                            end;
                        });
                    }
                    ValType::F64 => {
                        wasm!(self.func, {
                            local_get(idx);
                            local_get(xs); i32_load(0);
                            i32_ge_u;
                            local_get(idx); i32_const(0); i32_lt_s;
                            i32_or;
                            if_f64;
                        });
                        self.emit_expr(&args[2]);
                        wasm!(self.func, {
                            else_;
                              local_get(xs); i32_const(list_data_off); i32_add;
                              local_get(idx); i32_const(elem_size as i32); i32_mul; i32_add;
                              f64_load(0);
                            end;
                        });
                    }
                    _ => {
                        wasm!(self.func, {
                            local_get(idx);
                            local_get(xs); i32_load(0);
                            i32_ge_u;
                            local_get(idx); i32_const(0); i32_lt_s;
                            i32_or;
                            if_i32;
                        });
                        self.emit_expr(&args[2]);
                        wasm!(self.func, {
                            else_;
                              local_get(xs); i32_const(list_data_off); i32_add;
                              local_get(idx); i32_const(elem_size as i32); i32_mul; i32_add;
                              i32_load(0);
                            end;
                        });
                    }
                }
                self.scratch.free_i32(idx);
                self.scratch.free_i32(xs);
            }
            "take" => {
                // take(xs, n) → List[A]: first min(n, len) elements
                let elem_ty = self.resolve_list_elem(&args[0], None);
                let es = values::byte_size(&elem_ty) as i32;
                let xs = self.scratch.alloc_i32();
                let n = self.scratch.alloc_i32();
                let new_len = self.scratch.alloc_i32();
                let dst = self.scratch.alloc_i32();
                let i = self.scratch.alloc_i32();
                self.emit_expr(&args[0]);
                wasm!(self.func, { local_set(xs); });
                self.emit_expr(&args[1]);
                wasm!(self.func, {
                    i32_wrap_i64; local_set(n);
                    // new_len = min(n, len)
                    local_get(n); local_get(xs); i32_load(0); i32_lt_u;
                    if_i32; local_get(n); else_; local_get(xs); i32_load(0); end;
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
                self.emit_elem_copy(&elem_ty);
                wasm!(self.func, {
                      local_get(i); i32_const(1); i32_add; local_set(i);
                      br(0);
                    end; end;
                    local_get(dst);
                });
                self.scratch.free_i32(i);
                self.scratch.free_i32(dst);
                self.scratch.free_i32(new_len);
                self.scratch.free_i32(n);
                self.scratch.free_i32(xs);
            }
            "drop" => {
                // drop(xs, n): skip first n
                let elem_ty = self.resolve_list_elem(&args[0], None);
                let es = values::byte_size(&elem_ty) as i32;
                let xs = self.scratch.alloc_i32();
                let start = self.scratch.alloc_i32();
                let new_len = self.scratch.alloc_i32();
                let dst = self.scratch.alloc_i32();
                let i = self.scratch.alloc_i32();
                self.emit_expr(&args[0]);
                wasm!(self.func, { local_set(xs); });
                self.emit_expr(&args[1]);
                wasm!(self.func, {
                    i32_wrap_i64; local_set(start);
                    // start = min(n, len)
                    local_get(start); local_get(xs); i32_load(0); i32_lt_u;
                    if_i32; local_get(start); else_; local_get(xs); i32_load(0); end;
                    local_set(start);
                    // new_len = len - start
                    local_get(xs); i32_load(0); local_get(start); i32_sub;
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
                self.emit_elem_copy(&elem_ty);
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
                self.emit_expr(&args[1]); // start
                wasm!(self.func, { i32_wrap_i64; local_set(start); });
                self.emit_expr(&args[2]); // end
                wasm!(self.func, {
                    i32_wrap_i64; local_set(end);
                    // Clamp start and end to [0, len] using UNSIGNED comparison so that
                    // negative indices (huge when unsigned, matching native's i64→usize
                    // cast) and out-of-range indices saturate to len. Mirrors native:
                    //   s = start; e = min(end, len); if s >= e { [] } else { xs[s..e] }
                    // start = min_u(start, len)
                    local_get(start); local_get(len);
                      local_get(start); local_get(len); i32_lt_u; select;
                    local_set(start);
                    // end = min_u(end, len)
                    local_get(end); local_get(len);
                      local_get(end); local_get(len); i32_lt_u; select;
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
                self.emit_elem_copy(&elem_ty);
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
                self.emit_elem_copy(&elem_ty);
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
            "first" => {
                // first(xs) → Option[A]: xs[0] or none
                let elem_ty = self.resolve_list_elem(&args[0], None);
                let elem_size = values::byte_size(&elem_ty);
                let xs = self.scratch.alloc_i32();
                let result = self.scratch.alloc_i32();
                self.emit_expr(&args[0]);
                wasm!(self.func, {
                    local_set(xs);
                    local_get(xs); i32_load(0); i32_eqz;
                    if_i32; i32_const(0); // none
                    else_;
                      i32_const(elem_size as i32); call(self.emitter.rt.alloc); local_set(result);
                      local_get(result);
                      local_get(xs); i32_const(list_data_off); i32_add;
                });
                self.emit_elem_copy(&elem_ty);
                wasm!(self.func, { local_get(result); end; });
                self.scratch.free_i32(result);
                self.scratch.free_i32(xs);
            }
            "last" => {
                let elem_ty = self.resolve_list_elem(&args[0], None);
                let elem_size = values::byte_size(&elem_ty);
                let xs = self.scratch.alloc_i32();
                let result = self.scratch.alloc_i32();
                self.emit_expr(&args[0]);
                wasm!(self.func, {
                    local_set(xs);
                    local_get(xs); i32_load(0); i32_eqz;
                    if_i32; i32_const(0);
                    else_;
                      i32_const(elem_size as i32); call(self.emitter.rt.alloc); local_set(result);
                      local_get(result);
                      // src = xs + 4 + (len-1) * elem_size
                      local_get(xs); i32_const(list_data_off); i32_add;
                      local_get(xs); i32_load(0); i32_const(1); i32_sub;
                      i32_const(elem_size as i32); i32_mul; i32_add;
                });
                self.emit_elem_copy(&elem_ty);
                wasm!(self.func, { local_get(result); end; });
                self.scratch.free_i32(result);
                self.scratch.free_i32(xs);
            }
            "is_empty" => {
                self.emit_expr(&args[0]);
                wasm!(self.func, { i32_load(0); i32_eqz; });
            }
            "sum" => {
                // sum(xs: List[Int]) → Int
                let xs = self.scratch.alloc_i32();
                let i = self.scratch.alloc_i32();
                let acc = self.scratch.alloc_i64();
                self.emit_expr(&args[0]);
                wasm!(self.func, {
                    local_set(xs);
                    i64_const(0); local_set(acc);
                    i32_const(0); local_set(i);
                    block_empty; loop_empty;
                      local_get(i); local_get(xs); i32_load(0); i32_ge_u; br_if(1);
                      local_get(acc);
                      local_get(xs); i32_const(list_data_off); i32_add;
                      local_get(i); i32_const(8); i32_mul; i32_add;
                      i64_load(0);
                      i64_add; local_set(acc);
                      local_get(i); i32_const(1); i32_add; local_set(i);
                      br(0);
                    end; end;
                    local_get(acc);
                });
                self.scratch.free_i64(acc);
                self.scratch.free_i32(i);
                self.scratch.free_i32(xs);
            }
            "product" => {
                let xs = self.scratch.alloc_i32();
                let i = self.scratch.alloc_i32();
                let acc = self.scratch.alloc_i64();
                self.emit_expr(&args[0]);
                wasm!(self.func, {
                    local_set(xs);
                    i64_const(1); local_set(acc);
                    i32_const(0); local_set(i);
                    block_empty; loop_empty;
                      local_get(i); local_get(xs); i32_load(0); i32_ge_u; br_if(1);
                      local_get(acc);
                      local_get(xs); i32_const(list_data_off); i32_add;
                      local_get(i); i32_const(8); i32_mul; i32_add;
                      i64_load(0);
                      i64_mul; local_set(acc);
                      local_get(i); i32_const(1); i32_add; local_set(i);
                      br(0);
                    end; end;
                    local_get(acc);
                });
                self.scratch.free_i64(acc);
                self.scratch.free_i32(i);
                self.scratch.free_i32(xs);
            }
            "join" => {
                // list.join(xs, sep) — delegate to string.join
                self.emit_expr(&args[0]);
                self.emit_expr(&args[1]);
                wasm!(self.func, { call(self.emitter.rt.string.join); });
            }
            "flatten" => {
                // flatten(xss: List[List[T]]) → List[T]
                // Two-pass: count total, then copy
                let inner_list_ty = self.resolve_list_elem(&args[0], None); // List[T]
                let elem_ty = self.list_elem_ty(&inner_list_ty); // T
                let elem_size = values::byte_size(&elem_ty); // size of T
                let xss = self.scratch.alloc_i32();
                let total = self.scratch.alloc_i32();
                let i = self.scratch.alloc_i32();
                let dst = self.scratch.alloc_i32();
                let inner = self.scratch.alloc_i32();
                let j = self.scratch.alloc_i32();
                self.emit_expr(&args[0]);
                wasm!(self.func, {
                    local_set(xss);
                    // Pass 1: count total elements
                    i32_const(0); local_set(total);
                    i32_const(0); local_set(i);
                    block_empty; loop_empty;
                      local_get(i); local_get(xss); i32_load(0); i32_ge_u; br_if(1);
                      local_get(total);
                      local_get(xss); i32_const(list_data_off); i32_add;
                      local_get(i); i32_const(4); i32_mul; i32_add; // &xss[i]
                      i32_load(0); // inner list ptr
                      i32_load(0); // inner list len
                      i32_add; local_set(total);
                      local_get(i); i32_const(1); i32_add; local_set(i);
                      br(0);
                    end; end;
                    // Alloc result
                    i32_const(list_hdr); local_get(total); i32_const(elem_size as i32); i32_mul; i32_add;
                    call(self.emitter.rt.alloc); local_set(dst);
                    local_get(dst); local_get(total); i32_store(0);
                    // Pass 2: copy
                    i32_const(0); local_set(total); // dst offset (in elements)
                    i32_const(0); local_set(i); // i (outer)
                    block_empty; loop_empty;
                      local_get(i); local_get(xss); i32_load(0); i32_ge_u; br_if(1);
                      // inner = xss[i]
                      local_get(xss); i32_const(list_data_off); i32_add;
                      local_get(i); i32_const(4); i32_mul; i32_add;
                      i32_load(0); local_set(inner);
                      // Copy inner elements
                      i32_const(0); local_set(j);
                      block_empty; loop_empty;
                        local_get(j); local_get(inner); i32_load(0); i32_ge_u; br_if(1);
                        // dst[dst_offset + j]
                        local_get(dst); i32_const(list_data_off); i32_add;
                        local_get(total); local_get(j); i32_add;
                        i32_const(elem_size as i32); i32_mul; i32_add;
                        // src inner[j]
                        local_get(inner); i32_const(list_data_off); i32_add;
                        local_get(j); i32_const(elem_size as i32); i32_mul; i32_add;
                });
                self.emit_elem_copy(&elem_ty);
                wasm!(self.func, {
                        local_get(j); i32_const(1); i32_add; local_set(j);
                        br(0);
                      end; end;
                      local_get(total); local_get(inner); i32_load(0); i32_add; local_set(total);
                      local_get(i); i32_const(1); i32_add; local_set(i);
                      br(0);
                    end; end;
                    local_get(dst);
                });
                self.scratch.free_i32(j);
                self.scratch.free_i32(inner);
                self.scratch.free_i32(dst);
                self.scratch.free_i32(i);
                self.scratch.free_i32(total);
                self.scratch.free_i32(xss);
            }
            "sort" => {
                self.emit_list_sort(args);
                return true;
            }
            "index_of" => {
                self.emit_list_index_of(args);
            }
            "min" | "max" => {
                // min/max(xs: List[A]) → Option[A], for any totally-ordered A.
                //
                // Element handling is type-directed: the element width (stride,
                // load/store) comes from `byte_size`/`ty_to_valtype`, and the
                // candidate-vs-best test goes through the shared total-order
                // emitter `emit_ord_cmp3`. The old code hard-coded i64 (stride 8,
                // `i64_load`, `i64_lt_s`), which read garbage for every non-Int
                // element type — String/Tuple/List/Bool — producing wrong
                // extrema (and an unbounded display loop when a List-pointer was
                // mis-read as an i64). Matching the native oracle
                // `xs.iter().min()` (Rust derived `Ord`) needs the real type.
                let is_max = method == "max";
                let elem_ty = self.resolve_list_elem(&args[0], None);
                let es = values::byte_size(&elem_ty) as i32;
                let vt = values::ty_to_valtype(&elem_ty).unwrap_or(ValType::I32);
                let xs = self.scratch.alloc_i32();
                let i = self.scratch.alloc_i32();
                let out = self.scratch.alloc_i32();
                let best = self.scratch.alloc(vt);
                let candidate = self.scratch.alloc(vt);
                // Result of `emit_ord_cmp3(candidate, best)` (sign): take the
                // candidate when it is strictly more extreme than the running
                // best (`> 0` for max, `< 0` for min).
                let cmp = self.scratch.alloc_i32();
                self.emit_expr(&args[0]);
                wasm!(self.func, {
                    local_set(xs);
                    local_get(xs); i32_load(0); i32_eqz;
                    if_i32; i32_const(0); // none
                    else_;
                      // best = xs[0]
                      local_get(xs); i32_const(list_data_off); i32_add;
                });
                self.emit_load_at(&elem_ty, 0);
                wasm!(self.func, {
                      local_set(best);
                      i32_const(1); local_set(i); // i=1
                      block_empty; loop_empty;
                        local_get(i); local_get(xs); i32_load(0); i32_ge_u; br_if(1);
                        local_get(xs); i32_const(list_data_off); i32_add;
                        local_get(i); i32_const(es); i32_mul; i32_add;
                });
                self.emit_load_at(&elem_ty, 0);
                wasm!(self.func, { local_set(candidate); });
                // cmp = sign(candidate <=> best)
                wasm!(self.func, { local_get(candidate); local_get(best); });
                self.emit_ord_cmp3(&elem_ty);
                wasm!(self.func, { local_set(cmp); });
                if is_max {
                    wasm!(self.func, { local_get(cmp); i32_const(0); i32_gt_s; });
                } else {
                    wasm!(self.func, { local_get(cmp); i32_const(0); i32_lt_s; });
                }
                wasm!(self.func, {
                        if_empty; local_get(candidate); local_set(best); end;
                        local_get(i); i32_const(1); i32_add; local_set(i);
                        br(0);
                      end; end;
                      // some(best): allocate a payload slot sized for the element
                      // and store best at offset 0 with the matching width.
                      i32_const(es); call(self.emitter.rt.alloc); local_set(out);
                      local_get(out); local_get(best);
                });
                self.emit_store_at(&elem_ty, 0);
                wasm!(self.func, {
                      local_get(out);
                    end;
                });
                self.scratch.free_i32(cmp);
                self.scratch.free(candidate, vt);
                self.scratch.free(best, vt);
                self.scratch.free_i32(out);
                self.scratch.free_i32(i);
                self.scratch.free_i32(xs);
            }
            "intersperse" => {
                // intersperse(xs, sep) → List[A]: insert sep between elements
                let elem_ty = self.resolve_list_elem(&args[0], None);
                let elem_size = values::byte_size(&elem_ty);
                let vt = values::ty_to_valtype(&elem_ty).unwrap_or(ValType::I32);
                let xs = self.scratch.alloc_i32();
                let sep = self.scratch.alloc(vt);
                let len = self.scratch.alloc_i32();
                let dst = self.scratch.alloc_i32();
                let new_len = self.scratch.alloc_i32();
                let src_i = self.scratch.alloc_i32();
                let dst_i = self.scratch.alloc_i32();
                self.emit_expr(&args[0]);
                wasm!(self.func, { local_set(xs); });
                self.emit_expr(&args[1]); // sep
                wasm!(self.func, { local_set(sep); });
                wasm!(self.func, {
                    local_get(xs); i32_load(0); local_set(len);
                    // new_len = max(0, 2*len - 1)
                    local_get(len); i32_eqz;
                    if_i32;
                      // empty list
                      i32_const(list_hdr); call(self.emitter.rt.alloc); local_set(dst);
                      local_get(dst); i32_const(0); i32_store(0);
                      local_get(dst);
                    else_;
                      local_get(len); i32_const(2); i32_mul; i32_const(1); i32_sub; local_set(new_len);
                      i32_const(list_hdr); local_get(new_len); i32_const(elem_size as i32); i32_mul; i32_add;
                      call(self.emitter.rt.alloc); local_set(dst);
                      local_get(dst); local_get(new_len); i32_store(0);
                      // Fill
                      i32_const(0); local_set(src_i);
                      i32_const(0); local_set(dst_i);
                      block_empty; loop_empty;
                        local_get(src_i); local_get(len); i32_ge_u; br_if(1);
                        // Copy xs[src_i] to dst[dst_i]
                        local_get(dst); i32_const(list_data_off); i32_add;
                        local_get(dst_i); i32_const(elem_size as i32); i32_mul; i32_add;
                        local_get(xs); i32_const(list_data_off); i32_add;
                        local_get(src_i); i32_const(elem_size as i32); i32_mul; i32_add;
                });
                self.emit_elem_copy(&elem_ty);
                wasm!(self.func, {
                        local_get(dst_i); i32_const(1); i32_add; local_set(dst_i);
                        // Insert sep if not last
                        local_get(src_i); local_get(len); i32_const(1); i32_sub; i32_lt_u;
                        if_empty;
                          local_get(dst); i32_const(list_data_off); i32_add;
                          local_get(dst_i); i32_const(elem_size as i32); i32_mul; i32_add;
                          local_get(sep);
                });
                self.emit_elem_store(&elem_ty);
                wasm!(self.func, {
                          local_get(dst_i); i32_const(1); i32_add; local_set(dst_i);
                        end;
                        local_get(src_i); i32_const(1); i32_add; local_set(src_i);
                        br(0);
                      end; end;
                      local_get(dst);
                    end;
                });
                self.scratch.free_i32(dst_i);
                self.scratch.free_i32(src_i);
                self.scratch.free_i32(new_len);
                self.scratch.free_i32(dst);
                self.scratch.free_i32(len);
                self.scratch.free(sep, vt);
                self.scratch.free_i32(xs);
            }
            "zip" => {
                // zip(xs, ys) → List[(A, B)]
                // Each tuple is heap-allocated: [a_value, b_value]
                let a_ty = self.resolve_list_elem(&args[0], None);
                let b_ty = self.resolve_list_elem(&args[1], None);
                let a_size = values::byte_size(&a_ty);
                let b_size = values::byte_size(&b_ty);
                let tuple_size = a_size + b_size;
                let xs = self.scratch.alloc_i32();
                let ys = self.scratch.alloc_i32();
                let len = self.scratch.alloc_i32();
                let dst = self.scratch.alloc_i32();
                let i = self.scratch.alloc_i32();
                let tup = self.scratch.alloc_i32();
                self.emit_expr(&args[0]);
                wasm!(self.func, { local_set(xs); });
                self.emit_expr(&args[1]);
                wasm!(self.func, {
                    local_set(ys);
                    // len = min(xs.len, ys.len)
                    local_get(xs); i32_load(0);
                    local_get(ys); i32_load(0);
                    i32_lt_u;
                    if_i32;
                      local_get(xs); i32_load(0);
                    else_;
                      local_get(ys); i32_load(0);
                    end;
                    local_set(len);
                    // Alloc result: list of ptrs to tuples
                    i32_const(list_hdr); local_get(len); i32_const(4); i32_mul; i32_add;
                    call(self.emitter.rt.alloc); local_set(dst);
                    local_get(dst); local_get(len); i32_store(0);
                    i32_const(0); local_set(i);
                    block_empty; loop_empty;
                      local_get(i); local_get(len); i32_ge_u; br_if(1);
                      // Alloc tuple
                      i32_const(tuple_size as i32); call(self.emitter.rt.alloc); local_set(tup);
                      // Copy a: tuple[0] = xs[i]
                      local_get(tup);
                      local_get(xs); i32_const(list_data_off); i32_add;
                      local_get(i); i32_const(a_size as i32); i32_mul; i32_add;
                });
                self.emit_elem_copy(&a_ty);
                // Copy b: tuple[a_size] = ys[i]
                wasm!(self.func, {
                      local_get(tup); i32_const(a_size as i32); i32_add;
                      local_get(ys); i32_const(list_data_off); i32_add;
                      local_get(i); i32_const(b_size as i32); i32_mul; i32_add;
                });
                self.emit_elem_copy(&b_ty);
                wasm!(self.func, {
                      // result[i] = tuple_ptr
                      local_get(dst); i32_const(list_data_off); i32_add;
                      local_get(i); i32_const(4); i32_mul; i32_add;
                      local_get(tup); i32_store(0);
                      local_get(i); i32_const(1); i32_add; local_set(i);
                      br(0);
                    end; end;
                    local_get(dst);
                });
                self.scratch.free_i32(tup);
                self.scratch.free_i32(i);
                self.scratch.free_i32(dst);
                self.scratch.free_i32(len);
                self.scratch.free_i32(ys);
                self.scratch.free_i32(xs);
            }
            "set" => {
                // set(xs, i, val) → List[A]: copy + replace element at i
                let elem_ty = self.resolve_list_elem(&args[0], None);
                let es = values::byte_size(&elem_ty) as i32;
                let xs = self.scratch.alloc_i32();
                let idx = self.scratch.alloc_i32();
                let len = self.scratch.alloc_i32();
                let dst = self.scratch.alloc_i32();
                let i = self.scratch.alloc_i32();
                self.emit_expr(&args[0]);
                wasm!(self.func, { local_set(xs); });
                self.emit_expr(&args[1]); // i: i64
                wasm!(self.func, {
                    i32_wrap_i64; local_set(idx);
                    local_get(xs); i32_load(0); local_set(len);
                    // Alloc copy
                    i32_const(list_hdr); local_get(len); i32_const(es); i32_mul; i32_add;
                    call(self.emitter.rt.alloc); local_set(dst);
                    local_get(dst); local_get(len); i32_store(0);
                    // Copy all elements
                    i32_const(0); local_set(i);
                    block_empty; loop_empty;
                      local_get(i); local_get(len); i32_ge_u; br_if(1);
                      local_get(dst); i32_const(list_data_off); i32_add;
                      local_get(i); i32_const(es); i32_mul; i32_add;
                      local_get(xs); i32_const(list_data_off); i32_add;
                      local_get(i); i32_const(es); i32_mul; i32_add;
                });
                self.emit_elem_copy(&elem_ty);
                wasm!(self.func, {
                      local_get(i); i32_const(1); i32_add; local_set(i);
                      br(0);
                    end; end;
                });
                // Overwrite dst[idx] with val
                wasm!(self.func, {
                    local_get(dst); i32_const(list_data_off); i32_add;
                    local_get(idx); i32_const(es); i32_mul; i32_add;
                });
                self.emit_expr(&args[2]);
                self.emit_elem_store(&elem_ty);
                wasm!(self.func, { local_get(dst); });
                self.scratch.free_i32(i);
                self.scratch.free_i32(dst);
                self.scratch.free_i32(len);
                self.scratch.free_i32(idx);
                self.scratch.free_i32(xs);
            }
            "insert" => {
                // insert(xs, i, val) → List[A]: copy with element inserted at i
                let elem_ty = self.resolve_list_elem(&args[0], None);
                let es = values::byte_size(&elem_ty) as i32;
                let xs = self.scratch.alloc_i32();
                let idx = self.scratch.alloc_i32();
                let old_len = self.scratch.alloc_i32();
                let dst = self.scratch.alloc_i32();
                let i = self.scratch.alloc_i32();
                self.emit_expr(&args[0]);
                wasm!(self.func, { local_set(xs); });
                self.emit_expr(&args[1]);
                wasm!(self.func, {
                    i32_wrap_i64; local_set(idx);
                    local_get(xs); i32_load(0); local_set(old_len);
                    // Clamp idx to [0, old_len] (unsigned) so that out-of-range or negative
                    // indices append at the end. Mirrors native's idx = min(i as usize, len).
                    local_get(idx); local_get(old_len);
                      local_get(idx); local_get(old_len); i32_lt_u; select;
                    local_set(idx);
                    // new_len = old_len + 1
                    i32_const(list_hdr); local_get(old_len); i32_const(1); i32_add; i32_const(es); i32_mul; i32_add;
                    call(self.emitter.rt.alloc); local_set(dst);
                    local_get(dst); local_get(old_len); i32_const(1); i32_add; i32_store(0);
                    // Copy [0..idx)
                    i32_const(0); local_set(i);
                    block_empty; loop_empty;
                      local_get(i); local_get(idx); i32_ge_u; br_if(1);
                      local_get(dst); i32_const(list_data_off); i32_add;
                      local_get(i); i32_const(es); i32_mul; i32_add;
                      local_get(xs); i32_const(list_data_off); i32_add;
                      local_get(i); i32_const(es); i32_mul; i32_add;
                });
                self.emit_elem_copy(&elem_ty);
                wasm!(self.func, {
                      local_get(i); i32_const(1); i32_add; local_set(i);
                      br(0);
                    end; end;
                });
                // Insert val at idx
                wasm!(self.func, {
                    local_get(dst); i32_const(list_data_off); i32_add;
                    local_get(idx); i32_const(es); i32_mul; i32_add;
                });
                self.emit_expr(&args[2]);
                self.emit_elem_store(&elem_ty);
                // Copy [idx..old_len)
                wasm!(self.func, {
                    local_get(idx); local_set(i);
                    block_empty; loop_empty;
                      local_get(i); local_get(old_len); i32_ge_u; br_if(1);
                      local_get(dst); i32_const(list_data_off); i32_add;
                      local_get(i); i32_const(1); i32_add; i32_const(es); i32_mul; i32_add;
                      local_get(xs); i32_const(list_data_off); i32_add;
                      local_get(i); i32_const(es); i32_mul; i32_add;
                });
                self.emit_elem_copy(&elem_ty);
                wasm!(self.func, {
                      local_get(i); i32_const(1); i32_add; local_set(i);
                      br(0);
                    end; end;
                    local_get(dst);
                });
                self.scratch.free_i32(i);
                self.scratch.free_i32(dst);
                self.scratch.free_i32(old_len);
                self.scratch.free_i32(idx);
                self.scratch.free_i32(xs);
            }
            "remove_at" => {
                // remove_at(xs, i) → List[A]
                let elem_ty = self.resolve_list_elem(&args[0], None);
                let es = values::byte_size(&elem_ty) as i32;
                let xs = self.scratch.alloc_i32();
                let idx = self.scratch.alloc_i32();
                let old_len = self.scratch.alloc_i32();
                let dst = self.scratch.alloc_i32();
                let i = self.scratch.alloc_i32();
                self.emit_expr(&args[0]);
                wasm!(self.func, { local_set(xs); });
                self.emit_expr(&args[1]);
                wasm!(self.func, {
                    i32_wrap_i64; local_set(idx);
                    local_get(xs); i32_load(0); local_set(old_len);
                    // Native is a no-op when i >= len (returns the list unchanged). Guard
                    // with an UNSIGNED compare so negative indices (huge unsigned) are no-ops.
                    local_get(idx); local_get(old_len); i32_lt_u;
                    if_i32;
                    // new_len = old_len - 1
                    i32_const(list_hdr); local_get(old_len); i32_const(1); i32_sub; i32_const(es); i32_mul; i32_add;
                    call(self.emitter.rt.alloc); local_set(dst);
                    local_get(dst); local_get(old_len); i32_const(1); i32_sub; i32_store(0);
                    // Copy [0..idx)
                    i32_const(0); local_set(i);
                    block_empty; loop_empty;
                      local_get(i); local_get(idx); i32_ge_u; br_if(1);
                      local_get(dst); i32_const(list_data_off); i32_add;
                      local_get(i); i32_const(es); i32_mul; i32_add;
                      local_get(xs); i32_const(list_data_off); i32_add;
                      local_get(i); i32_const(es); i32_mul; i32_add;
                });
                self.emit_elem_copy(&elem_ty);
                wasm!(self.func, {
                      local_get(i); i32_const(1); i32_add; local_set(i);
                      br(0);
                    end; end;
                });
                // Copy [idx+1..old_len)
                wasm!(self.func, {
                    local_get(idx); i32_const(1); i32_add; local_set(i);
                    block_empty; loop_empty;
                      local_get(i); local_get(old_len); i32_ge_u; br_if(1);
                      local_get(dst); i32_const(list_data_off); i32_add;
                      local_get(i); i32_const(1); i32_sub; i32_const(es); i32_mul; i32_add;
                      local_get(xs); i32_const(list_data_off); i32_add;
                      local_get(i); i32_const(es); i32_mul; i32_add;
                });
                self.emit_elem_copy(&elem_ty);
                wasm!(self.func, {
                      local_get(i); i32_const(1); i32_add; local_set(i);
                      br(0);
                    end; end;
                    local_get(dst);
                    else_;
                    // i >= len → return the list unchanged
                    local_get(xs);
                    end;
                });
                self.scratch.free_i32(i);
                self.scratch.free_i32(dst);
                self.scratch.free_i32(old_len);
                self.scratch.free_i32(idx);
                self.scratch.free_i32(xs);
            }
            "unique" => {
                self.emit_list_unique(args);
            }
            "enumerate" => {
                self.emit_list_enumerate(args);
            }
            "get" => {
                // list.get(list, index) → Option[T]
                let elem_ty = self.resolve_list_elem(&args[0], None);
                let elem_size = values::byte_size(&elem_ty);

                let list = self.scratch.alloc_i32();
                let idx = self.scratch.alloc_i32();
                let result = self.scratch.alloc_i32();
                self.emit_expr(&args[0]);
                wasm!(self.func, { local_set(list); });
                self.emit_expr(&args[1]);
                // Index is always Int (i64) — wrap to i32 for memory addressing.
                // Check both explicit type and Unknown (which may actually be Int from TupleIndex etc.)
                if matches!(&args[1].ty, Ty::Int) || args[1].ty.is_unresolved() {
                    wasm!(self.func, { i32_wrap_i64; });
                }
                wasm!(self.func, {
                    local_set(idx);
                    // bounds: idx >= len → none(0)
                    local_get(idx);
                    local_get(list);
                    i32_load(0); // len
                    i32_ge_u;
                    if_i32;
                    i32_const(0); // none
                    else_;
                    // alloc
                    i32_const(elem_size as i32);
                    call(self.emitter.rt.alloc);
                    local_set(result);
                    // dst=result, src=list+4+idx*elem_size
                    local_get(result);
                    local_get(list);
                    i32_const(list_data_off);
                    i32_add;
                    local_get(idx);
                    i32_const(elem_size as i32);
                    i32_mul;
                    i32_add;
                });
                self.emit_load_at(&elem_ty, 0); // load elem
                self.emit_store_at(&elem_ty, 0); // store at dst
                wasm!(self.func, {
                    local_get(result); // return ptr
                    end;
                });
                self.scratch.free_i32(result);
                self.scratch.free_i32(idx);
                self.scratch.free_i32(list);
            }
            "contains" => {
                // list.contains(list, elem) -> Bool (i32)
                let elem_ty = self.resolve_list_elem(&args[0], None);
                let elem_size = values::byte_size(&elem_ty);
                let target_vt = values::ty_to_valtype(&elem_ty).unwrap_or(ValType::I32);
                let list_ptr = self.scratch.alloc_i32();
                let idx = self.scratch.alloc_i32();
                let result = self.scratch.alloc_i32();
                // Hold the search target in a valtype-matched register (i64 Int,
                // f64 Float, i32 pointer for String/compound). The element load and
                // compare below use `emit_load_at`/`emit_eq_typed(elem_ty)` so both
                // sides agree on width and use STRUCTURAL (deep) equality — matching
                // native `xs.contains(&x)`, not pointer identity.
                let target = self.scratch.alloc(target_vt);
                self.emit_expr(&args[0]);
                wasm!(self.func, { local_set(list_ptr); });
                self.emit_expr(&args[1]);
                wasm!(self.func, {
                    local_set(target);
                    i32_const(0); local_set(idx);
                    i32_const(0); local_set(result); // result = false
                    block_empty; loop_empty;
                      local_get(idx); local_get(list_ptr); i32_load(0); i32_ge_u; br_if(1);
                      local_get(list_ptr); i32_const(list_data_off); i32_add;
                      local_get(idx); i32_const(elem_size as i32); i32_mul; i32_add;
                });
                self.emit_load_at(&elem_ty, 0);
                wasm!(self.func, { local_get(target); });
                self.emit_eq_typed(&elem_ty);
                wasm!(self.func, {
                      if_empty;
                        i32_const(1); local_set(result); br(2);
                      end;
                      local_get(idx); i32_const(1); i32_add; local_set(idx);
                      br(0);
                    end; end;
                    local_get(result);
                });
                self.scratch.free(target, target_vt);
                self.scratch.free_i32(result);
                self.scratch.free_i32(idx);
                self.scratch.free_i32(list_ptr);
            }
            "with_capacity" => {
                // list.with_capacity(cap: Int) -> List[A]
                // Layout: [len:i32][cap:i32][data...]. Preallocate data space.
                //
                // Capacity is only a PERF HINT and the list starts empty
                // (len=0), so the eagerly-reserved data is clamped to a
                // memory-safe ceiling. Two failure modes this guards:
                //   1. `cap*elem_size` overflowing the i32 heap-size argument
                //      (i32::MAX wrapped mod 2^32 to ~0, so `alloc` reserved
                //      nothing while the stored cap claimed billions of slots —
                //      the next alloc overlapped this header → garbage `len`,
                //      and `push` wrote OOB).
                //   2. a multi-GB pre-reservation exhausting wasm linear memory
                //      (`memory.grow` fails → `unreachable` trap), where native
                //      returns a working empty list.
                // Clamping the PRE-RESERVATION is observably identical to native
                // (the list is empty either way); `push` grows past the clamped
                // cap normally. The STORED cap equals the backing buffer, so
                // push's `len < cap` fast path never outruns it.
                let elem_ty = self.resolve_list_elem(&args[0], None);
                let elem_size = values::byte_size(&elem_ty) as i64;
                // Ceiling on eagerly-reserved DATA bytes: large enough to honor
                // realistic hints, small enough to never exhaust wasm memory.
                const MAX_WITH_CAPACITY_PREALLOC_BYTES: i64 = 64 * 1024 * 1024; // 64 MiB
                let max_cap = (MAX_WITH_CAPACITY_PREALLOC_BYTES / elem_size) as i32;
                let new_ptr = self.scratch.alloc_i32();
                let cap_local = self.scratch.alloc_i32();
                let cap64 = self.scratch.alloc_i64();
                // Evaluate the requested capacity once (it is an i64 Int).
                self.emit_expr(&args[0]);
                wasm!(self.func, { local_set(cap64); });
                // cap_local = clamp(req, 0, max_cap), done on the i64 value so
                // the lossy wrap to i32 happens only on an already-safe number.
                wasm!(self.func, {
                    local_get(cap64); i64_const(0); i64_lt_s;
                    if_i32;
                      i32_const(0);                          // negative → empty
                    else_;
                      local_get(cap64);
                      i32_const(max_cap as i32); i64_extend_i32_s; i64_gt_s;
                      if_i32;
                        i32_const(max_cap as i32);           // saturate
                      else_;
                        local_get(cap64); i32_wrap_i64;      // fits
                      end;
                    end;
                    local_set(cap_local);
                });
                // alloc: hdr + cap * elem_size  (cap is clamped → no overflow)
                wasm!(self.func, {
                    i32_const(list_hdr);
                    local_get(cap_local); i32_const(elem_size as i32); i32_mul;
                    i32_add;
                    call(self.emitter.rt.alloc);
                    local_set(new_ptr);
                    // len = 0
                    local_get(new_ptr); i32_const(0); i32_store(0);
                    // cap = clamped capacity (matches the backing buffer)
                    local_get(new_ptr); local_get(cap_local); i32_store(4);
                    local_get(new_ptr);
                });
                self.scratch.free_i64(cap64);
                self.scratch.free_i32(cap_local);
                self.scratch.free_i32(new_ptr);
            }
            "push" => {
                // push(xs, v) → Unit. O(1) amortized via capacity tracking.
                // Layout: [len:i32][cap:i32][data...]
                // If len < cap: write in place. Else: realloc with 2x growth.
                let elem_ty = self.resolve_list_elem(&args[0], None);
                let elem_size = values::byte_size(&elem_ty);
                let vt = values::ty_to_valtype(&elem_ty).unwrap_or(ValType::I32);
                let ptr = self.scratch.alloc_i32();
                let len_local = self.scratch.alloc_i32();
                let cap_local = self.scratch.alloc_i32();
                let new_ptr = self.scratch.alloc_i32();
                let val_scratch = self.scratch.alloc(vt);

                // Evaluate value first (before reading xs, in case of side effects)
                self.emit_expr(&args[1]);
                wasm!(self.func, { local_set(val_scratch); });

                // Read xs pointer, len, cap
                self.emit_expr(&args[0]);
                wasm!(self.func, { local_set(ptr); });
                wasm!(self.func, {
                    local_get(ptr); i32_load(0); local_set(len_local);
                    local_get(ptr); i32_load(4); local_set(cap_local);
                });

                // if len < cap: write in place (fast path)
                wasm!(self.func, {
                    local_get(len_local);
                    local_get(cap_local);
                    i32_lt_u;
                    if_empty;
                });
                // Fast path: write element at ptr + 8 + len * elem_size
                wasm!(self.func, {
                    local_get(ptr); i32_const(list_data_off); i32_add;
                    local_get(len_local); i32_const(elem_size as i32); i32_mul;
                    i32_add;
                    local_get(val_scratch);
                });
                match vt {
                    ValType::I64 => { wasm!(self.func, { i64_store(0); }); }
                    ValType::F64 => { wasm!(self.func, { f64_store(0); }); }
                    _ => { wasm!(self.func, { i32_store(0); }); }
                }
                // Increment len
                wasm!(self.func, {
                    local_get(ptr);
                    local_get(len_local); i32_const(1); i32_add;
                    i32_store(0);
                    else_;
                });

                // Slow path: new_cap = max(cap * 2, len + 1, 8)
                // cap=0 is common (lists created without explicit cap), so
                // we MUST ensure new_cap >= len+1 to avoid buffer overflow.
                wasm!(self.func, {
                    // cap_local = max(cap * 2, 8)
                    local_get(cap_local); i32_const(1); i32_shl;
                    i32_const(8);
                    i32_gt_u;
                    if_i32;
                      local_get(cap_local); i32_const(1); i32_shl;
                    else_;
                      i32_const(8);
                    end;
                    local_set(cap_local);
                    // cap_local = max(cap_local, len + 1)
                    local_get(cap_local);
                    local_get(len_local); i32_const(1); i32_add;
                    i32_lt_u;
                    if_empty;
                      local_get(len_local); i32_const(1); i32_add;
                      local_set(cap_local);
                    end;
                });

                // Ensure new_cap > len (edge case: cap=0, len=0 → new_cap=8 > 0 ✓)
                // Allocate: 8 + new_cap * elem_size
                wasm!(self.func, {
                    i32_const(list_hdr);
                    local_get(cap_local); i32_const(elem_size as i32); i32_mul;
                    i32_add;
                    call(self.emitter.rt.alloc);
                    local_set(new_ptr);
                });

                // Write new len = old_len + 1
                wasm!(self.func, {
                    local_get(new_ptr);
                    local_get(len_local); i32_const(1); i32_add;
                    i32_store(0);
                });
                // Write new cap
                wasm!(self.func, {
                    local_get(new_ptr);
                    local_get(cap_local);
                    i32_store(4);
                });

                // Copy old data: memory.copy(new_ptr+8, ptr+8, len * elem_size)
                wasm!(self.func, {
                    local_get(new_ptr); i32_const(list_data_off); i32_add;
                    local_get(ptr); i32_const(list_data_off); i32_add;
                    local_get(len_local); i32_const(elem_size as i32); i32_mul;
                    memory_copy;
                });

                // Write new element at new_ptr + 8 + len * elem_size
                wasm!(self.func, {
                    local_get(new_ptr); i32_const(list_data_off); i32_add;
                    local_get(len_local); i32_const(elem_size as i32); i32_mul;
                    i32_add;
                    local_get(val_scratch);
                });
                match vt {
                    ValType::I64 => { wasm!(self.func, { i64_store(0); }); }
                    ValType::F64 => { wasm!(self.func, { f64_store(0); }); }
                    _ => { wasm!(self.func, { i32_store(0); }); }
                }

                // Write back the (possibly reallocated) list ptr — into the cell for a
                // mutable capture, else into the local/global. See emit_mutator_writeback.
                self.emit_mutator_writeback(&args[0], new_ptr);

                wasm!(self.func, { end; }); // end if/else

                self.scratch.free(val_scratch, vt);
                self.scratch.free_i32(new_ptr);
                self.scratch.free_i32(cap_local);
                self.scratch.free_i32(len_local);
                self.scratch.free_i32(ptr);
            }
            "pop" => {
                // pop(xs) → Option[A]. Removes last element, mutates xs.
                // Option layout: 0 = none, non-zero ptr = some (payload at ptr)
                let elem_ty = self.resolve_list_elem(&args[0], None);
                let elem_size = values::byte_size(&elem_ty);
                let list_ptr = self.scratch.alloc_i32();
                let len = self.scratch.alloc_i32();
                let result = self.scratch.alloc_i32();

                self.emit_expr(&args[0]);
                wasm!(self.func, { local_set(list_ptr); });
                wasm!(self.func, { local_get(list_ptr); i32_load(0); local_set(len); });

                // if len == 0 → none (0)
                // else → decrement len, copy last element into alloc'd payload
                wasm!(self.func, {
                    local_get(len); i32_eqz;
                    if_i32;
                      i32_const(0); // none
                    else_;
                });

                // Decrement len in place
                wasm!(self.func, {
                    local_get(list_ptr);
                    local_get(len); i32_const(1); i32_sub;
                    i32_store(0);
                });

                // Allocate payload (no tag — Option uses ptr==0 for none)
                wasm!(self.func, {
                    i32_const(elem_size as i32);
                    call(self.emitter.rt.alloc);
                    local_set(result);
                    // Copy last element: dst=result, src=list+4+(len-1)*elem_size
                    local_get(result);
                    local_get(list_ptr); i32_const(list_data_off); i32_add;
                    local_get(len); i32_const(1); i32_sub;
                    i32_const(elem_size as i32); i32_mul; i32_add;
                });
                self.emit_load_at(&elem_ty, 0);
                self.emit_store_at(&elem_ty, 0);
                wasm!(self.func, {
                      local_get(result);
                    end;
                });

                self.scratch.free_i32(result);
                self.scratch.free_i32(len);
                self.scratch.free_i32(list_ptr);
            }
            "clear" => {
                // clear(xs) → Unit. Sets len to 0 in place.
                self.emit_expr(&args[0]);
                wasm!(self.func, { i32_const(0); i32_store(0); });
            }
            _ => return self.emit_list_closure_call(method, args),
        }
        true
    }
}
