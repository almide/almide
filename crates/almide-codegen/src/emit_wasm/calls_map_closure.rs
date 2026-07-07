//! Map closure-based stdlib call dispatch for WASM codegen.
//!
//! Handles: fold, each, any, all, count, filter, map, find, update.

use super::FuncCompiler;
use super::values;
use almide_ir::IrExpr;
use wasm_encoder::ValType;

impl FuncCompiler<'_> {
    pub(super) fn emit_map_closure_call(&mut self, method: &str, args: &[IrExpr]) -> bool {
        match method {
            "fold" => {
                let (ks, vs) = self.map_kv_sizes(&args[0].ty);
                let key_ty = self.map_key_ty(&args[0].ty);
                let val_ty = self.map_val_ty(&args[0].ty);
                // The accumulator's wasm type is the init's type (== fold result),
                // NOT always i64 — a String/record/list accumulator is an i32 ptr,
                // a Float is f64. Hardcoding i64 broke the wasm module (validation
                // "expected i64, found i32") for any non-Int accumulator.
                let acc_vt = values::ty_to_valtype(&args[1].ty).unwrap_or(ValType::I32);
                // A HEAP accumulator needs explicit ownership through the loop:
                // the closure returns an OWNED value (fn returns are +1), so after
                // every call the PREVIOUS acc we owned must be released, and the
                // init must be acquired (+1) when stored so the zero-iteration
                // path hands back an owned handle too. Previously the empty-map
                // fold returned the init var's handle BARE — the caller's temp
                // dec then double-freed it against the var's own release (fuzz
                // #727: `map.fold(map.get_or(...), init, f)` printed [:] /
                // garbage and tripped the rc sentinel).
                let acc_heap = Self::is_heap_type(&args[1].ty);
                let acc = self.scratch.alloc(acc_vt);
                let closure = self.scratch.alloc_i32();
                let it = self.map_iter_begin(&args[0], ks + vs);
                self.emit_expr(&args[1]); // init
                if acc_heap {
                    wasm!(self.func, { call(self.emitter.rt.rc_inc); });
                }
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
                    let mut ct = vec![ValType::I32, acc_vt, key_vt];
                    if let Some(vt) = values::ty_to_valtype(&val_ty) { ct.push(vt); }
                    self.emit_call_indirect(ct, vec![acc_vt]);
                }
                if acc_heap {
                    // Release the pre-call acc we owned; the call's result is the
                    // new owned acc (identity closures return the SAME handle
                    // +1'd, so the dec still balances).
                    let newacc = self.scratch.alloc_i32();
                    wasm!(self.func, {
                        local_set(newacc);
                        local_get(acc); call(self.emitter.rt.rc_dec);
                        local_get(newacc); local_set(acc);
                    });
                    self.scratch.free_i32(newacc);
                } else {
                    wasm!(self.func, { local_set(acc); });
                }
                self.map_iter_loop_tail(&it);
                wasm!(self.func, { local_get(acc); });
                self.scratch.free(acc, acc_vt);
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
                // filter(m, pred) → Map: dense-walk the source entries[0..len] in
                // insertion order; for each entry where pred(k, v) is true, append it
                // to a fresh COD table (source keys are unique, so put_entry always
                // appends — order preserved). Dest cap = source cap (filter never grows).
                use super::engine::layout::{SWISS_MAP, map as lm};
                let map_cap_off = self.emitter.layout_reg.fixed_offset(SWISS_MAP, lm::CAP);
                let (ks, vs) = self.map_kv_sizes(&args[0].ty);
                let key_ty = self.map_key_ty(&args[0].ty);
                let val_ty = self.map_val_ty(&args[0].ty);
                let es = ks + vs;
                let m = self.scratch.alloc_i32();
                let closure = self.scratch.alloc_i32();
                let cap = self.scratch.alloc_i32();
                let len = self.scratch.alloc_i32();
                let nm = self.scratch.alloc_i32();
                let ib = self.scratch.alloc_i32();
                let eb_old = self.scratch.alloc_i32();
                let eb_new = self.scratch.alloc_i32();
                let i = self.scratch.alloc_i32();
                let entry = self.scratch.alloc_i32();

                self.emit_expr(&args[0]);
                wasm!(self.func, { local_set(m); });
                self.emit_expr(&args[1]);
                wasm!(self.func, {
                    local_set(closure);
                    local_get(m); i32_load(map_cap_off); local_set(cap);
                    local_get(m); i32_load(0); local_set(len);
                });
                self.emit_dict_alloc(nm, cap, es);
                self.emit_dict_index_base(nm, cap);
                wasm!(self.func, { local_set(ib); });
                self.emit_dict_entries_base(nm, cap);
                wasm!(self.func, { local_set(eb_new); });
                self.emit_dict_entries_base(m, cap);
                wasm!(self.func, {
                    local_set(eb_old);
                    i32_const(0); local_set(i);
                    block_empty; loop_empty;
                      local_get(i); local_get(len); i32_ge_u; br_if(1);
                      local_get(eb_old); local_get(i); i32_const(es as i32); i32_mul; i32_add; local_set(entry);
                      // pred(env, k, v)
                      local_get(closure); i32_load(4); // env
                      local_get(entry);
                });
                self.emit_key_load(&key_ty, 0);
                wasm!(self.func, { local_get(entry); i32_const(ks as i32); i32_add; });
                self.emit_load_at(&val_ty, 0);
                wasm!(self.func, { local_get(closure); i32_load(0); }); // table_idx
                {
                    let key_vt = Self::key_valtype(&key_ty);
                    let mut ct = vec![ValType::I32, key_vt];
                    if let Some(vt) = values::ty_to_valtype(&val_ty) { ct.push(vt); }
                    self.emit_call_indirect(ct, vec![ValType::I32]);
                }
                wasm!(self.func, { if_empty; }); // pred true → keep
                self.emit_dict_put_entry(nm, cap, ib, eb_new, entry, es, ks, vs, &key_ty, &val_ty);
                wasm!(self.func, {
                      end; // pred-true if
                      local_get(i); i32_const(1); i32_add; local_set(i); br(0);
                    end; end;
                    local_get(nm);
                });
                self.scratch.free_i32(entry);
                self.scratch.free_i32(i);
                self.scratch.free_i32(eb_new);
                self.scratch.free_i32(eb_old);
                self.scratch.free_i32(ib);
                self.scratch.free_i32(nm);
                self.scratch.free_i32(len);
                self.scratch.free_i32(cap);
                self.scratch.free_i32(closure);
                self.scratch.free_i32(m);
            }
            "map" => {
                // map(m, f) → Map[K, B]: rebuild the COD table transforming each
                // value via f. The closure's return type B can differ from the
                // source value type V — so the *new* value valtype/width follows
                // B, the call_indirect signature is (V)->B, and the rebuilt table
                // uses the new entry stride es_new = ks + vs_new. Keys, tags, and
                // the hash index are value-width-independent, so they are copied
                // verbatim (insertion order + lookups preserved). When B == V this
                // degenerates to the old same-width copy-and-transform.
                use super::engine::layout::map as lm;
                let (ks, vs) = self.map_kv_sizes(&args[0].ty);
                let val_ty = self.map_val_ty(&args[0].ty);
                // Closure return type (the NEW value type). Fall back to the
                // source value type when inference left it unresolved.
                let new_val_ty = self.resolve_closure_ret_ty(&args[1], &val_ty);
                let vs_new = values::byte_size(&new_val_ty);
                let es_old = ks + vs;
                let es_new = ks + vs_new;
                let closure = self.scratch.alloc_i32();
                let it = self.map_iter_begin(&args[0], es_old);
                self.emit_expr(&args[1]);
                wasm!(self.func, { local_set(closure); });
                // Allocate a fresh table at the new entry stride, then copy the
                // value-width-independent prefix (header + tags + index) verbatim
                // from the source so len/cap/tags/index match exactly.
                let new_map = self.scratch.alloc_i32();
                self.emit_dict_alloc(new_map, it.cap, es_new);
                // prefix_size = header + cap*(tag(1) + INDEX_SLOT_SIZE)
                let hdr = self.emitter.layout_reg.header_size(super::engine::layout::SWISS_MAP) as i32;
                let tag_plus_index = 1 + lm::INDEX_SLOT_SIZE as i32;
                wasm!(self.func, {
                    local_get(new_map);
                    local_get(it.map);
                    i32_const(hdr);
                    local_get(it.cap); i32_const(tag_plus_index); i32_mul; i32_add;
                    memory_copy;
                });
                let new_eb = self.scratch.alloc_i32();
                self.emit_dict_entries_base(new_map, it.cap);
                wasm!(self.func, { local_set(new_eb); });
                self.map_iter_loop_head(&it);
                // Copy the key bytes (unchanged) from source entry i into dst entry i.
                wasm!(self.func, {
                    local_get(new_eb);
                    local_get(it.i); i32_const(es_new as i32); i32_mul; i32_add;
                    local_get(it.eb);
                    local_get(it.i); i32_const(es_old as i32); i32_mul; i32_add;
                    i32_const(ks as i32); memory_copy;
                });
                // SHARE: the source dict survives and owns these keys.
                if crate::pass_perceus::is_heap_type(&self.map_key_ty(&args[0].ty)) {
                    wasm!(self.func, {
                        local_get(new_eb);
                        local_get(it.i); i32_const(es_new as i32); i32_mul; i32_add;
                        i32_load(0); call(self.emitter.rt.rc_inc); drop;
                    });
                }
                // dst = new_eb + i*es_new + ks (new val addr in the rebuilt table)
                wasm!(self.func, {
                    local_get(new_eb);
                    local_get(it.i); i32_const(es_new as i32); i32_mul; i32_add;
                    i32_const(ks as i32); i32_add;
                });
                // f(env, old_val) → new_val, then store at the new value width.
                wasm!(self.func, { local_get(closure); i32_load(4); });
                self.map_iter_val_addr(&it, ks);
                self.emit_load_at(&val_ty, 0);
                wasm!(self.func, { local_get(closure); i32_load(0); });
                self.emit_closure_call(&val_ty, &new_val_ty);
                self.emit_store_at(&new_val_ty, 0);
                self.map_iter_loop_tail(&it);
                wasm!(self.func, { local_get(new_map); });
                self.scratch.free_i32(new_eb);
                self.scratch.free_i32(new_map);
                self.scratch.free_i32(closure);
                self.map_iter_end(it);
            }
            "find" => {
                // find(m, pred) → Option[(K, V)]: dense-walk entries[0..len] in
                // insertion order; return the first (k, v) where pred(k, v) is true.
                use super::engine::layout::{SWISS_MAP, map as lm};
                let map_cap_off = self.emitter.layout_reg.fixed_offset(SWISS_MAP, lm::CAP);
                let (ks, vs) = self.map_kv_sizes(&args[0].ty);
                let key_ty = self.map_key_ty(&args[0].ty);
                let val_ty = self.map_val_ty(&args[0].ty);
                let entry = ks + vs;
                let map_ptr = self.scratch.alloc_i32();
                let closure = self.scratch.alloc_i32();
                let i = self.scratch.alloc_i32();
                let len = self.scratch.alloc_i32();
                let cap = self.scratch.alloc_i32();
                let eb = self.scratch.alloc_i32();
                let result = self.scratch.alloc_i32();
                let tuple_ptr = self.scratch.alloc_i32();
                let ent = self.scratch.alloc_i32();
                self.emit_expr(&args[0]);
                wasm!(self.func, { local_set(map_ptr); });
                self.emit_expr(&args[1]);
                wasm!(self.func, {
                    local_set(closure);
                    local_get(map_ptr); i32_load(0); local_set(len);
                    local_get(map_ptr); i32_load(map_cap_off); local_set(cap);
                });
                self.emit_dict_entries_base(map_ptr, cap);
                wasm!(self.func, {
                    local_set(eb);
                    i32_const(0); local_set(i);
                    i32_const(0); local_set(result); // none
                    block_empty; loop_empty;
                      local_get(i); local_get(len); i32_ge_u; br_if(1);
                      local_get(eb); local_get(i); i32_const(entry as i32); i32_mul; i32_add; local_set(ent);
                      local_get(closure); i32_load(4);
                      local_get(ent);
                });
                self.emit_key_load(&key_ty, 0);
                wasm!(self.func, { local_get(ent); i32_const(ks as i32); i32_add; });
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
                        // Alloc the (key, val) tuple (one contiguous block) and wrap in Some.
                        i32_const(entry as i32); call(self.emitter.rt.alloc); local_set(tuple_ptr);
                        local_get(tuple_ptr); local_get(ent); i32_const(entry as i32); memory_copy;
                        i32_const(4); call(self.emitter.rt.alloc); local_set(result);
                        local_get(result); local_get(tuple_ptr); i32_store(0);
                        br(2); // break out of loop
                      end;
                      local_get(i); i32_const(1); i32_add; local_set(i);
                      br(0);
                    end; end;
                    local_get(result); // result (none=0 or some ptr)
                });
                self.scratch.free_i32(ent);
                self.scratch.free_i32(tuple_ptr);
                self.scratch.free_i32(result);
                self.scratch.free_i32(eb);
                self.scratch.free_i32(cap);
                self.scratch.free_i32(len);
                self.scratch.free_i32(i);
                self.scratch.free_i32(closure);
                self.scratch.free_i32(map_ptr);
            }
            "update" => {
                // update(m, key, f) → Map: byte-copy the whole COD table (keys/tags/
                // index unchanged), dense-scan for the key (found_idx = dense entry
                // index), then apply f to the value at that dense position in the copy.
                use super::engine::layout::{SWISS_MAP, map as lm};
                let map_cap_off = self.emitter.layout_reg.fixed_offset(SWISS_MAP, lm::CAP);
                let map_hdr = self.emitter.layout_reg.header_size(SWISS_MAP) as i32;
                let (ks, vs) = self.map_kv_sizes(&args[0].ty);
                let key_ty = self.map_key_ty(&args[0].ty);
                let val_ty = self.map_val_ty(&args[0].ty);
                let entry = ks + vs;
                let per_slot = 1 + lm::INDEX_SLOT_SIZE as i32 + entry as i32;
                let map_ptr = self.scratch.alloc_i32();
                let closure = self.scratch.alloc_i32();
                let sk32 = self.scratch.alloc_i32();
                let sk64 = self.scratch.alloc_i64();
                let i = self.scratch.alloc_i32();
                let len = self.scratch.alloc_i32();
                let cap = self.scratch.alloc_i32();
                let eb = self.scratch.alloc_i32();
                let found_idx = self.scratch.alloc_i32();
                let new_map = self.scratch.alloc_i32();
                let total_size = self.scratch.alloc_i32();
                let valaddr = self.scratch.alloc_i32();
                self.emit_expr(&args[0]);
                wasm!(self.func, { local_set(map_ptr); });
                self.emit_expr(&args[1]); // key
                self.emit_search_key_store(&key_ty, sk32, sk64);
                self.emit_expr(&args[2]); // closure f
                wasm!(self.func, {
                    local_set(closure);
                    local_get(map_ptr); i32_load(0); local_set(len);
                    local_get(map_ptr); i32_load(map_cap_off); local_set(cap);
                });
                self.emit_dict_entries_base(map_ptr, cap);
                wasm!(self.func, {
                    local_set(eb);
                    i32_const(0); local_set(i);
                    i32_const(-1); local_set(found_idx);
                    block_empty; loop_empty;
                      local_get(i); local_get(len); i32_ge_u; br_if(1);
                      local_get(eb); local_get(i); i32_const(entry as i32); i32_mul; i32_add;
                });
                self.emit_key_load(&key_ty, 0);
                self.emit_search_key_load(&key_ty, sk32, sk64);
                self.emit_key_eq(&key_ty);
                wasm!(self.func, {
                      if_empty; local_get(i); local_set(found_idx); br(2); end; // found → break
                      local_get(i); i32_const(1); i32_add; local_set(i);
                      br(0);
                    end; end;
                    // If not found, return the original map unchanged —
                    // SHARE: the result is a SECOND reference to the input.
                    local_get(found_idx); i32_const(0); i32_lt_s;
                    if_i32; local_get(map_ptr); call(self.emitter.rt.rc_inc);
                    else_;
                      // Byte-copy the whole COD table: header + cap*per_slot.
                      i32_const(map_hdr);
                      local_get(cap); i32_const(per_slot); i32_mul; i32_add;
                      local_tee(total_size);
                      call(self.emitter.rt.alloc); local_set(new_map);
                      local_get(new_map); local_get(map_ptr); local_get(total_size);
                      memory_copy;
                });
                // SHARE dup over the copied table: the source survives and
                // owns every entry; the copy needs its own references. The
                // updated slot's OLD value ref is released below before the
                // closure result overwrites it.
                {
                    let kh = crate::pass_perceus::is_heap_type(&key_ty);
                    let vh = crate::pass_perceus::is_heap_type(&val_ty);
                    if kh || vh {
                        let di = self.scratch.alloc_i32();
                        let de = self.scratch.alloc_i32();
                        let dlen = self.scratch.alloc_i32();
                        wasm!(self.func, { local_get(new_map); i32_load(0); local_set(dlen); });
                        self.emit_dict_entries_base(new_map, cap);
                        wasm!(self.func, { local_set(de); i32_const(0); local_set(di);
                            block_empty; loop_empty;
                                local_get(di); local_get(dlen); i32_ge_u; br_if(1);
                        });
                        if kh {
                            wasm!(self.func, {
                                local_get(de); local_get(di); i32_const(entry as i32); i32_mul; i32_add;
                                i32_load(0); call(self.emitter.rt.rc_inc); drop;
                            });
                        }
                        if vh {
                            wasm!(self.func, {
                                local_get(de); local_get(di); i32_const(entry as i32); i32_mul; i32_add;
                                i32_const(ks as i32); i32_add;
                                i32_load(0); call(self.emitter.rt.rc_inc); drop;
                            });
                        }
                        wasm!(self.func, {
                                local_get(di); i32_const(1); i32_add; local_set(di);
                                br(0);
                            end; end;
                        });
                        self.scratch.free_i32(dlen);
                        self.scratch.free_i32(de);
                        self.scratch.free_i32(di);
                    }
                }
                // valaddr = dense entries base(new_map, cap) + found_idx*entry + ks
                self.emit_dict_entries_base(new_map, cap);
                wasm!(self.func, {
                      local_get(found_idx); i32_const(entry as i32); i32_mul; i32_add;
                      i32_const(ks as i32); i32_add; local_set(valaddr);
                      local_get(valaddr);              // dst val addr
                      local_get(closure); i32_load(4); // env
                      local_get(valaddr);              // load old val from same addr
                });
                self.emit_load_at(&val_ty, 0);
                // Capture the OLD value before the overwrite so its dup'd
                // reference (from the table walk above) can be released after
                // the closure result replaces it.
                let old_val = if crate::pass_perceus::is_heap_type(&val_ty) {
                    let ov = self.scratch.alloc_i32();
                    wasm!(self.func, { local_tee(ov); });
                    Some(ov)
                } else { None };
                wasm!(self.func, { local_get(closure); i32_load(0); }); // table_idx
                self.emit_closure_call(&val_ty, &val_ty);
                self.emit_store_at(&val_ty, 0);
                if let Some(ov) = old_val {
                    wasm!(self.func, { local_get(ov); call(self.emitter.rt.rc_dec); });
                    self.scratch.free_i32(ov);
                }
                wasm!(self.func, {
                      local_get(new_map);
                    end;
                });
                self.scratch.free_i32(valaddr);
                self.scratch.free_i32(total_size);
                self.scratch.free_i32(new_map);
                self.scratch.free_i32(found_idx);
                self.scratch.free_i32(eb);
                self.scratch.free_i32(cap);
                self.scratch.free_i32(len);
                self.scratch.free_i32(i);
                self.scratch.free_i32(sk32);
                self.scratch.free_i64(sk64);
                self.scratch.free_i32(closure);
                self.scratch.free_i32(map_ptr);
            }
            _ => return false,
        }
        true
    }

    // (filter method is handled via map.set insertion — see calls_map.rs)
}

