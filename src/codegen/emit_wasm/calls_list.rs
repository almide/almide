//! List stdlib call dispatch for WASM codegen (non-closure functions).

use super::FuncCompiler;
use super::values;
use crate::ir::IrExpr;
use crate::types::Ty;
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
                let s = self.match_i32_base + self.match_depth;
                // Store xs → mem[0], i → mem[4]
                wasm!(self.func, { i32_const(0); });
                self.emit_expr(&args[0]);
                wasm!(self.func, { i32_store(0); i32_const(4); });
                self.emit_expr(&args[1]); // i: i64
                wasm!(self.func, { i32_wrap_i64; i32_store(0); });
                // bounds check: i < 0 || i >= len → default
                match vt {
                    ValType::I64 => {
                        wasm!(self.func, {
                            i32_const(4); i32_load(0); // i
                            i32_const(0); i32_load(0); i32_load(0); // len
                            i32_ge_u;
                            i32_const(4); i32_load(0); i32_const(0); i32_lt_s;
                            i32_or;
                            if_i64;
                        });
                        self.emit_expr(&args[2]); // default
                        wasm!(self.func, {
                            else_;
                              i32_const(0); i32_load(0); i32_const(4); i32_add;
                              i32_const(4); i32_load(0); i32_const(elem_size as i32); i32_mul; i32_add;
                              i64_load(0);
                            end;
                        });
                    }
                    ValType::F64 => {
                        wasm!(self.func, {
                            i32_const(4); i32_load(0);
                            i32_const(0); i32_load(0); i32_load(0);
                            i32_ge_u;
                            i32_const(4); i32_load(0); i32_const(0); i32_lt_s;
                            i32_or;
                            if_f64;
                        });
                        self.emit_expr(&args[2]);
                        wasm!(self.func, {
                            else_;
                              i32_const(0); i32_load(0); i32_const(4); i32_add;
                              i32_const(4); i32_load(0); i32_const(elem_size as i32); i32_mul; i32_add;
                              f64_load(0);
                            end;
                        });
                    }
                    _ => {
                        wasm!(self.func, {
                            i32_const(4); i32_load(0);
                            i32_const(0); i32_load(0); i32_load(0);
                            i32_ge_u;
                            i32_const(4); i32_load(0); i32_const(0); i32_lt_s;
                            i32_or;
                            if_i32;
                        });
                        self.emit_expr(&args[2]);
                        wasm!(self.func, {
                            else_;
                              i32_const(0); i32_load(0); i32_const(4); i32_add;
                              i32_const(4); i32_load(0); i32_const(elem_size as i32); i32_mul; i32_add;
                              i32_load(0);
                            end;
                        });
                    }
                }
            }
            "take" => {
                // take(xs, n) → List[A]: first min(n, len) elements
                // mem[0]=xs, s=n, s+1=new_len, s+2=dst, s+3=i
                let elem_ty = self.list_elem_ty(&args[0].ty);
                let es = values::byte_size(&elem_ty) as i32;
                let s = self.match_i32_base + self.match_depth;
                wasm!(self.func, { i32_const(0); });
                self.emit_expr(&args[0]);
                wasm!(self.func, { i32_store(0); });
                self.emit_expr(&args[1]);
                wasm!(self.func, {
                    i32_wrap_i64; local_set(s);
                    // new_len = min(n, len)
                    local_get(s); i32_const(0); i32_load(0); i32_load(0); i32_lt_u;
                    if_i32; local_get(s); else_; i32_const(0); i32_load(0); i32_load(0); end;
                    local_set(s + 1);
                    i32_const(4); local_get(s + 1); i32_const(es); i32_mul; i32_add;
                    call(self.emitter.rt.alloc); local_set(s + 2);
                    local_get(s + 2); local_get(s + 1); i32_store(0);
                    i32_const(0); local_set(s + 3);
                    block_empty; loop_empty;
                      local_get(s + 3); local_get(s + 1); i32_ge_u; br_if(1);
                      local_get(s + 2); i32_const(4); i32_add;
                      local_get(s + 3); i32_const(es); i32_mul; i32_add;
                      i32_const(0); i32_load(0); i32_const(4); i32_add;
                      local_get(s + 3); i32_const(es); i32_mul; i32_add;
                });
                self.emit_elem_copy(&elem_ty);
                wasm!(self.func, {
                      local_get(s + 3); i32_const(1); i32_add; local_set(s + 3);
                      br(0);
                    end; end;
                    local_get(s + 2);
                });
            }
            "drop" => {
                // drop(xs, n): skip first n
                let elem_ty = self.list_elem_ty(&args[0].ty);
                let es = values::byte_size(&elem_ty) as i32;
                let s = self.match_i32_base + self.match_depth;
                wasm!(self.func, { i32_const(0); });
                self.emit_expr(&args[0]);
                wasm!(self.func, { i32_store(0); });
                self.emit_expr(&args[1]);
                wasm!(self.func, {
                    i32_wrap_i64; local_set(s);
                    // start = min(n, len)
                    local_get(s); i32_const(0); i32_load(0); i32_load(0); i32_lt_u;
                    if_i32; local_get(s); else_; i32_const(0); i32_load(0); i32_load(0); end;
                    local_set(s);
                    // new_len = len - start
                    i32_const(0); i32_load(0); i32_load(0); local_get(s); i32_sub;
                    local_set(s + 1);
                    i32_const(4); local_get(s + 1); i32_const(es); i32_mul; i32_add;
                    call(self.emitter.rt.alloc); local_set(s + 2);
                    local_get(s + 2); local_get(s + 1); i32_store(0);
                    i32_const(0); local_set(s + 3);
                    block_empty; loop_empty;
                      local_get(s + 3); local_get(s + 1); i32_ge_u; br_if(1);
                      local_get(s + 2); i32_const(4); i32_add;
                      local_get(s + 3); i32_const(es); i32_mul; i32_add;
                      i32_const(0); i32_load(0); i32_const(4); i32_add;
                      local_get(s); local_get(s + 3); i32_add;
                      i32_const(es); i32_mul; i32_add;
                });
                self.emit_elem_copy(&elem_ty);
                wasm!(self.func, {
                      local_get(s + 3); i32_const(1); i32_add; local_set(s + 3);
                      br(0);
                    end; end;
                    local_get(s + 2);
                });
            }
            "slice" => {
                // slice(xs, start, end) → List[A]
                let elem_ty = self.list_elem_ty(&args[0].ty);
                let es = values::byte_size(&elem_ty) as i32;
                let s = self.match_i32_base + self.match_depth;
                // mem[0]=xs
                wasm!(self.func, { i32_const(0); });
                self.emit_expr(&args[0]);
                wasm!(self.func, { i32_store(0); });
                self.emit_expr(&args[1]); // start
                wasm!(self.func, { i32_wrap_i64; local_set(s); });
                self.emit_expr(&args[2]); // end
                wasm!(self.func, {
                    i32_wrap_i64; local_set(s + 1);
                    local_get(s + 1); local_get(s); i32_sub; local_set(s + 2); // new_len
                    i32_const(4); local_get(s + 2); i32_const(es); i32_mul; i32_add;
                    call(self.emitter.rt.alloc); local_set(s + 3);
                    local_get(s + 3); local_get(s + 2); i32_store(0);
                    // copy loop
                    i32_const(0); local_set(s + 2); // reuse as i
                    block_empty; loop_empty;
                      local_get(s + 2); local_get(s + 1); local_get(s); i32_sub; i32_ge_u; br_if(1);
                      local_get(s + 3); i32_const(4); i32_add;
                      local_get(s + 2); i32_const(es); i32_mul; i32_add;
                      i32_const(0); i32_load(0); i32_const(4); i32_add;
                      local_get(s); local_get(s + 2); i32_add;
                      i32_const(es); i32_mul; i32_add;
                });
                self.emit_elem_copy(&elem_ty);
                wasm!(self.func, {
                      local_get(s + 2); i32_const(1); i32_add; local_set(s + 2);
                      br(0);
                    end; end;
                    local_get(s + 3);
                });
            }
            "reverse" => {
                let elem_ty = self.list_elem_ty(&args[0].ty);
                let elem_size = values::byte_size(&elem_ty);
                let s = self.match_i32_base + self.match_depth;
                wasm!(self.func, { i32_const(0); });
                self.emit_expr(&args[0]);
                wasm!(self.func, {
                    i32_store(0);
                    i32_const(0); i32_load(0); i32_load(0); local_set(s); // len
                    // alloc dst
                    i32_const(4); local_get(s); i32_const(elem_size as i32); i32_mul; i32_add;
                    call(self.emitter.rt.alloc); local_set(s + 1);
                    local_get(s + 1); local_get(s); i32_store(0);
                    // loop: dst[i] = src[len-1-i]
                    i32_const(0); local_set(s + 2); // i
                    block_empty; loop_empty;
                      local_get(s + 2); local_get(s); i32_ge_u; br_if(1);
                      // dst addr
                      local_get(s + 1); i32_const(4); i32_add;
                      local_get(s + 2); i32_const(elem_size as i32); i32_mul; i32_add;
                      // src addr
                      i32_const(0); i32_load(0); i32_const(4); i32_add;
                      local_get(s); i32_const(1); i32_sub; local_get(s + 2); i32_sub;
                      i32_const(elem_size as i32); i32_mul; i32_add;
                });
                // Copy elem_size bytes
                self.emit_elem_copy(&elem_ty);
                wasm!(self.func, {
                      local_get(s + 2); i32_const(1); i32_add; local_set(s + 2);
                      br(0);
                    end; end;
                    local_get(s + 1);
                });
            }
            "range" => {
                // range(start, end) → List[Int]
                // mem[0]=start(i32), mem[4]=len(i32), s=dst, s+1=i
                let s = self.match_i32_base + self.match_depth;
                wasm!(self.func, { i32_const(0); });
                self.emit_expr(&args[0]);
                wasm!(self.func, { i32_wrap_i64; i32_store(0); }); // mem[0] = start
                self.emit_expr(&args[1]);
                wasm!(self.func, {
                    i32_wrap_i64;
                    i32_const(0); i32_load(0); i32_sub; // len = end - start
                    local_set(s);
                    local_get(s); i32_const(0); i32_lt_s;
                    if_empty; i32_const(0); local_set(s); end; // clamp to 0
                    // mem[4] = len
                    i32_const(4); local_get(s); i32_store(0);
                    // alloc
                    i32_const(4); local_get(s); i32_const(8); i32_mul; i32_add;
                    call(self.emitter.rt.alloc); local_set(s);
                    local_get(s); i32_const(4); i32_load(0); i32_store(0); // dst.len
                    i32_const(0); local_set(s + 1); // i
                    block_empty; loop_empty;
                      local_get(s + 1); i32_const(4); i32_load(0); i32_ge_u; br_if(1);
                      local_get(s); i32_const(4); i32_add;
                      local_get(s + 1); i32_const(8); i32_mul; i32_add;
                      i32_const(0); i32_load(0); local_get(s + 1); i32_add;
                      i64_extend_i32_s;
                      i64_store(0);
                      local_get(s + 1); i32_const(1); i32_add; local_set(s + 1);
                      br(0);
                    end; end;
                    local_get(s);
                });
            }
            "first" => {
                // first(xs) → Option[A]: xs[0] or none
                let elem_ty = self.list_elem_ty(&args[0].ty);
                let elem_size = values::byte_size(&elem_ty);
                let s = self.match_i32_base + self.match_depth;
                self.emit_expr(&args[0]);
                wasm!(self.func, {
                    local_set(s);
                    local_get(s); i32_load(0); i32_eqz;
                    if_i32; i32_const(0); // none
                    else_;
                      i32_const(elem_size as i32); call(self.emitter.rt.alloc); local_set(s + 1);
                      local_get(s + 1);
                      local_get(s); i32_const(4); i32_add;
                });
                self.emit_elem_copy(&elem_ty);
                wasm!(self.func, { local_get(s + 1); end; });
            }
            "last" => {
                let elem_ty = self.list_elem_ty(&args[0].ty);
                let elem_size = values::byte_size(&elem_ty);
                let s = self.match_i32_base + self.match_depth;
                self.emit_expr(&args[0]);
                wasm!(self.func, {
                    local_set(s);
                    local_get(s); i32_load(0); i32_eqz;
                    if_i32; i32_const(0);
                    else_;
                      i32_const(elem_size as i32); call(self.emitter.rt.alloc); local_set(s + 1);
                      local_get(s + 1);
                      // src = xs + 4 + (len-1) * elem_size
                      local_get(s); i32_const(4); i32_add;
                      local_get(s); i32_load(0); i32_const(1); i32_sub;
                      i32_const(elem_size as i32); i32_mul; i32_add;
                });
                self.emit_elem_copy(&elem_ty);
                wasm!(self.func, { local_get(s + 1); end; });
            }
            "is_empty" => {
                self.emit_expr(&args[0]);
                wasm!(self.func, { i32_load(0); i32_eqz; });
            }
            "sum" => {
                // sum(xs: List[Int]) → Int
                let s = self.match_i32_base + self.match_depth;
                self.emit_expr(&args[0]);
                let s64 = self.match_i64_base + self.match_depth;
                wasm!(self.func, {
                    local_set(s); // xs
                    i64_const(0); local_set(s64); // acc
                    i32_const(0); local_set(s + 1); // i
                    block_empty; loop_empty;
                      local_get(s + 1); local_get(s); i32_load(0); i32_ge_u; br_if(1);
                      local_get(s64);
                      local_get(s); i32_const(4); i32_add;
                      local_get(s + 1); i32_const(8); i32_mul; i32_add;
                      i64_load(0);
                      i64_add; local_set(s64);
                      local_get(s + 1); i32_const(1); i32_add; local_set(s + 1);
                      br(0);
                    end; end;
                    local_get(s64);
                });
            }
            "product" => {
                let s = self.match_i32_base + self.match_depth;
                self.emit_expr(&args[0]);
                let s64 = self.match_i64_base + self.match_depth;
                wasm!(self.func, {
                    local_set(s);
                    i64_const(1); local_set(s64);
                    i32_const(0); local_set(s + 1);
                    block_empty; loop_empty;
                      local_get(s + 1); local_get(s); i32_load(0); i32_ge_u; br_if(1);
                      local_get(s64);
                      local_get(s); i32_const(4); i32_add;
                      local_get(s + 1); i32_const(8); i32_mul; i32_add;
                      i64_load(0);
                      i64_mul; local_set(s64);
                      local_get(s + 1); i32_const(1); i32_add; local_set(s + 1);
                      br(0);
                    end; end;
                    local_get(s64);
                });
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
                let s = self.match_i32_base + self.match_depth;
                self.emit_expr(&args[0]);
                wasm!(self.func, {
                    local_set(s); // xss
                    // Pass 1: count total elements
                    i32_const(0); local_set(s + 1); // total
                    i32_const(0); local_set(s + 2); // i
                    block_empty; loop_empty;
                      local_get(s + 2); local_get(s); i32_load(0); i32_ge_u; br_if(1);
                      local_get(s + 1);
                      local_get(s); i32_const(4); i32_add;
                      local_get(s + 2); i32_const(4); i32_mul; i32_add; // &xss[i]
                      i32_load(0); // inner list ptr
                      i32_load(0); // inner list len
                      i32_add; local_set(s + 1);
                      local_get(s + 2); i32_const(1); i32_add; local_set(s + 2);
                      br(0);
                    end; end;
                    // Alloc result
                    i32_const(4); local_get(s + 1); i32_const(elem_size as i32); i32_mul; i32_add;
                    call(self.emitter.rt.alloc); local_set(s + 3);
                    local_get(s + 3); local_get(s + 1); i32_store(0);
                    // Pass 2: copy
                    i32_const(0); local_set(s + 1); // dst offset (in elements)
                    i32_const(0); local_set(s + 2); // i (outer)
                    block_empty; loop_empty;
                      local_get(s + 2); local_get(s); i32_load(0); i32_ge_u; br_if(1);
                      // inner = xss[i]
                      local_get(s); i32_const(4); i32_add;
                      local_get(s + 2); i32_const(4); i32_mul; i32_add;
                      i32_load(0); local_set(s + 4); // inner list ptr
                      // Copy inner elements
                      i32_const(0); local_set(s + 5); // j
                      block_empty; loop_empty;
                        local_get(s + 5); local_get(s + 4); i32_load(0); i32_ge_u; br_if(1);
                        // dst[dst_offset + j]
                        local_get(s + 3); i32_const(4); i32_add;
                        local_get(s + 1); local_get(s + 5); i32_add;
                        i32_const(elem_size as i32); i32_mul; i32_add;
                        // src inner[j]
                        local_get(s + 4); i32_const(4); i32_add;
                        local_get(s + 5); i32_const(elem_size as i32); i32_mul; i32_add;
                });
                self.emit_elem_copy(&elem_ty);
                wasm!(self.func, {
                        local_get(s + 5); i32_const(1); i32_add; local_set(s + 5);
                        br(0);
                      end; end;
                      local_get(s + 1); local_get(s + 4); i32_load(0); i32_add; local_set(s + 1);
                      local_get(s + 2); i32_const(1); i32_add; local_set(s + 2);
                      br(0);
                    end; end;
                    local_get(s + 3);
                });
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
                let s = self.match_i32_base + self.match_depth;
                let s64 = self.match_i64_base + self.match_depth;
                self.emit_expr(&args[0]);
                wasm!(self.func, {
                    local_set(s);
                    local_get(s); i32_load(0); i32_eqz;
                    if_i32; i32_const(0); // none
                    else_;
                      // best = xs[0]
                      local_get(s); i32_const(4); i32_add; i64_load(0); local_set(s64);
                      i32_const(1); local_set(s + 1); // i=1
                      block_empty; loop_empty;
                        local_get(s + 1); local_get(s); i32_load(0); i32_ge_u; br_if(1);
                        local_get(s); i32_const(4); i32_add;
                        local_get(s + 1); i32_const(8); i32_mul; i32_add;
                        i64_load(0); local_set(s64 + 1); // candidate
                });
                if is_max {
                    wasm!(self.func, {
                        local_get(s64 + 1); local_get(s64); i64_gt_s;
                        if_empty; local_get(s64 + 1); local_set(s64); end;
                    });
                } else {
                    wasm!(self.func, {
                        local_get(s64 + 1); local_get(s64); i64_lt_s;
                        if_empty; local_get(s64 + 1); local_set(s64); end;
                    });
                }
                wasm!(self.func, {
                        local_get(s + 1); i32_const(1); i32_add; local_set(s + 1);
                        br(0);
                      end; end;
                      // some(best)
                      i32_const(8); call(self.emitter.rt.alloc); local_set(s + 1);
                      local_get(s + 1); local_get(s64); i64_store(0);
                      local_get(s + 1);
                    end;
                });
            }
            "intersperse" => {
                // intersperse(xs, sep) → List[A]: insert sep between elements
                let elem_ty = self.list_elem_ty(&args[0].ty);
                let elem_size = values::byte_size(&elem_ty);
                let s = self.match_i32_base + self.match_depth;
                wasm!(self.func, { i32_const(0); });
                self.emit_expr(&args[0]);
                wasm!(self.func, { i32_store(0); i32_const(4); });
                self.emit_expr(&args[1]); // sep
                self.emit_store_at(&elem_ty, 0); // mem[4] = sep (type-aware)
                wasm!(self.func, {
                    i32_const(0); i32_load(0); i32_load(0); local_set(s); // len
                    // new_len = max(0, 2*len - 1)
                    local_get(s); i32_eqz;
                    if_i32;
                      // empty list
                      i32_const(4); call(self.emitter.rt.alloc); local_set(s + 1);
                      local_get(s + 1); i32_const(0); i32_store(0);
                      local_get(s + 1);
                    else_;
                      local_get(s); i32_const(2); i32_mul; i32_const(1); i32_sub; local_set(s + 2); // new_len
                      i32_const(4); local_get(s + 2); i32_const(elem_size as i32); i32_mul; i32_add;
                      call(self.emitter.rt.alloc); local_set(s + 1);
                      local_get(s + 1); local_get(s + 2); i32_store(0);
                      // Fill
                      i32_const(0); local_set(s + 3); // src_i
                      i32_const(0); local_set(s + 4); // dst_i
                      block_empty; loop_empty;
                        local_get(s + 3); local_get(s); i32_ge_u; br_if(1);
                        // Copy xs[src_i] to dst[dst_i]
                        local_get(s + 1); i32_const(4); i32_add;
                        local_get(s + 4); i32_const(elem_size as i32); i32_mul; i32_add;
                        i32_const(0); i32_load(0); i32_const(4); i32_add;
                        local_get(s + 3); i32_const(elem_size as i32); i32_mul; i32_add;
                });
                self.emit_elem_copy(&elem_ty);
                wasm!(self.func, {
                        local_get(s + 4); i32_const(1); i32_add; local_set(s + 4);
                        // Insert sep if not last
                        local_get(s + 3); local_get(s); i32_const(1); i32_sub; i32_lt_u;
                        if_empty;
                          local_get(s + 1); i32_const(4); i32_add;
                          local_get(s + 4); i32_const(elem_size as i32); i32_mul; i32_add;
                          i32_const(4);
                });
                self.emit_load_at(&elem_ty, 0); // load sep from mem[4]
                self.emit_store_at(&elem_ty, 0);
                wasm!(self.func, {
                          local_get(s + 4); i32_const(1); i32_add; local_set(s + 4);
                        end;
                        local_get(s + 3); i32_const(1); i32_add; local_set(s + 3);
                        br(0);
                      end; end;
                      local_get(s + 1);
                    end;
                });
            }
            "zip" => {
                // zip(xs, ys) → List[(A, B)]
                // Each tuple is heap-allocated: [a_value, b_value]
                let a_ty = self.list_elem_ty(&args[0].ty);
                let b_ty = self.list_elem_ty(&args[1].ty);
                let a_size = values::byte_size(&a_ty);
                let b_size = values::byte_size(&b_ty);
                let tuple_size = a_size + b_size;
                let s = self.match_i32_base + self.match_depth;
                // mem[0]=xs, mem[4]=ys
                wasm!(self.func, { i32_const(0); });
                self.emit_expr(&args[0]);
                wasm!(self.func, { i32_store(0); i32_const(4); });
                self.emit_expr(&args[1]);
                wasm!(self.func, {
                    i32_store(0);
                    // len = min(xs.len, ys.len)
                    i32_const(0); i32_load(0); i32_load(0);
                    i32_const(4); i32_load(0); i32_load(0);
                    i32_lt_u;
                    if_i32;
                      i32_const(0); i32_load(0); i32_load(0);
                    else_;
                      i32_const(4); i32_load(0); i32_load(0);
                    end;
                    local_set(s); // len
                    // Alloc result: list of ptrs to tuples
                    i32_const(4); local_get(s); i32_const(4); i32_mul; i32_add;
                    call(self.emitter.rt.alloc); local_set(s + 1);
                    local_get(s + 1); local_get(s); i32_store(0);
                    i32_const(0); local_set(s + 2); // i
                    block_empty; loop_empty;
                      local_get(s + 2); local_get(s); i32_ge_u; br_if(1);
                      // Alloc tuple
                      i32_const(tuple_size as i32); call(self.emitter.rt.alloc); local_set(s + 3);
                      // Copy a: tuple[0] = xs[i]
                      local_get(s + 3);
                      i32_const(0); i32_load(0); i32_const(4); i32_add;
                      local_get(s + 2); i32_const(a_size as i32); i32_mul; i32_add;
                });
                self.emit_elem_copy(&a_ty);
                // Copy b: tuple[a_size] = ys[i]
                wasm!(self.func, {
                      local_get(s + 3); i32_const(a_size as i32); i32_add;
                      i32_const(4); i32_load(0); i32_const(4); i32_add;
                      local_get(s + 2); i32_const(b_size as i32); i32_mul; i32_add;
                });
                self.emit_elem_copy(&b_ty);
                wasm!(self.func, {
                      // result[i] = tuple_ptr
                      local_get(s + 1); i32_const(4); i32_add;
                      local_get(s + 2); i32_const(4); i32_mul; i32_add;
                      local_get(s + 3); i32_store(0);
                      local_get(s + 2); i32_const(1); i32_add; local_set(s + 2);
                      br(0);
                    end; end;
                    local_get(s + 1);
                });
            }
            "set" => {
                // set(xs, i, val) → List[A]: copy + replace element at i
                let elem_ty = self.list_elem_ty(&args[0].ty);
                let es = values::byte_size(&elem_ty) as i32;
                let s = self.match_i32_base + self.match_depth;
                // mem[0]=xs
                wasm!(self.func, { i32_const(0); });
                self.emit_expr(&args[0]);
                wasm!(self.func, { i32_store(0); });
                self.emit_expr(&args[1]); // i: i64
                wasm!(self.func, {
                    i32_wrap_i64; local_set(s); // s = idx
                    i32_const(0); i32_load(0); i32_load(0); local_set(s + 1); // len
                    // Alloc copy
                    i32_const(4); local_get(s + 1); i32_const(es); i32_mul; i32_add;
                    call(self.emitter.rt.alloc); local_set(s + 2);
                    local_get(s + 2); local_get(s + 1); i32_store(0);
                    // Copy all elements
                    i32_const(0); local_set(s + 3);
                    block_empty; loop_empty;
                      local_get(s + 3); local_get(s + 1); i32_ge_u; br_if(1);
                      local_get(s + 2); i32_const(4); i32_add;
                      local_get(s + 3); i32_const(es); i32_mul; i32_add;
                      i32_const(0); i32_load(0); i32_const(4); i32_add;
                      local_get(s + 3); i32_const(es); i32_mul; i32_add;
                });
                self.emit_elem_copy(&elem_ty);
                wasm!(self.func, {
                      local_get(s + 3); i32_const(1); i32_add; local_set(s + 3);
                      br(0);
                    end; end;
                });
                // Overwrite dst[idx] with val
                wasm!(self.func, {
                    local_get(s + 2); i32_const(4); i32_add;
                    local_get(s); i32_const(es); i32_mul; i32_add;
                });
                self.emit_expr(&args[2]);
                self.emit_elem_store(&elem_ty);
                wasm!(self.func, { local_get(s + 2); });
            }
            "insert" => {
                // insert(xs, i, val) → List[A]: copy with element inserted at i
                let elem_ty = self.list_elem_ty(&args[0].ty);
                let es = values::byte_size(&elem_ty) as i32;
                let s = self.match_i32_base + self.match_depth;
                wasm!(self.func, { i32_const(0); });
                self.emit_expr(&args[0]);
                wasm!(self.func, { i32_store(0); });
                self.emit_expr(&args[1]);
                wasm!(self.func, {
                    i32_wrap_i64; local_set(s); // idx
                    i32_const(0); i32_load(0); i32_load(0); local_set(s + 1); // old_len
                    // new_len = old_len + 1
                    i32_const(4); local_get(s + 1); i32_const(1); i32_add; i32_const(es); i32_mul; i32_add;
                    call(self.emitter.rt.alloc); local_set(s + 2);
                    local_get(s + 2); local_get(s + 1); i32_const(1); i32_add; i32_store(0);
                    // Copy [0..idx)
                    i32_const(0); local_set(s + 3);
                    block_empty; loop_empty;
                      local_get(s + 3); local_get(s); i32_ge_u; br_if(1);
                      local_get(s + 2); i32_const(4); i32_add;
                      local_get(s + 3); i32_const(es); i32_mul; i32_add;
                      i32_const(0); i32_load(0); i32_const(4); i32_add;
                      local_get(s + 3); i32_const(es); i32_mul; i32_add;
                });
                self.emit_elem_copy(&elem_ty);
                wasm!(self.func, {
                      local_get(s + 3); i32_const(1); i32_add; local_set(s + 3);
                      br(0);
                    end; end;
                });
                // Insert val at idx
                wasm!(self.func, {
                    local_get(s + 2); i32_const(4); i32_add;
                    local_get(s); i32_const(es); i32_mul; i32_add;
                });
                self.emit_expr(&args[2]);
                self.emit_elem_store(&elem_ty);
                // Copy [idx..old_len)
                wasm!(self.func, {
                    local_get(s); local_set(s + 3);
                    block_empty; loop_empty;
                      local_get(s + 3); local_get(s + 1); i32_ge_u; br_if(1);
                      local_get(s + 2); i32_const(4); i32_add;
                      local_get(s + 3); i32_const(1); i32_add; i32_const(es); i32_mul; i32_add;
                      i32_const(0); i32_load(0); i32_const(4); i32_add;
                      local_get(s + 3); i32_const(es); i32_mul; i32_add;
                });
                self.emit_elem_copy(&elem_ty);
                wasm!(self.func, {
                      local_get(s + 3); i32_const(1); i32_add; local_set(s + 3);
                      br(0);
                    end; end;
                    local_get(s + 2);
                });
            }
            "remove_at" => {
                // remove_at(xs, i) → List[A]
                let elem_ty = self.list_elem_ty(&args[0].ty);
                let es = values::byte_size(&elem_ty) as i32;
                let s = self.match_i32_base + self.match_depth;
                wasm!(self.func, { i32_const(0); });
                self.emit_expr(&args[0]);
                wasm!(self.func, { i32_store(0); });
                self.emit_expr(&args[1]);
                wasm!(self.func, {
                    i32_wrap_i64; local_set(s); // idx
                    i32_const(0); i32_load(0); i32_load(0); local_set(s + 1); // old_len
                    // new_len = old_len - 1
                    i32_const(4); local_get(s + 1); i32_const(1); i32_sub; i32_const(es); i32_mul; i32_add;
                    call(self.emitter.rt.alloc); local_set(s + 2);
                    local_get(s + 2); local_get(s + 1); i32_const(1); i32_sub; i32_store(0);
                    // Copy [0..idx)
                    i32_const(0); local_set(s + 3);
                    block_empty; loop_empty;
                      local_get(s + 3); local_get(s); i32_ge_u; br_if(1);
                      local_get(s + 2); i32_const(4); i32_add;
                      local_get(s + 3); i32_const(es); i32_mul; i32_add;
                      i32_const(0); i32_load(0); i32_const(4); i32_add;
                      local_get(s + 3); i32_const(es); i32_mul; i32_add;
                });
                self.emit_elem_copy(&elem_ty);
                wasm!(self.func, {
                      local_get(s + 3); i32_const(1); i32_add; local_set(s + 3);
                      br(0);
                    end; end;
                });
                // Copy [idx+1..old_len)
                wasm!(self.func, {
                    local_get(s); i32_const(1); i32_add; local_set(s + 3);
                    block_empty; loop_empty;
                      local_get(s + 3); local_get(s + 1); i32_ge_u; br_if(1);
                      local_get(s + 2); i32_const(4); i32_add;
                      local_get(s + 3); i32_const(1); i32_sub; i32_const(es); i32_mul; i32_add;
                      i32_const(0); i32_load(0); i32_const(4); i32_add;
                      local_get(s + 3); i32_const(es); i32_mul; i32_add;
                });
                self.emit_elem_copy(&elem_ty);
                wasm!(self.func, {
                      local_get(s + 3); i32_const(1); i32_add; local_set(s + 3);
                      br(0);
                    end; end;
                    local_get(s + 2);
                });
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

                // mem[0]=list, mem[4]=idx(i32)
                wasm!(self.func, { i32_const(0); });
                self.emit_expr(&args[0]);
                wasm!(self.func, {
                    i32_store(0);
                    i32_const(4);
                });
                self.emit_expr(&args[1]);
                if matches!(&args[1].ty, Ty::Int) {
                    wasm!(self.func, { i32_wrap_i64; });
                }
                wasm!(self.func, {
                    i32_store(0);
                    // bounds: idx >= len → none(0)
                    i32_const(4);
                    i32_load(0); // idx
                    i32_const(0);
                    i32_load(0); // list
                    i32_load(0); // len
                    i32_ge_u;
                    if_i32;
                    i32_const(0); // none
                    else_;
                    // alloc → mem[8]
                    i32_const(8);
                    i32_const(elem_size as i32);
                    call(self.emitter.rt.alloc);
                    i32_store(0);
                    // dst=mem[8], src=list+4+idx*elem_size
                    i32_const(8);
                    i32_load(0); // dst
                    i32_const(0);
                    i32_load(0); // list
                    i32_const(4);
                    i32_add;
                    i32_const(4);
                    i32_load(0); // idx
                    i32_const(elem_size as i32);
                    i32_mul;
                    i32_add;
                });
                self.emit_load_at(&elem_ty, 0); // load elem
                self.emit_store_at(&elem_ty, 0); // store at dst
                wasm!(self.func, {
                    i32_const(8);
                    i32_load(0); // return ptr
                    end;
                });
            }
            "contains" => {
                // list.contains(list, elem) -> Bool (i32)
                // Uses only scratch locals — no mem[] to avoid nested call conflicts
                let elem_ty = if let Ty::Applied(_, a) = &args[0].ty {
                    a.first().cloned().unwrap_or(Ty::Int)
                } else { Ty::Int };
                let elem_size = values::byte_size(&elem_ty);
                let s = self.match_i32_base + self.match_depth;
                // s=list_ptr, s+1=idx, s+2=result
                self.emit_expr(&args[0]);
                wasm!(self.func, { local_set(s); });
                // Save target to i64 scratch or i32 scratch depending on type
                match values::ty_to_valtype(&elem_ty) {
                    Some(ValType::I64) => {
                        let t = self.match_i64_base + self.match_depth;
                        self.emit_expr(&args[1]);
                        wasm!(self.func, {
                            local_set(t); // target in i64 local
                            i32_const(0); local_set(s + 1); // idx
                            i32_const(0); local_set(s + 2); // result = false
                            block_empty; loop_empty;
                              local_get(s + 1); local_get(s); i32_load(0); i32_ge_u; br_if(1);
                              local_get(s); i32_const(4); i32_add;
                              local_get(s + 1); i32_const(elem_size as i32); i32_mul; i32_add;
                              i64_load(0);
                              local_get(t); i64_eq;
                              if_empty;
                                i32_const(1); local_set(s + 2); br(2);
                              end;
                              local_get(s + 1); i32_const(1); i32_add; local_set(s + 1);
                              br(0);
                            end; end;
                            local_get(s + 2);
                        });
                    }
                    _ => {
                        // i32 types: String, Option, etc.
                        self.emit_expr(&args[1]);
                        wasm!(self.func, {
                            local_set(s + 3); // target in i32 local
                            i32_const(0); local_set(s + 1);
                            i32_const(0); local_set(s + 2);
                            block_empty; loop_empty;
                              local_get(s + 1); local_get(s); i32_load(0); i32_ge_u; br_if(1);
                              local_get(s); i32_const(4); i32_add;
                              local_get(s + 1); i32_const(elem_size as i32); i32_mul; i32_add;
                              i32_load(0);
                              local_get(s + 3);
                        });
                        match &elem_ty {
                            Ty::String => { wasm!(self.func, { call(self.emitter.rt.string.eq); }); }
                            _ => { wasm!(self.func, { i32_eq; }); }
                        }
                        wasm!(self.func, {
                              if_empty;
                                i32_const(1); local_set(s + 2); br(2);
                              end;
                              local_get(s + 1); i32_const(1); i32_add; local_set(s + 1);
                              br(0);
                            end; end;
                            local_get(s + 2);
                        });
                    }
                }
            }
            _ => return self.emit_list_closure_call(method, args),
        }
        true
    }
}
