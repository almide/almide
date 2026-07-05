// List stdlib closure-based call dispatch for WASM codegen (part 2, group 2).
//
// Sub-dispatch group for `emit_list_closure_call2`: update, scan, zip_with,
// unique_by. Included into `calls_list_closure2.rs` via `include!`; shares its
// module imports and the `FuncCompiler` impl. Arm patterns are DISJOINT from
// the other groups so chain order is irrelevant.

impl FuncCompiler<'_> {
    /// `emit_list_closure_call2` group 2. Returns true if handled.
    fn emit_list_closure_call2_g2(&mut self, method: &str, args: &[IrExpr]) -> bool {
        use super::engine::layout::{LIST, list as ll};
        let list_data_off = self.emitter.layout_reg.fixed_offset(LIST, ll::DATA) as i32;
        let list_hdr = self.emitter.layout_reg.header_size(LIST) as i32;
        match method {
            "update" => {
                // update(xs, i, f) → List[A]: copy with xs[i] replaced by f(xs[i])
                let elem_ty = self.resolve_list_elem(&args[0], None);
                let es = values::byte_size(&elem_ty) as i32;
                let xs = self.scratch.alloc_i32();
                let idx = self.scratch.alloc_i32();
                let in_bounds = self.scratch.alloc_i32();
                let closure = self.scratch.alloc_i32();
                let len = self.scratch.alloc_i32();
                let dst = self.scratch.alloc_i32();
                let copy_i = self.scratch.alloc_i32();
                self.emit_expr(&args[0]);
                wasm!(self.func, { local_set(xs); local_get(xs); i32_load(0); local_set(len); });
                self.emit_expr(&args[1]);
                // idx = min_u(i, len); in_bounds = (i_u < len) on the full i64.
                // Native `list.update` is a no-op when i is OOB AND does NOT
                // call `f` (the `if let Some` body is skipped); a huge/negative
                // i64 must take that path, not wrap to a small in-range slot
                // and run f on the wrong element (C-054).
                self.emit_checked_index_i32(len, in_bounds);
                wasm!(self.func, { local_set(idx); });
                self.emit_expr(&args[2]);
                wasm!(self.func, {
                    local_set(closure);
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
                self.emit_elem_copy_owned(&elem_ty);
                wasm!(self.func, {
                      local_get(copy_i); i32_const(1); i32_add; local_set(copy_i);
                      br(0);
                    end; end;
                });
                // Replace dst[idx] with f(dst[idx]) — only when in bounds.
                // Native skips both the write AND the f-call on OOB.
                wasm!(self.func, {
                    local_get(in_bounds);
                    if_empty;
                });
                // Release the owned copy of the element being replaced (it
                // received +1 in the copy loop and loses this slot). The
                // closure still reads it first — dec AFTER the call would be
                // ideal, but the call's argument is the same pointer and a
                // rc>1 dec never frees; at rc==1 the closure-arg read happens
                // before any reuse since nothing allocates in between.
                if crate::pass_perceus::is_heap_type(&elem_ty) {
                    wasm!(self.func, {
                        local_get(dst); i32_const(list_data_off); i32_add;
                        local_get(idx); i32_const(es); i32_mul; i32_add;
                        i32_load(0); call(self.emitter.rt.rc_dec);
                    });
                }
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
                wasm!(self.func, { end; local_get(dst); });
                self.scratch.free_i32(copy_i);
                self.scratch.free_i32(dst);
                self.scratch.free_i32(len);
                self.scratch.free_i32(closure);
                self.scratch.free_i32(in_bounds);
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
                self.emit_elem_copy_owned(&elem_ty);
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
            _ => return false,
        }
        true
    }
}
