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
                let (ks, vs) = self.map_kv_sizes(&args[0].ty);
                let key_ty = self.map_key_ty(&args[0].ty);
                let val_ty = self.map_val_ty(&args[0].ty);
                let acc = self.scratch.alloc_i64();
                let closure = self.scratch.alloc_i32();
                let it = self.map_iter_begin(&args[0], ks + vs);
                self.emit_expr(&args[1]); // init
                wasm!(self.func, { local_set(acc); });
                self.emit_expr(&args[2]); // closure
                wasm!(self.func, { local_set(closure); });
                self.map_iter_loop_head(&it);
                // f(env, acc, key, val)
                wasm!(self.func, { local_get(closure); i32_load(4); local_get(acc); });
                self.map_iter_key_addr(&it);
                self.emit_key_load(&key_ty, 0);
                self.map_iter_val_addr(&it, ks);
                self.emit_load_at(&val_ty, 0);
                wasm!(self.func, { local_get(closure); i32_load(0); });
                {
                    let key_vt = Self::key_valtype(&key_ty);
                    let mut ct = vec![ValType::I32, ValType::I64, key_vt];
                    if let Some(vt) = values::ty_to_valtype(&val_ty) { ct.push(vt); }
                    self.emit_call_indirect(ct, vec![ValType::I64]);
                }
                wasm!(self.func, { local_set(acc); });
                self.map_iter_loop_tail(&it);
                wasm!(self.func, { local_get(acc); });
                self.scratch.free_i64(acc);
                self.scratch.free_i32(closure);
                self.map_iter_end(it);
            }
            "each" => {
                let (ks, vs) = self.map_kv_sizes(&args[0].ty);
                let key_ty = self.map_key_ty(&args[0].ty);
                let val_ty = self.map_val_ty(&args[0].ty);
                let closure = self.scratch.alloc_i32();
                let it = self.map_iter_begin(&args[0], ks + vs);
                self.emit_expr(&args[1]);
                wasm!(self.func, { local_set(closure); });
                self.map_iter_loop_head(&it);
                wasm!(self.func, { local_get(closure); i32_load(4); });
                self.map_iter_key_addr(&it);
                self.emit_key_load(&key_ty, 0);
                self.map_iter_val_addr(&it, ks);
                self.emit_load_at(&val_ty, 0);
                wasm!(self.func, { local_get(closure); i32_load(0); });
                {
                    let key_vt = Self::key_valtype(&key_ty);
                    let mut ct = vec![ValType::I32, key_vt];
                    if let Some(vt) = values::ty_to_valtype(&val_ty) { ct.push(vt); }
                    self.emit_call_indirect(ct, vec![]);
                }
                self.map_iter_loop_tail(&it);
                self.scratch.free_i32(closure);
                self.map_iter_end(it);
            }
            "any" => {
                let (ks, vs) = self.map_kv_sizes(&args[0].ty);
                let key_ty = self.map_key_ty(&args[0].ty);
                let val_ty = self.map_val_ty(&args[0].ty);
                let closure = self.scratch.alloc_i32();
                let result = self.scratch.alloc_i32();
                let it = self.map_iter_begin(&args[0], ks + vs);
                self.emit_expr(&args[1]);
                wasm!(self.func, { local_set(closure); i32_const(0); local_set(result); });
                self.map_iter_loop_head(&it);
                wasm!(self.func, { local_get(closure); i32_load(4); });
                self.map_iter_key_addr(&it);
                self.emit_key_load(&key_ty, 0);
                self.map_iter_val_addr(&it, ks);
                self.emit_load_at(&val_ty, 0);
                wasm!(self.func, { local_get(closure); i32_load(0); });
                {
                    let key_vt = Self::key_valtype(&key_ty);
                    let mut ct = vec![ValType::I32, key_vt];
                    if let Some(vt) = values::ty_to_valtype(&val_ty) { ct.push(vt); }
                    self.emit_call_indirect(ct, vec![ValType::I32]);
                }
                wasm!(self.func, {
                    if_empty; i32_const(1); local_set(result); br(2); end;
                });
                self.map_iter_loop_tail(&it);
                wasm!(self.func, { local_get(result); });
                self.scratch.free_i32(result);
                self.scratch.free_i32(closure);
                self.map_iter_end(it);
            }
            "all" => {
                let (ks, vs) = self.map_kv_sizes(&args[0].ty);
                let key_ty = self.map_key_ty(&args[0].ty);
                let val_ty = self.map_val_ty(&args[0].ty);
                let closure = self.scratch.alloc_i32();
                let result = self.scratch.alloc_i32();
                let it = self.map_iter_begin(&args[0], ks + vs);
                self.emit_expr(&args[1]);
                wasm!(self.func, { local_set(closure); i32_const(1); local_set(result); });
                self.map_iter_loop_head(&it);
                wasm!(self.func, { local_get(closure); i32_load(4); });
                self.map_iter_key_addr(&it);
                self.emit_key_load(&key_ty, 0);
                self.map_iter_val_addr(&it, ks);
                self.emit_load_at(&val_ty, 0);
                wasm!(self.func, { local_get(closure); i32_load(0); });
                {
                    let key_vt = Self::key_valtype(&key_ty);
                    let mut ct = vec![ValType::I32, key_vt];
                    if let Some(vt) = values::ty_to_valtype(&val_ty) { ct.push(vt); }
                    self.emit_call_indirect(ct, vec![ValType::I32]);
                }
                wasm!(self.func, {
                    i32_eqz;
                    if_empty; i32_const(0); local_set(result); br(2); end;
                });
                self.map_iter_loop_tail(&it);
                wasm!(self.func, { local_get(result); });
                self.scratch.free_i32(result);
                self.scratch.free_i32(closure);
                self.map_iter_end(it);
            }
            "count" => {
                let (ks, vs) = self.map_kv_sizes(&args[0].ty);
                let key_ty = self.map_key_ty(&args[0].ty);
                let val_ty = self.map_val_ty(&args[0].ty);
                let closure = self.scratch.alloc_i32();
                let cnt = self.scratch.alloc_i64();
                let it = self.map_iter_begin(&args[0], ks + vs);
                self.emit_expr(&args[1]);
                wasm!(self.func, { local_set(closure); i64_const(0); local_set(cnt); });
                self.map_iter_loop_head(&it);
                wasm!(self.func, { local_get(closure); i32_load(4); });
                self.map_iter_key_addr(&it);
                self.emit_key_load(&key_ty, 0);
                self.map_iter_val_addr(&it, ks);
                self.emit_load_at(&val_ty, 0);
                wasm!(self.func, { local_get(closure); i32_load(0); });
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
                });
                self.map_iter_loop_tail(&it);
                wasm!(self.func, { local_get(cnt); });
                self.scratch.free_i64(cnt);
                self.scratch.free_i32(closure);
                self.map_iter_end(it);
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
                // map(m, f) → Map[K, B]: copy Swiss Table, transform each occupied value
                let (ks, vs) = self.map_kv_sizes(&args[0].ty);
                let val_ty = self.map_val_ty(&args[0].ty);
                let closure = self.scratch.alloc_i32();
                let it = self.map_iter_begin(&args[0], ks + vs);
                self.emit_expr(&args[1]);
                wasm!(self.func, { local_set(closure); });
                let new_map = self.map_copy_full(&it);
                let new_eb = self.map_copy_entry_base(new_map, &it);
                self.map_iter_loop_head(&it);
                // dst = new_eb + i*entry + ks (val addr in copy)
                wasm!(self.func, {
                    local_get(new_eb);
                    local_get(it.i); i32_const((ks + vs) as i32); i32_mul; i32_add;
                    i32_const(ks as i32); i32_add;
                });
                // f(env, old_val) → new_val
                wasm!(self.func, { local_get(closure); i32_load(4); });
                self.map_iter_val_addr(&it, ks);
                self.emit_load_at(&val_ty, 0);
                wasm!(self.func, { local_get(closure); i32_load(0); });
                self.emit_closure_call(&val_ty, &val_ty);
                self.emit_store_at(&val_ty, 0);
                self.map_iter_loop_tail(&it);
                wasm!(self.func, { local_get(new_map); });
                self.scratch.free_i32(new_eb);
                self.scratch.free_i32(new_map);
                self.scratch.free_i32(closure);
                self.map_iter_end(it);
            }
            "find" => {
                // find(m, pred) → Option[(K, V)]
                use super::engine::layout::{SWISS_MAP, map as lm};
                let MAP_TAGS_OFFSET = self.emitter.layout_reg.fixed_offset(SWISS_MAP, lm::TAGS) as i32;
                let MAP_CAP_OFFSET = self.emitter.layout_reg.fixed_offset(SWISS_MAP, lm::CAP);
                let (ks, vs) = self.map_kv_sizes(&args[0].ty);
                let key_ty = self.map_key_ty(&args[0].ty);
                let val_ty = self.map_val_ty(&args[0].ty);
                let entry = ks + vs;
                let map_ptr = self.scratch.alloc_i32();
                let closure = self.scratch.alloc_i32();
                let i = self.scratch.alloc_i32();
                let cap = self.scratch.alloc_i32();
                let eb = self.scratch.alloc_i32();
                let result = self.scratch.alloc_i32();
                let tuple_ptr = self.scratch.alloc_i32();
                self.emit_expr(&args[0]);
                wasm!(self.func, { local_set(map_ptr); });
                self.emit_expr(&args[1]);
                wasm!(self.func, {
                    local_set(closure);
                    local_get(map_ptr); i32_load(MAP_CAP_OFFSET as u32); local_set(cap);
                    local_get(map_ptr); i32_const(MAP_TAGS_OFFSET); i32_add;
                    local_get(cap); i32_add; local_set(eb);
                    i32_const(0); local_set(i);
                    i32_const(0); local_set(result); // none
                    block_empty; loop_empty;
                      local_get(i); local_get(cap); i32_ge_u; br_if(1);
                      local_get(map_ptr); i32_const(MAP_TAGS_OFFSET); i32_add;
                      local_get(i); i32_add; i32_load8_u(0); i32_eqz;
                      if_empty;
                        local_get(i); i32_const(1); i32_add; local_set(i); br(1);
                      end;
                      local_get(closure); i32_load(4);
                      local_get(eb);
                      local_get(i); i32_const(entry as i32); i32_mul; i32_add;
                });
                self.emit_key_load(&key_ty, 0);
                wasm!(self.func, {
                      local_get(eb);
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
                        local_get(eb);
                        local_get(i); i32_const(entry as i32); i32_mul; i32_add;
                });
                self.emit_elem_copy_sized(ks);
                // Copy val
                wasm!(self.func, {
                        local_get(tuple_ptr); i32_const(ks as i32); i32_add;
                        local_get(eb);
                        local_get(i); i32_const(entry as i32); i32_mul; i32_add;
                        i32_const(ks as i32); i32_add;
                });
                self.emit_elem_copy_sized(vs);
                wasm!(self.func, {
                        // Wrap in some
                        i32_const(4); call(self.emitter.rt.alloc); local_set(result);
                        local_get(result); local_get(tuple_ptr); i32_store(0);
                        br(2); // break out of loop
                      end;
                      local_get(i); i32_const(1); i32_add; local_set(i);
                      br(0);
                    end; end;
                    local_get(result); // result (none=0 or some ptr)
                });
                self.scratch.free_i32(tuple_ptr);
                self.scratch.free_i32(result);
                self.scratch.free_i32(eb);
                self.scratch.free_i32(cap);
                self.scratch.free_i32(i);
                self.scratch.free_i32(closure);
                self.scratch.free_i32(map_ptr);
            }
            "update" => {
                // update(m, key, f) → Map: apply f to value at key
                // Swiss Table: memcpy entire map, then find+modify value in the copy.
                use super::engine::layout::{SWISS_MAP, map as lm};
                let MAP_TAGS_OFFSET = self.emitter.layout_reg.fixed_offset(SWISS_MAP, lm::TAGS) as i32;
                let MAP_CAP_OFFSET = self.emitter.layout_reg.fixed_offset(SWISS_MAP, lm::CAP);
                let MAP_HEADER_SIZE = self.emitter.layout_reg.header_size(SWISS_MAP) as i32;
                let (ks, vs) = self.map_kv_sizes(&args[0].ty);
                let key_ty = self.map_key_ty(&args[0].ty);
                let val_ty = self.map_val_ty(&args[0].ty);
                let entry = ks + vs;
                let map_ptr = self.scratch.alloc_i32();
                let closure = self.scratch.alloc_i32();
                let search_key_i64 = self.scratch.alloc_i64();
                let search_key_i32 = self.scratch.alloc_i32();
                let i = self.scratch.alloc_i32();
                let cap = self.scratch.alloc_i32();
                let eb = self.scratch.alloc_i32();
                let found_idx = self.scratch.alloc_i32();
                let new_map = self.scratch.alloc_i32();
                let total_size = self.scratch.alloc_i32();
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
                    local_get(map_ptr); i32_load(MAP_CAP_OFFSET as u32); local_set(cap);
                    local_get(map_ptr); i32_const(MAP_TAGS_OFFSET); i32_add;
                    local_get(cap); i32_add; local_set(eb);
                    // Find key index by scanning occupied slots
                    i32_const(0); local_set(i);
                    i32_const(-1); local_set(found_idx);
                    block_empty; loop_empty;
                      local_get(i); local_get(cap); i32_ge_u; br_if(1);
                      // Skip empty slots
                      local_get(map_ptr); i32_const(MAP_TAGS_OFFSET); i32_add;
                      local_get(i); i32_add; i32_load8_u(0); i32_eqz;
                      if_empty;
                        local_get(i); i32_const(1); i32_add; local_set(i); br(1);
                      end;
                      local_get(eb);
                      local_get(i); i32_const(entry as i32); i32_mul; i32_add;
                });
                self.emit_key_load(&key_ty, 0);
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
                      if_empty; local_get(i); local_set(found_idx); br(2); end; // found → break
                      local_get(i); i32_const(1); i32_add; local_set(i);
                      br(0);
                    end; end;
                    // If not found, return original
                    local_get(found_idx); i32_const(0); i32_lt_s;
                    if_i32; local_get(map_ptr);
                    else_;
                      // Copy entire Swiss Table: header + tags + entries
                      // total_size = MAP_HEADER_SIZE + cap + cap * entry
                      i32_const(MAP_HEADER_SIZE);
                      local_get(cap); i32_add;
                      local_get(cap); i32_const(entry as i32); i32_mul; i32_add;
                      local_tee(total_size);
                      call(self.emitter.rt.alloc); local_set(new_map);
                      // memcpy entire map
                      local_get(new_map); local_get(map_ptr); local_get(total_size);
                      memory_copy;
                      // Compute entry base in new map
                      // new_eb = new_map + MAP_TAGS_OFFSET + cap
                      // Apply f to value at found_idx
                      local_get(new_map); i32_const(MAP_TAGS_OFFSET); i32_add;
                      local_get(cap); i32_add;
                      local_get(found_idx); i32_const(entry as i32); i32_mul; i32_add;
                      i32_const(ks as i32); i32_add; // dst val addr on stack
                      local_get(closure); i32_load(4); // env
                      // Load old val from same addr
                      local_get(new_map); i32_const(MAP_TAGS_OFFSET); i32_add;
                      local_get(cap); i32_add;
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
                self.scratch.free_i32(total_size);
                self.scratch.free_i32(new_map);
                self.scratch.free_i32(found_idx);
                self.scratch.free_i32(eb);
                self.scratch.free_i32(cap);
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

    // (filter method is handled via map.set insertion — see calls_map.rs)
}

