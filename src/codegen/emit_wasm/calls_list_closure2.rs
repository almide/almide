//! List stdlib closure-based call dispatch for WASM codegen (part 2).
//!
//! Functions: take_while, drop_while, count, partition, update, scan, zip_with,
//! unique_by, group_by, shuffle, filter, fold, map, and the emit_list_map helper.

use super::FuncCompiler;
use super::values;
use crate::ir::{IrExpr, IrExprKind};
use crate::types::Ty;
use wasm_encoder::ValType;

impl FuncCompiler<'_> {
    /// Dispatch list closure calls (second half). Returns true if handled.
    pub(super) fn emit_list_closure_call2(&mut self, method: &str, args: &[IrExpr]) -> bool {
        match method {
            "take_while" => {
                // take_while(xs, pred) → List[A]: take while pred returns true
                let elem_ty = self.list_elem_ty(&args[0].ty);
                let es = values::byte_size(&elem_ty) as i32;
                let xs = self.scratch.alloc_i32();
                let closure = self.scratch.alloc_i32();
                let len = self.scratch.alloc_i32();
                let count = self.scratch.alloc_i32();
                let dst = self.scratch.alloc_i32();
                let copy_i = self.scratch.alloc_i32();
                self.emit_expr(&args[0]);
                wasm!(self.func, { local_set(xs); });
                self.emit_expr(&args[1]);
                wasm!(self.func, {
                    local_set(closure);
                    local_get(xs); i32_load(0); local_set(len);
                    i32_const(0); local_set(count);
                    block_empty; loop_empty;
                      local_get(count); local_get(len); i32_ge_u; br_if(1);
                      local_get(closure); i32_load(4); // env
                      local_get(xs); i32_const(4); i32_add;
                      local_get(count); i32_const(es); i32_mul; i32_add;
                });
                self.emit_load_at(&elem_ty, 0);
                wasm!(self.func, {
                      local_get(closure); i32_load(0); // table_idx
                });
                self.emit_closure_call(&elem_ty, &Ty::Bool);
                wasm!(self.func, {
                      i32_eqz; br_if(1);
                      local_get(count); i32_const(1); i32_add; local_set(count);
                      br(0);
                    end; end;
                    // Alloc result
                    i32_const(4); local_get(count); i32_const(es); i32_mul; i32_add;
                    call(self.emitter.rt.alloc); local_set(dst);
                    local_get(dst); local_get(count); i32_store(0);
                    // Copy loop
                    i32_const(0); local_set(copy_i);
                    block_empty; loop_empty;
                      local_get(copy_i); local_get(count); i32_ge_u; br_if(1);
                      local_get(dst); i32_const(4); i32_add;
                      local_get(copy_i); i32_const(es); i32_mul; i32_add;
                      local_get(xs); i32_const(4); i32_add;
                      local_get(copy_i); i32_const(es); i32_mul; i32_add;
                });
                self.emit_elem_copy(&elem_ty);
                wasm!(self.func, {
                      local_get(copy_i); i32_const(1); i32_add; local_set(copy_i);
                      br(0);
                    end; end;
                    local_get(dst);
                });
                self.scratch.free_i32(copy_i);
                self.scratch.free_i32(dst);
                self.scratch.free_i32(count);
                self.scratch.free_i32(len);
                self.scratch.free_i32(closure);
                self.scratch.free_i32(xs);
            }
            "drop_while" => {
                // drop_while(xs, pred) → List[A]: drop while pred returns true
                let elem_ty = self.list_elem_ty(&args[0].ty);
                let es = values::byte_size(&elem_ty) as i32;
                let xs = self.scratch.alloc_i32();
                let closure = self.scratch.alloc_i32();
                let len = self.scratch.alloc_i32();
                let start = self.scratch.alloc_i32();
                let new_len = self.scratch.alloc_i32();
                let dst = self.scratch.alloc_i32();
                let copy_i = self.scratch.alloc_i32();
                self.emit_expr(&args[0]);
                wasm!(self.func, { local_set(xs); });
                self.emit_expr(&args[1]);
                wasm!(self.func, {
                    local_set(closure);
                    local_get(xs); i32_load(0); local_set(len);
                    i32_const(0); local_set(start);
                    block_empty; loop_empty;
                      local_get(start); local_get(len); i32_ge_u; br_if(1);
                      local_get(closure); i32_load(4); // env
                      local_get(xs); i32_const(4); i32_add;
                      local_get(start); i32_const(es); i32_mul; i32_add;
                });
                self.emit_load_at(&elem_ty, 0);
                wasm!(self.func, {
                      local_get(closure); i32_load(0); // table_idx
                });
                self.emit_closure_call(&elem_ty, &Ty::Bool);
                wasm!(self.func, {
                      i32_eqz; br_if(1);
                      local_get(start); i32_const(1); i32_add; local_set(start);
                      br(0);
                    end; end;
                    // new_len = len - start
                    local_get(len); local_get(start); i32_sub; local_set(new_len);
                    // Alloc result
                    i32_const(4); local_get(new_len); i32_const(es); i32_mul; i32_add;
                    call(self.emitter.rt.alloc); local_set(dst);
                    local_get(dst); local_get(new_len); i32_store(0);
                    // Copy loop
                    i32_const(0); local_set(copy_i);
                    block_empty; loop_empty;
                      local_get(copy_i); local_get(new_len); i32_ge_u; br_if(1);
                      local_get(dst); i32_const(4); i32_add;
                      local_get(copy_i); i32_const(es); i32_mul; i32_add;
                      local_get(xs); i32_const(4); i32_add;
                      local_get(start); local_get(copy_i); i32_add;
                      i32_const(es); i32_mul; i32_add;
                });
                self.emit_elem_copy(&elem_ty);
                wasm!(self.func, {
                      local_get(copy_i); i32_const(1); i32_add; local_set(copy_i);
                      br(0);
                    end; end;
                    local_get(dst);
                });
                self.scratch.free_i32(copy_i);
                self.scratch.free_i32(dst);
                self.scratch.free_i32(new_len);
                self.scratch.free_i32(start);
                self.scratch.free_i32(len);
                self.scratch.free_i32(closure);
                self.scratch.free_i32(xs);
            }
            "count" => {
                // count(xs, pred) → Int
                let elem_ty = self.list_elem_ty(&args[0].ty);
                let es = values::byte_size(&elem_ty) as i32;
                let xs = self.scratch.alloc_i32();
                let closure = self.scratch.alloc_i32();
                let i = self.scratch.alloc_i32();
                let cnt = self.scratch.alloc_i32();
                self.emit_expr(&args[0]);
                wasm!(self.func, { local_set(xs); });
                self.emit_expr(&args[1]);
                wasm!(self.func, {
                    local_set(closure);
                    i32_const(0); local_set(i);
                    i32_const(0); local_set(cnt);
                    block_empty; loop_empty;
                      local_get(i); local_get(xs); i32_load(0); i32_ge_u; br_if(1);
                      local_get(closure); i32_load(4); // env
                      local_get(xs); i32_const(4); i32_add;
                      local_get(i); i32_const(es); i32_mul; i32_add;
                });
                self.emit_load_at(&elem_ty, 0);
                wasm!(self.func, {
                      local_get(closure); i32_load(0); // table_idx
                });
                self.emit_closure_call(&elem_ty, &Ty::Bool);
                wasm!(self.func, {
                      if_empty;
                        local_get(cnt); i32_const(1); i32_add; local_set(cnt);
                      end;
                      local_get(i); i32_const(1); i32_add; local_set(i);
                      br(0);
                    end; end;
                    local_get(cnt); i64_extend_i32_u;
                });
                self.scratch.free_i32(cnt);
                self.scratch.free_i32(i);
                self.scratch.free_i32(closure);
                self.scratch.free_i32(xs);
            }
            "partition" => {
                // partition(xs, pred) → (List[A], List[A])
                let elem_ty = self.list_elem_ty(&args[0].ty);
                let es = values::byte_size(&elem_ty) as i32;
                let xs = self.scratch.alloc_i32();
                let closure = self.scratch.alloc_i32();
                let len = self.scratch.alloc_i32();
                let true_list = self.scratch.alloc_i32();
                let false_list = self.scratch.alloc_i32();
                let true_cnt = self.scratch.alloc_i32();
                let false_cnt = self.scratch.alloc_i32();
                let i = self.scratch.alloc_i32();
                let tuple_ptr = self.scratch.alloc_i32();
                self.emit_expr(&args[0]);
                wasm!(self.func, { local_set(xs); });
                self.emit_expr(&args[1]);
                wasm!(self.func, {
                    local_set(closure);
                    local_get(xs); i32_load(0); local_set(len);
                    i32_const(4); local_get(len); i32_const(es); i32_mul; i32_add;
                    call(self.emitter.rt.alloc); local_set(true_list);
                    i32_const(4); local_get(len); i32_const(es); i32_mul; i32_add;
                    call(self.emitter.rt.alloc); local_set(false_list);
                    i32_const(0); local_set(true_cnt);
                    i32_const(0); local_set(false_cnt);
                    i32_const(0); local_set(i);
                    block_empty; loop_empty;
                      local_get(i); local_get(len); i32_ge_u; br_if(1);
                      local_get(closure); i32_load(4); // env
                      local_get(xs); i32_const(4); i32_add;
                      local_get(i); i32_const(es); i32_mul; i32_add;
                });
                self.emit_load_at(&elem_ty, 0);
                wasm!(self.func, {
                      local_get(closure); i32_load(0); // table_idx
                });
                self.emit_closure_call(&elem_ty, &Ty::Bool);
                wasm!(self.func, {
                      if_i32;
                        local_get(true_list); i32_const(4); i32_add;
                        local_get(true_cnt); i32_const(es); i32_mul; i32_add;
                        local_get(xs); i32_const(4); i32_add;
                        local_get(i); i32_const(es); i32_mul; i32_add;
                });
                self.emit_elem_copy(&elem_ty);
                wasm!(self.func, {
                        local_get(true_cnt); i32_const(1); i32_add; local_set(true_cnt);
                        i32_const(0);
                      else_;
                        local_get(false_list); i32_const(4); i32_add;
                        local_get(false_cnt); i32_const(es); i32_mul; i32_add;
                        local_get(xs); i32_const(4); i32_add;
                        local_get(i); i32_const(es); i32_mul; i32_add;
                });
                self.emit_elem_copy(&elem_ty);
                wasm!(self.func, {
                        local_get(false_cnt); i32_const(1); i32_add; local_set(false_cnt);
                        i32_const(0);
                      end;
                      drop;
                      local_get(i); i32_const(1); i32_add; local_set(i);
                      br(0);
                    end; end;
                    local_get(true_list); local_get(true_cnt); i32_store(0);
                    local_get(false_list); local_get(false_cnt); i32_store(0);
                    i32_const(8); call(self.emitter.rt.alloc); local_set(tuple_ptr);
                    local_get(tuple_ptr); local_get(true_list); i32_store(0);
                    local_get(tuple_ptr); local_get(false_list); i32_store(4);
                    local_get(tuple_ptr);
                });
                self.scratch.free_i32(tuple_ptr);
                self.scratch.free_i32(i);
                self.scratch.free_i32(false_cnt);
                self.scratch.free_i32(true_cnt);
                self.scratch.free_i32(false_list);
                self.scratch.free_i32(true_list);
                self.scratch.free_i32(len);
                self.scratch.free_i32(closure);
                self.scratch.free_i32(xs);
            }
            "update" => {
                // update(xs, i, f) → List[A]: copy with xs[i] replaced by f(xs[i])
                let elem_ty = self.list_elem_ty(&args[0].ty);
                let es = values::byte_size(&elem_ty) as i32;
                let xs = self.scratch.alloc_i32();
                let idx = self.scratch.alloc_i32();
                let closure = self.scratch.alloc_i32();
                let len = self.scratch.alloc_i32();
                let dst = self.scratch.alloc_i32();
                let copy_i = self.scratch.alloc_i32();
                self.emit_expr(&args[0]);
                wasm!(self.func, { local_set(xs); });
                self.emit_expr(&args[1]);
                wasm!(self.func, { i32_wrap_i64; local_set(idx); });
                self.emit_expr(&args[2]);
                wasm!(self.func, {
                    local_set(closure);
                    local_get(xs); i32_load(0); local_set(len);
                    // Alloc copy
                    i32_const(4); local_get(len); i32_const(es); i32_mul; i32_add;
                    call(self.emitter.rt.alloc); local_set(dst);
                    local_get(dst); local_get(len); i32_store(0);
                    // Copy all elements
                    i32_const(0); local_set(copy_i);
                    block_empty; loop_empty;
                      local_get(copy_i); local_get(len); i32_ge_u; br_if(1);
                      local_get(dst); i32_const(4); i32_add;
                      local_get(copy_i); i32_const(es); i32_mul; i32_add;
                      local_get(xs); i32_const(4); i32_add;
                      local_get(copy_i); i32_const(es); i32_mul; i32_add;
                });
                self.emit_elem_copy(&elem_ty);
                wasm!(self.func, {
                      local_get(copy_i); i32_const(1); i32_add; local_set(copy_i);
                      br(0);
                    end; end;
                });
                // Replace dst[idx] with f(dst[idx])
                wasm!(self.func, {
                    local_get(dst); i32_const(4); i32_add;
                    local_get(idx); i32_const(es); i32_mul; i32_add;
                    // Call f(dst[idx])
                    local_get(closure); i32_load(4); // env
                    local_get(dst); i32_const(4); i32_add;
                    local_get(idx); i32_const(es); i32_mul; i32_add;
                });
                self.emit_load_at(&elem_ty, 0);
                wasm!(self.func, {
                    local_get(closure); i32_load(0); // table_idx
                });
                self.emit_closure_call(&elem_ty, &elem_ty);
                self.emit_elem_store(&elem_ty);
                wasm!(self.func, { local_get(dst); });
                self.scratch.free_i32(copy_i);
                self.scratch.free_i32(dst);
                self.scratch.free_i32(len);
                self.scratch.free_i32(closure);
                self.scratch.free_i32(idx);
                self.scratch.free_i32(xs);
            }
            "scan" => {
                // scan(xs, init, f) → List[B]: like fold but collect intermediates
                let elem_ty = self.list_elem_ty(&args[0].ty);
                let es = values::byte_size(&elem_ty) as i32;
                let acc_vt = values::ty_to_valtype(&args[1].ty).unwrap_or(ValType::I64);
                let acc_size = values::byte_size(&args[1].ty) as i32;
                let xs = self.scratch.alloc_i32();
                let closure = self.scratch.alloc_i32();
                let len = self.scratch.alloc_i32();
                let result = self.scratch.alloc_i32();
                let i = self.scratch.alloc_i32();
                let acc = self.scratch.alloc(acc_vt);
                self.emit_expr(&args[0]);
                wasm!(self.func, { local_set(xs); });
                self.emit_expr(&args[1]);
                wasm!(self.func, { local_set(acc); });
                self.emit_expr(&args[2]);
                wasm!(self.func, {
                    local_set(closure);
                    local_get(xs); i32_load(0); local_set(len);
                    i32_const(4); local_get(len); i32_const(acc_size); i32_mul; i32_add;
                    call(self.emitter.rt.alloc); local_set(result);
                    local_get(result); local_get(len); i32_store(0);
                    i32_const(0); local_set(i);
                    block_empty; loop_empty;
                      local_get(i); local_get(len); i32_ge_u; br_if(1);
                      local_get(closure); i32_load(4); // env
                      local_get(acc);
                      local_get(xs); i32_const(4); i32_add;
                      local_get(i); i32_const(es); i32_mul; i32_add;
                });
                self.emit_load_at(&elem_ty, 0);
                wasm!(self.func, {
                      local_get(closure); i32_load(0); // table_idx
                });
                {
                    let mut ct = vec![ValType::I32, acc_vt];
                    if let Some(vt) = values::ty_to_valtype(&elem_ty) { ct.push(vt); }
                    self.emit_call_indirect(ct, vec![acc_vt]);
                }
                wasm!(self.func, {
                      local_set(acc);
                      local_get(result); i32_const(4); i32_add;
                      local_get(i); i32_const(acc_size); i32_mul; i32_add;
                      local_get(acc);
                });
                match acc_vt {
                    ValType::F64 => { wasm!(self.func, { f64_store(0); }); }
                    _ => { wasm!(self.func, { i64_store(0); }); }
                }
                wasm!(self.func, {
                      local_get(i); i32_const(1); i32_add; local_set(i);
                      br(0);
                    end; end;
                    local_get(result);
                });
                self.scratch.free(acc, acc_vt);
                self.scratch.free_i32(i);
                self.scratch.free_i32(result);
                self.scratch.free_i32(len);
                self.scratch.free_i32(closure);
                self.scratch.free_i32(xs);
            }
            "zip_with" => {
                // zip_with(xs, ys, f) → List[C]
                let a_ty = self.list_elem_ty(&args[0].ty);
                let b_ty = self.list_elem_ty(&args[1].ty);
                let a_size = values::byte_size(&a_ty) as i32;
                let b_size = values::byte_size(&b_ty) as i32;
                let ret_elem_ty = self.list_elem_ty(&self.stub_ret_ty);
                let out_size = values::byte_size(&ret_elem_ty) as i32;
                let out_vt = values::ty_to_valtype(&ret_elem_ty).unwrap_or(ValType::I32);
                let xs = self.scratch.alloc_i32();
                let ys = self.scratch.alloc_i32();
                let closure = self.scratch.alloc_i32();
                let len = self.scratch.alloc_i32();
                let result = self.scratch.alloc_i32();
                let i = self.scratch.alloc_i32();
                self.emit_expr(&args[0]);
                wasm!(self.func, { local_set(xs); });
                self.emit_expr(&args[1]);
                wasm!(self.func, { local_set(ys); });
                self.emit_expr(&args[2]);
                wasm!(self.func, {
                    local_set(closure);
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
                    i32_const(4); local_get(len); i32_const(out_size); i32_mul; i32_add;
                    call(self.emitter.rt.alloc); local_set(result);
                    local_get(result); local_get(len); i32_store(0);
                    i32_const(0); local_set(i);
                    block_empty; loop_empty;
                      local_get(i); local_get(len); i32_ge_u; br_if(1);
                      // dst addr
                      local_get(result); i32_const(4); i32_add;
                      local_get(i); i32_const(out_size); i32_mul; i32_add;
                      // Call f(xs[i], ys[i])
                      local_get(closure); i32_load(4); // env
                      local_get(xs); i32_const(4); i32_add;
                      local_get(i); i32_const(a_size); i32_mul; i32_add;
                });
                self.emit_load_at(&a_ty, 0);
                wasm!(self.func, {
                      local_get(ys); i32_const(4); i32_add;
                      local_get(i); i32_const(b_size); i32_mul; i32_add;
                });
                self.emit_load_at(&b_ty, 0);
                wasm!(self.func, {
                      local_get(closure); i32_load(0); // table_idx
                });
                {
                    let mut ct = vec![ValType::I32];
                    if let Some(vt) = values::ty_to_valtype(&a_ty) { ct.push(vt); }
                    if let Some(vt) = values::ty_to_valtype(&b_ty) { ct.push(vt); }
                    self.emit_call_indirect(ct, values::ret_type(&ret_elem_ty));
                }
                match out_vt {
                    ValType::I64 => { wasm!(self.func, { i64_store(0); }); }
                    ValType::F64 => { wasm!(self.func, { f64_store(0); }); }
                    _ => { wasm!(self.func, { i32_store(0); }); }
                }
                wasm!(self.func, {
                      local_get(i); i32_const(1); i32_add; local_set(i);
                      br(0);
                    end; end;
                    local_get(result);
                });
                self.scratch.free_i32(i);
                self.scratch.free_i32(result);
                self.scratch.free_i32(len);
                self.scratch.free_i32(closure);
                self.scratch.free_i32(ys);
                self.scratch.free_i32(xs);
            }
            "unique_by" => {
                // unique_by(xs, f) → List[A]: remove dupes by key, keep first (O(n²))
                let elem_ty = self.list_elem_ty(&args[0].ty);
                let es = values::byte_size(&elem_ty) as i32;
                let xs = self.scratch.alloc_i32();
                let closure = self.scratch.alloc_i32();
                let len = self.scratch.alloc_i32();
                let keys = self.scratch.alloc_i32();
                let i = self.scratch.alloc_i32();
                let dst = self.scratch.alloc_i32();
                let seen_keys = self.scratch.alloc_i32();
                let out_count = self.scratch.alloc_i32();
                let j = self.scratch.alloc_i32();
                let found = self.scratch.alloc_i32();
                let key_val = self.scratch.alloc_i64();
                self.emit_expr(&args[0]);
                wasm!(self.func, { local_set(xs); });
                self.emit_expr(&args[1]);
                wasm!(self.func, {
                    local_set(closure);
                    local_get(xs); i32_load(0); local_set(len);
                    // Alloc keys array: len * 8
                    local_get(len); i32_const(8); i32_mul;
                    call(self.emitter.rt.alloc); local_set(keys);
                    // Compute all keys
                    i32_const(0); local_set(i);
                    block_empty; loop_empty;
                      local_get(i); local_get(len); i32_ge_u; br_if(1);
                      local_get(closure); i32_load(4); // env
                      local_get(xs); i32_const(4); i32_add;
                      local_get(i); i32_const(es); i32_mul; i32_add;
                });
                self.emit_load_at(&elem_ty, 0);
                wasm!(self.func, {
                      local_get(closure); i32_load(0); // table_idx
                });
                self.emit_closure_call(&elem_ty, &Ty::Int); // key fn returns Int (i64)
                wasm!(self.func, {
                      local_set(key_val);
                      local_get(keys);
                      local_get(i); i32_const(8); i32_mul; i32_add;
                      local_get(key_val); i64_store(0);
                      local_get(i); i32_const(1); i32_add; local_set(i);
                      br(0);
                    end; end;
                });
                // Build result: include xs[i] if keys[i] not in seen_keys[0..out_count]
                wasm!(self.func, {
                    i32_const(4); local_get(len); i32_const(es); i32_mul; i32_add;
                    call(self.emitter.rt.alloc); local_set(dst);
                    local_get(len); i32_const(8); i32_mul;
                    call(self.emitter.rt.alloc); local_set(seen_keys);
                    i32_const(0); local_set(out_count);
                    i32_const(0); local_set(i);
                    block_empty; loop_empty;
                      local_get(i); local_get(len); i32_ge_u; br_if(1);
                      local_get(keys);
                      local_get(i); i32_const(8); i32_mul; i32_add;
                      i64_load(0); local_set(key_val);
                      i32_const(0); local_set(j);
                      i32_const(0); local_set(found);
                      block_empty; loop_empty;
                        local_get(j); local_get(out_count); i32_ge_u; br_if(1);
                        local_get(seen_keys);
                        local_get(j); i32_const(8); i32_mul; i32_add;
                        i64_load(0); local_get(key_val); i64_eq;
                        if_empty; i32_const(1); local_set(found); br(2); end;
                        local_get(j); i32_const(1); i32_add; local_set(j);
                        br(0);
                      end; end;
                      local_get(found); i32_eqz;
                      if_empty;
                        local_get(dst); i32_const(4); i32_add;
                        local_get(out_count); i32_const(es); i32_mul; i32_add;
                        local_get(xs); i32_const(4); i32_add;
                        local_get(i); i32_const(es); i32_mul; i32_add;
                });
                self.emit_elem_copy(&elem_ty);
                wasm!(self.func, {
                        local_get(seen_keys);
                        local_get(out_count); i32_const(8); i32_mul; i32_add;
                        local_get(key_val); i64_store(0);
                        local_get(out_count); i32_const(1); i32_add; local_set(out_count);
                      end;
                      local_get(i); i32_const(1); i32_add; local_set(i);
                      br(0);
                    end; end;
                    local_get(dst); local_get(out_count); i32_store(0);
                    local_get(dst);
                });
                self.scratch.free_i64(key_val);
                self.scratch.free_i32(found);
                self.scratch.free_i32(j);
                self.scratch.free_i32(out_count);
                self.scratch.free_i32(seen_keys);
                self.scratch.free_i32(dst);
                self.scratch.free_i32(i);
                self.scratch.free_i32(keys);
                self.scratch.free_i32(len);
                self.scratch.free_i32(closure);
                self.scratch.free_i32(xs);
            }
            "group_by" => {
                // group_by(xs, f) → Map[B, List[A]]
                // Two-phase: compute keys, then build map with linear scan
                let elem_ty = self.resolve_list_elem(&args[0], args.get(1));
                let key_ty = if let Ty::Fn { ret, .. } = &args[1].ty {
                    (**ret).clone()
                } else { Ty::String };
                let es = values::byte_size(&elem_ty) as i32;
                let ks = values::byte_size(&key_ty) as i32;
                let entry_size = ks + 4; // key + list_ptr(i32)
                let key_is_i64 = matches!(&key_ty, Ty::Int);

                let xs = self.scratch.alloc_i32();
                let closure = self.scratch.alloc_i32();
                let len = self.scratch.alloc_i32();
                let keys_arr = self.scratch.alloc_i32();
                let map_ptr = self.scratch.alloc_i32();
                let map_len = self.scratch.alloc_i32();
                let i = self.scratch.alloc_i32();
                let j = self.scratch.alloc_i32();
                let found_idx = self.scratch.alloc_i32();
                let old_list = self.scratch.alloc_i32();
                let new_list = self.scratch.alloc_i32();
                let old_len = self.scratch.alloc_i32();
                let cur_key = if key_is_i64 { self.scratch.alloc_i64() } else { self.scratch.alloc_i32() };

                self.emit_expr(&args[0]);
                wasm!(self.func, { local_set(xs); });
                self.emit_expr(&args[1]);
                wasm!(self.func, {
                    local_set(closure);
                    local_get(xs); i32_load(0); local_set(len);
                });

                // Phase 1: Compute keys
                wasm!(self.func, {
                    local_get(len); i32_const(ks); i32_mul;
                    call(self.emitter.rt.alloc); local_set(keys_arr);
                    i32_const(0); local_set(i);
                    block_empty; loop_empty;
                      local_get(i); local_get(len); i32_ge_u; br_if(1);
                      local_get(closure); i32_load(4);
                      local_get(xs); i32_const(4); i32_add;
                      local_get(i); i32_const(es); i32_mul; i32_add;
                });
                self.emit_load_at(&elem_ty, 0);
                wasm!(self.func, { local_get(closure); i32_load(0); });
                self.emit_closure_call(&elem_ty, &key_ty);
                wasm!(self.func, { local_set(cur_key); });
                wasm!(self.func, {
                      local_get(keys_arr);
                      local_get(i); i32_const(ks); i32_mul; i32_add;
                      local_get(cur_key);
                });
                if key_is_i64 { wasm!(self.func, { i64_store(0); }); }
                else { wasm!(self.func, { i32_store(0); }); }
                wasm!(self.func, {
                      local_get(i); i32_const(1); i32_add; local_set(i);
                      br(0);
                    end; end;
                });

                // Phase 2: Build map
                wasm!(self.func, {
                    i32_const(4); local_get(len); i32_const(entry_size); i32_mul; i32_add;
                    call(self.emitter.rt.alloc); local_set(map_ptr);
                    i32_const(0); local_set(map_len);
                    i32_const(0); local_set(i);
                    block_empty; loop_empty;
                      local_get(i); local_get(len); i32_ge_u; br_if(1);
                      local_get(keys_arr);
                      local_get(i); i32_const(ks); i32_mul; i32_add;
                });
                if key_is_i64 { wasm!(self.func, { i64_load(0); local_set(cur_key); }); }
                else { wasm!(self.func, { i32_load(0); local_set(cur_key); }); }
                // Search map for cur_key → found_idx (-1 if not found)
                wasm!(self.func, {
                      i32_const(-1); local_set(found_idx);
                      i32_const(0); local_set(j);
                      block_empty; loop_empty;
                        local_get(j); local_get(map_len); i32_ge_u; br_if(1);
                        local_get(map_ptr); i32_const(4); i32_add;
                        local_get(j); i32_const(entry_size); i32_mul; i32_add;
                });
                if key_is_i64 { wasm!(self.func, { i64_load(0); local_get(cur_key); i64_eq; }); }
                else {
                    wasm!(self.func, { i32_load(0); local_get(cur_key); });
                    self.emit_key_eq(&key_ty);
                }
                wasm!(self.func, {
                        if_empty; local_get(j); local_set(found_idx); end;
                        local_get(j); i32_const(1); i32_add; local_set(j);
                        br(0);
                      end; end;
                      local_get(found_idx); i32_const(-1); i32_ne;
                      if_empty;
                        // === Found: append element to existing list ===
                        local_get(map_ptr); i32_const(4); i32_add;
                        local_get(found_idx); i32_const(entry_size); i32_mul; i32_add;
                        i32_const(ks); i32_add;
                        i32_load(0);
                        local_set(old_list);
                        local_get(old_list); i32_load(0); local_set(old_len);
                        i32_const(4);
                        local_get(old_len); i32_const(1); i32_add;
                        i32_const(es); i32_mul; i32_add;
                        call(self.emitter.rt.alloc); local_set(new_list);
                        local_get(new_list);
                        local_get(old_len); i32_const(1); i32_add; i32_store(0);
                        local_get(new_list); i32_const(4); i32_add;
                        local_get(old_list); i32_const(4); i32_add;
                        local_get(old_len); i32_const(es); i32_mul;
                        memory_copy;
                        local_get(new_list); i32_const(4); i32_add;
                        local_get(old_len); i32_const(es); i32_mul; i32_add;
                        local_get(xs); i32_const(4); i32_add;
                        local_get(i); i32_const(es); i32_mul; i32_add;
                });
                self.emit_load_at(&elem_ty, 0);
                self.emit_store_at(&elem_ty, 0);
                wasm!(self.func, {
                        local_get(map_ptr); i32_const(4); i32_add;
                        local_get(found_idx); i32_const(entry_size); i32_mul; i32_add;
                        i32_const(ks); i32_add;
                        local_get(new_list); i32_store(0);
                      else_;
                        // === Not found: create new entry ===
                        i32_const(4); i32_const(es); i32_add;
                        call(self.emitter.rt.alloc); local_set(new_list);
                        local_get(new_list); i32_const(1); i32_store(0);
                        local_get(new_list); i32_const(4); i32_add;
                        local_get(xs); i32_const(4); i32_add;
                        local_get(i); i32_const(es); i32_mul; i32_add;
                });
                self.emit_load_at(&elem_ty, 0);
                self.emit_store_at(&elem_ty, 0);
                wasm!(self.func, {
                        local_get(map_ptr); i32_const(4); i32_add;
                        local_get(map_len); i32_const(entry_size); i32_mul; i32_add;
                        local_get(cur_key);
                });
                if key_is_i64 { wasm!(self.func, { i64_store(0); }); }
                else { wasm!(self.func, { i32_store(0); }); }
                wasm!(self.func, {
                        local_get(map_ptr); i32_const(4); i32_add;
                        local_get(map_len); i32_const(entry_size); i32_mul; i32_add;
                        i32_const(ks); i32_add;
                        local_get(new_list); i32_store(0);
                        local_get(map_len); i32_const(1); i32_add; local_set(map_len);
                      end;
                      local_get(i); i32_const(1); i32_add; local_set(i);
                      br(0);
                    end; end;
                    local_get(map_ptr); local_get(map_len); i32_store(0);
                    local_get(map_ptr);
                });

                if key_is_i64 { self.scratch.free_i64(cur_key); } else { self.scratch.free_i32(cur_key); }
                self.scratch.free_i32(old_len);
                self.scratch.free_i32(new_list);
                self.scratch.free_i32(old_list);
                self.scratch.free_i32(found_idx);
                self.scratch.free_i32(j);
                self.scratch.free_i32(i);
                self.scratch.free_i32(map_len);
                self.scratch.free_i32(map_ptr);
                self.scratch.free_i32(keys_arr);
                self.scratch.free_i32(len);
                self.scratch.free_i32(closure);
                self.scratch.free_i32(xs);
            }
            "shuffle" => {
                // shuffle(xs) → List[A]: delegate to random.shuffle implementation
                self.emit_random_call("shuffle", args);
                return true;
            }
            "filter" => {
                // filter(list, fn) → new list with matching elements
                let elem_ty = self.resolve_list_elem(&args[0], args.get(1));
                let elem_size = values::byte_size(&elem_ty);
                let src = self.scratch.alloc_i32();
                let closure = self.scratch.alloc_i32();
                let len = self.scratch.alloc_i32();
                let idx = self.scratch.alloc_i32();
                let dst = self.scratch.alloc_i32();
                let out_idx = self.scratch.alloc_i32();
                self.emit_expr(&args[0]);
                wasm!(self.func, { local_set(src); });
                self.emit_expr(&args[1]);
                wasm!(self.func, {
                    local_set(closure);
                    local_get(src); i32_load(0); local_set(len);
                    // alloc dst (max size)
                    i32_const(4); local_get(len); i32_const(elem_size as i32); i32_mul; i32_add;
                    call(self.emitter.rt.alloc); local_set(dst);
                    i32_const(0); local_set(out_idx);
                    i32_const(0); local_set(idx);
                    block_empty; loop_empty;
                });
                let depth_guard = self.depth_push_n(2);
                wasm!(self.func, {
                    local_get(idx); local_get(len); i32_ge_u; br_if(1);
                    // Call predicate
                    local_get(closure); i32_load(4); // env
                    local_get(src); i32_const(4); i32_add;
                    local_get(idx); i32_const(elem_size as i32); i32_mul; i32_add;
                });
                self.emit_load_at(&elem_ty, 0);
                wasm!(self.func, { local_get(closure); i32_load(0); }); // table_idx
                self.emit_closure_call(&elem_ty, &Ty::Bool);
                wasm!(self.func, {
                    if_empty;
                    // dst[out_idx] = src[idx]
                    local_get(dst); i32_const(4); i32_add;
                    local_get(out_idx); i32_const(elem_size as i32); i32_mul; i32_add;
                    local_get(src); i32_const(4); i32_add;
                    local_get(idx); i32_const(elem_size as i32); i32_mul; i32_add;
                });
                self.emit_load_at(&elem_ty, 0);
                self.emit_store_at(&elem_ty, 0);
                wasm!(self.func, {
                    local_get(out_idx); i32_const(1); i32_add; local_set(out_idx);
                    end; // end if
                    local_get(idx); i32_const(1); i32_add; local_set(idx);
                    br(0);
                });
                self.depth_pop(depth_guard);
                wasm!(self.func, {
                    end; end;
                    local_get(dst); local_get(out_idx); i32_store(0);
                    local_get(dst);
                });
                self.scratch.free_i32(out_idx);
                self.scratch.free_i32(dst);
                self.scratch.free_i32(idx);
                self.scratch.free_i32(len);
                self.scratch.free_i32(closure);
                self.scratch.free_i32(src);
            }
            "fold" => {
                // fold(list, init, fn(acc, elem) → acc)
                // Resolve types from closure Fn signature when available
                // Derive element type from the input list (most reliable source)
                let list_elem_ty = if let Ty::Applied(_, a) = &args[0].ty {
                    a.first().cloned().unwrap_or(Ty::Int)
                } else { Ty::Int };

                // Resolve elem_ty: use list element type, Fn param, or lambda param — first concrete wins
                let elem_ty = [
                    Some(list_elem_ty),
                    if let Ty::Fn { params, .. } = &args[2].ty { params.get(1).cloned() } else { None },
                    if let crate::ir::IrExprKind::Lambda { params: lp, .. } = &args[2].kind { lp.get(1).map(|(_, t)| t.clone()) } else { None },
                ].into_iter().flatten()
                    .find(|t| !matches!(t, Ty::Unknown | Ty::TypeVar(_)))
                    .unwrap_or(Ty::Int);

                // Resolve acc type: use Fn return type or lambda body type, with TypeVar→concrete fallback
                let acc_ty_resolved = {
                    // Try closure Fn ret type
                    let fn_ret = if let Ty::Fn { ret, .. } = &args[2].ty { Some(*ret.clone()) } else { None };
                    // Try lambda body type
                    let body_ty = if let crate::ir::IrExprKind::Lambda { body, .. } = &args[2].kind { Some(body.ty.clone()) } else { None };
                    // Try init type
                    let init_ty = args[1].ty.clone();
                    // Pick first concrete (non-TypeVar/Unknown) type
                    [fn_ret, body_ty, Some(init_ty)].into_iter().flatten()
                        .find(|t| !matches!(t, Ty::Unknown | Ty::TypeVar(_)))
                        .unwrap_or_else(|| if let Ty::Fn { ret, .. } = &args[2].ty { *ret.clone() } else { args[1].ty.clone() })
                };
                // Resolve TypeVar inside Applied types (e.g., List[TypeVar(?0)] → List[elem_ty])
                let acc_ty_resolved = match acc_ty_resolved {
                    Ty::Applied(id, ref inner) if inner.iter().any(|t| matches!(t, Ty::TypeVar(_))) => {
                        let resolved_inner: Vec<Ty> = inner.iter().map(|t| {
                            if matches!(t, Ty::TypeVar(_)) { elem_ty.clone() } else { t.clone() }
                        }).collect();
                        Ty::Applied(id, resolved_inner)
                    }
                    other => other,
                };
                let elem_size = values::byte_size(&elem_ty);
                let acc_vt = values::ty_to_valtype(&acc_ty_resolved).unwrap_or(ValType::I32);
                let list_ptr = self.scratch.alloc_i32();
                let closure = self.scratch.alloc_i32();
                let len = self.scratch.alloc_i32();
                let idx = self.scratch.alloc_i32();
                let acc = self.scratch.alloc(acc_vt);
                self.emit_expr(&args[0]);
                wasm!(self.func, { local_set(list_ptr); });
                self.emit_expr(&args[1]);
                wasm!(self.func, { local_set(acc); });
                self.emit_expr(&args[2]);
                wasm!(self.func, {
                    local_set(closure);
                    local_get(list_ptr); i32_load(0); local_set(len);
                    i32_const(0); local_set(idx);
                    block_empty; loop_empty;
                });
                let depth_guard = self.depth_push_n(2);
                wasm!(self.func, {
                    local_get(idx); local_get(len); i32_ge_u; br_if(1);
                    local_get(closure); i32_load(4); // env
                    local_get(acc);
                    local_get(list_ptr); i32_const(4); i32_add;
                    local_get(idx); i32_const(elem_size as i32); i32_mul; i32_add;
                });
                self.emit_load_at(&elem_ty, 0);
                wasm!(self.func, { local_get(closure); i32_load(0); }); // table_idx
                {
                    // Build call_indirect type from resolved acc/elem types
                    let mut ct = vec![ValType::I32]; // env
                    if let Some(vt) = values::ty_to_valtype(&acc_ty_resolved) { ct.push(vt); }
                    if let Some(vt) = values::ty_to_valtype(&elem_ty) { ct.push(vt); }
                    self.emit_call_indirect(ct, values::ret_type(&acc_ty_resolved));
                }
                wasm!(self.func, {
                    local_set(acc);
                    local_get(idx); i32_const(1); i32_add; local_set(idx);
                    br(0);
                });
                self.depth_pop(depth_guard);
                wasm!(self.func, { end; end; local_get(acc); });
                self.scratch.free(acc, acc_vt);
                self.scratch.free_i32(idx);
                self.scratch.free_i32(len);
                self.scratch.free_i32(closure);
                self.scratch.free_i32(list_ptr);
            }
            "map" => {
                let ret_ty = self.stub_ret_ty.clone();
                self.emit_list_map(&args[0], &args[1], &ret_ty);
            }
            _ => return false,
        }
        true
    }

    /// Emit list.map(list, fn) → new list with fn applied to each element.
    /// Uses scratch locals (not mem[]) to survive nested calls from call_indirect.
    /// Key insight: compute dst address BEFORE call_indirect so result goes
    /// directly onto the stack in the right position for store.
    pub(super) fn emit_list_map(&mut self, list_arg: &IrExpr, fn_arg: &IrExpr, ret_ty: &Ty) {
        // Resolve input element type from multiple sources
        let in_elem_ty = {
            let from_list = if let Ty::Applied(_, args) = &list_arg.ty {
                args.first().cloned()
            } else { None };
            let from_var = if from_list.as_ref().map_or(true, |t| matches!(t, Ty::TypeVar(_) | Ty::Unknown)) {
                if let crate::ir::IrExprKind::Var { id } = &list_arg.kind {
                    if let Ty::Applied(_, a) = &self.var_table.get(*id).ty {
                        a.first().cloned()
                    } else { None }
                } else { None }
            } else { None };
            let from_fn = if let Ty::Fn { params, .. } = &fn_arg.ty {
                params.first().cloned()
            } else { None };
            let from_lambda = if let crate::ir::IrExprKind::Lambda { params, .. } = &fn_arg.kind {
                params.first().map(|(_, t)| t.clone())
            } else { None };
            [from_list, from_var, from_fn, from_lambda].into_iter().flatten()
                .find(|t| !matches!(t, Ty::TypeVar(_) | Ty::Unknown))
                .unwrap_or(Ty::Int)
        };
        let mut out_elem_ty = if let Ty::Applied(_, args) = ret_ty {
            args.first().cloned().unwrap_or(Ty::Int)
        } else { Ty::Int };
        // When the return type is unresolved (TypeVar/Unknown), derive the output
        // element type from the lambda body.  The call-site ret_ty can remain
        // unresolved when the map result is unused or only used in a type-agnostic
        // context, but the lambda body always carries the concrete type.
        if matches!(&out_elem_ty, Ty::TypeVar(_) | Ty::Unknown) {
            if let IrExprKind::Lambda { body, .. } = &fn_arg.kind {
                if !matches!(&body.ty, Ty::TypeVar(_) | Ty::Unknown) {
                    out_elem_ty = body.ty.clone();
                }
            }
            // Also try fn_arg.ty.ret as a secondary source
            if matches!(&out_elem_ty, Ty::TypeVar(_) | Ty::Unknown) {
                if let Ty::Fn { ret, .. } = &fn_arg.ty {
                    if !matches!(ret.as_ref(), Ty::TypeVar(_) | Ty::Unknown) {
                        out_elem_ty = *ret.clone();
                    }
                }
            }
        }
        let in_size = values::byte_size(&in_elem_ty);
        let out_size = values::byte_size(&out_elem_ty);

        let len_local = self.scratch.alloc_i32();
        let idx_local = self.scratch.alloc_i32();
        let src_local = self.scratch.alloc_i32();
        let closure_local = self.scratch.alloc_i32();
        let dst_local = self.scratch.alloc_i32();

        self.emit_expr(list_arg);
        wasm!(self.func, { local_set(src_local); });
        self.emit_expr(fn_arg);
        wasm!(self.func, {
            local_set(closure_local);
            local_get(src_local); i32_load(0); local_set(len_local);
            i32_const(4); local_get(len_local); i32_const(out_size as i32); i32_mul; i32_add;
            call(self.emitter.rt.alloc); local_set(dst_local);
            local_get(dst_local); local_get(len_local); i32_store(0);
            i32_const(0); local_set(idx_local);
            block_empty; loop_empty;
        });
        let depth_guard = self.depth_push_n(2);

        wasm!(self.func, {
            local_get(idx_local); local_get(len_local); i32_ge_u; br_if(1);
            // dst addr
            local_get(dst_local); i32_const(4); i32_add;
            local_get(idx_local); i32_const(out_size as i32); i32_mul; i32_add;
            // env_ptr
            local_get(closure_local); i32_load(4);
            // src element
            local_get(src_local); i32_const(4); i32_add;
            local_get(idx_local); i32_const(in_size as i32); i32_mul; i32_add;
        });
        self.emit_load_at(&in_elem_ty, 0);
        wasm!(self.func, { local_get(closure_local); i32_load(0); }); // table_idx
        self.emit_closure_call(&in_elem_ty, &out_elem_ty);
        self.emit_store_at(&out_elem_ty, 0);

        wasm!(self.func, {
            local_get(idx_local); i32_const(1); i32_add; local_set(idx_local);
            br(0);
        });
        self.depth_pop(depth_guard);
        wasm!(self.func, { end; end; local_get(dst_local); });

        self.scratch.free_i32(dst_local);
        self.scratch.free_i32(closure_local);
        self.scratch.free_i32(src_local);
        self.scratch.free_i32(idx_local);
        self.scratch.free_i32(len_local);
    }
}
