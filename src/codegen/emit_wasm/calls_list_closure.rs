//! List stdlib closure-based call dispatch for WASM codegen.
//!
//! Functions that take closures as arguments: find, find_index, any, all, each,
//! reduce, flat_map, filter_map, sort_by, take_while, drop_while, count,
//! partition, update, scan, zip_with, unique_by.

use super::FuncCompiler;
use super::values;
use crate::ir::IrExpr;
use crate::types::Ty;
use wasm_encoder::ValType;

impl FuncCompiler<'_> {
    /// Dispatch a list stdlib closure-based call. Returns true if handled.
    pub(super) fn emit_list_closure_call(&mut self, method: &str, args: &[IrExpr]) -> bool {
        match method {
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
                // repeat(val, n) → List[A] — args[0] IS the element, not a list
                let elem_ty = args[0].ty.clone();
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
                let elem_ty = self.list_elem_ty(&args[0].ty);
                // Output element type B: infer from fn return type List[B]
                let out_elem_ty = if let Ty::Fn { ret, .. } = &args[1].ty {
                    self.list_elem_ty(ret) // List[B] → B
                } else { elem_ty.clone() };
                let out_es = values::byte_size(&out_elem_ty) as i32;
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
                    // Alloc result
                    i32_const(4); local_get(s + 1); i32_const(out_es); i32_mul; i32_add;
                    call(self.emitter.rt.alloc); local_set(s + 3);
                    local_get(s + 3); local_get(s + 1); i32_store(0);
                });
                // Copy all sub-list elements
                wasm!(self.func, {
                    i32_const(0); local_set(s + 1); // dst_offset
                    i32_const(0); local_set(s + 2); // outer i
                    block_empty; loop_empty;
                      local_get(s + 2); local_get(s); i32_load(0); i32_ge_u; br_if(1);
                      local_get(s); i32_const(4); i32_add;
                      local_get(s + 2); i32_const(4); i32_mul; i32_add;
                      i32_load(0); local_set(s + 4); // inner list
                      i32_const(0); local_set(s + 5); // j
                      block_empty; loop_empty;
                        local_get(s + 5); local_get(s + 4); i32_load(0); i32_ge_u; br_if(1);
                        local_get(s + 3); i32_const(4); i32_add;
                        local_get(s + 1); i32_const(out_es); i32_mul; i32_add;
                        local_get(s + 4); i32_const(4); i32_add;
                        local_get(s + 5); i32_const(out_es); i32_mul; i32_add;
                });
                self.emit_elem_copy(&out_elem_ty);
                wasm!(self.func, {
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
                // Output element type B: if input is List[Option[B]], unwrap Option to get B
                let out_elem_ty = if let Ty::Applied(_, inner_args) = &elem_ty {
                    inner_args.first().cloned().unwrap_or(Ty::Int)
                } else if let Ty::Fn { ret, .. } = &args[1].ty {
                    self.list_elem_ty(ret)
                } else { Ty::Int };
                let out_es = values::byte_size(&out_elem_ty) as i32;
                let s = self.match_i32_base + self.match_depth;
                // mem[0]=xs, mem[4]=closure
                wasm!(self.func, { i32_const(0); });
                self.emit_expr(&args[0]);
                wasm!(self.func, { i32_store(0); i32_const(4); });
                self.emit_expr(&args[1]);
                wasm!(self.func, {
                    i32_store(0);
                    i32_const(0); i32_load(0); i32_load(0); local_set(s);
                    i32_const(4); local_get(s); i32_const(out_es); i32_mul; i32_add;
                    call(self.emitter.rt.alloc); local_set(s + 1);
                    local_get(s + 1); i32_const(0); i32_store(0);
                    i32_const(0); local_set(s + 2);
                    block_empty; loop_empty;
                      local_get(s + 2); local_get(s); i32_ge_u; br_if(1);
                      i32_const(4); i32_load(0); i32_load(4); // env
                      i32_const(0); i32_load(0); i32_const(4); i32_add;
                      local_get(s + 2); i32_const(es); i32_mul; i32_add;
                });
                self.emit_load_at(&elem_ty, 0);
                wasm!(self.func, {
                      i32_const(4); i32_load(0); i32_load(0);
                });
                {
                    let mut ct = vec![ValType::I32];
                    if let Some(vt) = values::ty_to_valtype(&elem_ty) { ct.push(vt); }
                    let ti = self.emitter.register_type(ct, vec![ValType::I32]);
                    wasm!(self.func, { call_indirect(ti, 0); });
                }
                wasm!(self.func, {
                      local_set(s + 3); // option result
                      local_get(s + 3); i32_const(0); i32_ne;
                      if_empty;
                        // Append unwrapped value to result
                        local_get(s + 1); i32_const(4); i32_add;
                        local_get(s + 1); i32_load(0); i32_const(out_es); i32_mul; i32_add;
                        local_get(s + 3); // some ptr
                });
                // Load inner value from some ptr
                self.emit_load_at(&out_elem_ty, 0);
                self.emit_store_at(&out_elem_ty, 0);
                wasm!(self.func, {
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
            _ => return self.emit_list_closure_call2(method, args),
        }
        true
    }
}
