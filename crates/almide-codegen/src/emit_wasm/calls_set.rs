//! Set stdlib call dispatch for WASM codegen.
//!
//! Set layout: [len:i32][elem0:K_size][elem1:K_size]...
//! Essentially a List with unique elements.

use super::FuncCompiler;
use super::values;
use almide_ir::IrExpr;
use almide_lang::types::Ty;
use wasm_encoder::ValType;

impl FuncCompiler<'_> {
    pub(super) fn emit_set_call(&mut self, method: &str, args: &[IrExpr]) -> bool {
        match method {
            "new" => {
                let s = self.scratch.alloc_i32();
                wasm!(self.func, {
                    i32_const(4); call(self.emitter.rt.alloc); local_set(s);
                    local_get(s); i32_const(0); i32_store(0);
                    local_get(s);
                });
                self.scratch.free_i32(s);
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
                let s = self.scratch.alloc_i32();
                let s1 = self.scratch.alloc_i32();
                let s2 = self.scratch.alloc_i32();
                let s3 = self.scratch.alloc_i32();
                let xs = self.scratch.alloc_i32();
                // Start with empty set
                wasm!(self.func, {
                    i32_const(4); call(self.emitter.rt.alloc); local_set(s);
                    local_get(s); i32_const(0); i32_store(0);
                });
                self.emit_expr(&args[0]);
                wasm!(self.func, {
                    local_set(xs); // xs
                    i32_const(0); local_set(s1); // i
                    block_empty; loop_empty;
                      local_get(s1); local_get(xs); i32_load(0); i32_ge_u; br_if(1);
                      // Check if elem already in set
                      i32_const(0); local_set(s2); // j
                      i32_const(0); local_set(s3); // found
                      block_empty; loop_empty;
                        local_get(s2); local_get(s); i32_load(0); i32_ge_u; br_if(1);
                        local_get(s); i32_const(4); i32_add;
                        local_get(s2); i32_const(es); i32_mul; i32_add;
                });
                self.emit_load_at(&elem_ty, 0);
                wasm!(self.func, {
                        local_get(xs); i32_const(4); i32_add;
                        local_get(s1); i32_const(es); i32_mul; i32_add;
                });
                self.emit_load_at(&elem_ty, 0);
                self.emit_set_elem_eq(&elem_ty);
                wasm!(self.func, {
                        if_empty; i32_const(1); local_set(s3); br(2); end;
                        local_get(s2); i32_const(1); i32_add; local_set(s2);
                        br(0);
                      end; end;
                      // If not found, append
                      local_get(s3); i32_eqz;
                      if_empty;
                        // Grow set: alloc new with len+1
                        i32_const(4); local_get(s); i32_load(0); i32_const(1); i32_add;
                        i32_const(es); i32_mul; i32_add;
                        call(self.emitter.rt.alloc); local_set(s2);
                        local_get(s2); local_get(s); i32_load(0); i32_const(1); i32_add; i32_store(0);
                        // Copy old elements
                        i32_const(0); local_set(s3);
                        block_empty; loop_empty;
                          local_get(s3); local_get(s); i32_load(0); i32_ge_u; br_if(1);
                          local_get(s2); i32_const(4); i32_add;
                          local_get(s3); i32_const(es); i32_mul; i32_add;
                          local_get(s); i32_const(4); i32_add;
                          local_get(s3); i32_const(es); i32_mul; i32_add;
                });
                self.emit_elem_copy(&elem_ty);
                wasm!(self.func, {
                          local_get(s3); i32_const(1); i32_add; local_set(s3);
                          br(0);
                        end; end;
                        // Append new element
                        local_get(s2); i32_const(4); i32_add;
                        local_get(s); i32_load(0); i32_const(es); i32_mul; i32_add;
                        local_get(xs); i32_const(4); i32_add;
                        local_get(s1); i32_const(es); i32_mul; i32_add;
                });
                self.emit_elem_copy(&elem_ty);
                wasm!(self.func, {
                        local_get(s2); local_set(s);
                      end;
                      local_get(s1); i32_const(1); i32_add; local_set(s1);
                      br(0);
                    end; end;
                    local_get(s);
                });
                self.scratch.free_i32(xs);
                self.scratch.free_i32(s3);
                self.scratch.free_i32(s2);
                self.scratch.free_i32(s1);
                self.scratch.free_i32(s);
            }
            "contains" => {
                let elem_ty = self.set_elem_ty(&args[0].ty);
                let es = values::byte_size(&elem_ty) as i32;
                let s = self.scratch.alloc_i32();
                let s1 = self.scratch.alloc_i32();
                let s2 = self.scratch.alloc_i32();
                self.emit_expr(&args[0]);
                wasm!(self.func, { local_set(s); });
                match values::ty_to_valtype(&elem_ty) {
                    Some(ValType::I64) => {
                        let s64 = self.scratch.alloc_i64();
                        self.emit_expr(&args[1]);
                        wasm!(self.func, {
                            local_set(s64);
                            i32_const(0); local_set(s1);
                            i32_const(0); local_set(s2);
                            block_empty; loop_empty;
                              local_get(s1); local_get(s); i32_load(0); i32_ge_u; br_if(1);
                              local_get(s); i32_const(4); i32_add;
                              local_get(s1); i32_const(es); i32_mul; i32_add;
                              i64_load(0); local_get(s64); i64_eq;
                              if_empty; i32_const(1); local_set(s2); br(2); end;
                              local_get(s1); i32_const(1); i32_add; local_set(s1);
                              br(0);
                            end; end;
                            local_get(s2);
                        });
                        self.scratch.free_i64(s64);
                    }
                    _ => {
                        let s3 = self.scratch.alloc_i32();
                        self.emit_expr(&args[1]);
                        wasm!(self.func, {
                            local_set(s3);
                            i32_const(0); local_set(s1);
                            i32_const(0); local_set(s2);
                            block_empty; loop_empty;
                              local_get(s1); local_get(s); i32_load(0); i32_ge_u; br_if(1);
                              local_get(s); i32_const(4); i32_add;
                              local_get(s1); i32_const(es); i32_mul; i32_add;
                              i32_load(0); local_get(s3);
                        });
                        if matches!(&elem_ty, Ty::String) {
                            wasm!(self.func, { call(self.emitter.rt.string.eq); });
                        } else {
                            wasm!(self.func, { i32_eq; });
                        }
                        wasm!(self.func, {
                              if_empty; i32_const(1); local_set(s2); br(2); end;
                              local_get(s1); i32_const(1); i32_add; local_set(s1);
                              br(0);
                            end; end;
                            local_get(s2);
                        });
                        self.scratch.free_i32(s3);
                    }
                }
                self.scratch.free_i32(s2);
                self.scratch.free_i32(s1);
                self.scratch.free_i32(s);
            }
            "insert" => {
                let elem_ty = self.set_elem_ty(&args[0].ty);
                let es = values::byte_size(&elem_ty) as i32;
                let vt = values::ty_to_valtype(&elem_ty).unwrap_or(ValType::I32);
                let s = self.scratch.alloc_i32();
                let s1 = self.scratch.alloc_i32();
                let s2 = self.scratch.alloc_i32();
                let val = self.scratch.alloc(vt);
                self.emit_expr(&args[0]);
                wasm!(self.func, { local_set(s); });
                self.emit_expr(&args[1]);
                wasm!(self.func, { local_set(val); });
                wasm!(self.func, {
                    i32_const(0); local_set(s1); // i
                    i32_const(0); local_set(s2); // found
                    block_empty; loop_empty;
                      local_get(s1); local_get(s); i32_load(0); i32_ge_u; br_if(1);
                      local_get(s); i32_const(4); i32_add;
                      local_get(s1); i32_const(es); i32_mul; i32_add;
                });
                self.emit_load_at(&elem_ty, 0);
                wasm!(self.func, { local_get(val); });
                self.emit_set_elem_eq(&elem_ty);
                wasm!(self.func, {
                      if_empty; i32_const(1); local_set(s2); br(2); end;
                      local_get(s1); i32_const(1); i32_add; local_set(s1);
                      br(0);
                    end; end;
                    local_get(s2);
                    if_i32; local_get(s);
                    else_;
                      i32_const(4); local_get(s); i32_load(0); i32_const(1); i32_add;
                      i32_const(es); i32_mul; i32_add;
                      call(self.emitter.rt.alloc); local_set(s1);
                      local_get(s1); local_get(s); i32_load(0); i32_const(1); i32_add; i32_store(0);
                      i32_const(0); local_set(s2);
                      block_empty; loop_empty;
                        local_get(s2); local_get(s); i32_load(0); i32_ge_u; br_if(1);
                        local_get(s1); i32_const(4); i32_add;
                        local_get(s2); i32_const(es); i32_mul; i32_add;
                        local_get(s); i32_const(4); i32_add;
                        local_get(s2); i32_const(es); i32_mul; i32_add;
                });
                self.emit_elem_copy(&elem_ty);
                wasm!(self.func, {
                        local_get(s2); i32_const(1); i32_add; local_set(s2);
                        br(0);
                      end; end;
                      local_get(s1); i32_const(4); i32_add;
                      local_get(s); i32_load(0); i32_const(es); i32_mul; i32_add;
                      local_get(val);
                });
                self.emit_store_at(&elem_ty, 0);
                wasm!(self.func, { local_get(s1); end; });
                self.scratch.free(val, vt);
                self.scratch.free_i32(s2);
                self.scratch.free_i32(s1);
                self.scratch.free_i32(s);
            }
            "remove" => {
                let elem_ty = self.set_elem_ty(&args[0].ty);
                let es = values::byte_size(&elem_ty) as i32;
                let vt = values::ty_to_valtype(&elem_ty).unwrap_or(ValType::I32);
                let s = self.scratch.alloc_i32();
                let s1 = self.scratch.alloc_i32();
                let s2 = self.scratch.alloc_i32();
                let s3 = self.scratch.alloc_i32();
                let val = self.scratch.alloc(vt);
                let orig = self.scratch.alloc_i32();
                self.emit_expr(&args[0]);
                wasm!(self.func, { local_set(s); });
                self.emit_expr(&args[1]);
                wasm!(self.func, { local_set(val); });
                wasm!(self.func, {
                    i32_const(-1); local_set(s1); // found_idx
                    i32_const(0); local_set(s2);
                    block_empty; loop_empty;
                      local_get(s2); local_get(s); i32_load(0); i32_ge_u; br_if(1);
                      local_get(s); i32_const(4); i32_add;
                      local_get(s2); i32_const(es); i32_mul; i32_add;
                });
                self.emit_load_at(&elem_ty, 0);
                wasm!(self.func, { local_get(val); });
                self.emit_set_elem_eq(&elem_ty);
                wasm!(self.func, {
                      if_empty; local_get(s2); local_set(s1); br(2); end;
                      local_get(s2); i32_const(1); i32_add; local_set(s2);
                      br(0);
                    end; end;
                    local_get(s1); i32_const(0); i32_lt_s;
                    if_i32; local_get(s); // not found
                    else_;
                      // Store original set ptr
                      local_get(s); local_set(orig);
                      i32_const(4); local_get(orig); i32_load(0); i32_const(1); i32_sub;
                      i32_const(es); i32_mul; i32_add;
                      call(self.emitter.rt.alloc); local_set(s2);
                      local_get(s2);
                      local_get(orig); i32_load(0); i32_const(1); i32_sub;
                      i32_store(0);
                      i32_const(0); local_set(s3); // src_i
                      i32_const(0); local_set(s);     // dst_i
                      block_empty; loop_empty;
                        local_get(s3);
                        local_get(orig); i32_load(0); // original len
                        i32_ge_u; br_if(1);
                        local_get(s3); local_get(s1); i32_ne;
                        if_empty;
                          local_get(s2); i32_const(4); i32_add;
                          local_get(s); i32_const(es); i32_mul; i32_add;
                          local_get(orig); i32_const(4); i32_add;
                          local_get(s3); i32_const(es); i32_mul; i32_add;
                });
                self.emit_elem_copy(&elem_ty);
                wasm!(self.func, {
                          local_get(s); i32_const(1); i32_add; local_set(s);
                        end;
                        local_get(s3); i32_const(1); i32_add; local_set(s3);
                        br(0);
                      end; end;
                      local_get(s2);
                    end;
                });
                self.scratch.free_i32(orig);
                self.scratch.free(val, vt);
                self.scratch.free_i32(s3);
                self.scratch.free_i32(s2);
                self.scratch.free_i32(s1);
                self.scratch.free_i32(s);
            }
            "union" => {
                // union(a, b) → Set[A]: all elements from a, plus elements from b not in a
                let elem_ty = self.set_elem_ty(&args[0].ty);
                let es = values::byte_size(&elem_ty) as i32;
                let a = self.scratch.alloc_i32();
                let b = self.scratch.alloc_i32();
                let result = self.scratch.alloc_i32();
                let out_count = self.scratch.alloc_i32();
                let i = self.scratch.alloc_i32();
                let j = self.scratch.alloc_i32();
                self.emit_expr(&args[0]);
                wasm!(self.func, { local_set(a); });
                self.emit_expr(&args[1]);
                wasm!(self.func, {
                    local_set(b);
                    // Alloc max size: a.len + b.len
                    i32_const(4); local_get(a); i32_load(0); local_get(b); i32_load(0); i32_add;
                    i32_const(es); i32_mul; i32_add;
                    call(self.emitter.rt.alloc); local_set(result);
                    // Copy all of a first
                    i32_const(0); local_set(i);
                    block_empty; loop_empty;
                      local_get(i); local_get(a); i32_load(0); i32_ge_u; br_if(1);
                      local_get(result); i32_const(4); i32_add;
                      local_get(i); i32_const(es); i32_mul; i32_add;
                      local_get(a); i32_const(4); i32_add;
                      local_get(i); i32_const(es); i32_mul; i32_add;
                });
                self.emit_elem_copy(&elem_ty);
                wasm!(self.func, {
                      local_get(i); i32_const(1); i32_add; local_set(i);
                      br(0);
                    end; end;
                    local_get(a); i32_load(0); local_set(out_count); // out_count = a.len
                    // Add elements from b that are not in a
                    i32_const(0); local_set(i);
                    block_empty; loop_empty;
                      local_get(i); local_get(b); i32_load(0); i32_ge_u; br_if(1);
                      // Check if b[i] is in a
                      i32_const(0); local_set(j);
                      block_empty; loop_empty;
                        local_get(j); local_get(a); i32_load(0); i32_ge_u; br_if(1);
                        local_get(b); i32_const(4); i32_add;
                        local_get(i); i32_const(es); i32_mul; i32_add;
                });
                self.emit_load_at(&elem_ty, 0);
                wasm!(self.func, {
                        local_get(a); i32_const(4); i32_add;
                        local_get(j); i32_const(es); i32_mul; i32_add;
                });
                self.emit_load_at(&elem_ty, 0);
                self.emit_set_elem_eq(&elem_ty);
                wasm!(self.func, {
                        br_if(1); // found → break inner loop
                        local_get(j); i32_const(1); i32_add; local_set(j);
                        br(0);
                      end; end;
                      // j >= a.len means NOT found → add b[i]
                      local_get(j); local_get(a); i32_load(0); i32_ge_u;
                      if_empty;
                        local_get(result); i32_const(4); i32_add;
                        local_get(out_count); i32_const(es); i32_mul; i32_add;
                        local_get(b); i32_const(4); i32_add;
                        local_get(i); i32_const(es); i32_mul; i32_add;
                });
                self.emit_elem_copy(&elem_ty);
                wasm!(self.func, {
                        local_get(out_count); i32_const(1); i32_add; local_set(out_count);
                      end;
                      local_get(i); i32_const(1); i32_add; local_set(i);
                      br(0);
                    end; end;
                    local_get(result); local_get(out_count); i32_store(0);
                    local_get(result);
                });
                self.scratch.free_i32(j);
                self.scratch.free_i32(i);
                self.scratch.free_i32(out_count);
                self.scratch.free_i32(result);
                self.scratch.free_i32(b);
                self.scratch.free_i32(a);
            }
            "to_list" => {
                // Set has same layout as List — just return the ptr
                self.emit_expr(&args[0]);
            }
            "intersection" => {
                // intersection(a, b) → Set[A]: elements in both a and b.
                // For each a[i], scan b for a match. Use j as loop counter;
                // after inner loop, j < b.len means we found a match (broke early).
                // Locals: s=result, s1=out_count, s2=i, s3=j
                let elem_ty = self.set_elem_ty(&args[0].ty);
                let es = values::byte_size(&elem_ty) as i32;
                let s = self.scratch.alloc_i32();
                let s1 = self.scratch.alloc_i32();
                let s2 = self.scratch.alloc_i32();
                let s3 = self.scratch.alloc_i32();
                let a = self.scratch.alloc_i32();
                let b = self.scratch.alloc_i32();
                // a, b
                self.emit_expr(&args[0]);
                wasm!(self.func, { local_set(a); });
                self.emit_expr(&args[1]);
                wasm!(self.func, {
                    local_set(b);
                    // result = alloc(4 + a.len * es)
                    i32_const(4); local_get(a); i32_load(0);
                    i32_const(es); i32_mul; i32_add;
                    call(self.emitter.rt.alloc); local_set(s);
                    i32_const(0); local_set(s1); // out_count
                    i32_const(0); local_set(s2); // i
                    block_empty; loop_empty;
                      local_get(s2); local_get(a); i32_load(0); i32_ge_u; br_if(1);
                      // Inner: scan b for a[i]
                      i32_const(0); local_set(s3); // j
                      block_empty; loop_empty;
                        local_get(s3); local_get(b); i32_load(0); i32_ge_u; br_if(1);
                        // Load a[i]
                        local_get(a); i32_const(4); i32_add;
                        local_get(s2); i32_const(es); i32_mul; i32_add;
                });
                self.emit_load_at(&elem_ty, 0);
                wasm!(self.func, {
                        // Load b[j]
                        local_get(b); i32_const(4); i32_add;
                        local_get(s3); i32_const(es); i32_mul; i32_add;
                });
                self.emit_load_at(&elem_ty, 0);
                self.emit_set_elem_eq(&elem_ty);
                wasm!(self.func, {
                        br_if(1); // found → break out of inner loop
                        local_get(s3); i32_const(1); i32_add; local_set(s3);
                        br(0);
                      end; end;
                      // After inner loop: j < b.len means found
                      local_get(s3); local_get(b); i32_load(0); i32_lt_u;
                      if_empty;
                        // Copy a[i] to result[out_count]
                        local_get(s); i32_const(4); i32_add;
                        local_get(s1); i32_const(es); i32_mul; i32_add;
                        local_get(a); i32_const(4); i32_add;
                        local_get(s2); i32_const(es); i32_mul; i32_add;
                });
                self.emit_elem_copy(&elem_ty);
                wasm!(self.func, {
                        local_get(s1); i32_const(1); i32_add; local_set(s1);
                      end;
                      local_get(s2); i32_const(1); i32_add; local_set(s2);
                      br(0);
                    end; end;
                    local_get(s); local_get(s1); i32_store(0);
                    local_get(s);
                });
                self.scratch.free_i32(b);
                self.scratch.free_i32(a);
                self.scratch.free_i32(s3);
                self.scratch.free_i32(s2);
                self.scratch.free_i32(s1);
                self.scratch.free_i32(s);
            }
            "difference" => {
                // difference(a, b) → Set[A]: elements in a but not in b.
                // Locals: s=result, s1=out_count, s2=i, s3=j
                let elem_ty = self.set_elem_ty(&args[0].ty);
                let es = values::byte_size(&elem_ty) as i32;
                let s = self.scratch.alloc_i32();
                let s1 = self.scratch.alloc_i32();
                let s2 = self.scratch.alloc_i32();
                let s3 = self.scratch.alloc_i32();
                let a = self.scratch.alloc_i32();
                let b = self.scratch.alloc_i32();
                self.emit_expr(&args[0]);
                wasm!(self.func, { local_set(a); });
                self.emit_expr(&args[1]);
                wasm!(self.func, {
                    local_set(b);
                    i32_const(4); local_get(a); i32_load(0);
                    i32_const(es); i32_mul; i32_add;
                    call(self.emitter.rt.alloc); local_set(s);
                    i32_const(0); local_set(s1);
                    i32_const(0); local_set(s2);
                    block_empty; loop_empty;
                      local_get(s2); local_get(a); i32_load(0); i32_ge_u; br_if(1);
                      i32_const(0); local_set(s3); // j
                      block_empty; loop_empty;
                        local_get(s3); local_get(b); i32_load(0); i32_ge_u; br_if(1);
                        local_get(a); i32_const(4); i32_add;
                        local_get(s2); i32_const(es); i32_mul; i32_add;
                });
                self.emit_load_at(&elem_ty, 0);
                wasm!(self.func, {
                        local_get(b); i32_const(4); i32_add;
                        local_get(s3); i32_const(es); i32_mul; i32_add;
                });
                self.emit_load_at(&elem_ty, 0);
                self.emit_set_elem_eq(&elem_ty);
                wasm!(self.func, {
                        br_if(1); // found → break
                        local_get(s3); i32_const(1); i32_add; local_set(s3);
                        br(0);
                      end; end;
                      // j >= b.len means NOT found → keep element
                      local_get(s3); local_get(b); i32_load(0); i32_ge_u;
                      if_empty;
                        local_get(s); i32_const(4); i32_add;
                        local_get(s1); i32_const(es); i32_mul; i32_add;
                        local_get(a); i32_const(4); i32_add;
                        local_get(s2); i32_const(es); i32_mul; i32_add;
                });
                self.emit_elem_copy(&elem_ty);
                wasm!(self.func, {
                        local_get(s1); i32_const(1); i32_add; local_set(s1);
                      end;
                      local_get(s2); i32_const(1); i32_add; local_set(s2);
                      br(0);
                    end; end;
                    local_get(s); local_get(s1); i32_store(0);
                    local_get(s);
                });
                self.scratch.free_i32(b);
                self.scratch.free_i32(a);
                self.scratch.free_i32(s3);
                self.scratch.free_i32(s2);
                self.scratch.free_i32(s1);
                self.scratch.free_i32(s);
            }
            "symmetric_difference" => {
                // symmetric_difference(a, b) = difference(a,b) ∪ difference(b,a)
                // Two passes, collecting into one result buffer.
                // Locals: s=result, s1=out_count, s2=i, s3=j
                let elem_ty = self.set_elem_ty(&args[0].ty);
                let es = values::byte_size(&elem_ty) as i32;
                let s = self.scratch.alloc_i32();
                let s1 = self.scratch.alloc_i32();
                let s2 = self.scratch.alloc_i32();
                let s3 = self.scratch.alloc_i32();
                let a = self.scratch.alloc_i32();
                let b = self.scratch.alloc_i32();
                // a, b
                self.emit_expr(&args[0]);
                wasm!(self.func, { local_set(a); });
                self.emit_expr(&args[1]);
                wasm!(self.func, {
                    local_set(b);
                    // result = alloc(4 + (a.len + b.len) * es)
                    i32_const(4);
                    local_get(a); i32_load(0);
                    local_get(b); i32_load(0);
                    i32_add; i32_const(es); i32_mul; i32_add;
                    call(self.emitter.rt.alloc); local_set(s);
                    i32_const(0); local_set(s1); // out_count
                });
                // Pass 1: elements of a not in b
                wasm!(self.func, {
                    i32_const(0); local_set(s2);
                    block_empty; loop_empty;
                      local_get(s2); local_get(a); i32_load(0); i32_ge_u; br_if(1);
                      i32_const(0); local_set(s3);
                      block_empty; loop_empty;
                        local_get(s3); local_get(b); i32_load(0); i32_ge_u; br_if(1);
                        local_get(a); i32_const(4); i32_add;
                        local_get(s2); i32_const(es); i32_mul; i32_add;
                });
                self.emit_load_at(&elem_ty, 0);
                wasm!(self.func, {
                        local_get(b); i32_const(4); i32_add;
                        local_get(s3); i32_const(es); i32_mul; i32_add;
                });
                self.emit_load_at(&elem_ty, 0);
                self.emit_set_elem_eq(&elem_ty);
                wasm!(self.func, {
                        br_if(1);
                        local_get(s3); i32_const(1); i32_add; local_set(s3);
                        br(0);
                      end; end;
                      local_get(s3); local_get(b); i32_load(0); i32_ge_u;
                      if_empty;
                        local_get(s); i32_const(4); i32_add;
                        local_get(s1); i32_const(es); i32_mul; i32_add;
                        local_get(a); i32_const(4); i32_add;
                        local_get(s2); i32_const(es); i32_mul; i32_add;
                });
                self.emit_elem_copy(&elem_ty);
                wasm!(self.func, {
                        local_get(s1); i32_const(1); i32_add; local_set(s1);
                      end;
                      local_get(s2); i32_const(1); i32_add; local_set(s2);
                      br(0);
                    end; end;
                });
                // Pass 2: elements of b not in a
                wasm!(self.func, {
                    i32_const(0); local_set(s2);
                    block_empty; loop_empty;
                      local_get(s2); local_get(b); i32_load(0); i32_ge_u; br_if(1);
                      i32_const(0); local_set(s3);
                      block_empty; loop_empty;
                        local_get(s3); local_get(a); i32_load(0); i32_ge_u; br_if(1);
                        local_get(b); i32_const(4); i32_add;
                        local_get(s2); i32_const(es); i32_mul; i32_add;
                });
                self.emit_load_at(&elem_ty, 0);
                wasm!(self.func, {
                        local_get(a); i32_const(4); i32_add;
                        local_get(s3); i32_const(es); i32_mul; i32_add;
                });
                self.emit_load_at(&elem_ty, 0);
                self.emit_set_elem_eq(&elem_ty);
                wasm!(self.func, {
                        br_if(1);
                        local_get(s3); i32_const(1); i32_add; local_set(s3);
                        br(0);
                      end; end;
                      local_get(s3); local_get(a); i32_load(0); i32_ge_u;
                      if_empty;
                        local_get(s); i32_const(4); i32_add;
                        local_get(s1); i32_const(es); i32_mul; i32_add;
                        local_get(b); i32_const(4); i32_add;
                        local_get(s2); i32_const(es); i32_mul; i32_add;
                });
                self.emit_elem_copy(&elem_ty);
                wasm!(self.func, {
                        local_get(s1); i32_const(1); i32_add; local_set(s1);
                      end;
                      local_get(s2); i32_const(1); i32_add; local_set(s2);
                      br(0);
                    end; end;
                    local_get(s); local_get(s1); i32_store(0);
                    local_get(s);
                });
                self.scratch.free_i32(b);
                self.scratch.free_i32(a);
                self.scratch.free_i32(s3);
                self.scratch.free_i32(s2);
                self.scratch.free_i32(s1);
                self.scratch.free_i32(s);
            }
            "is_subset" => {
                // is_subset(a, b) → Bool: all elements of a are in b.
                // Locals: s=result, s1=i, s2=j. Use j sentinel: j < b.len = found.
                let elem_ty = self.set_elem_ty(&args[0].ty);
                let es = values::byte_size(&elem_ty) as i32;
                let s = self.scratch.alloc_i32();
                let s1 = self.scratch.alloc_i32();
                let s2 = self.scratch.alloc_i32();
                let a = self.scratch.alloc_i32();
                let b = self.scratch.alloc_i32();
                self.emit_expr(&args[0]);
                wasm!(self.func, { local_set(a); });
                self.emit_expr(&args[1]);
                wasm!(self.func, {
                    local_set(b);
                    i32_const(1); local_set(s); // result = true
                    i32_const(0); local_set(s1); // i
                    block_empty; loop_empty;
                      local_get(s1); local_get(a); i32_load(0); i32_ge_u; br_if(1);
                      i32_const(0); local_set(s2); // j
                      block_empty; loop_empty;
                        local_get(s2); local_get(b); i32_load(0); i32_ge_u; br_if(1);
                        local_get(a); i32_const(4); i32_add;
                        local_get(s1); i32_const(es); i32_mul; i32_add;
                });
                self.emit_load_at(&elem_ty, 0);
                wasm!(self.func, {
                        local_get(b); i32_const(4); i32_add;
                        local_get(s2); i32_const(es); i32_mul; i32_add;
                });
                self.emit_load_at(&elem_ty, 0);
                self.emit_set_elem_eq(&elem_ty);
                wasm!(self.func, {
                        br_if(1); // found → break inner
                        local_get(s2); i32_const(1); i32_add; local_set(s2);
                        br(0);
                      end; end;
                      // j >= b.len means NOT found → a[i] not in b → result = false
                      local_get(s2); local_get(b); i32_load(0); i32_ge_u;
                      if_empty;
                        i32_const(0); local_set(s);
                        br(2); // break outer
                      end;
                      local_get(s1); i32_const(1); i32_add; local_set(s1);
                      br(0);
                    end; end;
                    local_get(s);
                });
                self.scratch.free_i32(b);
                self.scratch.free_i32(a);
                self.scratch.free_i32(s2);
                self.scratch.free_i32(s1);
                self.scratch.free_i32(s);
            }
            "is_disjoint" => {
                // is_disjoint(a, b) → Bool: no element of a is in b.
                // Locals: s=result, s1=i, s2=j
                let elem_ty = self.set_elem_ty(&args[0].ty);
                let es = values::byte_size(&elem_ty) as i32;
                let s = self.scratch.alloc_i32();
                let s1 = self.scratch.alloc_i32();
                let s2 = self.scratch.alloc_i32();
                let a = self.scratch.alloc_i32();
                let b = self.scratch.alloc_i32();
                self.emit_expr(&args[0]);
                wasm!(self.func, { local_set(a); });
                self.emit_expr(&args[1]);
                wasm!(self.func, {
                    local_set(b);
                    i32_const(1); local_set(s); // result = true
                    i32_const(0); local_set(s1); // i
                    block_empty; loop_empty;
                      local_get(s1); local_get(a); i32_load(0); i32_ge_u; br_if(1);
                      i32_const(0); local_set(s2); // j
                      block_empty; loop_empty;
                        local_get(s2); local_get(b); i32_load(0); i32_ge_u; br_if(1);
                        local_get(a); i32_const(4); i32_add;
                        local_get(s1); i32_const(es); i32_mul; i32_add;
                });
                self.emit_load_at(&elem_ty, 0);
                wasm!(self.func, {
                        local_get(b); i32_const(4); i32_add;
                        local_get(s2); i32_const(es); i32_mul; i32_add;
                });
                self.emit_load_at(&elem_ty, 0);
                self.emit_set_elem_eq(&elem_ty);
                wasm!(self.func, {
                        br_if(1); // found → break inner
                        local_get(s2); i32_const(1); i32_add; local_set(s2);
                        br(0);
                      end; end;
                      // j < b.len means found → result = false
                      local_get(s2); local_get(b); i32_load(0); i32_lt_u;
                      if_empty;
                        i32_const(0); local_set(s);
                        br(2); // break outer
                      end;
                      local_get(s1); i32_const(1); i32_add; local_set(s1);
                      br(0);
                    end; end;
                    local_get(s);
                });
                self.scratch.free_i32(b);
                self.scratch.free_i32(a);
                self.scratch.free_i32(s2);
                self.scratch.free_i32(s1);
                self.scratch.free_i32(s);
            }
            "map" => {
                // set.map = list.map + dedup
                // 1. Emit list.map (result on stack)
                self.emit_list_closure_call("map", args);
                // 2. Dedup the result using set.from_list logic (insert-if-not-exists)
                // The map result type is the closure return type = element type of the new set
                // For simplicity: use from_list which already deduplicates
                let result_elem_ty = self.set_elem_ty(&args[0].ty); // same elem type for now
                let es = values::byte_size(&result_elem_ty) as i32;
                let mapped = self.scratch.alloc_i32();
                let result = self.scratch.alloc_i32();
                let i = self.scratch.alloc_i32();
                let j = self.scratch.alloc_i32();
                let found = self.scratch.alloc_i32();
                wasm!(self.func, {
                    local_set(mapped);
                    // Start with empty set
                    i32_const(4); call(self.emitter.rt.alloc); local_set(result);
                    local_get(result); i32_const(0); i32_store(0);
                    // For each element in mapped, insert if not present
                    i32_const(0); local_set(i);
                    block_empty; loop_empty;
                      local_get(i); local_get(mapped); i32_load(0); i32_ge_u; br_if(1);
                      // Check if mapped[i] already in result
                      i32_const(0); local_set(j);
                      i32_const(0); local_set(found);
                      block_empty; loop_empty;
                        local_get(j); local_get(result); i32_load(0); i32_ge_u; br_if(1);
                        local_get(result); i32_const(4); i32_add;
                        local_get(j); i32_const(es); i32_mul; i32_add;
                });
                self.emit_load_at(&result_elem_ty, 0);
                wasm!(self.func, {
                        local_get(mapped); i32_const(4); i32_add;
                        local_get(i); i32_const(es); i32_mul; i32_add;
                });
                self.emit_load_at(&result_elem_ty, 0);
                self.emit_set_elem_eq(&result_elem_ty);
                wasm!(self.func, {
                        if_empty; i32_const(1); local_set(found); br(2); end;
                        local_get(j); i32_const(1); i32_add; local_set(j);
                        br(0);
                      end; end;
                      local_get(found); i32_eqz;
                      if_empty;
                        // Not found: append to result
                        i32_const(4); local_get(result); i32_load(0); i32_const(1); i32_add;
                        i32_const(es); i32_mul; i32_add;
                        call(self.emitter.rt.alloc); local_set(j); // new result
                        local_get(j); local_get(result); i32_load(0); i32_const(1); i32_add; i32_store(0);
                        // Copy old elements
                        i32_const(0); local_set(found);
                        block_empty; loop_empty;
                          local_get(found); local_get(result); i32_load(0); i32_ge_u; br_if(1);
                          local_get(j); i32_const(4); i32_add;
                          local_get(found); i32_const(es); i32_mul; i32_add;
                          local_get(result); i32_const(4); i32_add;
                          local_get(found); i32_const(es); i32_mul; i32_add;
                });
                self.emit_elem_copy(&result_elem_ty);
                wasm!(self.func, {
                          local_get(found); i32_const(1); i32_add; local_set(found);
                          br(0);
                        end; end;
                        // Copy new element
                        local_get(j); i32_const(4); i32_add;
                        local_get(result); i32_load(0); i32_const(es); i32_mul; i32_add;
                        local_get(mapped); i32_const(4); i32_add;
                        local_get(i); i32_const(es); i32_mul; i32_add;
                });
                self.emit_elem_copy(&result_elem_ty);
                wasm!(self.func, {
                        local_get(j); local_set(result);
                      end;
                      local_get(i); i32_const(1); i32_add; local_set(i);
                      br(0);
                    end; end;
                    local_get(result);
                });
                self.scratch.free_i32(found);
                self.scratch.free_i32(j);
                self.scratch.free_i32(i);
                self.scratch.free_i32(result);
                self.scratch.free_i32(mapped);
                return true;
            }
            "filter" | "fold" | "any" | "all" => {
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
        match ty {
            Ty::Applied(_, args) | Ty::Named(_, args) => {
                args.first().cloned().unwrap_or(Ty::Int)
            }
            _ => Ty::Int,
        }
    }
}
