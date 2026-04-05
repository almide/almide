//! List stdlib closure-based call dispatch for WASM codegen.
//!
//! Functions that take closures as arguments: find, find_index, any, all, each,
//! reduce, flat_map, filter_map, sort_by, take_while, drop_while, count,
//! partition, update, scan, zip_with, unique_by.

use super::FuncCompiler;
use super::values;
use almide_ir::IrExpr;
use almide_lang::types::Ty;
use wasm_encoder::ValType;

impl FuncCompiler<'_> {
    /// Dispatch a list stdlib closure-based call. Returns true if handled.
    pub(super) fn emit_list_closure_call(&mut self, method: &str, args: &[IrExpr]) -> bool {
        match method {
            "find" => {
                // find(xs, pred) → Option[A]: first element where pred(x) is true
                let elem_ty = self.list_elem_ty(&args[0].ty);
                let es = values::byte_size(&elem_ty) as i32;
                let xs = self.scratch.alloc_i32();
                let closure = self.scratch.alloc_i32();
                let i = self.scratch.alloc_i32();
                let tmp = self.scratch.alloc_i32();
                let result = self.scratch.alloc_i32();
                self.emit_expr(&args[0]);
                wasm!(self.func, { local_set(xs); });
                self.emit_expr(&args[1]);
                wasm!(self.func, {
                    local_set(closure);
                    i32_const(0); local_set(i); // i=0
                    i32_const(0); local_set(result); // result (default: none)
                    block_empty; loop_empty;
                      local_get(i); local_get(xs); i32_load(0); i32_ge_u; br_if(1);
                      // Call pred(xs[i])
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
                        // Found: alloc some(xs[i])
                        i32_const(es); call(self.emitter.rt.alloc); local_set(tmp);
                        local_get(tmp);
                        local_get(xs); i32_const(4); i32_add;
                        local_get(i); i32_const(es); i32_mul; i32_add;
                });
                self.emit_elem_copy(&elem_ty);
                wasm!(self.func, {
                        local_get(tmp); local_set(result); br(2);
                      end;
                      local_get(i); i32_const(1); i32_add; local_set(i);
                      br(0);
                    end; end;
                    local_get(result); // result (none if not found)
                });
                self.scratch.free_i32(result);
                self.scratch.free_i32(tmp);
                self.scratch.free_i32(i);
                self.scratch.free_i32(closure);
                self.scratch.free_i32(xs);
            }
            "find_index" if args.len() == 2 && matches!(&args[1].ty, Ty::Fn { .. }) => {
                // find_index(xs, pred) → Option[Int]
                let elem_ty = self.list_elem_ty(&args[0].ty);
                let es = values::byte_size(&elem_ty) as i32;
                let xs = self.scratch.alloc_i32();
                let closure = self.scratch.alloc_i32();
                let i = self.scratch.alloc_i32();
                let tmp = self.scratch.alloc_i32();
                let result = self.scratch.alloc_i32();
                self.emit_expr(&args[0]);
                wasm!(self.func, { local_set(xs); });
                self.emit_expr(&args[1]);
                wasm!(self.func, {
                    local_set(closure);
                    i32_const(0); local_set(i);
                    i32_const(0); local_set(result); // result (default: none)
                    block_empty; loop_empty;
                      local_get(i); local_get(xs); i32_load(0); i32_ge_u; br_if(1);
                      local_get(closure); i32_load(4); // env
                      local_get(xs); i32_const(4); i32_add;
                      local_get(i); i32_const(es); i32_mul; i32_add;
                });
                self.emit_load_at(&elem_ty, 0);
                wasm!(self.func, { local_get(closure); i32_load(0); });
                self.emit_closure_call(&elem_ty, &Ty::Bool);
                wasm!(self.func, {
                      if_empty;
                        i32_const(8); call(self.emitter.rt.alloc); local_set(tmp);
                        local_get(tmp); local_get(i); i64_extend_i32_u; i64_store(0);
                        local_get(tmp); local_set(result); br(2);
                      end;
                      local_get(i); i32_const(1); i32_add; local_set(i);
                      br(0);
                    end; end;
                    local_get(result); // result (none if not found)
                });
                self.scratch.free_i32(result);
                self.scratch.free_i32(tmp);
                self.scratch.free_i32(i);
                self.scratch.free_i32(closure);
                self.scratch.free_i32(xs);
            }
            "any" => {
                // any(xs, pred) → Bool
                let elem_ty = self.list_elem_ty(&args[0].ty);
                let es = values::byte_size(&elem_ty) as i32;
                let xs = self.scratch.alloc_i32();
                let closure = self.scratch.alloc_i32();
                let i = self.scratch.alloc_i32();
                let result = self.scratch.alloc_i32();
                self.emit_expr(&args[0]);
                wasm!(self.func, { local_set(xs); });
                self.emit_expr(&args[1]);
                wasm!(self.func, {
                    local_set(closure);
                    i32_const(0); local_set(i);
                    i32_const(0); local_set(result); // result (default: false)
                    block_empty; loop_empty;
                      local_get(i); local_get(xs); i32_load(0); i32_ge_u; br_if(1);
                      local_get(closure); i32_load(4);
                      local_get(xs); i32_const(4); i32_add;
                      local_get(i); i32_const(es); i32_mul; i32_add;
                });
                self.emit_load_at(&elem_ty, 0);
                wasm!(self.func, { local_get(closure); i32_load(0); });
                self.emit_closure_call(&elem_ty, &Ty::Bool);
                wasm!(self.func, {
                      if_empty; i32_const(1); local_set(result); br(2); end;
                      local_get(i); i32_const(1); i32_add; local_set(i);
                      br(0);
                    end; end;
                    local_get(result); // result
                });
                self.scratch.free_i32(result);
                self.scratch.free_i32(i);
                self.scratch.free_i32(closure);
                self.scratch.free_i32(xs);
            }
            "all" => {
                let elem_ty = self.list_elem_ty(&args[0].ty);
                let es = values::byte_size(&elem_ty) as i32;
                let xs = self.scratch.alloc_i32();
                let closure = self.scratch.alloc_i32();
                let i = self.scratch.alloc_i32();
                let result = self.scratch.alloc_i32();
                self.emit_expr(&args[0]);
                wasm!(self.func, { local_set(xs); });
                self.emit_expr(&args[1]);
                wasm!(self.func, {
                    local_set(closure);
                    i32_const(0); local_set(i);
                    i32_const(1); local_set(result); // result (default: true)
                    block_empty; loop_empty;
                      local_get(i); local_get(xs); i32_load(0); i32_ge_u; br_if(1);
                      local_get(closure); i32_load(4);
                      local_get(xs); i32_const(4); i32_add;
                      local_get(i); i32_const(es); i32_mul; i32_add;
                });
                self.emit_load_at(&elem_ty, 0);
                wasm!(self.func, { local_get(closure); i32_load(0); });
                self.emit_closure_call(&elem_ty, &Ty::Bool);
                wasm!(self.func, {
                      i32_eqz;
                      if_empty; i32_const(0); local_set(result); br(2); end;
                      local_get(i); i32_const(1); i32_add; local_set(i);
                      br(0);
                    end; end;
                    local_get(result); // result
                });
                self.scratch.free_i32(result);
                self.scratch.free_i32(i);
                self.scratch.free_i32(closure);
                self.scratch.free_i32(xs);
            }
            "each" => {
                let elem_ty = self.list_elem_ty(&args[0].ty);
                let es = values::byte_size(&elem_ty) as i32;
                let xs = self.scratch.alloc_i32();
                let closure = self.scratch.alloc_i32();
                let i = self.scratch.alloc_i32();
                self.emit_expr(&args[0]);
                wasm!(self.func, { local_set(xs); });
                self.emit_expr(&args[1]);
                wasm!(self.func, {
                    local_set(closure);
                    i32_const(0); local_set(i);
                    block_empty; loop_empty;
                      local_get(i); local_get(xs); i32_load(0); i32_ge_u; br_if(1);
                      local_get(closure); i32_load(4);
                      local_get(xs); i32_const(4); i32_add;
                      local_get(i); i32_const(es); i32_mul; i32_add;
                });
                self.emit_load_at(&elem_ty, 0);
                wasm!(self.func, { local_get(closure); i32_load(0); });
                self.emit_closure_call(&elem_ty, &Ty::Unit);
                wasm!(self.func, {
                      local_get(i); i32_const(1); i32_add; local_set(i);
                      br(0);
                    end; end;
                });
                self.scratch.free_i32(i);
                self.scratch.free_i32(closure);
                self.scratch.free_i32(xs);
            }
            "take_end" => {
                // take_end(xs, n) = drop(xs, max(0, len-n))
                let elem_ty = self.list_elem_ty(&args[0].ty);
                let es = values::byte_size(&elem_ty) as i32;
                let xs = self.scratch.alloc_i32();
                let n = self.scratch.alloc_i32();
                let start = self.scratch.alloc_i32();
                let new_len = self.scratch.alloc_i32();
                let dst = self.scratch.alloc_i32();
                let i = self.scratch.alloc_i32();
                self.emit_expr(&args[0]);
                wasm!(self.func, { local_set(xs); });
                self.emit_expr(&args[1]);
                wasm!(self.func, {
                    i32_wrap_i64; local_set(n);
                    // start = max(0, len - n)
                    local_get(xs); i32_load(0); local_get(n); i32_sub;
                    local_set(start);
                    local_get(start); i32_const(0); i32_lt_s;
                    if_empty; i32_const(0); local_set(start); end;
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
                self.scratch.free_i32(n);
                self.scratch.free_i32(xs);
            }
            "drop_end" => {
                // drop_end(xs, n) = take(xs, max(0, len-n))
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
                    local_get(xs); i32_load(0); local_get(n); i32_sub;
                    local_set(new_len); // new_len
                    local_get(new_len); i32_const(0); i32_lt_s;
                    if_empty; i32_const(0); local_set(new_len); end;
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
            "repeat" => {
                // repeat(val, n) → List[A] — args[0] IS the element, not a list
                let elem_ty = args[0].ty.clone();
                let es = values::byte_size(&elem_ty) as i32;
                let val_vt = values::ty_to_valtype(&elem_ty).unwrap_or(ValType::I32);
                let val = self.scratch.alloc(val_vt);
                let n = self.scratch.alloc_i32();
                let dst = self.scratch.alloc_i32();
                let i = self.scratch.alloc_i32();
                self.emit_expr(&args[0]); // val
                wasm!(self.func, { local_set(val); });
                self.emit_expr(&args[1]); // n
                wasm!(self.func, {
                    i32_wrap_i64; local_set(n);
                    i32_const(4); local_get(n); i32_const(es); i32_mul; i32_add;
                    call(self.emitter.rt.alloc); local_set(dst);
                    local_get(dst); local_get(n); i32_store(0);
                    i32_const(0); local_set(i);
                    block_empty; loop_empty;
                      local_get(i); local_get(n); i32_ge_u; br_if(1);
                      local_get(dst); i32_const(4); i32_add;
                      local_get(i); i32_const(es); i32_mul; i32_add;
                      local_get(val);
                });
                self.emit_store_at(&elem_ty, 0);
                wasm!(self.func, {
                      local_get(i); i32_const(1); i32_add; local_set(i);
                      br(0);
                    end; end;
                    local_get(dst);
                });
                self.scratch.free_i32(i);
                self.scratch.free_i32(dst);
                self.scratch.free_i32(n);
                self.scratch.free(val, val_vt);
            }
            "reduce" => {
                // reduce(xs, f) → Option[A]: fold starting from xs[0]
                let elem_ty = self.list_elem_ty(&args[0].ty);
                let es = values::byte_size(&elem_ty) as i32;
                let acc_vt = values::ty_to_valtype(&elem_ty).unwrap_or(ValType::I64);
                let xs = self.scratch.alloc_i32();
                let closure = self.scratch.alloc_i32();
                let i = self.scratch.alloc_i32();
                let acc = self.scratch.alloc(acc_vt);
                let tmp = self.scratch.alloc_i32();
                self.emit_expr(&args[0]);
                wasm!(self.func, { local_set(xs); });
                self.emit_expr(&args[1]); // fn(a, b) -> a
                wasm!(self.func, {
                    local_set(closure);
                    local_get(xs); i32_load(0); i32_eqz;
                    if_i32; i32_const(0); // empty → none
                    else_;
                      // acc = xs[0]
                      local_get(xs); i32_const(4); i32_add;
                });
                self.emit_load_at(&elem_ty, 0);
                wasm!(self.func, { local_set(acc); });
                wasm!(self.func, {
                      i32_const(1); local_set(i); // i = 1
                      block_empty; loop_empty;
                        local_get(i); local_get(xs); i32_load(0); i32_ge_u; br_if(1);
                        // Call f(acc, xs[i])
                        local_get(closure); i32_load(4); // env
                        local_get(acc); // acc
                        local_get(xs); i32_const(4); i32_add;
                        local_get(i); i32_const(es); i32_mul; i32_add;
                });
                self.emit_load_at(&elem_ty, 0);
                wasm!(self.func, {
                        local_get(closure); i32_load(0); // table_idx
                });
                // call_indirect (env, a, b) → a
                {
                    let mut ct = vec![ValType::I32]; // env
                    if let Some(vt) = values::ty_to_valtype(&elem_ty) { ct.push(vt); ct.push(vt); }
                    self.emit_call_indirect(ct, values::ret_type(&elem_ty));
                }
                wasm!(self.func, {
                        local_set(acc); // update acc
                        local_get(i); i32_const(1); i32_add; local_set(i);
                        br(0);
                      end; end;
                      // Wrap acc in some
                      i32_const(es); call(self.emitter.rt.alloc); local_set(tmp);
                      local_get(tmp); local_get(acc);
                });
                self.emit_store_at(&elem_ty, 0);
                wasm!(self.func, { local_get(tmp); end; });
                self.scratch.free_i32(tmp);
                self.scratch.free(acc, acc_vt);
                self.scratch.free_i32(i);
                self.scratch.free_i32(closure);
                self.scratch.free_i32(xs);
            }
            "flat_map" => {
                // flat_map(xs, f) → List[B]: f returns List[B], flatten results
                let elem_ty = self.list_elem_ty(&args[0].ty);
                // Output element type B: infer from fn return type List[B]
                let out_elem_ty = if let Ty::Fn { ret, .. } = &args[1].ty {
                    self.list_elem_ty(ret) // List[B] → B
                } else { elem_ty.clone() };
                let out_es = values::byte_size(&out_elem_ty) as i32;
                let es = values::byte_size(&elem_ty) as i32;
                let xs = self.scratch.alloc_i32();
                let closure = self.scratch.alloc_i32();
                let len = self.scratch.alloc_i32();
                let lol = self.scratch.alloc_i32(); // list-of-lists
                let i = self.scratch.alloc_i32();
                let total = self.scratch.alloc_i32();
                let result = self.scratch.alloc_i32();
                let inner = self.scratch.alloc_i32();
                let j = self.scratch.alloc_i32();
                self.emit_expr(&args[0]);
                wasm!(self.func, { local_set(xs); });
                self.emit_expr(&args[1]);
                wasm!(self.func, {
                    local_set(closure);
                    local_get(xs); i32_load(0); local_set(len);
                    // Alloc temp list-of-lists: [len][ptr0][ptr1]...
                    i32_const(4); local_get(len); i32_const(4); i32_mul; i32_add;
                    call(self.emitter.rt.alloc); local_set(lol);
                    local_get(lol); local_get(len); i32_store(0);
                    i32_const(0); local_set(i);
                    block_empty; loop_empty;
                      local_get(i); local_get(len); i32_ge_u; br_if(1);
                      // Call f(xs[i]) → List[B]
                      local_get(lol); i32_const(4); i32_add;
                      local_get(i); i32_const(4); i32_mul; i32_add; // dst addr for result ptr
                      local_get(closure); i32_load(4); // env
                      local_get(xs); i32_const(4); i32_add;
                      local_get(i); i32_const(es); i32_mul; i32_add;
                });
                self.emit_load_at(&elem_ty, 0);
                wasm!(self.func, {
                      local_get(closure); i32_load(0); // table_idx
                });
                self.emit_closure_call(&elem_ty, &Ty::Unknown); // returns List ptr (i32)
                wasm!(self.func, {
                      i32_store(0); // store result list ptr
                      local_get(i); i32_const(1); i32_add; local_set(i);
                      br(0);
                    end; end;
                });
                // Flatten: count total elements
                wasm!(self.func, {
                    i32_const(0); local_set(total);
                    i32_const(0); local_set(i);
                    block_empty; loop_empty;
                      local_get(i); local_get(lol); i32_load(0); i32_ge_u; br_if(1);
                      local_get(total);
                      local_get(lol); i32_const(4); i32_add;
                      local_get(i); i32_const(4); i32_mul; i32_add;
                      i32_load(0); i32_load(0);
                      i32_add; local_set(total);
                      local_get(i); i32_const(1); i32_add; local_set(i);
                      br(0);
                    end; end;
                    // Alloc result
                    i32_const(4); local_get(total); i32_const(out_es); i32_mul; i32_add;
                    call(self.emitter.rt.alloc); local_set(result);
                    local_get(result); local_get(total); i32_store(0);
                });
                // Copy all sub-list elements
                wasm!(self.func, {
                    i32_const(0); local_set(total); // reuse as dst_offset
                    i32_const(0); local_set(i);
                    block_empty; loop_empty;
                      local_get(i); local_get(lol); i32_load(0); i32_ge_u; br_if(1);
                      local_get(lol); i32_const(4); i32_add;
                      local_get(i); i32_const(4); i32_mul; i32_add;
                      i32_load(0); local_set(inner);
                      i32_const(0); local_set(j);
                      block_empty; loop_empty;
                        local_get(j); local_get(inner); i32_load(0); i32_ge_u; br_if(1);
                        local_get(result); i32_const(4); i32_add;
                        local_get(total); i32_const(out_es); i32_mul; i32_add;
                        local_get(inner); i32_const(4); i32_add;
                        local_get(j); i32_const(out_es); i32_mul; i32_add;
                });
                self.emit_elem_copy(&out_elem_ty);
                wasm!(self.func, {
                        local_get(total); i32_const(1); i32_add; local_set(total);
                        local_get(j); i32_const(1); i32_add; local_set(j);
                        br(0);
                      end; end;
                      local_get(i); i32_const(1); i32_add; local_set(i);
                      br(0);
                    end; end;
                    local_get(result);
                });
                self.scratch.free_i32(j);
                self.scratch.free_i32(inner);
                self.scratch.free_i32(result);
                self.scratch.free_i32(total);
                self.scratch.free_i32(i);
                self.scratch.free_i32(lol);
                self.scratch.free_i32(len);
                self.scratch.free_i32(closure);
                self.scratch.free_i32(xs);
            }
            "filter_map" => {
                // filter_map(xs, f) → List[B]: f returns Option[B], keep some values
                let elem_ty = self.list_elem_ty(&args[0].ty);
                let es = values::byte_size(&elem_ty) as i32;
                // Output element type B: if input is List[Option[B]], unwrap Option to get B
                let out_elem_ty = if let Ty::Applied(_, inner_args) = &elem_ty {
                    inner_args.first().cloned().unwrap_or(Ty::Int)
                } else if let Ty::Fn { ret, .. } = &args[1].ty {
                    self.list_elem_ty(ret)
                } else { Ty::Int };
                let out_es = values::byte_size(&out_elem_ty) as i32;
                let xs = self.scratch.alloc_i32();
                let closure = self.scratch.alloc_i32();
                let len = self.scratch.alloc_i32();
                let dst = self.scratch.alloc_i32();
                let i = self.scratch.alloc_i32();
                let opt = self.scratch.alloc_i32();
                self.emit_expr(&args[0]);
                wasm!(self.func, { local_set(xs); });
                self.emit_expr(&args[1]);
                wasm!(self.func, {
                    local_set(closure);
                    local_get(xs); i32_load(0); local_set(len);
                    i32_const(4); local_get(len); i32_const(out_es); i32_mul; i32_add;
                    call(self.emitter.rt.alloc); local_set(dst);
                    local_get(dst); i32_const(0); i32_store(0);
                    i32_const(0); local_set(i);
                    block_empty; loop_empty;
                      local_get(i); local_get(len); i32_ge_u; br_if(1);
                      local_get(closure); i32_load(4); // env
                      local_get(xs); i32_const(4); i32_add;
                      local_get(i); i32_const(es); i32_mul; i32_add;
                });
                self.emit_load_at(&elem_ty, 0);
                wasm!(self.func, {
                      local_get(closure); i32_load(0);
                });
                self.emit_closure_call(&elem_ty, &Ty::Unknown); // returns Option ptr (i32)
                wasm!(self.func, {
                      local_set(opt); // option result
                      local_get(opt); i32_const(0); i32_ne;
                      if_empty;
                        // Append unwrapped value to result
                        local_get(dst); i32_const(4); i32_add;
                        local_get(dst); i32_load(0); i32_const(out_es); i32_mul; i32_add;
                        local_get(opt); // some ptr
                });
                // Load inner value from some ptr
                self.emit_load_at(&out_elem_ty, 0);
                self.emit_store_at(&out_elem_ty, 0);
                wasm!(self.func, {
                        local_get(dst);
                        local_get(dst); i32_load(0); i32_const(1); i32_add;
                        i32_store(0);
                      end;
                      local_get(i); i32_const(1); i32_add; local_set(i);
                      br(0);
                    end; end;
                    local_get(dst);
                });
                self.scratch.free_i32(opt);
                self.scratch.free_i32(i);
                self.scratch.free_i32(dst);
                self.scratch.free_i32(len);
                self.scratch.free_i32(closure);
                self.scratch.free_i32(xs);
            }
            "swap" => {
                // swap(xs, i, j) → List[A]: copy with elements at i and j swapped
                let elem_ty = self.list_elem_ty(&args[0].ty);
                let es = values::byte_size(&elem_ty) as i32;
                let elem_vt = values::ty_to_valtype(&elem_ty).unwrap_or(ValType::I32);
                let xs = self.scratch.alloc_i32();
                let idx_i = self.scratch.alloc_i32();
                let idx_j = self.scratch.alloc_i32();
                let len = self.scratch.alloc_i32();
                let dst = self.scratch.alloc_i32();
                let k = self.scratch.alloc_i32();
                let tmp = self.scratch.alloc(elem_vt);
                self.emit_expr(&args[0]);
                wasm!(self.func, { local_set(xs); });
                self.emit_expr(&args[1]); // i
                wasm!(self.func, { i32_wrap_i64; local_set(idx_i); });
                self.emit_expr(&args[2]); // j
                wasm!(self.func, {
                    i32_wrap_i64; local_set(idx_j);
                    local_get(xs); i32_load(0); local_set(len);
                    // Alloc copy
                    i32_const(4); local_get(len); i32_const(es); i32_mul; i32_add;
                    call(self.emitter.rt.alloc); local_set(dst);
                    local_get(dst); local_get(len); i32_store(0);
                    // Copy all elements
                    i32_const(0); local_set(k);
                    block_empty; loop_empty;
                      local_get(k); local_get(len); i32_ge_u; br_if(1);
                      local_get(dst); i32_const(4); i32_add;
                      local_get(k); i32_const(es); i32_mul; i32_add;
                      local_get(xs); i32_const(4); i32_add;
                      local_get(k); i32_const(es); i32_mul; i32_add;
                });
                self.emit_elem_copy(&elem_ty);
                wasm!(self.func, {
                      local_get(k); i32_const(1); i32_add; local_set(k);
                      br(0);
                    end; end;
                });
                // Swap dst[i] and dst[j] using typed scratch local as temp
                // tmp = dst[i]
                wasm!(self.func, {
                    local_get(dst); i32_const(4); i32_add;
                    local_get(idx_i); i32_const(es); i32_mul; i32_add;
                });
                self.emit_load_at(&elem_ty, 0);
                wasm!(self.func, { local_set(tmp); });
                // dst[i] = dst[j]
                wasm!(self.func, {
                    local_get(dst); i32_const(4); i32_add;
                    local_get(idx_i); i32_const(es); i32_mul; i32_add;
                    local_get(dst); i32_const(4); i32_add;
                    local_get(idx_j); i32_const(es); i32_mul; i32_add;
                });
                self.emit_elem_copy(&elem_ty);
                // dst[j] = tmp
                wasm!(self.func, {
                    local_get(dst); i32_const(4); i32_add;
                    local_get(idx_j); i32_const(es); i32_mul; i32_add;
                    local_get(tmp);
                });
                self.emit_store_at(&elem_ty, 0);
                wasm!(self.func, { local_get(dst); });
                self.scratch.free(tmp, elem_vt);
                self.scratch.free_i32(k);
                self.scratch.free_i32(dst);
                self.scratch.free_i32(len);
                self.scratch.free_i32(idx_j);
                self.scratch.free_i32(idx_i);
                self.scratch.free_i32(xs);
            }
            "chunk" => {
                // chunk(xs, n) → List[List[A]]
                // Outer list of inner lists. Each inner list has up to n elements.
                let elem_ty = self.list_elem_ty(&args[0].ty);
                let es = values::byte_size(&elem_ty) as i32;
                let xs = self.scratch.alloc_i32();
                let n = self.scratch.alloc_i32();
                let len = self.scratch.alloc_i32();
                let num_chunks = self.scratch.alloc_i32();
                let outer = self.scratch.alloc_i32();
                let i = self.scratch.alloc_i32();
                let chunk_len = self.scratch.alloc_i32();
                let inner = self.scratch.alloc_i32();
                let j = self.scratch.alloc_i32();
                self.emit_expr(&args[0]);
                wasm!(self.func, { local_set(xs); });
                self.emit_expr(&args[1]); // n
                wasm!(self.func, {
                    i32_wrap_i64; local_set(n);
                    local_get(xs); i32_load(0); local_set(len);
                    // num_chunks = (len + n - 1) / n
                    local_get(len); local_get(n); i32_add; i32_const(1); i32_sub;
                    local_get(n); i32_div_u;
                    local_set(num_chunks);
                    // Alloc outer: 4 + num_chunks * 4 (list of ptrs)
                    i32_const(4); local_get(num_chunks); i32_const(4); i32_mul; i32_add;
                    call(self.emitter.rt.alloc); local_set(outer);
                    local_get(outer); local_get(num_chunks); i32_store(0);
                    i32_const(0); local_set(i);
                    block_empty; loop_empty;
                      local_get(i); local_get(num_chunks); i32_ge_u; br_if(1);
                      // chunk_len = min(n, len - i*n)
                      local_get(len); local_get(i); local_get(n); i32_mul; i32_sub;
                      local_set(chunk_len);
                      local_get(chunk_len); local_get(n); i32_gt_u;
                      if_empty; local_get(n); local_set(chunk_len); end;
                      // Alloc inner: 4 + chunk_len * es
                      i32_const(4); local_get(chunk_len); i32_const(es); i32_mul; i32_add;
                      call(self.emitter.rt.alloc); local_set(inner);
                      local_get(inner); local_get(chunk_len); i32_store(0);
                      // Copy elements
                      i32_const(0); local_set(j);
                      block_empty; loop_empty;
                        local_get(j); local_get(chunk_len); i32_ge_u; br_if(1);
                        local_get(inner); i32_const(4); i32_add;
                        local_get(j); i32_const(es); i32_mul; i32_add;
                        local_get(xs); i32_const(4); i32_add;
                        local_get(i); local_get(n); i32_mul;
                        local_get(j); i32_add;
                        i32_const(es); i32_mul; i32_add;
                });
                self.emit_elem_copy(&elem_ty);
                wasm!(self.func, {
                        local_get(j); i32_const(1); i32_add; local_set(j);
                        br(0);
                      end; end;
                      // outer[i] = inner_ptr
                      local_get(outer); i32_const(4); i32_add;
                      local_get(i); i32_const(4); i32_mul; i32_add;
                      local_get(inner); i32_store(0);
                      local_get(i); i32_const(1); i32_add; local_set(i);
                      br(0);
                    end; end;
                    local_get(outer);
                });
                self.scratch.free_i32(j);
                self.scratch.free_i32(inner);
                self.scratch.free_i32(chunk_len);
                self.scratch.free_i32(i);
                self.scratch.free_i32(outer);
                self.scratch.free_i32(num_chunks);
                self.scratch.free_i32(len);
                self.scratch.free_i32(n);
                self.scratch.free_i32(xs);
            }
            "windows" | "window" => {
                // windows(xs, n) → List[List[A]]: sliding windows of size n
                let elem_ty = self.list_elem_ty(&args[0].ty);
                let es = values::byte_size(&elem_ty) as i32;
                let xs = self.scratch.alloc_i32();
                let n = self.scratch.alloc_i32();
                let len = self.scratch.alloc_i32();
                let num_win = self.scratch.alloc_i32();
                let outer = self.scratch.alloc_i32();
                let i = self.scratch.alloc_i32();
                let inner = self.scratch.alloc_i32();
                let j = self.scratch.alloc_i32();
                self.emit_expr(&args[0]);
                wasm!(self.func, { local_set(xs); });
                self.emit_expr(&args[1]);
                wasm!(self.func, {
                    i32_wrap_i64; local_set(n);
                    local_get(xs); i32_load(0); local_set(len);
                    // num_win = if len >= n then len - n + 1 else 0
                    local_get(len); local_get(n); i32_ge_u;
                    if_i32;
                      local_get(len); local_get(n); i32_sub; i32_const(1); i32_add;
                    else_;
                      i32_const(0);
                    end;
                    local_set(num_win);
                    // Alloc outer: 4 + num_win * 4
                    i32_const(4); local_get(num_win); i32_const(4); i32_mul; i32_add;
                    call(self.emitter.rt.alloc); local_set(outer);
                    local_get(outer); local_get(num_win); i32_store(0);
                    i32_const(0); local_set(i);
                    block_empty; loop_empty;
                      local_get(i); local_get(num_win); i32_ge_u; br_if(1);
                      // Alloc inner: 4 + n * es
                      i32_const(4); local_get(n); i32_const(es); i32_mul; i32_add;
                      call(self.emitter.rt.alloc); local_set(inner);
                      local_get(inner); local_get(n); i32_store(0);
                      // Copy n elements starting at i
                      i32_const(0); local_set(j);
                      block_empty; loop_empty;
                        local_get(j); local_get(n); i32_ge_u; br_if(1);
                        local_get(inner); i32_const(4); i32_add;
                        local_get(j); i32_const(es); i32_mul; i32_add;
                        local_get(xs); i32_const(4); i32_add;
                        local_get(i); local_get(j); i32_add;
                        i32_const(es); i32_mul; i32_add;
                });
                self.emit_elem_copy(&elem_ty);
                wasm!(self.func, {
                        local_get(j); i32_const(1); i32_add; local_set(j);
                        br(0);
                      end; end;
                      // outer[i] = inner_ptr
                      local_get(outer); i32_const(4); i32_add;
                      local_get(i); i32_const(4); i32_mul; i32_add;
                      local_get(inner); i32_store(0);
                      local_get(i); i32_const(1); i32_add; local_set(i);
                      br(0);
                    end; end;
                    local_get(outer);
                });
                self.scratch.free_i32(j);
                self.scratch.free_i32(inner);
                self.scratch.free_i32(i);
                self.scratch.free_i32(outer);
                self.scratch.free_i32(num_win);
                self.scratch.free_i32(len);
                self.scratch.free_i32(n);
                self.scratch.free_i32(xs);
            }
            "dedup" => {
                // dedup(xs) → List[A]: remove consecutive duplicates
                let elem_ty = self.list_elem_ty(&args[0].ty);
                let es = values::byte_size(&elem_ty) as i32;
                let xs = self.scratch.alloc_i32();
                let len = self.scratch.alloc_i32();
                let dst = self.scratch.alloc_i32();
                let i = self.scratch.alloc_i32();
                let out_count = self.scratch.alloc_i32();
                self.emit_expr(&args[0]);
                wasm!(self.func, {
                    local_set(xs);
                    local_get(xs); i32_load(0); local_set(len);
                    // Alloc dst (max = len)
                    i32_const(4); local_get(len); i32_const(es); i32_mul; i32_add;
                    call(self.emitter.rt.alloc); local_set(dst);
                    i32_const(0); local_set(out_count);
                    // If empty, return empty
                    local_get(len); i32_eqz;
                    if_empty;
                      local_get(dst); i32_const(0); i32_store(0);
                    else_;
                      // Always include first element
                      local_get(dst); i32_const(4); i32_add;
                      local_get(xs); i32_const(4); i32_add;
                });
                self.emit_elem_copy(&elem_ty);
                wasm!(self.func, {
                      i32_const(1); local_set(out_count); // out_count = 1
                      i32_const(1); local_set(i); // i = 1
                      block_empty; loop_empty;
                        local_get(i); local_get(len); i32_ge_u; br_if(1);
                        // Compare xs[i] with xs[i-1]
                        local_get(xs); i32_const(4); i32_add;
                        local_get(i); i32_const(es); i32_mul; i32_add;
                        i32_load(0);
                        local_get(xs); i32_const(4); i32_add;
                        local_get(i); i32_const(1); i32_sub;
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
                          local_get(dst); i32_const(4); i32_add;
                          local_get(out_count); i32_const(es); i32_mul; i32_add;
                          local_get(xs); i32_const(4); i32_add;
                          local_get(i); i32_const(es); i32_mul; i32_add;
                });
                self.emit_elem_copy(&elem_ty);
                wasm!(self.func, {
                          local_get(out_count); i32_const(1); i32_add; local_set(out_count);
                        end;
                        local_get(i); i32_const(1); i32_add; local_set(i);
                        br(0);
                      end; end;
                      local_get(dst); local_get(out_count); i32_store(0);
                    end;
                    local_get(dst);
                });
                self.scratch.free_i32(out_count);
                self.scratch.free_i32(i);
                self.scratch.free_i32(dst);
                self.scratch.free_i32(len);
                self.scratch.free_i32(xs);
            }
            "sort_by" => {
                // sort_by(xs, f) → List[A]: bubble sort by key function
                // Strategy: copy list, compute keys into parallel array, bubble sort both
                let elem_ty = self.list_elem_ty(&args[0].ty);
                let es = values::byte_size(&elem_ty) as i32;
                let elem_vt = values::ty_to_valtype(&elem_ty).unwrap_or(ValType::I32);
                // Infer key type from closure return type. The closure's
                // `Ty::Fn.ret` can be Unknown/TypeVar when inference left the
                // Lambda param generic; fall back to the lifted function's
                // registered WASM return ValType so `key_is_str` and `ks`
                // match the closure's call_indirect signature exactly.
                let key_ty_initial = if let Ty::Fn { ret, .. } = &args[1].ty {
                    (**ret).clone()
                } else { Ty::Int };
                let is_unresolved = |t: &Ty| matches!(t, Ty::Unknown | Ty::TypeVar(_));
                let key_vt = if !is_unresolved(&key_ty_initial) {
                    values::ty_to_valtype(&key_ty_initial).unwrap_or(ValType::I32)
                } else {
                    self.resolve_closure_ret_valtype(&args[1]).unwrap_or(ValType::I64)
                };
                // i32 = heap pointer (String/List/Record). i64 = Int. f64 = Float.
                // For sort comparison, i32 keys use string.cmp, i64/f64 use numeric compare.
                // This is correct only when heap pointer keys are always String
                // (the only supported comparable heap type today) — other pointer
                // types will mis-sort, but would have been broken before too.
                let key_is_str = matches!(key_vt, ValType::I32);
                let ks: i32 = match key_vt {
                    ValType::I64 | ValType::F64 => 8,
                    _ => 4,
                };
                // Synthesize a concrete key_ty for `emit_closure_call` sizing.
                let key_ty = match key_vt {
                    ValType::I64 => Ty::Int,
                    ValType::F64 => Ty::Float,
                    _ => Ty::String,
                };
                let xs = self.scratch.alloc_i32();
                let closure = self.scratch.alloc_i32();
                let len = self.scratch.alloc_i32();
                let dst = self.scratch.alloc_i32();
                let keys = self.scratch.alloc_i32();
                let i = self.scratch.alloc_i32();
                let j = self.scratch.alloc_i32();
                let tmp_key = if key_is_str { self.scratch.alloc_i32() } else { self.scratch.alloc_i64() };
                let tmp_elem = self.scratch.alloc(elem_vt);
                self.emit_expr(&args[0]);
                wasm!(self.func, { local_set(xs); });
                self.emit_expr(&args[1]);
                wasm!(self.func, {
                    local_set(closure);
                    local_get(xs); i32_load(0); local_set(len);
                    // Alloc copy of elements
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
                // Alloc keys array: len * ks
                wasm!(self.func, {
                    local_get(len); i32_const(ks); i32_mul;
                    call(self.emitter.rt.alloc); local_set(keys);
                    // Compute keys for all elements
                    i32_const(0); local_set(i);
                    block_empty; loop_empty;
                      local_get(i); local_get(len); i32_ge_u; br_if(1);
                      local_get(closure); i32_load(4); // env
                      local_get(dst); i32_const(4); i32_add;
                      local_get(i); i32_const(es); i32_mul; i32_add;
                });
                self.emit_load_at(&elem_ty, 0);
                wasm!(self.func, {
                      local_get(closure); i32_load(0); // table_idx
                });
                self.emit_closure_call(&elem_ty, &key_ty);
                if key_is_str {
                    wasm!(self.func, {
                          local_set(tmp_key);
                          local_get(keys);
                          local_get(i); i32_const(ks); i32_mul; i32_add;
                          local_get(tmp_key); i32_store(0);
                    });
                } else {
                    wasm!(self.func, {
                          local_set(tmp_key);
                          local_get(keys);
                          local_get(i); i32_const(ks); i32_mul; i32_add;
                          local_get(tmp_key); i64_store(0);
                    });
                }
                wasm!(self.func, {
                      local_get(i); i32_const(1); i32_add; local_set(i);
                      br(0);
                    end; end;
                });
                // Bubble sort: outer loop i from 0..len-1, inner loop j from 0..len-1-i.
                // Skip entirely when len < 2 (nothing to compare) — `len - 1`
                // would underflow to u32::MAX for unsigned comparison and turn
                // the loop into an infinite memory-walker.
                wasm!(self.func, {
                    block_empty;
                      local_get(len); i32_const(2); i32_lt_u; br_if(0);
                    i32_const(0); local_set(i); // i (outer)
                    block_empty; loop_empty;
                      local_get(i); local_get(len); i32_const(1); i32_sub; i32_ge_u; br_if(1);
                      i32_const(0); local_set(j); // j (inner)
                      block_empty; loop_empty;
                        // j < len - 1 - i
                        local_get(len); i32_const(1); i32_sub; local_get(i); i32_sub;
                        local_get(j); i32_le_u; br_if(1);
                        // Compare keys[j] > keys[j+1]
                        local_get(keys);
                        local_get(j); i32_const(ks); i32_mul; i32_add;
                });
                if key_is_str {
                    wasm!(self.func, {
                        i32_load(0);
                        local_get(keys);
                        local_get(j); i32_const(1); i32_add; i32_const(ks); i32_mul; i32_add;
                        i32_load(0);
                        call(self.emitter.rt.string.cmp); i32_const(0); i32_gt_s;
                    });
                } else {
                    wasm!(self.func, {
                        i64_load(0);
                        local_get(keys);
                        local_get(j); i32_const(1); i32_add; i32_const(ks); i32_mul; i32_add;
                        i64_load(0);
                        i64_gt_s;
                    });
                }
                wasm!(self.func, {
                        if_empty;
                          // Swap keys[j] and keys[j+1]
                          local_get(keys);
                          local_get(j); i32_const(ks); i32_mul; i32_add;
                });
                if key_is_str {
                    wasm!(self.func, {
                          i32_load(0); local_set(tmp_key);
                          local_get(keys);
                          local_get(j); i32_const(ks); i32_mul; i32_add;
                          local_get(keys);
                          local_get(j); i32_const(1); i32_add; i32_const(ks); i32_mul; i32_add;
                          i32_load(0); i32_store(0);
                          local_get(keys);
                          local_get(j); i32_const(1); i32_add; i32_const(ks); i32_mul; i32_add;
                          local_get(tmp_key); i32_store(0);
                    });
                } else {
                    wasm!(self.func, {
                          i64_load(0); local_set(tmp_key);
                          local_get(keys);
                          local_get(j); i32_const(ks); i32_mul; i32_add;
                          local_get(keys);
                          local_get(j); i32_const(1); i32_add; i32_const(ks); i32_mul; i32_add;
                          i64_load(0); i64_store(0);
                          local_get(keys);
                          local_get(j); i32_const(1); i32_add; i32_const(ks); i32_mul; i32_add;
                          local_get(tmp_key); i64_store(0);
                    });
                }
                wasm!(self.func, {
                          // Swap dst[j] and dst[j+1] using typed scratch local
                          // tmp_elem = dst[j]
                          local_get(dst); i32_const(4); i32_add;
                          local_get(j); i32_const(es); i32_mul; i32_add;
                });
                self.emit_load_at(&elem_ty, 0);
                wasm!(self.func, {
                          local_set(tmp_elem);
                          // dst[j] = dst[j+1]
                          local_get(dst); i32_const(4); i32_add;
                          local_get(j); i32_const(es); i32_mul; i32_add;
                          local_get(dst); i32_const(4); i32_add;
                          local_get(j); i32_const(1); i32_add; i32_const(es); i32_mul; i32_add;
                });
                self.emit_elem_copy(&elem_ty);
                wasm!(self.func, {
                          // dst[j+1] = tmp_elem
                          local_get(dst); i32_const(4); i32_add;
                          local_get(j); i32_const(1); i32_add; i32_const(es); i32_mul; i32_add;
                          local_get(tmp_elem);
                });
                self.emit_store_at(&elem_ty, 0);
                wasm!(self.func, {
                        end; // end if (swap needed)
                        local_get(j); i32_const(1); i32_add; local_set(j);
                        br(0);
                      end; end; // end inner loop
                      local_get(i); i32_const(1); i32_add; local_set(i);
                      br(0);
                    end; end; // end outer loop
                    end; // end len<2 guard block
                    local_get(dst);
                });
                self.scratch.free(tmp_elem, elem_vt);
                if key_is_str { self.scratch.free_i32(tmp_key); } else { self.scratch.free_i64(tmp_key); }
                self.scratch.free_i32(j);
                self.scratch.free_i32(i);
                self.scratch.free_i32(keys);
                self.scratch.free_i32(dst);
                self.scratch.free_i32(len);
                self.scratch.free_i32(closure);
                self.scratch.free_i32(xs);
            }
            _ => return self.emit_list_closure_call2(method, args),
        }
        true
    }
}
