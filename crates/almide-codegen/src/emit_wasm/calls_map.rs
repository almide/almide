//! Map stdlib call dispatch for WASM codegen.
//!
//! Hash table layout: [len:i32][cap:i32][slots @ 8...]
//! Each slot: [tag:i32][key:K][val:V]  tag: 0=empty, 1=occupied
//! Cap is always a power of 2. Resize at 75% load factor.

use super::FuncCompiler;
use super::values;
use super::list_layout;
use almide_ir::IrExpr;
use almide_lang::types::Ty;
use wasm_encoder::ValType;

impl FuncCompiler<'_> {
    pub(super) fn emit_map_call(&mut self, method: &str, args: &[IrExpr]) -> bool {
        match method {
            "new" => {
                self.emit_map_new_hash();
            }
            "len" | "length" | "size" => {
                self.emit_expr(&args[0]);
                wasm!(self.func, { i32_load(0); i64_extend_i32_u; });
            }
            "is_empty" => {
                self.emit_expr(&args[0]);
                wasm!(self.func, { i32_load(0); i32_eqz; });
            }
            "get" => {
                let (ks, vs) = self.map_kv_sizes(&args[0].ty);
                let key_ty = self.map_key_ty(&args[0].ty);
                let slot_size = (4 + ks + vs) as i32;
                let map_ptr = self.scratch.alloc_i32();
                let sk_i32 = self.scratch.alloc_i32();
                let sk_i64 = self.scratch.alloc_i64();
                let cap = self.scratch.alloc_i32();
                let idx = self.scratch.alloc_i32();
                let slot_ptr = self.scratch.alloc_i32();
                let result = self.scratch.alloc_i32();
                let val_copy = self.scratch.alloc_i32();

                self.emit_expr(&args[0]);
                wasm!(self.func, { local_set(map_ptr); });
                self.emit_expr(&args[1]);
                self.emit_search_key_store(&key_ty, sk_i32, sk_i64);

                wasm!(self.func, {
                    i32_const(0); local_set(result);
                    local_get(map_ptr); i32_load(list_layout::MAP_CAP_OFFSET as u32); local_set(cap);
                    // If cap == 0 (empty map), skip probe
                    local_get(cap); i32_eqz;
                    if_empty; // empty → result stays 0 (none)
                    else_;
                });

                // Hash key → h2 + bucket index
                let h2 = self.scratch.alloc_i32();
                let tag_local = self.scratch.alloc_i32();
                self.emit_search_key_load(&key_ty, sk_i32, sk_i64);
                self.emit_hash_key(&key_ty);
                let hash_tmp = self.scratch.alloc_i32();
                wasm!(self.func, {
                    local_tee(hash_tmp);
                    local_get(cap); i32_const(1); i32_sub; i32_and;
                    local_set(idx);
                    local_get(hash_tmp);
                });
                self.emit_h2_from_hash();
                wasm!(self.func, { local_set(h2); });
                self.scratch.free_i32(hash_tmp);
                // Probe loop with h2 filter
                wasm!(self.func, {
                    block_empty; loop_empty;
                      local_get(map_ptr); i32_const(list_layout::MAP_DATA_OFFSET); i32_add;
                      local_get(idx); i32_const(slot_size); i32_mul; i32_add;
                      local_set(slot_ptr);
                      local_get(slot_ptr); i32_load(0); local_set(tag_local);
                      local_get(tag_local); i32_eqz; br_if(1); // empty → not found
                      // h2 filter
                      local_get(tag_local); local_get(h2); i32_eq;
                      if_empty;
                        local_get(slot_ptr); i32_const(4); i32_add;
                });
                self.emit_key_load(&key_ty, 0);
                self.emit_search_key_load(&key_ty, sk_i32, sk_i64);
                self.emit_key_eq(&key_ty);
                wasm!(self.func, {
                        if_empty;
                          // Found
                          i32_const(vs as i32); call(self.emitter.rt.alloc); local_set(val_copy);
                          local_get(val_copy);
                          local_get(slot_ptr); i32_const(4 + ks as i32); i32_add;
                });
                self.emit_elem_copy_sized(vs);
                wasm!(self.func, {
                          local_get(val_copy); local_set(result);
                          br(3); // exit: if→if→loop→block
                        end;
                      end; // end h2 check
                      local_get(idx); i32_const(1); i32_add;
                      local_get(cap); i32_const(1); i32_sub; i32_and;
                      local_set(idx);
                      br(0);
                    end; end;
                    end; // end cap != 0 check
                    local_get(result);
                });

                self.scratch.free_i32(tag_local);
                self.scratch.free_i32(h2);
                self.scratch.free_i32(val_copy);
                self.scratch.free_i32(result);
                self.scratch.free_i32(slot_ptr);
                self.scratch.free_i32(idx);
                self.scratch.free_i32(cap);
                self.scratch.free_i64(sk_i64);
                self.scratch.free_i32(sk_i32);
                self.scratch.free_i32(map_ptr);
            }
            "get_or" => {
                let (ks, vs) = self.map_kv_sizes(&args[0].ty);
                let key_ty = self.map_key_ty(&args[0].ty);
                let val_ty = self.map_val_ty(&args[0].ty);
                let slot_size = (4 + ks + vs) as i32;
                let vt = values::ty_to_valtype(&val_ty).unwrap_or(ValType::I32);
                let map_ptr = self.scratch.alloc_i32();
                let sk_i32 = self.scratch.alloc_i32();
                let sk_i64 = self.scratch.alloc_i64();
                let cap = self.scratch.alloc_i32();
                let idx = self.scratch.alloc_i32();
                let slot_ptr = self.scratch.alloc_i32();
                let found = self.scratch.alloc_i32();

                self.emit_expr(&args[0]);
                wasm!(self.func, { local_set(map_ptr); });
                self.emit_expr(&args[1]);
                self.emit_search_key_store(&key_ty, sk_i32, sk_i64);

                wasm!(self.func, {
                    i32_const(0); local_set(found);
                    local_get(map_ptr); i32_load(list_layout::MAP_CAP_OFFSET as u32); local_set(cap);
                });
                self.emit_search_key_load(&key_ty, sk_i32, sk_i64);
                self.emit_hash_key(&key_ty);
                wasm!(self.func, {
                    local_get(cap); i32_const(1); i32_sub; i32_and;
                    local_set(idx);
                    block_empty; loop_empty;
                      local_get(map_ptr); i32_const(list_layout::MAP_DATA_OFFSET); i32_add;
                      local_get(idx); i32_const(slot_size); i32_mul; i32_add;
                      local_set(slot_ptr);
                      local_get(slot_ptr); i32_load(0); i32_eqz; br_if(1);
                      local_get(slot_ptr); i32_const(4); i32_add;
                });
                self.emit_key_load(&key_ty, 0);
                self.emit_search_key_load(&key_ty, sk_i32, sk_i64);
                self.emit_key_eq(&key_ty);
                wasm!(self.func, {
                      if_empty; i32_const(1); local_set(found); br(2); end;
                      local_get(idx); i32_const(1); i32_add;
                      local_get(cap); i32_const(1); i32_sub; i32_and;
                      local_set(idx);
                      br(0);
                    end; end;
                    local_get(found); i32_eqz;
                });
                match vt {
                    ValType::I64 => { wasm!(self.func, { if_i64; }); }
                    ValType::F64 => { wasm!(self.func, { if_f64; }); }
                    _ => { wasm!(self.func, { if_i32; }); }
                }
                self.emit_expr(&args[2]); // default
                wasm!(self.func, { else_; });
                wasm!(self.func, {
                    local_get(slot_ptr); i32_const(4 + ks as i32); i32_add;
                });
                self.emit_load_at(&val_ty, 0);
                wasm!(self.func, { end; });

                self.scratch.free_i32(found);
                self.scratch.free_i32(slot_ptr);
                self.scratch.free_i32(idx);
                self.scratch.free_i32(cap);
                self.scratch.free_i64(sk_i64);
                self.scratch.free_i32(sk_i32);
                self.scratch.free_i32(map_ptr);
            }
            "contains" => {
                let (ks, vs) = self.map_kv_sizes(&args[0].ty);
                let key_ty = self.map_key_ty(&args[0].ty);
                let slot_size = (4 + ks + vs) as i32;
                let map_ptr = self.scratch.alloc_i32();
                let sk_i32 = self.scratch.alloc_i32();
                let sk_i64 = self.scratch.alloc_i64();
                let cap = self.scratch.alloc_i32();
                let idx = self.scratch.alloc_i32();
                let slot_ptr = self.scratch.alloc_i32();
                let result = self.scratch.alloc_i32();

                self.emit_expr(&args[0]);
                wasm!(self.func, { local_set(map_ptr); });
                self.emit_expr(&args[1]);
                self.emit_search_key_store(&key_ty, sk_i32, sk_i64);

                wasm!(self.func, {
                    i32_const(0); local_set(result);
                    local_get(map_ptr); i32_load(list_layout::MAP_CAP_OFFSET as u32); local_set(cap);
                    local_get(cap); i32_eqz;
                    if_empty; // cap == 0 → result stays 0
                    else_;
                });
                self.emit_search_key_load(&key_ty, sk_i32, sk_i64);
                self.emit_hash_key(&key_ty);
                wasm!(self.func, {
                    local_get(cap); i32_const(1); i32_sub; i32_and;
                    local_set(idx);
                    block_empty; loop_empty;
                      local_get(map_ptr); i32_const(list_layout::MAP_DATA_OFFSET); i32_add;
                      local_get(idx); i32_const(slot_size); i32_mul; i32_add;
                      local_set(slot_ptr);
                      local_get(slot_ptr); i32_load(0); i32_eqz; br_if(1);
                      local_get(slot_ptr); i32_const(4); i32_add;
                });
                self.emit_key_load(&key_ty, 0);
                self.emit_search_key_load(&key_ty, sk_i32, sk_i64);
                self.emit_key_eq(&key_ty);
                wasm!(self.func, {
                      if_empty; i32_const(1); local_set(result); br(2); end;
                      local_get(idx); i32_const(1); i32_add;
                      local_get(cap); i32_const(1); i32_sub; i32_and;
                      local_set(idx);
                      br(0);
                    end; end;
                    end; // end cap != 0
                    local_get(result);
                });

                self.scratch.free_i32(result);
                self.scratch.free_i32(slot_ptr);
                self.scratch.free_i32(idx);
                self.scratch.free_i32(cap);
                self.scratch.free_i64(sk_i64);
                self.scratch.free_i32(sk_i32);
                self.scratch.free_i32(map_ptr);
            }
            "set" => {
                // set(m, key, val) → new Map (immutable)
                // Copy hash table, then insert into copy
                let (ks, vs) = self.map_kv_sizes(&args[0].ty);
                let key_ty = self.map_key_ty(&args[0].ty);
                let val_ty = self.map_val_ty(&args[0].ty);
                let slot_size = (4 + ks + vs) as i32;
                let vt = values::ty_to_valtype(&val_ty).unwrap_or(ValType::I32);
                let map_ptr = self.scratch.alloc_i32();
                let sk_i32 = self.scratch.alloc_i32();
                let sk_i64 = self.scratch.alloc_i64();
                let sv = self.scratch.alloc(vt);
                let cap = self.scratch.alloc_i32();
                let new_map = self.scratch.alloc_i32();
                let idx = self.scratch.alloc_i32();
                let slot_ptr = self.scratch.alloc_i32();
                let tag = self.scratch.alloc_i32();

                self.emit_expr(&args[0]);
                wasm!(self.func, { local_set(map_ptr); });
                self.emit_expr(&args[1]);
                self.emit_search_key_store(&key_ty, sk_i32, sk_i64);
                self.emit_expr(&args[2]);
                wasm!(self.func, { local_set(sv); });

                // Copy entire hash table (handle cap==0)
                wasm!(self.func, {
                    local_get(map_ptr); i32_load(list_layout::MAP_CAP_OFFSET as u32); local_set(cap);
                    local_get(cap); i32_eqz;
                    if_empty;
                      i32_const(list_layout::MAP_INITIAL_CAP); local_set(cap);
                    end;
                    // total = MAP_HEADER_SIZE + cap * slot_size
                    i32_const(list_layout::MAP_HEADER_SIZE);
                    local_get(cap); i32_const(slot_size); i32_mul; i32_add;
                    local_tee(idx); // reuse idx as total temporarily
                    call(self.emitter.rt.alloc); local_set(new_map);
                    local_get(new_map); i32_const(0); i32_store(0); // len = 0 initially
                    local_get(new_map); local_get(cap); i32_store(list_layout::MAP_CAP_OFFSET as u32);
                    // Copy old data if it exists
                    local_get(map_ptr); i32_load(list_layout::MAP_CAP_OFFSET as u32);
                    if_empty;
                      local_get(new_map); local_get(map_ptr);
                      i32_const(list_layout::MAP_HEADER_SIZE);
                      local_get(map_ptr); i32_load(list_layout::MAP_CAP_OFFSET as u32); i32_const(slot_size); i32_mul; i32_add;
                      memory_copy;
                    end;
                });

                // Insert into copy: hash, probe, insert/update
                self.emit_search_key_load(&key_ty, sk_i32, sk_i64);
                self.emit_hash_key(&key_ty);
                wasm!(self.func, {
                    local_get(cap); i32_const(1); i32_sub; i32_and;
                    local_set(idx);
                    block_empty; loop_empty;
                      local_get(new_map); i32_const(list_layout::MAP_DATA_OFFSET); i32_add;
                      local_get(idx); i32_const(slot_size); i32_mul; i32_add;
                      local_set(slot_ptr);
                      local_get(slot_ptr); i32_load(0); local_set(tag);
                      local_get(tag); i32_eqz; br_if(1);
                      local_get(slot_ptr); i32_const(4); i32_add;
                });
                self.emit_key_load(&key_ty, 0);
                self.emit_search_key_load(&key_ty, sk_i32, sk_i64);
                self.emit_key_eq(&key_ty);
                wasm!(self.func, {
                      br_if(1);
                      local_get(idx); i32_const(1); i32_add;
                      local_get(cap); i32_const(1); i32_sub; i32_and;
                      local_set(idx); br(0);
                    end; end;
                    // Store value
                    local_get(slot_ptr); i32_const(4 + ks as i32); i32_add;
                    local_get(sv);
                });
                match vt {
                    ValType::I64 => { wasm!(self.func, { i64_store(0); }); }
                    ValType::F64 => { wasm!(self.func, { f64_store(0); }); }
                    _ => { wasm!(self.func, { i32_store(0); }); }
                }
                // If new entry
                wasm!(self.func, {
                    local_get(tag); i32_eqz;
                    if_empty;
                      local_get(slot_ptr); i32_const(1); i32_store(0);
                      local_get(slot_ptr); i32_const(4); i32_add;
                });
                self.emit_search_key_load(&key_ty, sk_i32, sk_i64);
                self.emit_key_store(&key_ty, 0);
                wasm!(self.func, {
                      local_get(new_map);
                      local_get(new_map); i32_load(0); i32_const(1); i32_add;
                      i32_store(0);
                    end;
                    local_get(new_map);
                });

                self.scratch.free_i32(tag);
                self.scratch.free_i32(slot_ptr);
                self.scratch.free_i32(idx);
                self.scratch.free_i32(new_map);
                self.scratch.free_i32(cap);
                self.scratch.free(sv, vt);
                self.scratch.free_i64(sk_i64);
                self.scratch.free_i32(sk_i32);
                self.scratch.free_i32(map_ptr);
            }
            "insert" => {
                // insert(mut m, key, val) → Unit. In-place hash insert with resize.
                let (ks, vs) = self.map_kv_sizes(&args[0].ty);
                let key_ty = self.map_key_ty(&args[0].ty);
                let val_ty = self.map_val_ty(&args[0].ty);
                let slot_size = (4 + ks + vs) as i32;
                let vt = values::ty_to_valtype(&val_ty).unwrap_or(ValType::I32);
                let map_ptr = self.scratch.alloc_i32();
                let sk_i32 = self.scratch.alloc_i32();
                let sk_i64 = self.scratch.alloc_i64();
                let sv = self.scratch.alloc(vt);
                let cap = self.scratch.alloc_i32();
                let idx = self.scratch.alloc_i32();
                let slot_ptr = self.scratch.alloc_i32();
                let tag = self.scratch.alloc_i32();

                self.emit_expr(&args[0]);
                wasm!(self.func, { local_set(map_ptr); });
                self.emit_expr(&args[1]);
                self.emit_search_key_store(&key_ty, sk_i32, sk_i64);
                self.emit_expr(&args[2]);
                wasm!(self.func, { local_set(sv); });

                wasm!(self.func, {
                    local_get(map_ptr); i32_load(list_layout::MAP_CAP_OFFSET as u32); local_set(cap);
                    // If cap == 0 (first insert), allocate initial table
                    local_get(cap); i32_eqz;
                    if_empty;
                      i32_const(list_layout::MAP_INITIAL_CAP); local_set(cap);
                      i32_const(list_layout::MAP_HEADER_SIZE);
                      i32_const(list_layout::MAP_INITIAL_CAP); i32_const(slot_size); i32_mul; i32_add;
                      call(self.emitter.rt.alloc); local_set(map_ptr);
                      local_get(map_ptr); i32_const(0); i32_store(0); // len = 0
                      local_get(map_ptr); i32_const(list_layout::MAP_INITIAL_CAP); i32_store(list_layout::MAP_CAP_OFFSET as u32);
                    end;
                });

                // Hash key → h2 + bucket index
                let h2 = self.scratch.alloc_i32();
                self.emit_search_key_load(&key_ty, sk_i32, sk_i64);
                self.emit_hash_key(&key_ty);
                // Stack: hash. Dup for h2 and idx.
                let hash_tmp = self.scratch.alloc_i32();
                wasm!(self.func, {
                    local_tee(hash_tmp);
                    local_get(cap); i32_const(1); i32_sub; i32_and;
                    local_set(idx);
                    local_get(hash_tmp);
                });
                self.emit_h2_from_hash();
                wasm!(self.func, { local_set(h2); });
                self.scratch.free_i32(hash_tmp);
                // Probe loop
                wasm!(self.func, {
                    block_empty; loop_empty;
                      local_get(map_ptr); i32_const(list_layout::MAP_DATA_OFFSET); i32_add;
                      local_get(idx); i32_const(slot_size); i32_mul; i32_add;
                      local_set(slot_ptr);
                      local_get(slot_ptr); i32_load(0); local_set(tag);
                      local_get(tag); i32_eqz; br_if(1); // tag == 0 → empty
                      // h2 filter: only compare key if tag == h2
                      local_get(tag); local_get(h2); i32_eq;
                      if_empty;
                        local_get(slot_ptr); i32_const(4); i32_add;
                });
                self.emit_key_load(&key_ty, 0);
                self.emit_search_key_load(&key_ty, sk_i32, sk_i64);
                self.emit_key_eq(&key_ty);
                wasm!(self.func, {
                        br_if(2); // found → exit probe loop (if=0, loop=1, block=2)
                      end; // end h2 check
                      local_get(idx); i32_const(1); i32_add;
                      local_get(cap); i32_const(1); i32_sub; i32_and;
                      local_set(idx); br(0);
                    end; end;
                });
                // Store value
                wasm!(self.func, {
                    local_get(slot_ptr); i32_const(4 + ks as i32); i32_add;
                    local_get(sv);
                });
                match vt {
                    ValType::I64 => { wasm!(self.func, { i64_store(0); }); }
                    ValType::F64 => { wasm!(self.func, { f64_store(0); }); }
                    _ => { wasm!(self.func, { i32_store(0); }); }
                }
                // If new entry (tag was 0 = empty)
                wasm!(self.func, {
                    local_get(tag); i32_eqz;
                    if_empty;
                      local_get(slot_ptr); local_get(h2); i32_store(0); // tag = h2
                      local_get(slot_ptr); i32_const(4); i32_add;
                });
                self.emit_search_key_load(&key_ty, sk_i32, sk_i64);
                self.emit_key_store(&key_ty, 0);
                wasm!(self.func, {
                      // len++
                      local_get(map_ptr);
                      local_get(map_ptr); i32_load(0); i32_const(1); i32_add;
                      i32_store(0);
                      // Resize check: len * 4 > cap * 3
                      local_get(map_ptr); i32_load(0); i32_const(4); i32_mul;
                      local_get(cap); i32_const(3); i32_mul;
                      i32_gt_u;
                      if_empty;
                });
                // Resize
                self.emit_map_resize(&key_ty, ks, vs, map_ptr, cap);
                wasm!(self.func, {
                      end; // end resize check
                    end; // end new entry check
                });
                // Write back map_ptr to var
                if let almide_ir::IrExprKind::Var { id } = &args[0].kind {
                    if let Some(&local_idx) = self.var_map.get(&id.0) {
                        wasm!(self.func, { local_get(map_ptr); local_set(local_idx); });
                    }
                }

                self.scratch.free_i32(h2);
                self.scratch.free_i32(tag);
                self.scratch.free_i32(slot_ptr);
                self.scratch.free_i32(idx);
                self.scratch.free_i32(cap);
                self.scratch.free(sv, vt);
                self.scratch.free_i64(sk_i64);
                self.scratch.free_i32(sk_i32);
                self.scratch.free_i32(map_ptr);
            }
            "remove" | "delete" => {
                // For delete (mut): rebuild without key using iterate+insert pattern
                // For remove (immutable): same but return new map
                let is_delete = method == "delete";
                let (ks, vs) = self.map_kv_sizes(&args[0].ty);
                let key_ty = self.map_key_ty(&args[0].ty);
                let slot_size = (4 + ks + vs) as i32;
                let map_ptr = self.scratch.alloc_i32();
                let sk_i32 = self.scratch.alloc_i32();
                let sk_i64 = self.scratch.alloc_i64();
                let cap = self.scratch.alloc_i32();
                let new_map = self.scratch.alloc_i32();
                let i = self.scratch.alloc_i32();
                let old_slot = self.scratch.alloc_i32();

                self.emit_expr(&args[0]);
                wasm!(self.func, { local_set(map_ptr); });
                self.emit_expr(&args[1]);
                self.emit_search_key_store(&key_ty, sk_i32, sk_i64);

                // Build new map by iterating old slots and inserting non-matching entries
                wasm!(self.func, {
                    local_get(map_ptr); i32_load(list_layout::MAP_CAP_OFFSET as u32); local_set(cap);
                    // Allocate new map with same capacity
                    i32_const(list_layout::MAP_HEADER_SIZE);
                    local_get(cap); i32_const(slot_size); i32_mul; i32_add;
                    call(self.emitter.rt.alloc); local_set(new_map);
                    local_get(new_map); i32_const(0); i32_store(0); // len = 0
                    local_get(new_map); local_get(cap); i32_store(list_layout::MAP_CAP_OFFSET as u32);
                });
                wasm!(self.func, {
                    i32_const(0); local_set(i);
                    block_empty; loop_empty;
                      local_get(i); local_get(cap); i32_ge_u; br_if(1);
                      local_get(map_ptr); i32_const(list_layout::MAP_DATA_OFFSET); i32_add;
                      local_get(i); i32_const(slot_size); i32_mul; i32_add;
                      local_set(old_slot);
                      local_get(old_slot); i32_load(0); // tag
                      if_empty; // occupied
                        // Compare key
                        local_get(old_slot); i32_const(4); i32_add;
                });
                self.emit_key_load(&key_ty, 0);
                self.emit_search_key_load(&key_ty, sk_i32, sk_i64);
                self.emit_key_eq(&key_ty);
                wasm!(self.func, {
                        i32_eqz; // NOT matching → copy this entry
                        if_empty;
                          // Copy slot to new_map (hash-insert without resize)
                          local_get(old_slot); i32_const(4); i32_add;
                });
                self.emit_key_load(&key_ty, 0); // load key for hashing
                self.emit_hash_key(&key_ty);
                // Probe new_map for empty slot
                let nidx = self.scratch.alloc_i32();
                let nslot = self.scratch.alloc_i32();
                wasm!(self.func, {
                          local_get(cap); i32_const(1); i32_sub; i32_and; local_set(nidx);
                          block_empty; loop_empty;
                            local_get(new_map); i32_const(list_layout::MAP_DATA_OFFSET); i32_add;
                            local_get(nidx); i32_const(slot_size); i32_mul; i32_add;
                            local_set(nslot);
                            local_get(nslot); i32_load(0); i32_eqz; br_if(1);
                            local_get(nidx); i32_const(1); i32_add;
                            local_get(cap); i32_const(1); i32_sub; i32_and;
                            local_set(nidx); br(0);
                          end; end;
                          // Copy slot
                          local_get(nslot); local_get(old_slot); i32_const(slot_size); memory_copy;
                          // new_map.len++
                          local_get(new_map);
                          local_get(new_map); i32_load(0); i32_const(1); i32_add;
                          i32_store(0);
                        end; // end if not matching
                      end; // end if occupied
                      local_get(i); i32_const(1); i32_add; local_set(i); br(0);
                    end; end;
                });
                self.scratch.free_i32(nslot);
                self.scratch.free_i32(nidx);

                if is_delete {
                    if let almide_ir::IrExprKind::Var { id } = &args[0].kind {
                        if let Some(&local_idx) = self.var_map.get(&id.0) {
                            wasm!(self.func, { local_get(new_map); local_set(local_idx); });
                        }
                    }
                } else {
                    wasm!(self.func, { local_get(new_map); });
                }

                self.scratch.free_i32(old_slot);
                self.scratch.free_i32(i);
                self.scratch.free_i32(new_map);
                self.scratch.free_i32(cap);
                self.scratch.free_i64(sk_i64);
                self.scratch.free_i32(sk_i32);
                self.scratch.free_i32(map_ptr);
            }
            "keys" => {
                let (ks, vs) = self.map_kv_sizes(&args[0].ty);
                let slot_size = (4 + ks + vs) as i32;
                let map_ptr = self.scratch.alloc_i32();
                let len = self.scratch.alloc_i32();
                let cap = self.scratch.alloc_i32();
                let result = self.scratch.alloc_i32();
                let i = self.scratch.alloc_i32();
                let j = self.scratch.alloc_i32();
                let slot_ptr = self.scratch.alloc_i32();

                self.emit_expr(&args[0]);
                wasm!(self.func, {
                    local_set(map_ptr);
                    local_get(map_ptr); i32_load(0); local_set(len);
                    local_get(map_ptr); i32_load(list_layout::MAP_CAP_OFFSET as u32); local_set(cap);
                    i32_const(list_layout::HEADER_SIZE); local_get(len); i32_const(ks as i32); i32_mul; i32_add;
                    call(self.emitter.rt.alloc); local_set(result);
                    local_get(result); local_get(len); i32_store(0);
                    i32_const(0); local_set(i);
                    i32_const(0); local_set(j);
                    block_empty; loop_empty;
                      local_get(i); local_get(cap); i32_ge_u; br_if(1);
                      local_get(map_ptr); i32_const(list_layout::MAP_DATA_OFFSET); i32_add;
                      local_get(i); i32_const(slot_size); i32_mul; i32_add;
                      local_set(slot_ptr);
                      local_get(slot_ptr); i32_load(0); // tag
                      if_empty;
                        // Copy key to result[j]
                        local_get(result); i32_const(list_layout::DATA_OFFSET); i32_add;
                        local_get(j); i32_const(ks as i32); i32_mul; i32_add;
                        local_get(slot_ptr); i32_const(4); i32_add;
                });
                self.emit_elem_copy_sized(ks);
                wasm!(self.func, {
                        local_get(j); i32_const(1); i32_add; local_set(j);
                      end;
                      local_get(i); i32_const(1); i32_add; local_set(i);
                      br(0);
                    end; end;
                    local_get(result);
                });

                self.scratch.free_i32(slot_ptr);
                self.scratch.free_i32(j);
                self.scratch.free_i32(i);
                self.scratch.free_i32(result);
                self.scratch.free_i32(cap);
                self.scratch.free_i32(len);
                self.scratch.free_i32(map_ptr);
            }
            "values" => {
                let (ks, vs) = self.map_kv_sizes(&args[0].ty);
                let slot_size = (4 + ks + vs) as i32;
                let map_ptr = self.scratch.alloc_i32();
                let len = self.scratch.alloc_i32();
                let cap = self.scratch.alloc_i32();
                let result = self.scratch.alloc_i32();
                let i = self.scratch.alloc_i32();
                let j = self.scratch.alloc_i32();
                let slot_ptr = self.scratch.alloc_i32();

                self.emit_expr(&args[0]);
                wasm!(self.func, {
                    local_set(map_ptr);
                    local_get(map_ptr); i32_load(0); local_set(len);
                    local_get(map_ptr); i32_load(list_layout::MAP_CAP_OFFSET as u32); local_set(cap);
                    i32_const(list_layout::HEADER_SIZE); local_get(len); i32_const(vs as i32); i32_mul; i32_add;
                    call(self.emitter.rt.alloc); local_set(result);
                    local_get(result); local_get(len); i32_store(0);
                    i32_const(0); local_set(i);
                    i32_const(0); local_set(j);
                    block_empty; loop_empty;
                      local_get(i); local_get(cap); i32_ge_u; br_if(1);
                      local_get(map_ptr); i32_const(list_layout::MAP_DATA_OFFSET); i32_add;
                      local_get(i); i32_const(slot_size); i32_mul; i32_add;
                      local_set(slot_ptr);
                      local_get(slot_ptr); i32_load(0);
                      if_empty;
                        local_get(result); i32_const(list_layout::DATA_OFFSET); i32_add;
                        local_get(j); i32_const(vs as i32); i32_mul; i32_add;
                        local_get(slot_ptr); i32_const(4 + ks as i32); i32_add;
                });
                self.emit_elem_copy_sized(vs);
                wasm!(self.func, {
                        local_get(j); i32_const(1); i32_add; local_set(j);
                      end;
                      local_get(i); i32_const(1); i32_add; local_set(i);
                      br(0);
                    end; end;
                    local_get(result);
                });

                self.scratch.free_i32(slot_ptr);
                self.scratch.free_i32(j);
                self.scratch.free_i32(i);
                self.scratch.free_i32(result);
                self.scratch.free_i32(cap);
                self.scratch.free_i32(len);
                self.scratch.free_i32(map_ptr);
            }
            "entries" => {
                let (ks, vs) = self.map_kv_sizes(&args[0].ty);
                let entry = ks + vs;
                let slot_size = (4 + entry) as i32;
                let map_ptr = self.scratch.alloc_i32();
                let len = self.scratch.alloc_i32();
                let cap = self.scratch.alloc_i32();
                let result = self.scratch.alloc_i32();
                let i = self.scratch.alloc_i32();
                let j = self.scratch.alloc_i32();
                let slot_ptr = self.scratch.alloc_i32();
                let tuple_ptr = self.scratch.alloc_i32();

                self.emit_expr(&args[0]);
                wasm!(self.func, {
                    local_set(map_ptr);
                    local_get(map_ptr); i32_load(0); local_set(len);
                    local_get(map_ptr); i32_load(list_layout::MAP_CAP_OFFSET as u32); local_set(cap);
                    i32_const(list_layout::HEADER_SIZE); local_get(len); i32_const(4); i32_mul; i32_add;
                    call(self.emitter.rt.alloc); local_set(result);
                    local_get(result); local_get(len); i32_store(0);
                    i32_const(0); local_set(i);
                    i32_const(0); local_set(j);
                    block_empty; loop_empty;
                      local_get(i); local_get(cap); i32_ge_u; br_if(1);
                      local_get(map_ptr); i32_const(list_layout::MAP_DATA_OFFSET); i32_add;
                      local_get(i); i32_const(slot_size); i32_mul; i32_add;
                      local_set(slot_ptr);
                      local_get(slot_ptr); i32_load(0);
                      if_empty;
                        // Alloc tuple and copy key+val
                        i32_const(entry as i32); call(self.emitter.rt.alloc); local_set(tuple_ptr);
                        local_get(tuple_ptr);
                        local_get(slot_ptr); i32_const(4); i32_add;
                        i32_const(entry as i32); memory_copy;
                        // Store tuple ptr in result list
                        local_get(result); i32_const(list_layout::DATA_OFFSET); i32_add;
                        local_get(j); i32_const(4); i32_mul; i32_add;
                        local_get(tuple_ptr); i32_store(0);
                        local_get(j); i32_const(1); i32_add; local_set(j);
                      end;
                      local_get(i); i32_const(1); i32_add; local_set(i);
                      br(0);
                    end; end;
                    local_get(result);
                });

                self.scratch.free_i32(tuple_ptr);
                self.scratch.free_i32(slot_ptr);
                self.scratch.free_i32(j);
                self.scratch.free_i32(i);
                self.scratch.free_i32(result);
                self.scratch.free_i32(cap);
                self.scratch.free_i32(len);
                self.scratch.free_i32(map_ptr);
            }
            "merge" => {
                // merge(a, b) → new map. Build from a, then insert each b entry.
                let (ks, vs) = self.map_kv_sizes(&args[0].ty);
                let key_ty = self.map_key_ty(&args[0].ty);
                let val_ty = self.map_val_ty(&args[0].ty);
                let slot_size = (4 + ks + vs) as i32;
                let vt = values::ty_to_valtype(&val_ty).unwrap_or(ValType::I32);
                let map_a = self.scratch.alloc_i32();
                let map_b = self.scratch.alloc_i32();
                let cap_a = self.scratch.alloc_i32();
                let cap_b = self.scratch.alloc_i32();
                let result = self.scratch.alloc_i32();
                let i = self.scratch.alloc_i32();
                let slot_ptr = self.scratch.alloc_i32();
                let r_cap = self.scratch.alloc_i32();
                let r_idx = self.scratch.alloc_i32();
                let r_slot = self.scratch.alloc_i32();
                let r_tag = self.scratch.alloc_i32();

                self.emit_expr(&args[0]);
                wasm!(self.func, { local_set(map_a); });
                self.emit_expr(&args[1]);
                wasm!(self.func, {
                    local_set(map_b);
                    local_get(map_a); i32_load(list_layout::MAP_CAP_OFFSET as u32); local_set(cap_a);
                    local_get(map_b); i32_load(list_layout::MAP_CAP_OFFSET as u32); local_set(cap_b);
                });

                // Result cap = max(cap_a, next_pow2((a.len + b.len) * 2))
                // Simplified: just use cap_a + cap_b (will be a power of 2 if both are)
                // Actually just copy a's table to start, then insert b's entries
                let total_sz = self.scratch.alloc_i32();
                wasm!(self.func, {
                    // Copy a
                    i32_const(list_layout::MAP_HEADER_SIZE);
                    local_get(cap_a); i32_const(slot_size); i32_mul; i32_add;
                    local_tee(total_sz);
                    call(self.emitter.rt.alloc); local_set(result);
                    local_get(result); local_get(map_a); local_get(total_sz); memory_copy;
                    local_get(result); i32_load(list_layout::MAP_CAP_OFFSET as u32); local_set(r_cap);
                    // Insert each b entry
                    i32_const(0); local_set(i);
                    block_empty; loop_empty;
                      local_get(i); local_get(cap_b); i32_ge_u; br_if(1);
                      local_get(map_b); i32_const(list_layout::MAP_DATA_OFFSET); i32_add;
                      local_get(i); i32_const(slot_size); i32_mul; i32_add;
                      local_set(slot_ptr);
                      local_get(slot_ptr); i32_load(0); // tag
                      if_empty;
                        // Hash b's key
                        local_get(slot_ptr); i32_const(4); i32_add;
                });
                self.emit_key_load(&key_ty, 0);
                self.emit_hash_key(&key_ty);
                wasm!(self.func, {
                        local_get(r_cap); i32_const(1); i32_sub; i32_and; local_set(r_idx);
                        // Probe result
                        block_empty; loop_empty;
                          local_get(result); i32_const(list_layout::MAP_DATA_OFFSET); i32_add;
                          local_get(r_idx); i32_const(slot_size); i32_mul; i32_add;
                          local_set(r_slot);
                          local_get(r_slot); i32_load(0); local_set(r_tag);
                          local_get(r_tag); i32_eqz; br_if(1);
                          local_get(r_slot); i32_const(4); i32_add;
                });
                self.emit_key_load(&key_ty, 0);
                wasm!(self.func, {
                          local_get(slot_ptr); i32_const(4); i32_add;
                });
                self.emit_key_load(&key_ty, 0);
                self.emit_key_eq(&key_ty);
                wasm!(self.func, {
                          br_if(1);
                          local_get(r_idx); i32_const(1); i32_add;
                          local_get(r_cap); i32_const(1); i32_sub; i32_and;
                          local_set(r_idx); br(0);
                        end; end;
                        // Copy slot from b
                        local_get(r_slot); local_get(slot_ptr); i32_const(slot_size); memory_copy;
                        // If new entry, len++
                        local_get(r_tag); i32_eqz;
                        if_empty;
                          local_get(result);
                          local_get(result); i32_load(0); i32_const(1); i32_add;
                          i32_store(0);
                        end;
                      end; // end if occupied
                      local_get(i); i32_const(1); i32_add; local_set(i); br(0);
                    end; end;
                    local_get(result);
                });

                self.scratch.free_i32(total_sz);
                self.scratch.free_i32(r_tag);
                self.scratch.free_i32(r_slot);
                self.scratch.free_i32(r_idx);
                self.scratch.free_i32(r_cap);
                self.scratch.free_i32(slot_ptr);
                self.scratch.free_i32(i);
                self.scratch.free_i32(result);
                self.scratch.free_i32(cap_b);
                self.scratch.free_i32(cap_a);
                self.scratch.free_i32(map_b);
                self.scratch.free_i32(map_a);
            }
            "from_list" => {
                // Build map from list of (K,V) tuples by repeated insert
                let pair_ty = self.resolve_list_elem(&args[0], None);
                let (ks, vs, key_ty) = if let Ty::Tuple(elems) = &pair_ty {
                    let k = elems.first().map(|t| values::byte_size(t)).unwrap_or(4);
                    let v = elems.get(1).map(|t| values::byte_size(t)).unwrap_or(4);
                    let kt = elems.first().cloned().unwrap_or(Ty::String);
                    (k, v, kt)
                } else { (4u32, 4u32, Ty::String) };
                let val_ty = if let Ty::Tuple(elems) = &pair_ty {
                    elems.get(1).cloned().unwrap_or(Ty::Int)
                } else { Ty::Int };
                let slot_size = (4 + ks + vs) as i32;
                let vt = values::ty_to_valtype(&val_ty).unwrap_or(ValType::I32);
                let pairs = self.scratch.alloc_i32();
                let len = self.scratch.alloc_i32();
                let result = self.scratch.alloc_i32();
                let i = self.scratch.alloc_i32();
                let tuple_ptr = self.scratch.alloc_i32();
                let cap = self.scratch.alloc_i32();
                let idx = self.scratch.alloc_i32();
                let slot_ptr = self.scratch.alloc_i32();
                let tag = self.scratch.alloc_i32();

                self.emit_expr(&args[0]);
                wasm!(self.func, {
                    local_set(pairs);
                    local_get(pairs); i32_load(0); local_set(len);
                    // cap = next_pow2(len * 2), min 16
                    i32_const(list_layout::MAP_INITIAL_CAP); local_set(cap);
                    block_empty; loop_empty;
                      local_get(cap); local_get(len); i32_const(1); i32_shl; i32_ge_u; br_if(1);
                      local_get(cap); i32_const(1); i32_shl; local_set(cap);
                      br(0);
                    end; end;
                    // Allocate
                    i32_const(list_layout::MAP_HEADER_SIZE);
                    local_get(cap); i32_const(slot_size); i32_mul; i32_add;
                    call(self.emitter.rt.alloc); local_set(result);
                    local_get(result); i32_const(0); i32_store(0);
                    local_get(result); local_get(cap); i32_store(list_layout::MAP_CAP_OFFSET as u32);
                });
                wasm!(self.func, {
                    i32_const(0); local_set(i);
                    block_empty; loop_empty;
                      local_get(i); local_get(len); i32_ge_u; br_if(1);
                      local_get(pairs); i32_const(list_layout::DATA_OFFSET); i32_add;
                      local_get(i); i32_const(4); i32_mul; i32_add;
                      i32_load(0); local_set(tuple_ptr);
                      // Hash key from tuple
                      local_get(tuple_ptr);
                });
                self.emit_key_load(&key_ty, 0);
                self.emit_hash_key(&key_ty);
                wasm!(self.func, {
                      local_get(cap); i32_const(1); i32_sub; i32_and; local_set(idx);
                      // Probe
                      block_empty; loop_empty;
                        local_get(result); i32_const(list_layout::MAP_DATA_OFFSET); i32_add;
                        local_get(idx); i32_const(slot_size); i32_mul; i32_add;
                        local_set(slot_ptr);
                        local_get(slot_ptr); i32_load(0); local_set(tag);
                        local_get(tag); i32_eqz; br_if(1);
                        local_get(slot_ptr); i32_const(4); i32_add;
                });
                self.emit_key_load(&key_ty, 0);
                wasm!(self.func, {
                        local_get(tuple_ptr);
                });
                self.emit_key_load(&key_ty, 0);
                self.emit_key_eq(&key_ty);
                wasm!(self.func, {
                        br_if(1);
                        local_get(idx); i32_const(1); i32_add;
                        local_get(cap); i32_const(1); i32_sub; i32_and;
                        local_set(idx); br(0);
                      end; end;
                      // Copy key+val from tuple to slot
                      local_get(slot_ptr); i32_const(1); i32_store(0); // tag = 1
                      local_get(slot_ptr); i32_const(4); i32_add;
                      local_get(tuple_ptr);
                      i32_const(ks as i32 + vs as i32); memory_copy;
                      // If new entry, len++
                      local_get(tag); i32_eqz;
                      if_empty;
                        local_get(result);
                        local_get(result); i32_load(0); i32_const(1); i32_add;
                        i32_store(0);
                      end;
                      local_get(i); i32_const(1); i32_add; local_set(i); br(0);
                    end; end;
                    local_get(result);
                });

                self.scratch.free_i32(tag);
                self.scratch.free_i32(slot_ptr);
                self.scratch.free_i32(idx);
                self.scratch.free_i32(cap);
                self.scratch.free_i32(tuple_ptr);
                self.scratch.free_i32(i);
                self.scratch.free_i32(result);
                self.scratch.free_i32(len);
                self.scratch.free_i32(pairs);
            }
            "clear" => {
                self.emit_map_new_hash();
                if let almide_ir::IrExprKind::Var { id } = &args[0].kind {
                    if let Some(&local_idx) = self.var_map.get(&id.0) {
                        wasm!(self.func, { local_set(local_idx); });
                    } else {
                        wasm!(self.func, { drop; });
                    }
                } else {
                    wasm!(self.func, { drop; });
                }
            }
            _ => return self.emit_map_closure_call(method, args),
        }
        true
    }

    // ── Hash table helpers ──

    /// Emit: create empty hash map with default capacity (16 slots).
    fn emit_map_new_hash(&mut self) {
        let init_cap = list_layout::MAP_INITIAL_CAP;
        let scratch = self.scratch.alloc_i32();
        // Slot size doesn't matter for empty map — allocate header + cap * max_slot
        // Actually, we don't know slot_size here. Use a fixed initial allocation.
        // The map is type-erased at this point, so allocate header + cap * 0 is wrong.
        // We'll allocate a generous size. Type-specific slot size is resolved at first insert.
        // For now, allocate header only; the insert will resize if needed.
        // Actually, we need cap slots even if empty, because probe needs them.
        // But we don't know slot_size... Let me just allocate with slot_size=0 placeholders.
        //
        // PROBLEM: map.new() doesn't know the type. But all callers will eventually
        // call insert/set with specific types. We need to allocate with correct slot size.
        //
        // SOLUTION: For map.new(), allocate with a slot_size of 0 and cap=0.
        // On first insert, detect cap==0 and allocate properly.
        // OR: use a sentinel cap value.
        //
        // SIMPLEST: Allocate just the header with cap=0. Insert will handle allocation.
        wasm!(self.func, {
            i32_const(list_layout::MAP_HEADER_SIZE);
            call(self.emitter.rt.alloc);
            local_set(scratch);
            local_get(scratch); i32_const(0); i32_store(0); // len = 0
            local_get(scratch); i32_const(0); i32_store(list_layout::MAP_CAP_OFFSET as u32); // cap = 0
            local_get(scratch);
        });
        self.scratch.free_i32(scratch);
    }

    /// Emit: create hash map with given capacity local into result local.
    fn emit_map_new_hash_with_cap(&mut self, result: u32, cap_local: u32) {
        // We don't know slot_size here. Use a dummy allocation.
        // Actually this is called from remove/delete where we know the type.
        // But the method signature doesn't carry slot_size.
        // Let me pass slot_size... this is getting complicated.
        //
        // For now, use the same approach as new: cap=0 header only.
        // The insert loop in remove/delete uses the cap from the input map.
        // So we need the new map to have the SAME cap and slot layout.
        //
        // SOLUTION: This method should take slot_size as parameter.
        // But that changes the API. Let me inline it at call sites instead.
        // For now, this is unused — the remove/delete code handles it directly.
        panic!("emit_map_new_hash_with_cap should not be called directly");
    }

    /// Emit: create hash map with capacity >= len*2 for from_list.
    fn emit_map_new_hash_for_len(&mut self, result: u32, cap_local: u32, _len_local: u32) {
        // Start with cap=16, we'll rely on resize during insert if needed.
        // For from_list, we should pre-allocate but we don't know slot_size here.
        // The caller has slot_size. So this just allocates header with cap=16.
        // The caller inlines the slot allocation.
        //
        // Actually, from_list's caller DOES know the slot_size. So let me
        // have the caller compute total_size and allocate directly.
        // This method is a placeholder.
        panic!("emit_map_new_hash_for_len should not be called directly");
    }

    /// Emit: compute h2 tag (upper 7 bits of hash) from i32 hash on stack.
    /// Leaves i32 h2 (0x01..0x7F) on stack. Never 0 (0 = empty tag).
    fn emit_h2_from_hash(&mut self) {
        let h2 = self.scratch.alloc_i32();
        wasm!(self.func, {
            i32_const(25); i32_shr_u; i32_const(0x7F); i32_and;
            local_tee(h2);
            i32_eqz;
            if_i32; i32_const(1); // 0 → 1 (avoid collision with empty tag)
            else_; local_get(h2);
            end;
        });
        self.scratch.free_i32(h2);
    }

    /// Emit: hash a key value on the WASM stack, producing i32 hash.
    pub(super) fn emit_hash_key(&mut self, key_ty: &Ty) {
        match key_ty {
            Ty::Int => {
                // h = wrap((key XOR (key >> 32)) * golden_ratio)
                let tmp = self.scratch.alloc_i64();
                wasm!(self.func, {
                    local_tee(tmp);
                    i64_const(32); i64_shr_u;
                    local_get(tmp); i64_xor;
                    i64_const(0x9E3779B97F4A7C15u64 as i64); i64_mul;
                    i64_const(32); i64_shr_u;
                    i32_wrap_i64;
                });
                self.scratch.free_i64(tmp);
            }
            Ty::String => {
                // FNV-1a over string bytes
                let s = self.scratch.alloc_i32();
                let h = self.scratch.alloc_i32();
                let slen = self.scratch.alloc_i32();
                let si = self.scratch.alloc_i32();
                wasm!(self.func, {
                    local_set(s);
                    i32_const(0x811C9DC5u32 as i32); local_set(h); // FNV offset basis
                    local_get(s); i32_load(0); local_set(slen);
                    i32_const(0); local_set(si);
                    block_empty; loop_empty;
                      local_get(si); local_get(slen); i32_ge_u; br_if(1);
                      local_get(h);
                      local_get(s); i32_const(list_layout::STRING_DATA_OFFSET); i32_add;
                      local_get(si); i32_add; i32_load8_u(0);
                      i32_xor;
                      i32_const(0x01000193u32 as i32); i32_mul; // FNV prime
                      local_set(h);
                      local_get(si); i32_const(1); i32_add; local_set(si);
                      br(0);
                    end; end;
                    local_get(h);
                });
                self.scratch.free_i32(si);
                self.scratch.free_i32(slen);
                self.scratch.free_i32(h);
                self.scratch.free_i32(s);
            }
            Ty::Bool => {
                // Bool is i32 (0 or 1), use as hash directly
            }
            _ => {
                // Pointer types: use pointer value as hash
            }
        }
    }

    /// Emit resize: double the capacity, rehash all entries.
    /// Updates map_ptr local in-place.
    fn emit_map_resize(&mut self, key_ty: &Ty, ks: u32, vs: u32, map_ptr: u32, cap: u32) {
        let slot_size = (4 + ks + vs) as i32;
        let new_cap = self.scratch.alloc_i32();
        let new_map = self.scratch.alloc_i32();
        let ri = self.scratch.alloc_i32();
        let old_slot = self.scratch.alloc_i32();
        let new_idx = self.scratch.alloc_i32();
        let new_slot = self.scratch.alloc_i32();

        wasm!(self.func, {
            local_get(cap); i32_const(1); i32_shl; local_set(new_cap); // new_cap = cap * 2
            i32_const(list_layout::MAP_HEADER_SIZE);
            local_get(new_cap); i32_const(slot_size); i32_mul; i32_add;
            call(self.emitter.rt.alloc); local_set(new_map);
            local_get(new_map); local_get(map_ptr); i32_load(0); i32_store(0); // len
            local_get(new_map); local_get(new_cap); i32_store(list_layout::MAP_CAP_OFFSET as u32); // cap
            // Rehash loop
            i32_const(0); local_set(ri);
            block_empty; loop_empty;
              local_get(ri); local_get(cap); i32_ge_u; br_if(1);
              local_get(map_ptr); i32_const(list_layout::MAP_DATA_OFFSET); i32_add;
              local_get(ri); i32_const(slot_size); i32_mul; i32_add;
              local_set(old_slot);
              local_get(old_slot); i32_load(0); // tag
              if_empty;
                // Hash key from old slot
                local_get(old_slot); i32_const(4); i32_add;
        });
        self.emit_key_load(key_ty, 0);
        self.emit_hash_key(key_ty);
        wasm!(self.func, {
                local_get(new_cap); i32_const(1); i32_sub; i32_and; local_set(new_idx);
                // Probe for empty slot in new map
                block_empty; loop_empty;
                  local_get(new_map); i32_const(list_layout::MAP_DATA_OFFSET); i32_add;
                  local_get(new_idx); i32_const(slot_size); i32_mul; i32_add;
                  local_set(new_slot);
                  local_get(new_slot); i32_load(0); i32_eqz; br_if(1);
                  local_get(new_idx); i32_const(1); i32_add;
                  local_get(new_cap); i32_const(1); i32_sub; i32_and;
                  local_set(new_idx); br(0);
                end; end;
                // Copy slot
                local_get(new_slot); local_get(old_slot); i32_const(slot_size); memory_copy;
              end; // end if occupied
              local_get(ri); i32_const(1); i32_add; local_set(ri); br(0);
            end; end;
            local_get(new_map); local_set(map_ptr);
        });

        self.scratch.free_i32(new_slot);
        self.scratch.free_i32(new_idx);
        self.scratch.free_i32(old_slot);
        self.scratch.free_i32(ri);
        self.scratch.free_i32(new_map);
        self.scratch.free_i32(new_cap);
    }

    // ── Map helpers ──

    pub(super) fn map_kv_sizes(&self, ty: &Ty) -> (u32, u32) {
        if let Ty::Applied(_, args) = ty {
            let ks = args.first().map(|t| values::byte_size(t)).unwrap_or(4);
            let vs = args.get(1).map(|t| values::byte_size(t)).unwrap_or(4);
            (ks, vs)
        } else { (4, 4) }
    }

    pub(super) fn map_val_ty(&self, ty: &Ty) -> Ty {
        if let Ty::Applied(_, args) = ty {
            args.get(1).cloned().unwrap_or(Ty::Int)
        } else { Ty::Int }
    }

    pub(super) fn map_key_ty(&self, ty: &Ty) -> Ty {
        if let Ty::Applied(_, args) = ty {
            args.first().cloned().unwrap_or(Ty::String)
        } else { Ty::String }
    }

    pub(super) fn emit_key_load(&mut self, key_ty: &Ty, offset: u32) {
        match key_ty {
            Ty::Int => { wasm!(self.func, { i64_load(offset); }); }
            _ => { wasm!(self.func, { i32_load(offset); }); }
        }
    }

    pub(super) fn emit_key_store(&mut self, key_ty: &Ty, offset: u32) {
        match key_ty {
            Ty::Int => { wasm!(self.func, { i64_store(offset); }); }
            _ => { wasm!(self.func, { i32_store(offset); }); }
        }
    }

    pub(super) fn emit_key_eq(&mut self, key_ty: &Ty) {
        match key_ty {
            Ty::Int => { wasm!(self.func, { i64_eq; }); }
            Ty::String => { wasm!(self.func, { call(self.emitter.rt.string.eq); }); }
            Ty::Bool => { wasm!(self.func, { i32_eq; }); }
            _ => { wasm!(self.func, { i32_eq; }); }
        }
    }

    pub(super) fn emit_search_key_store(&mut self, key_ty: &Ty, scratch_i32: u32, scratch_i64: u32) {
        match key_ty {
            Ty::Int => { wasm!(self.func, { local_set(scratch_i64); }); }
            _ => { wasm!(self.func, { local_set(scratch_i32); }); }
        }
    }

    pub(super) fn emit_search_key_load(&mut self, key_ty: &Ty, scratch_i32: u32, scratch_i64: u32) {
        match key_ty {
            Ty::Int => { wasm!(self.func, { local_get(scratch_i64); }); }
            _ => { wasm!(self.func, { local_get(scratch_i32); }); }
        }
    }

    pub(super) fn key_valtype(key_ty: &Ty) -> ValType {
        match key_ty {
            Ty::Int => ValType::I64,
            _ => ValType::I32,
        }
    }

    pub(super) fn emit_elem_copy_sized(&mut self, size: u32) {
        match size {
            8 => { wasm!(self.func, { i64_load(0); i64_store(0); }); }
            4 => { wasm!(self.func, { i32_load(0); i32_store(0); }); }
            _ => { wasm!(self.func, { i32_load(0); i32_store(0); }); }
        }
    }

    fn emit_entry_copy(&mut self, entry_size: u32) {
        match entry_size {
            8 => { wasm!(self.func, { i64_load(0); i64_store(0); }); }
            12 => {
                let dst = self.scratch.alloc_i32();
                let src = self.scratch.alloc_i32();
                wasm!(self.func, {
                    local_set(src); local_set(dst);
                    local_get(dst); local_get(src); i32_load(0); i32_store(0);
                    local_get(dst); local_get(src); i32_load(4); i32_store(4);
                    local_get(dst); local_get(src); i32_load(8); i32_store(8);
                });
                self.scratch.free_i32(src);
                self.scratch.free_i32(dst);
            }
            _ => {
                let dst = self.scratch.alloc_i32();
                let src = self.scratch.alloc_i32();
                wasm!(self.func, { local_set(src); local_set(dst); });
                let words = (entry_size + 3) / 4;
                for w in 0..words {
                    let off = w * 4;
                    wasm!(self.func, {
                        local_get(dst); local_get(src);
                        i32_load(off); i32_store(off);
                    });
                }
                self.scratch.free_i32(src);
                self.scratch.free_i32(dst);
            }
        }
    }
}