// ── Swiss Table iteration helpers ──

/// Scratch locals for iterating a Swiss Table map.
pub(super) struct MapIter {
    pub map: u32,       // map pointer
    pub cap: u32,       // capacity (number of slots)
    pub eb: u32,        // entry base (pointer to first entry, after tags)
    pub i: u32,         // slot index (0..cap)
    pub entry_size: u32, // key_size + val_size
}

impl FuncCompiler<'_> {
    /// Allocate scratch locals and emit Swiss Table setup:
    /// cap = map[CAP_OFFSET], eb = map + TAGS_OFFSET + cap, i = 0.
    pub(super) fn map_iter_begin(&mut self, map_expr: &IrExpr, entry_size: u32) -> MapIter {
        use super::engine::layout::{SWISS_MAP, map as lm};
                let MAP_TAGS_OFFSET = self.emitter.layout_reg.fixed_offset(SWISS_MAP, lm::TAGS) as i32;
                let MAP_CAP_OFFSET = self.emitter.layout_reg.fixed_offset(SWISS_MAP, lm::CAP);
        let map = self.scratch.alloc_i32();
        let cap = self.scratch.alloc_i32();
        let eb = self.scratch.alloc_i32();
        let i = self.scratch.alloc_i32();
        self.emit_expr(map_expr);
        wasm!(self.func, {
            local_set(map);
            local_get(map); i32_load(MAP_CAP_OFFSET as u32); local_set(cap);
            local_get(map); i32_const(MAP_TAGS_OFFSET); i32_add;
            local_get(cap); i32_add; local_set(eb);
            i32_const(0); local_set(i);
        });
        MapIter { map, cap, eb, i, entry_size }
    }

    /// Emit loop header: block/loop, break if i >= cap, skip empty tag.
    /// After this, the current slot is guaranteed occupied.
    pub(super) fn map_iter_loop_head(&mut self, it: &MapIter) {
        use super::engine::layout::{SWISS_MAP, map as lm};
        let MAP_TAGS_OFFSET = self.emitter.layout_reg.fixed_offset(SWISS_MAP, lm::TAGS) as i32;
        wasm!(self.func, {
            block_empty; loop_empty;
              local_get(it.i); local_get(it.cap); i32_ge_u; br_if(1);
              // tag check: skip empty
              local_get(it.map); i32_const(MAP_TAGS_OFFSET); i32_add;
              local_get(it.i); i32_add; i32_load8_u(0); i32_eqz;
              if_empty;
                local_get(it.i); i32_const(1); i32_add; local_set(it.i); br(1);
              end;
        });
    }

    /// Emit: push address of current entry's key onto stack.
    pub(super) fn map_iter_key_addr(&mut self, it: &MapIter) {
        wasm!(self.func, {
            local_get(it.eb);
            local_get(it.i); i32_const(it.entry_size as i32); i32_mul; i32_add;
        });
    }

    /// Emit: push address of current entry's value onto stack.
    pub(super) fn map_iter_val_addr(&mut self, it: &MapIter, key_size: u32) {
        wasm!(self.func, {
            local_get(it.eb);
            local_get(it.i); i32_const(it.entry_size as i32); i32_mul; i32_add;
            i32_const(key_size as i32); i32_add;
        });
    }

    /// Emit loop footer: i++, br(0), end, end.
    pub(super) fn map_iter_loop_tail(&mut self, it: &MapIter) {
        wasm!(self.func, {
            local_get(it.i); i32_const(1); i32_add; local_set(it.i);
            br(0);
          end; end;
        });
    }

    /// Free all scratch locals.
    pub(super) fn map_iter_end(&mut self, it: MapIter) {
        self.scratch.free_i32(it.i);
        self.scratch.free_i32(it.eb);
        self.scratch.free_i32(it.cap);
        self.scratch.free_i32(it.map);
    }

    /// Allocate a full copy of a Swiss Table map (header + tags + entries).
    /// Returns scratch local holding the new map pointer.
    pub(super) fn map_copy_full(&mut self, it: &MapIter) -> u32 {
        use super::engine::layout::{SWISS_MAP, map as lm};
        let MAP_TAGS_OFFSET = self.emitter.layout_reg.fixed_offset(SWISS_MAP, lm::TAGS) as i32;
        let MAP_HEADER_SIZE = self.emitter.layout_reg.header_size(SWISS_MAP) as i32;
        let total = self.scratch.alloc_i32();
        let new_map = self.scratch.alloc_i32();
        wasm!(self.func, {
            // total = HEADER + cap + cap * entry_size
            i32_const(MAP_HEADER_SIZE);
            local_get(it.cap); i32_add;
            local_get(it.cap); i32_const(it.entry_size as i32); i32_mul; i32_add;
            local_tee(total);
            call(self.emitter.rt.alloc); local_set(new_map);
            local_get(new_map); local_get(it.map); local_get(total);
            memory_copy;
        });
        self.scratch.free_i32(total);
        new_map
    }

    /// Compute entry base for a copied map: new_map + TAGS_OFFSET + cap.
    pub(super) fn map_copy_entry_base(&mut self, new_map: u32, it: &MapIter) -> u32 {
        use super::engine::layout::{SWISS_MAP, map as lm};
        let MAP_TAGS_OFFSET = self.emitter.layout_reg.fixed_offset(SWISS_MAP, lm::TAGS) as i32;
        let new_eb = self.scratch.alloc_i32();
        wasm!(self.func, {
            local_get(new_map); i32_const(MAP_TAGS_OFFSET); i32_add;
            local_get(it.cap); i32_add; local_set(new_eb);
        });
        new_eb
    }
}
