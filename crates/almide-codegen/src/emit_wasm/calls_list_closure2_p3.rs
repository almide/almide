// List stdlib closure-based call dispatch for WASM codegen (part 2, group 3).
//
// Sub-dispatch group for `emit_list_closure_call2`: group_by, shuffle.
// Included into `calls_list_closure2.rs` via `include!`; shares its module
// imports and the `FuncCompiler` impl. Arm patterns are DISJOINT from the
// other groups so chain order is irrelevant.

impl FuncCompiler<'_> {
    /// `emit_list_closure_call2` group 3. Returns true if handled.
    fn emit_list_closure_call2_g3(&mut self, method: &str, args: &[IrExpr]) -> bool {
        use super::engine::layout::{LIST, list as ll, map as lm};
        let list_data_off = self.emitter.layout_reg.fixed_offset(LIST, ll::DATA) as i32;
        let list_hdr = self.emitter.layout_reg.header_size(LIST) as i32;
        match method {
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
            _ => return false,
        }
        true
    }
}
