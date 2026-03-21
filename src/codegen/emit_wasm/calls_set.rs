//! Set stdlib call dispatch for WASM codegen.
//!
//! Set layout: [len:i32][elem0:K_size][elem1:K_size]...
//! Essentially a List with unique elements.

use super::FuncCompiler;
use super::values;
use crate::ir::IrExpr;
use crate::types::Ty;
use wasm_encoder::ValType;

impl FuncCompiler<'_> {
    pub(super) fn emit_set_call(&mut self, method: &str, args: &[IrExpr]) -> bool {
        match method {
            "new" => {
                let s = self.match_i32_base + self.match_depth;
                wasm!(self.func, {
                    i32_const(4); call(self.emitter.rt.alloc); local_set(s);
                    local_get(s); i32_const(0); i32_store(0);
                    local_get(s);
                });
            }
            "len" | "size" => {
                self.emit_expr(&args[0]);
                wasm!(self.func, { i32_load(0); i64_extend_i32_u; });
            }
            "is_empty" => {
                self.emit_expr(&args[0]);
                wasm!(self.func, { i32_load(0); i32_eqz; });
            }
            "from_list" => {
                // from_list(xs) → Set[A]: copy list, dedup
                let elem_ty = self.list_elem_ty(&args[0].ty);
                let es = values::byte_size(&elem_ty) as i32;
                let s = self.match_i32_base + self.match_depth;
                // Start with empty set
                wasm!(self.func, {
                    i32_const(4); call(self.emitter.rt.alloc); local_set(s);
                    local_get(s); i32_const(0); i32_store(0);
                    i32_const(0);
                });
                self.emit_expr(&args[0]);
                wasm!(self.func, {
                    i32_store(0); // mem[0] = xs
                    i32_const(0); local_set(s + 1); // i
                    block_empty; loop_empty;
                      local_get(s + 1); i32_const(0); i32_load(0); i32_load(0); i32_ge_u; br_if(1);
                      // Check if elem already in set
                      i32_const(0); local_set(s + 2); // j
                      i32_const(0); local_set(s + 3); // found
                      block_empty; loop_empty;
                        local_get(s + 2); local_get(s); i32_load(0); i32_ge_u; br_if(1);
                        local_get(s); i32_const(4); i32_add;
                        local_get(s + 2); i32_const(es); i32_mul; i32_add;
                });
                self.emit_load_at(&elem_ty, 0);
                wasm!(self.func, {
                        i32_const(0); i32_load(0); i32_const(4); i32_add;
                        local_get(s + 1); i32_const(es); i32_mul; i32_add;
                });
                self.emit_load_at(&elem_ty, 0);
                self.emit_set_elem_eq(&elem_ty);
                wasm!(self.func, {
                        if_empty; i32_const(1); local_set(s + 3); br(2); end;
                        local_get(s + 2); i32_const(1); i32_add; local_set(s + 2);
                        br(0);
                      end; end;
                      // If not found, append
                      local_get(s + 3); i32_eqz;
                      if_empty;
                        // Grow set: alloc new with len+1
                        i32_const(4); local_get(s); i32_load(0); i32_const(1); i32_add;
                        i32_const(es); i32_mul; i32_add;
                        call(self.emitter.rt.alloc); local_set(s + 2);
                        local_get(s + 2); local_get(s); i32_load(0); i32_const(1); i32_add; i32_store(0);
                        // Copy old elements
                        i32_const(0); local_set(s + 3);
                        block_empty; loop_empty;
                          local_get(s + 3); local_get(s); i32_load(0); i32_ge_u; br_if(1);
                          local_get(s + 2); i32_const(4); i32_add;
                          local_get(s + 3); i32_const(es); i32_mul; i32_add;
                          local_get(s); i32_const(4); i32_add;
                          local_get(s + 3); i32_const(es); i32_mul; i32_add;
                });
                self.emit_elem_copy(&elem_ty);
                wasm!(self.func, {
                          local_get(s + 3); i32_const(1); i32_add; local_set(s + 3);
                          br(0);
                        end; end;
                        // Append new element
                        local_get(s + 2); i32_const(4); i32_add;
                        local_get(s); i32_load(0); i32_const(es); i32_mul; i32_add;
                        i32_const(0); i32_load(0); i32_const(4); i32_add;
                        local_get(s + 1); i32_const(es); i32_mul; i32_add;
                });
                self.emit_elem_copy(&elem_ty);
                wasm!(self.func, {
                        local_get(s + 2); local_set(s);
                      end;
                      local_get(s + 1); i32_const(1); i32_add; local_set(s + 1);
                      br(0);
                    end; end;
                    local_get(s);
                });
            }
            "contains" => {
                let elem_ty = self.set_elem_ty(&args[0].ty);
                let es = values::byte_size(&elem_ty) as i32;
                let s = self.match_i32_base + self.match_depth;
                self.emit_expr(&args[0]);
                wasm!(self.func, { local_set(s); });
                match values::ty_to_valtype(&elem_ty) {
                    Some(ValType::I64) => {
                        let s64 = self.match_i64_base + self.match_depth;
                        self.emit_expr(&args[1]);
                        wasm!(self.func, {
                            local_set(s64);
                            i32_const(0); local_set(s + 1);
                            i32_const(0); local_set(s + 2);
                            block_empty; loop_empty;
                              local_get(s + 1); local_get(s); i32_load(0); i32_ge_u; br_if(1);
                              local_get(s); i32_const(4); i32_add;
                              local_get(s + 1); i32_const(es); i32_mul; i32_add;
                              i64_load(0); local_get(s64); i64_eq;
                              if_empty; i32_const(1); local_set(s + 2); br(2); end;
                              local_get(s + 1); i32_const(1); i32_add; local_set(s + 1);
                              br(0);
                            end; end;
                            local_get(s + 2);
                        });
                    }
                    _ => {
                        self.emit_expr(&args[1]);
                        wasm!(self.func, {
                            local_set(s + 3);
                            i32_const(0); local_set(s + 1);
                            i32_const(0); local_set(s + 2);
                            block_empty; loop_empty;
                              local_get(s + 1); local_get(s); i32_load(0); i32_ge_u; br_if(1);
                              local_get(s); i32_const(4); i32_add;
                              local_get(s + 1); i32_const(es); i32_mul; i32_add;
                              i32_load(0); local_get(s + 3);
                        });
                        if matches!(&elem_ty, Ty::String) {
                            wasm!(self.func, { call(self.emitter.rt.string.eq); });
                        } else {
                            wasm!(self.func, { i32_eq; });
                        }
                        wasm!(self.func, {
                              if_empty; i32_const(1); local_set(s + 2); br(2); end;
                              local_get(s + 1); i32_const(1); i32_add; local_set(s + 1);
                              br(0);
                            end; end;
                            local_get(s + 2);
                        });
                    }
                }
            }
            "insert" => {
                let elem_ty = self.set_elem_ty(&args[0].ty);
                let es = values::byte_size(&elem_ty) as i32;
                let s = self.match_i32_base + self.match_depth;
                self.emit_expr(&args[0]);
                wasm!(self.func, { local_set(s); i32_const(0); });
                self.emit_expr(&args[1]);
                self.emit_store_at(&elem_ty, 0); // mem[0] = val
                wasm!(self.func, {
                    i32_const(0); local_set(s + 1); // i
                    i32_const(0); local_set(s + 2); // found
                    block_empty; loop_empty;
                      local_get(s + 1); local_get(s); i32_load(0); i32_ge_u; br_if(1);
                      local_get(s); i32_const(4); i32_add;
                      local_get(s + 1); i32_const(es); i32_mul; i32_add;
                });
                self.emit_load_at(&elem_ty, 0);
                wasm!(self.func, { i32_const(0); });
                self.emit_load_at(&elem_ty, 0);
                self.emit_set_elem_eq(&elem_ty);
                wasm!(self.func, {
                      if_empty; i32_const(1); local_set(s + 2); br(2); end;
                      local_get(s + 1); i32_const(1); i32_add; local_set(s + 1);
                      br(0);
                    end; end;
                    local_get(s + 2);
                    if_i32; local_get(s);
                    else_;
                      i32_const(4); local_get(s); i32_load(0); i32_const(1); i32_add;
                      i32_const(es); i32_mul; i32_add;
                      call(self.emitter.rt.alloc); local_set(s + 1);
                      local_get(s + 1); local_get(s); i32_load(0); i32_const(1); i32_add; i32_store(0);
                      i32_const(0); local_set(s + 2);
                      block_empty; loop_empty;
                        local_get(s + 2); local_get(s); i32_load(0); i32_ge_u; br_if(1);
                        local_get(s + 1); i32_const(4); i32_add;
                        local_get(s + 2); i32_const(es); i32_mul; i32_add;
                        local_get(s); i32_const(4); i32_add;
                        local_get(s + 2); i32_const(es); i32_mul; i32_add;
                });
                self.emit_elem_copy(&elem_ty);
                wasm!(self.func, {
                        local_get(s + 2); i32_const(1); i32_add; local_set(s + 2);
                        br(0);
                      end; end;
                      local_get(s + 1); i32_const(4); i32_add;
                      local_get(s); i32_load(0); i32_const(es); i32_mul; i32_add;
                      i32_const(0);
                });
                self.emit_load_at(&elem_ty, 0);
                self.emit_store_at(&elem_ty, 0);
                wasm!(self.func, { local_get(s + 1); end; });
            }
            "remove" => {
                let elem_ty = self.set_elem_ty(&args[0].ty);
                let es = values::byte_size(&elem_ty) as i32;
                let s = self.match_i32_base + self.match_depth;
                self.emit_expr(&args[0]);
                wasm!(self.func, { local_set(s); i32_const(0); });
                self.emit_expr(&args[1]);
                self.emit_store_at(&elem_ty, 0); // mem[0] = val
                wasm!(self.func, {
                    i32_const(-1); local_set(s + 1); // found_idx
                    i32_const(0); local_set(s + 2);
                    block_empty; loop_empty;
                      local_get(s + 2); local_get(s); i32_load(0); i32_ge_u; br_if(1);
                      local_get(s); i32_const(4); i32_add;
                      local_get(s + 2); i32_const(es); i32_mul; i32_add;
                });
                self.emit_load_at(&elem_ty, 0);
                wasm!(self.func, { i32_const(0); });
                self.emit_load_at(&elem_ty, 0);
                self.emit_set_elem_eq(&elem_ty);
                wasm!(self.func, {
                      if_empty; local_get(s + 2); local_set(s + 1); br(2); end;
                      local_get(s + 2); i32_const(1); i32_add; local_set(s + 2);
                      br(0);
                    end; end;
                    local_get(s + 1); i32_const(0); i32_lt_s;
                    if_i32; local_get(s); // not found
                    else_;
                      // Store original set ptr at mem[4]
                      i32_const(4); local_get(s); i32_store(0);
                      i32_const(4); i32_const(4); i32_load(0); i32_load(0); i32_const(1); i32_sub;
                      i32_const(es); i32_mul; i32_add;
                      call(self.emitter.rt.alloc); local_set(s + 2);
                      local_get(s + 2);
                      i32_const(4); i32_load(0); i32_load(0); i32_const(1); i32_sub;
                      i32_store(0);
                      i32_const(0); local_set(s + 3); // src_i
                      i32_const(0); local_set(s);     // dst_i
                      block_empty; loop_empty;
                        local_get(s + 3);
                        i32_const(4); i32_load(0); i32_load(0); // original len
                        i32_ge_u; br_if(1);
                        local_get(s + 3); local_get(s + 1); i32_ne;
                        if_empty;
                          local_get(s + 2); i32_const(4); i32_add;
                          local_get(s); i32_const(es); i32_mul; i32_add;
                          i32_const(4); i32_load(0); i32_const(4); i32_add;
                          local_get(s + 3); i32_const(es); i32_mul; i32_add;
                });
                self.emit_elem_copy(&elem_ty);
                wasm!(self.func, {
                          local_get(s); i32_const(1); i32_add; local_set(s);
                        end;
                        local_get(s + 3); i32_const(1); i32_add; local_set(s + 3);
                        br(0);
                      end; end;
                      local_get(s + 2);
                    end;
                });
            }
            "union" => {
                let elem_ty = self.set_elem_ty(&args[0].ty);
                let es = values::byte_size(&elem_ty) as i32;
                let s = self.match_i32_base + self.match_depth;
                self.emit_expr(&args[0]);
                wasm!(self.func, { local_set(s); i32_const(0); });
                self.emit_expr(&args[1]);
                wasm!(self.func, {
                    i32_store(0); // mem[0] = b
                    local_get(s); i32_load(0); local_set(s + 1); // a_len
                    i32_const(0); i32_load(0); i32_load(0); local_set(s + 2); // b_len
                    i32_const(4); local_get(s + 1); local_get(s + 2); i32_add;
                    i32_const(es); i32_mul; i32_add;
                    call(self.emitter.rt.alloc); local_set(s + 3);
                    local_get(s + 3); local_get(s + 1); local_get(s + 2); i32_add; i32_store(0);
                    // Copy a
                    i32_const(0); local_set(s + 2); // i
                    block_empty; loop_empty;
                      local_get(s + 2); local_get(s + 1); i32_ge_u; br_if(1);
                      local_get(s + 3); i32_const(4); i32_add;
                      local_get(s + 2); i32_const(es); i32_mul; i32_add;
                      local_get(s); i32_const(4); i32_add;
                      local_get(s + 2); i32_const(es); i32_mul; i32_add;
                });
                self.emit_elem_copy(&elem_ty);
                wasm!(self.func, {
                      local_get(s + 2); i32_const(1); i32_add; local_set(s + 2);
                      br(0);
                    end; end;
                    // Copy b
                    i32_const(0); local_set(s + 2);
                    block_empty; loop_empty;
                      local_get(s + 2); i32_const(0); i32_load(0); i32_load(0); i32_ge_u; br_if(1);
                      local_get(s + 3); i32_const(4); i32_add;
                      local_get(s + 1); local_get(s + 2); i32_add;
                      i32_const(es); i32_mul; i32_add;
                      i32_const(0); i32_load(0); i32_const(4); i32_add;
                      local_get(s + 2); i32_const(es); i32_mul; i32_add;
                });
                self.emit_elem_copy(&elem_ty);
                wasm!(self.func, {
                      local_get(s + 2); i32_const(1); i32_add; local_set(s + 2);
                      br(0);
                    end; end;
                    local_get(s + 3);
                });
            }
            "to_list" => {
                // Set has same layout as List — just return the ptr
                self.emit_expr(&args[0]);
            }
            "intersection" => {
                // intersection(a, b) → Set[A]: elements in both a and b.
                // For each a[i], scan b for a match. Use j as loop counter;
                // after inner loop, j < b.len means we found a match (broke early).
                // Locals: s=result, s+1=out_count, s+2=i, s+3=j
                let elem_ty = self.set_elem_ty(&args[0].ty);
                let es = values::byte_size(&elem_ty) as i32;
                let s = self.match_i32_base + self.match_depth;
                // mem[0]=a, mem[4]=b
                wasm!(self.func, { i32_const(0); });
                self.emit_expr(&args[0]);
                wasm!(self.func, { i32_store(0); i32_const(4); });
                self.emit_expr(&args[1]);
                wasm!(self.func, {
                    i32_store(0);
                    // result = alloc(4 + a.len * es)
                    i32_const(4); i32_const(0); i32_load(0); i32_load(0);
                    i32_const(es); i32_mul; i32_add;
                    call(self.emitter.rt.alloc); local_set(s);
                    i32_const(0); local_set(s + 1); // out_count
                    i32_const(0); local_set(s + 2); // i
                    block_empty; loop_empty;
                      local_get(s + 2); i32_const(0); i32_load(0); i32_load(0); i32_ge_u; br_if(1);
                      // Inner: scan b for a[i]
                      i32_const(0); local_set(s + 3); // j
                      block_empty; loop_empty;
                        local_get(s + 3); i32_const(4); i32_load(0); i32_load(0); i32_ge_u; br_if(1);
                        // Load a[i]
                        i32_const(0); i32_load(0); i32_const(4); i32_add;
                        local_get(s + 2); i32_const(es); i32_mul; i32_add;
                });
                self.emit_load_at(&elem_ty, 0);
                wasm!(self.func, {
                        // Load b[j]
                        i32_const(4); i32_load(0); i32_const(4); i32_add;
                        local_get(s + 3); i32_const(es); i32_mul; i32_add;
                });
                self.emit_load_at(&elem_ty, 0);
                self.emit_set_elem_eq(&elem_ty);
                wasm!(self.func, {
                        br_if(1); // found → break out of inner loop
                        local_get(s + 3); i32_const(1); i32_add; local_set(s + 3);
                        br(0);
                      end; end;
                      // After inner loop: j < b.len means found
                      local_get(s + 3); i32_const(4); i32_load(0); i32_load(0); i32_lt_u;
                      if_empty;
                        // Copy a[i] to result[out_count]
                        local_get(s); i32_const(4); i32_add;
                        local_get(s + 1); i32_const(es); i32_mul; i32_add;
                        i32_const(0); i32_load(0); i32_const(4); i32_add;
                        local_get(s + 2); i32_const(es); i32_mul; i32_add;
                });
                self.emit_elem_copy(&elem_ty);
                wasm!(self.func, {
                        local_get(s + 1); i32_const(1); i32_add; local_set(s + 1);
                      end;
                      local_get(s + 2); i32_const(1); i32_add; local_set(s + 2);
                      br(0);
                    end; end;
                    local_get(s); local_get(s + 1); i32_store(0);
                    local_get(s);
                });
            }
            "difference" => {
                // difference(a, b) → Set[A]: elements in a but not in b.
                // Locals: s=result, s+1=out_count, s+2=i, s+3=j
                let elem_ty = self.set_elem_ty(&args[0].ty);
                let es = values::byte_size(&elem_ty) as i32;
                let s = self.match_i32_base + self.match_depth;
                wasm!(self.func, { i32_const(0); });
                self.emit_expr(&args[0]);
                wasm!(self.func, { i32_store(0); i32_const(4); });
                self.emit_expr(&args[1]);
                wasm!(self.func, {
                    i32_store(0);
                    i32_const(4); i32_const(0); i32_load(0); i32_load(0);
                    i32_const(es); i32_mul; i32_add;
                    call(self.emitter.rt.alloc); local_set(s);
                    i32_const(0); local_set(s + 1);
                    i32_const(0); local_set(s + 2);
                    block_empty; loop_empty;
                      local_get(s + 2); i32_const(0); i32_load(0); i32_load(0); i32_ge_u; br_if(1);
                      i32_const(0); local_set(s + 3); // j
                      block_empty; loop_empty;
                        local_get(s + 3); i32_const(4); i32_load(0); i32_load(0); i32_ge_u; br_if(1);
                        i32_const(0); i32_load(0); i32_const(4); i32_add;
                        local_get(s + 2); i32_const(es); i32_mul; i32_add;
                });
                self.emit_load_at(&elem_ty, 0);
                wasm!(self.func, {
                        i32_const(4); i32_load(0); i32_const(4); i32_add;
                        local_get(s + 3); i32_const(es); i32_mul; i32_add;
                });
                self.emit_load_at(&elem_ty, 0);
                self.emit_set_elem_eq(&elem_ty);
                wasm!(self.func, {
                        br_if(1); // found → break
                        local_get(s + 3); i32_const(1); i32_add; local_set(s + 3);
                        br(0);
                      end; end;
                      // j >= b.len means NOT found → keep element
                      local_get(s + 3); i32_const(4); i32_load(0); i32_load(0); i32_ge_u;
                      if_empty;
                        local_get(s); i32_const(4); i32_add;
                        local_get(s + 1); i32_const(es); i32_mul; i32_add;
                        i32_const(0); i32_load(0); i32_const(4); i32_add;
                        local_get(s + 2); i32_const(es); i32_mul; i32_add;
                });
                self.emit_elem_copy(&elem_ty);
                wasm!(self.func, {
                        local_get(s + 1); i32_const(1); i32_add; local_set(s + 1);
                      end;
                      local_get(s + 2); i32_const(1); i32_add; local_set(s + 2);
                      br(0);
                    end; end;
                    local_get(s); local_get(s + 1); i32_store(0);
                    local_get(s);
                });
            }
            "symmetric_difference" => {
                // symmetric_difference(a, b) = difference(a,b) ∪ difference(b,a)
                // Two passes, collecting into one result buffer.
                // Locals: s=result, s+1=out_count, s+2=i, s+3=j
                let elem_ty = self.set_elem_ty(&args[0].ty);
                let es = values::byte_size(&elem_ty) as i32;
                let s = self.match_i32_base + self.match_depth;
                // mem[0]=a, mem[4]=b
                wasm!(self.func, { i32_const(0); });
                self.emit_expr(&args[0]);
                wasm!(self.func, { i32_store(0); i32_const(4); });
                self.emit_expr(&args[1]);
                wasm!(self.func, {
                    i32_store(0);
                    // result = alloc(4 + (a.len + b.len) * es)
                    i32_const(4);
                    i32_const(0); i32_load(0); i32_load(0);
                    i32_const(4); i32_load(0); i32_load(0);
                    i32_add; i32_const(es); i32_mul; i32_add;
                    call(self.emitter.rt.alloc); local_set(s);
                    i32_const(0); local_set(s + 1); // out_count
                });
                // Pass 1: elements of a not in b
                wasm!(self.func, {
                    i32_const(0); local_set(s + 2);
                    block_empty; loop_empty;
                      local_get(s + 2); i32_const(0); i32_load(0); i32_load(0); i32_ge_u; br_if(1);
                      i32_const(0); local_set(s + 3);
                      block_empty; loop_empty;
                        local_get(s + 3); i32_const(4); i32_load(0); i32_load(0); i32_ge_u; br_if(1);
                        i32_const(0); i32_load(0); i32_const(4); i32_add;
                        local_get(s + 2); i32_const(es); i32_mul; i32_add;
                });
                self.emit_load_at(&elem_ty, 0);
                wasm!(self.func, {
                        i32_const(4); i32_load(0); i32_const(4); i32_add;
                        local_get(s + 3); i32_const(es); i32_mul; i32_add;
                });
                self.emit_load_at(&elem_ty, 0);
                self.emit_set_elem_eq(&elem_ty);
                wasm!(self.func, {
                        br_if(1);
                        local_get(s + 3); i32_const(1); i32_add; local_set(s + 3);
                        br(0);
                      end; end;
                      local_get(s + 3); i32_const(4); i32_load(0); i32_load(0); i32_ge_u;
                      if_empty;
                        local_get(s); i32_const(4); i32_add;
                        local_get(s + 1); i32_const(es); i32_mul; i32_add;
                        i32_const(0); i32_load(0); i32_const(4); i32_add;
                        local_get(s + 2); i32_const(es); i32_mul; i32_add;
                });
                self.emit_elem_copy(&elem_ty);
                wasm!(self.func, {
                        local_get(s + 1); i32_const(1); i32_add; local_set(s + 1);
                      end;
                      local_get(s + 2); i32_const(1); i32_add; local_set(s + 2);
                      br(0);
                    end; end;
                });
                // Pass 2: elements of b not in a
                wasm!(self.func, {
                    i32_const(0); local_set(s + 2);
                    block_empty; loop_empty;
                      local_get(s + 2); i32_const(4); i32_load(0); i32_load(0); i32_ge_u; br_if(1);
                      i32_const(0); local_set(s + 3);
                      block_empty; loop_empty;
                        local_get(s + 3); i32_const(0); i32_load(0); i32_load(0); i32_ge_u; br_if(1);
                        i32_const(4); i32_load(0); i32_const(4); i32_add;
                        local_get(s + 2); i32_const(es); i32_mul; i32_add;
                });
                self.emit_load_at(&elem_ty, 0);
                wasm!(self.func, {
                        i32_const(0); i32_load(0); i32_const(4); i32_add;
                        local_get(s + 3); i32_const(es); i32_mul; i32_add;
                });
                self.emit_load_at(&elem_ty, 0);
                self.emit_set_elem_eq(&elem_ty);
                wasm!(self.func, {
                        br_if(1);
                        local_get(s + 3); i32_const(1); i32_add; local_set(s + 3);
                        br(0);
                      end; end;
                      local_get(s + 3); i32_const(0); i32_load(0); i32_load(0); i32_ge_u;
                      if_empty;
                        local_get(s); i32_const(4); i32_add;
                        local_get(s + 1); i32_const(es); i32_mul; i32_add;
                        i32_const(4); i32_load(0); i32_const(4); i32_add;
                        local_get(s + 2); i32_const(es); i32_mul; i32_add;
                });
                self.emit_elem_copy(&elem_ty);
                wasm!(self.func, {
                        local_get(s + 1); i32_const(1); i32_add; local_set(s + 1);
                      end;
                      local_get(s + 2); i32_const(1); i32_add; local_set(s + 2);
                      br(0);
                    end; end;
                    local_get(s); local_get(s + 1); i32_store(0);
                    local_get(s);
                });
            }
            "is_subset" => {
                // is_subset(a, b) → Bool: all elements of a are in b.
                // Locals: s=result, s+1=i, s+2=j. Use j sentinel: j < b.len = found.
                let elem_ty = self.set_elem_ty(&args[0].ty);
                let es = values::byte_size(&elem_ty) as i32;
                let s = self.match_i32_base + self.match_depth;
                wasm!(self.func, { i32_const(0); });
                self.emit_expr(&args[0]);
                wasm!(self.func, { i32_store(0); i32_const(4); });
                self.emit_expr(&args[1]);
                wasm!(self.func, {
                    i32_store(0);
                    i32_const(1); local_set(s); // result = true
                    i32_const(0); local_set(s + 1); // i
                    block_empty; loop_empty;
                      local_get(s + 1); i32_const(0); i32_load(0); i32_load(0); i32_ge_u; br_if(1);
                      i32_const(0); local_set(s + 2); // j
                      block_empty; loop_empty;
                        local_get(s + 2); i32_const(4); i32_load(0); i32_load(0); i32_ge_u; br_if(1);
                        i32_const(0); i32_load(0); i32_const(4); i32_add;
                        local_get(s + 1); i32_const(es); i32_mul; i32_add;
                });
                self.emit_load_at(&elem_ty, 0);
                wasm!(self.func, {
                        i32_const(4); i32_load(0); i32_const(4); i32_add;
                        local_get(s + 2); i32_const(es); i32_mul; i32_add;
                });
                self.emit_load_at(&elem_ty, 0);
                self.emit_set_elem_eq(&elem_ty);
                wasm!(self.func, {
                        br_if(1); // found → break inner
                        local_get(s + 2); i32_const(1); i32_add; local_set(s + 2);
                        br(0);
                      end; end;
                      // j >= b.len means NOT found → a[i] not in b → result = false
                      local_get(s + 2); i32_const(4); i32_load(0); i32_load(0); i32_ge_u;
                      if_empty;
                        i32_const(0); local_set(s);
                        br(2); // break outer
                      end;
                      local_get(s + 1); i32_const(1); i32_add; local_set(s + 1);
                      br(0);
                    end; end;
                    local_get(s);
                });
            }
            "is_disjoint" => {
                // is_disjoint(a, b) → Bool: no element of a is in b.
                // Locals: s=result, s+1=i, s+2=j
                let elem_ty = self.set_elem_ty(&args[0].ty);
                let es = values::byte_size(&elem_ty) as i32;
                let s = self.match_i32_base + self.match_depth;
                wasm!(self.func, { i32_const(0); });
                self.emit_expr(&args[0]);
                wasm!(self.func, { i32_store(0); i32_const(4); });
                self.emit_expr(&args[1]);
                wasm!(self.func, {
                    i32_store(0);
                    i32_const(1); local_set(s); // result = true
                    i32_const(0); local_set(s + 1); // i
                    block_empty; loop_empty;
                      local_get(s + 1); i32_const(0); i32_load(0); i32_load(0); i32_ge_u; br_if(1);
                      i32_const(0); local_set(s + 2); // j
                      block_empty; loop_empty;
                        local_get(s + 2); i32_const(4); i32_load(0); i32_load(0); i32_ge_u; br_if(1);
                        i32_const(0); i32_load(0); i32_const(4); i32_add;
                        local_get(s + 1); i32_const(es); i32_mul; i32_add;
                });
                self.emit_load_at(&elem_ty, 0);
                wasm!(self.func, {
                        i32_const(4); i32_load(0); i32_const(4); i32_add;
                        local_get(s + 2); i32_const(es); i32_mul; i32_add;
                });
                self.emit_load_at(&elem_ty, 0);
                self.emit_set_elem_eq(&elem_ty);
                wasm!(self.func, {
                        br_if(1); // found → break inner
                        local_get(s + 2); i32_const(1); i32_add; local_set(s + 2);
                        br(0);
                      end; end;
                      // j < b.len means found → result = false
                      local_get(s + 2); i32_const(4); i32_load(0); i32_load(0); i32_lt_u;
                      if_empty;
                        i32_const(0); local_set(s);
                        br(2); // break outer
                      end;
                      local_get(s + 1); i32_const(1); i32_add; local_set(s + 1);
                      br(0);
                    end; end;
                    local_get(s);
                });
            }
            "filter" | "map" | "fold" | "any" | "all" => {
                // Set has the same memory layout as List — delegate to list implementations
                return self.emit_list_closure_call(method, args);
            }
            _ => return false,
        }
        true
    }

    /// Emit equality comparison for set elements.
    /// Expects two values of elem_ty on the WASM stack, leaves i32 (0 or 1).
    fn emit_set_elem_eq(&mut self, elem_ty: &Ty) {
        match values::ty_to_valtype(elem_ty) {
            Some(ValType::I64) => { wasm!(self.func, { i64_eq; }); }
            _ => {
                if matches!(elem_ty, Ty::String) {
                    wasm!(self.func, { call(self.emitter.rt.string.eq); });
                } else {
                    wasm!(self.func, { i32_eq; });
                }
            }
        }
    }

    fn set_elem_ty(&self, ty: &Ty) -> Ty {
        if let Ty::Applied(_, args) = ty {
            args.first().cloned().unwrap_or(Ty::Int)
        } else { Ty::Int }
    }
}
