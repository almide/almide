//! Map stdlib call dispatch for WASM codegen.
//!
//! Map layout: [len:i32][key0:K][val0:V][key1:K][val1:V]...
//! Key comparison is type-aware: string.eq for String, i64_eq for Int, i32_eq for Bool.

use super::FuncCompiler;
use super::values;
use almide_ir::IrExpr;
use almide_lang::types::Ty;
use wasm_encoder::ValType;

impl FuncCompiler<'_> {
    pub(super) fn emit_map_call(&mut self, method: &str, args: &[IrExpr]) -> bool {
        match method {
            "new" => {
                // map.new() → empty map [len=0]
                let result = self.scratch.alloc_i32();
                wasm!(self.func, {
                    i32_const(4); call(self.emitter.rt.alloc); local_set(result);
                    local_get(result); i32_const(0); i32_store(0);
                    local_get(result);
                });
                self.scratch.free_i32(result);
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
                // get(m, key) → Option[V]
                //
                // Emits an Option pointer (i32, where 0 = none, non-zero = some(val_ptr)).
                // Must use `br` to exit the loop on match — NOT `return_` — because
                // this emit is inlined into the caller's function body, and the
                // caller may have a different return type.
                let (ks, vs) = self.map_kv_sizes(&args[0].ty);
                let key_ty = self.map_key_ty(&args[0].ty);
                let _val_ty = self.map_val_ty(&args[0].ty);
                let entry = ks + vs;
                let map_ptr = self.scratch.alloc_i32();
                let sk_i32 = self.scratch.alloc_i32();
                let sk_i64 = self.scratch.alloc_i64();
                let i = self.scratch.alloc_i32();
                let val_copy = self.scratch.alloc_i32();
                let result = self.scratch.alloc_i32();
                self.emit_expr(&args[0]);
                wasm!(self.func, { local_set(map_ptr); });
                self.emit_expr(&args[1]); // key
                self.emit_search_key_store(&key_ty, sk_i32, sk_i64);
                wasm!(self.func, {
                    i32_const(0); local_set(result); // default: none
                    i32_const(0); local_set(i);
                    block_empty; loop_empty;
                      local_get(i); local_get(map_ptr); i32_load(0); i32_ge_u; br_if(1);
                      // Compare map_key[i] with search key
                      local_get(map_ptr); i32_const(4); i32_add;
                      local_get(i); i32_const(entry as i32); i32_mul; i32_add;
                });
                self.emit_key_load(&key_ty, 0);
                self.emit_search_key_load(&key_ty, sk_i32, sk_i64);
                self.emit_key_eq(&key_ty);
                wasm!(self.func, {
                      if_empty;
                        // Found: build some(val) and stash in `result`, then exit loop
                        i32_const(vs as i32); call(self.emitter.rt.alloc); local_set(val_copy);
                        local_get(val_copy);
                        local_get(map_ptr); i32_const(4); i32_add;
                        local_get(i); i32_const(entry as i32); i32_mul; i32_add;
                        i32_const(ks as i32); i32_add; // val offset
                });
                self.emit_elem_copy_sized(vs);
                wasm!(self.func, {
                        local_get(val_copy); local_set(result); br(2);
                      end;
                      local_get(i); i32_const(1); i32_add; local_set(i);
                      br(0);
                    end; end;
                    local_get(result);
                });
                self.scratch.free_i32(result);
                self.scratch.free_i32(val_copy);
                self.scratch.free_i32(i);
                self.scratch.free_i64(sk_i64);
                self.scratch.free_i32(sk_i32);
                self.scratch.free_i32(map_ptr);
            }
            "get_or" => {
                // get_or(m, key, default) → V
                //
                // If key found → return map value. Else → return default.
                // Must use `br` to exit the loop, NOT `return_` — inline emit.
                let (ks, vs) = self.map_kv_sizes(&args[0].ty);
                let key_ty = self.map_key_ty(&args[0].ty);
                let val_ty = self.map_val_ty(&args[0].ty);
                let entry = ks + vs;
                let vt = values::ty_to_valtype(&val_ty).unwrap_or(ValType::I32);
                let map_ptr = self.scratch.alloc_i32();
                let sk_i32 = self.scratch.alloc_i32();
                let sk_i64 = self.scratch.alloc_i64();
                let i = self.scratch.alloc_i32();
                let found = self.scratch.alloc_i32();
                self.emit_expr(&args[0]);
                wasm!(self.func, { local_set(map_ptr); });
                self.emit_expr(&args[1]); // key
                self.emit_search_key_store(&key_ty, sk_i32, sk_i64);
                wasm!(self.func, {
                    i32_const(0); local_set(found);
                    i32_const(0); local_set(i);
                    block_empty; loop_empty;
                      local_get(i); local_get(map_ptr); i32_load(0); i32_ge_u; br_if(1);
                      local_get(map_ptr); i32_const(4); i32_add;
                      local_get(i); i32_const(entry as i32); i32_mul; i32_add;
                });
                self.emit_key_load(&key_ty, 0);
                self.emit_search_key_load(&key_ty, sk_i32, sk_i64);
                self.emit_key_eq(&key_ty);
                wasm!(self.func, {
                      if_empty;
                        i32_const(1); local_set(found);
                        br(2);
                      end;
                      local_get(i); i32_const(1); i32_add; local_set(i);
                      br(0);
                    end; end;
                });
                // If found → load val at (map_ptr + 4 + i*entry + ks); else → emit default.
                wasm!(self.func, {
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
                    local_get(map_ptr); i32_const(4); i32_add;
                    local_get(i); i32_const(entry as i32); i32_mul; i32_add;
                    i32_const(ks as i32); i32_add;
                });
                self.emit_load_at(&val_ty, 0);
                wasm!(self.func, { end; });
                self.scratch.free_i32(found);
                self.scratch.free_i32(i);
                self.scratch.free_i64(sk_i64);
                self.scratch.free_i32(sk_i32);
                self.scratch.free_i32(map_ptr);
            }
            "contains" => {
                let (ks, vs) = self.map_kv_sizes(&args[0].ty);
                let key_ty = self.map_key_ty(&args[0].ty);
                let entry = ks + vs;
                let map_ptr = self.scratch.alloc_i32();
                let sk_i32 = self.scratch.alloc_i32();
                let sk_i64 = self.scratch.alloc_i64();
                let i = self.scratch.alloc_i32();
                self.emit_expr(&args[0]);
                wasm!(self.func, { local_set(map_ptr); });
                self.emit_expr(&args[1]);
                // Inline emit: stash result in a local, use `br` to exit (not return_).
                let contains_result = self.scratch.alloc_i32();
                self.emit_search_key_store(&key_ty, sk_i32, sk_i64);
                wasm!(self.func, {
                    i32_const(0); local_set(contains_result); // default: not found
                    i32_const(0); local_set(i);
                    block_empty; loop_empty;
                      local_get(i); local_get(map_ptr); i32_load(0); i32_ge_u; br_if(1);
                      local_get(map_ptr); i32_const(4); i32_add;
                      local_get(i); i32_const(entry as i32); i32_mul; i32_add;
                });
                self.emit_key_load(&key_ty, 0);
                self.emit_search_key_load(&key_ty, sk_i32, sk_i64);
                self.emit_key_eq(&key_ty);
                wasm!(self.func, {
                      if_empty;
                        i32_const(1); local_set(contains_result);
                        br(2);
                      end;
                      local_get(i); i32_const(1); i32_add; local_set(i);
                      br(0);
                    end; end;
                    local_get(contains_result);
                });
                self.scratch.free_i32(contains_result);
                self.scratch.free_i32(i);
                self.scratch.free_i64(sk_i64);
                self.scratch.free_i32(sk_i32);
                self.scratch.free_i32(map_ptr);
            }
            "set" => {
                // set(m, key, value) → new Map
                // Copy existing entries, update if key exists, append if new
                let (ks, vs) = self.map_kv_sizes(&args[0].ty);
                let key_ty = self.map_key_ty(&args[0].ty);
                let val_ty = self.map_val_ty(&args[0].ty);
                let entry = ks + vs;
                let vt = values::ty_to_valtype(&val_ty).unwrap_or(ValType::I32);
                let map_ptr = self.scratch.alloc_i32();
                let sk_i32 = self.scratch.alloc_i32();
                let sk_i64 = self.scratch.alloc_i64();
                let val_scratch = self.scratch.alloc(vt);
                let old_len = self.scratch.alloc_i32();
                let found_idx = self.scratch.alloc_i32();
                let i = self.scratch.alloc_i32();
                let new_map = self.scratch.alloc_i32();
                self.emit_expr(&args[0]);
                wasm!(self.func, { local_set(map_ptr); });
                // Store search key
                self.emit_expr(&args[1]); // key
                self.emit_search_key_store(&key_ty, sk_i32, sk_i64);
                // Store value in scratch local
                self.emit_expr(&args[2]); // value
                wasm!(self.func, { local_set(val_scratch); });
                wasm!(self.func, {
                    // Find if key exists
                    local_get(map_ptr); i32_load(0); local_set(old_len);
                    i32_const(-1); local_set(found_idx); // found_idx = -1
                    i32_const(0); local_set(i);
                    block_empty; loop_empty;
                      local_get(i); local_get(old_len); i32_ge_u; br_if(1);
                      local_get(map_ptr); i32_const(4); i32_add;
                      local_get(i); i32_const(entry as i32); i32_mul; i32_add;
                });
                self.emit_key_load(&key_ty, 0);
                self.emit_search_key_load(&key_ty, sk_i32, sk_i64);
                self.emit_key_eq(&key_ty);
                wasm!(self.func, {
                      if_empty;
                        local_get(i); local_set(found_idx); // found
                      end;
                      local_get(i); i32_const(1); i32_add; local_set(i);
                      br(0);
                    end; end;
                    // new_len = found >= 0 ? old_len : old_len + 1
                    local_get(found_idx); i32_const(0); i32_lt_s; i32_eqz;
                    if_i32; local_get(old_len); else_; local_get(old_len); i32_const(1); i32_add; end;
                    local_set(i); // reuse i as new_len
                    // Alloc new map
                    i32_const(4); local_get(i); i32_const(entry as i32); i32_mul; i32_add;
                    call(self.emitter.rt.alloc); local_set(new_map);
                    local_get(new_map); local_get(i); i32_store(0);
                });
                // Copy old entries, replacing found_idx
                wasm!(self.func, {
                    i32_const(0); local_set(i); // i
                    block_empty; loop_empty;
                      local_get(i); local_get(old_len); i32_ge_u; br_if(1);
                      // Copy key: dst entry, src entry
                      local_get(new_map); i32_const(4); i32_add;
                      local_get(i); i32_const(entry as i32); i32_mul; i32_add;
                      local_get(map_ptr); i32_const(4); i32_add;
                      local_get(i); i32_const(entry as i32); i32_mul; i32_add;
                });
                self.emit_elem_copy_sized(ks);
                // Copy value: if this is the found_idx, use new value from scratch
                wasm!(self.func, {
                      local_get(i); local_get(found_idx); i32_eq;
                      if_empty;
                        // Replace value from scratch local
                        local_get(new_map); i32_const(4); i32_add;
                        local_get(i); i32_const(entry as i32); i32_mul; i32_add;
                        i32_const(ks as i32); i32_add;
                        local_get(val_scratch);
                });
                match vt {
                    ValType::I64 => { wasm!(self.func, { i64_store(0); }); }
                    ValType::F64 => { wasm!(self.func, { f64_store(0); }); }
                    _ => { wasm!(self.func, { i32_store(0); }); }
                }
                wasm!(self.func, {
                      else_;
                        // Copy original value
                        local_get(new_map); i32_const(4); i32_add;
                        local_get(i); i32_const(entry as i32); i32_mul; i32_add;
                        i32_const(ks as i32); i32_add;
                        local_get(map_ptr); i32_const(4); i32_add;
                        local_get(i); i32_const(entry as i32); i32_mul; i32_add;
                        i32_const(ks as i32); i32_add;
                });
                self.emit_elem_copy_sized(vs);
                wasm!(self.func, {
                      end;
                      local_get(i); i32_const(1); i32_add; local_set(i);
                      br(0);
                    end; end;
                });
                // If key was new, append at end
                wasm!(self.func, {
                    local_get(found_idx); i32_const(0); i32_lt_s;
                    if_empty;
                      // Append key: dst[old_len]
                      local_get(new_map); i32_const(4); i32_add;
                      local_get(old_len); i32_const(entry as i32); i32_mul; i32_add;
                });
                self.emit_search_key_load(&key_ty, sk_i32, sk_i64);
                self.emit_key_store(&key_ty, 0);
                // Append value
                wasm!(self.func, {
                      local_get(new_map); i32_const(4); i32_add;
                      local_get(old_len); i32_const(entry as i32); i32_mul; i32_add;
                      i32_const(ks as i32); i32_add;
                      local_get(val_scratch);
                });
                match vt {
                    ValType::I64 => { wasm!(self.func, { i64_store(0); }); }
                    ValType::F64 => { wasm!(self.func, { f64_store(0); }); }
                    _ => { wasm!(self.func, { i32_store(0); }); }
                }
                wasm!(self.func, {
                    end;
                    local_get(new_map);
                });
                self.scratch.free_i32(new_map);
                self.scratch.free_i32(i);
                self.scratch.free_i32(found_idx);
                self.scratch.free_i32(old_len);
                self.scratch.free(val_scratch, vt);
                self.scratch.free_i64(sk_i64);
                self.scratch.free_i32(sk_i32);
                self.scratch.free_i32(map_ptr);
            }
            "remove" => {
                let (ks, vs) = self.map_kv_sizes(&args[0].ty);
                let key_ty = self.map_key_ty(&args[0].ty);
                let entry = ks + vs;
                let map_ptr = self.scratch.alloc_i32();
                let sk_i32 = self.scratch.alloc_i32();
                let sk_i64 = self.scratch.alloc_i64();
                let old_len = self.scratch.alloc_i32();
                let found_idx = self.scratch.alloc_i32();
                let src_i = self.scratch.alloc_i32();
                let new_map = self.scratch.alloc_i32();
                let dst_i = self.scratch.alloc_i32();
                self.emit_expr(&args[0]);
                wasm!(self.func, { local_set(map_ptr); });
                self.emit_expr(&args[1]);
                self.emit_search_key_store(&key_ty, sk_i32, sk_i64);
                wasm!(self.func, {
                    local_get(map_ptr); i32_load(0); local_set(old_len);
                    // Find key index
                    i32_const(-1); local_set(found_idx);
                    i32_const(0); local_set(src_i);
                    block_empty; loop_empty;
                      local_get(src_i); local_get(old_len); i32_ge_u; br_if(1);
                      local_get(map_ptr); i32_const(4); i32_add;
                      local_get(src_i); i32_const(entry as i32); i32_mul; i32_add;
                });
                self.emit_key_load(&key_ty, 0);
                self.emit_search_key_load(&key_ty, sk_i32, sk_i64);
                self.emit_key_eq(&key_ty);
                wasm!(self.func, {
                      if_empty; local_get(src_i); local_set(found_idx); end;
                      local_get(src_i); i32_const(1); i32_add; local_set(src_i);
                      br(0);
                    end; end;
                    // Not found → return original
                    local_get(found_idx); i32_const(0); i32_lt_s;
                    if_i32; local_get(map_ptr);
                    else_;
                      // Alloc new map with len-1
                      i32_const(4); local_get(old_len); i32_const(1); i32_sub;
                      i32_const(entry as i32); i32_mul; i32_add;
                      call(self.emitter.rt.alloc); local_set(new_map);
                      local_get(new_map); local_get(old_len); i32_const(1); i32_sub; i32_store(0);
                      // Copy entries skipping found_idx
                      i32_const(0); local_set(src_i);
                      i32_const(0); local_set(dst_i);
                      block_empty; loop_empty;
                        local_get(src_i); local_get(old_len); i32_ge_u; br_if(1);
                        local_get(src_i); local_get(found_idx); i32_ne;
                        if_empty;
                          // Copy entire entry (key+val)
                          local_get(new_map); i32_const(4); i32_add;
                          local_get(dst_i); i32_const(entry as i32); i32_mul; i32_add;
                          local_get(map_ptr); i32_const(4); i32_add;
                          local_get(src_i); i32_const(entry as i32); i32_mul; i32_add;
                });
                self.emit_entry_copy(entry);
                wasm!(self.func, {
                          local_get(dst_i); i32_const(1); i32_add; local_set(dst_i);
                        end;
                        local_get(src_i); i32_const(1); i32_add; local_set(src_i);
                        br(0);
                      end; end;
                      local_get(new_map);
                    end;
                });
                self.scratch.free_i32(dst_i);
                self.scratch.free_i32(new_map);
                self.scratch.free_i32(src_i);
                self.scratch.free_i32(found_idx);
                self.scratch.free_i32(old_len);
                self.scratch.free_i64(sk_i64);
                self.scratch.free_i32(sk_i32);
                self.scratch.free_i32(map_ptr);
            }
            "keys" => {
                let (ks, vs) = self.map_kv_sizes(&args[0].ty);
                let _key_ty = self.map_key_ty(&args[0].ty);
                let entry = ks + vs;
                let map_ptr = self.scratch.alloc_i32();
                let len = self.scratch.alloc_i32();
                let result = self.scratch.alloc_i32();
                let i = self.scratch.alloc_i32();
                self.emit_expr(&args[0]);
                wasm!(self.func, {
                    local_set(map_ptr);
                    local_get(map_ptr); i32_load(0); local_set(len);
                    i32_const(4); local_get(len); i32_const(ks as i32); i32_mul; i32_add;
                    call(self.emitter.rt.alloc); local_set(result);
                    local_get(result); local_get(len); i32_store(0);
                    i32_const(0); local_set(i);
                    block_empty; loop_empty;
                      local_get(i); local_get(len); i32_ge_u; br_if(1);
                      // dst: result[4 + i*ks]
                      local_get(result); i32_const(4); i32_add;
                      local_get(i); i32_const(ks as i32); i32_mul; i32_add;
                      // src: map[4 + i*entry]
                      local_get(map_ptr); i32_const(4); i32_add;
                      local_get(i); i32_const(entry as i32); i32_mul; i32_add;
                });
                self.emit_elem_copy_sized(ks);
                wasm!(self.func, {
                      local_get(i); i32_const(1); i32_add; local_set(i);
                      br(0);
                    end; end;
                    local_get(result);
                });
                self.scratch.free_i32(i);
                self.scratch.free_i32(result);
                self.scratch.free_i32(len);
                self.scratch.free_i32(map_ptr);
            }
            "values" => {
                let (ks, vs) = self.map_kv_sizes(&args[0].ty);
                let _val_ty = self.map_val_ty(&args[0].ty);
                let entry = ks + vs;
                let map_ptr = self.scratch.alloc_i32();
                let len = self.scratch.alloc_i32();
                let result = self.scratch.alloc_i32();
                let i = self.scratch.alloc_i32();
                self.emit_expr(&args[0]);
                wasm!(self.func, {
                    local_set(map_ptr);
                    local_get(map_ptr); i32_load(0); local_set(len);
                    i32_const(4); local_get(len); i32_const(vs as i32); i32_mul; i32_add;
                    call(self.emitter.rt.alloc); local_set(result);
                    local_get(result); local_get(len); i32_store(0);
                    i32_const(0); local_set(i);
                    block_empty; loop_empty;
                      local_get(i); local_get(len); i32_ge_u; br_if(1);
                      local_get(result); i32_const(4); i32_add;
                      local_get(i); i32_const(vs as i32); i32_mul; i32_add;
                      local_get(map_ptr); i32_const(4); i32_add;
                      local_get(i); i32_const(entry as i32); i32_mul; i32_add;
                      i32_const(ks as i32); i32_add;
                });
                self.emit_elem_copy_sized(vs);
                wasm!(self.func, {
                      local_get(i); i32_const(1); i32_add; local_set(i);
                      br(0);
                    end; end;
                    local_get(result);
                });
                self.scratch.free_i32(i);
                self.scratch.free_i32(result);
                self.scratch.free_i32(len);
                self.scratch.free_i32(map_ptr);
            }
            "entries" => {
                // entries(m) → List[(K, V)] — list of heap-allocated tuple pointers
                let (ks, vs) = self.map_kv_sizes(&args[0].ty);
                let entry = ks + vs;
                let map_ptr = self.scratch.alloc_i32();
                let len = self.scratch.alloc_i32();
                let result = self.scratch.alloc_i32();
                let i = self.scratch.alloc_i32();
                let tuple_ptr = self.scratch.alloc_i32();
                self.emit_expr(&args[0]);
                wasm!(self.func, {
                    local_set(map_ptr);
                    local_get(map_ptr); i32_load(0); local_set(len);
                    // Alloc list of ptrs: [len:4][ptr0:4][ptr1:4]...
                    i32_const(4); local_get(len); i32_const(4); i32_mul; i32_add;
                    call(self.emitter.rt.alloc); local_set(result);
                    local_get(result); local_get(len); i32_store(0);
                    i32_const(0); local_set(i);
                    block_empty; loop_empty;
                      local_get(i); local_get(len); i32_ge_u; br_if(1);
                      // Alloc tuple: [key:ks][val:vs]
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
                      // Store tuple ptr in result list
                      local_get(result); i32_const(4); i32_add;
                      local_get(i); i32_const(4); i32_mul; i32_add;
                      local_get(tuple_ptr); i32_store(0);
                      local_get(i); i32_const(1); i32_add; local_set(i);
                      br(0);
                    end; end;
                    local_get(result);
                });
                self.scratch.free_i32(tuple_ptr);
                self.scratch.free_i32(i);
                self.scratch.free_i32(result);
                self.scratch.free_i32(len);
                self.scratch.free_i32(map_ptr);
            }
            "merge" => {
                // merge(a, b) → new map with a's entries then b's, where b overwrites
                // any duplicate keys (matches Rust runtime's HashMap::insert semantics).
                // Allocates worst-case (a_len + b_len) but stores actual unique count.
                let (ks, vs) = self.map_kv_sizes(&args[0].ty);
                let key_ty = self.map_key_ty(&args[0].ty);
                let entry = ks + vs;
                let map_a = self.scratch.alloc_i32();
                let map_b = self.scratch.alloc_i32();
                let a_len = self.scratch.alloc_i32();
                let b_len = self.scratch.alloc_i32();
                let result = self.scratch.alloc_i32();
                let result_len = self.scratch.alloc_i32();
                let i = self.scratch.alloc_i32();
                let j = self.scratch.alloc_i32();
                let found_idx = self.scratch.alloc_i32();
                let bk_i32 = self.scratch.alloc_i32();
                let bk_i64 = self.scratch.alloc_i64();
                self.emit_expr(&args[0]);
                wasm!(self.func, { local_set(map_a); });
                self.emit_expr(&args[1]);
                wasm!(self.func, {
                    local_set(map_b);
                    local_get(map_a); i32_load(0); local_set(a_len);
                    local_get(map_b); i32_load(0); local_set(b_len);
                    // Alloc result of worst case (a_len + b_len) entries
                    i32_const(4); local_get(a_len); local_get(b_len); i32_add;
                    i32_const(entry as i32); i32_mul; i32_add;
                    call(self.emitter.rt.alloc); local_set(result);
                    // Copy a entries first
                    i32_const(0); local_set(i);
                    block_empty; loop_empty;
                      local_get(i); local_get(a_len); i32_ge_u; br_if(1);
                      // Copy entry i from a → result[i]
                      local_get(result); i32_const(4); i32_add;
                      local_get(i); i32_const(entry as i32); i32_mul; i32_add;
                      local_get(map_a); i32_const(4); i32_add;
                      local_get(i); i32_const(entry as i32); i32_mul; i32_add;
                });
                self.emit_elem_copy_sized(ks);
                self.emit_merge_copy_val(entry, ks, vs, result, i, map_a);
                wasm!(self.func, {
                      local_get(i); i32_const(1); i32_add; local_set(i);
                      br(0);
                    end; end;
                    local_get(a_len); local_set(result_len);
                    // For each b entry: search result for matching key, overwrite if found, else append
                    i32_const(0); local_set(j);
                    block_empty; loop_empty;
                      local_get(j); local_get(b_len); i32_ge_u; br_if(1);
                      // Load b's key into search slot
                      local_get(map_b); i32_const(4); i32_add;
                      local_get(j); i32_const(entry as i32); i32_mul; i32_add;
                });
                self.emit_key_load(&key_ty, 0);
                self.emit_search_key_store(&key_ty, bk_i32, bk_i64);
                wasm!(self.func, {
                      // Linear search result[0..result_len] for matching key
                      i32_const(-1); local_set(found_idx);
                      i32_const(0); local_set(i);
                      block_empty; loop_empty;
                        local_get(i); local_get(result_len); i32_ge_u; br_if(1);
                        local_get(result); i32_const(4); i32_add;
                        local_get(i); i32_const(entry as i32); i32_mul; i32_add;
                });
                self.emit_key_load(&key_ty, 0);
                self.emit_search_key_load(&key_ty, bk_i32, bk_i64);
                self.emit_key_eq(&key_ty);
                wasm!(self.func, {
                        if_empty; local_get(i); local_set(found_idx); end;
                        local_get(i); i32_const(1); i32_add; local_set(i);
                        br(0);
                      end; end;
                      // Choose insert position: found_idx if >=0 else result_len (append)
                      local_get(found_idx); i32_const(0); i32_lt_s;
                      if_empty;
                        local_get(result_len); local_set(found_idx);
                        local_get(result_len); i32_const(1); i32_add; local_set(result_len);
                      end;
                      // Copy entry j from b → result[found_idx]
                      local_get(result); i32_const(4); i32_add;
                      local_get(found_idx); i32_const(entry as i32); i32_mul; i32_add;
                      local_get(map_b); i32_const(4); i32_add;
                      local_get(j); i32_const(entry as i32); i32_mul; i32_add;
                });
                self.emit_elem_copy_sized(ks);
                // Copy value: dst = result + 4 + found_idx*entry + ks,
                //             src = map_b + 4 + j*entry + ks.
                wasm!(self.func, {
                      local_get(result); i32_const(4); i32_add;
                      local_get(found_idx); i32_const(entry as i32); i32_mul; i32_add;
                      i32_const(ks as i32); i32_add;
                      local_get(map_b); i32_const(4); i32_add;
                      local_get(j); i32_const(entry as i32); i32_mul; i32_add;
                      i32_const(ks as i32); i32_add;
                });
                self.emit_elem_copy_sized(vs);
                wasm!(self.func, {
                      local_get(j); i32_const(1); i32_add; local_set(j);
                      br(0);
                    end; end;
                    // Store final length and return result
                    local_get(result); local_get(result_len); i32_store(0);
                    local_get(result);
                });
                self.scratch.free_i64(bk_i64);
                self.scratch.free_i32(bk_i32);
                self.scratch.free_i32(found_idx);
                self.scratch.free_i32(j);
                self.scratch.free_i32(i);
                self.scratch.free_i32(result_len);
                self.scratch.free_i32(result);
                self.scratch.free_i32(b_len);
                self.scratch.free_i32(a_len);
                self.scratch.free_i32(map_b);
                self.scratch.free_i32(map_a);
            }
            "from_list" => {
                // from_list(pairs: List[(K,V)]) → Map, last-write-wins on duplicate keys
                // (matches Rust runtime's HashMap::from semantics).
                let pair_ty = self.resolve_list_elem(&args[0], None);
                let (ks, vs, key_ty) = if let Ty::Tuple(elems) = &pair_ty {
                    let k = elems.first().map(|t| values::byte_size(t)).unwrap_or(4);
                    let v = elems.get(1).map(|t| values::byte_size(t)).unwrap_or(4);
                    let kt = elems.first().cloned().unwrap_or(Ty::String);
                    (k, v, kt)
                } else { (4u32, 4u32, Ty::String) };
                let entry = ks + vs;
                let pairs = self.scratch.alloc_i32();
                let len = self.scratch.alloc_i32();
                let result = self.scratch.alloc_i32();
                let result_len = self.scratch.alloc_i32();
                let i = self.scratch.alloc_i32();
                let j = self.scratch.alloc_i32();
                let found_idx = self.scratch.alloc_i32();
                let tuple_ptr = self.scratch.alloc_i32();
                let pk_i32 = self.scratch.alloc_i32();
                let pk_i64 = self.scratch.alloc_i64();
                self.emit_expr(&args[0]);
                wasm!(self.func, {
                    local_set(pairs);
                    local_get(pairs); i32_load(0); local_set(len);
                    // Alloc map: 4 + len * entry (worst case, no dedup)
                    i32_const(4); local_get(len); i32_const(entry as i32); i32_mul; i32_add;
                    call(self.emitter.rt.alloc); local_set(result);
                    i32_const(0); local_set(result_len);
                    i32_const(0); local_set(i);
                    block_empty; loop_empty;
                      local_get(i); local_get(len); i32_ge_u; br_if(1);
                      // tuple_ptr = pairs[4 + i*4]
                      local_get(pairs); i32_const(4); i32_add;
                      local_get(i); i32_const(4); i32_mul; i32_add;
                      i32_load(0); local_set(tuple_ptr);
                      // Load tuple's key into search slot
                      local_get(tuple_ptr);
                });
                self.emit_key_load(&key_ty, 0);
                self.emit_search_key_store(&key_ty, pk_i32, pk_i64);
                wasm!(self.func, {
                      // Linear search result[0..result_len] for matching key
                      i32_const(-1); local_set(found_idx);
                      i32_const(0); local_set(j);
                      block_empty; loop_empty;
                        local_get(j); local_get(result_len); i32_ge_u; br_if(1);
                        local_get(result); i32_const(4); i32_add;
                        local_get(j); i32_const(entry as i32); i32_mul; i32_add;
                });
                self.emit_key_load(&key_ty, 0);
                self.emit_search_key_load(&key_ty, pk_i32, pk_i64);
                self.emit_key_eq(&key_ty);
                wasm!(self.func, {
                        if_empty; local_get(j); local_set(found_idx); end;
                        local_get(j); i32_const(1); i32_add; local_set(j);
                        br(0);
                      end; end;
                      // Insert position: found_idx if >=0 else result_len (append)
                      local_get(found_idx); i32_const(0); i32_lt_s;
                      if_empty;
                        local_get(result_len); local_set(found_idx);
                        local_get(result_len); i32_const(1); i32_add; local_set(result_len);
                      end;
                      // Copy key: result[found_idx].key = tuple[0]
                      local_get(result); i32_const(4); i32_add;
                      local_get(found_idx); i32_const(entry as i32); i32_mul; i32_add;
                      local_get(tuple_ptr);
                });
                self.emit_elem_copy_sized(ks);
                wasm!(self.func, {
                      // Copy val: result[found_idx].val = tuple[ks]
                      local_get(result); i32_const(4); i32_add;
                      local_get(found_idx); i32_const(entry as i32); i32_mul; i32_add;
                      i32_const(ks as i32); i32_add;
                      local_get(tuple_ptr); i32_const(ks as i32); i32_add;
                });
                self.emit_elem_copy_sized(vs);
                wasm!(self.func, {
                      local_get(i); i32_const(1); i32_add; local_set(i);
                      br(0);
                    end; end;
                    local_get(result); local_get(result_len); i32_store(0);
                    local_get(result);
                });
                self.scratch.free_i64(pk_i64);
                self.scratch.free_i32(pk_i32);
                self.scratch.free_i32(tuple_ptr);
                self.scratch.free_i32(found_idx);
                self.scratch.free_i32(j);
                self.scratch.free_i32(i);
                self.scratch.free_i32(result_len);
                self.scratch.free_i32(result);
                self.scratch.free_i32(len);
                self.scratch.free_i32(pairs);
            }
            "insert" => {
                // insert(m, key, value) → Unit. Mutates m via map.set + writeback.
                // Reuse "set" to produce a new map, then write back to var.
                let set_args = args.to_vec();
                self.emit_map_call("set", &set_args);
                // Write back to var
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
            "delete" => {
                // delete(m, key) → Unit. Removes key from map in place.
                let (ks, vs) = self.map_kv_sizes(&args[0].ty);
                let key_ty = self.map_key_ty(&args[0].ty);
                let entry = ks + vs;
                let map_ptr = self.scratch.alloc_i32();
                let sk_i32 = self.scratch.alloc_i32();
                let sk_i64 = self.scratch.alloc_i64();
                let old_len = self.scratch.alloc_i32();
                let new_map = self.scratch.alloc_i32();
                let i = self.scratch.alloc_i32();
                let j = self.scratch.alloc_i32();

                self.emit_expr(&args[0]);
                wasm!(self.func, { local_set(map_ptr); });
                self.emit_expr(&args[1]); // key
                self.emit_search_key_store(&key_ty, sk_i32, sk_i64);
                wasm!(self.func, {
                    local_get(map_ptr); i32_load(0); local_set(old_len);
                    // Alloc new map with old_len entries (may be 1 less)
                    i32_const(4); local_get(old_len); i32_const(entry as i32); i32_mul; i32_add;
                    call(self.emitter.rt.alloc); local_set(new_map);
                    i32_const(0); local_set(i);
                    i32_const(0); local_set(j);
                    block_empty; loop_empty;
                      local_get(i); local_get(old_len); i32_ge_u; br_if(1);
                      // Compare key[i] with search key
                      local_get(map_ptr); i32_const(4); i32_add;
                      local_get(i); i32_const(entry as i32); i32_mul; i32_add;
                });
                self.emit_key_load(&key_ty, 0);
                self.emit_search_key_load(&key_ty, sk_i32, sk_i64);
                self.emit_key_eq(&key_ty);
                wasm!(self.func, {
                      if_empty;
                        // Skip this entry (key matches)
                      else_;
                        // Copy entry to new_map[j]
                        local_get(new_map); i32_const(4); i32_add;
                        local_get(j); i32_const(entry as i32); i32_mul; i32_add;
                        local_get(map_ptr); i32_const(4); i32_add;
                        local_get(i); i32_const(entry as i32); i32_mul; i32_add;
                        i32_const(entry as i32);
                        memory_copy;
                        local_get(j); i32_const(1); i32_add; local_set(j);
                      end;
                      local_get(i); i32_const(1); i32_add; local_set(i);
                      br(0);
                    end; end;
                    // Set new len = j
                    local_get(new_map); local_get(j); i32_store(0);
                });

                // Write back to var
                if let almide_ir::IrExprKind::Var { id } = &args[0].kind {
                    if let Some(&local_idx) = self.var_map.get(&id.0) {
                        wasm!(self.func, { local_get(new_map); local_set(local_idx); });
                    }
                }

                self.scratch.free_i32(j);
                self.scratch.free_i32(i);
                self.scratch.free_i32(new_map);
                self.scratch.free_i32(old_len);
                self.scratch.free_i64(sk_i64);
                self.scratch.free_i32(sk_i32);
                self.scratch.free_i32(map_ptr);
            }
            "clear" => {
                // clear(m) → Unit. Replace with empty map.
                let new_map = self.scratch.alloc_i32();
                wasm!(self.func, {
                    i32_const(4); call(self.emitter.rt.alloc); local_set(new_map);
                    local_get(new_map); i32_const(0); i32_store(0);
                });
                if let almide_ir::IrExprKind::Var { id } = &args[0].kind {
                    if let Some(&local_idx) = self.var_map.get(&id.0) {
                        wasm!(self.func, { local_get(new_map); local_set(local_idx); });
                    }
                }
                self.scratch.free_i32(new_map);
            }
            _ => return self.emit_map_closure_call(method, args),
        }
        true
    }

    // ── Map helpers ──

    pub(super) fn map_kv_sizes(&self, ty: &Ty) -> (u32, u32) {
        if let Ty::Applied(_, args) = ty {
            let ks = args.first().map(|t| values::byte_size(t)).unwrap_or(4); // key (usually String=i32)
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

    /// Load a key from memory at [addr_on_stack + offset].
    /// For Int keys: i64_load. For String/Bool/other: i32_load.
    pub(super) fn emit_key_load(&mut self, key_ty: &Ty, offset: u32) {
        match key_ty {
            Ty::Int => { wasm!(self.func, { i64_load(offset); }); }
            _ => { wasm!(self.func, { i32_load(offset); }); }
        }
    }

    /// Store a key to memory at [addr_on_stack, val_on_stack].
    /// For Int keys: i64_store. For String/Bool/other: i32_store.
    pub(super) fn emit_key_store(&mut self, key_ty: &Ty, offset: u32) {
        match key_ty {
            Ty::Int => { wasm!(self.func, { i64_store(offset); }); }
            _ => { wasm!(self.func, { i32_store(offset); }); }
        }
    }

    /// Emit key comparison: consumes [key_a, key_b], produces i32 (1=equal).
    pub(super) fn emit_key_eq(&mut self, key_ty: &Ty) {
        match key_ty {
            Ty::Int => { wasm!(self.func, { i64_eq; }); }
            Ty::String => { wasm!(self.func, { call(self.emitter.rt.string.eq); }); }
            Ty::Bool => { wasm!(self.func, { i32_eq; }); }
            _ => { wasm!(self.func, { i32_eq; }); } // pointer equality for other types
        }
    }

    /// Store search key to scratch local. For Int keys, stores to i64 local.
    /// For other keys, stores to i32 local.
    pub(super) fn emit_search_key_store(&mut self, key_ty: &Ty, scratch_i32: u32, scratch_i64: u32) {
        match key_ty {
            Ty::Int => {
                wasm!(self.func, { local_set(scratch_i64); });
            }
            _ => {
                wasm!(self.func, { local_set(scratch_i32); });
            }
        }
    }

    /// Load search key from scratch local. For Int keys, loads i64 local.
    /// For other keys, loads i32 local.
    pub(super) fn emit_search_key_load(&mut self, key_ty: &Ty, scratch_i32: u32, scratch_i64: u32) {
        match key_ty {
            Ty::Int => {
                wasm!(self.func, { local_get(scratch_i64); });
            }
            _ => {
                wasm!(self.func, { local_get(scratch_i32); });
            }
        }
    }

    /// Key ValType for call_indirect signatures.
    pub(super) fn key_valtype(key_ty: &Ty) -> ValType {
        match key_ty {
            Ty::Int => ValType::I64,
            _ => ValType::I32,
        }
    }

    fn emit_merge_copy_val(&mut self, entry: u32, ks: u32, vs: u32, result: u32, i: u32, map_a: u32) {
        wasm!(self.func, {
              local_get(result); i32_const(4); i32_add;
              local_get(i); i32_const(entry as i32); i32_mul; i32_add;
              i32_const(ks as i32); i32_add;
              local_get(map_a); i32_const(4); i32_add;
              local_get(i); i32_const(entry as i32); i32_mul; i32_add;
              i32_const(ks as i32); i32_add;
        });
        self.emit_elem_copy_sized(vs);
    }

    fn emit_merge_copy_val2(&mut self, entry: u32, ks: u32, vs: u32, result: u32, a_len: u32, i: u32, map_b: u32) {
        wasm!(self.func, {
              local_get(result); i32_const(4); i32_add;
              local_get(a_len); local_get(i); i32_add;
              i32_const(entry as i32); i32_mul; i32_add;
              i32_const(ks as i32); i32_add;
              local_get(map_b); i32_const(4); i32_add;
              local_get(i); i32_const(entry as i32); i32_mul; i32_add;
              i32_const(ks as i32); i32_add;
        });
        self.emit_elem_copy_sized(vs);
    }

    pub(super) fn emit_elem_copy_sized(&mut self, size: u32) {
        // Copy `size` bytes from [stack: dst, src]
        match size {
            8 => { wasm!(self.func, { i64_load(0); i64_store(0); }); }
            4 => { wasm!(self.func, { i32_load(0); i32_store(0); }); }
            _ => {
                // Generic byte copy — for now just use i32 for small sizes
                wasm!(self.func, { i32_load(0); i32_store(0); });
            }
        }
    }

    fn emit_entry_copy(&mut self, entry_size: u32) {
        // Copy entire entry (key + val) — byte by byte for arbitrary sizes
        match entry_size {
            8 => { wasm!(self.func, { i64_load(0); i64_store(0); }); }
            12 => {
                let dst = self.scratch.alloc_i32();
                let src = self.scratch.alloc_i32();
                wasm!(self.func, {
                    local_set(src);
                    local_set(dst);
                    local_get(dst); local_get(src); i32_load(0); i32_store(0);
                    local_get(dst); local_get(src); i32_load(4); i32_store(4);
                    local_get(dst); local_get(src); i32_load(8); i32_store(8);
                });
                self.scratch.free_i32(src);
                self.scratch.free_i32(dst);
            }
            _ => {
                // Fallback: copy as i32 words
                let dst = self.scratch.alloc_i32();
                let src = self.scratch.alloc_i32();
                wasm!(self.func, {
                    local_set(src);
                    local_set(dst);
                });
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
