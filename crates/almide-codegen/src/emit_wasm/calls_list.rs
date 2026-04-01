//! List stdlib call dispatch for WASM codegen (non-closure functions).

use super::FuncCompiler;
use super::values;
use almide_ir::IrExpr;
use almide_lang::types::Ty;
use wasm_encoder::ValType;

impl FuncCompiler<'_> {
    /// Dispatch a list stdlib method call (non-closure). Returns true if handled.
    pub(super) fn emit_list_call(&mut self, method: &str, args: &[IrExpr]) -> bool {
        match method {
            "len" | "length" => {
                self.emit_expr(&args[0]);
                wasm!(self.func, { i32_load(0); i64_extend_i32_u; });
            }
            "get_or" => {
                // get_or(xs, i, default) → A
                let elem_ty = self.list_elem_ty(&args[0].ty);
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
                              local_get(xs); i32_const(4); i32_add;
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
                              local_get(xs); i32_const(4); i32_add;
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
                              local_get(xs); i32_const(4); i32_add;
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
                let elem_ty = self.list_elem_ty(&args[0].ty);
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
                    i32_const(4); local_get(new_len); i32_const(es); i32_mul; i32_add;
                    call(self.emitter.rt.alloc); local_set(dst);
                    local_get(dst); local_get(new_len); i32_store(0);
                    i32_const(0); local_set(i);
                    block_empty; loop_empty;
                      local_get(i); local_get(new_len); i32_ge_u; br_if(1);
                      local_get(dst); i32_const(4); i32_add;
                      local_get(i); i32_const(es); i32_mul; i32_add;
                      local_get(xs); i32_const(4); i32_add;
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
                let elem_ty = self.list_elem_ty(&args[0].ty);
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
                    i32_const(4); local_get(new_len); i32_const(es); i32_mul; i32_add;
                    call(self.emitter.rt.alloc); local_set(dst);
                    local_get(dst); local_get(new_len); i32_store(0);
                    i32_const(0); local_set(i);
                    block_empty; loop_empty;
                      local_get(i); local_get(new_len); i32_ge_u; br_if(1);
                      local_get(dst); i32_const(4); i32_add;
                      local_get(i); i32_const(es); i32_mul; i32_add;
                      local_get(xs); i32_const(4); i32_add;
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
                let elem_ty = self.list_elem_ty(&args[0].ty);
                let es = values::byte_size(&elem_ty) as i32;
                let xs = self.scratch.alloc_i32();
                let start = self.scratch.alloc_i32();
                let end = self.scratch.alloc_i32();
                let new_len = self.scratch.alloc_i32();
                let dst = self.scratch.alloc_i32();
                self.emit_expr(&args[0]);
                wasm!(self.func, { local_set(xs); });
                self.emit_expr(&args[1]); // start
                wasm!(self.func, { i32_wrap_i64; local_set(start); });
                self.emit_expr(&args[2]); // end
                wasm!(self.func, {
                    i32_wrap_i64; local_set(end);
                    local_get(end); local_get(start); i32_sub; local_set(new_len);
                    i32_const(4); local_get(new_len); i32_const(es); i32_mul; i32_add;
                    call(self.emitter.rt.alloc); local_set(dst);
                    local_get(dst); local_get(new_len); i32_store(0);
                    // copy loop — reuse new_len as i
                    i32_const(0); local_set(new_len);
                    block_empty; loop_empty;
                      local_get(new_len); local_get(end); local_get(start); i32_sub; i32_ge_u; br_if(1);
                      local_get(dst); i32_const(4); i32_add;
                      local_get(new_len); i32_const(es); i32_mul; i32_add;
                      local_get(xs); i32_const(4); i32_add;
                      local_get(start); local_get(new_len); i32_add;
                      i32_const(es); i32_mul; i32_add;
                });
                self.emit_elem_copy(&elem_ty);
                wasm!(self.func, {
                      local_get(new_len); i32_const(1); i32_add; local_set(new_len);
                      br(0);
                    end; end;
                    local_get(dst);
                });
                self.scratch.free_i32(dst);
                self.scratch.free_i32(new_len);
                self.scratch.free_i32(end);
                self.scratch.free_i32(start);
                self.scratch.free_i32(xs);
            }
            "reverse" => {
                let elem_ty = self.list_elem_ty(&args[0].ty);
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
                    i32_const(4); local_get(len); i32_const(elem_size as i32); i32_mul; i32_add;
                    call(self.emitter.rt.alloc); local_set(dst);
                    local_get(dst); local_get(len); i32_store(0);
                    // loop: dst[i] = src[len-1-i]
                    i32_const(0); local_set(i);
                    block_empty; loop_empty;
                      local_get(i); local_get(len); i32_ge_u; br_if(1);
                      // dst addr
                      local_get(dst); i32_const(4); i32_add;
                      local_get(i); i32_const(elem_size as i32); i32_mul; i32_add;
                      // src addr
                      local_get(xs); i32_const(4); i32_add;
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
                    i32_const(4); local_get(len); i32_const(8); i32_mul; i32_add;
                    call(self.emitter.rt.alloc); local_set(dst);
                    local_get(dst); local_get(len); i32_store(0); // dst.len
                    i32_const(0); local_set(i);
                    block_empty; loop_empty;
                      local_get(i); local_get(len); i32_ge_u; br_if(1);
                      local_get(dst); i32_const(4); i32_add;
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
                let elem_ty = self.list_elem_ty(&args[0].ty);
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
                      local_get(xs); i32_const(4); i32_add;
                });
                self.emit_elem_copy(&elem_ty);
                wasm!(self.func, { local_get(result); end; });
                self.scratch.free_i32(result);
                self.scratch.free_i32(xs);
            }
            "last" => {
                let elem_ty = self.list_elem_ty(&args[0].ty);
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
                      local_get(xs); i32_const(4); i32_add;
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
                      local_get(xs); i32_const(4); i32_add;
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
                      local_get(xs); i32_const(4); i32_add;
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
                let inner_list_ty = self.list_elem_ty(&args[0].ty); // List[T]
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
                      local_get(xss); i32_const(4); i32_add;
                      local_get(i); i32_const(4); i32_mul; i32_add; // &xss[i]
                      i32_load(0); // inner list ptr
                      i32_load(0); // inner list len
                      i32_add; local_set(total);
                      local_get(i); i32_const(1); i32_add; local_set(i);
                      br(0);
                    end; end;
                    // Alloc result
                    i32_const(4); local_get(total); i32_const(elem_size as i32); i32_mul; i32_add;
                    call(self.emitter.rt.alloc); local_set(dst);
                    local_get(dst); local_get(total); i32_store(0);
                    // Pass 2: copy
                    i32_const(0); local_set(total); // dst offset (in elements)
                    i32_const(0); local_set(i); // i (outer)
                    block_empty; loop_empty;
                      local_get(i); local_get(xss); i32_load(0); i32_ge_u; br_if(1);
                      // inner = xss[i]
                      local_get(xss); i32_const(4); i32_add;
                      local_get(i); i32_const(4); i32_mul; i32_add;
                      i32_load(0); local_set(inner);
                      // Copy inner elements
                      i32_const(0); local_set(j);
                      block_empty; loop_empty;
                        local_get(j); local_get(inner); i32_load(0); i32_ge_u; br_if(1);
                        // dst[dst_offset + j]
                        local_get(dst); i32_const(4); i32_add;
                        local_get(total); local_get(j); i32_add;
                        i32_const(elem_size as i32); i32_mul; i32_add;
                        // src inner[j]
                        local_get(inner); i32_const(4); i32_add;
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
                // min/max(xs: List[Int]) → Option[Int]
                let is_max = method == "max";
                let xs = self.scratch.alloc_i32();
                let i = self.scratch.alloc_i32();
                let best = self.scratch.alloc_i64();
                let candidate = self.scratch.alloc_i64();
                self.emit_expr(&args[0]);
                wasm!(self.func, {
                    local_set(xs);
                    local_get(xs); i32_load(0); i32_eqz;
                    if_i32; i32_const(0); // none
                    else_;
                      // best = xs[0]
                      local_get(xs); i32_const(4); i32_add; i64_load(0); local_set(best);
                      i32_const(1); local_set(i); // i=1
                      block_empty; loop_empty;
                        local_get(i); local_get(xs); i32_load(0); i32_ge_u; br_if(1);
                        local_get(xs); i32_const(4); i32_add;
                        local_get(i); i32_const(8); i32_mul; i32_add;
                        i64_load(0); local_set(candidate);
                });
                if is_max {
                    wasm!(self.func, {
                        local_get(candidate); local_get(best); i64_gt_s;
                        if_empty; local_get(candidate); local_set(best); end;
                    });
                } else {
                    wasm!(self.func, {
                        local_get(candidate); local_get(best); i64_lt_s;
                        if_empty; local_get(candidate); local_set(best); end;
                    });
                }
                wasm!(self.func, {
                        local_get(i); i32_const(1); i32_add; local_set(i);
                        br(0);
                      end; end;
                      // some(best)
                      i32_const(8); call(self.emitter.rt.alloc); local_set(i);
                      local_get(i); local_get(best); i64_store(0);
                      local_get(i);
                    end;
                });
                self.scratch.free_i64(candidate);
                self.scratch.free_i64(best);
                self.scratch.free_i32(i);
                self.scratch.free_i32(xs);
            }
            "intersperse" => {
                // intersperse(xs, sep) → List[A]: insert sep between elements
                let elem_ty = self.list_elem_ty(&args[0].ty);
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
                      i32_const(4); call(self.emitter.rt.alloc); local_set(dst);
                      local_get(dst); i32_const(0); i32_store(0);
                      local_get(dst);
                    else_;
                      local_get(len); i32_const(2); i32_mul; i32_const(1); i32_sub; local_set(new_len);
                      i32_const(4); local_get(new_len); i32_const(elem_size as i32); i32_mul; i32_add;
                      call(self.emitter.rt.alloc); local_set(dst);
                      local_get(dst); local_get(new_len); i32_store(0);
                      // Fill
                      i32_const(0); local_set(src_i);
                      i32_const(0); local_set(dst_i);
                      block_empty; loop_empty;
                        local_get(src_i); local_get(len); i32_ge_u; br_if(1);
                        // Copy xs[src_i] to dst[dst_i]
                        local_get(dst); i32_const(4); i32_add;
                        local_get(dst_i); i32_const(elem_size as i32); i32_mul; i32_add;
                        local_get(xs); i32_const(4); i32_add;
                        local_get(src_i); i32_const(elem_size as i32); i32_mul; i32_add;
                });
                self.emit_elem_copy(&elem_ty);
                wasm!(self.func, {
                        local_get(dst_i); i32_const(1); i32_add; local_set(dst_i);
                        // Insert sep if not last
                        local_get(src_i); local_get(len); i32_const(1); i32_sub; i32_lt_u;
                        if_empty;
                          local_get(dst); i32_const(4); i32_add;
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
                let a_ty = self.list_elem_ty(&args[0].ty);
                let b_ty = self.list_elem_ty(&args[1].ty);
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
                    i32_const(4); local_get(len); i32_const(4); i32_mul; i32_add;
                    call(self.emitter.rt.alloc); local_set(dst);
                    local_get(dst); local_get(len); i32_store(0);
                    i32_const(0); local_set(i);
                    block_empty; loop_empty;
                      local_get(i); local_get(len); i32_ge_u; br_if(1);
                      // Alloc tuple
                      i32_const(tuple_size as i32); call(self.emitter.rt.alloc); local_set(tup);
                      // Copy a: tuple[0] = xs[i]
                      local_get(tup);
                      local_get(xs); i32_const(4); i32_add;
                      local_get(i); i32_const(a_size as i32); i32_mul; i32_add;
                });
                self.emit_elem_copy(&a_ty);
                // Copy b: tuple[a_size] = ys[i]
                wasm!(self.func, {
                      local_get(tup); i32_const(a_size as i32); i32_add;
                      local_get(ys); i32_const(4); i32_add;
                      local_get(i); i32_const(b_size as i32); i32_mul; i32_add;
                });
                self.emit_elem_copy(&b_ty);
                wasm!(self.func, {
                      // result[i] = tuple_ptr
                      local_get(dst); i32_const(4); i32_add;
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
                let elem_ty = self.list_elem_ty(&args[0].ty);
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
                    i32_const(4); local_get(len); i32_const(es); i32_mul; i32_add;
                    call(self.emitter.rt.alloc); local_set(dst);
                    local_get(dst); local_get(len); i32_store(0);
                    // Copy all elements
                    i32_const(0); local_set(i);
                    block_empty; loop_empty;
                      local_get(i); local_get(len); i32_ge_u; br_if(1);
                      local_get(dst); i32_const(4); i32_add;
                      local_get(i); i32_const(es); i32_mul; i32_add;
                      local_get(xs); i32_const(4); i32_add;
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
                    local_get(dst); i32_const(4); i32_add;
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
                let elem_ty = self.list_elem_ty(&args[0].ty);
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
                    // new_len = old_len + 1
                    i32_const(4); local_get(old_len); i32_const(1); i32_add; i32_const(es); i32_mul; i32_add;
                    call(self.emitter.rt.alloc); local_set(dst);
                    local_get(dst); local_get(old_len); i32_const(1); i32_add; i32_store(0);
                    // Copy [0..idx)
                    i32_const(0); local_set(i);
                    block_empty; loop_empty;
                      local_get(i); local_get(idx); i32_ge_u; br_if(1);
                      local_get(dst); i32_const(4); i32_add;
                      local_get(i); i32_const(es); i32_mul; i32_add;
                      local_get(xs); i32_const(4); i32_add;
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
                    local_get(dst); i32_const(4); i32_add;
                    local_get(idx); i32_const(es); i32_mul; i32_add;
                });
                self.emit_expr(&args[2]);
                self.emit_elem_store(&elem_ty);
                // Copy [idx..old_len)
                wasm!(self.func, {
                    local_get(idx); local_set(i);
                    block_empty; loop_empty;
                      local_get(i); local_get(old_len); i32_ge_u; br_if(1);
                      local_get(dst); i32_const(4); i32_add;
                      local_get(i); i32_const(1); i32_add; i32_const(es); i32_mul; i32_add;
                      local_get(xs); i32_const(4); i32_add;
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
                let elem_ty = self.list_elem_ty(&args[0].ty);
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
                    // new_len = old_len - 1
                    i32_const(4); local_get(old_len); i32_const(1); i32_sub; i32_const(es); i32_mul; i32_add;
                    call(self.emitter.rt.alloc); local_set(dst);
                    local_get(dst); local_get(old_len); i32_const(1); i32_sub; i32_store(0);
                    // Copy [0..idx)
                    i32_const(0); local_set(i);
                    block_empty; loop_empty;
                      local_get(i); local_get(idx); i32_ge_u; br_if(1);
                      local_get(dst); i32_const(4); i32_add;
                      local_get(i); i32_const(es); i32_mul; i32_add;
                      local_get(xs); i32_const(4); i32_add;
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
                      local_get(dst); i32_const(4); i32_add;
                      local_get(i); i32_const(1); i32_sub; i32_const(es); i32_mul; i32_add;
                      local_get(xs); i32_const(4); i32_add;
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
            "unique" => {
                self.emit_list_unique(args);
            }
            "enumerate" => {
                self.emit_list_enumerate(args);
            }
            "get" => {
                // list.get(list, index) → Option[T]
                let elem_ty = if let Ty::Applied(_, a) = &args[0].ty {
                    a.first().cloned().unwrap_or(Ty::Int)
                } else { Ty::Int };
                let elem_size = values::byte_size(&elem_ty);

                let list = self.scratch.alloc_i32();
                let idx = self.scratch.alloc_i32();
                let result = self.scratch.alloc_i32();
                self.emit_expr(&args[0]);
                wasm!(self.func, { local_set(list); });
                self.emit_expr(&args[1]);
                // Index is always Int (i64) — wrap to i32 for memory addressing.
                // Check both explicit type and Unknown (which may actually be Int from TupleIndex etc.)
                if matches!(&args[1].ty, Ty::Int | Ty::Unknown | Ty::TypeVar(_)) {
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
                    i32_const(4);
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
                let elem_ty = if let Ty::Applied(_, a) = &args[0].ty {
                    a.first().cloned().unwrap_or(Ty::Int)
                } else { Ty::Int };
                let elem_size = values::byte_size(&elem_ty);
                let list_ptr = self.scratch.alloc_i32();
                let idx = self.scratch.alloc_i32();
                let result = self.scratch.alloc_i32();
                self.emit_expr(&args[0]);
                wasm!(self.func, { local_set(list_ptr); });
                // Save target to i64 scratch or i32 scratch depending on type
                match values::ty_to_valtype(&elem_ty) {
                    Some(ValType::I64) => {
                        let target = self.scratch.alloc_i64();
                        self.emit_expr(&args[1]);
                        wasm!(self.func, {
                            local_set(target);
                            i32_const(0); local_set(idx);
                            i32_const(0); local_set(result); // result = false
                            block_empty; loop_empty;
                              local_get(idx); local_get(list_ptr); i32_load(0); i32_ge_u; br_if(1);
                              local_get(list_ptr); i32_const(4); i32_add;
                              local_get(idx); i32_const(elem_size as i32); i32_mul; i32_add;
                              i64_load(0);
                              local_get(target); i64_eq;
                              if_empty;
                                i32_const(1); local_set(result); br(2);
                              end;
                              local_get(idx); i32_const(1); i32_add; local_set(idx);
                              br(0);
                            end; end;
                            local_get(result);
                        });
                        self.scratch.free_i64(target);
                    }
                    _ => {
                        // i32 types: String, Option, etc.
                        let target = self.scratch.alloc_i32();
                        self.emit_expr(&args[1]);
                        wasm!(self.func, {
                            local_set(target);
                            i32_const(0); local_set(idx);
                            i32_const(0); local_set(result);
                            block_empty; loop_empty;
                              local_get(idx); local_get(list_ptr); i32_load(0); i32_ge_u; br_if(1);
                              local_get(list_ptr); i32_const(4); i32_add;
                              local_get(idx); i32_const(elem_size as i32); i32_mul; i32_add;
                              i32_load(0);
                              local_get(target);
                        });
                        match &elem_ty {
                            Ty::String => { wasm!(self.func, { call(self.emitter.rt.string.eq); }); }
                            _ => { wasm!(self.func, { i32_eq; }); }
                        }
                        wasm!(self.func, {
                              if_empty;
                                i32_const(1); local_set(result); br(2);
                              end;
                              local_get(idx); i32_const(1); i32_add; local_set(idx);
                              br(0);
                            end; end;
                            local_get(result);
                        });
                        self.scratch.free_i32(target);
                    }
                }
                self.scratch.free_i32(result);
                self.scratch.free_i32(idx);
                self.scratch.free_i32(list_ptr);
            }
            "push" => {
                // push(xs, v) → Unit. Mutates xs in place by reallocating.
                // args[0] = xs (var), args[1] = value
                let elem_ty = self.list_elem_ty(&args[0].ty);
                let elem_size = values::byte_size(&elem_ty);
                let vt = values::ty_to_valtype(&elem_ty).unwrap_or(ValType::I32);
                let old_ptr = self.scratch.alloc_i32();
                let old_len = self.scratch.alloc_i32();
                let new_ptr = self.scratch.alloc_i32();
                let val_scratch = self.scratch.alloc(vt);

                // Evaluate value first (before reading xs, in case of side effects)
                self.emit_expr(&args[1]);
                wasm!(self.func, { local_set(val_scratch); });

                // Read xs pointer
                self.emit_expr(&args[0]);
                wasm!(self.func, { local_set(old_ptr); });
                wasm!(self.func, { local_get(old_ptr); i32_load(0); local_set(old_len); });

                // Allocate new list: 4 + (old_len + 1) * elem_size
                wasm!(self.func, {
                    i32_const(4);
                    local_get(old_len); i32_const(1); i32_add;
                    i32_const(elem_size as i32); i32_mul;
                    i32_add;
                    call(self.emitter.rt.alloc);
                    local_set(new_ptr);
                });

                // Write new len
                wasm!(self.func, {
                    local_get(new_ptr);
                    local_get(old_len); i32_const(1); i32_add;
                    i32_store(0);
                });

                // Copy old data: memory.copy(new_ptr+4, old_ptr+4, old_len * elem_size)
                wasm!(self.func, {
                    local_get(new_ptr); i32_const(4); i32_add;
                    local_get(old_ptr); i32_const(4); i32_add;
                    local_get(old_len); i32_const(elem_size as i32); i32_mul;
                    memory_copy;
                });

                // Write new element at new_ptr + 4 + old_len * elem_size
                wasm!(self.func, {
                    local_get(new_ptr); i32_const(4); i32_add;
                    local_get(old_len); i32_const(elem_size as i32); i32_mul;
                    i32_add;
                    local_get(val_scratch);
                });
                match vt {
                    ValType::I64 => { wasm!(self.func, { i64_store(0); }); }
                    ValType::F64 => { wasm!(self.func, { f64_store(0); }); }
                    _ => { wasm!(self.func, { i32_store(0); }); }
                }

                // Write back to var: xs = new_ptr
                if let almide_ir::IrExprKind::Var { id } = &args[0].kind {
                    if let Some(&local_idx) = self.var_map.get(&id.0) {
                        wasm!(self.func, { local_get(new_ptr); local_set(local_idx); });
                    }
                }

                self.scratch.free(val_scratch, vt);
                self.scratch.free_i32(new_ptr);
                self.scratch.free_i32(old_len);
                self.scratch.free_i32(old_ptr);
            }
            "pop" => {
                // pop(xs) → Option[A]. Removes last element, mutates xs.
                // Option layout: 0 = none, non-zero ptr = some (payload at ptr)
                let elem_ty = self.list_elem_ty(&args[0].ty);
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
                    local_get(list_ptr); i32_const(4); i32_add;
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
