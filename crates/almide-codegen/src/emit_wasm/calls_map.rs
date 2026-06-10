//! Map stdlib call dispatch for WASM codegen — Swiss Table layout.
//!
//! Layout: [len:i32 @ 0][cap:i32 @ 4][tags @ 8 (cap bytes)][entries @ 8+cap]
//! Tags: 1 byte each, h2 = hash upper 7 bits (0x01..0x7F). 0x00 = empty.
//! Entries: key + val per slot (no tag), contiguous array.
//! Total size = 8 + cap + cap * entry_size.
//! Cap is always a power of 2. Resize at 75% load factor.

use super::FuncCompiler;
use super::values;
use almide_ir::IrExpr;
use almide_lang::types::Ty;
use wasm_encoder::ValType;

impl FuncCompiler<'_> {
    pub(super) fn emit_map_call(&mut self, method: &str, args: &[IrExpr]) -> bool {
        use super::engine::layout::{SWISS_MAP, LIST, map as lm, list as ll};
        let map_cap_off = self.emitter.layout_reg.fixed_offset(SWISS_MAP, lm::CAP);
        let map_tags_off = self.emitter.layout_reg.fixed_offset(SWISS_MAP, lm::TAGS) as i32;
        let map_hdr = self.emitter.layout_reg.header_size(SWISS_MAP) as i32;
        let list_data_off = self.emitter.layout_reg.fixed_offset(LIST, ll::DATA) as i32;
        let list_hdr = self.emitter.layout_reg.header_size(LIST) as i32;
        match method {
            "new" => {
                // Empty map: header only, cap=0
                let scratch = self.scratch.alloc_i32();
                wasm!(self.func, {
                    i32_const(map_hdr);
                    call(self.emitter.rt.alloc); local_set(scratch);
                    local_get(scratch); i32_const(0); i32_store(0);
                    local_get(scratch); i32_const(0); i32_store(map_cap_off);
                    local_get(scratch);
                });
                self.scratch.free_i32(scratch);
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
                // Probe the hash INDEX, then read the dense entry it points at.
                let (ks, vs) = self.map_kv_sizes(&args[0].ty);
                let key_ty = self.map_key_ty(&args[0].ty);
                let es = ks + vs;
                let m = self.scratch.alloc_i32();
                let sk32 = self.scratch.alloc_i32();
                let sk64 = self.scratch.alloc_i64();
                let cap = self.scratch.alloc_i32();
                let idx = self.scratch.alloc_i32();   // probe slot
                let ib = self.scratch.alloc_i32();    // index base
                let eb = self.scratch.alloc_i32();    // entries base
                let ei = self.scratch.alloc_i32();    // dense entry index
                let h2 = self.scratch.alloc_i32();
                let tg = self.scratch.alloc_i32();
                let result = self.scratch.alloc_i32();
                let vc = self.scratch.alloc_i32();

                self.emit_expr(&args[0]);
                wasm!(self.func, { local_set(m); });
                self.emit_expr(&args[1]);
                self.emit_search_key_store(&key_ty, sk32, sk64);

                wasm!(self.func, {
                    i32_const(0); local_set(result);
                    local_get(m); i32_load(map_cap_off); local_set(cap);
                    local_get(cap); i32_eqz;
                    if_empty; else_;
                });
                self.emit_dict_index_base(m, cap);
                wasm!(self.func, { local_set(ib); });
                self.emit_dict_entries_base(m, cap);
                wasm!(self.func, { local_set(eb); });
                self.emit_search_key_load(&key_ty, sk32, sk64);
                self.emit_hash_key(&key_ty);
                self.emit_h1_h2(cap, idx, h2);
                wasm!(self.func, {
                    block_empty; loop_empty;
                      local_get(m); i32_const(map_tags_off); i32_add;
                      local_get(idx); i32_add; i32_load8_u(0); local_set(tg);
                      local_get(tg); i32_eqz; br_if(1); // empty slot → absent
                      local_get(tg); local_get(h2); i32_eq;
                      if_empty;
                        // ei = index[idx] - 1 ; entry = eb + ei*es (key @0)
                        local_get(ib); local_get(idx); i32_const(lm::INDEX_SLOT_SIZE as i32); i32_mul; i32_add;
                        i32_load(0); i32_const(1); i32_sub; local_set(ei);
                        local_get(eb); local_get(ei); i32_const(es as i32); i32_mul; i32_add;
                });
                self.emit_key_load(&key_ty, 0);
                self.emit_search_key_load(&key_ty, sk32, sk64);
                self.emit_key_eq(&key_ty);
                wasm!(self.func, {
                        if_empty;
                          i32_const(vs as i32); call(self.emitter.rt.alloc); local_set(vc);
                          local_get(vc);
                          local_get(eb); local_get(ei); i32_const(es as i32); i32_mul; i32_add;
                          i32_const(ks as i32); i32_add;
                });
                self.emit_elem_copy_sized(vs);
                wasm!(self.func, {
                          local_get(vc); local_set(result); br(3);
                        end;
                      end;
                      local_get(idx); i32_const(1); i32_add;
                      local_get(cap); i32_const(1); i32_sub; i32_and;
                      local_set(idx); br(0);
                    end; end;
                    end; // cap!=0
                    local_get(result);
                });
                self.scratch.free_i32(vc);
                self.scratch.free_i32(result);
                self.scratch.free_i32(tg);
                self.scratch.free_i32(h2);
                self.scratch.free_i32(ei);
                self.scratch.free_i32(eb);
                self.scratch.free_i32(ib);
                self.scratch.free_i32(idx);
                self.scratch.free_i32(cap);
                self.scratch.free_i64(sk64);
                self.scratch.free_i32(sk32);
                self.scratch.free_i32(m);
            }
            "get_or" => {
                let (ks, vs) = self.map_kv_sizes(&args[0].ty);
                let key_ty = self.map_key_ty(&args[0].ty);
                let val_ty = self.map_val_ty(&args[0].ty);
                let es = ks + vs;
                let vt = values::ty_to_valtype(&val_ty).unwrap_or(ValType::I32);
                let m = self.scratch.alloc_i32();
                let sk32 = self.scratch.alloc_i32();
                let sk64 = self.scratch.alloc_i64();
                let cap = self.scratch.alloc_i32();
                let idx = self.scratch.alloc_i32();
                let ib = self.scratch.alloc_i32();
                let eb = self.scratch.alloc_i32();
                let ei = self.scratch.alloc_i32();
                let h2 = self.scratch.alloc_i32();
                let tg = self.scratch.alloc_i32();
                let found = self.scratch.alloc_i32();

                self.emit_expr(&args[0]);
                wasm!(self.func, { local_set(m); });
                self.emit_expr(&args[1]);
                self.emit_search_key_store(&key_ty, sk32, sk64);
                wasm!(self.func, {
                    i32_const(0); local_set(found);
                    local_get(m); i32_load(map_cap_off); local_set(cap);
                    local_get(cap); i32_eqz;
                    if_empty; else_;
                });
                self.emit_dict_index_base(m, cap);
                wasm!(self.func, { local_set(ib); });
                self.emit_dict_entries_base(m, cap);
                wasm!(self.func, { local_set(eb); });
                self.emit_search_key_load(&key_ty, sk32, sk64);
                self.emit_hash_key(&key_ty);
                self.emit_h1_h2(cap, idx, h2);
                wasm!(self.func, {
                    block_empty; loop_empty;
                      local_get(m); i32_const(map_tags_off); i32_add;
                      local_get(idx); i32_add; i32_load8_u(0); local_set(tg);
                      local_get(tg); i32_eqz; br_if(1);
                      local_get(tg); local_get(h2); i32_eq;
                      if_empty;
                        local_get(ib); local_get(idx); i32_const(lm::INDEX_SLOT_SIZE as i32); i32_mul; i32_add;
                        i32_load(0); i32_const(1); i32_sub; local_set(ei);
                        local_get(eb); local_get(ei); i32_const(es as i32); i32_mul; i32_add;
                });
                self.emit_key_load(&key_ty, 0);
                self.emit_search_key_load(&key_ty, sk32, sk64);
                self.emit_key_eq(&key_ty);
                wasm!(self.func, {
                        if_empty; i32_const(1); local_set(found); br(3); end;
                      end;
                      local_get(idx); i32_const(1); i32_add;
                      local_get(cap); i32_const(1); i32_sub; i32_and;
                      local_set(idx); br(0);
                    end; end;
                    end;
                    local_get(found); i32_eqz;
                });
                match vt {
                    ValType::I64 => { wasm!(self.func, { if_i64; }); }
                    ValType::F64 => { wasm!(self.func, { if_f64; }); }
                    _ => { wasm!(self.func, { if_i32; }); }
                }
                self.emit_expr(&args[2]);
                wasm!(self.func, { else_; });
                wasm!(self.func, {
                    local_get(eb); local_get(ei); i32_const(es as i32); i32_mul; i32_add;
                    i32_const(ks as i32); i32_add;
                });
                self.emit_load_at(&val_ty, 0);
                wasm!(self.func, { end; });

                self.scratch.free_i32(found);
                self.scratch.free_i32(tg);
                self.scratch.free_i32(h2);
                self.scratch.free_i32(ei);
                self.scratch.free_i32(eb);
                self.scratch.free_i32(ib);
                self.scratch.free_i32(idx);
                self.scratch.free_i32(cap);
                self.scratch.free_i64(sk64);
                self.scratch.free_i32(sk32);
                self.scratch.free_i32(m);
            }
            "contains" => {
                let (ks, vs) = self.map_kv_sizes(&args[0].ty);
                let key_ty = self.map_key_ty(&args[0].ty);
                let es = ks + vs;
                let m = self.scratch.alloc_i32();
                let sk32 = self.scratch.alloc_i32();
                let sk64 = self.scratch.alloc_i64();
                let cap = self.scratch.alloc_i32();
                let idx = self.scratch.alloc_i32();
                let ib = self.scratch.alloc_i32();
                let eb = self.scratch.alloc_i32();
                let ei = self.scratch.alloc_i32();
                let h2 = self.scratch.alloc_i32();
                let tg = self.scratch.alloc_i32();
                let result = self.scratch.alloc_i32();

                self.emit_expr(&args[0]);
                wasm!(self.func, { local_set(m); });
                self.emit_expr(&args[1]);
                self.emit_search_key_store(&key_ty, sk32, sk64);
                wasm!(self.func, {
                    i32_const(0); local_set(result);
                    local_get(m); i32_load(map_cap_off); local_set(cap);
                    local_get(cap); i32_eqz;
                    if_empty; else_;
                });
                self.emit_dict_index_base(m, cap);
                wasm!(self.func, { local_set(ib); });
                self.emit_dict_entries_base(m, cap);
                wasm!(self.func, { local_set(eb); });
                self.emit_search_key_load(&key_ty, sk32, sk64);
                self.emit_hash_key(&key_ty);
                self.emit_h1_h2(cap, idx, h2);
                wasm!(self.func, {
                    block_empty; loop_empty;
                      local_get(m); i32_const(map_tags_off); i32_add;
                      local_get(idx); i32_add; i32_load8_u(0); local_set(tg);
                      local_get(tg); i32_eqz; br_if(1);
                      local_get(tg); local_get(h2); i32_eq;
                      if_empty;
                        local_get(ib); local_get(idx); i32_const(lm::INDEX_SLOT_SIZE as i32); i32_mul; i32_add;
                        i32_load(0); i32_const(1); i32_sub; local_set(ei);
                        local_get(eb); local_get(ei); i32_const(es as i32); i32_mul; i32_add;
                });
                self.emit_key_load(&key_ty, 0);
                self.emit_search_key_load(&key_ty, sk32, sk64);
                self.emit_key_eq(&key_ty);
                wasm!(self.func, {
                        if_empty; i32_const(1); local_set(result); br(3); end;
                      end;
                      local_get(idx); i32_const(1); i32_add;
                      local_get(cap); i32_const(1); i32_sub; i32_and;
                      local_set(idx); br(0);
                    end; end;
                    end;
                    local_get(result);
                });
                self.scratch.free_i32(result);
                self.scratch.free_i32(tg);
                self.scratch.free_i32(h2);
                self.scratch.free_i32(ei);
                self.scratch.free_i32(eb);
                self.scratch.free_i32(ib);
                self.scratch.free_i32(idx);
                self.scratch.free_i32(cap);
                self.scratch.free_i64(sk64);
                self.scratch.free_i32(sk32);
                self.scratch.free_i32(m);
            }
            "set" => {
                // Immutable COD insert: fresh table sized for old_len+1, copy the old
                // entries + rebuild index, then probe-and-place the (key,val) — overwrite
                // in place (position kept) or append densely (insertion order).
                let (ks, vs) = self.map_kv_sizes(&args[0].ty);
                let key_ty = self.map_key_ty(&args[0].ty);
                let val_ty = self.map_val_ty(&args[0].ty);
                let es = ks + vs;
                let vt = values::ty_to_valtype(&val_ty).unwrap_or(ValType::I32);
                let m = self.scratch.alloc_i32();
                let tmp = self.scratch.alloc_i32();
                let old_cap = self.scratch.alloc_i32();
                let n = self.scratch.alloc_i32();
                let cap = self.scratch.alloc_i32();
                let nm = self.scratch.alloc_i32();
                let ib = self.scratch.alloc_i32();
                let eb = self.scratch.alloc_i32();

                self.emit_expr(&args[0]);
                wasm!(self.func, { local_set(m); });
                // Materialize the (key,val) into a temp entry buffer (key@0, val@ks).
                wasm!(self.func, { i32_const(es as i32); call(self.emitter.rt.alloc); local_set(tmp); local_get(tmp); });
                self.emit_expr(&args[1]);
                self.emit_key_store(&key_ty, 0);
                wasm!(self.func, { local_get(tmp); i32_const(ks as i32); i32_add; });
                self.emit_expr(&args[2]);
                match vt {
                    ValType::I64 => { wasm!(self.func, { i64_store(0); }); }
                    ValType::F64 => { wasm!(self.func, { f64_store(0); }); }
                    _ => { wasm!(self.func, { i32_store(0); }); }
                }
                wasm!(self.func, {
                    local_get(m); i32_load(map_cap_off); local_set(old_cap);
                    local_get(m); i32_load(0); i32_const(1); i32_add; local_set(n);
                });
                self.emit_dict_fit_cap(n, cap);
                self.emit_dict_alloc(nm, cap, es);
                self.emit_dict_recap(m, old_cap, nm, cap, es, &key_ty);
                self.emit_dict_index_base(nm, cap);
                wasm!(self.func, { local_set(ib); });
                self.emit_dict_entries_base(nm, cap);
                wasm!(self.func, { local_set(eb); });
                self.emit_dict_put_entry(nm, cap, ib, eb, tmp, es, ks, vs, &key_ty, &val_ty);
                wasm!(self.func, { local_get(nm); });

                self.scratch.free_i32(eb);
                self.scratch.free_i32(ib);
                self.scratch.free_i32(nm);
                self.scratch.free_i32(cap);
                self.scratch.free_i32(n);
                self.scratch.free_i32(old_cap);
                self.scratch.free_i32(tmp);
                self.scratch.free_i32(m);
            }
            "insert" => {
                // Mutable in-place COD insert: ensure allocated, probe-and-place the
                // (key,val), then grow (rehash into 4× table) if the load factor trips.
                let (ks, vs) = self.map_kv_sizes(&args[0].ty);
                let key_ty = self.map_key_ty(&args[0].ty);
                let val_ty = self.map_val_ty(&args[0].ty);
                let es = ks + vs;
                let vt = values::ty_to_valtype(&val_ty).unwrap_or(ValType::I32);
                let m = self.scratch.alloc_i32();
                let tmp = self.scratch.alloc_i32();
                let cap = self.scratch.alloc_i32();
                let ib = self.scratch.alloc_i32();
                let eb = self.scratch.alloc_i32();

                self.emit_expr(&args[0]);
                wasm!(self.func, { local_set(m); });
                // Materialize the (key,val) into a temp entry buffer (key@0, val@ks).
                wasm!(self.func, { i32_const(es as i32); call(self.emitter.rt.alloc); local_set(tmp); local_get(tmp); });
                self.emit_expr(&args[1]);
                self.emit_key_store(&key_ty, 0);
                wasm!(self.func, { local_get(tmp); i32_const(ks as i32); i32_add; });
                self.emit_expr(&args[2]);
                match vt {
                    ValType::I64 => { wasm!(self.func, { i64_store(0); }); }
                    ValType::F64 => { wasm!(self.func, { f64_store(0); }); }
                    _ => { wasm!(self.func, { i32_store(0); }); }
                }
                // Ensure allocated (cap==0 → fresh INITIAL_CAP table into m).
                wasm!(self.func, {
                    local_get(m); i32_load(map_cap_off); local_set(cap);
                    local_get(cap); i32_eqz;
                    if_empty;
                      i32_const(lm::INITIAL_CAP as i32); local_set(cap);
                });
                self.emit_dict_alloc(m, cap, es);
                wasm!(self.func, { end; });
                self.emit_dict_index_base(m, cap);
                wasm!(self.func, { local_set(ib); });
                self.emit_dict_entries_base(m, cap);
                wasm!(self.func, { local_set(eb); });
                self.emit_dict_put_entry(m, cap, ib, eb, tmp, es, ks, vs, &key_ty, &val_ty);
                // Grow if the load factor is exceeded (only a new key can trip it).
                wasm!(self.func, {
                    local_get(m); i32_load(0); i32_const(lm::LOAD_DEN as i32); i32_mul;
                    local_get(cap); i32_const(lm::LOAD_NUM as i32); i32_mul;
                    i32_gt_u;
                    if_empty;
                });
                self.emit_dict_grow(m, cap, es, &key_ty);
                wasm!(self.func, { end; });
                // Write back the (possibly reallocated) map ptr — into the cell for a
                // mutable capture, else into the local/global. See emit_mutator_writeback.
                self.emit_mutator_writeback(&args[0], m);
                self.scratch.free_i32(eb);
                self.scratch.free_i32(ib);
                self.scratch.free_i32(cap);
                self.scratch.free_i32(tmp);
                self.scratch.free_i32(m);
            }
            "remove" | "delete" => {
                // Dense COD rebuild excluding the matching key: walk the old entries in
                // insertion order, copy every survivor densely (order preserved), then
                // rebuild the index over the survivors.
                let is_delete = method == "delete";
                let (ks, vs) = self.map_kv_sizes(&args[0].ty);
                let key_ty = self.map_key_ty(&args[0].ty);
                let es = ks + vs;
                let m = self.scratch.alloc_i32();
                let sk32 = self.scratch.alloc_i32();
                let sk64 = self.scratch.alloc_i64();
                let cap_old = self.scratch.alloc_i32();
                let len = self.scratch.alloc_i32();
                let new_cap = self.scratch.alloc_i32();
                let nm = self.scratch.alloc_i32();
                let i = self.scratch.alloc_i32();
                let ne = self.scratch.alloc_i32();
                let eb_old = self.scratch.alloc_i32();
                let eb_new = self.scratch.alloc_i32();
                let entry = self.scratch.alloc_i32();

                self.emit_expr(&args[0]);
                wasm!(self.func, { local_set(m); });
                self.emit_expr(&args[1]);
                self.emit_search_key_store(&key_ty, sk32, sk64);
                wasm!(self.func, {
                    local_get(m); i32_load(map_cap_off); local_set(cap_old);
                    local_get(m); i32_load(0); local_set(len);
                });
                self.emit_dict_fit_cap(len, new_cap);
                self.emit_dict_alloc(nm, new_cap, es);
                self.emit_dict_entries_base(m, cap_old);
                wasm!(self.func, { local_set(eb_old); });
                self.emit_dict_entries_base(nm, new_cap);
                wasm!(self.func, {
                    local_set(eb_new);
                    i32_const(0); local_set(ne);
                    i32_const(0); local_set(i);
                    block_empty; loop_empty;
                      local_get(i); local_get(len); i32_ge_u; br_if(1);
                      local_get(eb_old); local_get(i); i32_const(es as i32); i32_mul; i32_add; local_set(entry);
                      local_get(entry);
                });
                self.emit_key_load(&key_ty, 0);
                self.emit_search_key_load(&key_ty, sk32, sk64);
                self.emit_key_eq(&key_ty);
                wasm!(self.func, {
                      i32_eqz;
                      if_empty;  // key != search key → keep (append densely)
                        local_get(eb_new); local_get(ne); i32_const(es as i32); i32_mul; i32_add;
                        local_get(entry); i32_const(es as i32); memory_copy;
                        local_get(ne); i32_const(1); i32_add; local_set(ne);
                      end;
                      local_get(i); i32_const(1); i32_add; local_set(i); br(0);
                    end; end;
                    local_get(nm); local_get(ne); i32_store(0); // nm.len = survivor count
                });
                self.emit_dict_rebuild_index(nm, new_cap, es, &key_ty);
                if is_delete {
                    // delete: in-place, write the rebuilt map back to its binding.
                    self.emit_mutator_writeback(&args[0], nm);
                } else {
                    // remove: returns the rebuilt map as the expression value.
                    wasm!(self.func, { local_get(nm); });
                }
                self.scratch.free_i32(entry);
                self.scratch.free_i32(eb_new);
                self.scratch.free_i32(eb_old);
                self.scratch.free_i32(ne);
                self.scratch.free_i32(i);
                self.scratch.free_i32(nm);
                self.scratch.free_i32(new_cap);
                self.scratch.free_i32(len);
                self.scratch.free_i32(cap_old);
                self.scratch.free_i64(sk64);
                self.scratch.free_i32(sk32);
                self.scratch.free_i32(m);
            }
            "keys" | "values" | "entries" => {
                // Dense walk of entries[0..len] = insertion order. No tag scan.
                let (ks, vs) = self.map_kv_sizes(&args[0].ty);
                let es = ks + vs;
                let m = self.scratch.alloc_i32();
                let len = self.scratch.alloc_i32();
                let cap = self.scratch.alloc_i32();
                let result = self.scratch.alloc_i32();
                let i = self.scratch.alloc_i32();
                let eb = self.scratch.alloc_i32();
                let elem_size = match method {
                    "keys" => ks, "values" => vs, _ => 4, // entries: tuple ptr
                };
                let tp = self.scratch.alloc_i32();

                self.emit_expr(&args[0]);
                wasm!(self.func, {
                    local_set(m);
                    local_get(m); i32_load(0); local_set(len);
                    local_get(m); i32_load(map_cap_off); local_set(cap);
                });
                self.emit_dict_entries_base(m, cap);
                wasm!(self.func, {
                    local_set(eb);
                    i32_const(list_hdr); local_get(len); i32_const(elem_size as i32); i32_mul; i32_add;
                    call(self.emitter.rt.alloc); local_set(result);
                    local_get(result); local_get(len); i32_store(0);
                    i32_const(0); local_set(i);
                    block_empty; loop_empty;
                      local_get(i); local_get(len); i32_ge_u; br_if(1);
                });
                match method {
                    "keys" => {
                        wasm!(self.func, {
                            local_get(result); i32_const(list_data_off); i32_add;
                            local_get(i); i32_const(ks as i32); i32_mul; i32_add;
                            local_get(eb); local_get(i); i32_const(es as i32); i32_mul; i32_add;
                        });
                        self.emit_elem_copy_sized(ks);
                    }
                    "values" => {
                        wasm!(self.func, {
                            local_get(result); i32_const(list_data_off); i32_add;
                            local_get(i); i32_const(vs as i32); i32_mul; i32_add;
                            local_get(eb); local_get(i); i32_const(es as i32); i32_mul; i32_add;
                            i32_const(ks as i32); i32_add;
                        });
                        self.emit_elem_copy_sized(vs);
                    }
                    _ => {
                        wasm!(self.func, {
                            i32_const(es as i32); call(self.emitter.rt.alloc); local_set(tp);
                            local_get(tp);
                            local_get(eb); local_get(i); i32_const(es as i32); i32_mul; i32_add;
                            i32_const(es as i32); memory_copy;
                            local_get(result); i32_const(list_data_off); i32_add;
                            local_get(i); i32_const(elem_size as i32); i32_mul; i32_add;
                            local_get(tp); i32_store(0);
                        });
                    }
                }
                wasm!(self.func, {
                      local_get(i); i32_const(1); i32_add; local_set(i); br(0);
                    end; end;
                    local_get(result);
                });
                self.scratch.free_i32(tp);
                self.scratch.free_i32(eb);
                self.scratch.free_i32(i);
                self.scratch.free_i32(result);
                self.scratch.free_i32(cap);
                self.scratch.free_i32(len);
                self.scratch.free_i32(m);
            }
            "merge" => {
                // Copy a (entries + index) into a table sized for a.len + b.len, then
                // put each b entry in insertion order: a-keys keep their position and
                // value is overwritten by b; b-only keys append after a's entries.
                let (ks, vs) = self.map_kv_sizes(&args[0].ty);
                let key_ty = self.map_key_ty(&args[0].ty);
                let es = ks + vs;
                let ma = self.scratch.alloc_i32();
                let mb = self.scratch.alloc_i32();
                let cap_a = self.scratch.alloc_i32();
                let cap_b = self.scratch.alloc_i32();
                let len_b = self.scratch.alloc_i32();
                let n = self.scratch.alloc_i32();
                let r_cap = self.scratch.alloc_i32();
                let result = self.scratch.alloc_i32();
                let ib = self.scratch.alloc_i32();
                let eb = self.scratch.alloc_i32();
                let eb_b = self.scratch.alloc_i32();
                let j = self.scratch.alloc_i32();
                let src = self.scratch.alloc_i32();

                self.emit_expr(&args[0]);
                wasm!(self.func, { local_set(ma); });
                self.emit_expr(&args[1]);
                wasm!(self.func, {
                    local_set(mb);
                    local_get(ma); i32_load(map_cap_off); local_set(cap_a);
                    local_get(mb); i32_load(map_cap_off); local_set(cap_b);
                    local_get(mb); i32_load(0); local_set(len_b);
                    local_get(ma); i32_load(0); local_get(len_b); i32_add; local_set(n);
                });
                self.emit_dict_fit_cap(n, r_cap);
                self.emit_dict_alloc(result, r_cap, es);
                self.emit_dict_recap(ma, cap_a, result, r_cap, es, &key_ty);
                self.emit_dict_index_base(result, r_cap);
                wasm!(self.func, { local_set(ib); });
                self.emit_dict_entries_base(result, r_cap);
                wasm!(self.func, { local_set(eb); });
                self.emit_dict_entries_base(mb, cap_b);
                wasm!(self.func, {
                    local_set(eb_b);
                    i32_const(0); local_set(j);
                    block_empty; loop_empty;
                      local_get(j); local_get(len_b); i32_ge_u; br_if(1);
                      local_get(eb_b); local_get(j); i32_const(es as i32); i32_mul; i32_add; local_set(src);
                });
                let val_ty = self.map_val_ty(&args[0].ty);
                self.emit_dict_put_entry(result, r_cap, ib, eb, src, es, ks, vs, &key_ty, &val_ty);
                wasm!(self.func, {
                      local_get(j); i32_const(1); i32_add; local_set(j); br(0);
                    end; end;
                    local_get(result);
                });
                self.scratch.free_i32(src);
                self.scratch.free_i32(j);
                self.scratch.free_i32(eb_b);
                self.scratch.free_i32(eb);
                self.scratch.free_i32(ib);
                self.scratch.free_i32(result);
                self.scratch.free_i32(r_cap);
                self.scratch.free_i32(n);
                self.scratch.free_i32(len_b);
                self.scratch.free_i32(cap_b);
                self.scratch.free_i32(cap_a);
                self.scratch.free_i32(mb);
                self.scratch.free_i32(ma);
            }
            "from_list" => {
                // Build a COD from a List[(K,V)]: size for plen, then put each pair in
                // order (duplicate keys → last value wins, position kept).
                let pair_ty = self.resolve_list_elem(&args[0], None);
                let (ks, vs, key_ty, val_ty) = if let Ty::Tuple(elems) = &pair_ty {
                    (elems.first().map(|t| values::byte_size(t)).unwrap_or(4),
                     elems.get(1).map(|t| values::byte_size(t)).unwrap_or(4),
                     elems.first().cloned().unwrap_or(Ty::String),
                     elems.get(1).cloned().unwrap_or(Ty::Int))
                } else { (4u32, 4u32, Ty::String, Ty::Int) };
                let es = ks + vs;
                let pairs = self.scratch.alloc_i32();
                let plen = self.scratch.alloc_i32();
                let result = self.scratch.alloc_i32();
                let cap = self.scratch.alloc_i32();
                let ib = self.scratch.alloc_i32();
                let eb = self.scratch.alloc_i32();
                let i = self.scratch.alloc_i32();
                let tp = self.scratch.alloc_i32();

                self.emit_expr(&args[0]);
                wasm!(self.func, {
                    local_set(pairs);
                    local_get(pairs); i32_load(0); local_set(plen);
                });
                self.emit_dict_fit_cap(plen, cap);
                self.emit_dict_alloc(result, cap, es);
                self.emit_dict_index_base(result, cap);
                wasm!(self.func, { local_set(ib); });
                self.emit_dict_entries_base(result, cap);
                wasm!(self.func, {
                    local_set(eb);
                    i32_const(0); local_set(i);
                    block_empty; loop_empty;
                      local_get(i); local_get(plen); i32_ge_u; br_if(1);
                      local_get(pairs); i32_const(list_data_off); i32_add;
                      local_get(i); i32_const(4); i32_mul; i32_add;
                      i32_load(0); local_set(tp); // tp = pairs.data[i] = (K,V) tuple ptr
                });
                self.emit_dict_put_entry(result, cap, ib, eb, tp, es, ks, vs, &key_ty, &val_ty);
                wasm!(self.func, {
                      local_get(i); i32_const(1); i32_add; local_set(i); br(0);
                    end; end;
                    local_get(result);
                });
                self.scratch.free_i32(tp);
                self.scratch.free_i32(i);
                self.scratch.free_i32(eb);
                self.scratch.free_i32(ib);
                self.scratch.free_i32(cap);
                self.scratch.free_i32(result);
                self.scratch.free_i32(plen);
                self.scratch.free_i32(pairs);
            }
            "clear" => {
                let scratch = self.scratch.alloc_i32();
                wasm!(self.func, {
                    i32_const(map_hdr);
                    call(self.emitter.rt.alloc); local_set(scratch);
                    local_get(scratch); i32_const(0); i32_store(0);
                    local_get(scratch); i32_const(0); i32_store(map_cap_off);
                });
                // clear allocates a fresh empty map; write it back to the binding —
                // into the cell for a mutable capture. clear -> Unit, so nothing is
                // left on the stack (the old `drop` branches were unreachable: this
                // arm pushes no operand). See emit_mutator_writeback.
                self.emit_mutator_writeback(&args[0], scratch);
                self.scratch.free_i32(scratch);
            }
            _ => return self.emit_map_closure_call(method, args),
        }
        true
    }

    // ── Compact-ordered-dict hash helpers ──

    /// Split hash on stack into h1 (bucket index) → idx_local and h2 (tag) → h2_local.
    pub(super) fn emit_h1_h2(&mut self, cap: u32, idx_local: u32, h2_local: u32) {
        use super::engine::layout::map as lm;
        let ht = self.scratch.alloc_i32();
        wasm!(self.func, {
            local_tee(ht);
            local_get(cap); i32_const(1); i32_sub; i32_and;
            local_set(idx_local);
            local_get(ht);
            i32_const(lm::H2_SHIFT as i32); i32_shr_u; i32_const(lm::H2_MASK as i32); i32_and;
            local_tee(h2_local);
            i32_eqz;
            if_empty; i32_const(1); local_set(h2_local); end; // avoid 0 (empty)
        });
        self.scratch.free_i32(ht);
    }

    pub(super) fn emit_hash_key(&mut self, key_ty: &Ty) {
        use super::engine::layout::{STRING, string as ls};
        let str_data_off = self.emitter.layout_reg.fixed_offset(STRING, ls::DATA) as i32;
        match key_ty {
            Ty::Int => {
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
                let s = self.scratch.alloc_i32();
                let h = self.scratch.alloc_i32();
                let slen = self.scratch.alloc_i32();
                let si = self.scratch.alloc_i32();
                wasm!(self.func, {
                    local_set(s);
                    i32_const(0x811C9DC5u32 as i32); local_set(h);
                    local_get(s); i32_load(0); local_set(slen);
                    i32_const(0); local_set(si);
                    block_empty; loop_empty;
                      local_get(si); local_get(slen); i32_ge_u; br_if(1);
                      local_get(h);
                      local_get(s); i32_const(str_data_off); i32_add;
                      local_get(si); i32_add; i32_load8_u(0);
                      i32_xor;
                      i32_const(0x01000193u32 as i32); i32_mul;
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
            Ty::Bool => {} // identity hash: 0 or 1, already i32
            // Tuple keys, and records/Named-records whose fields include a heap
            // POINTER (e.g. a String or nested list field), must hash by VALUE so
            // two structurally-equal keys built from distinct allocations land in
            // the same bucket — otherwise probing never finds the match even with
            // a correct value-equality. Walk the fields and fold each field's
            // value-hash recursively (`emit_hash_value`). Records with only inline
            // value fields keep the cheaper byte-FNV path below.
            Ty::Tuple(_) => {
                let fields = self.key_struct_fields(key_ty).unwrap_or_default();
                self.emit_hash_fields(&fields);
            }
            Ty::Named(_, _) | Ty::Record { .. } | Ty::Variant { .. } => {
                let struct_fields = self.key_struct_fields(key_ty);
                let has_ptr = struct_fields.as_ref()
                    .map(|fs| fs.iter().any(|(_, t)| !Self::ty_is_inline_value(t)))
                    .unwrap_or(false);
                if let (true, Some(fields)) = (has_ptr, struct_fields) {
                    // Record/Named-record with pointer field(s): value-structural hash.
                    self.emit_hash_fields(&fields);
                } else {
                    // Records and variants are heap structs; FNV-1a over their content
                    // bytes (dereferencing the pointer). For variants this is the tag —
                    // hashing the POINTER would be identity-on-allocation and break value
                    // equality, since each constructor call allocates a fresh struct.
                    let size = self.key_content_size(key_ty);
                    if size == 0 {
                        // No known content layout: identity hash (the key value itself).
                    } else {
                        let ptr = self.scratch.alloc_i32();
                        let h = self.scratch.alloc_i32();
                        wasm!(self.func, {
                            local_set(ptr);
                            i32_const(0x811C9DC5u32 as i32); local_set(h);
                        });
                        for b in 0..size {
                            wasm!(self.func, {
                                local_get(h);
                                local_get(ptr); i32_load8_u(b);
                                i32_xor;
                                i32_const(0x01000193u32 as i32); i32_mul;
                                local_set(h);
                            });
                        }
                        wasm!(self.func, { local_get(h); });
                        self.scratch.free_i32(h);
                        self.scratch.free_i32(ptr);
                    }
                }
            }
            _ => {} // other pointers: identity hash
        }
    }

    /// True if a value of `ty` is stored INLINE (no heap pointer). Strings, lists,
    /// records, tuples, etc. are pointers; ints/floats/bools/narrow-ints are inline.
    /// Used to decide whether a compound key needs value-structural hashing/eq.
    fn ty_is_inline_value(ty: &Ty) -> bool {
        matches!(ty,
            Ty::Int | Ty::Float | Ty::Bool | Ty::Unit
            | Ty::Int8 | Ty::Int16 | Ty::Int32 | Ty::Int64
            | Ty::UInt8 | Ty::UInt16 | Ty::UInt32 | Ty::UInt64
            | Ty::Float32 | Ty::Float64)
    }

    /// Flat (name, ty) fields of a struct-like key (Tuple or Record/Named-record),
    /// laid out sequentially in memory. None for variants and non-struct keys.
    /// Single source of truth so hashing and equality see identical layout.
    fn key_struct_fields(&self, key_ty: &Ty) -> Option<Vec<(String, Ty)>> {
        match key_ty {
            Ty::Tuple(elems) => Some(
                elems.iter().enumerate()
                    .map(|(i, t)| (format!("_{i}"), t.clone()))
                    .collect()),
            Ty::Record { fields } => Some(
                fields.iter().map(|(n, t)| (n.to_string(), t.clone())).collect()),
            Ty::Named(name, _) if !self.emitter.variant_info.contains_key(name.as_str()) => {
                self.emitter.record_fields.get(name.as_str()).cloned()
            }
            _ => None,
        }
    }

    /// Value-structural FNV-1a hash of a struct-like key. Consumes the struct
    /// pointer on the stack, leaves an i32 hash. Folds each field's value-hash
    /// (`emit_hash_value`) at its sequential offset so structurally-equal keys —
    /// even with pointer fields like Strings — hash identically.
    fn emit_hash_fields(&mut self, fields: &[(String, Ty)]) {
        let ptr = self.scratch.alloc_i32();
        let h = self.scratch.alloc_i32();
        wasm!(self.func, {
            local_set(ptr);
            i32_const(0x811C9DC5u32 as i32); local_set(h);
        });
        let mut offset = 0u32;
        for (_, fty) in fields {
            // h = (h ^ field_hash) * FNV_prime
            wasm!(self.func, {
                local_get(ptr);
            });
            self.emit_load_at(fty, offset);
            self.emit_hash_value(fty);
            wasm!(self.func, {
                local_get(h); i32_xor;
                i32_const(0x01000193u32 as i32); i32_mul;
                local_set(h);
            });
            offset += values::byte_size(fty);
        }
        wasm!(self.func, { local_get(h); });
        self.scratch.free_i32(h);
        self.scratch.free_i32(ptr);
    }

    /// Value-structural hash of a single value of `ty` on the stack → i32.
    /// Mirrors `emit_eq_typed`'s notion of equality so hash and eq never disagree:
    /// ints by value, strings by content bytes, tuples/records by their fields.
    fn emit_hash_value(&mut self, ty: &Ty) {
        match ty {
            // Int: reuse the i64 mix from emit_hash_key.
            Ty::Int | Ty::Int64 | Ty::UInt64 => self.emit_hash_key(&Ty::Int),
            // Narrow ints ride in i32 already — fold their value directly.
            Ty::Int8 | Ty::Int16 | Ty::Int32
            | Ty::UInt8 | Ty::UInt16 | Ty::UInt32 | Ty::Bool => { /* value already i32 */ }
            // Float bits → i32 mix (reinterpret then fold high/low halves).
            Ty::Float | Ty::Float64 => {
                let tmp = self.scratch.alloc_i64();
                wasm!(self.func, {
                    i64_reinterpret_f64; local_tee(tmp);
                    i64_const(32); i64_shr_u; local_get(tmp); i64_xor;
                    i32_wrap_i64;
                });
                self.scratch.free_i64(tmp);
            }
            Ty::Float32 => { wasm!(self.func, { i32_reinterpret_f32; }); }
            Ty::String | Ty::Bytes => self.emit_hash_key(&Ty::String),
            Ty::Tuple(_) | Ty::Record { .. } | Ty::Named(_, _) | Ty::Variant { .. } => {
                self.emit_hash_key(ty);
            }
            // Other heap types (List, Option, …): identity hash of the pointer.
            // Rare as nested key fields; equality still recurses structurally, so a
            // weaker hash only costs probe collisions, never correctness.
            _ => {}
        }
    }

    // ── Map type helpers ──

    pub(super) fn map_kv_sizes(&self, ty: &Ty) -> (u32, u32) {
        if let Ty::Applied(_, args) = ty {
            (args.first().map(|t| values::byte_size(t)).unwrap_or(4),
             args.get(1).map(|t| values::byte_size(t)).unwrap_or(4))
        } else { (4, 4) }
    }
    pub(super) fn map_val_ty(&self, ty: &Ty) -> Ty {
        if let Ty::Applied(_, args) = ty { args.get(1).cloned().unwrap_or(Ty::Int) } else { Ty::Int }
    }
    pub(super) fn map_key_ty(&self, ty: &Ty) -> Ty {
        if let Ty::Applied(_, args) = ty { args.first().cloned().unwrap_or(Ty::String) } else { Ty::String }
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
            // Tuple keys, and records/Named-records with a heap POINTER field
            // (String / nested list), need STRUCTURAL equality — `mem_eq` would
            // compare the field's pointer bytes, not its contents, so two equal-
            // content keys built from distinct allocations would miss. Route through
            // the shared type-directed deep equality (matching `emit_hash_key`, which
            // hashes these by value too, so hash and eq stay consistent).
            Ty::Tuple(_) => { self.emit_eq_typed(key_ty); }
            Ty::Named(_, _) | Ty::Record { .. } | Ty::Variant { .. } => {
                let struct_fields = self.key_struct_fields(key_ty);
                let has_ptr = struct_fields.as_ref()
                    .map(|fs| fs.iter().any(|(_, t)| !Self::ty_is_inline_value(t)))
                    .unwrap_or(false);
                if has_ptr {
                    self.emit_eq_typed(key_ty);
                } else {
                    // Compare the dereferenced content (matching emit_hash_key's coverage so
                    // hash and equality stay consistent): full record bytes, or a variant's
                    // tag. byte_size(record/variant) is only the pointer size (4), so the old
                    // mem_eq(4) compared just the first 4 content bytes. When the layout is
                    // unknown, fall back to pointer identity (matching the identity hash).
                    let size = self.key_content_size(key_ty);
                    if size == 0 {
                        wasm!(self.func, { i32_eq; });
                    } else {
                        wasm!(self.func, { i32_const(size as i32); call(self.emitter.rt.mem_eq); });
                    }
                }
            }
            _ => { wasm!(self.func, { i32_eq; }); }
        }
    }
    /// Resolve a record/Named key's fields (name, type), or empty if the layout is
    /// unknown. Single source of truth for hashing AND equality so they can't drift.
    fn record_key_fields(&self, key_ty: &Ty) -> Vec<(String, Ty)> {
        if let Ty::Record { fields } = key_ty {
            fields.iter().map(|(n, t)| (n.to_string(), t.clone())).collect::<Vec<_>>()
        } else if let Ty::Named(name, _) = key_ty {
            self.emitter.record_fields.get(name.as_str()).cloned().unwrap_or_default()
        } else if let Some(fs) = self.emitter.record_fields.get("") {
            fs.clone()
        } else {
            vec![]
        }
    }
    /// Byte count of a heap key's content for hashing/equality (0 if unknown).
    /// Variants hash/compare their TAG only: each constructor allocates a fresh
    /// struct (so the pointer is not stable) and the payload padding to the
    /// variant's max size is uninitialized (so comparing it is non-deterministic);
    /// the tag uniquely identifies a nullary case. Records use their full field size.
    fn key_content_size(&self, key_ty: &Ty) -> u32 {
        match key_ty {
            Ty::Variant { .. } => 4,
            Ty::Named(name, _) if self.emitter.variant_info.contains_key(name.as_str()) => 4,
            _ => self.record_key_fields(key_ty).iter().map(|(_, t)| super::values::byte_size(t)).sum(),
        }
    }
    pub(super) fn emit_search_key_store(&mut self, key_ty: &Ty, s32: u32, s64: u32) {
        match key_ty { Ty::Int => { wasm!(self.func, { local_set(s64); }); } _ => { wasm!(self.func, { local_set(s32); }); } }
    }
    pub(super) fn emit_search_key_load(&mut self, key_ty: &Ty, s32: u32, s64: u32) {
        match key_ty { Ty::Int => { wasm!(self.func, { local_get(s64); }); } _ => { wasm!(self.func, { local_get(s32); }); } }
    }
    pub(super) fn key_valtype(key_ty: &Ty) -> ValType {
        match key_ty { Ty::Int => ValType::I64, _ => ValType::I32 }
    }
    pub(super) fn emit_elem_copy_sized(&mut self, size: u32) {
        match size {
            8 => { wasm!(self.func, { i64_load(0); i64_store(0); }); }
            4 => { wasm!(self.func, { i32_load(0); i32_store(0); }); }
            _ => { wasm!(self.func, { i32_load(0); i32_store(0); }); }
        }
    }

    // ── Compact-ordered-dict addressing — every offset by name, zero magic numbers.
    // Layout (SWISS_MAP id, header_size=8):
    //   [len@0][cap@4][tags:u8[cap]@8][index:i32[cap]@8+cap][entries:(K,V)[cap]@8+cap+cap*INDEX_SLOT_SIZE]
    // tags = h2 fast-reject (0=empty); index 1-based (slot v → entries[v-1], 0=empty);
    // entries dense, insertion order [0..len], stride es=ks+vs, key@0 val@ks.

    /// Push the INDEX region base `map + header + cap` (slots start after the tags).
    pub(super) fn emit_dict_index_base(&mut self, map: u32, cap: u32) {
        let hdr = self.emitter.layout_reg.header_size(super::engine::layout::SWISS_MAP) as i32;
        wasm!(self.func, { local_get(map); i32_const(hdr); i32_add; local_get(cap); i32_add; });
    }

    /// Push the dense ENTRIES base `map + header + cap + cap*INDEX_SLOT_SIZE`.
    pub(super) fn emit_dict_entries_base(&mut self, map: u32, cap: u32) {
        use super::engine::layout::{SWISS_MAP, map as lm};
        let hdr = self.emitter.layout_reg.header_size(SWISS_MAP) as i32;
        wasm!(self.func, {
            local_get(map); i32_const(hdr); i32_add;
            local_get(cap); i32_add;
            local_get(cap); i32_const(lm::INDEX_SLOT_SIZE as i32); i32_mul; i32_add;
        });
    }

    /// Allocate + zero-init a COD table of `cap` slots (entry stride `es`) into `out`.
    /// total = header + cap*(tag(1) + INDEX_SLOT_SIZE + es); len=0; cap=cap; tags+index zeroed
    /// (the allocator reuses a free list, so it does NOT return zeroed memory).
    pub(super) fn emit_dict_alloc(&mut self, out: u32, cap: u32, es: u32) {
        use super::engine::layout::{SWISS_MAP, map as lm};
        let hdr = self.emitter.layout_reg.header_size(SWISS_MAP) as i32;
        let cap_off = self.emitter.layout_reg.fixed_offset(SWISS_MAP, lm::CAP);
        let per_slot = 1 + lm::INDEX_SLOT_SIZE as i32 + es as i32;
        let tag_plus_index = 1 + lm::INDEX_SLOT_SIZE as i32;
        wasm!(self.func, {
            i32_const(hdr); local_get(cap); i32_const(per_slot); i32_mul; i32_add;
            call(self.emitter.rt.alloc); local_set(out);
            local_get(out); i32_const(0); i32_store(0);             // len = 0
            local_get(out); local_get(cap); i32_store(cap_off, 0);  // cap
            // zero tags+index: memory_fill(out+header, 0, cap*(1+INDEX_SLOT_SIZE))
            local_get(out); i32_const(hdr); i32_add;
            i32_const(0);
            local_get(cap); i32_const(tag_plus_index); i32_mul;
            memory_fill(0);
        });
    }

    /// Grow `cap_out` (a local) to the smallest pow2 ≥ INITIAL_CAP that keeps
    /// `n` entries under the load factor: n*LOAD_DEN ≤ cap*LOAD_NUM.
    pub(super) fn emit_dict_fit_cap(&mut self, n: u32, cap_out: u32) {
        use super::engine::layout::map as lm;
        wasm!(self.func, {
            i32_const(lm::INITIAL_CAP as i32); local_set(cap_out);
            block_empty; loop_empty;
              local_get(n); i32_const(lm::LOAD_DEN as i32); i32_mul;
              local_get(cap_out); i32_const(lm::LOAD_NUM as i32); i32_mul;
              i32_le_u; br_if(1);
              local_get(cap_out); i32_const(1); i32_shl; local_set(cap_out);
              br(0);
            end; end;
        });
    }

    /// Rebuild the hash index (tags + index slots) of a COD table whose dense
    /// entries[0..len] are already populated and whose tags+index are zeroed.
    /// For each dense entry i: hash its key, probe for an empty slot, write
    /// tags[slot]=h2 and index[slot]=i+1 (1-based pointer back to the entry).
    pub(super) fn emit_dict_rebuild_index(&mut self, map: u32, cap: u32, es: u32, key_ty: &Ty) {
        use super::engine::layout::{SWISS_MAP, map as lm};
        let tags_off = self.emitter.layout_reg.fixed_offset(SWISS_MAP, lm::TAGS) as i32;
        let len = self.scratch.alloc_i32();
        let i = self.scratch.alloc_i32();
        let eb = self.scratch.alloc_i32();
        let ib = self.scratch.alloc_i32();
        let idx = self.scratch.alloc_i32();
        let h2 = self.scratch.alloc_i32();
        wasm!(self.func, { local_get(map); i32_load(0); local_set(len); });
        self.emit_dict_index_base(map, cap);
        wasm!(self.func, { local_set(ib); });
        self.emit_dict_entries_base(map, cap);
        wasm!(self.func, {
            local_set(eb);
            i32_const(0); local_set(i);
            block_empty; loop_empty;
              local_get(i); local_get(len); i32_ge_u; br_if(1);
              local_get(eb); local_get(i); i32_const(es as i32); i32_mul; i32_add;
        });
        self.emit_key_load(key_ty, 0);
        self.emit_hash_key(key_ty);
        self.emit_h1_h2(cap, idx, h2);
        wasm!(self.func, {
              block_empty; loop_empty;
                local_get(map); i32_const(tags_off); i32_add;
                local_get(idx); i32_add; i32_load8_u(0); i32_eqz; br_if(1);
                local_get(idx); i32_const(1); i32_add;
                local_get(cap); i32_const(1); i32_sub; i32_and; local_set(idx); br(0);
              end; end;
              local_get(map); i32_const(tags_off); i32_add; local_get(idx); i32_add;
              local_get(h2); i32_store8(0);
              local_get(ib); local_get(idx); i32_const(lm::INDEX_SLOT_SIZE as i32); i32_mul; i32_add;
              local_get(i); i32_const(1); i32_add; i32_store(0);
              local_get(i); i32_const(1); i32_add; local_set(i); br(0);
            end; end;
        });
        self.scratch.free_i32(h2);
        self.scratch.free_i32(idx);
        self.scratch.free_i32(ib);
        self.scratch.free_i32(eb);
        self.scratch.free_i32(i);
        self.scratch.free_i32(len);
    }

    /// Copy `src`'s dense entries (src.len of them) into the freshly-alloced `dst`
    /// table, set dst.len = src.len, and rebuild dst's index. `dst` must already
    /// be `emit_dict_alloc`-ed at `dst_cap` (entries stride `es`, tags+index zeroed).
    pub(super) fn emit_dict_recap(&mut self, src: u32, src_cap: u32, dst: u32, dst_cap: u32, es: u32, key_ty: &Ty) {
        let len = self.scratch.alloc_i32();
        wasm!(self.func, { local_get(src); i32_load(0); local_set(len); });
        self.emit_dict_entries_base(dst, dst_cap); // memory_copy dest
        self.emit_dict_entries_base(src, src_cap); // memory_copy src
        wasm!(self.func, {
            local_get(len); i32_const(es as i32); i32_mul; memory_copy;
            local_get(dst); local_get(len); i32_store(0); // dst.len = src.len
        });
        self.emit_dict_rebuild_index(dst, dst_cap, es, key_ty);
        self.scratch.free_i32(len);
    }

    /// Grow a COD table in place: quadruple `cap`, recap into the bigger table,
    /// and update the `map`/`cap` locals to point at the new table.
    pub(super) fn emit_dict_grow(&mut self, map: u32, cap: u32, es: u32, key_ty: &Ty) {
        use super::engine::layout::map as lm;
        let nc = self.scratch.alloc_i32();
        let nm = self.scratch.alloc_i32();
        wasm!(self.func, { local_get(cap); i32_const(lm::GROWTH_SHIFT as i32); i32_shl; local_set(nc); });
        self.emit_dict_alloc(nm, nc, es);
        self.emit_dict_recap(map, cap, nm, nc, es, key_ty);
        wasm!(self.func, { local_get(nm); local_set(map); local_get(nc); local_set(cap); });
        self.scratch.free_i32(nm);
        self.scratch.free_i32(nc);
    }

    /// The single COD insertion workhorse. `src` points to a contiguous `(key,val)`
    /// entry (key@0, val@ks, total `es` bytes). Probe `map`'s index for the key:
    /// existing → overwrite its value in place (dense position kept); new → append
    /// at entries[len], write tags[slot]=h2, index[slot]=len+1, bump len. Assumes
    /// the caller has reserved capacity (no grow). `ib`/`eb` are the index/entries bases.
    pub(super) fn emit_dict_put_entry(&mut self, map: u32, cap: u32, ib: u32, eb: u32, src: u32, es: u32, ks: u32, vs: u32, key_ty: &Ty, val_ty: &Ty) {
        use super::engine::layout::{SWISS_MAP, map as lm};
        let tags_off = self.emitter.layout_reg.fixed_offset(SWISS_MAP, lm::TAGS) as i32;
        let idx = self.scratch.alloc_i32();
        let h2 = self.scratch.alloc_i32();
        let tg = self.scratch.alloc_i32();
        let ei = self.scratch.alloc_i32();
        wasm!(self.func, { local_get(src); });
        self.emit_key_load(key_ty, 0);
        self.emit_hash_key(key_ty);
        self.emit_h1_h2(cap, idx, h2);
        wasm!(self.func, {
            block_empty; loop_empty;
              local_get(map); i32_const(tags_off); i32_add; local_get(idx); i32_add; i32_load8_u(0); local_set(tg);
              local_get(tg); i32_eqz; br_if(1);              // empty slot → new key
              local_get(tg); local_get(h2); i32_eq;
              if_empty;
                local_get(ib); local_get(idx); i32_const(lm::INDEX_SLOT_SIZE as i32); i32_mul; i32_add;
                i32_load(0); i32_const(1); i32_sub; local_set(ei);   // ei = index[idx]-1
                local_get(eb); local_get(ei); i32_const(es as i32); i32_mul; i32_add;
        });
        self.emit_key_load(key_ty, 0);          // existing entry key
        wasm!(self.func, { local_get(src); });
        self.emit_key_load(key_ty, 0);          // src key
        self.emit_key_eq(key_ty);
        wasm!(self.func, {
                br_if(2);                        // equal → existing, ei set, exit probe
              end;
              local_get(idx); i32_const(1); i32_add;
              local_get(cap); i32_const(1); i32_sub; i32_and; local_set(idx); br(0);
            end; end;
            // New key (tg==0): append at dense entries[len].
            local_get(tg); i32_eqz;
            if_empty;
              local_get(map); i32_load(0); local_set(ei);          // ei = len
              local_get(map); i32_const(tags_off); i32_add; local_get(idx); i32_add;
              local_get(h2); i32_store8(0);                        // tags[idx] = h2
              local_get(ib); local_get(idx); i32_const(lm::INDEX_SLOT_SIZE as i32); i32_mul; i32_add;
              local_get(ei); i32_const(1); i32_add; i32_store(0);  // index[idx] = ei+1
              local_get(map); local_get(ei); i32_const(1); i32_add; i32_store(0); // map.len = ei+1
              local_get(eb); local_get(ei); i32_const(es as i32); i32_mul; i32_add;
              local_get(src); i32_const(ks as i32); memory_copy;   // copy key bytes
        });
        // SHARE: a NEW heap key was just copied (by reference) from the borrowed
        // source — dup it so the dict owns its own reference, else the source's
        // scope-end Dec deep-frees the key the dict now holds (double-free). Only on
        // the new-key path (an existing key keeps the reference it already owns).
        if Self::is_heap_type(key_ty) {
            wasm!(self.func, {
              local_get(eb); local_get(ei); i32_const(es as i32); i32_mul; i32_add;
              i32_load(0); call(self.emitter.rt.rc_inc); drop;
            });
        }
        wasm!(self.func, {
            end;
            // Copy value bytes into entries[ei]+ks (both new and existing).
            local_get(eb); local_get(ei); i32_const(es as i32); i32_mul; i32_add; i32_const(ks as i32); i32_add;
            local_get(src); i32_const(ks as i32); i32_add;
            i32_const(vs as i32); memory_copy;
        });
        // SHARE: the heap value was copied (by reference) from the borrowed source —
        // dup it for the same reason. (Overwriting an existing heap value leaks the
        // old one — a separate, non-crashing gap, not a double-free.)
        if Self::is_heap_type(val_ty) {
            wasm!(self.func, {
              local_get(eb); local_get(ei); i32_const(es as i32); i32_mul; i32_add; i32_const(ks as i32); i32_add;
              i32_load(0); call(self.emitter.rt.rc_inc); drop;
            });
        }
        self.scratch.free_i32(ei);
        self.scratch.free_i32(tg);
        self.scratch.free_i32(h2);
        self.scratch.free_i32(idx);
    }

    /// Push a closure pair's env pointer (field load via the CLOSURE_PAIR layout).
    pub(super) fn emit_closure_env(&mut self, closure: u32) {
        use super::engine::layout::{CLOSURE_PAIR, closure as lc};
        let off = self.emitter.layout_reg.fixed_offset(CLOSURE_PAIR, lc::ENV_PTR);
        wasm!(self.func, { local_get(closure); i32_load(off); });
    }

    /// Push a closure pair's function-table index.
    pub(super) fn emit_closure_table_idx(&mut self, closure: u32) {
        use super::engine::layout::{CLOSURE_PAIR, closure as lc};
        let off = self.emitter.layout_reg.fixed_offset(CLOSURE_PAIR, lc::TABLE_IDX);
        wasm!(self.func, { local_get(closure); i32_load(off); });
    }
}
