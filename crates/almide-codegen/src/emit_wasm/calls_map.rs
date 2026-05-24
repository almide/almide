//! Map stdlib call dispatch for WASM codegen — Swiss Table layout.
//!
//! Layout: [len:i32 @ 0][cap:i32 @ 4][tags @ 8 (cap bytes)][entries @ 8+cap]
//! Tags: 1 byte each, h2 = hash upper 7 bits (0x01..0x7F). 0x00 = empty.
//! Entries: key + val per slot (no tag), contiguous array.
//! Total size = 8 + cap + cap * entry_size.
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
                // Empty map: header only, cap=0
                let scratch = self.scratch.alloc_i32();
                wasm!(self.func, {
                    i32_const(list_layout::MAP_HEADER_SIZE);
                    call(self.emitter.rt.alloc); local_set(scratch);
                    local_get(scratch); i32_const(0); i32_store(0);
                    local_get(scratch); i32_const(0); i32_store(list_layout::MAP_CAP_OFFSET as u32);
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
                let (ks, vs) = self.map_kv_sizes(&args[0].ty);
                let key_ty = self.map_key_ty(&args[0].ty);
                let es = ks + vs; // entry_size (no tag)
                let m = self.scratch.alloc_i32();
                let sk32 = self.scratch.alloc_i32();
                let sk64 = self.scratch.alloc_i64();
                let cap = self.scratch.alloc_i32();
                let idx = self.scratch.alloc_i32();
                let eb = self.scratch.alloc_i32(); // entries_base
                let h2 = self.scratch.alloc_i32();
                let tg = self.scratch.alloc_i32(); // tag
                let result = self.scratch.alloc_i32();
                let vc = self.scratch.alloc_i32(); // val_copy

                self.emit_expr(&args[0]);
                wasm!(self.func, { local_set(m); });
                self.emit_expr(&args[1]);
                self.emit_search_key_store(&key_ty, sk32, sk64);

                wasm!(self.func, {
                    i32_const(0); local_set(result);
                    local_get(m); i32_load(list_layout::MAP_CAP_OFFSET as u32); local_set(cap);
                    local_get(cap); i32_eqz;
                    if_empty; else_;
                    // entries_base = m + 8 + cap
                    local_get(m); i32_const(list_layout::MAP_TAGS_OFFSET); i32_add;
                    local_get(cap); i32_add; local_set(eb);
                });
                // Hash → h2 + idx
                self.emit_search_key_load(&key_ty, sk32, sk64);
                self.emit_hash_key(&key_ty);
                self.emit_h1_h2(cap, idx, h2);
                // Probe
                wasm!(self.func, {
                    block_empty; loop_empty;
                      // tag = mem[m + 8 + idx] (1 byte)
                      local_get(m); i32_const(list_layout::MAP_TAGS_OFFSET); i32_add;
                      local_get(idx); i32_add; i32_load8_u(0); local_set(tg);
                      local_get(tg); i32_eqz; br_if(1); // empty
                      local_get(tg); local_get(h2); i32_eq;
                      if_empty;
                        // entry = eb + idx * es
                        local_get(eb); local_get(idx); i32_const(es as i32); i32_mul; i32_add;
                });
                self.emit_key_load(&key_ty, 0);
                self.emit_search_key_load(&key_ty, sk32, sk64);
                self.emit_key_eq(&key_ty);
                wasm!(self.func, {
                        if_empty;
                          i32_const(vs as i32); call(self.emitter.rt.alloc); local_set(vc);
                          local_get(vc);
                          local_get(eb); local_get(idx); i32_const(es as i32); i32_mul; i32_add;
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
                self.scratch.free_i32(eb);
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
                let eb = self.scratch.alloc_i32();
                let h2 = self.scratch.alloc_i32();
                let tg = self.scratch.alloc_i32();
                let found = self.scratch.alloc_i32();

                self.emit_expr(&args[0]);
                wasm!(self.func, { local_set(m); });
                self.emit_expr(&args[1]);
                self.emit_search_key_store(&key_ty, sk32, sk64);
                wasm!(self.func, {
                    i32_const(0); local_set(found);
                    local_get(m); i32_load(list_layout::MAP_CAP_OFFSET as u32); local_set(cap);
                    local_get(cap); i32_eqz;
                    if_empty; else_;
                    local_get(m); i32_const(list_layout::MAP_TAGS_OFFSET); i32_add;
                    local_get(cap); i32_add; local_set(eb);
                });
                self.emit_search_key_load(&key_ty, sk32, sk64);
                self.emit_hash_key(&key_ty);
                self.emit_h1_h2(cap, idx, h2);
                wasm!(self.func, {
                    block_empty; loop_empty;
                      local_get(m); i32_const(list_layout::MAP_TAGS_OFFSET); i32_add;
                      local_get(idx); i32_add; i32_load8_u(0); local_set(tg);
                      local_get(tg); i32_eqz; br_if(1);
                      local_get(tg); local_get(h2); i32_eq;
                      if_empty;
                        local_get(eb); local_get(idx); i32_const(es as i32); i32_mul; i32_add;
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
                    local_get(eb); local_get(idx); i32_const(es as i32); i32_mul; i32_add;
                    i32_const(ks as i32); i32_add;
                });
                self.emit_load_at(&val_ty, 0);
                wasm!(self.func, { end; });

                self.scratch.free_i32(found);
                self.scratch.free_i32(tg);
                self.scratch.free_i32(h2);
                self.scratch.free_i32(eb);
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
                let eb = self.scratch.alloc_i32();
                let h2 = self.scratch.alloc_i32();
                let tg = self.scratch.alloc_i32();
                let result = self.scratch.alloc_i32();

                self.emit_expr(&args[0]);
                wasm!(self.func, { local_set(m); });
                self.emit_expr(&args[1]);
                self.emit_search_key_store(&key_ty, sk32, sk64);
                wasm!(self.func, {
                    i32_const(0); local_set(result);
                    local_get(m); i32_load(list_layout::MAP_CAP_OFFSET as u32); local_set(cap);
                    local_get(cap); i32_eqz;
                    if_empty; else_;
                    local_get(m); i32_const(list_layout::MAP_TAGS_OFFSET); i32_add;
                    local_get(cap); i32_add; local_set(eb);
                });
                self.emit_search_key_load(&key_ty, sk32, sk64);
                self.emit_hash_key(&key_ty);
                self.emit_h1_h2(cap, idx, h2);
                wasm!(self.func, {
                    block_empty; loop_empty;
                      local_get(m); i32_const(list_layout::MAP_TAGS_OFFSET); i32_add;
                      local_get(idx); i32_add; i32_load8_u(0); local_set(tg);
                      local_get(tg); i32_eqz; br_if(1);
                      local_get(tg); local_get(h2); i32_eq;
                      if_empty;
                        local_get(eb); local_get(idx); i32_const(es as i32); i32_mul; i32_add;
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
                self.scratch.free_i32(eb);
                self.scratch.free_i32(idx);
                self.scratch.free_i32(cap);
                self.scratch.free_i64(sk64);
                self.scratch.free_i32(sk32);
                self.scratch.free_i32(m);
            }
            "set" => {
                // Immutable: copy table, then insert into copy
                let (ks, vs) = self.map_kv_sizes(&args[0].ty);
                let key_ty = self.map_key_ty(&args[0].ty);
                let val_ty = self.map_val_ty(&args[0].ty);
                let es = ks + vs;
                let vt = values::ty_to_valtype(&val_ty).unwrap_or(ValType::I32);
                let m = self.scratch.alloc_i32();
                let sk32 = self.scratch.alloc_i32();
                let sk64 = self.scratch.alloc_i64();
                let sv = self.scratch.alloc(vt);
                let cap = self.scratch.alloc_i32();
                let nm = self.scratch.alloc_i32(); // new_map
                let idx = self.scratch.alloc_i32();
                let eb = self.scratch.alloc_i32();
                let h2 = self.scratch.alloc_i32();
                let tg = self.scratch.alloc_i32();

                self.emit_expr(&args[0]);
                wasm!(self.func, { local_set(m); });
                self.emit_expr(&args[1]);
                self.emit_search_key_store(&key_ty, sk32, sk64);
                self.emit_expr(&args[2]);
                wasm!(self.func, { local_set(sv); });

                // Copy table (or create initial if cap==0)
                wasm!(self.func, {
                    local_get(m); i32_load(list_layout::MAP_CAP_OFFSET as u32); local_set(cap);
                    local_get(cap); i32_eqz;
                    if_empty; i32_const(list_layout::MAP_INITIAL_CAP); local_set(cap); end;
                });
                self.emit_alloc_table(nm, cap, es as i32);
                wasm!(self.func, {
                    // Copy old data if exists
                    local_get(m); i32_load(list_layout::MAP_CAP_OFFSET as u32);
                    if_empty;
                      local_get(nm); i32_const(list_layout::MAP_TAGS_OFFSET); i32_add;
                      local_get(m); i32_const(list_layout::MAP_TAGS_OFFSET); i32_add;
                      local_get(m); i32_load(list_layout::MAP_CAP_OFFSET as u32);
                      local_tee(idx); // reuse idx as old_cap temp
                      local_get(idx); i32_const(es as i32); i32_mul; i32_add; // tags + entries
                      memory_copy;
                      local_get(nm); local_get(m); i32_load(0); i32_store(0); // copy len
                    end;
                    local_get(nm); i32_const(list_layout::MAP_TAGS_OFFSET); i32_add;
                    local_get(cap); i32_add; local_set(eb);
                });
                // Insert into copy
                self.emit_search_key_load(&key_ty, sk32, sk64);
                self.emit_hash_key(&key_ty);
                self.emit_h1_h2(cap, idx, h2);
                wasm!(self.func, {
                    block_empty; loop_empty;
                      local_get(nm); i32_const(list_layout::MAP_TAGS_OFFSET); i32_add;
                      local_get(idx); i32_add; i32_load8_u(0); local_set(tg);
                      local_get(tg); i32_eqz; br_if(1);
                      local_get(tg); local_get(h2); i32_eq;
                      if_empty;
                        local_get(eb); local_get(idx); i32_const(es as i32); i32_mul; i32_add;
                });
                self.emit_key_load(&key_ty, 0);
                self.emit_search_key_load(&key_ty, sk32, sk64);
                self.emit_key_eq(&key_ty);
                wasm!(self.func, {
                        br_if(2);
                      end;
                      local_get(idx); i32_const(1); i32_add;
                      local_get(cap); i32_const(1); i32_sub; i32_and;
                      local_set(idx); br(0);
                    end; end;
                    // Store val
                    local_get(eb); local_get(idx); i32_const(es as i32); i32_mul; i32_add;
                    i32_const(ks as i32); i32_add;
                    local_get(sv);
                });
                match vt {
                    ValType::I64 => { wasm!(self.func, { i64_store(0); }); }
                    ValType::F64 => { wasm!(self.func, { f64_store(0); }); }
                    _ => { wasm!(self.func, { i32_store(0); }); }
                }
                // If new entry
                wasm!(self.func, {
                    local_get(tg); i32_eqz;
                    if_empty;
                      // Store tag (h2) as 1 byte
                      local_get(nm); i32_const(list_layout::MAP_TAGS_OFFSET); i32_add;
                      local_get(idx); i32_add; local_get(h2); i32_store8(0);
                      // Store key
                      local_get(eb); local_get(idx); i32_const(es as i32); i32_mul; i32_add;
                });
                self.emit_search_key_load(&key_ty, sk32, sk64);
                self.emit_key_store(&key_ty, 0);
                wasm!(self.func, {
                      local_get(nm);
                      local_get(nm); i32_load(0); i32_const(1); i32_add;
                      i32_store(0);
                    end;
                    local_get(nm);
                });
                self.scratch.free_i32(tg);
                self.scratch.free_i32(h2);
                self.scratch.free_i32(eb);
                self.scratch.free_i32(idx);
                self.scratch.free_i32(nm);
                self.scratch.free_i32(cap);
                self.scratch.free(sv, vt);
                self.scratch.free_i64(sk64);
                self.scratch.free_i32(sk32);
                self.scratch.free_i32(m);
            }
            "insert" => {
                let (ks, vs) = self.map_kv_sizes(&args[0].ty);
                let key_ty = self.map_key_ty(&args[0].ty);
                let val_ty = self.map_val_ty(&args[0].ty);
                let es = ks + vs;
                let vt = values::ty_to_valtype(&val_ty).unwrap_or(ValType::I32);
                let m = self.scratch.alloc_i32();
                let sk32 = self.scratch.alloc_i32();
                let sk64 = self.scratch.alloc_i64();
                let sv = self.scratch.alloc(vt);
                let cap = self.scratch.alloc_i32();
                let idx = self.scratch.alloc_i32();
                let eb = self.scratch.alloc_i32();
                let h2 = self.scratch.alloc_i32();
                let tg = self.scratch.alloc_i32();

                self.emit_expr(&args[0]);
                wasm!(self.func, { local_set(m); });
                self.emit_expr(&args[1]);
                self.emit_search_key_store(&key_ty, sk32, sk64);
                self.emit_expr(&args[2]);
                wasm!(self.func, { local_set(sv); });

                wasm!(self.func, {
                    local_get(m); i32_load(list_layout::MAP_CAP_OFFSET as u32); local_set(cap);
                    local_get(cap); i32_eqz;
                    if_empty;
                      i32_const(list_layout::MAP_INITIAL_CAP); local_set(cap);
                });
                self.emit_alloc_table(m, cap, es as i32);
                wasm!(self.func, { end; });
                // entries_base
                wasm!(self.func, {
                    local_get(m); i32_const(list_layout::MAP_TAGS_OFFSET); i32_add;
                    local_get(cap); i32_add; local_set(eb);
                });

                // Hash → h2 + idx
                self.emit_search_key_load(&key_ty, sk32, sk64);
                self.emit_hash_key(&key_ty);
                self.emit_h1_h2(cap, idx, h2);
                // Probe
                wasm!(self.func, {
                    block_empty; loop_empty;
                      local_get(m); i32_const(list_layout::MAP_TAGS_OFFSET); i32_add;
                      local_get(idx); i32_add; i32_load8_u(0); local_set(tg);
                      local_get(tg); i32_eqz; br_if(1);
                      local_get(tg); local_get(h2); i32_eq;
                      if_empty;
                        local_get(eb); local_get(idx); i32_const(es as i32); i32_mul; i32_add;
                });
                self.emit_key_load(&key_ty, 0);
                self.emit_search_key_load(&key_ty, sk32, sk64);
                self.emit_key_eq(&key_ty);
                wasm!(self.func, {
                        br_if(2);
                      end;
                      local_get(idx); i32_const(1); i32_add;
                      local_get(cap); i32_const(1); i32_sub; i32_and;
                      local_set(idx); br(0);
                    end; end;
                    // Store value
                    local_get(eb); local_get(idx); i32_const(es as i32); i32_mul; i32_add;
                    i32_const(ks as i32); i32_add;
                    local_get(sv);
                });
                match vt {
                    ValType::I64 => { wasm!(self.func, { i64_store(0); }); }
                    ValType::F64 => { wasm!(self.func, { f64_store(0); }); }
                    _ => { wasm!(self.func, { i32_store(0); }); }
                }
                // If new entry
                wasm!(self.func, {
                    local_get(tg); i32_eqz;
                    if_empty;
                      local_get(m); i32_const(list_layout::MAP_TAGS_OFFSET); i32_add;
                      local_get(idx); i32_add; local_get(h2); i32_store8(0);
                      local_get(eb); local_get(idx); i32_const(es as i32); i32_mul; i32_add;
                });
                self.emit_search_key_load(&key_ty, sk32, sk64);
                self.emit_key_store(&key_ty, 0);
                wasm!(self.func, {
                      local_get(m);
                      local_get(m); i32_load(0); i32_const(1); i32_add;
                      i32_store(0);
                      // Resize check
                      local_get(m); i32_load(0); i32_const(4); i32_mul;
                      local_get(cap); i32_const(3); i32_mul;
                      i32_gt_u;
                      if_empty;
                });
                self.emit_map_resize(&key_ty, ks, vs, m, cap);
                wasm!(self.func, {
                      end;
                    end;
                });
                // Write back
                if let almide_ir::IrExprKind::Var { id } = &args[0].kind {
                    if let Some(&local_idx) = self.var_map.get(&id.0) {
                        wasm!(self.func, { local_get(m); local_set(local_idx); });
                    }
                }
                self.scratch.free_i32(tg);
                self.scratch.free_i32(h2);
                self.scratch.free_i32(eb);
                self.scratch.free_i32(idx);
                self.scratch.free_i32(cap);
                self.scratch.free(sv, vt);
                self.scratch.free_i64(sk64);
                self.scratch.free_i32(sk32);
                self.scratch.free_i32(m);
            }
            "remove" | "delete" => {
                let is_delete = method == "delete";
                let (ks, vs) = self.map_kv_sizes(&args[0].ty);
                let key_ty = self.map_key_ty(&args[0].ty);
                let es = ks + vs;
                let m = self.scratch.alloc_i32();
                let sk32 = self.scratch.alloc_i32();
                let sk64 = self.scratch.alloc_i64();
                let cap = self.scratch.alloc_i32();
                let nm = self.scratch.alloc_i32();
                let i = self.scratch.alloc_i32();
                let eb_old = self.scratch.alloc_i32();
                let eb_new = self.scratch.alloc_i32();
                let h2r = self.scratch.alloc_i32();
                let nidx = self.scratch.alloc_i32();

                self.emit_expr(&args[0]);
                wasm!(self.func, { local_set(m); });
                self.emit_expr(&args[1]);
                self.emit_search_key_store(&key_ty, sk32, sk64);
                wasm!(self.func, {
                    local_get(m); i32_load(list_layout::MAP_CAP_OFFSET as u32); local_set(cap);
                });
                self.emit_alloc_table(nm, cap, es as i32);
                wasm!(self.func, {
                    local_get(m); i32_const(list_layout::MAP_TAGS_OFFSET); i32_add;
                    local_get(cap); i32_add; local_set(eb_old);
                    local_get(nm); i32_const(list_layout::MAP_TAGS_OFFSET); i32_add;
                    local_get(cap); i32_add; local_set(eb_new);
                    i32_const(0); local_set(i);
                    block_empty; loop_empty;
                      local_get(i); local_get(cap); i32_ge_u; br_if(1);
                      local_get(m); i32_const(list_layout::MAP_TAGS_OFFSET); i32_add;
                      local_get(i); i32_add; i32_load8_u(0);
                      if_empty; // occupied
                        // Compare key
                        local_get(eb_old); local_get(i); i32_const(es as i32); i32_mul; i32_add;
                });
                self.emit_key_load(&key_ty, 0);
                self.emit_search_key_load(&key_ty, sk32, sk64);
                self.emit_key_eq(&key_ty);
                wasm!(self.func, {
                        i32_eqz;
                        if_empty;
                          // Rehash into new_map
                          local_get(eb_old); local_get(i); i32_const(es as i32); i32_mul; i32_add;
                });
                self.emit_key_load(&key_ty, 0);
                self.emit_hash_key(&key_ty);
                self.emit_h1_h2(cap, nidx, h2r);
                wasm!(self.func, {
                          block_empty; loop_empty;
                            local_get(nm); i32_const(list_layout::MAP_TAGS_OFFSET); i32_add;
                            local_get(nidx); i32_add; i32_load8_u(0);
                            i32_eqz; br_if(1);
                            local_get(nidx); i32_const(1); i32_add;
                            local_get(cap); i32_const(1); i32_sub; i32_and;
                            local_set(nidx); br(0);
                          end; end;
                          // Copy tag
                          local_get(nm); i32_const(list_layout::MAP_TAGS_OFFSET); i32_add;
                          local_get(nidx); i32_add;
                          local_get(m); i32_const(list_layout::MAP_TAGS_OFFSET); i32_add;
                          local_get(i); i32_add; i32_load8_u(0);
                          i32_store8(0);
                          // Copy entry
                          local_get(eb_new); local_get(nidx); i32_const(es as i32); i32_mul; i32_add;
                          local_get(eb_old); local_get(i); i32_const(es as i32); i32_mul; i32_add;
                          i32_const(es as i32); memory_copy;
                          // len++
                          local_get(nm);
                          local_get(nm); i32_load(0); i32_const(1); i32_add;
                          i32_store(0);
                        end;
                      end;
                      local_get(i); i32_const(1); i32_add; local_set(i); br(0);
                    end; end;
                });
                if is_delete {
                    if let almide_ir::IrExprKind::Var { id } = &args[0].kind {
                        if let Some(&local_idx) = self.var_map.get(&id.0) {
                            wasm!(self.func, { local_get(nm); local_set(local_idx); });
                        }
                    }
                } else {
                    wasm!(self.func, { local_get(nm); });
                }
                self.scratch.free_i32(nidx);
                self.scratch.free_i32(h2r);
                self.scratch.free_i32(eb_new);
                self.scratch.free_i32(eb_old);
                self.scratch.free_i32(i);
                self.scratch.free_i32(nm);
                self.scratch.free_i32(cap);
                self.scratch.free_i64(sk64);
                self.scratch.free_i32(sk32);
                self.scratch.free_i32(m);
            }
            "keys" | "values" | "entries" => {
                let (ks, vs) = self.map_kv_sizes(&args[0].ty);
                let es = ks + vs;
                let m = self.scratch.alloc_i32();
                let len = self.scratch.alloc_i32();
                let cap = self.scratch.alloc_i32();
                let result = self.scratch.alloc_i32();
                let i = self.scratch.alloc_i32();
                let j = self.scratch.alloc_i32();
                let eb = self.scratch.alloc_i32();
                let elem_size = match method {
                    "keys" => ks, "values" => vs, _ => 4, // entries: ptr
                };
                let tp = self.scratch.alloc_i32(); // tuple_ptr for entries

                self.emit_expr(&args[0]);
                wasm!(self.func, {
                    local_set(m);
                    local_get(m); i32_load(0); local_set(len);
                    local_get(m); i32_load(list_layout::MAP_CAP_OFFSET as u32); local_set(cap);
                    local_get(m); i32_const(list_layout::MAP_TAGS_OFFSET); i32_add;
                    local_get(cap); i32_add; local_set(eb);
                    i32_const(list_layout::HEADER_SIZE); local_get(len); i32_const(elem_size as i32); i32_mul; i32_add;
                    call(self.emitter.rt.alloc); local_set(result);
                    local_get(result); local_get(len); i32_store(0);
                    i32_const(0); local_set(i);
                    i32_const(0); local_set(j);
                    block_empty; loop_empty;
                      local_get(i); local_get(cap); i32_ge_u; br_if(1);
                      local_get(m); i32_const(list_layout::MAP_TAGS_OFFSET); i32_add;
                      local_get(i); i32_add; i32_load8_u(0);
                      if_empty;
                });
                match method {
                    "keys" => {
                        wasm!(self.func, {
                            local_get(result); i32_const(list_layout::DATA_OFFSET); i32_add;
                            local_get(j); i32_const(ks as i32); i32_mul; i32_add;
                            local_get(eb); local_get(i); i32_const(es as i32); i32_mul; i32_add;
                        });
                        self.emit_elem_copy_sized(ks);
                    }
                    "values" => {
                        wasm!(self.func, {
                            local_get(result); i32_const(list_layout::DATA_OFFSET); i32_add;
                            local_get(j); i32_const(vs as i32); i32_mul; i32_add;
                            local_get(eb); local_get(i); i32_const(es as i32); i32_mul; i32_add;
                            i32_const(ks as i32); i32_add;
                        });
                        self.emit_elem_copy_sized(vs);
                    }
                    _ => {
                        // entries: alloc tuple, copy key+val
                        wasm!(self.func, {
                            i32_const(es as i32); call(self.emitter.rt.alloc); local_set(tp);
                            local_get(tp);
                            local_get(eb); local_get(i); i32_const(es as i32); i32_mul; i32_add;
                            i32_const(es as i32); memory_copy;
                            local_get(result); i32_const(list_layout::DATA_OFFSET); i32_add;
                            local_get(j); i32_const(4); i32_mul; i32_add;
                            local_get(tp); i32_store(0);
                        });
                    }
                }
                wasm!(self.func, {
                        local_get(j); i32_const(1); i32_add; local_set(j);
                      end;
                      local_get(i); i32_const(1); i32_add; local_set(i); br(0);
                    end; end;
                    local_get(result);
                });
                self.scratch.free_i32(tp);
                self.scratch.free_i32(eb);
                self.scratch.free_i32(j);
                self.scratch.free_i32(i);
                self.scratch.free_i32(result);
                self.scratch.free_i32(cap);
                self.scratch.free_i32(len);
                self.scratch.free_i32(m);
            }
            "merge" => {
                // Copy a, then insert each b entry
                let (ks, vs) = self.map_kv_sizes(&args[0].ty);
                let key_ty = self.map_key_ty(&args[0].ty);
                let es = ks + vs;
                let ma = self.scratch.alloc_i32();
                let mb = self.scratch.alloc_i32();
                let cap_a = self.scratch.alloc_i32();
                let cap_b = self.scratch.alloc_i32();
                let result = self.scratch.alloc_i32();
                let i = self.scratch.alloc_i32();
                let eb_b = self.scratch.alloc_i32();
                let eb_r = self.scratch.alloc_i32();
                let r_cap = self.scratch.alloc_i32();
                let ri = self.scratch.alloc_i32();
                let h2r = self.scratch.alloc_i32();
                let tg = self.scratch.alloc_i32();

                self.emit_expr(&args[0]);
                wasm!(self.func, { local_set(ma); });
                self.emit_expr(&args[1]);
                wasm!(self.func, {
                    local_set(mb);
                    local_get(ma); i32_load(list_layout::MAP_CAP_OFFSET as u32); local_set(cap_a);
                    local_get(mb); i32_load(list_layout::MAP_CAP_OFFSET as u32); local_set(cap_b);
                    // Copy a's table
                    i32_const(list_layout::MAP_HEADER_SIZE);
                    local_get(cap_a); local_get(cap_a); i32_const(es as i32); i32_mul; i32_add; i32_add;
                    local_tee(ri); // temp total
                    call(self.emitter.rt.alloc); local_set(result);
                    local_get(result); local_get(ma); local_get(ri); memory_copy;
                    local_get(cap_a); local_set(r_cap);
                    local_get(mb); i32_const(list_layout::MAP_TAGS_OFFSET); i32_add;
                    local_get(cap_b); i32_add; local_set(eb_b);
                    local_get(result); i32_const(list_layout::MAP_TAGS_OFFSET); i32_add;
                    local_get(r_cap); i32_add; local_set(eb_r);
                    // Insert each b entry
                    i32_const(0); local_set(i);
                    block_empty; loop_empty;
                      local_get(i); local_get(cap_b); i32_ge_u; br_if(1);
                      local_get(mb); i32_const(list_layout::MAP_TAGS_OFFSET); i32_add;
                      local_get(i); i32_add; i32_load8_u(0);
                      if_empty;
                        local_get(eb_b); local_get(i); i32_const(es as i32); i32_mul; i32_add;
                });
                self.emit_key_load(&key_ty, 0);
                self.emit_hash_key(&key_ty);
                self.emit_h1_h2(r_cap, ri, h2r);
                wasm!(self.func, {
                        block_empty; loop_empty;
                          local_get(result); i32_const(list_layout::MAP_TAGS_OFFSET); i32_add;
                          local_get(ri); i32_add; i32_load8_u(0); local_set(tg);
                          local_get(tg); i32_eqz; br_if(1);
                          local_get(tg); local_get(h2r); i32_eq;
                          if_empty;
                            local_get(eb_r); local_get(ri); i32_const(es as i32); i32_mul; i32_add;
                });
                self.emit_key_load(&key_ty, 0);
                wasm!(self.func, {
                            local_get(eb_b); local_get(i); i32_const(es as i32); i32_mul; i32_add;
                });
                self.emit_key_load(&key_ty, 0);
                self.emit_key_eq(&key_ty);
                wasm!(self.func, {
                            br_if(2);
                          end;
                          local_get(ri); i32_const(1); i32_add;
                          local_get(r_cap); i32_const(1); i32_sub; i32_and;
                          local_set(ri); br(0);
                        end; end;
                        // Copy tag + entry
                        local_get(result); i32_const(list_layout::MAP_TAGS_OFFSET); i32_add;
                        local_get(ri); i32_add;
                        local_get(mb); i32_const(list_layout::MAP_TAGS_OFFSET); i32_add;
                        local_get(i); i32_add; i32_load8_u(0);
                        i32_store8(0);
                        local_get(eb_r); local_get(ri); i32_const(es as i32); i32_mul; i32_add;
                        local_get(eb_b); local_get(i); i32_const(es as i32); i32_mul; i32_add;
                        i32_const(es as i32); memory_copy;
                        local_get(tg); i32_eqz;
                        if_empty;
                          local_get(result);
                          local_get(result); i32_load(0); i32_const(1); i32_add;
                          i32_store(0);
                        end;
                      end;
                      local_get(i); i32_const(1); i32_add; local_set(i); br(0);
                    end; end;
                    local_get(result);
                });
                self.scratch.free_i32(tg);
                self.scratch.free_i32(h2r);
                self.scratch.free_i32(ri);
                self.scratch.free_i32(r_cap);
                self.scratch.free_i32(eb_r);
                self.scratch.free_i32(eb_b);
                self.scratch.free_i32(i);
                self.scratch.free_i32(result);
                self.scratch.free_i32(cap_b);
                self.scratch.free_i32(cap_a);
                self.scratch.free_i32(mb);
                self.scratch.free_i32(ma);
            }
            "from_list" => {
                let pair_ty = self.resolve_list_elem(&args[0], None);
                let (ks, vs, key_ty) = if let Ty::Tuple(elems) = &pair_ty {
                    (elems.first().map(|t| values::byte_size(t)).unwrap_or(4),
                     elems.get(1).map(|t| values::byte_size(t)).unwrap_or(4),
                     elems.first().cloned().unwrap_or(Ty::String))
                } else { (4u32, 4u32, Ty::String) };
                let es = ks + vs;
                let pairs = self.scratch.alloc_i32();
                let plen = self.scratch.alloc_i32();
                let result = self.scratch.alloc_i32();
                let cap = self.scratch.alloc_i32();
                let i = self.scratch.alloc_i32();
                let tp = self.scratch.alloc_i32();
                let eb = self.scratch.alloc_i32();
                let idx = self.scratch.alloc_i32();
                let h2 = self.scratch.alloc_i32();
                let tg = self.scratch.alloc_i32();

                self.emit_expr(&args[0]);
                wasm!(self.func, {
                    local_set(pairs);
                    local_get(pairs); i32_load(0); local_set(plen);
                    i32_const(list_layout::MAP_INITIAL_CAP); local_set(cap);
                    block_empty; loop_empty;
                      local_get(cap); local_get(plen); i32_const(1); i32_shl; i32_ge_u; br_if(1);
                      local_get(cap); i32_const(1); i32_shl; local_set(cap); br(0);
                    end; end;
                });
                self.emit_alloc_table(result, cap, es as i32);
                wasm!(self.func, {
                    local_get(result); i32_const(list_layout::MAP_TAGS_OFFSET); i32_add;
                    local_get(cap); i32_add; local_set(eb);
                    i32_const(0); local_set(i);
                    block_empty; loop_empty;
                      local_get(i); local_get(plen); i32_ge_u; br_if(1);
                      local_get(pairs); i32_const(list_layout::DATA_OFFSET); i32_add;
                      local_get(i); i32_const(4); i32_mul; i32_add;
                      i32_load(0); local_set(tp);
                      local_get(tp);
                });
                self.emit_key_load(&key_ty, 0);
                self.emit_hash_key(&key_ty);
                self.emit_h1_h2(cap, idx, h2);
                wasm!(self.func, {
                      // Probe for slot
                      block_empty; loop_empty;
                        local_get(result); i32_const(list_layout::MAP_TAGS_OFFSET); i32_add;
                        local_get(idx); i32_add; i32_load8_u(0); local_set(tg);
                        local_get(tg); i32_eqz; br_if(1);
                        local_get(tg); local_get(h2); i32_eq;
                        if_empty;
                          local_get(eb); local_get(idx); i32_const(es as i32); i32_mul; i32_add;
                });
                self.emit_key_load(&key_ty, 0);
                wasm!(self.func, { local_get(tp); });
                self.emit_key_load(&key_ty, 0);
                self.emit_key_eq(&key_ty);
                wasm!(self.func, {
                          br_if(2);
                        end;
                        local_get(idx); i32_const(1); i32_add;
                        local_get(cap); i32_const(1); i32_sub; i32_and;
                        local_set(idx); br(0);
                      end; end;
                      // Store tag + entry
                      local_get(result); i32_const(list_layout::MAP_TAGS_OFFSET); i32_add;
                      local_get(idx); i32_add; local_get(h2); i32_store8(0);
                      local_get(eb); local_get(idx); i32_const(es as i32); i32_mul; i32_add;
                      local_get(tp); i32_const(es as i32); memory_copy;
                      local_get(tg); i32_eqz;
                      if_empty;
                        local_get(result);
                        local_get(result); i32_load(0); i32_const(1); i32_add;
                        i32_store(0);
                      end;
                      local_get(i); i32_const(1); i32_add; local_set(i); br(0);
                    end; end;
                    local_get(result);
                });
                self.scratch.free_i32(tg);
                self.scratch.free_i32(h2);
                self.scratch.free_i32(idx);
                self.scratch.free_i32(eb);
                self.scratch.free_i32(tp);
                self.scratch.free_i32(i);
                self.scratch.free_i32(cap);
                self.scratch.free_i32(result);
                self.scratch.free_i32(plen);
                self.scratch.free_i32(pairs);
            }
            "clear" => {
                let scratch = self.scratch.alloc_i32();
                wasm!(self.func, {
                    i32_const(list_layout::MAP_HEADER_SIZE);
                    call(self.emitter.rt.alloc); local_set(scratch);
                    local_get(scratch); i32_const(0); i32_store(0);
                    local_get(scratch); i32_const(0); i32_store(list_layout::MAP_CAP_OFFSET as u32);
                });
                if let almide_ir::IrExprKind::Var { id } = &args[0].kind {
                    if let Some(&local_idx) = self.var_map.get(&id.0) {
                        wasm!(self.func, { local_get(scratch); local_set(local_idx); });
                    } else {
                        wasm!(self.func, { drop; });
                    }
                } else {
                    wasm!(self.func, { drop; });
                }
                self.scratch.free_i32(scratch);
            }
            _ => return self.emit_map_closure_call(method, args),
        }
        true
    }

    // ── Swiss Table helpers ──

    /// Allocate a new table: [len=0][cap][tags:cap bytes][entries:cap*es bytes]
    fn emit_alloc_table(&mut self, out: u32, cap: u32, es: i32) {
        wasm!(self.func, {
            i32_const(list_layout::MAP_HEADER_SIZE);
            local_get(cap); // tags size
            i32_add;
            local_get(cap); i32_const(es); i32_mul; // entries size
            i32_add;
            call(self.emitter.rt.alloc); local_set(out);
            local_get(out); i32_const(0); i32_store(0); // len = 0
            local_get(out); local_get(cap); i32_store(list_layout::MAP_CAP_OFFSET as u32);
            // Zero-fill tag array so reused memory has clean empty slots.
            // Required for region-based memory reuse (heap_restore).
            local_get(out); i32_const(list_layout::MAP_TAGS_OFFSET); i32_add;
            i32_const(0);
            local_get(cap);
            memory_fill(0);
        });
    }

    /// Split hash on stack into h1 (bucket index) → idx_local and h2 (tag) → h2_local.
    fn emit_h1_h2(&mut self, cap: u32, idx_local: u32, h2_local: u32) {
        let ht = self.scratch.alloc_i32();
        wasm!(self.func, {
            local_tee(ht);
            local_get(cap); i32_const(1); i32_sub; i32_and;
            local_set(idx_local);
            local_get(ht);
            i32_const(25); i32_shr_u; i32_const(0x7F); i32_and;
            local_tee(h2_local);
            i32_eqz;
            if_empty; i32_const(1); local_set(h2_local); end; // avoid 0 (empty)
        });
        self.scratch.free_i32(ht);
    }

    pub(super) fn emit_hash_key(&mut self, key_ty: &Ty) {
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
                      local_get(s); i32_const(list_layout::STRING_DATA_OFFSET); i32_add;
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
            _ => {} // Bool/pointer: identity hash, already i32
        }
    }

    /// Resize: double cap, rehash all entries into new table.
    fn emit_map_resize(&mut self, key_ty: &Ty, ks: u32, vs: u32, m: u32, cap: u32) {
        let es = (ks + vs) as i32;
        let nc = self.scratch.alloc_i32(); // new_cap
        let nm = self.scratch.alloc_i32(); // new_map
        let ri = self.scratch.alloc_i32();
        let eb_old = self.scratch.alloc_i32();
        let eb_new = self.scratch.alloc_i32();
        let ni = self.scratch.alloc_i32();
        let h2r = self.scratch.alloc_i32();

        wasm!(self.func, {
            local_get(cap); i32_const(2); i32_shl; local_set(nc); // 4x growth
        });
        self.emit_alloc_table(nm, nc, es);
        wasm!(self.func, {
            local_get(nm); local_get(m); i32_load(0); i32_store(0); // copy len
            local_get(m); i32_const(list_layout::MAP_TAGS_OFFSET); i32_add;
            local_get(cap); i32_add; local_set(eb_old);
            local_get(nm); i32_const(list_layout::MAP_TAGS_OFFSET); i32_add;
            local_get(nc); i32_add; local_set(eb_new);
            i32_const(0); local_set(ri);
            block_empty; loop_empty;
              local_get(ri); local_get(cap); i32_ge_u; br_if(1);
              local_get(m); i32_const(list_layout::MAP_TAGS_OFFSET); i32_add;
              local_get(ri); i32_add; i32_load8_u(0);
              if_empty;
                // Hash key from old entry
                local_get(eb_old); local_get(ri); i32_const(es); i32_mul; i32_add;
        });
        self.emit_key_load(key_ty, 0);
        self.emit_hash_key(key_ty);
        self.emit_h1_h2(nc, ni, h2r);
        wasm!(self.func, {
                // Probe new table for empty
                block_empty; loop_empty;
                  local_get(nm); i32_const(list_layout::MAP_TAGS_OFFSET); i32_add;
                  local_get(ni); i32_add; i32_load8_u(0);
                  i32_eqz; br_if(1);
                  local_get(ni); i32_const(1); i32_add;
                  local_get(nc); i32_const(1); i32_sub; i32_and;
                  local_set(ni); br(0);
                end; end;
                // Copy tag (recomputed h2)
                local_get(nm); i32_const(list_layout::MAP_TAGS_OFFSET); i32_add;
                local_get(ni); i32_add; local_get(h2r); i32_store8(0);
                // Copy entry
                local_get(eb_new); local_get(ni); i32_const(es); i32_mul; i32_add;
                local_get(eb_old); local_get(ri); i32_const(es); i32_mul; i32_add;
                i32_const(es); memory_copy;
              end;
              local_get(ri); i32_const(1); i32_add; local_set(ri); br(0);
            end; end;
            local_get(nm); local_set(m);
        });
        self.scratch.free_i32(h2r);
        self.scratch.free_i32(ni);
        self.scratch.free_i32(eb_new);
        self.scratch.free_i32(eb_old);
        self.scratch.free_i32(ri);
        self.scratch.free_i32(nm);
        self.scratch.free_i32(nc);
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
            _ => { wasm!(self.func, { i32_eq; }); }
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
}
