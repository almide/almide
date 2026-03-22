//! Map closure-based stdlib call dispatch for WASM codegen.
//!
//! Handles: fold, each, any, all, count, filter, map, find, update.

use super::FuncCompiler;
use super::values;
use crate::ir::IrExpr;
use crate::types::Ty;
use wasm_encoder::ValType;

impl FuncCompiler<'_> {
    pub(super) fn emit_map_closure_call(&mut self, method: &str, args: &[IrExpr]) -> bool {
        match method {
            "fold" => {
                // fold(m, init, f) → A: f(acc, key, val) for each entry
                let (ks, vs) = self.map_kv_sizes(&args[0].ty);
                let key_ty = self.map_key_ty(&args[0].ty);
                let val_ty = self.map_val_ty(&args[0].ty);
                let entry = ks + vs;
                let s = self.match_i32_base + self.match_depth;
                let s64 = self.match_i64_base + self.match_depth;
                // mem[0]=map, mem[4]=closure
                wasm!(self.func, { i32_const(0); });
                self.emit_expr(&args[0]);
                wasm!(self.func, { i32_store(0); });
                // init → acc
                self.emit_expr(&args[1]);
                wasm!(self.func, { local_set(s64); }); // acc (as i64; works for i32 via reinterpret)
                wasm!(self.func, { i32_const(4); });
                self.emit_expr(&args[2]); // closure
                wasm!(self.func, {
                    i32_store(0);
                    i32_const(0); local_set(s); // i
                    block_empty; loop_empty;
                      local_get(s); i32_const(0); i32_load(0); i32_load(0); i32_ge_u; br_if(1);
                      // Call f(acc, key, val)
                      i32_const(4); i32_load(0); i32_load(4); // env
                      local_get(s64); // acc
                      // key = map[4 + i*entry]
                      i32_const(0); i32_load(0); i32_const(4); i32_add;
                      local_get(s); i32_const(entry as i32); i32_mul; i32_add;
                });
                self.emit_key_load(&key_ty, 0);
                wasm!(self.func, {
                      // val = map[4 + i*entry + ks]
                      i32_const(0); i32_load(0); i32_const(4); i32_add;
                      local_get(s); i32_const(entry as i32); i32_mul; i32_add;
                      i32_const(ks as i32); i32_add;
                });
                self.emit_load_at(&val_ty, 0);
                wasm!(self.func, {
                      i32_const(4); i32_load(0); i32_load(0); // table_idx
                });
                // call_indirect (env, acc, key, val) → acc
                {
                    let acc_vt = ValType::I64; // assume Int acc
                    let key_vt = Self::key_valtype(&key_ty);
                    let mut ct = vec![ValType::I32, acc_vt, key_vt]; // env, acc, key
                    if let Some(vt) = values::ty_to_valtype(&val_ty) { ct.push(vt); }
                    let ti = self.emitter.register_type(ct, vec![acc_vt]);
                    wasm!(self.func, { call_indirect(ti, 0); });
                }
                wasm!(self.func, {
                      local_set(s64);
                      local_get(s); i32_const(1); i32_add; local_set(s);
                      br(0);
                    end; end;
                    local_get(s64);
                });
            }
            "each" => {
                let (ks, vs) = self.map_kv_sizes(&args[0].ty);
                let key_ty = self.map_key_ty(&args[0].ty);
                let val_ty = self.map_val_ty(&args[0].ty);
                let entry = ks + vs;
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
                      i32_const(4); i32_load(0); i32_load(4); // env
                      i32_const(0); i32_load(0); i32_const(4); i32_add;
                      local_get(s); i32_const(entry as i32); i32_mul; i32_add;
                });
                self.emit_key_load(&key_ty, 0);
                wasm!(self.func, {
                      i32_const(0); i32_load(0); i32_const(4); i32_add;
                      local_get(s); i32_const(entry as i32); i32_mul; i32_add;
                      i32_const(ks as i32); i32_add;
                });
                self.emit_load_at(&val_ty, 0);
                wasm!(self.func, {
                      i32_const(4); i32_load(0); i32_load(0);
                });
                {
                    let key_vt = Self::key_valtype(&key_ty);
                    let mut ct = vec![ValType::I32, key_vt]; // env, key
                    if let Some(vt) = values::ty_to_valtype(&val_ty) { ct.push(vt); }
                    let ti = self.emitter.register_type(ct, vec![]);
                    wasm!(self.func, { call_indirect(ti, 0); });
                }
                wasm!(self.func, {
                      local_get(s); i32_const(1); i32_add; local_set(s);
                      br(0);
                    end; end;
                });
            }
            "any" => {
                let (ks, vs) = self.map_kv_sizes(&args[0].ty);
                let key_ty = self.map_key_ty(&args[0].ty);
                let val_ty = self.map_val_ty(&args[0].ty);
                let entry = ks + vs;
                let s = self.match_i32_base + self.match_depth;
                wasm!(self.func, { i32_const(0); });
                self.emit_expr(&args[0]);
                wasm!(self.func, { i32_store(0); i32_const(4); });
                self.emit_expr(&args[1]);
                wasm!(self.func, {
                    i32_store(0);
                    i32_const(0); local_set(s);
                    i32_const(0); local_set(s + 1); // result
                    block_empty; loop_empty;
                      local_get(s); i32_const(0); i32_load(0); i32_load(0); i32_ge_u; br_if(1);
                      i32_const(4); i32_load(0); i32_load(4);
                      i32_const(0); i32_load(0); i32_const(4); i32_add;
                      local_get(s); i32_const(entry as i32); i32_mul; i32_add;
                });
                self.emit_key_load(&key_ty, 0);
                wasm!(self.func, {
                      i32_const(0); i32_load(0); i32_const(4); i32_add;
                      local_get(s); i32_const(entry as i32); i32_mul; i32_add;
                      i32_const(ks as i32); i32_add;
                });
                self.emit_load_at(&val_ty, 0);
                wasm!(self.func, {
                      i32_const(4); i32_load(0); i32_load(0);
                });
                {
                    let key_vt = Self::key_valtype(&key_ty);
                    let mut ct = vec![ValType::I32, key_vt]; // env, key
                    if let Some(vt) = values::ty_to_valtype(&val_ty) { ct.push(vt); }
                    let ti = self.emitter.register_type(ct, vec![ValType::I32]);
                    wasm!(self.func, { call_indirect(ti, 0); });
                }
                wasm!(self.func, {
                      if_empty;
                        i32_const(1); local_set(s + 1); br(2);
                      end;
                      local_get(s); i32_const(1); i32_add; local_set(s);
                      br(0);
                    end; end;
                    local_get(s + 1);
                });
            }
            "all" => {
                let (ks, vs) = self.map_kv_sizes(&args[0].ty);
                let key_ty = self.map_key_ty(&args[0].ty);
                let val_ty = self.map_val_ty(&args[0].ty);
                let entry = ks + vs;
                let s = self.match_i32_base + self.match_depth;
                wasm!(self.func, { i32_const(0); });
                self.emit_expr(&args[0]);
                wasm!(self.func, { i32_store(0); i32_const(4); });
                self.emit_expr(&args[1]);
                wasm!(self.func, {
                    i32_store(0);
                    i32_const(0); local_set(s);
                    i32_const(1); local_set(s + 1);
                    block_empty; loop_empty;
                      local_get(s); i32_const(0); i32_load(0); i32_load(0); i32_ge_u; br_if(1);
                      i32_const(4); i32_load(0); i32_load(4);
                      i32_const(0); i32_load(0); i32_const(4); i32_add;
                      local_get(s); i32_const(entry as i32); i32_mul; i32_add;
                });
                self.emit_key_load(&key_ty, 0);
                wasm!(self.func, {
                      i32_const(0); i32_load(0); i32_const(4); i32_add;
                      local_get(s); i32_const(entry as i32); i32_mul; i32_add;
                      i32_const(ks as i32); i32_add;
                });
                self.emit_load_at(&val_ty, 0);
                wasm!(self.func, {
                      i32_const(4); i32_load(0); i32_load(0);
                });
                {
                    let key_vt = Self::key_valtype(&key_ty);
                    let mut ct = vec![ValType::I32, key_vt];
                    if let Some(vt) = values::ty_to_valtype(&val_ty) { ct.push(vt); }
                    let ti = self.emitter.register_type(ct, vec![ValType::I32]);
                    wasm!(self.func, { call_indirect(ti, 0); });
                }
                wasm!(self.func, {
                      i32_eqz;
                      if_empty;
                        i32_const(0); local_set(s + 1); br(2);
                      end;
                      local_get(s); i32_const(1); i32_add; local_set(s);
                      br(0);
                    end; end;
                    local_get(s + 1);
                });
            }
            "count" => {
                let (ks, vs) = self.map_kv_sizes(&args[0].ty);
                let key_ty = self.map_key_ty(&args[0].ty);
                let val_ty = self.map_val_ty(&args[0].ty);
                let entry = ks + vs;
                let s = self.match_i32_base + self.match_depth;
                let s64 = self.match_i64_base + self.match_depth;
                wasm!(self.func, { i32_const(0); });
                self.emit_expr(&args[0]);
                wasm!(self.func, { i32_store(0); i32_const(4); });
                self.emit_expr(&args[1]);
                wasm!(self.func, {
                    i32_store(0);
                    i32_const(0); local_set(s); // i
                    i64_const(0); local_set(s64); // count
                    block_empty; loop_empty;
                      local_get(s); i32_const(0); i32_load(0); i32_load(0); i32_ge_u; br_if(1);
                      i32_const(4); i32_load(0); i32_load(4);
                      i32_const(0); i32_load(0); i32_const(4); i32_add;
                      local_get(s); i32_const(entry as i32); i32_mul; i32_add;
                });
                self.emit_key_load(&key_ty, 0);
                wasm!(self.func, {
                      i32_const(0); i32_load(0); i32_const(4); i32_add;
                      local_get(s); i32_const(entry as i32); i32_mul; i32_add;
                      i32_const(ks as i32); i32_add;
                });
                self.emit_load_at(&val_ty, 0);
                wasm!(self.func, {
                      i32_const(4); i32_load(0); i32_load(0);
                });
                {
                    let key_vt = Self::key_valtype(&key_ty);
                    let mut ct = vec![ValType::I32, key_vt];
                    if let Some(vt) = values::ty_to_valtype(&val_ty) { ct.push(vt); }
                    let ti = self.emitter.register_type(ct, vec![ValType::I32]);
                    wasm!(self.func, { call_indirect(ti, 0); });
                }
                wasm!(self.func, {
                      if_empty;
                        local_get(s64); i64_const(1); i64_add; local_set(s64);
                      end;
                      local_get(s); i32_const(1); i32_add; local_set(s);
                      br(0);
                    end; end;
                    local_get(s64);
                });
            }
            "filter" => {
                // filter(m, pred) → Map: keep entries where pred(k, v) is true
                let (ks, vs) = self.map_kv_sizes(&args[0].ty);
                let key_ty = self.map_key_ty(&args[0].ty);
                let val_ty = self.map_val_ty(&args[0].ty);
                let entry = ks + vs;
                let s = self.match_i32_base + self.match_depth;
                wasm!(self.func, { i32_const(0); });
                self.emit_expr(&args[0]);
                wasm!(self.func, { i32_store(0); i32_const(4); });
                self.emit_expr(&args[1]);
                wasm!(self.func, {
                    i32_store(0);
                    // Alloc max-size result
                    i32_const(4); i32_const(0); i32_load(0); i32_load(0);
                    i32_const(entry as i32); i32_mul; i32_add;
                    call(self.emitter.rt.alloc); local_set(s); // result
                    local_get(s); i32_const(0); i32_store(0); // len=0
                    i32_const(0); local_set(s + 1); // i
                    block_empty; loop_empty;
                      local_get(s + 1); i32_const(0); i32_load(0); i32_load(0); i32_ge_u; br_if(1);
                      i32_const(4); i32_load(0); i32_load(4);
                      i32_const(0); i32_load(0); i32_const(4); i32_add;
                      local_get(s + 1); i32_const(entry as i32); i32_mul; i32_add;
                });
                self.emit_key_load(&key_ty, 0);
                wasm!(self.func, {
                      i32_const(0); i32_load(0); i32_const(4); i32_add;
                      local_get(s + 1); i32_const(entry as i32); i32_mul; i32_add;
                      i32_const(ks as i32); i32_add;
                });
                self.emit_load_at(&val_ty, 0);
                wasm!(self.func, {
                      i32_const(4); i32_load(0); i32_load(0);
                });
                {
                    let key_vt = Self::key_valtype(&key_ty);
                    let mut ct = vec![ValType::I32, key_vt];
                    if let Some(vt) = values::ty_to_valtype(&val_ty) { ct.push(vt); }
                    let ti = self.emitter.register_type(ct, vec![ValType::I32]);
                    wasm!(self.func, { call_indirect(ti, 0); });
                }
                wasm!(self.func, {
                      if_empty;
                        // Copy key to result[result.len]
                        local_get(s); i32_const(4); i32_add;
                        local_get(s); i32_load(0); i32_const(entry as i32); i32_mul; i32_add;
                        i32_const(0); i32_load(0); i32_const(4); i32_add;
                        local_get(s + 1); i32_const(entry as i32); i32_mul; i32_add;
                });
                self.emit_elem_copy_sized(ks);
                // Copy val
                wasm!(self.func, {
                        local_get(s); i32_const(4); i32_add;
                        local_get(s); i32_load(0); i32_const(entry as i32); i32_mul; i32_add;
                        i32_const(ks as i32); i32_add;
                        i32_const(0); i32_load(0); i32_const(4); i32_add;
                        local_get(s + 1); i32_const(entry as i32); i32_mul; i32_add;
                        i32_const(ks as i32); i32_add;
                });
                self.emit_elem_copy_sized(vs);
                wasm!(self.func, {
                        local_get(s);
                        local_get(s); i32_load(0); i32_const(1); i32_add;
                        i32_store(0);
                      end;
                      local_get(s + 1); i32_const(1); i32_add; local_set(s + 1);
                      br(0);
                    end; end;
                    local_get(s);
                });
            }
            "map" => {
                // map(m, f) → Map[K, B]: transform values
                let (ks, vs) = self.map_kv_sizes(&args[0].ty);
                let val_ty = self.map_val_ty(&args[0].ty);
                let entry = ks + vs;
                let s = self.match_i32_base + self.match_depth;
                wasm!(self.func, { i32_const(0); });
                self.emit_expr(&args[0]);
                wasm!(self.func, { i32_store(0); i32_const(4); });
                self.emit_expr(&args[1]); // f(v) -> B
                wasm!(self.func, {
                    i32_store(0);
                    i32_const(0); i32_load(0); i32_load(0); local_set(s); // len
                    // Result: same key size, assume same val size (B might differ but we use same entry layout)
                    i32_const(4); local_get(s); i32_const(entry as i32); i32_mul; i32_add;
                    call(self.emitter.rt.alloc); local_set(s + 1);
                    local_get(s + 1); local_get(s); i32_store(0);
                    i32_const(0); local_set(s + 2); // i
                    block_empty; loop_empty;
                      local_get(s + 2); local_get(s); i32_ge_u; br_if(1);
                      // Copy key
                      local_get(s + 1); i32_const(4); i32_add;
                      local_get(s + 2); i32_const(entry as i32); i32_mul; i32_add;
                      i32_const(0); i32_load(0); i32_const(4); i32_add;
                      local_get(s + 2); i32_const(entry as i32); i32_mul; i32_add;
                });
                self.emit_elem_copy_sized(ks);
                wasm!(self.func, {
                      // Call f(val) → new val
                      local_get(s + 1); i32_const(4); i32_add;
                      local_get(s + 2); i32_const(entry as i32); i32_mul; i32_add;
                      i32_const(ks as i32); i32_add; // dst val addr
                      i32_const(4); i32_load(0); i32_load(4); // env
                      i32_const(0); i32_load(0); i32_const(4); i32_add;
                      local_get(s + 2); i32_const(entry as i32); i32_mul; i32_add;
                      i32_const(ks as i32); i32_add;
                });
                self.emit_load_at(&val_ty, 0);
                wasm!(self.func, {
                      i32_const(4); i32_load(0); i32_load(0); // table_idx
                });
                {
                    let mut ct = vec![ValType::I32]; // env
                    if let Some(vt) = values::ty_to_valtype(&val_ty) { ct.push(vt); }
                    // Assume result type same as val type for now
                    let rt = if let Some(vt) = values::ty_to_valtype(&val_ty) { vec![vt] } else { vec![ValType::I32] };
                    let ti = self.emitter.register_type(ct, rt);
                    wasm!(self.func, { call_indirect(ti, 0); });
                }
                // Store result val
                self.emit_store_at(&val_ty, 0);
                wasm!(self.func, {
                      local_get(s + 2); i32_const(1); i32_add; local_set(s + 2);
                      br(0);
                    end; end;
                    local_get(s + 1);
                });
            }
            "find" => {
                // find(m, pred) → Option[(K, V)]
                let (ks, vs) = self.map_kv_sizes(&args[0].ty);
                let key_ty = self.map_key_ty(&args[0].ty);
                let val_ty = self.map_val_ty(&args[0].ty);
                let entry = ks + vs;
                let s = self.match_i32_base + self.match_depth;
                wasm!(self.func, { i32_const(0); });
                self.emit_expr(&args[0]);
                wasm!(self.func, { i32_store(0); i32_const(4); });
                self.emit_expr(&args[1]);
                wasm!(self.func, {
                    i32_store(0);
                    i32_const(0); local_set(s); // i
                    i32_const(0); local_set(s + 1); // result (none)
                    block_empty; loop_empty;
                      local_get(s); i32_const(0); i32_load(0); i32_load(0); i32_ge_u; br_if(1);
                      i32_const(4); i32_load(0); i32_load(4);
                      i32_const(0); i32_load(0); i32_const(4); i32_add;
                      local_get(s); i32_const(entry as i32); i32_mul; i32_add;
                });
                self.emit_key_load(&key_ty, 0);
                wasm!(self.func, {
                      i32_const(0); i32_load(0); i32_const(4); i32_add;
                      local_get(s); i32_const(entry as i32); i32_mul; i32_add;
                      i32_const(ks as i32); i32_add;
                });
                self.emit_load_at(&val_ty, 0);
                wasm!(self.func, {
                      i32_const(4); i32_load(0); i32_load(0);
                });
                {
                    let key_vt = Self::key_valtype(&key_ty);
                    let mut ct = vec![ValType::I32, key_vt];
                    if let Some(vt) = values::ty_to_valtype(&val_ty) { ct.push(vt); }
                    let ti = self.emitter.register_type(ct, vec![ValType::I32]);
                    wasm!(self.func, { call_indirect(ti, 0); });
                }
                wasm!(self.func, {
                      if_empty;
                        // Alloc tuple (key, val) and wrap in Option
                        i32_const(entry as i32); call(self.emitter.rt.alloc); local_set(s + 2);
                        // Copy key
                        local_get(s + 2);
                        i32_const(0); i32_load(0); i32_const(4); i32_add;
                        local_get(s); i32_const(entry as i32); i32_mul; i32_add;
                });
                self.emit_elem_copy_sized(ks);
                // Copy val
                wasm!(self.func, {
                        local_get(s + 2); i32_const(ks as i32); i32_add;
                        i32_const(0); i32_load(0); i32_const(4); i32_add;
                        local_get(s); i32_const(entry as i32); i32_mul; i32_add;
                        i32_const(ks as i32); i32_add;
                });
                self.emit_elem_copy_sized(vs);
                wasm!(self.func, {
                        // Wrap in some
                        i32_const(4); call(self.emitter.rt.alloc); local_set(s + 1);
                        local_get(s + 1); local_get(s + 2); i32_store(0);
                        br(2); // break out
                      end;
                      local_get(s); i32_const(1); i32_add; local_set(s);
                      br(0);
                    end; end;
                    local_get(s + 1); // result (none=0 or some ptr)
                });
            }
            "update" => {
                // update(m, key, f) → Map: apply f to value at key
                let (ks, vs) = self.map_kv_sizes(&args[0].ty);
                let key_ty = self.map_key_ty(&args[0].ty);
                let val_ty = self.map_val_ty(&args[0].ty);
                let entry = ks + vs;
                let s = self.match_i32_base + self.match_depth;
                let s64 = self.match_i64_base + self.match_depth;
                // Reuse set logic: find key, apply f to value, then set
                // mem[0]=map, closure stored at mem[4] (or mem[8] for non-Int keys)
                let closure_mem_offset = if matches!(key_ty, Ty::Int) { 4u32 } else { 8u32 };
                wasm!(self.func, { i32_const(0); });
                self.emit_expr(&args[0]);
                wasm!(self.func, { i32_store(0); });
                // Store search key
                if !matches!(key_ty, Ty::Int) {
                    wasm!(self.func, { i32_const(4); });
                }
                self.emit_expr(&args[1]); // key
                self.emit_search_key_store(&key_ty, 4, s64);
                wasm!(self.func, { i32_const(closure_mem_offset as i32); });
                self.emit_expr(&args[2]); // closure f
                wasm!(self.func, {
                    i32_store(0); // mem[closure_mem_offset] = closure
                    // Find key index
                    i32_const(0); local_set(s); // i
                    i32_const(-1); local_set(s + 1); // found_idx
                    block_empty; loop_empty;
                      local_get(s); i32_const(0); i32_load(0); i32_load(0); i32_ge_u; br_if(1);
                      i32_const(0); i32_load(0); i32_const(4); i32_add;
                      local_get(s); i32_const(entry as i32); i32_mul; i32_add;
                });
                self.emit_key_load(&key_ty, 0);
                self.emit_search_key_load(&key_ty, 4, s64);
                self.emit_key_eq(&key_ty);
                wasm!(self.func, {
                      if_empty; local_get(s); local_set(s + 1); end;
                      local_get(s); i32_const(1); i32_add; local_set(s);
                      br(0);
                    end; end;
                    // If not found, return original
                    local_get(s + 1); i32_const(0); i32_lt_s;
                    if_i32; i32_const(0); i32_load(0);
                    else_;
                      // Copy map, replace value at found_idx with f(old_val)
                      i32_const(0); i32_load(0); i32_load(0); local_set(s); // len
                      i32_const(4); local_get(s); i32_const(entry as i32); i32_mul; i32_add;
                      call(self.emitter.rt.alloc); local_set(s + 2);
                      local_get(s + 2); local_get(s); i32_store(0);
                      // Copy all entries
                      i32_const(0); local_set(s + 3);
                      block_empty; loop_empty;
                        local_get(s + 3); local_get(s); i32_ge_u; br_if(1);
                        // Copy key
                        local_get(s + 2); i32_const(4); i32_add;
                        local_get(s + 3); i32_const(entry as i32); i32_mul; i32_add;
                        i32_const(0); i32_load(0); i32_const(4); i32_add;
                        local_get(s + 3); i32_const(entry as i32); i32_mul; i32_add;
                });
                self.emit_elem_copy_sized(ks);
                // Copy val
                wasm!(self.func, {
                        local_get(s + 2); i32_const(4); i32_add;
                        local_get(s + 3); i32_const(entry as i32); i32_mul; i32_add;
                        i32_const(ks as i32); i32_add;
                        i32_const(0); i32_load(0); i32_const(4); i32_add;
                        local_get(s + 3); i32_const(entry as i32); i32_mul; i32_add;
                        i32_const(ks as i32); i32_add;
                });
                self.emit_elem_copy_sized(vs);
                wasm!(self.func, {
                        local_get(s + 3); i32_const(1); i32_add; local_set(s + 3);
                        br(0);
                      end; end;
                      // Now apply f to the value at found_idx
                      local_get(s + 2); i32_const(4); i32_add;
                      local_get(s + 1); i32_const(entry as i32); i32_mul; i32_add;
                      i32_const(ks as i32); i32_add; // dst val addr
                      i32_const(closure_mem_offset as i32); i32_load(0); i32_load(4); // env
                      // Load old val
                      local_get(s + 2); i32_const(4); i32_add;
                      local_get(s + 1); i32_const(entry as i32); i32_mul; i32_add;
                      i32_const(ks as i32); i32_add;
                });
                self.emit_load_at(&val_ty, 0);
                wasm!(self.func, {
                      i32_const(closure_mem_offset as i32); i32_load(0); i32_load(0); // table_idx
                });
                {
                    let mut ct = vec![ValType::I32]; // env
                    if let Some(vt) = values::ty_to_valtype(&val_ty) { ct.push(vt); }
                    let rt = if let Some(vt) = values::ty_to_valtype(&val_ty) { vec![vt] } else { vec![ValType::I32] };
                    let ti = self.emitter.register_type(ct, rt);
                    wasm!(self.func, { call_indirect(ti, 0); });
                }
                self.emit_store_at(&val_ty, 0);
                wasm!(self.func, {
                      local_get(s + 2);
                    end;
                });
            }
            _ => return false,
        }
        true
    }
}
