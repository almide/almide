// List stdlib call dispatch for WASM codegen — group 3 (split from calls_list.rs).
//
// Sub-match of `emit_list_call` over the SAME `method` scrutinee, restricted to
// a disjoint set of method strings. Arm bodies are verbatim; each arm yields
// `true` (handled). Chained from `emit_list_call` before the group-1 match.
//
// NOTE: This file is `include!`d into calls_list.rs; it intentionally has NO
// `use` items of its own (they would collide with the parent's imports). The
// referenced types/macros resolve from calls_list.rs's module scope.

impl FuncCompiler<'_> {
    /// Group-3 sub-dispatch of `emit_list_call`. Returns true if `method` was
    /// handled here.
    pub(super) fn emit_list_call_g3(&mut self, method: &str, args: &[IrExpr]) -> bool {
        use super::engine::layout::{LIST, list as ll};
        let list_data_off = self.emitter.layout_reg.fixed_offset(LIST, ll::DATA) as i32;
        let list_hdr = self.emitter.layout_reg.header_size(LIST) as i32;
        match method {
            "zip" => {
                // zip(xs, ys) → List[(A, B)]
                // Each tuple is heap-allocated: [a_value, b_value]
                let a_ty = self.resolve_list_elem(&args[0], None);
                let b_ty = self.resolve_list_elem(&args[1], None);
                let a_size = values::byte_size(&a_ty);
                let b_size = values::byte_size(&b_ty);
                let tuple_size = a_size + b_size;
                let xs = self.scratch.alloc_i32();
                let ys = self.scratch.alloc_i32();
                let len = self.scratch.alloc_i32();
                let dst = self.scratch.alloc_i32();
                let i = self.scratch.alloc_i32();
                let tup = self.scratch.alloc_i32();
                self.emit_expr(&args[0]);
                wasm!(self.func, { local_set(xs); });
                self.emit_expr(&args[1]);
                wasm!(self.func, {
                    local_set(ys);
                    // len = min(xs.len, ys.len)
                    local_get(xs); i32_load(0);
                    local_get(ys); i32_load(0);
                    i32_lt_u;
                    if_i32;
                      local_get(xs); i32_load(0);
                    else_;
                      local_get(ys); i32_load(0);
                    end;
                    local_set(len);
                    // Alloc result: list of ptrs to tuples
                    i32_const(list_hdr); local_get(len); i32_const(4); i32_mul; i32_add;
                    call(self.emitter.rt.alloc); local_set(dst);
                    local_get(dst); local_get(len); i32_store(0);
                    i32_const(0); local_set(i);
                    block_empty; loop_empty;
                      local_get(i); local_get(len); i32_ge_u; br_if(1);
                      // Alloc tuple
                      i32_const(tuple_size as i32); call(self.emitter.rt.alloc); local_set(tup);
                      // Copy a: tuple[0] = xs[i]
                      local_get(tup);
                      local_get(xs); i32_const(list_data_off); i32_add;
                      local_get(i); i32_const(a_size as i32); i32_mul; i32_add;
                });
                self.emit_elem_copy_owned(&a_ty);
                // Copy b: tuple[a_size] = ys[i]
                wasm!(self.func, {
                      local_get(tup); i32_const(a_size as i32); i32_add;
                      local_get(ys); i32_const(list_data_off); i32_add;
                      local_get(i); i32_const(b_size as i32); i32_mul; i32_add;
                });
                self.emit_elem_copy_owned(&b_ty);
                wasm!(self.func, {
                      // result[i] = tuple_ptr
                      local_get(dst); i32_const(list_data_off); i32_add;
                      local_get(i); i32_const(4); i32_mul; i32_add;
                      local_get(tup); i32_store(0);
                      local_get(i); i32_const(1); i32_add; local_set(i);
                      br(0);
                    end; end;
                    local_get(dst);
                });
                self.scratch.free_i32(tup);
                self.scratch.free_i32(i);
                self.scratch.free_i32(dst);
                self.scratch.free_i32(len);
                self.scratch.free_i32(ys);
                self.scratch.free_i32(xs);
            }
            "set" => {
                // set(xs, i, val) → List[A]: copy + replace element at i
                let elem_ty = self.resolve_list_elem(&args[0], None);
                let es = values::byte_size(&elem_ty) as i32;
                let val_vt = values::ty_to_valtype(&elem_ty).unwrap_or(ValType::I32);
                let xs = self.scratch.alloc_i32();
                let idx = self.scratch.alloc_i32();
                let in_bounds = self.scratch.alloc_i32();
                let len = self.scratch.alloc_i32();
                let dst = self.scratch.alloc_i32();
                let i = self.scratch.alloc_i32();
                let val = self.scratch.alloc(val_vt);
                self.emit_expr(&args[0]);
                wasm!(self.func, { local_set(xs); local_get(xs); i32_load(0); local_set(len); });
                self.emit_expr(&args[1]); // i: i64
                // idx = min_u(i, len); in_bounds = (i_u < len) on the full i64.
                // Native `list.set` no-ops when i is OOB (`get_mut` → None), so
                // a huge/negative i64 must take the no-op path, not wrap to a
                // small in-range index and overwrite the wrong slot (C-054).
                self.emit_checked_index_i32(len, in_bounds);
                wasm!(self.func, { local_set(idx); });
                // Evaluate `val` EAGERLY (native passes it by value before the
                // call), then store conditionally — so a side-effecting value
                // expr runs whether or not the index is in bounds. Stored-field
                // contract: an alias value gets its own reference.
                self.emit_stored_field(&args[2]);
                wasm!(self.func, {
                    local_set(val);
                    // Alloc copy
                    i32_const(list_hdr); local_get(len); i32_const(es); i32_mul; i32_add;
                    call(self.emitter.rt.alloc); local_set(dst);
                    local_get(dst); local_get(len); i32_store(0);
                    // Copy all elements
                    i32_const(0); local_set(i);
                    block_empty; loop_empty;
                      local_get(i); local_get(len); i32_ge_u; br_if(1);
                      local_get(dst); i32_const(list_data_off); i32_add;
                      local_get(i); i32_const(es); i32_mul; i32_add;
                      local_get(xs); i32_const(list_data_off); i32_add;
                      local_get(i); i32_const(es); i32_mul; i32_add;
                });
                self.emit_elem_copy_owned(&elem_ty);
                wasm!(self.func, {
                      local_get(i); i32_const(1); i32_add; local_set(i);
                      br(0);
                    end; end;
                    // Overwrite dst[idx] with val — only when in bounds.
                    local_get(in_bounds);
                    if_empty;
                });
                // Release the owned copy of the element being replaced (it
                // received +1 in the copy loop above and loses this slot).
                if crate::pass_perceus::is_heap_type(&elem_ty) {
                    wasm!(self.func, {
                      local_get(dst); i32_const(list_data_off); i32_add;
                      local_get(idx); i32_const(es); i32_mul; i32_add;
                      i32_load(0); call(self.emitter.rt.rc_dec);
                    });
                }
                wasm!(self.func, {
                      local_get(dst); i32_const(list_data_off); i32_add;
                      local_get(idx); i32_const(es); i32_mul; i32_add;
                      local_get(val);
                });
                self.emit_elem_store(&elem_ty);
                wasm!(self.func, { end; local_get(dst); });
                self.scratch.free(val, val_vt);
                self.scratch.free_i32(i);
                self.scratch.free_i32(dst);
                self.scratch.free_i32(len);
                self.scratch.free_i32(in_bounds);
                self.scratch.free_i32(idx);
                self.scratch.free_i32(xs);
            }
            "insert" => {
                // insert(xs, i, val) → List[A]: copy with element inserted at i
                let elem_ty = self.resolve_list_elem(&args[0], None);
                let es = values::byte_size(&elem_ty) as i32;
                let xs = self.scratch.alloc_i32();
                let idx = self.scratch.alloc_i32();
                let old_len = self.scratch.alloc_i32();
                let dst = self.scratch.alloc_i32();
                let i = self.scratch.alloc_i32();
                self.emit_expr(&args[0]);
                wasm!(self.func, { local_set(xs); local_get(xs); i32_load(0); local_set(old_len); });
                self.emit_expr(&args[1]);
                // idx = min_u(i, old_len) on the full i64 (C-054): out-of-range
                // or negative indices append at the end, mirroring native's
                // `idx = (i as usize).min(len)`. The old i32_wrap_i64 + i32 min
                // let an index >= 2^32 wrap to a small in-range insert point.
                self.emit_clamp_index_to_len_i32(old_len);
                wasm!(self.func, {
                    local_set(idx);
                    // new_len = old_len + 1
                    i32_const(list_hdr); local_get(old_len); i32_const(1); i32_add; i32_const(es); i32_mul; i32_add;
                    call(self.emitter.rt.alloc); local_set(dst);
                    local_get(dst); local_get(old_len); i32_const(1); i32_add; i32_store(0);
                    // Copy [0..idx)
                    i32_const(0); local_set(i);
                    block_empty; loop_empty;
                      local_get(i); local_get(idx); i32_ge_u; br_if(1);
                      local_get(dst); i32_const(list_data_off); i32_add;
                      local_get(i); i32_const(es); i32_mul; i32_add;
                      local_get(xs); i32_const(list_data_off); i32_add;
                      local_get(i); i32_const(es); i32_mul; i32_add;
                });
                self.emit_elem_copy_owned(&elem_ty);
                wasm!(self.func, {
                      local_get(i); i32_const(1); i32_add; local_set(i);
                      br(0);
                    end; end;
                });
                // Insert val at idx (stored-field: alias values dup)
                wasm!(self.func, {
                    local_get(dst); i32_const(list_data_off); i32_add;
                    local_get(idx); i32_const(es); i32_mul; i32_add;
                });
                self.emit_stored_field(&args[2]);
                self.emit_elem_store(&elem_ty);
                // Copy [idx..old_len)
                wasm!(self.func, {
                    local_get(idx); local_set(i);
                    block_empty; loop_empty;
                      local_get(i); local_get(old_len); i32_ge_u; br_if(1);
                      local_get(dst); i32_const(list_data_off); i32_add;
                      local_get(i); i32_const(1); i32_add; i32_const(es); i32_mul; i32_add;
                      local_get(xs); i32_const(list_data_off); i32_add;
                      local_get(i); i32_const(es); i32_mul; i32_add;
                });
                self.emit_elem_copy_owned(&elem_ty);
                wasm!(self.func, {
                      local_get(i); i32_const(1); i32_add; local_set(i);
                      br(0);
                    end; end;
                    local_get(dst);
                });
                self.scratch.free_i32(i);
                self.scratch.free_i32(dst);
                self.scratch.free_i32(old_len);
                self.scratch.free_i32(idx);
                self.scratch.free_i32(xs);
            }
            "remove_at" => {
                // remove_at(xs, i) → List[A]
                let elem_ty = self.resolve_list_elem(&args[0], None);
                let es = values::byte_size(&elem_ty) as i32;
                let xs = self.scratch.alloc_i32();
                let idx = self.scratch.alloc_i32();
                let in_bounds = self.scratch.alloc_i32();
                let old_len = self.scratch.alloc_i32();
                let dst = self.scratch.alloc_i32();
                let i = self.scratch.alloc_i32();
                self.emit_expr(&args[0]);
                wasm!(self.func, { local_set(xs); local_get(xs); i32_load(0); local_set(old_len); });
                self.emit_expr(&args[1]);
                // idx = min_u(i, old_len); in_bounds = (i_u < old_len) on the
                // full i64. Native is a no-op when i >= len; an index >= 2^32
                // must take the no-op path, not wrap to a small in-range index
                // and remove the wrong element (C-054).
                self.emit_checked_index_i32(old_len, in_bounds);
                wasm!(self.func, {
                    local_set(idx);
                    local_get(in_bounds);
                    if_i32;
                    // new_len = old_len - 1
                    i32_const(list_hdr); local_get(old_len); i32_const(1); i32_sub; i32_const(es); i32_mul; i32_add;
                    call(self.emitter.rt.alloc); local_set(dst);
                    local_get(dst); local_get(old_len); i32_const(1); i32_sub; i32_store(0);
                    // Copy [0..idx)
                    i32_const(0); local_set(i);
                    block_empty; loop_empty;
                      local_get(i); local_get(idx); i32_ge_u; br_if(1);
                      local_get(dst); i32_const(list_data_off); i32_add;
                      local_get(i); i32_const(es); i32_mul; i32_add;
                      local_get(xs); i32_const(list_data_off); i32_add;
                      local_get(i); i32_const(es); i32_mul; i32_add;
                });
                self.emit_elem_copy_owned(&elem_ty);
                wasm!(self.func, {
                      local_get(i); i32_const(1); i32_add; local_set(i);
                      br(0);
                    end; end;
                });
                // Copy [idx+1..old_len)
                wasm!(self.func, {
                    local_get(idx); i32_const(1); i32_add; local_set(i);
                    block_empty; loop_empty;
                      local_get(i); local_get(old_len); i32_ge_u; br_if(1);
                      local_get(dst); i32_const(list_data_off); i32_add;
                      local_get(i); i32_const(1); i32_sub; i32_const(es); i32_mul; i32_add;
                      local_get(xs); i32_const(list_data_off); i32_add;
                      local_get(i); i32_const(es); i32_mul; i32_add;
                });
                self.emit_elem_copy_owned(&elem_ty);
                wasm!(self.func, {
                      local_get(i); i32_const(1); i32_add; local_set(i);
                      br(0);
                    end; end;
                    local_get(dst);
                    else_;
                    // i >= len → return the list unchanged. SHARE: the result
                    // is a SECOND reference to xs (native returns a clone) —
                    // without the inc the result temp's Dec freed xs's live
                    // backing (list_count_index_truncation silent corruption).
                    local_get(xs); call(self.emitter.rt.rc_inc);
                    end;
                });
                self.scratch.free_i32(i);
                self.scratch.free_i32(dst);
                self.scratch.free_i32(old_len);
                self.scratch.free_i32(in_bounds);
                self.scratch.free_i32(idx);
                self.scratch.free_i32(xs);
            }
            "unique" => {
                self.emit_list_unique(args);
            }
            "enumerate" => {
                self.emit_list_enumerate(args);
            }
            "get" => {
                // list.get(list, index) → Option[T]
                let elem_ty = self.resolve_list_elem(&args[0], None);
                let elem_size = values::byte_size(&elem_ty);

                let list = self.scratch.alloc_i32();
                let idx = self.scratch.alloc_i32();
                let in_bounds = self.scratch.alloc_i32();
                let len = self.scratch.alloc_i32();
                let result = self.scratch.alloc_i32();
                self.emit_expr(&args[0]);
                wasm!(self.func, { local_set(list); local_get(list); i32_load(0); local_set(len); });
                self.emit_expr(&args[1]);
                // Index is normally Int (i64); a tuple-index residual may arrive
                // already i32. For the i64 case the bounds check runs on the FULL
                // i64 (C-054) so a negative or >= 2^32 index returns `none`
                // instead of wrapping to a small in-range slot. The i32 case
                // keeps the simple unsigned compare.
                if matches!(&args[1].ty, Ty::Int) || args[1].ty.is_unresolved() {
                    self.emit_checked_index_i32(len, in_bounds);
                    wasm!(self.func, { local_set(idx); });
                } else {
                    wasm!(self.func, {
                        local_set(idx);
                        local_get(idx); local_get(len); i32_lt_u; local_set(in_bounds);
                    });
                }
                wasm!(self.func, {
                    // bounds: !in_bounds → none(0)
                    local_get(in_bounds);
                    i32_eqz;
                    if_i32;
                    i32_const(0); // none
                    else_;
                    // alloc
                    i32_const(elem_size as i32);
                    call(self.emitter.rt.alloc);
                    local_set(result);
                    // dst=result, src=list+4+idx*elem_size
                    local_get(result);
                    local_get(list);
                    i32_const(list_data_off);
                    i32_add;
                    local_get(idx);
                    i32_const(elem_size as i32);
                    i32_mul;
                    i32_add;
                });
                self.emit_load_at(&elem_ty, 0); // load elem
                // SHARE: the some-box holds a second reference to the element
                // (the bind-level alias-Inc covers the LOCAL the caller binds;
                // this covers the BOX, whose typed dec releases it).
                if crate::pass_perceus::is_heap_type(&elem_ty) {
                    wasm!(self.func, { call(self.emitter.rt.rc_inc); });
                }
                self.emit_store_at(&elem_ty, 0); // store at dst
                wasm!(self.func, {
                    local_get(result); // return ptr
                    end;
                });
                self.scratch.free_i32(result);
                self.scratch.free_i32(len);
                self.scratch.free_i32(in_bounds);
                self.scratch.free_i32(idx);
                self.scratch.free_i32(list);
            }
            "contains" => {
                // list.contains(list, elem) -> Bool (i32)
                let elem_ty = self.resolve_list_elem(&args[0], None);
                let elem_size = values::byte_size(&elem_ty);
                let target_vt = values::ty_to_valtype(&elem_ty).unwrap_or(ValType::I32);
                let list_ptr = self.scratch.alloc_i32();
                let idx = self.scratch.alloc_i32();
                let result = self.scratch.alloc_i32();
                // Hold the search target in a valtype-matched register (i64 Int,
                // f64 Float, i32 pointer for String/compound). The element load and
                // compare below use `emit_load_at`/`emit_eq_typed(elem_ty)` so both
                // sides agree on width and use STRUCTURAL (deep) equality — matching
                // native `xs.contains(&x)`, not pointer identity.
                let target = self.scratch.alloc(target_vt);
                self.emit_expr(&args[0]);
                wasm!(self.func, { local_set(list_ptr); });
                self.emit_expr(&args[1]);
                wasm!(self.func, {
                    local_set(target);
                    i32_const(0); local_set(idx);
                    i32_const(0); local_set(result); // result = false
                    block_empty; loop_empty;
                      local_get(idx); local_get(list_ptr); i32_load(0); i32_ge_u; br_if(1);
                      local_get(list_ptr); i32_const(list_data_off); i32_add;
                      local_get(idx); i32_const(elem_size as i32); i32_mul; i32_add;
                });
                self.emit_load_at(&elem_ty, 0);
                wasm!(self.func, { local_get(target); });
                self.emit_eq_typed(&elem_ty);
                wasm!(self.func, {
                      if_empty;
                        i32_const(1); local_set(result); br(2);
                      end;
                      local_get(idx); i32_const(1); i32_add; local_set(idx);
                      br(0);
                    end; end;
                    local_get(result);
                });
                self.scratch.free(target, target_vt);
                self.scratch.free_i32(result);
                self.scratch.free_i32(idx);
                self.scratch.free_i32(list_ptr);
            }
            "with_capacity" => {
                // list.with_capacity(cap: Int) -> List[A]
                // Layout: [len:i32][cap:i32][data...]. Preallocate data space.
                //
                // Capacity is only a PERF HINT and the list starts empty
                // (len=0), so the eagerly-reserved data is clamped to a
                // memory-safe ceiling. Two failure modes this guards:
                //   1. `cap*elem_size` overflowing the i32 heap-size argument
                //      (i32::MAX wrapped mod 2^32 to ~0, so `alloc` reserved
                //      nothing while the stored cap claimed billions of slots —
                //      the next alloc overlapped this header → garbage `len`,
                //      and `push` wrote OOB).
                //   2. a multi-GB pre-reservation exhausting wasm linear memory
                //      (`memory.grow` fails → `unreachable` trap), where native
                //      returns a working empty list.
                // Clamping the PRE-RESERVATION is observably identical to native
                // (the list is empty either way); `push` grows past the clamped
                // cap normally. The STORED cap equals the backing buffer, so
                // push's `len < cap` fast path never outruns it.
                let elem_ty = self.resolve_list_elem(&args[0], None);
                let elem_size = values::byte_size(&elem_ty) as i64;
                // Ceiling on eagerly-reserved DATA bytes: large enough to honor
                // realistic hints, small enough to never exhaust wasm memory.
                // SHARED with native — runtime/rs/src/list.rs clamps its eager
                // Vec reservation to the same named ceiling (C-034), so a huge
                // hint aborts on NEITHER target.
                const MAX_WITH_CAPACITY_PREALLOC_BYTES: i64 = 64 * 1024 * 1024; // 64 MiB
                let max_cap = (MAX_WITH_CAPACITY_PREALLOC_BYTES / elem_size) as i32;
                let new_ptr = self.scratch.alloc_i32();
                let cap_local = self.scratch.alloc_i32();
                let cap64 = self.scratch.alloc_i64();
                // Evaluate the requested capacity once (it is an i64 Int).
                self.emit_expr(&args[0]);
                wasm!(self.func, { local_set(cap64); });
                // cap_local = clamp(req, 0, max_cap), done on the i64 value so
                // the lossy wrap to i32 happens only on an already-safe number.
                wasm!(self.func, {
                    local_get(cap64); i64_const(0); i64_lt_s;
                    if_i32;
                      i32_const(0);                          // negative → empty
                    else_;
                      local_get(cap64);
                      i32_const(max_cap as i32); i64_extend_i32_s; i64_gt_s;
                      if_i32;
                        i32_const(max_cap as i32);           // saturate
                      else_;
                        local_get(cap64); i32_wrap_i64;      // fits
                      end;
                    end;
                    local_set(cap_local);
                });
                // alloc: hdr + cap * elem_size  (cap is clamped → no overflow)
                wasm!(self.func, {
                    i32_const(list_hdr);
                    local_get(cap_local); i32_const(elem_size as i32); i32_mul;
                    i32_add;
                    call(self.emitter.rt.alloc);
                    local_set(new_ptr);
                    // len = 0
                    local_get(new_ptr); i32_const(0); i32_store(0);
                    // cap = clamped capacity (matches the backing buffer)
                    local_get(new_ptr); local_get(cap_local); i32_store(4);
                    local_get(new_ptr);
                });
                self.scratch.free_i64(cap64);
                self.scratch.free_i32(cap_local);
                self.scratch.free_i32(new_ptr);
            }
            "push" => {
                // push(xs, v) → Unit. O(1) amortized via capacity tracking.
                // Layout: [len:i32][cap:i32][data...]
                // If len < cap: write in place. Else: realloc with 2x growth.
                let elem_ty = self.resolve_list_elem(&args[0], None);
                let elem_size = values::byte_size(&elem_ty);
                let vt = values::ty_to_valtype(&elem_ty).unwrap_or(ValType::I32);
                let ptr = self.scratch.alloc_i32();
                let len_local = self.scratch.alloc_i32();
                let cap_local = self.scratch.alloc_i32();
                let new_ptr = self.scratch.alloc_i32();
                let val_scratch = self.scratch.alloc(vt);

                // Evaluate value first (before reading xs, in case of side
                // effects); stored-field contract — an alias value pushed
                // into the list needs its own reference.
                self.emit_stored_field(&args[1]);
                wasm!(self.func, { local_set(val_scratch); });

                // Read xs pointer, len, cap
                self.emit_expr(&args[0]);
                wasm!(self.func, { local_set(ptr); });
                wasm!(self.func, {
                    local_get(ptr); i32_load(0); local_set(len_local);
                    local_get(ptr); i32_load(4); local_set(cap_local);
                });

                // if len < cap: write in place (fast path)
                wasm!(self.func, {
                    local_get(len_local);
                    local_get(cap_local);
                    i32_lt_u;
                    if_empty;
                });
                // Fast path: write element at ptr + 8 + len * elem_size
                wasm!(self.func, {
                    local_get(ptr); i32_const(list_data_off); i32_add;
                    local_get(len_local); i32_const(elem_size as i32); i32_mul;
                    i32_add;
                    local_get(val_scratch);
                });
                match vt {
                    ValType::I64 => { wasm!(self.func, { i64_store(0); }); }
                    ValType::F64 => { wasm!(self.func, { f64_store(0); }); }
                    _ => { wasm!(self.func, { i32_store(0); }); }
                }
                // Increment len
                wasm!(self.func, {
                    local_get(ptr);
                    local_get(len_local); i32_const(1); i32_add;
                    i32_store(0);
                    else_;
                });

                // Slow path: new_cap = max(cap * 2, len + 1, 8)
                // cap=0 is common (lists created without explicit cap), so
                // we MUST ensure new_cap >= len+1 to avoid buffer overflow.
                wasm!(self.func, {
                    // cap_local = max(cap * 2, 8)
                    local_get(cap_local); i32_const(1); i32_shl;
                    i32_const(8);
                    i32_gt_u;
                    if_i32;
                      local_get(cap_local); i32_const(1); i32_shl;
                    else_;
                      i32_const(8);
                    end;
                    local_set(cap_local);
                    // cap_local = max(cap_local, len + 1)
                    local_get(cap_local);
                    local_get(len_local); i32_const(1); i32_add;
                    i32_lt_u;
                    if_empty;
                      local_get(len_local); i32_const(1); i32_add;
                      local_set(cap_local);
                    end;
                });

                // Ensure new_cap > len (edge case: cap=0, len=0 → new_cap=8 > 0 ✓)
                // Allocate: 8 + new_cap * elem_size
                wasm!(self.func, {
                    i32_const(list_hdr);
                    local_get(cap_local); i32_const(elem_size as i32); i32_mul;
                    i32_add;
                    call(self.emitter.rt.alloc);
                    local_set(new_ptr);
                });

                // Write new len = old_len + 1
                wasm!(self.func, {
                    local_get(new_ptr);
                    local_get(len_local); i32_const(1); i32_add;
                    i32_store(0);
                });
                // Write new cap
                wasm!(self.func, {
                    local_get(new_ptr);
                    local_get(cap_local);
                    i32_store(4);
                });

                // Copy old data: memory.copy(new_ptr+8, ptr+8, len * elem_size)
                wasm!(self.func, {
                    local_get(new_ptr); i32_const(list_data_off); i32_add;
                    local_get(ptr); i32_const(list_data_off); i32_add;
                    local_get(len_local); i32_const(elem_size as i32); i32_mul;
                    memory_copy;
                });

                // Write new element at new_ptr + 8 + len * elem_size
                wasm!(self.func, {
                    local_get(new_ptr); i32_const(list_data_off); i32_add;
                    local_get(len_local); i32_const(elem_size as i32); i32_mul;
                    i32_add;
                    local_get(val_scratch);
                });
                match vt {
                    ValType::I64 => { wasm!(self.func, { i64_store(0); }); }
                    ValType::F64 => { wasm!(self.func, { f64_store(0); }); }
                    _ => { wasm!(self.func, { i32_store(0); }); }
                }

                // Write back the (possibly reallocated) list ptr — into the cell for a
                // mutable capture, else into the local/global. See emit_mutator_writeback.
                self.emit_mutator_writeback(&args[0], new_ptr);

                wasm!(self.func, { end; }); // end if/else

                self.scratch.free(val_scratch, vt);
                self.scratch.free_i32(new_ptr);
                self.scratch.free_i32(cap_local);
                self.scratch.free_i32(len_local);
                self.scratch.free_i32(ptr);
            }
            "pop" => {
                // pop(xs) → Option[A]. Removes last element, mutates xs.
                // Option layout: 0 = none, non-zero ptr = some (payload at ptr)
                let elem_ty = self.resolve_list_elem(&args[0], None);
                let elem_size = values::byte_size(&elem_ty);
                let list_ptr = self.scratch.alloc_i32();
                let len = self.scratch.alloc_i32();
                let result = self.scratch.alloc_i32();

                self.emit_expr(&args[0]);
                wasm!(self.func, { local_set(list_ptr); });
                wasm!(self.func, { local_get(list_ptr); i32_load(0); local_set(len); });

                // if len == 0 → none (0)
                // else → decrement len, copy last element into alloc'd payload
                wasm!(self.func, {
                    local_get(len); i32_eqz;
                    if_i32;
                      i32_const(0); // none
                    else_;
                });

                // Decrement len in place
                wasm!(self.func, {
                    local_get(list_ptr);
                    local_get(len); i32_const(1); i32_sub;
                    i32_store(0);
                });

                // Allocate payload (no tag — Option uses ptr==0 for none)
                wasm!(self.func, {
                    i32_const(elem_size as i32);
                    call(self.emitter.rt.alloc);
                    local_set(result);
                    // Copy last element: dst=result, src=list+4+(len-1)*elem_size
                    local_get(result);
                    local_get(list_ptr); i32_const(list_data_off); i32_add;
                    local_get(len); i32_const(1); i32_sub;
                    i32_const(elem_size as i32); i32_mul; i32_add;
                });
                self.emit_load_at(&elem_ty, 0);
                self.emit_store_at(&elem_ty, 0);
                wasm!(self.func, {
                      local_get(result);
                    end;
                });

                self.scratch.free_i32(result);
                self.scratch.free_i32(len);
                self.scratch.free_i32(list_ptr);
            }
            "clear" => {
                // clear(xs) → Unit. Sets len to 0 in place.
                self.emit_expr(&args[0]);
                wasm!(self.func, { i32_const(0); i32_store(0); });
            }
            _ => return false,
        }
        true
    }
}
