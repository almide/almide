// List stdlib call dispatch for WASM codegen — group 2 (split from calls_list.rs).
//
// Sub-match of `emit_list_call` over the SAME `method` scrutinee, restricted to
// a disjoint set of method strings. Arm bodies are verbatim; each arm yields
// `true` (handled). Chained from `emit_list_call` before the group-1 match.
//
// NOTE: This file is `include!`d into calls_list.rs; it intentionally has NO
// `use` items of its own (they would collide with the parent's imports). The
// referenced types/macros resolve from calls_list.rs's module scope.

impl FuncCompiler<'_> {
    /// Group-2 sub-dispatch of `emit_list_call`. Returns true if `method` was
    /// handled here.
    pub(super) fn emit_list_call_g2(&mut self, method: &str, args: &[IrExpr]) -> bool {
        use super::engine::layout::{LIST, list as ll};
        let list_data_off = self.emitter.layout_reg.fixed_offset(LIST, ll::DATA) as i32;
        let list_hdr = self.emitter.layout_reg.header_size(LIST) as i32;
        match method {
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
                self.emit_elem_copy_owned(&elem_ty);
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
                self.emit_elem_copy_owned(&elem_ty);
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
                self.emit_elem_copy_owned(&elem_ty);
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
                // SHARE: the winner pointer aliases an element of a list the
                // caller still owns; the some() box must own its own ref —
                // a freed-then-reused winner is how a list's len became a
                // free-list next pointer (the repr-loop wall-clock hang).
                if crate::pass_perceus::is_heap_type(&elem_ty) {
                    wasm!(self.func, { call(self.emitter.rt.rc_inc); });
                }
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
                self.emit_elem_copy_owned(&elem_ty);
                wasm!(self.func, {
                        local_get(dst_i); i32_const(1); i32_add; local_set(dst_i);
                        // Insert sep if not last
                        local_get(src_i); local_get(len); i32_const(1); i32_sub; i32_lt_u;
                        if_empty;
                          local_get(dst); i32_const(list_data_off); i32_add;
                          local_get(dst_i); i32_const(elem_size as i32); i32_mul; i32_add;
                          local_get(sep);
                });
                // SHARE: the sep value's source survives; every stored slot
                // needs its own reference.
                if crate::pass_perceus::is_heap_type(&elem_ty) {
                    wasm!(self.func, { call(self.emitter.rt.rc_inc); });
                }
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
            _ => return false,
        }
        true
    }
}
