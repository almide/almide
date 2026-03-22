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
                // Store all args to mem[] BEFORE closure emit to avoid scratch conflicts
                let elem_ty = self.list_elem_ty(&args[0].ty);
                let es = values::byte_size(&elem_ty) as i32;
                let s = self.match_i32_base + self.match_depth;
                // mem[0] = xs
                wasm!(self.func, { i32_const(0); });
                self.emit_expr(&args[0]);
                wasm!(self.func, { i32_store(0); });
                // mem[4] = idx (i32)
                wasm!(self.func, { i32_const(4); });
                self.emit_expr(&args[1]);
                wasm!(self.func, { i32_wrap_i64; i32_store(0); });
                // mem[8] = closure
                wasm!(self.func, { i32_const(8); });
                self.emit_expr(&args[2]);
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
                // Use mem[4]=idx, mem[8]=closure
                wasm!(self.func, {
                    // dst addr for store
                    local_get(s + 2); i32_const(4); i32_add;
                    i32_const(4); i32_load(0); // idx from mem[4]
                    i32_const(es); i32_mul; i32_add;
                    // Call f(dst[idx])
                    i32_const(8); i32_load(0); i32_load(4); // env from mem[8]
                    local_get(s + 2); i32_const(4); i32_add;
                    i32_const(4); i32_load(0); // idx from mem[4]
                    i32_const(es); i32_mul; i32_add;
                });
                self.emit_load_at(&elem_ty, 0);
                wasm!(self.func, {
                    i32_const(8); i32_load(0); i32_load(0); // table_idx from mem[8]
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
                // Determine return element type from call return type (concrete)
                let ret_elem_ty = self.list_elem_ty(&self.stub_ret_ty);
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
            "filter" => {
                // filter(list, fn) → new list with matching elements
                let elem_ty = if let Ty::Applied(_, a) = &args[0].ty {
                    a.first().cloned().unwrap_or(Ty::Int)
                } else { Ty::Int };
                let elem_size = values::byte_size(&elem_ty);
                let s = self.match_i32_base + self.match_depth;
                let len_local = s;
                let idx_local = s + 1;

                // mem[0]=src, mem[4]=fn, mem[8]=dst, mem[12]=out_idx
                wasm!(self.func, { i32_const(0); });
                self.emit_expr(&args[0]);
                wasm!(self.func, {
                    i32_store(0);
                    i32_const(4);
                });
                self.emit_expr(&args[1]);
                wasm!(self.func, {
                    i32_store(0);
                    // len
                    i32_const(0);
                    i32_load(0);
                    i32_load(0);
                    local_set(len_local);
                    // alloc dst (max size = 4 + len * elem_size) → mem[8]
                    i32_const(8);
                    i32_const(4);
                    local_get(len_local);
                    i32_const(elem_size as i32);
                    i32_mul;
                    i32_add;
                    call(self.emitter.rt.alloc);
                    i32_store(0);
                    // out_idx = 0 → mem[12]
                    i32_const(12);
                    i32_const(0);
                    i32_store(0);
                    // idx = 0
                    i32_const(0);
                    local_set(idx_local);
                    // Loop
                    block_empty;
                    loop_empty;
                });
                let saved = self.depth; self.depth += 2;

                wasm!(self.func, {
                    local_get(idx_local);
                    local_get(len_local);
                    i32_ge_u;
                    br_if(1);
                    // Call predicate: fn(element) → bool (i32)
                    // Load closure
                    i32_const(4);
                    i32_load(0);
                });
                wasm!(self.func, {
                    i32_load(4);
                    // Load element
                    i32_const(0);
                    i32_load(0);
                    i32_const(4);
                    i32_add;
                    local_get(idx_local);
                    i32_const(elem_size as i32);
                    i32_mul;
                    i32_add;
                });
                self.emit_load_at(&elem_ty, 0);
                // table_idx
                wasm!(self.func, {
                    i32_const(4);
                    i32_load(0);
                    i32_load(0);
                });
                // call_indirect: filter predicate signature (env: i32, elem: elem_ty) -> i32 (Bool)
                {
                    let mut ct = vec![ValType::I32]; // env
                    if let Some(vt) = values::ty_to_valtype(&elem_ty) { ct.push(vt); }
                    let ti = self.emitter.register_type(ct, vec![ValType::I32]);
                    wasm!(self.func, { call_indirect(ti, 0); });
                }
                // If true, copy element to dst
                wasm!(self.func, {
                    if_empty;
                    // dst[out_idx] = src[idx]
                    i32_const(8);
                    i32_load(0);
                    i32_const(4);
                    i32_add;
                    i32_const(12);
                    i32_load(0);
                    i32_const(elem_size as i32);
                    i32_mul;
                    i32_add;
                    // load src element
                    i32_const(0);
                    i32_load(0);
                    i32_const(4);
                    i32_add;
                    local_get(idx_local);
                    i32_const(elem_size as i32);
                    i32_mul;
                    i32_add;
                });
                self.emit_load_at(&elem_ty, 0);
                self.emit_store_at(&elem_ty, 0);
                wasm!(self.func, {
                    // out_idx++
                    i32_const(12);
                    i32_const(12);
                    i32_load(0);
                    i32_const(1);
                    i32_add;
                    i32_store(0);
                    end; // end if
                    // idx++
                    local_get(idx_local);
                    i32_const(1);
                    i32_add;
                    local_set(idx_local);
                    br(0);
                });

                self.depth = saved;
                wasm!(self.func, {
                    end;
                    end;
                    // Set dst.len = out_idx
                    i32_const(8);
                    i32_load(0);
                    i32_const(12);
                    i32_load(0);
                    i32_store(0);
                    // Return dst
                    i32_const(8);
                    i32_load(0);
                });
            }
            "fold" => {
                // fold(list, init, fn(acc, elem) → acc)
                let elem_ty = if let Ty::Applied(_, a) = &args[0].ty {
                    a.first().cloned().unwrap_or(Ty::Int)
                } else { Ty::Int };
                let elem_size = values::byte_size(&elem_ty);
                let s = self.match_i32_base + self.match_depth;
                let len_local = s;
                let idx_local = s + 1;
                // Accumulator local: use i64 for Int/Float, i32 for everything else
                let acc_local = match values::ty_to_valtype(&args[1].ty) {
                    Some(ValType::I64) | Some(ValType::F64) => self.match_i64_base + self.match_depth,
                    _ => self.match_i32_base + self.match_depth + 2, // after len + idx
                };

                // mem[0]=list, mem[4]=fn
                wasm!(self.func, { i32_const(0); });
                self.emit_expr(&args[0]);
                wasm!(self.func, { i32_store(0); });
                // acc = init
                self.emit_expr(&args[1]);
                wasm!(self.func, {
                    local_set(acc_local);
                    i32_const(4);
                });
                self.emit_expr(&args[2]);
                wasm!(self.func, {
                    i32_store(0);
                    // len
                    i32_const(0);
                    i32_load(0);
                    i32_load(0);
                    local_set(len_local);
                    i32_const(0);
                    local_set(idx_local);
                    block_empty;
                    loop_empty;
                });
                let saved = self.depth; self.depth += 2;

                wasm!(self.func, {
                    local_get(idx_local);
                    local_get(len_local);
                    i32_ge_u;
                    br_if(1);
                    // acc = fn(acc, elem)
                    i32_const(4);
                    i32_load(0);
                });
                wasm!(self.func, {
                    i32_load(4);
                    local_get(acc_local);
                    // load elem
                    i32_const(0);
                    i32_load(0);
                    i32_const(4);
                    i32_add;
                    local_get(idx_local);
                    i32_const(elem_size as i32);
                    i32_mul;
                    i32_add;
                });
                self.emit_load_at(&elem_ty, 0);
                // table_idx
                wasm!(self.func, {
                    i32_const(4);
                    i32_load(0);
                    i32_load(0);
                });
                // Build call_indirect type from concrete types (not lambda Fn type which may have Unknown params)
                // fold signature: (env: i32, acc: acc_ty, elem: elem_ty) -> acc_ty
                {
                    let acc_ty = &args[1].ty;
                    let mut ct = vec![ValType::I32]; // env
                    if let Some(vt) = values::ty_to_valtype(acc_ty) { ct.push(vt); }
                    if let Some(vt) = values::ty_to_valtype(&elem_ty) { ct.push(vt); }
                    let rt = values::ret_type(acc_ty);
                    let ti = self.emitter.register_type(ct, rt);
                    wasm!(self.func, { call_indirect(ti, 0); });
                }
                wasm!(self.func, {
                    local_set(acc_local);
                    local_get(idx_local);
                    i32_const(1);
                    i32_add;
                    local_set(idx_local);
                    br(0);
                });

                self.depth = saved;
                wasm!(self.func, {
                    end;
                    end;
                    local_get(acc_local);
                });
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
        let in_elem_ty = if let Ty::Applied(_, args) = &list_arg.ty {
            args.first().cloned().unwrap_or(Ty::Int)
        } else { Ty::Int };
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

        let s = self.match_i32_base + self.match_depth;
        let len_local = s;
        let idx_local = s + 1;
        let src_local = s + 2;
        let closure_local = s + 3;

        // Store src_ptr and closure in scratch locals (not mem[]) to survive nested calls
        self.emit_expr(list_arg);
        wasm!(self.func, { local_set(src_local); });
        self.emit_expr(fn_arg);
        wasm!(self.func, {
            local_set(closure_local);
            local_get(src_local);
            i32_load(0);
            local_set(len_local);
            // Alloc dst
            i32_const(4);
            local_get(len_local);
            i32_const(out_size as i32);
            i32_mul;
            i32_add;
            call(self.emitter.rt.alloc);
            local_set(s + 4); // dst_local
        });
        let dst_local = s + 4;
        wasm!(self.func, {
            // Set dst.len
            local_get(dst_local);
            local_get(len_local);
            i32_store(0);
            // idx = 0
            i32_const(0);
            local_set(idx_local);
            // Loop
            block_empty;
            loop_empty;
        });
        let saved = self.depth;
        self.depth += 2;

        wasm!(self.func, {
            local_get(idx_local);
            local_get(len_local);
            i32_ge_u;
            br_if(1);
            // dst addr
            local_get(dst_local);
            i32_const(4);
            i32_add;
            local_get(idx_local);
            i32_const(out_size as i32);
            i32_mul;
            i32_add;
            // env_ptr from closure
            local_get(closure_local);
            i32_load(4);
            // src element
            local_get(src_local);
            i32_const(4);
            i32_add;
            local_get(idx_local);
            i32_const(in_size as i32);
            i32_mul;
            i32_add;
        });
        self.emit_load_at(&in_elem_ty, 0);
        // table_idx from closure
        wasm!(self.func, {
            local_get(closure_local);
            i32_load(0);
        });
        // Stack: [dst_elem_addr, env_ptr, element, table_idx]

        // call_indirect: map signature (env: i32, elem: in_elem_ty) -> out_elem_ty
        // Always use concrete types from list/return type, not lambda Fn type (which may have Unknown params)
        {
            let mut ct = vec![ValType::I32]; // env
            if let Some(vt) = values::ty_to_valtype(&in_elem_ty) { ct.push(vt); }
            let rt = values::ret_type(&out_elem_ty);
            let ti = self.emitter.register_type(ct, rt);
            wasm!(self.func, { call_indirect(ti, 0); });
        }
        // Stack: [dst_elem_addr, result]

        // ── Store result at dst addr ──
        self.emit_store_at(&out_elem_ty, 0);
        // Stack: []

        // idx++
        wasm!(self.func, {
            local_get(idx_local);
            i32_const(1);
            i32_add;
            local_set(idx_local);
            br(0);
        });

        self.depth = saved;
        wasm!(self.func, {
            end;
            end;
            local_get(dst_local);
        });
    }
}
