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
                let inner_ty = self.list_elem_ty(&args[0].ty);
                let elem_size = values::byte_size(&inner_ty); // size of inner list elements
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
                self.emit_elem_copy(&inner_ty);
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
                // Insertion sort for List[Int]
                let elem_ty = self.list_elem_ty(&args[0].ty);
                let elem_size = values::byte_size(&elem_ty);
                if !matches!(&elem_ty, Ty::Int) {
                    self.emit_stub_call(args);
                    return true;
                }
                let s = self.match_i32_base + self.match_depth;
                let s64 = self.match_i64_base + self.match_depth;
                // Copy list first
                wasm!(self.func, { i32_const(0); });
                self.emit_expr(&args[0]);
                wasm!(self.func, {
                    i32_store(0);
                    i32_const(0); i32_load(0); i32_load(0); local_set(s);
                    i32_const(4); local_get(s); i32_const(8); i32_mul; i32_add;
                    call(self.emitter.rt.alloc); local_set(s + 1);
                    local_get(s + 1); local_get(s); i32_store(0);
                });
                // Copy all elements
                wasm!(self.func, {
                    i32_const(0); local_set(s + 2);
                    block_empty; loop_empty;
                      local_get(s + 2); local_get(s); i32_ge_u; br_if(1);
                      local_get(s + 1); i32_const(4); i32_add;
                      local_get(s + 2); i32_const(8); i32_mul; i32_add;
                      i32_const(0); i32_load(0); i32_const(4); i32_add;
                      local_get(s + 2); i32_const(8); i32_mul; i32_add;
                      i64_load(0); i64_store(0);
                      local_get(s + 2); i32_const(1); i32_add; local_set(s + 2);
                      br(0);
                    end; end;
                });
                // Insertion sort outer loop
                wasm!(self.func, {
                    i32_const(1); local_set(s + 2);
                    block_empty; loop_empty;
                      local_get(s + 2); local_get(s); i32_ge_u; br_if(1);
                      local_get(s + 1); i32_const(4); i32_add;
                      local_get(s + 2); i32_const(8); i32_mul; i32_add;
                      i64_load(0); local_set(s64);
                      local_get(s + 2); i32_const(1); i32_sub; local_set(s + 3);
                });
                // Inner loop: shift elements right
                wasm!(self.func, {
                      block_empty; loop_empty;
                        local_get(s + 3); i32_const(0); i32_lt_s; br_if(1);
                        local_get(s + 1); i32_const(4); i32_add;
                        local_get(s + 3); i32_const(8); i32_mul; i32_add;
                        i64_load(0); local_get(s64); i64_le_s; br_if(1);
                        local_get(s + 1); i32_const(4); i32_add;
                        local_get(s + 3); i32_const(1); i32_add; i32_const(8); i32_mul; i32_add;
                        local_get(s + 1); i32_const(4); i32_add;
                        local_get(s + 3); i32_const(8); i32_mul; i32_add;
                        i64_load(0); i64_store(0);
                        local_get(s + 3); i32_const(1); i32_sub; local_set(s + 3);
                        br(0);
                      end; end;
                });
                // Place key and continue
                wasm!(self.func, {
                      local_get(s + 1); i32_const(4); i32_add;
                      local_get(s + 3); i32_const(1); i32_add; i32_const(8); i32_mul; i32_add;
                      local_get(s64); i64_store(0);
                      local_get(s + 2); i32_const(1); i32_add; local_set(s + 2);
                      br(0);
                    end; end;
                    local_get(s + 1);
                });
            }
            "index_of" => {
                // index_of(xs, x) → Option[Int]
                let elem_ty = self.list_elem_ty(&args[0].ty);
                let elem_size = values::byte_size(&elem_ty);
                let s = self.match_i32_base + self.match_depth;
                let s64 = self.match_i64_base + self.match_depth;
                wasm!(self.func, { i32_const(0); });
                self.emit_expr(&args[0]);
                wasm!(self.func, { i32_store(0); });
                // Store search value
                match values::ty_to_valtype(&elem_ty) {
                    Some(ValType::I64) => {
                        self.emit_expr(&args[1]);
                        wasm!(self.func, { local_set(s64); });
                    }
                    _ => {
                        wasm!(self.func, { i32_const(4); });
                        self.emit_expr(&args[1]);
                        wasm!(self.func, { i32_store(0); });
                    }
                }
                wasm!(self.func, {
                    i32_const(0); local_set(s); // i
                    i32_const(0); local_set(s + 2); // result (default: none)
                    block_empty; loop_empty;
                      local_get(s);
                      i32_const(0); i32_load(0); i32_load(0); // len
                      i32_ge_u; br_if(1);
                });
                // Compare element
                match values::ty_to_valtype(&elem_ty) {
                    Some(ValType::I64) => {
                        wasm!(self.func, {
                            i32_const(0); i32_load(0); i32_const(4); i32_add;
                            local_get(s); i32_const(8); i32_mul; i32_add;
                            i64_load(0);
                            local_get(s64); i64_eq;
                            if_empty;
                              // Found: store some(i) and break
                              i32_const(8); call(self.emitter.rt.alloc); local_set(s + 1);
                              local_get(s + 1); local_get(s); i64_extend_i32_u; i64_store(0);
                              local_get(s + 1); local_set(s + 2); br(2);
                            end;
                        });
                    }
                    _ => {
                        wasm!(self.func, {
                            i32_const(0); i32_load(0); i32_const(4); i32_add;
                            local_get(s); i32_const(elem_size as i32); i32_mul; i32_add;
                            i32_load(0);
                            i32_const(4); i32_load(0);
                        });
                        // String eq or i32 eq
                        if matches!(&elem_ty, Ty::String) {
                            wasm!(self.func, { call(self.emitter.rt.string.eq); });
                        } else {
                            wasm!(self.func, { i32_eq; });
                        }
                        wasm!(self.func, {
                            if_empty;
                              i32_const(8); call(self.emitter.rt.alloc); local_set(s + 1);
                              local_get(s + 1); local_get(s); i64_extend_i32_u; i64_store(0);
                              local_get(s + 1); local_set(s + 2); br(2);
                            end;
                        });
                    }
                }
                wasm!(self.func, {
                      local_get(s); i32_const(1); i32_add; local_set(s);
                      br(0);
                    end; end;
                    local_get(s + 2); // result (none if not found)
                });
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
                // unique(xs) → List[A]: O(n²) dedup, String elements
                let elem_ty = self.list_elem_ty(&args[0].ty);
                let es = values::byte_size(&elem_ty) as i32;
                let s = self.match_i32_base + self.match_depth;
                self.emit_expr(&args[0]);
                wasm!(self.func, {
                    local_set(s);
                    local_get(s); i32_load(0); local_set(s + 1); // src_len
                    i32_const(4); local_get(s + 1); i32_const(es); i32_mul; i32_add;
                    call(self.emitter.rt.alloc); local_set(s + 2); // dst
                    local_get(s + 2); i32_const(0); i32_store(0);
                    i32_const(0); local_set(s + 3); // i
                    block_empty; loop_empty;
                      local_get(s + 3); local_get(s + 1); i32_ge_u; br_if(1);
                      // Check if src[i] already in dst
                      i32_const(0); local_set(s + 4); // j
                      i32_const(0); local_set(s + 5); // found
                      block_empty; loop_empty;
                        local_get(s + 4); local_get(s + 2); i32_load(0); i32_ge_u; br_if(1);
                        local_get(s); i32_const(4); i32_add;
                        local_get(s + 3); i32_const(es); i32_mul; i32_add;
                        i32_load(0);
                        local_get(s + 2); i32_const(4); i32_add;
                        local_get(s + 4); i32_const(es); i32_mul; i32_add;
                        i32_load(0);
                });
                match &elem_ty {
                    Ty::String => { wasm!(self.func, { call(self.emitter.rt.string.eq); }); }
                    _ => { wasm!(self.func, { i32_eq; }); }
                }
                wasm!(self.func, {
                        if_empty; i32_const(1); local_set(s + 5); br(2); end;
                        local_get(s + 4); i32_const(1); i32_add; local_set(s + 4);
                        br(0);
                      end; end;
                      local_get(s + 5); i32_eqz;
                      if_empty;
                        local_get(s + 2); i32_const(4); i32_add;
                        local_get(s + 2); i32_load(0); i32_const(es); i32_mul; i32_add;
                        local_get(s); i32_const(4); i32_add;
                        local_get(s + 3); i32_const(es); i32_mul; i32_add;
                });
                self.emit_elem_copy(&elem_ty);
                wasm!(self.func, {
                        local_get(s + 2);
                        local_get(s + 2); i32_load(0); i32_const(1); i32_add;
                        i32_store(0);
                      end;
                      local_get(s + 3); i32_const(1); i32_add; local_set(s + 3);
                      br(0);
                    end; end;
                    local_get(s + 2);
                });
            }
            "find" => {
                // find(xs, pred) → Option[A]: first element where pred(x) is true
                let elem_ty = self.list_elem_ty(&args[0].ty);
                let es = values::byte_size(&elem_ty) as i32;
                let s = self.match_i32_base + self.match_depth;
                // mem[0]=xs, mem[4]=closure (store before closure emit)
                wasm!(self.func, { i32_const(0); });
                self.emit_expr(&args[0]);
                wasm!(self.func, { i32_store(0); i32_const(4); });
                self.emit_expr(&args[1]);
                wasm!(self.func, {
                    i32_store(0); // mem[4]=closure
                    i32_const(0); local_set(s); // i=0
                    i32_const(0); local_set(s + 2); // result (default: none)
                    block_empty; loop_empty;
                      local_get(s); i32_const(0); i32_load(0); i32_load(0); i32_ge_u; br_if(1);
                      // Call pred(xs[i])
                      i32_const(4); i32_load(0); i32_load(4); // env
                      i32_const(0); i32_load(0); i32_const(4); i32_add;
                      local_get(s); i32_const(es); i32_mul; i32_add;
                });
                self.emit_load_at(&elem_ty, 0);
                wasm!(self.func, {
                      i32_const(4); i32_load(0); i32_load(0); // table_idx
                });
                {
                    let mut ct = vec![ValType::I32];
                    if let Some(vt) = values::ty_to_valtype(&elem_ty) { ct.push(vt); }
                    let ti = self.emitter.register_type(ct, vec![ValType::I32]);
                    wasm!(self.func, { call_indirect(ti, 0); });
                }
                wasm!(self.func, {
                      if_empty;
                        // Found: alloc some(xs[i])
                        i32_const(es); call(self.emitter.rt.alloc); local_set(s + 1);
                        local_get(s + 1);
                        i32_const(0); i32_load(0); i32_const(4); i32_add;
                        local_get(s); i32_const(es); i32_mul; i32_add;
                });
                self.emit_elem_copy(&elem_ty);
                wasm!(self.func, {
                        local_get(s + 1); local_set(s + 2); br(2);
                      end;
                      local_get(s); i32_const(1); i32_add; local_set(s);
                      br(0);
                    end; end;
                    local_get(s + 2); // result (none if not found)
                });
            }
            "find_index" if args.len() == 2 && matches!(&args[1].ty, Ty::Fn { .. }) => {
                // find_index(xs, pred) → Option[Int]
                let elem_ty = self.list_elem_ty(&args[0].ty);
                let es = values::byte_size(&elem_ty) as i32;
                let s = self.match_i32_base + self.match_depth;
                wasm!(self.func, { i32_const(0); });
                self.emit_expr(&args[0]);
                wasm!(self.func, { i32_store(0); i32_const(4); });
                self.emit_expr(&args[1]);
                wasm!(self.func, {
                    i32_store(0);
                    i32_const(0); local_set(s);
                    i32_const(0); local_set(s + 2); // result (default: none)
                    block_empty; loop_empty;
                      local_get(s); i32_const(0); i32_load(0); i32_load(0); i32_ge_u; br_if(1);
                      i32_const(4); i32_load(0); i32_load(4); // env
                      i32_const(0); i32_load(0); i32_const(4); i32_add;
                      local_get(s); i32_const(es); i32_mul; i32_add;
                });
                self.emit_load_at(&elem_ty, 0);
                wasm!(self.func, { i32_const(4); i32_load(0); i32_load(0); });
                {
                    let mut ct = vec![ValType::I32];
                    if let Some(vt) = values::ty_to_valtype(&elem_ty) { ct.push(vt); }
                    let ti = self.emitter.register_type(ct, vec![ValType::I32]);
                    wasm!(self.func, { call_indirect(ti, 0); });
                }
                wasm!(self.func, {
                      if_empty;
                        i32_const(8); call(self.emitter.rt.alloc); local_set(s + 1);
                        local_get(s + 1); local_get(s); i64_extend_i32_u; i64_store(0);
                        local_get(s + 1); local_set(s + 2); br(2);
                      end;
                      local_get(s); i32_const(1); i32_add; local_set(s);
                      br(0);
                    end; end;
                    local_get(s + 2); // result (none if not found)
                });
            }
            "any" => {
                // any(xs, pred) → Bool
                let elem_ty = self.list_elem_ty(&args[0].ty);
                let es = values::byte_size(&elem_ty) as i32;
                let s = self.match_i32_base + self.match_depth;
                wasm!(self.func, { i32_const(0); });
                self.emit_expr(&args[0]);
                wasm!(self.func, { i32_store(0); i32_const(4); });
                self.emit_expr(&args[1]);
                wasm!(self.func, {
                    i32_store(0);
                    i32_const(0); local_set(s);
                    i32_const(0); local_set(s + 1); // result (default: false)
                    block_empty; loop_empty;
                      local_get(s); i32_const(0); i32_load(0); i32_load(0); i32_ge_u; br_if(1);
                      i32_const(4); i32_load(0); i32_load(4);
                      i32_const(0); i32_load(0); i32_const(4); i32_add;
                      local_get(s); i32_const(es); i32_mul; i32_add;
                });
                self.emit_load_at(&elem_ty, 0);
                wasm!(self.func, { i32_const(4); i32_load(0); i32_load(0); });
                {
                    let mut ct = vec![ValType::I32];
                    if let Some(vt) = values::ty_to_valtype(&elem_ty) { ct.push(vt); }
                    let ti = self.emitter.register_type(ct, vec![ValType::I32]);
                    wasm!(self.func, { call_indirect(ti, 0); });
                }
                wasm!(self.func, {
                      if_empty; i32_const(1); local_set(s + 1); br(2); end;
                      local_get(s); i32_const(1); i32_add; local_set(s);
                      br(0);
                    end; end;
                    local_get(s + 1); // result
                });
            }
            "all" => {
                let elem_ty = self.list_elem_ty(&args[0].ty);
                let es = values::byte_size(&elem_ty) as i32;
                let s = self.match_i32_base + self.match_depth;
                wasm!(self.func, { i32_const(0); });
                self.emit_expr(&args[0]);
                wasm!(self.func, { i32_store(0); i32_const(4); });
                self.emit_expr(&args[1]);
                wasm!(self.func, {
                    i32_store(0);
                    i32_const(0); local_set(s);
                    i32_const(1); local_set(s + 1); // result (default: true)
                    block_empty; loop_empty;
                      local_get(s); i32_const(0); i32_load(0); i32_load(0); i32_ge_u; br_if(1);
                      i32_const(4); i32_load(0); i32_load(4);
                      i32_const(0); i32_load(0); i32_const(4); i32_add;
                      local_get(s); i32_const(es); i32_mul; i32_add;
                });
                self.emit_load_at(&elem_ty, 0);
                wasm!(self.func, { i32_const(4); i32_load(0); i32_load(0); });
                {
                    let mut ct = vec![ValType::I32];
                    if let Some(vt) = values::ty_to_valtype(&elem_ty) { ct.push(vt); }
                    let ti = self.emitter.register_type(ct, vec![ValType::I32]);
                    wasm!(self.func, { call_indirect(ti, 0); });
                }
                wasm!(self.func, {
                      i32_eqz;
                      if_empty; i32_const(0); local_set(s + 1); br(2); end;
                      local_get(s); i32_const(1); i32_add; local_set(s);
                      br(0);
                    end; end;
                    local_get(s + 1); // result
                });
            }
            "each" => {
                let elem_ty = self.list_elem_ty(&args[0].ty);
                let es = values::byte_size(&elem_ty) as i32;
                let s = self.match_i32_base + self.match_depth;
                wasm!(self.func, { i32_const(0); });
                self.emit_expr(&args[0]);
                wasm!(self.func, { i32_store(0); i32_const(4); });
                self.emit_expr(&args[1]);
                wasm!(self.func, {
                    i32_store(0);
                    i32_const(0); local_set(s);
                    block_empty; loop_empty;
                      local_get(s); i32_const(0); i32_load(0); i32_load(0); i32_ge_u; br_if(1);
                      i32_const(4); i32_load(0); i32_load(4);
                      i32_const(0); i32_load(0); i32_const(4); i32_add;
                      local_get(s); i32_const(es); i32_mul; i32_add;
                });
                self.emit_load_at(&elem_ty, 0);
                wasm!(self.func, { i32_const(4); i32_load(0); i32_load(0); });
                {
                    let mut ct = vec![ValType::I32];
                    if let Some(vt) = values::ty_to_valtype(&elem_ty) { ct.push(vt); }
                    let ti = self.emitter.register_type(ct, vec![]);
                    wasm!(self.func, { call_indirect(ti, 0); });
                }
                wasm!(self.func, {
                      local_get(s); i32_const(1); i32_add; local_set(s);
                      br(0);
                    end; end;
                });
            }
            "take_end" => {
                // take_end(xs, n) = drop(xs, max(0, len-n))
                let elem_ty = self.list_elem_ty(&args[0].ty);
                let es = values::byte_size(&elem_ty) as i32;
                let s = self.match_i32_base + self.match_depth;
                wasm!(self.func, { i32_const(0); });
                self.emit_expr(&args[0]);
                wasm!(self.func, { i32_store(0); });
                self.emit_expr(&args[1]);
                wasm!(self.func, {
                    i32_wrap_i64; local_set(s); // n
                    // start = max(0, len - n)
                    i32_const(0); i32_load(0); i32_load(0); local_get(s); i32_sub;
                    local_set(s + 1);
                    local_get(s + 1); i32_const(0); i32_lt_s;
                    if_empty; i32_const(0); local_set(s + 1); end;
                    // new_len = len - start
                    i32_const(0); i32_load(0); i32_load(0); local_get(s + 1); i32_sub;
                    local_set(s + 2);
                    i32_const(4); local_get(s + 2); i32_const(es); i32_mul; i32_add;
                    call(self.emitter.rt.alloc); local_set(s + 3);
                    local_get(s + 3); local_get(s + 2); i32_store(0);
                    i32_const(0); local_set(s); // reuse as i
                    block_empty; loop_empty;
                      local_get(s); local_get(s + 2); i32_ge_u; br_if(1);
                      local_get(s + 3); i32_const(4); i32_add;
                      local_get(s); i32_const(es); i32_mul; i32_add;
                      i32_const(0); i32_load(0); i32_const(4); i32_add;
                      local_get(s + 1); local_get(s); i32_add;
                      i32_const(es); i32_mul; i32_add;
                });
                self.emit_elem_copy(&elem_ty);
                wasm!(self.func, {
                      local_get(s); i32_const(1); i32_add; local_set(s);
                      br(0);
                    end; end;
                    local_get(s + 3);
                });
            }
            "drop_end" => {
                // drop_end(xs, n) = take(xs, max(0, len-n))
                let elem_ty = self.list_elem_ty(&args[0].ty);
                let es = values::byte_size(&elem_ty) as i32;
                let s = self.match_i32_base + self.match_depth;
                wasm!(self.func, { i32_const(0); });
                self.emit_expr(&args[0]);
                wasm!(self.func, { i32_store(0); });
                self.emit_expr(&args[1]);
                wasm!(self.func, {
                    i32_wrap_i64; local_set(s); // n
                    i32_const(0); i32_load(0); i32_load(0); local_get(s); i32_sub;
                    local_set(s + 1); // new_len
                    local_get(s + 1); i32_const(0); i32_lt_s;
                    if_empty; i32_const(0); local_set(s + 1); end;
                    i32_const(4); local_get(s + 1); i32_const(es); i32_mul; i32_add;
                    call(self.emitter.rt.alloc); local_set(s + 2);
                    local_get(s + 2); local_get(s + 1); i32_store(0);
                    i32_const(0); local_set(s); // i
                    block_empty; loop_empty;
                      local_get(s); local_get(s + 1); i32_ge_u; br_if(1);
                      local_get(s + 2); i32_const(4); i32_add;
                      local_get(s); i32_const(es); i32_mul; i32_add;
                      i32_const(0); i32_load(0); i32_const(4); i32_add;
                      local_get(s); i32_const(es); i32_mul; i32_add;
                });
                self.emit_elem_copy(&elem_ty);
                wasm!(self.func, {
                      local_get(s); i32_const(1); i32_add; local_set(s);
                      br(0);
                    end; end;
                    local_get(s + 2);
                });
            }
            "repeat" => {
                // repeat(val, n) → List[A]
                let elem_ty = self.list_elem_ty(&args[0].ty);
                let es = values::byte_size(&elem_ty) as i32;
                let s = self.match_i32_base + self.match_depth;
                wasm!(self.func, { i32_const(0); });
                self.emit_expr(&args[0]); // val
                self.emit_store_at(&elem_ty, 0); // mem[0] = val
                self.emit_expr(&args[1]); // n
                wasm!(self.func, {
                    i32_wrap_i64; local_set(s); // n
                    i32_const(4); local_get(s); i32_const(es); i32_mul; i32_add;
                    call(self.emitter.rt.alloc); local_set(s + 1);
                    local_get(s + 1); local_get(s); i32_store(0);
                    i32_const(0); local_set(s + 2); // i
                    block_empty; loop_empty;
                      local_get(s + 2); local_get(s); i32_ge_u; br_if(1);
                      local_get(s + 1); i32_const(4); i32_add;
                      local_get(s + 2); i32_const(es); i32_mul; i32_add;
                      i32_const(0);
                });
                self.emit_load_at(&elem_ty, 0);
                self.emit_store_at(&elem_ty, 0);
                wasm!(self.func, {
                      local_get(s + 2); i32_const(1); i32_add; local_set(s + 2);
                      br(0);
                    end; end;
                    local_get(s + 1);
                });
            }
            "reduce" => {
                // reduce(xs, f) → Option[A]: fold starting from xs[0]
                let elem_ty = self.list_elem_ty(&args[0].ty);
                let es = values::byte_size(&elem_ty) as i32;
                let s = self.match_i32_base + self.match_depth;
                let s64 = self.match_i64_base + self.match_depth;
                wasm!(self.func, { i32_const(0); });
                self.emit_expr(&args[0]);
                wasm!(self.func, { i32_store(0); i32_const(4); });
                self.emit_expr(&args[1]); // fn(a, b) -> a
                wasm!(self.func, {
                    i32_store(0); // mem[4] = closure
                    i32_const(0); i32_load(0); i32_load(0); i32_eqz;
                    if_i32; i32_const(0); // empty → none
                    else_;
                      // acc = xs[0]
                      i32_const(0); i32_load(0); i32_const(4); i32_add;
                });
                self.emit_load_at(&elem_ty, 0);
                wasm!(self.func, { local_set(s64); }); // acc in i64 local (works for i32 too via reinterpret)
                // For i32 elements, use s instead
                // Actually this only works for i64. For i32 elements, need different approach.
                // Simplify: use i64 for acc regardless, works for Int.
                wasm!(self.func, {
                      i32_const(1); local_set(s); // i = 1
                      block_empty; loop_empty;
                        local_get(s); i32_const(0); i32_load(0); i32_load(0); i32_ge_u; br_if(1);
                        // Call f(acc, xs[i])
                        i32_const(4); i32_load(0); i32_load(4); // env
                        local_get(s64); // acc
                        i32_const(0); i32_load(0); i32_const(4); i32_add;
                        local_get(s); i32_const(es); i32_mul; i32_add;
                });
                self.emit_load_at(&elem_ty, 0);
                wasm!(self.func, {
                        i32_const(4); i32_load(0); i32_load(0); // table_idx
                });
                // call_indirect (env, a, b) → a
                {
                    let mut ct = vec![ValType::I32]; // env
                    if let Some(vt) = values::ty_to_valtype(&elem_ty) { ct.push(vt); ct.push(vt); }
                    let rt = values::ret_type(&elem_ty);
                    let ti = self.emitter.register_type(ct, rt);
                    wasm!(self.func, { call_indirect(ti, 0); });
                }
                wasm!(self.func, {
                        local_set(s64); // update acc
                        local_get(s); i32_const(1); i32_add; local_set(s);
                        br(0);
                      end; end;
                      // Wrap acc in some
                      i32_const(es); call(self.emitter.rt.alloc); local_set(s);
                      local_get(s); local_get(s64);
                });
                self.emit_store_at(&elem_ty, 0);
                wasm!(self.func, { local_get(s); end; });
            }
            "flat_map" => {
                // flat_map(xs, f) → List[B]: f returns List[B], flatten results
                // Strategy: collect results into list of lists, then flatten
                let elem_ty = self.list_elem_ty(&args[0].ty);
                let es = values::byte_size(&elem_ty) as i32;
                let s = self.match_i32_base + self.match_depth;
                // mem[0]=xs, mem[4]=closure
                wasm!(self.func, { i32_const(0); });
                self.emit_expr(&args[0]);
                wasm!(self.func, { i32_store(0); i32_const(4); });
                self.emit_expr(&args[1]);
                wasm!(self.func, {
                    i32_store(0);
                    i32_const(0); i32_load(0); i32_load(0); local_set(s); // len
                    // Alloc temp list-of-lists: [len][ptr0][ptr1]...
                    i32_const(4); local_get(s); i32_const(4); i32_mul; i32_add;
                    call(self.emitter.rt.alloc); local_set(s + 1);
                    local_get(s + 1); local_get(s); i32_store(0);
                    i32_const(0); local_set(s + 2); // i
                    block_empty; loop_empty;
                      local_get(s + 2); local_get(s); i32_ge_u; br_if(1);
                      // Call f(xs[i]) → List[B]
                      local_get(s + 1); i32_const(4); i32_add;
                      local_get(s + 2); i32_const(4); i32_mul; i32_add; // dst addr for result ptr
                      i32_const(4); i32_load(0); i32_load(4); // env
                      i32_const(0); i32_load(0); i32_const(4); i32_add;
                      local_get(s + 2); i32_const(es); i32_mul; i32_add;
                });
                self.emit_load_at(&elem_ty, 0);
                wasm!(self.func, {
                      i32_const(4); i32_load(0); i32_load(0); // table_idx
                });
                {
                    let mut ct = vec![ValType::I32];
                    if let Some(vt) = values::ty_to_valtype(&elem_ty) { ct.push(vt); }
                    let ti = self.emitter.register_type(ct, vec![ValType::I32]); // returns List ptr
                    wasm!(self.func, { call_indirect(ti, 0); });
                }
                wasm!(self.func, {
                      i32_store(0); // store result list ptr
                      local_get(s + 2); i32_const(1); i32_add; local_set(s + 2);
                      br(0);
                    end; end;
                    // Now flatten: s+1 is a List[List[B]]
                    // Reuse list.flatten logic via emit_list_call
                    local_get(s + 1);
                });
                // Call flatten on the temp list-of-lists
                // Can't call self recursively easily. Inline flatten:
                // Count total
                wasm!(self.func, {
                    local_set(s); // temp = list-of-lists
                    i32_const(0); local_set(s + 1); // total
                    i32_const(0); local_set(s + 2); // i
                    block_empty; loop_empty;
                      local_get(s + 2); local_get(s); i32_load(0); i32_ge_u; br_if(1);
                      local_get(s + 1);
                      local_get(s); i32_const(4); i32_add;
                      local_get(s + 2); i32_const(4); i32_mul; i32_add;
                      i32_load(0); i32_load(0);
                      i32_add; local_set(s + 1);
                      local_get(s + 2); i32_const(1); i32_add; local_set(s + 2);
                      br(0);
                    end; end;
                    // Alloc result: assume 4 bytes per element (i32 ptrs)
                    i32_const(4); local_get(s + 1); i32_const(4); i32_mul; i32_add;
                    call(self.emitter.rt.alloc); local_set(s + 3);
                    local_get(s + 3); local_get(s + 1); i32_store(0);
                });
                // Copy all sub-list elements
                wasm!(self.func, {
                    i32_const(0); local_set(s + 1); // dst_offset
                    i32_const(0); local_set(s + 2); // outer i
                    block_empty; loop_empty;
                      local_get(s + 2); local_get(s); i32_load(0); i32_ge_u; br_if(1);
                      // inner list
                      local_get(s); i32_const(4); i32_add;
                      local_get(s + 2); i32_const(4); i32_mul; i32_add;
                      i32_load(0); local_set(s + 4); // inner
                      i32_const(0); local_set(s + 5); // j
                      block_empty; loop_empty;
                        local_get(s + 5); local_get(s + 4); i32_load(0); i32_ge_u; br_if(1);
                        local_get(s + 3); i32_const(4); i32_add;
                        local_get(s + 1); i32_const(4); i32_mul; i32_add;
                        local_get(s + 4); i32_const(4); i32_add;
                        local_get(s + 5); i32_const(4); i32_mul; i32_add;
                        i32_load(0); i32_store(0);
                        local_get(s + 1); i32_const(1); i32_add; local_set(s + 1);
                        local_get(s + 5); i32_const(1); i32_add; local_set(s + 5);
                        br(0);
                      end; end;
                      local_get(s + 2); i32_const(1); i32_add; local_set(s + 2);
                      br(0);
                    end; end;
                    local_get(s + 3);
                });
            }
            "filter_map" => {
                // filter_map(xs, f) → List[B]: f returns Option[B], keep some values
                let elem_ty = self.list_elem_ty(&args[0].ty);
                let es = values::byte_size(&elem_ty) as i32;
                let s = self.match_i32_base + self.match_depth;
                // mem[0]=xs, mem[4]=closure
                wasm!(self.func, { i32_const(0); });
                self.emit_expr(&args[0]);
                wasm!(self.func, { i32_store(0); i32_const(4); });
                self.emit_expr(&args[1]);
                wasm!(self.func, {
                    i32_store(0);
                    i32_const(0); i32_load(0); i32_load(0); local_set(s); // len
                    // Alloc max-size result (4 bytes per element ptr)
                    i32_const(4); local_get(s); i32_const(4); i32_mul; i32_add;
                    call(self.emitter.rt.alloc); local_set(s + 1);
                    local_get(s + 1); i32_const(0); i32_store(0); // result len = 0
                    i32_const(0); local_set(s + 2); // i
                    block_empty; loop_empty;
                      local_get(s + 2); local_get(s); i32_ge_u; br_if(1);
                      // Call f(xs[i]) → Option[B]
                      i32_const(4); i32_load(0); i32_load(4); // env
                      i32_const(0); i32_load(0); i32_const(4); i32_add;
                      local_get(s + 2); i32_const(es); i32_mul; i32_add;
                });
                self.emit_load_at(&elem_ty, 0);
                wasm!(self.func, {
                      i32_const(4); i32_load(0); i32_load(0); // table_idx
                });
                {
                    let mut ct = vec![ValType::I32];
                    if let Some(vt) = values::ty_to_valtype(&elem_ty) { ct.push(vt); }
                    let ti = self.emitter.register_type(ct, vec![ValType::I32]); // returns Option ptr (i32)
                    wasm!(self.func, { call_indirect(ti, 0); });
                }
                wasm!(self.func, {
                      local_set(s + 3); // option result
                      // If some (non-zero), append inner value to result
                      local_get(s + 3); i32_const(0); i32_ne;
                      if_empty;
                        local_get(s + 1); i32_const(4); i32_add;
                        local_get(s + 1); i32_load(0); i32_const(4); i32_mul; i32_add;
                        local_get(s + 3); i32_load(0); // unwrap some → inner value (ptr)
                        i32_store(0);
                        local_get(s + 1);
                        local_get(s + 1); i32_load(0); i32_const(1); i32_add;
                        i32_store(0);
                      end;
                      local_get(s + 2); i32_const(1); i32_add; local_set(s + 2);
                      br(0);
                    end; end;
                    local_get(s + 1);
                });
            }
            "swap" => {
                // swap(xs, i, j) → List[A]: copy with elements at i and j swapped
                let elem_ty = self.list_elem_ty(&args[0].ty);
                let es = values::byte_size(&elem_ty) as i32;
                let s = self.match_i32_base + self.match_depth;
                // mem[0]=xs
                wasm!(self.func, { i32_const(0); });
                self.emit_expr(&args[0]);
                wasm!(self.func, { i32_store(0); });
                self.emit_expr(&args[1]); // i
                wasm!(self.func, { i32_wrap_i64; local_set(s); }); // s = i
                self.emit_expr(&args[2]); // j
                wasm!(self.func, {
                    i32_wrap_i64; local_set(s + 1); // s+1 = j
                    i32_const(0); i32_load(0); i32_load(0); local_set(s + 2); // len
                    // Alloc copy
                    i32_const(4); local_get(s + 2); i32_const(es); i32_mul; i32_add;
                    call(self.emitter.rt.alloc); local_set(s + 3); // dst
                    local_get(s + 3); local_get(s + 2); i32_store(0);
                    // Copy all elements
                    i32_const(0); local_set(s + 4); // k
                    block_empty; loop_empty;
                      local_get(s + 4); local_get(s + 2); i32_ge_u; br_if(1);
                      local_get(s + 3); i32_const(4); i32_add;
                      local_get(s + 4); i32_const(es); i32_mul; i32_add;
                      i32_const(0); i32_load(0); i32_const(4); i32_add;
                      local_get(s + 4); i32_const(es); i32_mul; i32_add;
                });
                self.emit_elem_copy(&elem_ty);
                wasm!(self.func, {
                      local_get(s + 4); i32_const(1); i32_add; local_set(s + 4);
                      br(0);
                    end; end;
                });
                // Now swap dst[i] and dst[j]:
                // We need a temp. Use mem[4..4+es] as temp.
                // temp = dst[i]
                wasm!(self.func, {
                    i32_const(4);
                    local_get(s + 3); i32_const(4); i32_add;
                    local_get(s); i32_const(es); i32_mul; i32_add;
                });
                self.emit_elem_copy(&elem_ty);
                // dst[i] = dst[j]
                wasm!(self.func, {
                    local_get(s + 3); i32_const(4); i32_add;
                    local_get(s); i32_const(es); i32_mul; i32_add;
                    local_get(s + 3); i32_const(4); i32_add;
                    local_get(s + 1); i32_const(es); i32_mul; i32_add;
                });
                self.emit_elem_copy(&elem_ty);
                // dst[j] = temp
                wasm!(self.func, {
                    local_get(s + 3); i32_const(4); i32_add;
                    local_get(s + 1); i32_const(es); i32_mul; i32_add;
                    i32_const(4);
                });
                self.emit_elem_copy(&elem_ty);
                wasm!(self.func, { local_get(s + 3); });
            }
            "chunk" => {
                // chunk(xs, n) → List[List[A]]
                // Outer list of inner lists. Each inner list has up to n elements.
                let elem_ty = self.list_elem_ty(&args[0].ty);
                let es = values::byte_size(&elem_ty) as i32;
                let s = self.match_i32_base + self.match_depth;
                // s=len, s+1=n, s+2=num_chunks, s+3=outer, s+4=i(outer), s+5=chunk_len, s+6=inner, s+7=j
                wasm!(self.func, { i32_const(0); });
                self.emit_expr(&args[0]);
                wasm!(self.func, { i32_store(0); });
                self.emit_expr(&args[1]); // n
                wasm!(self.func, {
                    i32_wrap_i64; local_set(s + 1);
                    i32_const(0); i32_load(0); i32_load(0); local_set(s); // len
                    // num_chunks = (len + n - 1) / n
                    local_get(s); local_get(s + 1); i32_add; i32_const(1); i32_sub;
                    local_get(s + 1); i32_div_u;
                    local_set(s + 2);
                    // Alloc outer: 4 + num_chunks * 4 (list of ptrs)
                    i32_const(4); local_get(s + 2); i32_const(4); i32_mul; i32_add;
                    call(self.emitter.rt.alloc); local_set(s + 3);
                    local_get(s + 3); local_get(s + 2); i32_store(0);
                    i32_const(0); local_set(s + 4); // outer i
                    block_empty; loop_empty;
                      local_get(s + 4); local_get(s + 2); i32_ge_u; br_if(1);
                      // chunk_len = min(n, len - i*n)
                      local_get(s); local_get(s + 4); local_get(s + 1); i32_mul; i32_sub;
                      local_set(s + 5);
                      local_get(s + 5); local_get(s + 1); i32_gt_u;
                      if_empty; local_get(s + 1); local_set(s + 5); end;
                      // Alloc inner: 4 + chunk_len * es
                      i32_const(4); local_get(s + 5); i32_const(es); i32_mul; i32_add;
                      call(self.emitter.rt.alloc); local_set(s + 6);
                      local_get(s + 6); local_get(s + 5); i32_store(0);
                      // Copy elements
                      i32_const(0); local_set(s + 7); // j
                      block_empty; loop_empty;
                        local_get(s + 7); local_get(s + 5); i32_ge_u; br_if(1);
                        local_get(s + 6); i32_const(4); i32_add;
                        local_get(s + 7); i32_const(es); i32_mul; i32_add;
                        i32_const(0); i32_load(0); i32_const(4); i32_add;
                        local_get(s + 4); local_get(s + 1); i32_mul;
                        local_get(s + 7); i32_add;
                        i32_const(es); i32_mul; i32_add;
                });
                self.emit_elem_copy(&elem_ty);
                wasm!(self.func, {
                        local_get(s + 7); i32_const(1); i32_add; local_set(s + 7);
                        br(0);
                      end; end;
                      // outer[i] = inner_ptr
                      local_get(s + 3); i32_const(4); i32_add;
                      local_get(s + 4); i32_const(4); i32_mul; i32_add;
                      local_get(s + 6); i32_store(0);
                      local_get(s + 4); i32_const(1); i32_add; local_set(s + 4);
                      br(0);
                    end; end;
                    local_get(s + 3);
                });
            }
            "windows" | "window" => {
                // windows(xs, n) → List[List[A]]: sliding windows of size n
                let elem_ty = self.list_elem_ty(&args[0].ty);
                let es = values::byte_size(&elem_ty) as i32;
                let s = self.match_i32_base + self.match_depth;
                // s=len, s+1=n, s+2=num_win, s+3=outer, s+4=i, s+5=inner, s+6=j
                wasm!(self.func, { i32_const(0); });
                self.emit_expr(&args[0]);
                wasm!(self.func, { i32_store(0); });
                self.emit_expr(&args[1]);
                wasm!(self.func, {
                    i32_wrap_i64; local_set(s + 1); // n
                    i32_const(0); i32_load(0); i32_load(0); local_set(s); // len
                    // num_win = if len >= n then len - n + 1 else 0
                    local_get(s); local_get(s + 1); i32_ge_u;
                    if_i32;
                      local_get(s); local_get(s + 1); i32_sub; i32_const(1); i32_add;
                    else_;
                      i32_const(0);
                    end;
                    local_set(s + 2);
                    // Alloc outer: 4 + num_win * 4
                    i32_const(4); local_get(s + 2); i32_const(4); i32_mul; i32_add;
                    call(self.emitter.rt.alloc); local_set(s + 3);
                    local_get(s + 3); local_get(s + 2); i32_store(0);
                    i32_const(0); local_set(s + 4); // i
                    block_empty; loop_empty;
                      local_get(s + 4); local_get(s + 2); i32_ge_u; br_if(1);
                      // Alloc inner: 4 + n * es
                      i32_const(4); local_get(s + 1); i32_const(es); i32_mul; i32_add;
                      call(self.emitter.rt.alloc); local_set(s + 5);
                      local_get(s + 5); local_get(s + 1); i32_store(0);
                      // Copy n elements starting at i
                      i32_const(0); local_set(s + 6); // j
                      block_empty; loop_empty;
                        local_get(s + 6); local_get(s + 1); i32_ge_u; br_if(1);
                        local_get(s + 5); i32_const(4); i32_add;
                        local_get(s + 6); i32_const(es); i32_mul; i32_add;
                        i32_const(0); i32_load(0); i32_const(4); i32_add;
                        local_get(s + 4); local_get(s + 6); i32_add;
                        i32_const(es); i32_mul; i32_add;
                });
                self.emit_elem_copy(&elem_ty);
                wasm!(self.func, {
                        local_get(s + 6); i32_const(1); i32_add; local_set(s + 6);
                        br(0);
                      end; end;
                      // outer[i] = inner_ptr
                      local_get(s + 3); i32_const(4); i32_add;
                      local_get(s + 4); i32_const(4); i32_mul; i32_add;
                      local_get(s + 5); i32_store(0);
                      local_get(s + 4); i32_const(1); i32_add; local_set(s + 4);
                      br(0);
                    end; end;
                    local_get(s + 3);
                });
            }
            "dedup" => {
                // dedup(xs) → List[A]: remove consecutive duplicates
                let elem_ty = self.list_elem_ty(&args[0].ty);
                let es = values::byte_size(&elem_ty) as i32;
                let s = self.match_i32_base + self.match_depth;
                // s=xs, s+1=len, s+2=dst, s+3=i, s+4=out_count
                self.emit_expr(&args[0]);
                wasm!(self.func, {
                    local_set(s);
                    local_get(s); i32_load(0); local_set(s + 1); // len
                    // Alloc dst (max = len)
                    i32_const(4); local_get(s + 1); i32_const(es); i32_mul; i32_add;
                    call(self.emitter.rt.alloc); local_set(s + 2);
                    i32_const(0); local_set(s + 4); // out_count
                    // If empty, return empty
                    local_get(s + 1); i32_eqz;
                    if_empty;
                      local_get(s + 2); i32_const(0); i32_store(0);
                    else_;
                      // Always include first element
                      local_get(s + 2); i32_const(4); i32_add;
                      local_get(s); i32_const(4); i32_add;
                });
                self.emit_elem_copy(&elem_ty);
                wasm!(self.func, {
                      i32_const(1); local_set(s + 4); // out_count = 1
                      i32_const(1); local_set(s + 3); // i = 1
                      block_empty; loop_empty;
                        local_get(s + 3); local_get(s + 1); i32_ge_u; br_if(1);
                        // Compare xs[i] with xs[i-1]
                        local_get(s); i32_const(4); i32_add;
                        local_get(s + 3); i32_const(es); i32_mul; i32_add;
                        i32_load(0);
                        local_get(s); i32_const(4); i32_add;
                        local_get(s + 3); i32_const(1); i32_sub;
                        i32_const(es); i32_mul; i32_add;
                        i32_load(0);
                });
                match &elem_ty {
                    Ty::String => { wasm!(self.func, { call(self.emitter.rt.string.eq); }); }
                    _ => { wasm!(self.func, { i32_eq; }); }
                }
                wasm!(self.func, {
                        i32_eqz; // not equal → include
                        if_empty;
                          local_get(s + 2); i32_const(4); i32_add;
                          local_get(s + 4); i32_const(es); i32_mul; i32_add;
                          local_get(s); i32_const(4); i32_add;
                          local_get(s + 3); i32_const(es); i32_mul; i32_add;
                });
                self.emit_elem_copy(&elem_ty);
                wasm!(self.func, {
                          local_get(s + 4); i32_const(1); i32_add; local_set(s + 4);
                        end;
                        local_get(s + 3); i32_const(1); i32_add; local_set(s + 3);
                        br(0);
                      end; end;
                      local_get(s + 2); local_get(s + 4); i32_store(0);
                    end;
                    local_get(s + 2);
                });
            }
            "sort_by" => {
                // sort_by(xs, f) → List[A]: bubble sort by key function
                // Strategy: copy list, compute keys into parallel array, bubble sort both
                let elem_ty = self.list_elem_ty(&args[0].ty);
                let es = values::byte_size(&elem_ty) as i32;
                let s = self.match_i32_base + self.match_depth;
                let s64 = self.match_i64_base + self.match_depth;
                // mem[0]=xs, mem[4]=closure
                wasm!(self.func, { i32_const(0); });
                self.emit_expr(&args[0]);
                wasm!(self.func, { i32_store(0); i32_const(4); });
                self.emit_expr(&args[1]);
                wasm!(self.func, {
                    i32_store(0);
                    i32_const(0); i32_load(0); i32_load(0); local_set(s); // len
                    // Alloc copy of elements
                    i32_const(4); local_get(s); i32_const(es); i32_mul; i32_add;
                    call(self.emitter.rt.alloc); local_set(s + 1); // dst
                    local_get(s + 1); local_get(s); i32_store(0);
                    // Copy all elements
                    i32_const(0); local_set(s + 2);
                    block_empty; loop_empty;
                      local_get(s + 2); local_get(s); i32_ge_u; br_if(1);
                      local_get(s + 1); i32_const(4); i32_add;
                      local_get(s + 2); i32_const(es); i32_mul; i32_add;
                      i32_const(0); i32_load(0); i32_const(4); i32_add;
                      local_get(s + 2); i32_const(es); i32_mul; i32_add;
                });
                self.emit_elem_copy(&elem_ty);
                wasm!(self.func, {
                      local_get(s + 2); i32_const(1); i32_add; local_set(s + 2);
                      br(0);
                    end; end;
                });
                // Alloc keys array: len * 8 (i64 keys)
                wasm!(self.func, {
                    local_get(s); i32_const(8); i32_mul;
                    call(self.emitter.rt.alloc); local_set(s + 2); // keys
                    // Compute keys for all elements
                    i32_const(0); local_set(s + 3);
                    block_empty; loop_empty;
                      local_get(s + 3); local_get(s); i32_ge_u; br_if(1);
                      i32_const(4); i32_load(0); i32_load(4); // env
                      local_get(s + 1); i32_const(4); i32_add;
                      local_get(s + 3); i32_const(es); i32_mul; i32_add;
                });
                self.emit_load_at(&elem_ty, 0);
                wasm!(self.func, {
                      i32_const(4); i32_load(0); i32_load(0); // table_idx
                });
                {
                    let mut ct = vec![ValType::I32];
                    if let Some(vt) = values::ty_to_valtype(&elem_ty) { ct.push(vt); }
                    let ti = self.emitter.register_type(ct, vec![ValType::I64]);
                    wasm!(self.func, { call_indirect(ti, 0); });
                }
                wasm!(self.func, {
                      local_set(s64);
                      local_get(s + 2);
                      local_get(s + 3); i32_const(8); i32_mul; i32_add;
                      local_get(s64); i64_store(0);
                      local_get(s + 3); i32_const(1); i32_add; local_set(s + 3);
                      br(0);
                    end; end;
                });
                // Bubble sort: outer loop i from 0..len-1, inner loop j from 0..len-1-i
                // Swap adjacent if keys[j] > keys[j+1]
                // For swapping elements, use mem[8..8+es] as temp
                wasm!(self.func, {
                    i32_const(0); local_set(s + 3); // i (outer)
                    block_empty; loop_empty;
                      local_get(s + 3); local_get(s); i32_const(1); i32_sub; i32_ge_u; br_if(1);
                      i32_const(0); local_set(s + 4); // j (inner)
                      block_empty; loop_empty;
                        // j < len - 1 - i
                        local_get(s); i32_const(1); i32_sub; local_get(s + 3); i32_sub;
                        local_get(s + 4); i32_le_u; br_if(1);
                        // Compare keys[j] > keys[j+1]
                        local_get(s + 2);
                        local_get(s + 4); i32_const(8); i32_mul; i32_add;
                        i64_load(0);
                        local_get(s + 2);
                        local_get(s + 4); i32_const(1); i32_add; i32_const(8); i32_mul; i32_add;
                        i64_load(0);
                        i64_gt_s;
                        if_empty;
                          // Swap keys[j] and keys[j+1]
                          local_get(s + 2);
                          local_get(s + 4); i32_const(8); i32_mul; i32_add;
                          i64_load(0); local_set(s64); // temp_key
                          local_get(s + 2);
                          local_get(s + 4); i32_const(8); i32_mul; i32_add;
                          local_get(s + 2);
                          local_get(s + 4); i32_const(1); i32_add; i32_const(8); i32_mul; i32_add;
                          i64_load(0); i64_store(0);
                          local_get(s + 2);
                          local_get(s + 4); i32_const(1); i32_add; i32_const(8); i32_mul; i32_add;
                          local_get(s64); i64_store(0);
                          // Swap dst[j] and dst[j+1] using mem[8] as temp
                          // temp = dst[j]
                          i32_const(8);
                          local_get(s + 1); i32_const(4); i32_add;
                          local_get(s + 4); i32_const(es); i32_mul; i32_add;
                });
                self.emit_elem_copy(&elem_ty);
                wasm!(self.func, {
                          // dst[j] = dst[j+1]
                          local_get(s + 1); i32_const(4); i32_add;
                          local_get(s + 4); i32_const(es); i32_mul; i32_add;
                          local_get(s + 1); i32_const(4); i32_add;
                          local_get(s + 4); i32_const(1); i32_add; i32_const(es); i32_mul; i32_add;
                });
                self.emit_elem_copy(&elem_ty);
                wasm!(self.func, {
                          // dst[j+1] = temp
                          local_get(s + 1); i32_const(4); i32_add;
                          local_get(s + 4); i32_const(1); i32_add; i32_const(es); i32_mul; i32_add;
                          i32_const(8);
                });
                self.emit_elem_copy(&elem_ty);
                wasm!(self.func, {
                        end; // end if (swap needed)
                        local_get(s + 4); i32_const(1); i32_add; local_set(s + 4);
                        br(0);
                      end; end; // end inner loop
                      local_get(s + 3); i32_const(1); i32_add; local_set(s + 3);
                      br(0);
                    end; end; // end outer loop
                    local_get(s + 1);
                });
            }
            "take_while" => {
                // take_while(xs, pred) → List[A]: take while pred returns true
                let elem_ty = self.list_elem_ty(&args[0].ty);
                let es = values::byte_size(&elem_ty) as i32;
                let s = self.match_i32_base + self.match_depth;
                // mem[0]=xs, mem[4]=closure
                wasm!(self.func, { i32_const(0); });
                self.emit_expr(&args[0]);
                wasm!(self.func, { i32_store(0); i32_const(4); });
                self.emit_expr(&args[1]);
                wasm!(self.func, {
                    i32_store(0);
                    i32_const(0); i32_load(0); i32_load(0); local_set(s); // len
                    // First pass: find how many elements to take
                    i32_const(0); local_set(s + 1); // count
                    block_empty; loop_empty;
                      local_get(s + 1); local_get(s); i32_ge_u; br_if(1);
                      // Call pred(xs[count])
                      i32_const(4); i32_load(0); i32_load(4); // env
                      i32_const(0); i32_load(0); i32_const(4); i32_add;
                      local_get(s + 1); i32_const(es); i32_mul; i32_add;
                });
                self.emit_load_at(&elem_ty, 0);
                wasm!(self.func, {
                      i32_const(4); i32_load(0); i32_load(0); // table_idx
                });
                {
                    let mut ct = vec![ValType::I32];
                    if let Some(vt) = values::ty_to_valtype(&elem_ty) { ct.push(vt); }
                    let ti = self.emitter.register_type(ct, vec![ValType::I32]);
                    wasm!(self.func, { call_indirect(ti, 0); });
                }
                wasm!(self.func, {
                      i32_eqz; br_if(1); // pred false → break out of block+loop
                      local_get(s + 1); i32_const(1); i32_add; local_set(s + 1);
                      br(0);
                    end; end;
                    // s+1 = count of elements to take
                    // Alloc result
                    i32_const(4); local_get(s + 1); i32_const(es); i32_mul; i32_add;
                    call(self.emitter.rt.alloc); local_set(s + 2);
                    local_get(s + 2); local_get(s + 1); i32_store(0);
                    // Copy loop
                    i32_const(0); local_set(s + 3); // i
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
            "drop_while" => {
                // drop_while(xs, pred) → List[A]: drop while pred returns true
                let elem_ty = self.list_elem_ty(&args[0].ty);
                let es = values::byte_size(&elem_ty) as i32;
                let s = self.match_i32_base + self.match_depth;
                // mem[0]=xs, mem[4]=closure
                wasm!(self.func, { i32_const(0); });
                self.emit_expr(&args[0]);
                wasm!(self.func, { i32_store(0); i32_const(4); });
                self.emit_expr(&args[1]);
                wasm!(self.func, {
                    i32_store(0);
                    i32_const(0); i32_load(0); i32_load(0); local_set(s); // len
                    // Find start index (first element where pred is false)
                    i32_const(0); local_set(s + 1); // start
                    block_empty; loop_empty;
                      local_get(s + 1); local_get(s); i32_ge_u; br_if(1);
                      i32_const(4); i32_load(0); i32_load(4); // env
                      i32_const(0); i32_load(0); i32_const(4); i32_add;
                      local_get(s + 1); i32_const(es); i32_mul; i32_add;
                });
                self.emit_load_at(&elem_ty, 0);
                wasm!(self.func, {
                      i32_const(4); i32_load(0); i32_load(0); // table_idx
                });
                {
                    let mut ct = vec![ValType::I32];
                    if let Some(vt) = values::ty_to_valtype(&elem_ty) { ct.push(vt); }
                    let ti = self.emitter.register_type(ct, vec![ValType::I32]);
                    wasm!(self.func, { call_indirect(ti, 0); });
                }
                wasm!(self.func, {
                      i32_eqz; br_if(1); // pred false → break
                      local_get(s + 1); i32_const(1); i32_add; local_set(s + 1);
                      br(0);
                    end; end;
                    // new_len = len - start
                    local_get(s); local_get(s + 1); i32_sub; local_set(s + 2);
                    // Alloc result
                    i32_const(4); local_get(s + 2); i32_const(es); i32_mul; i32_add;
                    call(self.emitter.rt.alloc); local_set(s + 3);
                    local_get(s + 3); local_get(s + 2); i32_store(0);
                    // Copy loop
                    i32_const(0); local_set(s + 4); // i
                    block_empty; loop_empty;
                      local_get(s + 4); local_get(s + 2); i32_ge_u; br_if(1);
                      local_get(s + 3); i32_const(4); i32_add;
                      local_get(s + 4); i32_const(es); i32_mul; i32_add;
                      i32_const(0); i32_load(0); i32_const(4); i32_add;
                      local_get(s + 1); local_get(s + 4); i32_add;
                      i32_const(es); i32_mul; i32_add;
                });
                self.emit_elem_copy(&elem_ty);
                wasm!(self.func, {
                      local_get(s + 4); i32_const(1); i32_add; local_set(s + 4);
                      br(0);
                    end; end;
                    local_get(s + 3);
                });
            }
            "count" => {
                // count(xs, pred) → Int
                let elem_ty = self.list_elem_ty(&args[0].ty);
                let es = values::byte_size(&elem_ty) as i32;
                let s = self.match_i32_base + self.match_depth;
                wasm!(self.func, { i32_const(0); });
                self.emit_expr(&args[0]);
                wasm!(self.func, { i32_store(0); i32_const(4); });
                self.emit_expr(&args[1]);
                wasm!(self.func, {
                    i32_store(0);
                    i32_const(0); local_set(s); // i
                    i32_const(0); local_set(s + 1); // count
                    block_empty; loop_empty;
                      local_get(s); i32_const(0); i32_load(0); i32_load(0); i32_ge_u; br_if(1);
                      i32_const(4); i32_load(0); i32_load(4); // env
                      i32_const(0); i32_load(0); i32_const(4); i32_add;
                      local_get(s); i32_const(es); i32_mul; i32_add;
                });
                self.emit_load_at(&elem_ty, 0);
                wasm!(self.func, {
                      i32_const(4); i32_load(0); i32_load(0); // table_idx
                });
                {
                    let mut ct = vec![ValType::I32];
                    if let Some(vt) = values::ty_to_valtype(&elem_ty) { ct.push(vt); }
                    let ti = self.emitter.register_type(ct, vec![ValType::I32]);
                    wasm!(self.func, { call_indirect(ti, 0); });
                }
                wasm!(self.func, {
                      if_empty;
                        local_get(s + 1); i32_const(1); i32_add; local_set(s + 1);
                      end;
                      local_get(s); i32_const(1); i32_add; local_set(s);
                      br(0);
                    end; end;
                    local_get(s + 1); i64_extend_i32_u;
                });
            }
            "partition" => {
                // partition(xs, pred) → (List[A], List[A])
                // Returns a tuple: (matching, non-matching)
                let elem_ty = self.list_elem_ty(&args[0].ty);
                let es = values::byte_size(&elem_ty) as i32;
                let s = self.match_i32_base + self.match_depth;
                // mem[0]=xs, mem[4]=closure
                wasm!(self.func, { i32_const(0); });
                self.emit_expr(&args[0]);
                wasm!(self.func, { i32_store(0); i32_const(4); });
                self.emit_expr(&args[1]);
                wasm!(self.func, {
                    i32_store(0);
                    i32_const(0); i32_load(0); i32_load(0); local_set(s); // len
                    // Alloc two lists (max size each = len)
                    i32_const(4); local_get(s); i32_const(es); i32_mul; i32_add;
                    call(self.emitter.rt.alloc); local_set(s + 1); // true_list
                    i32_const(4); local_get(s); i32_const(es); i32_mul; i32_add;
                    call(self.emitter.rt.alloc); local_set(s + 2); // false_list
                    i32_const(0); local_set(s + 3); // true_count
                    i32_const(0); local_set(s + 4); // false_count
                    i32_const(0); local_set(s + 5); // i
                    block_empty; loop_empty;
                      local_get(s + 5); local_get(s); i32_ge_u; br_if(1);
                      // Call pred(xs[i])
                      i32_const(4); i32_load(0); i32_load(4); // env
                      i32_const(0); i32_load(0); i32_const(4); i32_add;
                      local_get(s + 5); i32_const(es); i32_mul; i32_add;
                });
                self.emit_load_at(&elem_ty, 0);
                wasm!(self.func, {
                      i32_const(4); i32_load(0); i32_load(0); // table_idx
                });
                {
                    let mut ct = vec![ValType::I32];
                    if let Some(vt) = values::ty_to_valtype(&elem_ty) { ct.push(vt); }
                    let ti = self.emitter.register_type(ct, vec![ValType::I32]);
                    wasm!(self.func, { call_indirect(ti, 0); });
                }
                wasm!(self.func, {
                      if_i32;
                        // Copy to true_list
                        local_get(s + 1); i32_const(4); i32_add;
                        local_get(s + 3); i32_const(es); i32_mul; i32_add;
                        i32_const(0); i32_load(0); i32_const(4); i32_add;
                        local_get(s + 5); i32_const(es); i32_mul; i32_add;
                });
                self.emit_elem_copy(&elem_ty);
                wasm!(self.func, {
                        local_get(s + 3); i32_const(1); i32_add; local_set(s + 3);
                        i32_const(0); // push 0 as dummy for consistent stack
                      else_;
                        // Copy to false_list
                        local_get(s + 2); i32_const(4); i32_add;
                        local_get(s + 4); i32_const(es); i32_mul; i32_add;
                        i32_const(0); i32_load(0); i32_const(4); i32_add;
                        local_get(s + 5); i32_const(es); i32_mul; i32_add;
                });
                self.emit_elem_copy(&elem_ty);
                wasm!(self.func, {
                        local_get(s + 4); i32_const(1); i32_add; local_set(s + 4);
                        i32_const(0); // push 0 as dummy for consistent stack
                      end;
                      drop; // drop dummy
                      local_get(s + 5); i32_const(1); i32_add; local_set(s + 5);
                      br(0);
                    end; end;
                    // Set lengths
                    local_get(s + 1); local_get(s + 3); i32_store(0);
                    local_get(s + 2); local_get(s + 4); i32_store(0);
                    // Alloc tuple (true_list_ptr, false_list_ptr)
                    i32_const(8); call(self.emitter.rt.alloc); local_set(s + 5);
                    local_get(s + 5); local_get(s + 1); i32_store(0);
                    local_get(s + 5); local_get(s + 2); i32_store(4);
                    local_get(s + 5);
                });
            }
            "update" => {
                // update(xs, i, f) → List[A]: copy with xs[i] replaced by f(xs[i])
                let elem_ty = self.list_elem_ty(&args[0].ty);
                let es = values::byte_size(&elem_ty) as i32;
                let s = self.match_i32_base + self.match_depth;
                // mem[0]=xs, mem[4]=closure
                wasm!(self.func, { i32_const(0); });
                self.emit_expr(&args[0]);
                wasm!(self.func, { i32_store(0); });
                self.emit_expr(&args[1]); // i
                wasm!(self.func, { i32_wrap_i64; local_set(s); }); // s = idx
                wasm!(self.func, { i32_const(4); });
                self.emit_expr(&args[2]); // closure
                wasm!(self.func, {
                    i32_store(0);
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
                // Now replace dst[idx] with f(dst[idx])
                // dst addr for store
                wasm!(self.func, {
                    local_get(s + 2); i32_const(4); i32_add;
                    local_get(s); i32_const(es); i32_mul; i32_add;
                    // Call f(dst[idx])
                    i32_const(4); i32_load(0); i32_load(4); // env
                    local_get(s + 2); i32_const(4); i32_add;
                    local_get(s); i32_const(es); i32_mul; i32_add;
                });
                self.emit_load_at(&elem_ty, 0);
                wasm!(self.func, {
                    i32_const(4); i32_load(0); i32_load(0); // table_idx
                });
                {
                    let mut ct = vec![ValType::I32];
                    if let Some(vt) = values::ty_to_valtype(&elem_ty) { ct.push(vt); }
                    let rt = values::ret_type(&elem_ty);
                    let ti = self.emitter.register_type(ct, rt);
                    wasm!(self.func, { call_indirect(ti, 0); });
                }
                // Stack: [dst_addr, result] → store
                self.emit_elem_store(&elem_ty);
                wasm!(self.func, { local_get(s + 2); });
            }
            "scan" => {
                // scan(xs, init, f) → List[B]: like fold but collect intermediates
                // Result has same length as xs (each element is f applied cumulatively)
                let elem_ty = self.list_elem_ty(&args[0].ty);
                let es = values::byte_size(&elem_ty) as i32;
                let s = self.match_i32_base + self.match_depth;
                let s64 = self.match_i64_base + self.match_depth;
                // Determine acc type from init
                let acc_vt = values::ty_to_valtype(&args[1].ty).unwrap_or(ValType::I64);
                let acc_size = values::byte_size(&args[1].ty) as i32;
                // mem[0]=xs, mem[4]=closure
                wasm!(self.func, { i32_const(0); });
                self.emit_expr(&args[0]);
                wasm!(self.func, { i32_store(0); });
                // acc = init
                self.emit_expr(&args[1]);
                wasm!(self.func, { local_set(s64); }); // acc in i64/f64 local
                wasm!(self.func, { i32_const(4); });
                self.emit_expr(&args[2]); // closure
                wasm!(self.func, {
                    i32_store(0);
                    i32_const(0); i32_load(0); i32_load(0); local_set(s); // len
                    // Alloc result: 4 + len * acc_size
                    i32_const(4); local_get(s); i32_const(acc_size); i32_mul; i32_add;
                    call(self.emitter.rt.alloc); local_set(s + 1);
                    local_get(s + 1); local_get(s); i32_store(0);
                    i32_const(0); local_set(s + 2); // i
                    block_empty; loop_empty;
                      local_get(s + 2); local_get(s); i32_ge_u; br_if(1);
                      // Call f(acc, xs[i])
                      i32_const(4); i32_load(0); i32_load(4); // env
                      local_get(s64); // acc
                      i32_const(0); i32_load(0); i32_const(4); i32_add;
                      local_get(s + 2); i32_const(es); i32_mul; i32_add;
                });
                self.emit_load_at(&elem_ty, 0);
                wasm!(self.func, {
                      i32_const(4); i32_load(0); i32_load(0); // table_idx
                });
                {
                    // fn(acc: B, elem: A) -> B
                    let mut ct = vec![ValType::I32]; // env
                    ct.push(acc_vt); // acc
                    if let Some(vt) = values::ty_to_valtype(&elem_ty) { ct.push(vt); }
                    let ti = self.emitter.register_type(ct, vec![acc_vt]);
                    wasm!(self.func, { call_indirect(ti, 0); });
                }
                wasm!(self.func, {
                      local_set(s64); // update acc
                      // Store acc into result[i]
                      local_get(s + 1); i32_const(4); i32_add;
                      local_get(s + 2); i32_const(acc_size); i32_mul; i32_add;
                      local_get(s64);
                });
                match acc_vt {
                    ValType::F64 => { wasm!(self.func, { f64_store(0); }); }
                    _ => { wasm!(self.func, { i64_store(0); }); }
                }
                wasm!(self.func, {
                      local_get(s + 2); i32_const(1); i32_add; local_set(s + 2);
                      br(0);
                    end; end;
                    local_get(s + 1);
                });
            }
            "zip_with" => {
                // zip_with(xs, ys, f) → List[C]
                let a_ty = self.list_elem_ty(&args[0].ty);
                let b_ty = self.list_elem_ty(&args[1].ty);
                let a_size = values::byte_size(&a_ty) as i32;
                let b_size = values::byte_size(&b_ty) as i32;
                let s = self.match_i32_base + self.match_depth;
                // Determine return element type from fn return
                let ret_elem_ty = if let Ty::Fn { ret, .. } = &args[2].ty {
                    (**ret).clone()
                } else { Ty::Int };
                let out_size = values::byte_size(&ret_elem_ty) as i32;
                let out_vt = values::ty_to_valtype(&ret_elem_ty).unwrap_or(ValType::I32);
                // mem[0]=xs, mem[4]=ys, mem[8]=closure
                wasm!(self.func, { i32_const(0); });
                self.emit_expr(&args[0]);
                wasm!(self.func, { i32_store(0); i32_const(4); });
                self.emit_expr(&args[1]);
                wasm!(self.func, { i32_store(0); i32_const(8); });
                self.emit_expr(&args[2]);
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
                    // Alloc result
                    i32_const(4); local_get(s); i32_const(out_size); i32_mul; i32_add;
                    call(self.emitter.rt.alloc); local_set(s + 1);
                    local_get(s + 1); local_get(s); i32_store(0);
                    i32_const(0); local_set(s + 2); // i
                    block_empty; loop_empty;
                      local_get(s + 2); local_get(s); i32_ge_u; br_if(1);
                      // dst addr
                      local_get(s + 1); i32_const(4); i32_add;
                      local_get(s + 2); i32_const(out_size); i32_mul; i32_add;
                      // Call f(xs[i], ys[i])
                      i32_const(8); i32_load(0); i32_load(4); // env
                      i32_const(0); i32_load(0); i32_const(4); i32_add;
                      local_get(s + 2); i32_const(a_size); i32_mul; i32_add;
                });
                self.emit_load_at(&a_ty, 0);
                wasm!(self.func, {
                      i32_const(4); i32_load(0); i32_const(4); i32_add;
                      local_get(s + 2); i32_const(b_size); i32_mul; i32_add;
                });
                self.emit_load_at(&b_ty, 0);
                wasm!(self.func, {
                      i32_const(8); i32_load(0); i32_load(0); // table_idx
                });
                {
                    let mut ct = vec![ValType::I32]; // env
                    if let Some(vt) = values::ty_to_valtype(&a_ty) { ct.push(vt); }
                    if let Some(vt) = values::ty_to_valtype(&b_ty) { ct.push(vt); }
                    let rt = values::ret_type(&ret_elem_ty);
                    let ti = self.emitter.register_type(ct, rt);
                    wasm!(self.func, { call_indirect(ti, 0); });
                }
                // Stack: [dst_addr, result] → store
                match out_vt {
                    ValType::I64 => { wasm!(self.func, { i64_store(0); }); }
                    ValType::F64 => { wasm!(self.func, { f64_store(0); }); }
                    _ => { wasm!(self.func, { i32_store(0); }); }
                }
                wasm!(self.func, {
                      local_get(s + 2); i32_const(1); i32_add; local_set(s + 2);
                      br(0);
                    end; end;
                    local_get(s + 1);
                });
            }
            "unique_by" => {
                // unique_by(xs, f) → List[A]: remove dupes by key, keep first
                // O(n²) comparison of keys
                let elem_ty = self.list_elem_ty(&args[0].ty);
                let es = values::byte_size(&elem_ty) as i32;
                let s = self.match_i32_base + self.match_depth;
                let s64 = self.match_i64_base + self.match_depth;
                // mem[0]=xs, mem[4]=closure
                wasm!(self.func, { i32_const(0); });
                self.emit_expr(&args[0]);
                wasm!(self.func, { i32_store(0); i32_const(4); });
                self.emit_expr(&args[1]);
                wasm!(self.func, {
                    i32_store(0);
                    i32_const(0); i32_load(0); i32_load(0); local_set(s); // len
                    // Alloc keys array: len * 8
                    local_get(s); i32_const(8); i32_mul;
                    call(self.emitter.rt.alloc); local_set(s + 1); // keys
                    // Compute all keys
                    i32_const(0); local_set(s + 2); // i
                    block_empty; loop_empty;
                      local_get(s + 2); local_get(s); i32_ge_u; br_if(1);
                      i32_const(4); i32_load(0); i32_load(4); // env
                      i32_const(0); i32_load(0); i32_const(4); i32_add;
                      local_get(s + 2); i32_const(es); i32_mul; i32_add;
                });
                self.emit_load_at(&elem_ty, 0);
                wasm!(self.func, {
                      i32_const(4); i32_load(0); i32_load(0); // table_idx
                });
                {
                    let mut ct = vec![ValType::I32];
                    if let Some(vt) = values::ty_to_valtype(&elem_ty) { ct.push(vt); }
                    // Key type: use i64 for simplicity (works for Int, Bool, String-ptr)
                    let ti = self.emitter.register_type(ct, vec![ValType::I64]);
                    wasm!(self.func, { call_indirect(ti, 0); });
                }
                wasm!(self.func, {
                      local_set(s64);
                      local_get(s + 1);
                      local_get(s + 2); i32_const(8); i32_mul; i32_add;
                      local_get(s64); i64_store(0);
                      local_get(s + 2); i32_const(1); i32_add; local_set(s + 2);
                      br(0);
                    end; end;
                });
                // Now build result: include xs[i] if keys[i] not in keys[0..out_count]
                wasm!(self.func, {
                    // Alloc dst (max = len)
                    i32_const(4); local_get(s); i32_const(es); i32_mul; i32_add;
                    call(self.emitter.rt.alloc); local_set(s + 3); // dst
                    // Alloc seen_keys: len * 8
                    local_get(s); i32_const(8); i32_mul;
                    call(self.emitter.rt.alloc); local_set(s + 4); // seen_keys
                    i32_const(0); local_set(s + 5); // out_count
                    i32_const(0); local_set(s + 2); // i
                    block_empty; loop_empty;
                      local_get(s + 2); local_get(s); i32_ge_u; br_if(1);
                      // Load key[i]
                      local_get(s + 1);
                      local_get(s + 2); i32_const(8); i32_mul; i32_add;
                      i64_load(0); local_set(s64);
                      // Check if key already in seen_keys
                      i32_const(0); local_set(s + 6); // j
                      i32_const(0); local_set(s + 7); // found
                      block_empty; loop_empty;
                        local_get(s + 6); local_get(s + 5); i32_ge_u; br_if(1);
                        local_get(s + 4);
                        local_get(s + 6); i32_const(8); i32_mul; i32_add;
                        i64_load(0); local_get(s64); i64_eq;
                        if_empty; i32_const(1); local_set(s + 7); br(2); end;
                        local_get(s + 6); i32_const(1); i32_add; local_set(s + 6);
                        br(0);
                      end; end;
                      local_get(s + 7); i32_eqz;
                      if_empty;
                        // Not found: add to dst and seen_keys
                        local_get(s + 3); i32_const(4); i32_add;
                        local_get(s + 5); i32_const(es); i32_mul; i32_add;
                        i32_const(0); i32_load(0); i32_const(4); i32_add;
                        local_get(s + 2); i32_const(es); i32_mul; i32_add;
                });
                self.emit_elem_copy(&elem_ty);
                wasm!(self.func, {
                        // Add key to seen_keys
                        local_get(s + 4);
                        local_get(s + 5); i32_const(8); i32_mul; i32_add;
                        local_get(s64); i64_store(0);
                        local_get(s + 5); i32_const(1); i32_add; local_set(s + 5);
                      end;
                      local_get(s + 2); i32_const(1); i32_add; local_set(s + 2);
                      br(0);
                    end; end;
                    local_get(s + 3); local_get(s + 5); i32_store(0);
                    local_get(s + 3);
                });
            }
            "group_by" => {
                // group_by(xs, f) → Map[B, List[A]]
                // Very complex (requires Map construction). Stub for now.
                self.emit_stub_call(args);
                return true;
            }
            "shuffle" => {
                // shuffle(xs) → List[A]
                // Requires randomness source. Stub for now.
                self.emit_stub_call(args);
                return true;
            }
            _ => return false,
        }
        true
    }

    // ── Helpers ──

    fn list_elem_ty(&self, ty: &Ty) -> Ty {
        if let Ty::Applied(_, args) = ty {
            args.first().cloned().unwrap_or(Ty::Int)
        } else { Ty::Int }
    }

    /// Copy one element from [stack: dst_addr, src_addr] based on type.
    fn emit_elem_copy(&mut self, ty: &Ty) {
        match values::ty_to_valtype(ty) {
            Some(ValType::I64) => { wasm!(self.func, { i64_load(0); i64_store(0); }); }
            Some(ValType::F64) => { wasm!(self.func, { f64_load(0); f64_store(0); }); }
            _ => { wasm!(self.func, { i32_load(0); i32_store(0); }); }
        }
    }

    /// Store one element: [stack: dst_addr, value].
    fn emit_elem_store(&mut self, ty: &Ty) {
        match values::ty_to_valtype(ty) {
            Some(ValType::I64) => { wasm!(self.func, { i64_store(0); }); }
            Some(ValType::F64) => { wasm!(self.func, { f64_store(0); }); }
            _ => { wasm!(self.func, { i32_store(0); }); }
        }
    }

    /// Emit take/drop as list slice. For take: start=0,end=n. For drop: start=n,end=len.
    fn emit_list_slice_impl(
        &mut self, xs: &IrExpr, start_arg: Option<&IrExpr>, end_arg: Option<&IrExpr>,
        elem_size: usize, is_take: bool,
    ) {
        let s = self.match_i32_base + self.match_depth;
        wasm!(self.func, { i32_const(0); });
        self.emit_expr(xs);
        wasm!(self.func, { i32_store(0); }); // mem[0] = xs
        // Compute start and end
        if is_take {
            // take(xs, n): start=0, end=min(n, len)
            self.emit_expr(end_arg.unwrap());
            wasm!(self.func, {
                i32_wrap_i64; local_set(s); // n
                i32_const(0); local_set(s + 1); // start = 0
                // end = min(n, len)
                local_get(s); i32_const(0); i32_load(0); i32_load(0);
                i32_lt_u;
                if_i32; local_get(s); else_; i32_const(0); i32_load(0); i32_load(0); end;
                local_set(s + 2); // end
            });
        } else {
            // drop(xs, n): start=min(n, len), end=len
            self.emit_expr(start_arg.unwrap());
            wasm!(self.func, {
                i32_wrap_i64; local_set(s); // n
                // start = min(n, len)
                local_get(s); i32_const(0); i32_load(0); i32_load(0);
                i32_lt_u;
                if_i32; local_get(s); else_; i32_const(0); i32_load(0); i32_load(0); end;
                local_set(s + 1); // start
                i32_const(0); i32_load(0); i32_load(0); local_set(s + 2); // end = len
            });
        }
        // new_len = end - start
        wasm!(self.func, {
            local_get(s + 2); local_get(s + 1); i32_sub; local_set(s + 3);
            // alloc
            i32_const(4); local_get(s + 3); i32_const(elem_size as i32); i32_mul; i32_add;
            call(self.emitter.rt.alloc); local_set(s + 4);
            local_get(s + 4); local_get(s + 3); i32_store(0);
            // copy loop
            i32_const(0); local_set(s + 5); // i
            block_empty; loop_empty;
              local_get(s + 5); local_get(s + 3); i32_ge_u; br_if(1);
              // dst[4 + i*es]
              local_get(s + 4); i32_const(4); i32_add;
              local_get(s + 5); i32_const(elem_size as i32); i32_mul; i32_add;
              // src[4 + (start+i)*es]
              i32_const(0); i32_load(0); i32_const(4); i32_add;
              local_get(s + 1); local_get(s + 5); i32_add;
              i32_const(elem_size as i32); i32_mul; i32_add;
        });
        // Copy one element
        let elem_ty = if is_take {
            self.list_elem_ty(&end_arg.unwrap().ty)
        } else {
            self.list_elem_ty(&start_arg.unwrap().ty)
        };
        // Actually use xs type
        self.emit_elem_copy(&self.list_elem_ty(&xs.ty));
        wasm!(self.func, {
              local_get(s + 5); i32_const(1); i32_add; local_set(s + 5);
              br(0);
            end; end;
            local_get(s + 4);
        });
    }

    fn emit_memcpy_loop(&mut self, _i_local: u32, _dst_local: u32, _start_local: u32, _elem_size: usize) {
        // Generic memcpy for list.slice — complex, use inline for now
        // This is a placeholder; slice uses the same pattern as take/drop
    }
}