// ── Compact-ordered-dict iteration helpers ──

/// Scratch locals for iterating a compact-ordered-dict map in insertion order.
pub(super) struct MapIter {
    pub map: u32,       // map pointer
    pub cap: u32,       // capacity (slot count) — needed only to derive the entries base
    pub len: u32,       // entry count (dense walk bound)
    pub eb: u32,        // dense entries base (map + header + cap + cap*INDEX_SLOT_SIZE)
    pub i: u32,         // dense entry index (0..len)
    pub entry_size: u32, // key_size + val_size
}

impl FuncCompiler<'_> {
    /// Allocate scratch locals and emit COD setup: len = map[0], cap = map[CAP],
    /// eb = dense entries base, i = 0. Iteration walks the dense entries[0..len].
    pub(super) fn map_iter_begin(&mut self, map_expr: &IrExpr, entry_size: u32) -> MapIter {
        use super::engine::layout::{SWISS_MAP, map as lm};
        let map_cap_off = self.emitter.layout_reg.fixed_offset(SWISS_MAP, lm::CAP);
        let map = self.scratch.alloc_i32();
        let cap = self.scratch.alloc_i32();
        let len = self.scratch.alloc_i32();
        let eb = self.scratch.alloc_i32();
        let i = self.scratch.alloc_i32();
        self.emit_expr(map_expr);
        wasm!(self.func, {
            local_set(map);
            local_get(map); i32_load(0); local_set(len);
            local_get(map); i32_load(map_cap_off); local_set(cap);
        });
        self.emit_dict_entries_base(map, cap);
        wasm!(self.func, { local_set(eb); i32_const(0); local_set(i); });
        MapIter { map, cap, len, eb, i, entry_size }
    }

    /// Emit loop header: block/loop, break if i >= len. Dense entries are all
    /// occupied (no tag scan), so the current entry is always valid.
    pub(super) fn map_iter_loop_head(&mut self, it: &MapIter) {
        wasm!(self.func, {
            block_empty; loop_empty;
              local_get(it.i); local_get(it.len); i32_ge_u; br_if(1);
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
        self.scratch.free_i32(it.len);
        self.scratch.free_i32(it.cap);
        self.scratch.free_i32(it.map);
    }
}
