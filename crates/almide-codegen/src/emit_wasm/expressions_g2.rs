// IrExpr → WASM: emit_expr group 2 (runtime-call / map / fan / range arms).
//
// Part of `expressions.rs` — `include!`d at the END of the parent, so it shares
// the parent module's imports. Sub-match over the SAME `&expr.kind` scrutinee
// restricted to a DISJOINT set of arms; returns `true` when it handled the expr.
// Arm bodies are moved VERBATIM (only a trailing `true` and the `_ => false`
// fallthrough are added). Chained from `emit_expr` before its group-1 match.

impl FuncCompiler<'_> {
    pub(super) fn emit_expr_g2(&mut self, expr: &IrExpr) -> bool {
        match &expr.kind {
            // ── Resolved runtime call (@intrinsic) ──
            IrExprKind::RuntimeCall { symbol, args } => {
                // In-place stdlib mutators (`list.push`, `map.insert`, `string.push`,
                // bytes builders, …) mutate args[0] through its shared pointer. If
                // args[0] is a copy-aliased COW target, clone it into its own local
                // first so the sibling binding is not corrupted. Fires BEFORE any
                // dispatch branch reads args[0]. No-op for non-COW vars.
                if is_inplace_mutator(symbol.as_str()) {
                    if let Some(IrExprKind::Var { id }) = args.first().map(|a| &a.kind) {
                        self.cow_if_needed(id.0);
                    }
                }
                // Resolved runtime call from @intrinsic. Preferred path:
                // look up the mangled symbol in `func_map` and emit
                // `call(idx)` after each arg. Fallback: the WASM runtime
                // fn may not be registered yet (migration in progress).
                // Decode the symbol back to (module, func) and route
                // through the legacy `emit_<m>_call` dispatcher so the
                // inline-emitted variant (`int.abs` as i64 ops, etc.)
                // keeps working until the runtime fn lands.
                let sym = symbol.as_str();
                // mem.save / mem.restore: direct runtime calls
                if sym == "almide_rt_mem_save" {
                    wasm!(self.func, { call(self.emitter.rt.heap_save); i64_extend_i32_u; });
                } else if sym == "almide_rt_mem_restore" {
                    self.emit_expr(&args[0]);
                    wasm!(self.func, { i32_wrap_i64; call(self.emitter.rt.heap_restore); });
                } else if let Some(&idx) = self.emitter.func_map.get(sym) {
                    for a in args { self.emit_expr(a); }
                    wasm!(self.func, { call(idx); });
                } else if let Some((module, func)) = self.emitter.intrinsic_symbol_to_fn.get(sym).cloned() {
                    // Preferred: use the Almide (module, fn) that declared
                    // the `@intrinsic` — the symbol may rename the fn
                    // (e.g. `map.map` → `almide_rt_map_map_values`).
                    if !self.dispatch_runtime_fallback(&module, &func, args, &expr.ty) {
                        panic!(
                            "[ICE] emit_wasm: RuntimeCall `{}` declared by `{}.{}` \
                             — no WASM runtime fn and no legacy dispatcher arm. \
                             Register the runtime fn or add a dispatch arm.",
                            sym, module, func
                        );
                    }
                } else if let Some(rest) = sym.strip_prefix("almide_rt_") {
                    // Legacy fallback: decode module/fn from the mangled
                    // symbol name. Used when the runtime symbol matches the
                    // Almide fn name 1:1 and the bundled `@intrinsic` map
                    // hasn't claimed it.
                    if let Some(underscore) = rest.find('_') {
                        let module = &rest[..underscore];
                        let func = &rest[underscore + 1..];
                        if !self.dispatch_runtime_fallback(module, func, args, &expr.ty) {
                            panic!(
                                "[ICE] emit_wasm: RuntimeCall `{}` — no WASM \
                                 runtime fn and no legacy dispatcher fallback. \
                                 Register the runtime fn or add a dispatch arm \
                                 for `{}.{}`.",
                                sym, module, func
                            );
                        }
                    } else {
                        panic!(
                            "[ICE] emit_wasm: RuntimeCall symbol `{}` has no \
                             recoverable (module, func) prefix for fallback dispatch.",
                            sym
                        );
                    }
                } else {
                    panic!(
                        "[ICE] emit_wasm: RuntimeCall symbol `{}` lacks the \
                         `almide_rt_` prefix — cannot look up in func_map or \
                         derive fallback dispatch.",
                        sym
                    );
                }
                true
            }

            // ── Map ──
            IrExprKind::EmptyMap => {
                // Empty hash map: [len=0][cap=0]
                let scratch = self.scratch.alloc_i32();
                wasm!(self.func, {
                    i32_const(self.emitter.layout_reg.header_size(super::engine::layout::SWISS_MAP) as i32);
                    call(self.emitter.rt.alloc);
                    local_set(scratch);
                    local_get(scratch); i32_const(0); i32_store(0); // len = 0
                    local_get(scratch); i32_const(0); i32_store(self.emitter.layout_reg.fixed_offset(super::engine::layout::SWISS_MAP, super::engine::layout::map::CAP)); // cap = 0
                    local_get(scratch);
                });
                self.scratch.free_i32(scratch);
                true
            }
            IrExprKind::MapLiteral { entries } => {
                // Map literal: build hash table from entries.
                // Allocate hash table with capacity = next power of 2 >= n * 2 (min 16).
                let n = entries.len() as u32;
                if n == 0 {
                    // Empty map
                    let scratch = self.scratch.alloc_i32();
                    wasm!(self.func, {
                        i32_const(self.emitter.layout_reg.header_size(super::engine::layout::SWISS_MAP) as i32);
                        call(self.emitter.rt.alloc);
                        local_set(scratch);
                        local_get(scratch); i32_const(0); i32_store(0);
                        local_get(scratch); i32_const(0); i32_store(self.emitter.layout_reg.fixed_offset(super::engine::layout::SWISS_MAP, super::engine::layout::map::CAP));
                        local_get(scratch);
                    });
                    self.scratch.free_i32(scratch);
                } else {
                    // COD construction: alloc a table sized for n, then put each
                    // (key, val) via the shared probe-and-place helper (duplicate
                    // literal keys → last value wins, dense insertion order kept).
                    let ks = if let Some((k, _)) = entries.first() { values::byte_size(&k.ty) } else { 4 };
                    let vs = if let Some((_, v)) = entries.first() { values::byte_size(&v.ty) } else { 4 };
                    let es = ks + vs;
                    let key_ty = if let Some((k, _)) = entries.first() { k.ty.clone() } else { Ty::String };
                    let val_ty = if let Some((_, v)) = entries.first() { v.ty.clone() } else { Ty::Int };
                    let mut cap = super::engine::layout::map::INITIAL_CAP;
                    while cap < n * 2 { cap *= 2; }

                    let map = self.scratch.alloc_i32();
                    let cap_local = self.scratch.alloc_i32();
                    let ib = self.scratch.alloc_i32();
                    let eb = self.scratch.alloc_i32();
                    let tmp = self.scratch.alloc_i32();
                    wasm!(self.func, { i32_const(cap as i32); local_set(cap_local); });
                    self.emit_dict_alloc(map, cap_local, es);
                    self.emit_dict_index_base(map, cap_local);
                    wasm!(self.func, { local_set(ib); });
                    self.emit_dict_entries_base(map, cap_local);
                    wasm!(self.func, { local_set(eb); });

                    for (key, val) in entries {
                        // Materialize the (key, val) into a temp entry buffer.
                        wasm!(self.func, { i32_const(es as i32); call(self.emitter.rt.alloc); local_set(tmp); local_get(tmp); });
                        self.emit_expr(key);
                        self.emit_key_store(&key_ty, 0);
                        wasm!(self.func, { local_get(tmp); i32_const(ks as i32); i32_add; });
                        self.emit_expr(val);
                        self.emit_store_at(&val.ty, 0);
                        self.emit_dict_put_entry(map, cap_local, ib, eb, tmp, es, ks, vs, &key_ty, &val_ty);
                    }
                    wasm!(self.func, { local_get(map); });
                    self.scratch.free_i32(tmp);
                    self.scratch.free_i32(eb);
                    self.scratch.free_i32(ib);
                    self.scratch.free_i32(cap_local);
                    self.scratch.free_i32(map);
                }
                true
            }

            // ── Fan block (sequential fallback — no parallelism in WASM) ──
            IrExprKind::Fan { exprs } => {
                if exprs.len() == 1 {
                    // Single expr: emit with auto-unwrap if Result
                    self.emit_expr(&exprs[0]);
                    if let Ty::Applied(almide_lang::types::constructor::TypeConstructorId::Result, _) = &exprs[0].ty {
                        let scratch = self.scratch.alloc_i32();
                        wasm!(self.func, {
                            local_set(scratch);
                            local_get(scratch); i32_load(0); i32_const(0); i32_ne;
                            if_empty;
                        });
                        // EARLY-RETURN LEAK FIX: free live heap locals before the bare
                        // `return_` (skips the terminal rc_decs) — else they leak on wasm.
                        self.emit_early_return_decs();
                        wasm!(self.func, {
                            local_get(scratch); return_; end;
                            local_get(scratch);
                        });
                        self.emit_load_at(&expr.ty, 4);
                        self.scratch.free_i32(scratch);
                    }
                } else {
                    // Fan with multiple exprs → Tuple of unwrapped results
                    // Each expr returns Result[T, E]. Unwrap each, build tuple of T values.
                    let elem_types: Vec<Ty> = if let Ty::Tuple(tys) = &expr.ty {
                        tys.clone()
                    } else {
                        exprs.iter().map(|e| e.ty.clone()).collect()
                    };
                    let total_size: u32 = elem_types.iter().map(|t| values::byte_size(t)).sum();
                    let tuple_scratch = self.scratch.alloc_i32();
                    wasm!(self.func, {
                        i32_const(total_size as i32);
                        call(self.emitter.rt.alloc);
                        local_set(tuple_scratch);
                    });
                    let mut offset = 0u32;
                    for (i, e) in exprs.iter().enumerate() {
                        let elem_ty = elem_types.get(i).cloned().unwrap_or(Ty::Int);
                        let elem_size = values::byte_size(&elem_ty);
                        // Fan exprs are typically effect fn calls → Result[T, E]
                        // Auto-unwrap: if err, return Result early; if ok, store unwrapped value
                        let is_result = matches!(&e.ty, Ty::Applied(almide_lang::types::constructor::TypeConstructorId::Result, _));
                        if is_result {
                            self.emit_expr(e);
                            let res_scratch = self.scratch.alloc_i32();
                            wasm!(self.func, {
                                local_set(res_scratch);
                                local_get(res_scratch); i32_load(0); i32_const(0); i32_ne;
                                if_empty;
                            });
                            // EARLY-RETURN LEAK FIX: free live heap locals before the bare
                            // `return_` (skips the terminal rc_decs). NOTE: the partially-
                            // built `tuple_scratch` still leaks on this path — a separate,
                            // pre-existing Fan-multi scratch leak (partial init can't be
                            // typed-dec'd); this change does not regress it.
                            self.emit_early_return_decs();
                            wasm!(self.func, {
                                local_get(res_scratch); return_; end;
                                local_get(tuple_scratch);
                                local_get(res_scratch);
                            });
                            self.emit_load_at(&elem_ty, 4);
                            self.emit_store_at(&elem_ty, offset);
                            self.scratch.free_i32(res_scratch);
                        } else {
                            // Non-Result: push tuple_ptr, emit expr, store
                            wasm!(self.func, { local_get(tuple_scratch); });
                            self.emit_expr(e);
                            self.emit_store_at(&elem_ty, offset);
                        }
                        offset += elem_size;
                    }
                    wasm!(self.func, { local_get(tuple_scratch); });
                    self.scratch.free_i32(tuple_scratch);
                }
                true
            }

            // ── Range → materialize as List[Int] ──
            IrExprKind::Range { start, end, inclusive } => {
                let s = self.scratch.alloc_i64();
                let e = self.scratch.alloc_i64();
                let len = self.scratch.alloc_i32();
                let dst = self.scratch.alloc_i32();
                let i = self.scratch.alloc_i32();
                self.emit_expr(start);
                wasm!(self.func, { local_set(s); });
                self.emit_expr(end);
                wasm!(self.func, { local_set(e); });
                // len = max(0, end - start [+ 1 if inclusive])
                wasm!(self.func, {
                    local_get(e); local_get(s); i64_sub;
                });
                if *inclusive {
                    wasm!(self.func, { i64_const(1); i64_add; });
                }
                wasm!(self.func, {
                    i64_const(0); i64_gt_s;
                    if_i32;
                      local_get(e); local_get(s); i64_sub;
                });
                if *inclusive {
                    wasm!(self.func, { i64_const(1); i64_add; });
                }
                wasm!(self.func, {
                      i32_wrap_i64;
                    else_;
                      i32_const(0);
                    end;
                    local_set(len);
                    // alloc: 8 + len * 8 (header: [len:i32][cap:i32])
                    i32_const(8); local_get(len); i32_const(8); i32_mul; i32_add;
                    call(self.emitter.rt.alloc); local_set(dst);
                    local_get(dst); local_get(len); i32_store(0);
                    local_get(dst); local_get(len); i32_store(4); // cap = len
                    // fill elements
                    i32_const(0); local_set(i);
                    block_empty; loop_empty;
                      local_get(i); local_get(len); i32_ge_u; br_if(1);
                      local_get(dst); i32_const(self.emitter.layout_reg.fixed_offset(super::engine::layout::LIST, super::engine::layout::list::DATA) as i32); i32_add;
                      local_get(i); i32_const(8); i32_mul; i32_add;
                      // value = start + i
                      local_get(s); local_get(i); i64_extend_i32_u; i64_add;
                      i64_store(0);
                      local_get(i); i32_const(1); i32_add; local_set(i);
                      br(0);
                    end; end;
                    local_get(dst);
                });
                self.scratch.free_i32(i);
                self.scratch.free_i32(dst);
                self.scratch.free_i32(len);
                self.scratch.free_i64(e);
                self.scratch.free_i64(s);
                true
            }
            _ => false,
        }
    }
}
