// List stdlib closure-based call dispatch for WASM codegen (group 2).
//
// Split out of `calls_list_closure.rs` (Technique B): a disjoint sub-match of
// `emit_list_closure_call` over the SAME `method` scrutinee. Each method string
// matches exactly one group, so the dispatcher's chain order is irrelevant and
// behavior is identical.
//
// Group 2 methods: find, find_index, any, all, each, take_end, drop_end,
// repeat, reduce.
//
// This file is `include!`d at the bottom of `calls_list_closure.rs`, so it
// shares that module's `use` imports (FuncCompiler, values, IrExpr, Ty,
// ValType) — do NOT re-declare them here or they will collide.

impl FuncCompiler<'_> {
    /// Group-2 sub-match of `emit_list_closure_call`. Returns true if handled.
    pub(super) fn emit_list_closure_call_g2(&mut self, method: &str, args: &[IrExpr]) -> bool {
        use super::engine::layout::{LIST, list as ll};
        let list_data_off = self.emitter.layout_reg.fixed_offset(LIST, ll::DATA) as i32;
        let list_hdr = self.emitter.layout_reg.header_size(LIST) as i32;
        match method {
            "find" => {
                // find(xs, pred) → Option[A]: first element where pred(x) is true
                let elem_ty = self.resolve_list_elem(&args[0], None);
                let es = values::byte_size(&elem_ty) as i32;
                let xs = self.scratch.alloc_i32();
                let closure = self.scratch.alloc_i32();
                let i = self.scratch.alloc_i32();
                let tmp = self.scratch.alloc_i32();
                let result = self.scratch.alloc_i32();
                self.emit_expr(&args[0]);
                wasm!(self.func, { local_set(xs); });
                self.emit_expr(&args[1]);
                wasm!(self.func, {
                    local_set(closure);
                    i32_const(0); local_set(i); // i=0
                    i32_const(0); local_set(result); // result (default: none)
                    block_empty; loop_empty;
                      local_get(i); local_get(xs); i32_load(0); i32_ge_u; br_if(1);
                      // Call pred(xs[i])
                      local_get(closure); i32_load(4); // env
                      local_get(xs); i32_const(list_data_off); i32_add;
                      local_get(i); i32_const(es); i32_mul; i32_add;
                });
                self.emit_load_at(&elem_ty, 0);
                wasm!(self.func, {
                      local_get(closure); i32_load(0); // table_idx
                });
                self.emit_closure_call(&elem_ty, &Ty::Bool);
                wasm!(self.func, {
                      if_empty;
                        // Found: alloc some(xs[i])
                        i32_const(es); call(self.emitter.rt.alloc); local_set(tmp);
                        local_get(tmp);
                        local_get(xs); i32_const(list_data_off); i32_add;
                        local_get(i); i32_const(es); i32_mul; i32_add;
                });
                self.emit_elem_copy_owned(&elem_ty);
                wasm!(self.func, {
                        local_get(tmp); local_set(result); br(2);
                      end;
                      local_get(i); i32_const(1); i32_add; local_set(i);
                      br(0);
                    end; end;
                    local_get(result); // result (none if not found)
                });
                self.scratch.free_i32(result);
                self.scratch.free_i32(tmp);
                self.scratch.free_i32(i);
                self.scratch.free_i32(closure);
                self.scratch.free_i32(xs);
            }
            "find_index" if args.len() == 2 && matches!(&args[1].ty, Ty::Fn { .. }) => {
                // find_index(xs, pred) → Option[Int]
                let elem_ty = self.resolve_list_elem(&args[0], None);
                let es = values::byte_size(&elem_ty) as i32;
                let xs = self.scratch.alloc_i32();
                let closure = self.scratch.alloc_i32();
                let i = self.scratch.alloc_i32();
                let tmp = self.scratch.alloc_i32();
                let result = self.scratch.alloc_i32();
                self.emit_expr(&args[0]);
                wasm!(self.func, { local_set(xs); });
                self.emit_expr(&args[1]);
                wasm!(self.func, {
                    local_set(closure);
                    i32_const(0); local_set(i);
                    i32_const(0); local_set(result); // result (default: none)
                    block_empty; loop_empty;
                      local_get(i); local_get(xs); i32_load(0); i32_ge_u; br_if(1);
                      local_get(closure); i32_load(4); // env
                      local_get(xs); i32_const(list_data_off); i32_add;
                      local_get(i); i32_const(es); i32_mul; i32_add;
                });
                self.emit_load_at(&elem_ty, 0);
                wasm!(self.func, { local_get(closure); i32_load(0); });
                self.emit_closure_call(&elem_ty, &Ty::Bool);
                wasm!(self.func, {
                      if_empty;
                        i32_const(8); call(self.emitter.rt.alloc); local_set(tmp);
                        local_get(tmp); local_get(i); i64_extend_i32_u; i64_store(0);
                        local_get(tmp); local_set(result); br(2);
                      end;
                      local_get(i); i32_const(1); i32_add; local_set(i);
                      br(0);
                    end; end;
                    local_get(result); // result (none if not found)
                });
                self.scratch.free_i32(result);
                self.scratch.free_i32(tmp);
                self.scratch.free_i32(i);
                self.scratch.free_i32(closure);
                self.scratch.free_i32(xs);
            }
            "any" => {
                // any(xs, pred) → Bool
                let elem_ty = self.resolve_list_elem(&args[0], None);
                let es = values::byte_size(&elem_ty) as i32;
                let xs = self.scratch.alloc_i32();
                let closure = self.scratch.alloc_i32();
                let i = self.scratch.alloc_i32();
                let result = self.scratch.alloc_i32();
                self.emit_expr(&args[0]);
                wasm!(self.func, { local_set(xs); });
                self.emit_expr(&args[1]);
                wasm!(self.func, {
                    local_set(closure);
                    i32_const(0); local_set(i);
                    i32_const(0); local_set(result); // result (default: false)
                    block_empty; loop_empty;
                      local_get(i); local_get(xs); i32_load(0); i32_ge_u; br_if(1);
                      local_get(closure); i32_load(4);
                      local_get(xs); i32_const(list_data_off); i32_add;
                      local_get(i); i32_const(es); i32_mul; i32_add;
                });
                self.emit_load_at(&elem_ty, 0);
                wasm!(self.func, { local_get(closure); i32_load(0); });
                self.emit_closure_call(&elem_ty, &Ty::Bool);
                wasm!(self.func, {
                      if_empty; i32_const(1); local_set(result); br(2); end;
                      local_get(i); i32_const(1); i32_add; local_set(i);
                      br(0);
                    end; end;
                    local_get(result); // result
                });
                self.scratch.free_i32(result);
                self.scratch.free_i32(i);
                self.scratch.free_i32(closure);
                self.scratch.free_i32(xs);
            }
            "all" => {
                let elem_ty = self.resolve_list_elem(&args[0], None);
                let es = values::byte_size(&elem_ty) as i32;
                let xs = self.scratch.alloc_i32();
                let closure = self.scratch.alloc_i32();
                let i = self.scratch.alloc_i32();
                let result = self.scratch.alloc_i32();
                self.emit_expr(&args[0]);
                wasm!(self.func, { local_set(xs); });
                self.emit_expr(&args[1]);
                wasm!(self.func, {
                    local_set(closure);
                    i32_const(0); local_set(i);
                    i32_const(1); local_set(result); // result (default: true)
                    block_empty; loop_empty;
                      local_get(i); local_get(xs); i32_load(0); i32_ge_u; br_if(1);
                      local_get(closure); i32_load(4);
                      local_get(xs); i32_const(list_data_off); i32_add;
                      local_get(i); i32_const(es); i32_mul; i32_add;
                });
                self.emit_load_at(&elem_ty, 0);
                wasm!(self.func, { local_get(closure); i32_load(0); });
                self.emit_closure_call(&elem_ty, &Ty::Bool);
                wasm!(self.func, {
                      i32_eqz;
                      if_empty; i32_const(0); local_set(result); br(2); end;
                      local_get(i); i32_const(1); i32_add; local_set(i);
                      br(0);
                    end; end;
                    local_get(result); // result
                });
                self.scratch.free_i32(result);
                self.scratch.free_i32(i);
                self.scratch.free_i32(closure);
                self.scratch.free_i32(xs);
            }
            "each" => {
                let elem_ty = self.resolve_list_elem(&args[0], None);
                let es = values::byte_size(&elem_ty) as i32;
                let xs = self.scratch.alloc_i32();
                let closure = self.scratch.alloc_i32();
                let i = self.scratch.alloc_i32();
                self.emit_expr(&args[0]);
                wasm!(self.func, { local_set(xs); });
                self.emit_expr(&args[1]);
                wasm!(self.func, {
                    local_set(closure);
                    i32_const(0); local_set(i);
                    block_empty; loop_empty;
                      local_get(i); local_get(xs); i32_load(0); i32_ge_u; br_if(1);
                      local_get(closure); i32_load(4);
                      local_get(xs); i32_const(list_data_off); i32_add;
                      local_get(i); i32_const(es); i32_mul; i32_add;
                });
                self.emit_load_at(&elem_ty, 0);
                wasm!(self.func, { local_get(closure); i32_load(0); });
                self.emit_closure_call(&elem_ty, &Ty::Unit);
                wasm!(self.func, {
                      local_get(i); i32_const(1); i32_add; local_set(i);
                      br(0);
                    end; end;
                });
                self.scratch.free_i32(i);
                self.scratch.free_i32(closure);
                self.scratch.free_i32(xs);
            }
            "take_end" => {
                // take_end(xs, n) = drop(xs, max(0, len-n))
                let elem_ty = self.resolve_list_elem(&args[0], None);
                let es = values::byte_size(&elem_ty) as i32;
                let xs = self.scratch.alloc_i32();
                let len = self.scratch.alloc_i32();
                let n = self.scratch.alloc_i32();
                let start = self.scratch.alloc_i32();
                let new_len = self.scratch.alloc_i32();
                let dst = self.scratch.alloc_i32();
                let i = self.scratch.alloc_i32();
                self.emit_expr(&args[0]);
                wasm!(self.func, { local_set(xs); local_get(xs); i32_load(0); local_set(len); });
                self.emit_expr(&args[1]);
                // n = min_u(n, len) on the i64 count (C-054); then
                // start = len - n and new_len = len - start never underflow. A
                // negative `n` (huge as usize) saturates to len → start 0 →
                // whole list, matching native take_end (`n as usize >= len`).
                self.emit_clamp_count_to_i32(super::calls_list_helpers::ClampHi::LenLocal(len));
                wasm!(self.func, {
                    local_set(n);
                    // start = len - n  (n already <= len, so >= 0)
                    local_get(len); local_get(n); i32_sub;
                    local_set(start);
                    // new_len = len - start
                    local_get(len); local_get(start); i32_sub;
                    local_set(new_len);
                    i32_const(list_hdr); local_get(new_len); i32_const(es); i32_mul; i32_add;
                    call(self.emitter.rt.alloc); local_set(dst);
                    local_get(dst); local_get(new_len); i32_store(0);
                    i32_const(0); local_set(i);
                    block_empty; loop_empty;
                      local_get(i); local_get(new_len); i32_ge_u; br_if(1);
                      local_get(dst); i32_const(list_data_off); i32_add;
                      local_get(i); i32_const(es); i32_mul; i32_add;
                      local_get(xs); i32_const(list_data_off); i32_add;
                      local_get(start); local_get(i); i32_add;
                      i32_const(es); i32_mul; i32_add;
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
                self.scratch.free_i32(new_len);
                self.scratch.free_i32(start);
                self.scratch.free_i32(n);
                self.scratch.free_i32(len);
                self.scratch.free_i32(xs);
            }
            "drop_end" => {
                // drop_end(xs, n) = take(xs, max(0, len-n))
                let elem_ty = self.resolve_list_elem(&args[0], None);
                let es = values::byte_size(&elem_ty) as i32;
                let xs = self.scratch.alloc_i32();
                let len = self.scratch.alloc_i32();
                let n = self.scratch.alloc_i32();
                let new_len = self.scratch.alloc_i32();
                let dst = self.scratch.alloc_i32();
                let i = self.scratch.alloc_i32();
                self.emit_expr(&args[0]);
                wasm!(self.func, { local_set(xs); local_get(xs); i32_load(0); local_set(len); });
                self.emit_expr(&args[1]);
                // n = min_u(n, len) on the i64 count (C-054); then
                // new_len = len - n never underflows and the wrapped-huge
                // count no longer reads OOB into uninitialized heap. A negative
                // `n` (huge as usize) saturates to len → new_len 0 → empty,
                // matching native drop_end (`n as usize >= len`).
                self.emit_clamp_count_to_i32(super::calls_list_helpers::ClampHi::LenLocal(len));
                wasm!(self.func, {
                    local_set(n);
                    local_get(len); local_get(n); i32_sub;
                    local_set(new_len); // new_len = len - n  (>= 0)
                    i32_const(list_hdr); local_get(new_len); i32_const(es); i32_mul; i32_add;
                    call(self.emitter.rt.alloc); local_set(dst);
                    local_get(dst); local_get(new_len); i32_store(0);
                    i32_const(0); local_set(i);
                    block_empty; loop_empty;
                      local_get(i); local_get(new_len); i32_ge_u; br_if(1);
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
                    local_get(dst);
                });
                self.scratch.free_i32(i);
                self.scratch.free_i32(dst);
                self.scratch.free_i32(new_len);
                self.scratch.free_i32(n);
                self.scratch.free_i32(len);
                self.scratch.free_i32(xs);
            }
            "repeat" => {
                // repeat(val, n) → List[A] — args[0] IS the element, not a list
                let elem_ty = args[0].ty.clone();
                let es = values::byte_size(&elem_ty) as i32;
                let val_vt = values::ty_to_valtype(&elem_ty).unwrap_or(ValType::I32);
                let val = self.scratch.alloc(val_vt);
                let n = self.scratch.alloc_i32();
                let dst = self.scratch.alloc_i32();
                let i = self.scratch.alloc_i32();
                self.emit_expr(&args[0]); // val
                wasm!(self.func, { local_set(val); });
                self.emit_expr(&args[1]); // n
                // Unsigned-saturate the i64 count to a non-negative i32 BEFORE
                // narrowing (C-054). A bare i32_wrap_i64 turned `2^32` into 0
                // (silent empty list) and `2^32-1` into a giant span. `repeat`
                // produces `n` OBSERVABLE elements (so we cannot clamp to a small
                // ceiling like with_capacity), but a count past i32::MAX cannot
                // be materialized on EITHER target — native `vec![x; n as usize]`
                // aborts on the multi-GB request just as wasm `memory.grow`
                // fails — so saturating to the i32::MAX sentinel keeps the wrap
                // lossless and the two targets BOTH-FAIL instead of
                // wasm-silently-emptying. Native `n as usize` is UNSIGNED, so a
                // NEGATIVE count is huge (→ also both-fail), NOT empty; the
                // unsigned clamp reproduces that. Fixtures test only small COUNTS
                // (the multi-GB SIZE boundary is machine-dependent, excluded).
                const REPEAT_MAX_COUNT: i64 = i32::MAX as i64;
                self.emit_clamp_count_to_i32(super::calls_list_helpers::ClampHi::Const(REPEAT_MAX_COUNT));
                wasm!(self.func, {
                    local_set(n);
                    i32_const(list_hdr); local_get(n); i32_const(es); i32_mul; i32_add;
                    call(self.emitter.rt.alloc); local_set(dst);
                    local_get(dst); local_get(n); i32_store(0);
                    i32_const(0); local_set(i);
                    block_empty; loop_empty;
                      local_get(i); local_get(n); i32_ge_u; br_if(1);
                      local_get(dst); i32_const(list_data_off); i32_add;
                      local_get(i); i32_const(es); i32_mul; i32_add;
                      local_get(val);
                });
                // The result owns n references to the SAME element block —
                // without one inc per stored slot, a deep Dec of the result
                // decs that single block n times (sentinel trap; surfaced via
                // the for+concat-push → list.repeat rewrite). A fresh-arg
                // call leaks exactly one count (no owner for the original
                // ref) — the safe direction.
                if crate::pass_perceus::is_heap_type(&elem_ty) {
                    wasm!(self.func, { call(self.emitter.rt.rc_inc); });
                }
                self.emit_store_at(&elem_ty, 0);
                wasm!(self.func, {
                      local_get(i); i32_const(1); i32_add; local_set(i);
                      br(0);
                    end; end;
                    local_get(dst);
                });
                self.scratch.free_i32(i);
                self.scratch.free_i32(dst);
                self.scratch.free_i32(n);
                self.scratch.free(val, val_vt);
            }
            "reduce" => {
                // reduce(xs, f) → Option[A]: fold starting from xs[0]
                let elem_ty = self.resolve_list_elem(&args[0], None);
                let es = values::byte_size(&elem_ty) as i32;
                let acc_vt = values::ty_to_valtype(&elem_ty).unwrap_or(ValType::I64);
                let xs = self.scratch.alloc_i32();
                let closure = self.scratch.alloc_i32();
                let i = self.scratch.alloc_i32();
                let acc = self.scratch.alloc(acc_vt);
                let tmp = self.scratch.alloc_i32();
                self.emit_expr(&args[0]);
                wasm!(self.func, { local_set(xs); });
                self.emit_expr(&args[1]); // fn(a, b) -> a
                wasm!(self.func, {
                    local_set(closure);
                    local_get(xs); i32_load(0); i32_eqz;
                    if_i32; i32_const(0); // empty → none
                    else_;
                      // acc = xs[0]
                      local_get(xs); i32_const(list_data_off); i32_add;
                });
                self.emit_load_at(&elem_ty, 0);
                wasm!(self.func, { local_set(acc); });
                wasm!(self.func, {
                      i32_const(1); local_set(i); // i = 1
                      block_empty; loop_empty;
                        local_get(i); local_get(xs); i32_load(0); i32_ge_u; br_if(1);
                        // Call f(acc, xs[i])
                        local_get(closure); i32_load(4); // env
                        local_get(acc); // acc
                        local_get(xs); i32_const(list_data_off); i32_add;
                        local_get(i); i32_const(es); i32_mul; i32_add;
                });
                self.emit_load_at(&elem_ty, 0);
                wasm!(self.func, {
                        local_get(closure); i32_load(0); // table_idx
                });
                // call_indirect (env, a, b) → a
                {
                    let mut ct = vec![ValType::I32]; // env
                    if let Some(vt) = values::ty_to_valtype(&elem_ty) { ct.push(vt); ct.push(vt); }
                    self.emit_call_indirect(ct, values::ret_type(&elem_ty));
                }
                wasm!(self.func, {
                        local_set(acc); // update acc
                        local_get(i); i32_const(1); i32_add; local_set(i);
                        br(0);
                      end; end;
                      // Wrap acc in some
                      i32_const(es); call(self.emitter.rt.alloc); local_set(tmp);
                      local_get(tmp); local_get(acc);
                });
                self.emit_store_at(&elem_ty, 0);
                wasm!(self.func, { local_get(tmp); end; });
                self.scratch.free_i32(tmp);
                self.scratch.free(acc, acc_vt);
                self.scratch.free_i32(i);
                self.scratch.free_i32(closure);
                self.scratch.free_i32(xs);
            }
            _ => return false,
        }
        true
    }
}
