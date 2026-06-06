//! List stdlib closure-based call dispatch for WASM codegen (part 2).
//!
//! Functions: take_while, drop_while, count, partition, update, scan, zip_with,
//! unique_by, group_by, shuffle, filter, fold, map, and the emit_list_map helper.

use super::FuncCompiler;
use super::values;
use almide_ir::{BinOp, IrExpr, IrExprKind};
use almide_lang::types::Ty;
use wasm_encoder::ValType;

/// A pipeline stage for stream fusion.
enum PipelineStage<'a> {
    Map(&'a IrExpr),    // lambda expr
    Filter(&'a IrExpr), // lambda expr
}

/// SIMD-eligible map operation for Int→Int.
#[derive(Clone, Copy)]
enum SimdMapOp {
    Mul,
    Add,
    Sub,
}

impl FuncCompiler<'_> {
    /// Dispatch list closure calls (second half). Returns true if handled.
    pub(super) fn emit_list_closure_call2(&mut self, method: &str, args: &[IrExpr]) -> bool {
        use super::engine::layout::{LIST, SWISS_MAP, list as ll, map as lm};
        let list_data_off = self.emitter.layout_reg.fixed_offset(LIST, ll::DATA) as i32;
        let list_hdr = self.emitter.layout_reg.header_size(LIST) as i32;
        let map_tags_off = self.emitter.layout_reg.fixed_offset(SWISS_MAP, lm::TAGS) as i32;
        let map_hdr = self.emitter.layout_reg.header_size(SWISS_MAP) as i32;
        let map_cap_off = self.emitter.layout_reg.fixed_offset(SWISS_MAP, lm::CAP);
        match method {
            "take_while" => {
                // take_while(xs, pred) → List[A]: take while pred returns true
                let elem_ty = self.resolve_list_elem(&args[0], None);
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
                      local_get(xs); i32_const(list_data_off); i32_add;
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
                    i32_const(list_hdr); local_get(count); i32_const(es); i32_mul; i32_add;
                    call(self.emitter.rt.alloc); local_set(dst);
                    local_get(dst); local_get(count); i32_store(0);
                    // Copy loop
                    i32_const(0); local_set(copy_i);
                    block_empty; loop_empty;
                      local_get(copy_i); local_get(count); i32_ge_u; br_if(1);
                      local_get(dst); i32_const(list_data_off); i32_add;
                      local_get(copy_i); i32_const(es); i32_mul; i32_add;
                      local_get(xs); i32_const(list_data_off); i32_add;
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
                let elem_ty = self.resolve_list_elem(&args[0], None);
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
                      local_get(xs); i32_const(list_data_off); i32_add;
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
                    i32_const(list_hdr); local_get(new_len); i32_const(es); i32_mul; i32_add;
                    call(self.emitter.rt.alloc); local_set(dst);
                    local_get(dst); local_get(new_len); i32_store(0);
                    // Copy loop
                    i32_const(0); local_set(copy_i);
                    block_empty; loop_empty;
                      local_get(copy_i); local_get(new_len); i32_ge_u; br_if(1);
                      local_get(dst); i32_const(list_data_off); i32_add;
                      local_get(copy_i); i32_const(es); i32_mul; i32_add;
                      local_get(xs); i32_const(list_data_off); i32_add;
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
                let elem_ty = self.resolve_list_elem(&args[0], None);
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
                      local_get(xs); i32_const(list_data_off); i32_add;
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
                let elem_ty = self.resolve_list_elem(&args[0], None);
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
                    i32_const(list_hdr); local_get(len); i32_const(es); i32_mul; i32_add;
                    call(self.emitter.rt.alloc); local_set(true_list);
                    i32_const(list_hdr); local_get(len); i32_const(es); i32_mul; i32_add;
                    call(self.emitter.rt.alloc); local_set(false_list);
                    i32_const(0); local_set(true_cnt);
                    i32_const(0); local_set(false_cnt);
                    i32_const(0); local_set(i);
                    block_empty; loop_empty;
                      local_get(i); local_get(len); i32_ge_u; br_if(1);
                      local_get(closure); i32_load(4); // env
                      local_get(xs); i32_const(list_data_off); i32_add;
                      local_get(i); i32_const(es); i32_mul; i32_add;
                });
                self.emit_load_at(&elem_ty, 0);
                wasm!(self.func, {
                      local_get(closure); i32_load(0); // table_idx
                });
                self.emit_closure_call(&elem_ty, &Ty::Bool);
                wasm!(self.func, {
                      if_i32;
                        local_get(true_list); i32_const(list_data_off); i32_add;
                        local_get(true_cnt); i32_const(es); i32_mul; i32_add;
                        local_get(xs); i32_const(list_data_off); i32_add;
                        local_get(i); i32_const(es); i32_mul; i32_add;
                });
                self.emit_elem_copy(&elem_ty);
                wasm!(self.func, {
                        local_get(true_cnt); i32_const(1); i32_add; local_set(true_cnt);
                        i32_const(0);
                      else_;
                        local_get(false_list); i32_const(list_data_off); i32_add;
                        local_get(false_cnt); i32_const(es); i32_mul; i32_add;
                        local_get(xs); i32_const(list_data_off); i32_add;
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
                    i32_const(list_hdr); call(self.emitter.rt.alloc); local_set(tuple_ptr);
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
                let elem_ty = self.resolve_list_elem(&args[0], None);
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
                    i32_const(list_hdr); local_get(len); i32_const(es); i32_mul; i32_add;
                    call(self.emitter.rt.alloc); local_set(dst);
                    local_get(dst); local_get(len); i32_store(0);
                    // Copy all elements
                    i32_const(0); local_set(copy_i);
                    block_empty; loop_empty;
                      local_get(copy_i); local_get(len); i32_ge_u; br_if(1);
                      local_get(dst); i32_const(list_data_off); i32_add;
                      local_get(copy_i); i32_const(es); i32_mul; i32_add;
                      local_get(xs); i32_const(list_data_off); i32_add;
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
                    local_get(dst); i32_const(list_data_off); i32_add;
                    local_get(idx); i32_const(es); i32_mul; i32_add;
                    // Call f(dst[idx])
                    local_get(closure); i32_load(4); // env
                    local_get(dst); i32_const(list_data_off); i32_add;
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
                let elem_ty = self.resolve_list_elem(&args[0], None);
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
                    i32_const(list_hdr); local_get(len); i32_const(acc_size); i32_mul; i32_add;
                    call(self.emitter.rt.alloc); local_set(result);
                    local_get(result); local_get(len); i32_store(0);
                    i32_const(0); local_set(i);
                    block_empty; loop_empty;
                      local_get(i); local_get(len); i32_ge_u; br_if(1);
                      local_get(closure); i32_load(4); // env
                      local_get(acc);
                      local_get(xs); i32_const(list_data_off); i32_add;
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
                      local_get(result); i32_const(list_data_off); i32_add;
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
                let a_ty = self.resolve_list_elem(&args[0], None);
                let b_ty = self.resolve_list_elem(&args[1], None);
                let a_size = values::byte_size(&a_ty) as i32;
                let b_size = values::byte_size(&b_ty) as i32;
                // The result element type is the COMBINER's return type — take it
                // directly so the `call_indirect` return type matches the
                // combiner's compiled signature. For a closure-list zip the
                // stub_ret_ty's element can be unresolved (defaults to i32), which
                // mismatched the combiner's real i64 result and trapped.
                let ret_elem_ty = match &args[2].ty {
                    Ty::Fn { ret, .. } if !matches!(ret.as_ref(), Ty::Unknown) => (**ret).clone(),
                    _ => self.list_elem_ty(&self.stub_ret_ty),
                };
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
                    i32_const(list_hdr); local_get(len); i32_const(out_size); i32_mul; i32_add;
                    call(self.emitter.rt.alloc); local_set(result);
                    local_get(result); local_get(len); i32_store(0);
                    i32_const(0); local_set(i);
                    block_empty; loop_empty;
                      local_get(i); local_get(len); i32_ge_u; br_if(1);
                      // dst addr
                      local_get(result); i32_const(list_data_off); i32_add;
                      local_get(i); i32_const(out_size); i32_mul; i32_add;
                      // Call f(xs[i], ys[i])
                      local_get(closure); i32_load(4); // env
                      local_get(xs); i32_const(list_data_off); i32_add;
                      local_get(i); i32_const(a_size); i32_mul; i32_add;
                });
                self.emit_load_at(&a_ty, 0);
                wasm!(self.func, {
                      local_get(ys); i32_const(list_data_off); i32_add;
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
                // unique_by(xs, f) → List[A]: remove dupes by key, keep first (O(n²)).
                //
                // The key function's return type is resolved from the closure
                // (was hard-coded to `Ty::Int`/i64). When the real key was a
                // Bool (i32) the i64 `call_indirect` signature disagreed with
                // the registered closure ABI → `indirect call type mismatch`
                // trap on wasm. Keys are now stored at their natural width and
                // compared with the shared `emit_eq_typed`, matching native
                // `HashSet`-based dedup for any `Eq` key type.
                let elem_ty = self.resolve_list_elem(&args[0], None);
                let es = values::byte_size(&elem_ty) as i32;
                let key_ty = self.resolve_closure_ret_ty(&args[1], &Ty::Int);
                let ks = values::byte_size(&key_ty) as i32;
                let key_vt = values::ty_to_valtype(&key_ty).unwrap_or(ValType::I32);
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
                let key_val = self.scratch.alloc(key_vt);
                self.emit_expr(&args[0]);
                wasm!(self.func, { local_set(xs); });
                self.emit_expr(&args[1]);
                wasm!(self.func, {
                    local_set(closure);
                    local_get(xs); i32_load(0); local_set(len);
                    // Alloc keys array: len * ks
                    local_get(len); i32_const(ks); i32_mul;
                    call(self.emitter.rt.alloc); local_set(keys);
                    // Compute all keys
                    i32_const(0); local_set(i);
                    block_empty; loop_empty;
                      local_get(i); local_get(len); i32_ge_u; br_if(1);
                      local_get(closure); i32_load(4); // env
                      local_get(xs); i32_const(list_data_off); i32_add;
                      local_get(i); i32_const(es); i32_mul; i32_add;
                });
                self.emit_load_at(&elem_ty, 0);
                wasm!(self.func, {
                      local_get(closure); i32_load(0); // table_idx
                });
                self.emit_closure_call(&elem_ty, &key_ty);
                wasm!(self.func, {
                      local_set(key_val);
                      local_get(keys);
                      local_get(i); i32_const(ks); i32_mul; i32_add;
                      local_get(key_val);
                });
                self.emit_store_at(&key_ty, 0);
                wasm!(self.func, {
                      local_get(i); i32_const(1); i32_add; local_set(i);
                      br(0);
                    end; end;
                });
                // Build result: include xs[i] if keys[i] not in seen_keys[0..out_count]
                wasm!(self.func, {
                    i32_const(list_hdr); local_get(len); i32_const(es); i32_mul; i32_add;
                    call(self.emitter.rt.alloc); local_set(dst);
                    local_get(len); i32_const(ks); i32_mul;
                    call(self.emitter.rt.alloc); local_set(seen_keys);
                    i32_const(0); local_set(out_count);
                    i32_const(0); local_set(i);
                    block_empty; loop_empty;
                      local_get(i); local_get(len); i32_ge_u; br_if(1);
                      local_get(keys);
                      local_get(i); i32_const(ks); i32_mul; i32_add;
                });
                self.emit_load_at(&key_ty, 0);
                wasm!(self.func, {
                      local_set(key_val);
                      i32_const(0); local_set(j);
                      i32_const(0); local_set(found);
                      block_empty; loop_empty;
                        local_get(j); local_get(out_count); i32_ge_u; br_if(1);
                        local_get(seen_keys);
                        local_get(j); i32_const(ks); i32_mul; i32_add;
                });
                self.emit_load_at(&key_ty, 0);
                wasm!(self.func, { local_get(key_val); });
                self.emit_eq_typed(&key_ty);
                wasm!(self.func, {
                        if_empty; i32_const(1); local_set(found); br(2); end;
                        local_get(j); i32_const(1); i32_add; local_set(j);
                        br(0);
                      end; end;
                      local_get(found); i32_eqz;
                      if_empty;
                        local_get(dst); i32_const(list_data_off); i32_add;
                        local_get(out_count); i32_const(es); i32_mul; i32_add;
                        local_get(xs); i32_const(list_data_off); i32_add;
                        local_get(i); i32_const(es); i32_mul; i32_add;
                });
                self.emit_elem_copy(&elem_ty);
                wasm!(self.func, {
                        local_get(seen_keys);
                        local_get(out_count); i32_const(ks); i32_mul; i32_add;
                        local_get(key_val);
                });
                self.emit_store_at(&key_ty, 0);
                wasm!(self.func, {
                        local_get(out_count); i32_const(1); i32_add; local_set(out_count);
                      end;
                      local_get(i); i32_const(1); i32_add; local_set(i);
                      br(0);
                    end; end;
                    local_get(dst); local_get(out_count); i32_store(0);
                    local_get(dst);
                });
                self.scratch.free(key_val, key_vt);
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
                      local_get(xs); i32_const(list_data_off); i32_add;
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

                // Phase 2: Build the compact-ordered-dict map.
                // cap = next_pow2(max(len*2, INITIAL_CAP)) — always > distinct keys, so no grow.
                let cap_local = self.scratch.alloc_i32();
                let ib = self.scratch.alloc_i32(); // index base
                let eb = self.scratch.alloc_i32(); // dense entries base
                let cand_ei = self.scratch.alloc_i32(); // candidate dense entry index during probe
                let h2 = self.scratch.alloc_i32();
                let probe_idx = self.scratch.alloc_i32();
                wasm!(self.func, {
                    // cap = next power of 2 >= max(len * 2, INITIAL_CAP)
                    i32_const(lm::INITIAL_CAP as i32); local_set(cap_local);
                    block_empty; loop_empty;
                      local_get(cap_local); local_get(len); i32_const(2); i32_mul; i32_ge_u; br_if(1);
                      local_get(cap_local); i32_const(1); i32_shl; local_set(cap_local);
                      br(0);
                    end; end;
                });
                self.emit_dict_alloc(map_ptr, cap_local, entry_size as u32);
                self.emit_dict_index_base(map_ptr, cap_local);
                wasm!(self.func, { local_set(ib); });
                self.emit_dict_entries_base(map_ptr, cap_local);
                wasm!(self.func, { local_set(eb); });
                wasm!(self.func, {
                    i32_const(0); local_set(map_len);
                    i32_const(0); local_set(i);
                    block_empty; loop_empty;
                      local_get(i); local_get(len); i32_ge_u; br_if(1);
                      // Load key[i]
                      local_get(keys_arr);
                      local_get(i); i32_const(ks); i32_mul; i32_add;
                });
                if key_is_i64 { wasm!(self.func, { i64_load(0); local_set(cur_key); }); }
                else { wasm!(self.func, { i32_load(0); local_set(cur_key); }); }
                // Hash key → h1 (probe index) + h2 (tag). Push the RAW key (i64 for
                // an Int key, ptr for String); `emit_hash_key` consumes the key's
                // natural type and does its OWN i32 reduction. Pre-wrapping an Int
                // key to i32 here fed `emit_hash_key`'s `local.tee` (an i64 temp) an
                // i32 → "local.set's value type must be correct" (invalid module).
                wasm!(self.func, { local_get(cur_key); });
                self.emit_hash_key(&key_ty);
                self.emit_h1_h2(cap_local, probe_idx, h2);
                // Probe for existing key or empty slot
                wasm!(self.func, {
                      i32_const(-1); local_set(found_idx);
                      block_empty; loop_empty;
                });
                self.emit_swiss_tag_load(map_ptr, probe_idx);
                wasm!(self.func, {
                        local_set(j); // reuse j as tag
                        local_get(j); i32_eqz;
                        if_empty;
                          // Empty slot: not found
                          br(2);
                        end;
                        local_get(j); local_get(h2); i32_eq;
                        if_empty;
                          // Tag matches: deref index[probe_idx]-1 → dense entry, compare key
                          local_get(ib); local_get(probe_idx); i32_const(lm::INDEX_SLOT_SIZE as i32); i32_mul; i32_add;
                          i32_load(0); i32_const(1); i32_sub; local_set(cand_ei);
                          local_get(eb); local_get(cand_ei); i32_const(entry_size); i32_mul; i32_add;
                });
                if key_is_i64 { wasm!(self.func, { i64_load(0); local_get(cur_key); i64_eq; }); }
                else {
                    wasm!(self.func, { i32_load(0); local_get(cur_key); });
                    self.emit_key_eq(&key_ty);
                }
                wasm!(self.func, {
                          if_empty;
                            local_get(cand_ei); local_set(found_idx);
                            br(3);
                          end;
                        end;
                        // Advance probe
                        local_get(probe_idx); i32_const(1); i32_add;
                        local_get(cap_local); i32_const(1); i32_sub; i32_and;
                        local_set(probe_idx);
                        br(0);
                      end; end;

                      local_get(found_idx); i32_const(-1); i32_ne;
                      if_empty;
                        // === Found: append element to existing list ===
                        local_get(eb); local_get(found_idx); i32_const(entry_size); i32_mul; i32_add;
                        i32_const(ks); i32_add;
                        i32_load(0);
                        local_set(old_list);
                        local_get(old_list); i32_load(0); local_set(old_len);
                        i32_const(list_hdr);
                        local_get(old_len); i32_const(1); i32_add;
                        i32_const(es); i32_mul; i32_add;
                        call(self.emitter.rt.alloc); local_set(new_list);
                        local_get(new_list);
                        local_get(old_len); i32_const(1); i32_add; i32_store(0);
                        local_get(new_list); i32_const(list_data_off); i32_add;
                        local_get(old_list); i32_const(list_data_off); i32_add;
                        local_get(old_len); i32_const(es); i32_mul;
                        memory_copy;
                        local_get(new_list); i32_const(list_data_off); i32_add;
                        local_get(old_len); i32_const(es); i32_mul; i32_add;
                        local_get(xs); i32_const(list_data_off); i32_add;
                        local_get(i); i32_const(es); i32_mul; i32_add;
                });
                self.emit_load_at(&elem_ty, 0);
                self.emit_store_at(&elem_ty, 0);
                wasm!(self.func, {
                        // Update list ptr in entry
                        local_get(eb); local_get(found_idx); i32_const(entry_size); i32_mul; i32_add;
                        i32_const(ks); i32_add;
                        local_get(new_list); i32_store(0);
                      else_;
                        // === Not found: append a new dense entry at map_len ===
                });
                // Write tag (h2) at the probe slot
                wasm!(self.func, { local_get(h2); });
                self.emit_swiss_tag_store(map_ptr, probe_idx);
                wasm!(self.func, {
                        // index[probe_idx] = map_len + 1 (1-based pointer into dense entries)
                        local_get(ib); local_get(probe_idx); i32_const(lm::INDEX_SLOT_SIZE as i32); i32_mul; i32_add;
                        local_get(map_len); i32_const(1); i32_add; i32_store(0);
                        // Write key at dense entries[map_len]
                        local_get(eb); local_get(map_len); i32_const(entry_size); i32_mul; i32_add;
                        local_get(cur_key);
                });
                if key_is_i64 { wasm!(self.func, { i64_store(0); }); }
                else { wasm!(self.func, { i32_store(0); }); }
                wasm!(self.func, {
                        // Create single-element list
                        i32_const(list_hdr); i32_const(es); i32_add;
                        call(self.emitter.rt.alloc); local_set(new_list);
                        local_get(new_list); i32_const(1); i32_store(0);
                        local_get(new_list); i32_const(list_data_off); i32_add;
                        local_get(xs); i32_const(list_data_off); i32_add;
                        local_get(i); i32_const(es); i32_mul; i32_add;
                });
                self.emit_load_at(&elem_ty, 0);
                self.emit_store_at(&elem_ty, 0);
                wasm!(self.func, {
                        // Write list ptr at dense entries[map_len] + ks
                        local_get(eb); local_get(map_len); i32_const(entry_size); i32_mul; i32_add;
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
                self.scratch.free_i32(probe_idx);
                self.scratch.free_i32(h2);
                self.scratch.free_i32(cand_ei);
                self.scratch.free_i32(eb);
                self.scratch.free_i32(ib);
                self.scratch.free_i32(cap_local);

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
                // Pointer-based iteration + branchless write
                let elem_ty = self.resolve_list_elem(&args[0], args.get(1));
                let elem_size = values::byte_size(&elem_ty);
                // Perceus in-place reuse: single-use source → write results into same buffer
                let in_place = self.is_single_use_var(&args[0]);
                let src = self.scratch.alloc_i32();
                let closure = self.scratch.alloc_i32();
                let dst = self.scratch.alloc_i32();
                let src_ptr = self.scratch.alloc_i32();
                let end_ptr = self.scratch.alloc_i32();
                let dst_ptr = self.scratch.alloc_i32();
                let out_count = self.scratch.alloc_i32();
                let is_inline_lambda = matches!(&args[1].kind, almide_ir::IrExprKind::Lambda { .. });
                self.emit_expr(&args[0]);
                wasm!(self.func, { local_set(src); });
                if !is_inline_lambda {
                    self.emit_expr(&args[1]);
                    wasm!(self.func, { local_set(closure); });
                }
                if in_place {
                    // In-place: dst = src (compact matching elements to front)
                    wasm!(self.func, {
                        i32_const(0); local_set(out_count);
                        local_get(src); local_set(dst);
                        local_get(src); i32_const(list_data_off); i32_add;
                        local_tee(src_ptr);
                        local_get(src); i32_load(0); i32_const(elem_size as i32); i32_mul;
                        i32_add; local_set(end_ptr);
                        local_get(src); i32_const(list_data_off); i32_add;
                        local_set(dst_ptr);
                        block_empty; loop_empty;
                    });
                } else {
                    wasm!(self.func, {
                        // Alloc max-size output
                        i32_const(list_hdr);
                        local_get(src); i32_load(0);
                        i32_const(elem_size as i32); i32_mul; i32_add;
                        call(self.emitter.rt.alloc); local_set(dst);
                        i32_const(0); local_set(out_count);
                        // src_ptr = src + DATA_OFFSET
                        local_get(src); i32_const(list_data_off); i32_add;
                        local_tee(src_ptr);
                        // end_ptr = src_ptr + len * elem_size
                        local_get(src); i32_load(0); i32_const(elem_size as i32); i32_mul;
                        i32_add; local_set(end_ptr);
                        // dst_ptr = dst + DATA_OFFSET
                        local_get(dst); i32_const(list_data_off); i32_add;
                        local_set(dst_ptr);
                        block_empty; loop_empty;
                    });
                }
                let depth_guard = self.depth_push_n(2);
                wasm!(self.func, {
                    local_get(src_ptr); local_get(end_ptr); i32_ge_u; br_if(1);
                });
                let filter_param_local;
                if let almide_ir::IrExprKind::Lambda { params, body, .. } = &args[1].kind {
                    let param_var = params.first().map(|(v, _)| *v);
                    let pvt = values::ty_to_valtype(&elem_ty).unwrap_or(ValType::I32);
                    filter_param_local = Some((self.scratch.alloc(pvt), pvt));
                    let pl = filter_param_local.unwrap().0;
                    wasm!(self.func, { local_get(src_ptr); });
                    self.emit_load_at(&elem_ty, 0);
                    wasm!(self.func, { local_set(pl); });
                    if let Some(vid) = param_var {
                        self.var_map.insert(vid.0, pl);
                    }
                    self.emit_expr(body);
                    if let Some(vid) = param_var {
                        self.var_map.remove(&vid.0);
                    }
                } else {
                    filter_param_local = None;
                    wasm!(self.func, {
                        local_get(closure); i32_load(4); // env
                        local_get(src_ptr);
                    });
                    self.emit_load_at(&elem_ty, 0);
                    wasm!(self.func, { local_get(closure); i32_load(0); }); // table_idx
                    self.emit_closure_call(&elem_ty, &Ty::Bool);
                }
                // Branchless: always write, conditionally advance dst_ptr
                let pred_result = self.scratch.alloc_i32();
                wasm!(self.func, {
                    local_set(pred_result);
                    local_get(dst_ptr);
                });
                if let Some((pl, _)) = filter_param_local {
                    wasm!(self.func, { local_get(pl); });
                } else {
                    wasm!(self.func, { local_get(src_ptr); });
                    self.emit_load_at(&elem_ty, 0);
                }
                self.emit_store_at(&elem_ty, 0);
                wasm!(self.func, {
                    // dst_ptr += pred_result * elem_size (branchless)
                    local_get(dst_ptr);
                    local_get(pred_result); i32_const(elem_size as i32); i32_mul;
                    i32_add; local_set(dst_ptr);
                    // out_count += pred_result
                    local_get(out_count); local_get(pred_result); i32_add; local_set(out_count);
                    // src_ptr += elem_size
                    local_get(src_ptr); i32_const(elem_size as i32); i32_add; local_set(src_ptr);
                    br(0);
                });
                self.scratch.free_i32(pred_result);
                self.depth_pop(depth_guard);
                wasm!(self.func, {
                    end; end;
                    local_get(dst); local_get(out_count); i32_store(0);
                    local_get(dst);
                });
                if let Some((pl, pvt)) = filter_param_local {
                    self.scratch.free(pl, pvt);
                }
                self.scratch.free_i32(out_count);
                self.scratch.free_i32(dst_ptr);
                self.scratch.free_i32(end_ptr);
                self.scratch.free_i32(src_ptr);
                self.scratch.free_i32(dst);
                self.scratch.free_i32(closure);
                self.scratch.free_i32(src);
            }
            "fold" => {
                // fold(list, init, fn(acc, elem) → acc)
                // Resolve types from closure Fn signature when available
                // Derive element type from the input list (most reliable source)
                let list_elem_ty = self.resolve_list_elem(&args[0], None);

                // Resolve elem_ty: use list element type, Fn param, or lambda param — first concrete wins
                let elem_ty = [
                    Some(list_elem_ty),
                    if let Ty::Fn { params, .. } = &args[2].ty { params.get(1).cloned() } else { None },
                    if let almide_ir::IrExprKind::Lambda { params: lp, .. } = &args[2].kind { lp.get(1).map(|(_, t)| t.clone()) } else { None },
                ].into_iter().flatten()
                    .find(|t| !t.is_unresolved())
                    .unwrap_or(Ty::Int);

                // Resolve acc type: use Fn return type or lambda body type, with TypeVar→concrete fallback
                let acc_ty_resolved = {
                    // Try closure Fn ret type
                    let fn_ret = if let Ty::Fn { ret, .. } = &args[2].ty { Some(*ret.clone()) } else { None };
                    // Try lambda body type
                    let body_ty = if let almide_ir::IrExprKind::Lambda { body, .. } = &args[2].kind { Some(body.ty.clone()) } else { None };
                    // Try init type
                    let init_ty = args[1].ty.clone();
                    // Pick first concrete (non-TypeVar/Unknown) type
                    [fn_ret, body_ty, Some(init_ty)].into_iter().flatten()
                        .find(|t| !t.is_unresolved())
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
                let is_inline_lambda = matches!(&args[2].kind, almide_ir::IrExprKind::Lambda { .. });

                // ── Stream Fusion: detect map/filter pipeline feeding into fold ──
                let pipeline = self.detect_pipeline(&args[0]);
                if !pipeline.is_empty() && is_inline_lambda {
                    // Fused pipeline: iterate source with pointer-based iteration
                    let (source_expr, stages) = self.extract_pipeline(&args[0]);
                    self.emit_expr(source_expr);
                    wasm!(self.func, { local_set(list_ptr); });
                    let source_elem_ty = self.resolve_list_elem(source_expr, None);
                    let source_elem_size = values::byte_size(&source_elem_ty);
                    self.emit_expr(&args[1]);
                    wasm!(self.func, { local_set(acc); });
                    // Pointer-based iteration: ptr and end instead of idx
                    let ptr = self.scratch.alloc_i32();
                    let end_ptr = self.scratch.alloc_i32();
                    wasm!(self.func, {
                        // ptr = list_ptr + DATA_OFFSET
                        local_get(list_ptr); i32_const(list_data_off); i32_add;
                        local_set(ptr);
                        // end = ptr + len * elem_size
                        local_get(ptr);
                        local_get(list_ptr); i32_load(0); i32_const(source_elem_size as i32); i32_mul;
                        i32_add; local_set(end_ptr);
                        block_empty; loop_empty;
                    });
                    let depth_guard = self.depth_push_n(2);
                    wasm!(self.func, {
                        local_get(ptr); local_get(end_ptr); i32_ge_u; br_if(1);
                    });
                    // Load source element via pointer
                    let mut cur_ty = source_elem_ty.clone();
                    let mut cur_vt = values::ty_to_valtype(&cur_ty).unwrap_or(ValType::I32);
                    let mut cur_local = self.scratch.alloc(cur_vt);
                    wasm!(self.func, { local_get(ptr); });
                    self.emit_load_at(&cur_ty, 0);
                    wasm!(self.func, { local_set(cur_local); });
                    // Apply each pipeline stage
                    let mut skip_label_depth = 0u32;
                    for stage in &stages {
                        match stage {
                            PipelineStage::Map(lambda) => {
                                if let almide_ir::IrExprKind::Lambda { params, body, .. } = &lambda.kind {
                                    if let Some((vid, _)) = params.first() {
                                        self.var_map.insert(vid.0, cur_local);
                                    }
                                    self.emit_expr(body);
                                    // Map may change the value type (e.g. Tuple → Float).
                                    // Re-alloc cur_local with the correct type.
                                    let new_ty = body.ty.clone();
                                    let new_vt = values::ty_to_valtype(&new_ty).unwrap_or(ValType::I32);
                                    if new_vt != cur_vt {
                                        let new_local = self.scratch.alloc(new_vt);
                                        wasm!(self.func, { local_set(new_local); });
                                        self.scratch.free(cur_local, cur_vt);
                                        cur_local = new_local;
                                        cur_vt = new_vt;
                                    } else {
                                        wasm!(self.func, { local_set(cur_local); });
                                    }
                                    if let Some((vid, _)) = params.first() {
                                        self.var_map.remove(&vid.0);
                                    }
                                    cur_ty = new_ty;
                                }
                            }
                            PipelineStage::Filter(lambda) => {
                                if let almide_ir::IrExprKind::Lambda { params, body, .. } = &lambda.kind {
                                    if let Some((vid, _)) = params.first() {
                                        self.var_map.insert(vid.0, cur_local);
                                    }
                                    self.emit_expr(body);
                                    if let Some((vid, _)) = params.first() {
                                        self.var_map.remove(&vid.0);
                                    }
                                    // If false, skip to next iteration (ptr += elem_size then br to loop)
                                    wasm!(self.func, {
                                        i32_eqz;
                                        if_empty;
                                          local_get(ptr); i32_const(source_elem_size as i32); i32_add; local_set(ptr);
                                          br(1); // br to loop_empty
                                        end;
                                    });
                                }
                            }
                        }
                    }
                    // Apply fold body with cur_local as element
                    if let almide_ir::IrExprKind::Lambda { params, body, .. } = &args[2].kind {
                        let acc_param = params.first().map(|(v, _)| *v);
                        let elem_param = params.get(1).map(|(v, _)| *v);
                        if let Some(vid) = acc_param { self.var_map.insert(vid.0, acc); }
                        if let Some(vid) = elem_param { self.var_map.insert(vid.0, cur_local); }
                        self.emit_expr(body);
                        wasm!(self.func, { local_set(acc); });
                        if let Some(vid) = acc_param { self.var_map.remove(&vid.0); }
                        if let Some(vid) = elem_param { self.var_map.remove(&vid.0); }
                    }
                    self.scratch.free(cur_local, cur_vt);
                    wasm!(self.func, {
                        local_get(ptr); i32_const(source_elem_size as i32); i32_add; local_set(ptr);
                        br(0);
                    });
                    self.depth_pop(depth_guard);
                    wasm!(self.func, { end; end; local_get(acc); });
                    self.scratch.free_i32(end_ptr);
                    self.scratch.free_i32(ptr);
                    self.scratch.free(acc, acc_vt);
                    self.scratch.free_i32(idx);
                    self.scratch.free_i32(len);
                    self.scratch.free_i32(closure);
                    self.scratch.free_i32(list_ptr);
                    return true;
                }

                // Pointer-based iteration for fold
                let ptr = self.scratch.alloc_i32();
                let end_ptr = self.scratch.alloc_i32();
                self.emit_expr(&args[0]);
                wasm!(self.func, { local_set(list_ptr); });
                self.emit_expr(&args[1]);
                wasm!(self.func, { local_set(acc); });
                if !is_inline_lambda {
                    self.emit_expr(&args[2]);
                    wasm!(self.func, { local_set(closure); });
                }
                wasm!(self.func, {
                    // ptr = list_ptr + DATA_OFFSET
                    local_get(list_ptr); i32_const(list_data_off); i32_add;
                    local_tee(ptr);
                    // end_ptr = ptr + len * elem_size
                    local_get(list_ptr); i32_load(0); i32_const(elem_size as i32); i32_mul;
                    i32_add; local_set(end_ptr);
                    block_empty; loop_empty;
                });
                let depth_guard = self.depth_push_n(2);
                wasm!(self.func, {
                    local_get(ptr); local_get(end_ptr); i32_ge_u; br_if(1);
                });
                if let almide_ir::IrExprKind::Lambda { params, body, .. } = &args[2].kind {
                    let acc_param = params.first().map(|(v, _)| *v);
                    let elem_param = params.get(1).map(|(v, _)| *v);
                    let elem_local = self.scratch.alloc(values::ty_to_valtype(&elem_ty).unwrap_or(ValType::I32));
                    wasm!(self.func, { local_get(ptr); });
                    self.emit_load_at(&elem_ty, 0);
                    wasm!(self.func, { local_set(elem_local); });
                    if let Some(vid) = acc_param {
                        self.var_map.insert(vid.0, acc);
                    }
                    if let Some(vid) = elem_param {
                        self.var_map.insert(vid.0, elem_local);
                    }
                    self.emit_expr(body);
                    if let Some(vid) = acc_param {
                        self.var_map.remove(&vid.0);
                    }
                    if let Some(vid) = elem_param {
                        self.var_map.remove(&vid.0);
                    }
                    self.scratch.free(elem_local, values::ty_to_valtype(&elem_ty).unwrap_or(ValType::I32));
                } else {
                    wasm!(self.func, {
                        local_get(closure); i32_load(4); // env
                        local_get(acc);
                        local_get(ptr);
                    });
                    self.emit_load_at(&elem_ty, 0);
                    wasm!(self.func, { local_get(closure); i32_load(0); }); // table_idx
                    {
                        let mut ct = vec![ValType::I32]; // env
                        if let Some(vt) = values::ty_to_valtype(&acc_ty_resolved) { ct.push(vt); }
                        if let Some(vt) = values::ty_to_valtype(&elem_ty) { ct.push(vt); }
                        self.emit_call_indirect(ct, values::ret_type(&acc_ty_resolved));
                    }
                }
                wasm!(self.func, {
                    local_set(acc);
                    local_get(ptr); i32_const(elem_size as i32); i32_add; local_set(ptr);
                    br(0);
                });
                self.depth_pop(depth_guard);
                wasm!(self.func, { end; end; local_get(acc); });
                self.scratch.free_i32(end_ptr);
                self.scratch.free_i32(ptr);
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

    // ── Stream Fusion helpers ──

    /// Detect fusible pipeline stages in a list expression.
    /// Returns non-empty vec if the expr is a chain of list.map/filter calls.
    fn detect_pipeline(&self, expr: &IrExpr) -> Vec<&str> {
        let mut stages = Vec::new();
        let mut cur = expr;
        loop {
            if let Some((op, fn_arg, source)) = self.match_list_pipeline_stage(cur) {
                if !matches!(&fn_arg.kind, IrExprKind::Lambda { .. }) {
                    break;
                }
                stages.push(op);
                cur = source;
            } else {
                break;
            }
        }
        stages
    }

    fn extract_pipeline<'b>(&self, expr: &'b IrExpr) -> (&'b IrExpr, Vec<PipelineStage<'b>>) {
        let mut stages = Vec::new();
        let mut cur = expr;
        loop {
            if let Some((op, fn_arg, source)) = self.match_list_pipeline_stage(cur) {
                if !matches!(&fn_arg.kind, IrExprKind::Lambda { .. }) {
                    break;
                }
                match op {
                    "map" => stages.push(PipelineStage::Map(fn_arg)),
                    "filter" => stages.push(PipelineStage::Filter(fn_arg)),
                    _ => break,
                }
                cur = source;
            } else {
                break;
            }
        }
        stages.reverse();
        (cur, stages)
    }

    /// Match a list.map or list.filter call, handling both Module and RuntimeCall forms.
    /// Returns (op_name, fn_arg, source_list) if matched.
    fn match_list_pipeline_stage<'b>(&self, expr: &'b IrExpr) -> Option<(&'static str, &'b IrExpr, &'b IrExpr)> {
        match &expr.kind {
            IrExprKind::Call { target: almide_ir::CallTarget::Module { module, func, .. }, args, .. }
                if module.as_str() == "list" && args.len() >= 2 =>
            {
                match func.as_str() {
                    "map" => Some(("map", &args[1], &args[0])),
                    "filter" => Some(("filter", &args[1], &args[0])),
                    _ => None,
                }
            }
            IrExprKind::RuntimeCall { symbol, args } if args.len() >= 2 => {
                let s = symbol.as_str();
                if s.contains("list_map") {
                    Some(("map", &args[1], &args[0]))
                } else if s.contains("list_filter") {
                    Some(("filter", &args[1], &args[0]))
                } else {
                    None
                }
            }
            _ => None,
        }
    }

    /// Emit list.map(list, fn) → new list with fn applied to each element.
    /// Uses scratch locals (not mem[]) to survive nested calls from call_indirect.
    /// Key insight: compute dst address BEFORE call_indirect so result goes
    /// directly onto the stack in the right position for store.
    pub(super) fn emit_list_map(&mut self, list_arg: &IrExpr, fn_arg: &IrExpr, ret_ty: &Ty) {
        use super::engine::layout::{LIST, list as ll};
        let list_data_off = self.emitter.layout_reg.fixed_offset(LIST, ll::DATA) as i32;
        let list_hdr = self.emitter.layout_reg.header_size(LIST) as i32;
        let in_elem_ty = self.resolve_list_elem(list_arg, Some(fn_arg));
        let mut out_elem_ty = if let Ty::Applied(_, args) = ret_ty {
            args.first().cloned().unwrap_or(Ty::Int)
        } else { Ty::Int };
        // When the return type is unresolved (TypeVar/Unknown), derive the output
        // element type from the lambda body.  The call-site ret_ty can remain
        // unresolved when the map result is unused or only used in a type-agnostic
        // context, but the lambda body always carries the concrete type.
        if out_elem_ty.is_unresolved() {
            if let IrExprKind::Lambda { body, .. } = &fn_arg.kind {
                if !body.ty.is_unresolved() {
                    out_elem_ty = body.ty.clone();
                }
            }
            // Also try fn_arg.ty.ret as a secondary source
            if out_elem_ty.is_unresolved() {
                if let Ty::Fn { ret, .. } = &fn_arg.ty {
                    if !ret.is_unresolved() {
                        out_elem_ty = *ret.clone();
                    }
                }
            }
        }
        // Final fallback: inspect the lifted closure's actual registered WASM
        // param/ret valtypes. This handles the case where inference left both
        // the list element and lambda body as Unknown but the lifted function
        // has a concrete signature (e.g. from our closure-conversion VarTable
        // propagation + anonymous record fallback).
        let in_vt = values::ty_to_valtype(&in_elem_ty);
        let in_elem_ty = match self.resolve_closure_param_valtype(fn_arg, 0) {
            Some(actual) if Some(actual) != in_vt => values::vt_to_placeholder_ty(actual),
            _ => in_elem_ty,
        };
        let out_vt = values::ty_to_valtype(&out_elem_ty);
        let out_elem_ty = match self.resolve_closure_ret_valtype(fn_arg) {
            Some(actual) if Some(actual) != out_vt => values::vt_to_placeholder_ty(actual),
            _ => out_elem_ty,
        };
        let in_size = values::byte_size(&in_elem_ty);
        let out_size = values::byte_size(&out_elem_ty);

        // SIMD detection: Int→Int with simple arithmetic lambda
        let simd_op = if matches!(&in_elem_ty, Ty::Int) && matches!(&out_elem_ty, Ty::Int) {
            Self::detect_simd_map_op(fn_arg)
        } else { None };

        let src_local = self.scratch.alloc_i32();
        let closure_local = self.scratch.alloc_i32();
        let dst_local = self.scratch.alloc_i32();
        let src_ptr = self.scratch.alloc_i32();
        let dst_ptr = self.scratch.alloc_i32();
        let end_ptr = self.scratch.alloc_i32();

        let direct_fn = self.try_resolve_direct_call(fn_arg);

        // Perceus in-place reuse: if source is single-use AND element sizes match,
        // skip allocation and write mapped results directly into the source list.
        let in_place = self.is_single_use_var(list_arg) && in_size == out_size;

        self.emit_expr(list_arg);
        wasm!(self.func, { local_set(src_local); });
        if direct_fn.is_none() && !matches!(&fn_arg.kind, almide_ir::IrExprKind::Lambda { .. }) {
            self.emit_expr(fn_arg);
            wasm!(self.func, { local_set(closure_local); });
        }
        let len_local = self.scratch.alloc_i32();
        if in_place {
            // In-place: dst = src (no allocation)
            wasm!(self.func, {
                local_get(src_local); i32_load(0); local_set(len_local);
                local_get(src_local); local_set(dst_local);
                local_get(src_local); i32_const(list_data_off); i32_add; local_set(src_ptr);
                local_get(src_local); i32_const(list_data_off); i32_add; local_set(dst_ptr);
                local_get(src_ptr); local_get(len_local); i32_const(in_size as i32); i32_mul; i32_add; local_set(end_ptr);
            });
        } else {
            wasm!(self.func, {
                local_get(src_local); i32_load(0); local_set(len_local);
                // alloc dst
                i32_const(list_hdr);
                local_get(len_local); i32_const(out_size as i32); i32_mul; i32_add;
                call(self.emitter.rt.alloc); local_set(dst_local);
                local_get(dst_local); local_get(len_local); i32_store(0);
                // Pointer-based iteration
                local_get(src_local); i32_const(list_data_off); i32_add; local_set(src_ptr);
                local_get(dst_local); i32_const(list_data_off); i32_add; local_set(dst_ptr);
                local_get(src_ptr); local_get(len_local); i32_const(in_size as i32); i32_mul; i32_add; local_set(end_ptr);
            });
        }

        // SIMD fast path: process 8 i64 elements per iteration (4× v128 unrolled)
        if let Some((simd_kind, simd_const_val)) = simd_op {
            let simd_end = self.scratch.alloc_i32();
            let use_shift = matches!(simd_kind, SimdMapOp::Mul)
                && simd_const_val > 0
                && (simd_const_val as u64).is_power_of_two();
            let simd_vec = if !use_shift { Some(self.scratch.alloc_v128()) } else { None };
            // simd_end = src_ptr + (len / 8) * 64  (round down to multiple of 8)
            wasm!(self.func, {
                local_get(src_ptr);
                local_get(len_local); i32_const(3); i32_shr_u; // len / 8
                i32_const(64); i32_mul;
                i32_add; local_set(simd_end);
            });
            if let Some(sv) = simd_vec {
                wasm!(self.func, {
                    i64_const(simd_const_val);
                    i64x2_splat;
                    local_set(sv);
                });
            }

            // Emit a single SIMD operation: load from src_ptr+offset, apply op, store to dst_ptr+offset
            let emit_simd_op = |fc: &mut FuncCompiler, offset: u64, sv: Option<u32>| {
                wasm!(fc.func, {
                    local_get(dst_ptr);
                    local_get(src_ptr);
                    v128_load(offset);
                });
                if use_shift {
                    let shift = (simd_const_val as u64).trailing_zeros();
                    wasm!(fc.func, { i32_const(shift as i32); });
                    fc.func.instruction(&wasm_encoder::Instruction::I64x2Shl);
                } else {
                    let sv = sv.unwrap();
                    wasm!(fc.func, { local_get(sv); });
                    match simd_kind {
                        SimdMapOp::Mul => { wasm!(fc.func, { i64x2_mul; }); }
                        SimdMapOp::Add => { wasm!(fc.func, { i64x2_add; }); }
                        SimdMapOp::Sub => { wasm!(fc.func, { i64x2_sub; }); }
                    }
                }
                wasm!(fc.func, { v128_store(offset); });
            };

            wasm!(self.func, {
                block_empty; loop_empty;
                  local_get(src_ptr); local_get(simd_end); i32_ge_u; br_if(1);
            });
            // 4× unrolled: process 8 elements (64 bytes) per iteration
            emit_simd_op(self, 0, simd_vec);
            emit_simd_op(self, 16, simd_vec);
            emit_simd_op(self, 32, simd_vec);
            emit_simd_op(self, 48, simd_vec);
            wasm!(self.func, {
                  local_get(src_ptr); i32_const(64); i32_add; local_set(src_ptr);
                  local_get(dst_ptr); i32_const(64); i32_add; local_set(dst_ptr);
                  br(0);
                end; end;
            });
            if let Some(sv) = simd_vec { self.scratch.free_v128(sv); }
            self.scratch.free_i32(simd_end);
        }

        // Scalar loop (handles tail after SIMD, or all elements if no SIMD)
        wasm!(self.func, {
            block_empty; loop_empty;
        });
        let depth_guard = self.depth_push_n(2);

        wasm!(self.func, {
            local_get(src_ptr); local_get(end_ptr); i32_ge_u; br_if(1);
            // dst addr on stack
            local_get(dst_ptr);
        });
        if let almide_ir::IrExprKind::Lambda { params, body, .. } = &fn_arg.kind {
            let param_var = params.first().map(|(v, _)| *v);
            let param_local = self.scratch.alloc(values::ty_to_valtype(&in_elem_ty).unwrap_or(ValType::I32));
            wasm!(self.func, { local_get(src_ptr); });
            self.emit_load_at(&in_elem_ty, 0);
            wasm!(self.func, { local_set(param_local); });
            // Bind param var to local
            if let Some(vid) = param_var {
                self.var_map.insert(vid.0, param_local);
            }
            self.emit_expr(body);
            // Clean up
            if let Some(vid) = param_var {
                self.var_map.remove(&vid.0);
            }
            self.scratch.free(param_local, values::ty_to_valtype(&in_elem_ty).unwrap_or(ValType::I32));
        } else if let Some(fn_idx) = direct_fn {
            wasm!(self.func, {
                i32_const(0);
                local_get(src_ptr);
            });
            self.emit_load_at(&in_elem_ty, 0);
            wasm!(self.func, { call(fn_idx); });
        } else {
            wasm!(self.func, {
                local_get(closure_local); i32_load(4);
                local_get(src_ptr);
            });
            self.emit_load_at(&in_elem_ty, 0);
            wasm!(self.func, { local_get(closure_local); i32_load(0); });
            self.emit_closure_call(&in_elem_ty, &out_elem_ty);
        }
        self.emit_store_at(&out_elem_ty, 0);

        wasm!(self.func, {
            local_get(src_ptr); i32_const(in_size as i32); i32_add; local_set(src_ptr);
            local_get(dst_ptr); i32_const(out_size as i32); i32_add; local_set(dst_ptr);
            br(0);
        });
        self.depth_pop(depth_guard);
        wasm!(self.func, { end; end; });

        // Perceus: if source was single-use and NOT in-place, free via rc_dec.
        if !in_place && self.is_single_use_var(list_arg) {
            wasm!(self.func, { local_get(src_local); call(self.emitter.rt.rc_dec); });
        }

        wasm!(self.func, { local_get(dst_local); });

        self.scratch.free_i32(len_local);
        self.scratch.free_i32(end_ptr);
        self.scratch.free_i32(dst_ptr);
        self.scratch.free_i32(src_ptr);
        self.scratch.free_i32(dst_local);
        self.scratch.free_i32(closure_local);
        self.scratch.free_i32(src_local);
    }

    /// Check if a list expression is consumed exactly once (safe for in-place reuse).
    /// True for: single-use variables (use_count == 1) OR temporary expressions
    /// (Call results, RuntimeCall results) that are not bound to any variable.
    fn is_single_use_var(&self, expr: &IrExpr) -> bool {
        match &expr.kind {
            IrExprKind::Var { id } => self.var_table.get(*id).use_count == 1,
            // Temporary expression results: consumed exactly here, never aliased
            IrExprKind::Call { .. }
            | IrExprKind::TailCall { .. }
            | IrExprKind::RuntimeCall { .. } => true,
            _ => false,
        }
    }

    fn detect_simd_map_op(fn_arg: &IrExpr) -> Option<(SimdMapOp, i64)> {
        if let IrExprKind::Lambda { params, body, .. } = &fn_arg.kind {
            let param_id = params.first().map(|(v, _)| v.0)?;
            match &body.kind {
                IrExprKind::BinOp { op, left, right } => {
                    let (var_side, lit_side, commutative) = match op {
                        BinOp::MulInt => (true, true, true),
                        BinOp::AddInt => (true, true, true),
                        BinOp::SubInt => (true, true, false), // x - k only
                        _ => return None,
                    };
                    let _ = (var_side, lit_side, commutative);
                    let simd_kind = match op {
                        BinOp::MulInt => SimdMapOp::Mul,
                        BinOp::AddInt => SimdMapOp::Add,
                        BinOp::SubInt => SimdMapOp::Sub,
                        _ => unreachable!(),
                    };
                    // x op k
                    if let (IrExprKind::Var { id }, IrExprKind::LitInt { value: k }) = (&left.kind, &right.kind) {
                        if id.0 == param_id { return Some((simd_kind, *k)); }
                    }
                    // k op x (commutative only)
                    if matches!(op, BinOp::MulInt | BinOp::AddInt) {
                        if let (IrExprKind::LitInt { value: k }, IrExprKind::Var { id }) = (&left.kind, &right.kind) {
                            if id.0 == param_id { return Some((simd_kind, *k)); }
                        }
                    }
                    None
                }
                _ => None,
            }
        } else {
            None
        }
    }
}
