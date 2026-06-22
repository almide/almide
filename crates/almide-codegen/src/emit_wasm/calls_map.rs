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
                self.emit_elem_copy_sized(vs, false);
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
                self.emit_dict_recap(m, old_cap, nm, cap, es, &key_ty, Some((&key_ty, &val_ty)));
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
                });
                // SHARE dup: the source dict survives `remove` and owns the
                // kept entries — the survivor copy needs its own references.
                {
                    let kh = crate::pass_perceus::is_heap_type(&key_ty);
                    let vh = crate::pass_perceus::is_heap_type(&self.map_val_ty(&args[0].ty));
                    if kh {
                        wasm!(self.func, {
                        local_get(eb_new); local_get(ne); i32_const(es as i32); i32_mul; i32_add;
                        i32_load(0); call(self.emitter.rt.rc_inc); drop;
                        });
                    }
                    if vh {
                        wasm!(self.func, {
                        local_get(eb_new); local_get(ne); i32_const(es as i32); i32_mul; i32_add;
                        i32_const(ks as i32); i32_add;
                        i32_load(0); call(self.emitter.rt.rc_inc); drop;
                        });
                    }
                }
                wasm!(self.func, {
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
                        // SHARE: the dict survives and owns these keys.
                        let dup = crate::pass_perceus::is_heap_type(&self.map_key_ty(&args[0].ty));
                        self.emit_elem_copy_sized(ks, dup);
                    }
                    "values" => {
                        wasm!(self.func, {
                            local_get(result); i32_const(list_data_off); i32_add;
                            local_get(i); i32_const(vs as i32); i32_mul; i32_add;
                            local_get(eb); local_get(i); i32_const(es as i32); i32_mul; i32_add;
                            i32_const(ks as i32); i32_add;
                        });
                        let dup = crate::pass_perceus::is_heap_type(&self.map_val_ty(&args[0].ty));
                        self.emit_elem_copy_sized(vs, dup);
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
                let val_ty = self.map_val_ty(&args[0].ty);
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
                self.emit_dict_recap(ma, cap_a, result, r_cap, es, &key_ty, Some((&key_ty, &val_ty)));
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
}

include!("calls_map_p2.rs");
include!("calls_map_p3.rs");
