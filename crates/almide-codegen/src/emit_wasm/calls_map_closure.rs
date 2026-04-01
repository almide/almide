//! Map closure-based stdlib call dispatch for WASM codegen.
//!
//! Handles: fold, each, any, all, count, filter, map, find, update.

use super::FuncCompiler;
use super::values;
use almide_ir::IrExpr;
use almide_lang::types::Ty;
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
                let map_ptr = self.scratch.alloc_i32();
                let closure = self.scratch.alloc_i32();
                let i = self.scratch.alloc_i32();
                let acc = self.scratch.alloc_i64();
                self.emit_expr(&args[0]);
                wasm!(self.func, { local_set(map_ptr); });
                // init → acc
                self.emit_expr(&args[1]);
                wasm!(self.func, { local_set(acc); });
                self.emit_expr(&args[2]); // closure
                wasm!(self.func, {
                    local_set(closure);
                    i32_const(0); local_set(i);
                    block_empty; loop_empty;
                      local_get(i); local_get(map_ptr); i32_load(0); i32_ge_u; br_if(1);
                      // Call f(acc, key, val)
                      local_get(closure); i32_load(4); // env
                      local_get(acc); // acc
                      // key = map[4 + i*entry]
                      local_get(map_ptr); i32_const(4); i32_add;
                      local_get(i); i32_const(entry as i32); i32_mul; i32_add;
                });
                self.emit_key_load(&key_ty, 0);
                wasm!(self.func, {
                      // val = map[4 + i*entry + ks]
                      local_get(map_ptr); i32_const(4); i32_add;
                      local_get(i); i32_const(entry as i32); i32_mul; i32_add;
                      i32_const(ks as i32); i32_add;
                });
                self.emit_load_at(&val_ty, 0);
                wasm!(self.func, {
                      local_get(closure); i32_load(0); // table_idx
                });
                // call_indirect (env, acc, key, val) → acc
                {
                    let acc_vt = ValType::I64; // assume Int acc
                    let key_vt = Self::key_valtype(&key_ty);
                    let mut ct = vec![ValType::I32, acc_vt, key_vt]; // env, acc, key
                    if let Some(vt) = values::ty_to_valtype(&val_ty) { ct.push(vt); }
                    self.emit_call_indirect(ct, vec![acc_vt]);
                }
                wasm!(self.func, {
                      local_set(acc);
                      local_get(i); i32_const(1); i32_add; local_set(i);
                      br(0);
                    end; end;
                    local_get(acc);
                });
                self.scratch.free_i64(acc);
                self.scratch.free_i32(i);
                self.scratch.free_i32(closure);
                self.scratch.free_i32(map_ptr);
            }
            "each" => {
                let (ks, vs) = self.map_kv_sizes(&args[0].ty);
                let key_ty = self.map_key_ty(&args[0].ty);
                let val_ty = self.map_val_ty(&args[0].ty);
                let entry = ks + vs;
                let map_ptr = self.scratch.alloc_i32();
                let closure = self.scratch.alloc_i32();
                let i = self.scratch.alloc_i32();
                self.emit_expr(&args[0]);
                wasm!(self.func, { local_set(map_ptr); });
                self.emit_expr(&args[1]);
                wasm!(self.func, {
                    local_set(closure);
                    i32_const(0); local_set(i);
                    block_empty; loop_empty;
                      local_get(i); local_get(map_ptr); i32_load(0); i32_ge_u; br_if(1);
                      local_get(closure); i32_load(4); // env
                      local_get(map_ptr); i32_const(4); i32_add;
                      local_get(i); i32_const(entry as i32); i32_mul; i32_add;
                });
                self.emit_key_load(&key_ty, 0);
                wasm!(self.func, {
                      local_get(map_ptr); i32_const(4); i32_add;
                      local_get(i); i32_const(entry as i32); i32_mul; i32_add;
                      i32_const(ks as i32); i32_add;
                });
                self.emit_load_at(&val_ty, 0);
                wasm!(self.func, {
                      local_get(closure); i32_load(0);
                });
                {
                    let key_vt = Self::key_valtype(&key_ty);
                    let mut ct = vec![ValType::I32, key_vt]; // env, key
                    if let Some(vt) = values::ty_to_valtype(&val_ty) { ct.push(vt); }
                    self.emit_call_indirect(ct, vec![]);
                }
                wasm!(self.func, {
                      local_get(i); i32_const(1); i32_add; local_set(i);
                      br(0);
                    end; end;
                });
                self.scratch.free_i32(i);
                self.scratch.free_i32(closure);
                self.scratch.free_i32(map_ptr);
            }
            "any" => {
                let (ks, vs) = self.map_kv_sizes(&args[0].ty);
                let key_ty = self.map_key_ty(&args[0].ty);
                let val_ty = self.map_val_ty(&args[0].ty);
                let entry = ks + vs;
                let map_ptr = self.scratch.alloc_i32();
                let closure = self.scratch.alloc_i32();
                let i = self.scratch.alloc_i32();
                let result = self.scratch.alloc_i32();
                self.emit_expr(&args[0]);
                wasm!(self.func, { local_set(map_ptr); });
                self.emit_expr(&args[1]);
                wasm!(self.func, {
                    local_set(closure);
                    i32_const(0); local_set(i);
                    i32_const(0); local_set(result);
                    block_empty; loop_empty;
                      local_get(i); local_get(map_ptr); i32_load(0); i32_ge_u; br_if(1);
                      local_get(closure); i32_load(4);
                      local_get(map_ptr); i32_const(4); i32_add;
                      local_get(i); i32_const(entry as i32); i32_mul; i32_add;
                });
                self.emit_key_load(&key_ty, 0);
                wasm!(self.func, {
                      local_get(map_ptr); i32_const(4); i32_add;
                      local_get(i); i32_const(entry as i32); i32_mul; i32_add;
                      i32_const(ks as i32); i32_add;
                });
                self.emit_load_at(&val_ty, 0);
                wasm!(self.func, {
                      local_get(closure); i32_load(0);
                });
                {
                    let key_vt = Self::key_valtype(&key_ty);
                    let mut ct = vec![ValType::I32, key_vt];
                    if let Some(vt) = values::ty_to_valtype(&val_ty) { ct.push(vt); }
                    self.emit_call_indirect(ct, vec![ValType::I32]);
                }
                wasm!(self.func, {
                      if_empty;
                        i32_const(1); local_set(result); br(2);
                      end;
                      local_get(i); i32_const(1); i32_add; local_set(i);
                      br(0);
                    end; end;
                    local_get(result);
                });
                self.scratch.free_i32(result);
                self.scratch.free_i32(i);
                self.scratch.free_i32(closure);
                self.scratch.free_i32(map_ptr);
            }
            "all" => {
                let (ks, vs) = self.map_kv_sizes(&args[0].ty);
                let key_ty = self.map_key_ty(&args[0].ty);
                let val_ty = self.map_val_ty(&args[0].ty);
                let entry = ks + vs;
                let map_ptr = self.scratch.alloc_i32();
                let closure = self.scratch.alloc_i32();
                let i = self.scratch.alloc_i32();
                let result = self.scratch.alloc_i32();
                self.emit_expr(&args[0]);
                wasm!(self.func, { local_set(map_ptr); });
                self.emit_expr(&args[1]);
                wasm!(self.func, {
                    local_set(closure);
                    i32_const(0); local_set(i);
                    i32_const(1); local_set(result);
                    block_empty; loop_empty;
                      local_get(i); local_get(map_ptr); i32_load(0); i32_ge_u; br_if(1);
                      local_get(closure); i32_load(4);
                      local_get(map_ptr); i32_const(4); i32_add;
                      local_get(i); i32_const(entry as i32); i32_mul; i32_add;
                });
                self.emit_key_load(&key_ty, 0);
                wasm!(self.func, {
                      local_get(map_ptr); i32_const(4); i32_add;
                      local_get(i); i32_const(entry as i32); i32_mul; i32_add;
                      i32_const(ks as i32); i32_add;
                });
                self.emit_load_at(&val_ty, 0);
                wasm!(self.func, {
                      local_get(closure); i32_load(0);
                });
                {
                    let key_vt = Self::key_valtype(&key_ty);
                    let mut ct = vec![ValType::I32, key_vt];
                    if let Some(vt) = values::ty_to_valtype(&val_ty) { ct.push(vt); }
                    self.emit_call_indirect(ct, vec![ValType::I32]);
                }
                wasm!(self.func, {
                      i32_eqz;
                      if_empty;
                        i32_const(0); local_set(result); br(2);
                      end;
                      local_get(i); i32_const(1); i32_add; local_set(i);
                      br(0);
                    end; end;
                    local_get(result);
                });
                self.scratch.free_i32(result);
                self.scratch.free_i32(i);
                self.scratch.free_i32(closure);
                self.scratch.free_i32(map_ptr);
            }
            "count" => {
                let (ks, vs) = self.map_kv_sizes(&args[0].ty);
                let key_ty = self.map_key_ty(&args[0].ty);
                let val_ty = self.map_val_ty(&args[0].ty);
                let entry = ks + vs;
                let map_ptr = self.scratch.alloc_i32();
                let closure = self.scratch.alloc_i32();
                let i = self.scratch.alloc_i32();
                let cnt = self.scratch.alloc_i64();
                self.emit_expr(&args[0]);
                wasm!(self.func, { local_set(map_ptr); });
                self.emit_expr(&args[1]);
                wasm!(self.func, {
                    local_set(closure);
                    i32_const(0); local_set(i);
                    i64_const(0); local_set(cnt);
                    block_empty; loop_empty;
                      local_get(i); local_get(map_ptr); i32_load(0); i32_ge_u; br_if(1);
                      local_get(closure); i32_load(4);
                      local_get(map_ptr); i32_const(4); i32_add;
                      local_get(i); i32_const(entry as i32); i32_mul; i32_add;
                });
                self.emit_key_load(&key_ty, 0);
                wasm!(self.func, {
                      local_get(map_ptr); i32_const(4); i32_add;
                      local_get(i); i32_const(entry as i32); i32_mul; i32_add;
                      i32_const(ks as i32); i32_add;
                });
                self.emit_load_at(&val_ty, 0);
                wasm!(self.func, {
                      local_get(closure); i32_load(0);
                });
                {
                    let key_vt = Self::key_valtype(&key_ty);
                    let mut ct = vec![ValType::I32, key_vt];
                    if let Some(vt) = values::ty_to_valtype(&val_ty) { ct.push(vt); }
                    self.emit_call_indirect(ct, vec![ValType::I32]);
                }
                wasm!(self.func, {
                      if_empty;
                        local_get(cnt); i64_const(1); i64_add; local_set(cnt);
                      end;
                      local_get(i); i32_const(1); i32_add; local_set(i);
                      br(0);
                    end; end;
                    local_get(cnt);
                });
                self.scratch.free_i64(cnt);
                self.scratch.free_i32(i);
                self.scratch.free_i32(closure);
                self.scratch.free_i32(map_ptr);
            }
            "filter" => {
                // filter(m, pred) → Map: keep entries where pred(k, v) is true
                let (ks, vs) = self.map_kv_sizes(&args[0].ty);
                let key_ty = self.map_key_ty(&args[0].ty);
                let val_ty = self.map_val_ty(&args[0].ty);
                let entry = ks + vs;
                let map_ptr = self.scratch.alloc_i32();
                let closure = self.scratch.alloc_i32();
                let result = self.scratch.alloc_i32();
                let i = self.scratch.alloc_i32();
                self.emit_expr(&args[0]);
                wasm!(self.func, { local_set(map_ptr); });
                self.emit_expr(&args[1]);
                wasm!(self.func, {
                    local_set(closure);
                    // Alloc max-size result
                    i32_const(4); local_get(map_ptr); i32_load(0);
                    i32_const(entry as i32); i32_mul; i32_add;
                    call(self.emitter.rt.alloc); local_set(result);
                    local_get(result); i32_const(0); i32_store(0); // len=0
                    i32_const(0); local_set(i);
                    block_empty; loop_empty;
                      local_get(i); local_get(map_ptr); i32_load(0); i32_ge_u; br_if(1);
                      local_get(closure); i32_load(4);
                      local_get(map_ptr); i32_const(4); i32_add;
                      local_get(i); i32_const(entry as i32); i32_mul; i32_add;
                });
                self.emit_key_load(&key_ty, 0);
                wasm!(self.func, {
                      local_get(map_ptr); i32_const(4); i32_add;
                      local_get(i); i32_const(entry as i32); i32_mul; i32_add;
                      i32_const(ks as i32); i32_add;
                });
                self.emit_load_at(&val_ty, 0);
                wasm!(self.func, {
                      local_get(closure); i32_load(0);
                });
                {
                    let key_vt = Self::key_valtype(&key_ty);
                    let mut ct = vec![ValType::I32, key_vt];
                    if let Some(vt) = values::ty_to_valtype(&val_ty) { ct.push(vt); }
                    self.emit_call_indirect(ct, vec![ValType::I32]);
                }
                wasm!(self.func, {
                      if_empty;
                        // Copy key to result[result.len]
                        local_get(result); i32_const(4); i32_add;
                        local_get(result); i32_load(0); i32_const(entry as i32); i32_mul; i32_add;
                        local_get(map_ptr); i32_const(4); i32_add;
                        local_get(i); i32_const(entry as i32); i32_mul; i32_add;
                });
                self.emit_elem_copy_sized(ks);
                // Copy val
                wasm!(self.func, {
                        local_get(result); i32_const(4); i32_add;
                        local_get(result); i32_load(0); i32_const(entry as i32); i32_mul; i32_add;
                        i32_const(ks as i32); i32_add;
                        local_get(map_ptr); i32_const(4); i32_add;
                        local_get(i); i32_const(entry as i32); i32_mul; i32_add;
                        i32_const(ks as i32); i32_add;
                });
                self.emit_elem_copy_sized(vs);
                wasm!(self.func, {
                        local_get(result);
                        local_get(result); i32_load(0); i32_const(1); i32_add;
                        i32_store(0);
                      end;
                      local_get(i); i32_const(1); i32_add; local_set(i);
                      br(0);
                    end; end;
                    local_get(result);
                });
                self.scratch.free_i32(i);
                self.scratch.free_i32(result);
                self.scratch.free_i32(closure);
                self.scratch.free_i32(map_ptr);
            }
            "map" => {
                // map(m, f) → Map[K, B]: transform values
                let (ks, vs) = self.map_kv_sizes(&args[0].ty);
                let val_ty = self.map_val_ty(&args[0].ty);
                let entry = ks + vs;
                let map_ptr = self.scratch.alloc_i32();
                let closure = self.scratch.alloc_i32();
                let len = self.scratch.alloc_i32();
                let result = self.scratch.alloc_i32();
                let i = self.scratch.alloc_i32();
                self.emit_expr(&args[0]);
                wasm!(self.func, { local_set(map_ptr); });
                self.emit_expr(&args[1]); // f(v) -> B
                wasm!(self.func, {
                    local_set(closure);
                    local_get(map_ptr); i32_load(0); local_set(len);
                    // Result: same key size, assume same val size (B might differ but we use same entry layout)
                    i32_const(4); local_get(len); i32_const(entry as i32); i32_mul; i32_add;
                    call(self.emitter.rt.alloc); local_set(result);
                    local_get(result); local_get(len); i32_store(0);
                    i32_const(0); local_set(i);
                    block_empty; loop_empty;
                      local_get(i); local_get(len); i32_ge_u; br_if(1);
                      // Copy key
                      local_get(result); i32_const(4); i32_add;
                      local_get(i); i32_const(entry as i32); i32_mul; i32_add;
                      local_get(map_ptr); i32_const(4); i32_add;
                      local_get(i); i32_const(entry as i32); i32_mul; i32_add;
                });
                self.emit_elem_copy_sized(ks);
                wasm!(self.func, {
                      // Call f(val) → new val
                      local_get(result); i32_const(4); i32_add;
                      local_get(i); i32_const(entry as i32); i32_mul; i32_add;
                      i32_const(ks as i32); i32_add; // dst val addr
                      local_get(closure); i32_load(4); // env
                      local_get(map_ptr); i32_const(4); i32_add;
                      local_get(i); i32_const(entry as i32); i32_mul; i32_add;
                      i32_const(ks as i32); i32_add;
                });
                self.emit_load_at(&val_ty, 0);
                wasm!(self.func, {
                      local_get(closure); i32_load(0); // table_idx
                });
                self.emit_closure_call(&val_ty, &val_ty);
                // Store result val
                self.emit_store_at(&val_ty, 0);
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
                self.scratch.free_i32(map_ptr);
            }
            "find" => {
                // find(m, pred) → Option[(K, V)]
                let (ks, vs) = self.map_kv_sizes(&args[0].ty);
                let key_ty = self.map_key_ty(&args[0].ty);
                let val_ty = self.map_val_ty(&args[0].ty);
                let entry = ks + vs;
                let map_ptr = self.scratch.alloc_i32();
                let closure = self.scratch.alloc_i32();
                let i = self.scratch.alloc_i32();
                let result = self.scratch.alloc_i32();
                let tuple_ptr = self.scratch.alloc_i32();
                self.emit_expr(&args[0]);
                wasm!(self.func, { local_set(map_ptr); });
                self.emit_expr(&args[1]);
                wasm!(self.func, {
                    local_set(closure);
                    i32_const(0); local_set(i);
                    i32_const(0); local_set(result); // none
                    block_empty; loop_empty;
                      local_get(i); local_get(map_ptr); i32_load(0); i32_ge_u; br_if(1);
                      local_get(closure); i32_load(4);
                      local_get(map_ptr); i32_const(4); i32_add;
                      local_get(i); i32_const(entry as i32); i32_mul; i32_add;
                });
                self.emit_key_load(&key_ty, 0);
                wasm!(self.func, {
                      local_get(map_ptr); i32_const(4); i32_add;
                      local_get(i); i32_const(entry as i32); i32_mul; i32_add;
                      i32_const(ks as i32); i32_add;
                });
                self.emit_load_at(&val_ty, 0);
                wasm!(self.func, {
                      local_get(closure); i32_load(0);
                });
                {
                    let key_vt = Self::key_valtype(&key_ty);
                    let mut ct = vec![ValType::I32, key_vt];
                    if let Some(vt) = values::ty_to_valtype(&val_ty) { ct.push(vt); }
                    self.emit_call_indirect(ct, vec![ValType::I32]);
                }
                wasm!(self.func, {
                      if_empty;
                        // Alloc tuple (key, val) and wrap in Option
                        i32_const(entry as i32); call(self.emitter.rt.alloc); local_set(tuple_ptr);
                        // Copy key
                        local_get(tuple_ptr);
                        local_get(map_ptr); i32_const(4); i32_add;
                        local_get(i); i32_const(entry as i32); i32_mul; i32_add;
                });
                self.emit_elem_copy_sized(ks);
                // Copy val
                wasm!(self.func, {
                        local_get(tuple_ptr); i32_const(ks as i32); i32_add;
                        local_get(map_ptr); i32_const(4); i32_add;
                        local_get(i); i32_const(entry as i32); i32_mul; i32_add;
                        i32_const(ks as i32); i32_add;
                });
                self.emit_elem_copy_sized(vs);
                wasm!(self.func, {
                        // Wrap in some
                        i32_const(4); call(self.emitter.rt.alloc); local_set(result);
                        local_get(result); local_get(tuple_ptr); i32_store(0);
                        br(2); // break out
                      end;
                      local_get(i); i32_const(1); i32_add; local_set(i);
                      br(0);
                    end; end;
                    local_get(result); // result (none=0 or some ptr)
                });
                self.scratch.free_i32(tuple_ptr);
                self.scratch.free_i32(result);
                self.scratch.free_i32(i);
                self.scratch.free_i32(closure);
                self.scratch.free_i32(map_ptr);
            }
            "update" => {
                // update(m, key, f) → Map: apply f to value at key
                let (ks, vs) = self.map_kv_sizes(&args[0].ty);
                let key_ty = self.map_key_ty(&args[0].ty);
                let val_ty = self.map_val_ty(&args[0].ty);
                let entry = ks + vs;
                let map_ptr = self.scratch.alloc_i32();
                let closure = self.scratch.alloc_i32();
                let search_key_i64 = self.scratch.alloc_i64();
                let search_key_i32 = self.scratch.alloc_i32();
                let i = self.scratch.alloc_i32();
                let found_idx = self.scratch.alloc_i32();
                let new_map = self.scratch.alloc_i32();
                let copy_i = self.scratch.alloc_i32();
                self.emit_expr(&args[0]);
                wasm!(self.func, { local_set(map_ptr); });
                // Store search key
                self.emit_expr(&args[1]); // key
                match &key_ty {
                    Ty::Int => {
                        wasm!(self.func, { local_set(search_key_i64); });
                    }
                    _ => {
                        wasm!(self.func, { local_set(search_key_i32); });
                    }
                }
                self.emit_expr(&args[2]); // closure f
                wasm!(self.func, {
                    local_set(closure);
                    // Find key index
                    i32_const(0); local_set(i);
                    i32_const(-1); local_set(found_idx);
                    block_empty; loop_empty;
                      local_get(i); local_get(map_ptr); i32_load(0); i32_ge_u; br_if(1);
                      local_get(map_ptr); i32_const(4); i32_add;
                      local_get(i); i32_const(entry as i32); i32_mul; i32_add;
                });
                self.emit_key_load(&key_ty, 0);
                // Load search key for comparison
                match &key_ty {
                    Ty::Int => {
                        wasm!(self.func, { local_get(search_key_i64); });
                    }
                    _ => {
                        wasm!(self.func, { local_get(search_key_i32); });
                    }
                }
                self.emit_key_eq(&key_ty);
                wasm!(self.func, {
                      if_empty; local_get(i); local_set(found_idx); end;
                      local_get(i); i32_const(1); i32_add; local_set(i);
                      br(0);
                    end; end;
                    // If not found, return original
                    local_get(found_idx); i32_const(0); i32_lt_s;
                    if_i32; local_get(map_ptr);
                    else_;
                      // Copy map, replace value at found_idx with f(old_val)
                      local_get(map_ptr); i32_load(0); local_set(i); // reuse i as len
                      i32_const(4); local_get(i); i32_const(entry as i32); i32_mul; i32_add;
                      call(self.emitter.rt.alloc); local_set(new_map);
                      local_get(new_map); local_get(i); i32_store(0);
                      // Copy all entries
                      i32_const(0); local_set(copy_i);
                      block_empty; loop_empty;
                        local_get(copy_i); local_get(i); i32_ge_u; br_if(1);
                        // Copy key
                        local_get(new_map); i32_const(4); i32_add;
                        local_get(copy_i); i32_const(entry as i32); i32_mul; i32_add;
                        local_get(map_ptr); i32_const(4); i32_add;
                        local_get(copy_i); i32_const(entry as i32); i32_mul; i32_add;
                });
                self.emit_elem_copy_sized(ks);
                // Copy val
                wasm!(self.func, {
                        local_get(new_map); i32_const(4); i32_add;
                        local_get(copy_i); i32_const(entry as i32); i32_mul; i32_add;
                        i32_const(ks as i32); i32_add;
                        local_get(map_ptr); i32_const(4); i32_add;
                        local_get(copy_i); i32_const(entry as i32); i32_mul; i32_add;
                        i32_const(ks as i32); i32_add;
                });
                self.emit_elem_copy_sized(vs);
                wasm!(self.func, {
                        local_get(copy_i); i32_const(1); i32_add; local_set(copy_i);
                        br(0);
                      end; end;
                      // Now apply f to the value at found_idx
                      local_get(new_map); i32_const(4); i32_add;
                      local_get(found_idx); i32_const(entry as i32); i32_mul; i32_add;
                      i32_const(ks as i32); i32_add; // dst val addr
                      local_get(closure); i32_load(4); // env
                      // Load old val
                      local_get(new_map); i32_const(4); i32_add;
                      local_get(found_idx); i32_const(entry as i32); i32_mul; i32_add;
                      i32_const(ks as i32); i32_add;
                });
                self.emit_load_at(&val_ty, 0);
                wasm!(self.func, {
                      local_get(closure); i32_load(0); // table_idx
                });
                self.emit_closure_call(&val_ty, &val_ty);
                self.emit_store_at(&val_ty, 0);
                wasm!(self.func, {
                      local_get(new_map);
                    end;
                });
                self.scratch.free_i32(copy_i);
                self.scratch.free_i32(new_map);
                self.scratch.free_i32(found_idx);
                self.scratch.free_i32(i);
                self.scratch.free_i32(search_key_i32);
                self.scratch.free_i64(search_key_i64);
                self.scratch.free_i32(closure);
                self.scratch.free_i32(map_ptr);
            }
            _ => return false,
        }
        true
    }
}
