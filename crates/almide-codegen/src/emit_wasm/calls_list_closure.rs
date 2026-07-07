//! List stdlib closure-based call dispatch for WASM codegen.
//!
//! Functions that take closures as arguments: find, find_index, any, all, each,
//! reduce, flat_map, filter_map, sort_by, take_while, drop_while, count,
//! partition, update, scan, zip_with, unique_by.

use super::FuncCompiler;
use super::values;
use almide_ir::IrExpr;
use almide_lang::types::Ty;
use wasm_encoder::ValType;

impl FuncCompiler<'_> {
    /// Dispatch a list stdlib closure-based call. Returns true if handled.
    pub(super) fn emit_list_closure_call(&mut self, method: &str, args: &[IrExpr]) -> bool {
        // Group-2 methods (find, find_index, any, all, each, take_end,
        // drop_end, repeat, reduce) live in a disjoint sub-match in the
        // _p2 part file. Patterns are disjoint, so chain order is irrelevant.
        if self.emit_list_closure_call_g2(method, args) {
            return true;
        }
        use super::engine::layout::{LIST, list as ll};
        let list_data_off = self.emitter.layout_reg.fixed_offset(LIST, ll::DATA) as i32;
        let list_hdr = self.emitter.layout_reg.header_size(LIST) as i32;
        match method {
            "flat_map" => {
                // flat_map(xs, f) → List[B]: f returns List[B], flatten results
                let elem_ty = self.resolve_list_elem(&args[0], None);
                // Output element type B: infer from fn return type List[B]
                let out_elem_ty = if let Ty::Fn { ret, .. } = &args[1].ty {
                    self.list_elem_ty(ret) // List[B] → B
                } else { elem_ty.clone() };
                let out_es = values::byte_size(&out_elem_ty) as i32;
                let es = values::byte_size(&elem_ty) as i32;
                let xs = self.scratch.alloc_i32();
                let closure = self.scratch.alloc_i32();
                let len = self.scratch.alloc_i32();
                let lol = self.scratch.alloc_i32(); // list-of-lists
                let i = self.scratch.alloc_i32();
                let total = self.scratch.alloc_i32();
                let result = self.scratch.alloc_i32();
                let inner = self.scratch.alloc_i32();
                let j = self.scratch.alloc_i32();
                self.emit_expr(&args[0]);
                wasm!(self.func, { local_set(xs); });
                self.emit_expr(&args[1]);
                wasm!(self.func, {
                    local_set(closure);
                    local_get(xs); i32_load(0); local_set(len);
                    // Alloc temp list-of-lists: [len][ptr0][ptr1]...
                    i32_const(list_hdr); local_get(len); i32_const(4); i32_mul; i32_add;
                    call(self.emitter.rt.alloc); local_set(lol);
                    local_get(lol); local_get(len); i32_store(0);
                    i32_const(0); local_set(i);
                    block_empty; loop_empty;
                      local_get(i); local_get(len); i32_ge_u; br_if(1);
                      // Call f(xs[i]) → List[B]
                      local_get(lol); i32_const(list_data_off); i32_add;
                      local_get(i); i32_const(4); i32_mul; i32_add; // dst addr for result ptr
                      local_get(closure); i32_load(4); // env
                      local_get(xs); i32_const(list_data_off); i32_add;
                      local_get(i); i32_const(es); i32_mul; i32_add;
                });
                self.emit_load_at(&elem_ty, 0);
                wasm!(self.func, {
                      local_get(closure); i32_load(0); // table_idx
                });
                self.emit_closure_call(&elem_ty, &Ty::Unknown); // returns List ptr (i32)
                wasm!(self.func, {
                      i32_store(0); // store result list ptr
                      local_get(i); i32_const(1); i32_add; local_set(i);
                      br(0);
                    end; end;
                });
                // Flatten: count total elements
                wasm!(self.func, {
                    i32_const(0); local_set(total);
                    i32_const(0); local_set(i);
                    block_empty; loop_empty;
                      local_get(i); local_get(lol); i32_load(0); i32_ge_u; br_if(1);
                      local_get(total);
                      local_get(lol); i32_const(list_data_off); i32_add;
                      local_get(i); i32_const(4); i32_mul; i32_add;
                      i32_load(0); i32_load(0);
                      i32_add; local_set(total);
                      local_get(i); i32_const(1); i32_add; local_set(i);
                      br(0);
                    end; end;
                    // Alloc result
                    i32_const(list_hdr); local_get(total); i32_const(out_es); i32_mul; i32_add;
                    call(self.emitter.rt.alloc); local_set(result);
                    local_get(result); local_get(total); i32_store(0);
                });
                // Copy all sub-list elements
                wasm!(self.func, {
                    i32_const(0); local_set(total); // reuse as dst_offset
                    i32_const(0); local_set(i);
                    block_empty; loop_empty;
                      local_get(i); local_get(lol); i32_load(0); i32_ge_u; br_if(1);
                      local_get(lol); i32_const(list_data_off); i32_add;
                      local_get(i); i32_const(4); i32_mul; i32_add;
                      i32_load(0); local_set(inner);
                      i32_const(0); local_set(j);
                      block_empty; loop_empty;
                        local_get(j); local_get(inner); i32_load(0); i32_ge_u; br_if(1);
                        local_get(result); i32_const(list_data_off); i32_add;
                        local_get(total); i32_const(out_es); i32_mul; i32_add;
                        local_get(inner); i32_const(list_data_off); i32_add;
                        local_get(j); i32_const(out_es); i32_mul; i32_add;
                });
                self.emit_elem_copy(&out_elem_ty);
                wasm!(self.func, {
                        local_get(total); i32_const(1); i32_add; local_set(total);
                        local_get(j); i32_const(1); i32_add; local_set(j);
                        br(0);
                      end; end;
                      local_get(i); i32_const(1); i32_add; local_set(i);
                      br(0);
                    end; end;
                    local_get(result);
                });
                self.scratch.free_i32(j);
                self.scratch.free_i32(inner);
                self.scratch.free_i32(result);
                self.scratch.free_i32(total);
                self.scratch.free_i32(i);
                self.scratch.free_i32(lol);
                self.scratch.free_i32(len);
                self.scratch.free_i32(closure);
                self.scratch.free_i32(xs);
            }
            "filter_map" => {
                // filter_map(xs, f) → List[B]: f returns Option[B], keep some values
                let elem_ty = self.resolve_list_elem(&args[0], Some(&args[1]));
                let es = values::byte_size(&elem_ty) as i32;
                // Output element type B from the return type of the function or
                // the overall call return type. After mono, TypeVars in fn.ret
                // may still be unresolved — fall back to the call's ret_ty.
                let out_elem_ty = if let Ty::Fn { ret, .. } = &args[1].ty {
                    if let Ty::Applied(_, inner) = ret.as_ref() {
                        inner.first().cloned().filter(|t| !t.is_unresolved()).unwrap_or(Ty::Int)
                    } else { self.list_elem_ty(ret) }
                } else if let Some(vt) = self.resolve_closure_ret_valtype(&args[1]) {
                    // From lifted closure's registered WASM type: Option[B] → i32 ptr,
                    // but inner B must be resolved from call ret_ty.
                    values::vt_to_placeholder_ty(vt)
                } else { Ty::Int };
                let out_es = values::byte_size(&out_elem_ty) as i32;
                let xs = self.scratch.alloc_i32();
                let closure = self.scratch.alloc_i32();
                let len = self.scratch.alloc_i32();
                let dst = self.scratch.alloc_i32();
                let i = self.scratch.alloc_i32();
                let opt = self.scratch.alloc_i32();
                self.emit_expr(&args[0]);
                wasm!(self.func, { local_set(xs); });
                self.emit_expr(&args[1]);
                wasm!(self.func, {
                    local_set(closure);
                    local_get(xs); i32_load(0); local_set(len);
                    i32_const(list_hdr); local_get(len); i32_const(out_es); i32_mul; i32_add;
                    call(self.emitter.rt.alloc); local_set(dst);
                    local_get(dst); i32_const(0); i32_store(0);
                    i32_const(0); local_set(i);
                    block_empty; loop_empty;
                      local_get(i); local_get(len); i32_ge_u; br_if(1);
                      local_get(closure); i32_load(4); // env
                      local_get(xs); i32_const(list_data_off); i32_add;
                      local_get(i); i32_const(es); i32_mul; i32_add;
                });
                self.emit_load_at(&elem_ty, 0);
                wasm!(self.func, {
                      local_get(closure); i32_load(0);
                });
                self.emit_closure_call(&elem_ty, &Ty::Unknown); // returns Option ptr (i32)
                wasm!(self.func, {
                      local_set(opt); // option result
                      local_get(opt); i32_const(0); i32_ne;
                      if_empty;
                        // Append unwrapped value to result
                        local_get(dst); i32_const(list_data_off); i32_add;
                        local_get(dst); i32_load(0); i32_const(out_es); i32_mul; i32_add;
                        local_get(opt); // some ptr
                });
                // Load inner value from some ptr
                self.emit_load_at(&out_elem_ty, 0);
                self.emit_store_at(&out_elem_ty, 0);
                wasm!(self.func, {
                        local_get(dst);
                        local_get(dst); i32_load(0); i32_const(1); i32_add;
                        i32_store(0);
                      end;
                      local_get(i); i32_const(1); i32_add; local_set(i);
                      br(0);
                    end; end;
                    local_get(dst);
                });
                self.scratch.free_i32(opt);
                self.scratch.free_i32(i);
                self.scratch.free_i32(dst);
                self.scratch.free_i32(len);
                self.scratch.free_i32(closure);
                self.scratch.free_i32(xs);
            }
            "swap" => {
                // swap(xs, i, j) → List[A]: copy with elements at i and j swapped
                let elem_ty = self.resolve_list_elem(&args[0], None);
                let es = values::byte_size(&elem_ty) as i32;
                let elem_vt = values::ty_to_valtype(&elem_ty).unwrap_or(ValType::I32);
                let xs = self.scratch.alloc_i32();
                let idx_i = self.scratch.alloc_i32();
                let idx_j = self.scratch.alloc_i32();
                let in_i = self.scratch.alloc_i32();
                let in_j = self.scratch.alloc_i32();
                let len = self.scratch.alloc_i32();
                let dst = self.scratch.alloc_i32();
                let k = self.scratch.alloc_i32();
                let tmp = self.scratch.alloc(elem_vt);
                self.emit_expr(&args[0]);
                wasm!(self.func, { local_set(xs); local_get(xs); i32_load(0); local_set(len); });
                // Both indices checked on the full i64 (C-054): a negative or
                // >= 2^32 index must take native's no-op path, not wrap to a
                // small in-range slot and swap the wrong pair. idx_* are
                // saturated to [0,len]; in_* gate the in-place swap.
                self.emit_expr(&args[1]); // i
                self.emit_checked_index_i32(len, in_i);
                wasm!(self.func, { local_set(idx_i); });
                self.emit_expr(&args[2]); // j
                self.emit_checked_index_i32(len, in_j);
                wasm!(self.func, {
                    local_set(idx_j);
                    // Alloc copy
                    i32_const(list_hdr); local_get(len); i32_const(es); i32_mul; i32_add;
                    call(self.emitter.rt.alloc); local_set(dst);
                    local_get(dst); local_get(len); i32_store(0);
                    // Copy all elements
                    i32_const(0); local_set(k);
                    block_empty; loop_empty;
                      local_get(k); local_get(len); i32_ge_u; br_if(1);
                      local_get(dst); i32_const(list_data_off); i32_add;
                      local_get(k); i32_const(es); i32_mul; i32_add;
                      local_get(xs); i32_const(list_data_off); i32_add;
                      local_get(k); i32_const(es); i32_mul; i32_add;
                });
                self.emit_elem_copy_owned(&elem_ty);
                wasm!(self.func, {
                      local_get(k); i32_const(1); i32_add; local_set(k);
                      br(0);
                    end; end;
                });
                // Native is a no-op (returns the copy unchanged) unless BOTH
                // indices are in range (in_i && in_j, computed on the full i64).
                wasm!(self.func, {
                    local_get(in_i); local_get(in_j); i32_and;
                    if_empty;
                });
                // Swap dst[i] and dst[j] using typed scratch local as temp
                // tmp = dst[i]
                wasm!(self.func, {
                    local_get(dst); i32_const(list_data_off); i32_add;
                    local_get(idx_i); i32_const(es); i32_mul; i32_add;
                });
                self.emit_load_at(&elem_ty, 0);
                wasm!(self.func, { local_set(tmp); });
                // dst[i] = dst[j]
                wasm!(self.func, {
                    local_get(dst); i32_const(list_data_off); i32_add;
                    local_get(idx_i); i32_const(es); i32_mul; i32_add;
                    local_get(dst); i32_const(list_data_off); i32_add;
                    local_get(idx_j); i32_const(es); i32_mul; i32_add;
                });
                self.emit_elem_copy(&elem_ty);
                // dst[j] = tmp
                wasm!(self.func, {
                    local_get(dst); i32_const(list_data_off); i32_add;
                    local_get(idx_j); i32_const(es); i32_mul; i32_add;
                    local_get(tmp);
                });
                self.emit_store_at(&elem_ty, 0);
                wasm!(self.func, { end; local_get(dst); });
                self.scratch.free(tmp, elem_vt);
                self.scratch.free_i32(k);
                self.scratch.free_i32(dst);
                self.scratch.free_i32(len);
                self.scratch.free_i32(in_j);
                self.scratch.free_i32(in_i);
                self.scratch.free_i32(idx_j);
                self.scratch.free_i32(idx_i);
                self.scratch.free_i32(xs);
            }
            "chunk" => {
                // chunk(xs, n) → List[List[A]]
                // Outer list of inner lists. Each inner list has up to n elements.
                let elem_ty = self.resolve_list_elem(&args[0], None);
                let es = values::byte_size(&elem_ty) as i32;
                let xs = self.scratch.alloc_i32();
                let n = self.scratch.alloc_i32();
                let len = self.scratch.alloc_i32();
                let num_chunks = self.scratch.alloc_i32();
                let outer = self.scratch.alloc_i32();
                let i = self.scratch.alloc_i32();
                let chunk_len = self.scratch.alloc_i32();
                let inner = self.scratch.alloc_i32();
                let j = self.scratch.alloc_i32();
                self.emit_expr(&args[0]);
                wasm!(self.func, { local_set(xs); local_get(xs); i32_load(0); local_set(len); });
                self.emit_expr(&args[1]); // n
                // Unsigned-saturate n to [0, i32::MAX] on the i64 count (C-054)
                // so a huge chunk size cannot wrap to a small in-range value.
                // Native `chunks(n as usize)`: a NEGATIVE n (huge as usize) or
                // any n >= len groups into ONE chunk — the ALS-T4 norm. The
                // unsigned clamp sends -1/2^32/… to the i32::MAX sentinel
                // (>= len → one chunk); ONLY a genuine 0 stays 0 and aborts
                // below with the ALS-T6 form, byte-matching native.
                const CHUNK_MAX_N: i64 = i32::MAX as i64;
                self.emit_clamp_count_to_i32(super::calls_list_helpers::ClampHi::Const(CHUNK_MAX_N));
                // chunk(xs, 0) is a domain error (ALS-T4): `Error: chunk size
                // must be positive` + exit 1 on BOTH targets — previously a raw
                // native `chunks(0)` panic (exit 101) vs a wasm div-by-zero trap.
                let chunk_zero_msg =
                    self.emitter.intern_string("Error: chunk size must be positive\n") as i32;
                let div_trap = self.emitter.rt.div_trap;
                wasm!(self.func, {
                    local_set(n);
                    local_get(n); i32_eqz;
                    if_empty;
                      i32_const(chunk_zero_msg);
                      call(div_trap);
                    end;
                    // num_chunks: n >= len → one chunk of all (0 when len == 0,
                    // matching native `[].chunks(k>0)` = empty); else
                    // ceil(len / n) = (len + n - 1) / n. The fast path avoids
                    // the `len + n - 1` i32 overflow for huge n; n == 0 already
                    // aborted above.
                    local_get(n); local_get(len); i32_ge_u;  // n >= len
                    if_i32;
                      local_get(len); i32_const(0); i32_gt_u;
                      if_i32; i32_const(1); else_; i32_const(0); end;
                    else_;
                      local_get(len); local_get(n); i32_add; i32_const(1); i32_sub;
                      local_get(n); i32_div_u;
                    end;
                    local_set(num_chunks);
                    // Alloc outer: 4 + num_chunks * 4 (list of ptrs)
                    i32_const(list_hdr); local_get(num_chunks); i32_const(4); i32_mul; i32_add;
                    call(self.emitter.rt.alloc); local_set(outer);
                    local_get(outer); local_get(num_chunks); i32_store(0);
                    i32_const(0); local_set(i);
                    block_empty; loop_empty;
                      local_get(i); local_get(num_chunks); i32_ge_u; br_if(1);
                      // chunk_len = min(n, len - i*n)
                      local_get(len); local_get(i); local_get(n); i32_mul; i32_sub;
                      local_set(chunk_len);
                      local_get(chunk_len); local_get(n); i32_gt_u;
                      if_empty; local_get(n); local_set(chunk_len); end;
                      // Alloc inner: 4 + chunk_len * es
                      i32_const(list_hdr); local_get(chunk_len); i32_const(es); i32_mul; i32_add;
                      call(self.emitter.rt.alloc); local_set(inner);
                      local_get(inner); local_get(chunk_len); i32_store(0);
                      // Copy elements
                      i32_const(0); local_set(j);
                      block_empty; loop_empty;
                        local_get(j); local_get(chunk_len); i32_ge_u; br_if(1);
                        local_get(inner); i32_const(list_data_off); i32_add;
                        local_get(j); i32_const(es); i32_mul; i32_add;
                        local_get(xs); i32_const(list_data_off); i32_add;
                        local_get(i); local_get(n); i32_mul;
                        local_get(j); i32_add;
                        i32_const(es); i32_mul; i32_add;
                });
                self.emit_elem_copy_owned(&elem_ty);
                wasm!(self.func, {
                        local_get(j); i32_const(1); i32_add; local_set(j);
                        br(0);
                      end; end;
                      // outer[i] = inner_ptr
                      local_get(outer); i32_const(list_data_off); i32_add;
                      local_get(i); i32_const(4); i32_mul; i32_add;
                      local_get(inner); i32_store(0);
                      local_get(i); i32_const(1); i32_add; local_set(i);
                      br(0);
                    end; end;
                    local_get(outer);
                });
                self.scratch.free_i32(j);
                self.scratch.free_i32(inner);
                self.scratch.free_i32(chunk_len);
                self.scratch.free_i32(i);
                self.scratch.free_i32(outer);
                self.scratch.free_i32(num_chunks);
                self.scratch.free_i32(len);
                self.scratch.free_i32(n);
                self.scratch.free_i32(xs);
            }
            "windows" | "window" => {
                // windows(xs, n) → List[List[A]]: sliding windows of size n
                let elem_ty = self.resolve_list_elem(&args[0], None);
                let es = values::byte_size(&elem_ty) as i32;
                let xs = self.scratch.alloc_i32();
                let n = self.scratch.alloc_i32();
                let len = self.scratch.alloc_i32();
                let num_win = self.scratch.alloc_i32();
                let outer = self.scratch.alloc_i32();
                let i = self.scratch.alloc_i32();
                let inner = self.scratch.alloc_i32();
                let j = self.scratch.alloc_i32();
                self.emit_expr(&args[0]);
                wasm!(self.func, { local_set(xs); local_get(xs); i32_load(0); local_set(len); });
                self.emit_expr(&args[1]);
                // Unsigned-saturate n to [0, i32::MAX] on the i64 count (C-054)
                // so a huge window size cannot wrap to a small in-range value.
                // Native `windows`: `(n as usize) > len → []`, so a NEGATIVE n
                // (huge as usize) or any n > len → empty — the ALS-T4 norm.
                const WINDOWS_MAX_N: i64 = i32::MAX as i64;
                self.emit_clamp_count_to_i32(super::calls_list_helpers::ClampHi::Const(WINDOWS_MAX_N));
                // windows(xs, 0) is a domain error (ALS-T4): `Error: window size
                // must be positive` + exit 1 on BOTH targets. Previously wasm
                // SILENTLY returned len+1 empty windows (num_win = len - 0 + 1)
                // while native panicked raw — a silent-wrong divergence.
                let window_zero_msg =
                    self.emitter.intern_string("Error: window size must be positive\n") as i32;
                let div_trap = self.emitter.rt.div_trap;
                wasm!(self.func, {
                    local_set(n);
                    local_get(n); i32_eqz;
                    if_empty;
                      i32_const(window_zero_msg);
                      call(div_trap);
                    end;
                    // num_win = if len >= n then len - n + 1 else 0
                    local_get(len); local_get(n); i32_ge_u;
                    if_i32;
                      local_get(len); local_get(n); i32_sub; i32_const(1); i32_add;
                    else_;
                      i32_const(0);
                    end;
                    local_set(num_win);
                    // Alloc outer: 4 + num_win * 4
                    i32_const(list_hdr); local_get(num_win); i32_const(4); i32_mul; i32_add;
                    call(self.emitter.rt.alloc); local_set(outer);
                    local_get(outer); local_get(num_win); i32_store(0);
                    i32_const(0); local_set(i);
                    block_empty; loop_empty;
                      local_get(i); local_get(num_win); i32_ge_u; br_if(1);
                      // Alloc inner: 4 + n * es
                      i32_const(list_hdr); local_get(n); i32_const(es); i32_mul; i32_add;
                      call(self.emitter.rt.alloc); local_set(inner);
                      local_get(inner); local_get(n); i32_store(0);
                      // Copy n elements starting at i
                      i32_const(0); local_set(j);
                      block_empty; loop_empty;
                        local_get(j); local_get(n); i32_ge_u; br_if(1);
                        local_get(inner); i32_const(list_data_off); i32_add;
                        local_get(j); i32_const(es); i32_mul; i32_add;
                        local_get(xs); i32_const(list_data_off); i32_add;
                        local_get(i); local_get(j); i32_add;
                        i32_const(es); i32_mul; i32_add;
                });
                self.emit_elem_copy_owned(&elem_ty);
                wasm!(self.func, {
                        local_get(j); i32_const(1); i32_add; local_set(j);
                        br(0);
                      end; end;
                      // outer[i] = inner_ptr
                      local_get(outer); i32_const(list_data_off); i32_add;
                      local_get(i); i32_const(4); i32_mul; i32_add;
                      local_get(inner); i32_store(0);
                      local_get(i); i32_const(1); i32_add; local_set(i);
                      br(0);
                    end; end;
                    local_get(outer);
                });
                self.scratch.free_i32(j);
                self.scratch.free_i32(inner);
                self.scratch.free_i32(i);
                self.scratch.free_i32(outer);
                self.scratch.free_i32(num_win);
                self.scratch.free_i32(len);
                self.scratch.free_i32(n);
                self.scratch.free_i32(xs);
            }
            "dedup" => {
                // dedup(xs) → List[A]: remove consecutive duplicates
                let elem_ty = self.resolve_list_elem(&args[0], None);
                let es = values::byte_size(&elem_ty) as i32;
                let xs = self.scratch.alloc_i32();
                let len = self.scratch.alloc_i32();
                let dst = self.scratch.alloc_i32();
                let i = self.scratch.alloc_i32();
                let out_count = self.scratch.alloc_i32();
                self.emit_expr(&args[0]);
                wasm!(self.func, {
                    local_set(xs);
                    local_get(xs); i32_load(0); local_set(len);
                    // Alloc dst (max = len)
                    i32_const(list_hdr); local_get(len); i32_const(es); i32_mul; i32_add;
                    call(self.emitter.rt.alloc); local_set(dst);
                    i32_const(0); local_set(out_count);
                    // If empty, return empty
                    local_get(len); i32_eqz;
                    if_empty;
                      local_get(dst); i32_const(0); i32_store(0);
                    else_;
                      // Always include first element
                      local_get(dst); i32_const(list_data_off); i32_add;
                      local_get(xs); i32_const(list_data_off); i32_add;
                });
                self.emit_elem_copy_owned(&elem_ty); // SHARE: copy from borrowed xs into fresh dst
                wasm!(self.func, {
                      i32_const(1); local_set(out_count); // out_count = 1
                      i32_const(1); local_set(i); // i = 1
                      block_empty; loop_empty;
                        local_get(i); local_get(len); i32_ge_u; br_if(1);
                        // Compare xs[i] with xs[i-1]
                        local_get(xs); i32_const(list_data_off); i32_add;
                        local_get(i); i32_const(es); i32_mul; i32_add;
                });
                self.emit_load_at(&elem_ty, 0);
                wasm!(self.func, {
                        local_get(xs); i32_const(list_data_off); i32_add;
                        local_get(i); i32_const(1); i32_sub;
                        i32_const(es); i32_mul; i32_add;
                });
                self.emit_load_at(&elem_ty, 0);
                // Structural eq: collapse consecutive value-equal elements
                // (String + compound), matching native dedup-by-`==`, not pointer
                // identity.
                self.emit_eq_typed(&elem_ty);
                wasm!(self.func, {
                        i32_eqz; // not equal → include
                        if_empty;
                          local_get(dst); i32_const(list_data_off); i32_add;
                          local_get(out_count); i32_const(es); i32_mul; i32_add;
                          local_get(xs); i32_const(list_data_off); i32_add;
                          local_get(i); i32_const(es); i32_mul; i32_add;
                });
                self.emit_elem_copy_owned(&elem_ty); // SHARE: copy from borrowed xs into fresh dst
                wasm!(self.func, {
                          local_get(out_count); i32_const(1); i32_add; local_set(out_count);
                        end;
                        local_get(i); i32_const(1); i32_add; local_set(i);
                        br(0);
                      end; end;
                      local_get(dst); local_get(out_count); i32_store(0);
                    end;
                    local_get(dst);
                });
                self.scratch.free_i32(out_count);
                self.scratch.free_i32(i);
                self.scratch.free_i32(dst);
                self.scratch.free_i32(len);
                self.scratch.free_i32(xs);
            }
            "sort_by" => {
                // sort_by(xs, f) → List[A]: bubble sort by key function
                // Strategy: copy list, compute keys into parallel array, bubble sort both
                let elem_ty = self.resolve_list_elem(&args[0], None);
                let es = values::byte_size(&elem_ty) as i32;
                let elem_vt = values::ty_to_valtype(&elem_ty).unwrap_or(ValType::I32);
                // Infer key type from closure return type. The closure's
                // `Ty::Fn.ret` can be Unknown/TypeVar when inference left the
                // Lambda param generic; fall back to the lifted function's
                // registered WASM return ValType so `key_is_str` and `ks`
                // match the closure's call_indirect signature exactly.
                let key_ty_initial = if let Ty::Fn { ret, .. } = &args[1].ty {
                    (**ret).clone()
                } else { Ty::Int };
                let key_vt = if !key_ty_initial.is_unresolved() {
                    values::ty_to_valtype(&key_ty_initial).unwrap_or(ValType::I32)
                } else {
                    self.resolve_closure_ret_valtype(&args[1]).unwrap_or(ValType::I64)
                };
                // The keys array stores each key by its WASM bucket: i32 (a
                // Bool value, a String/List/Tuple heap pointer) or i64/f64 (Int/
                // Float). `key_is_str` selects only the *storage/swap width*
                // (i32 vs i64). The *comparison* is type-directed via
                // `emit_ord_cmp3` below, so a Bool key (i32 value 0/1) is no
                // longer mistaken for a String pointer and fed to `string.cmp`.
                let key_is_str = matches!(key_vt, ValType::I32);
                // A Float key is PRE-TRANSFORMED to its i64 totalOrder key at
                // compute time (`emit_f64_total_order_key`), so the parallel
                // key array stores, loads, compares, and swaps it on the plain
                // i64 path — fixing the old `local.set(i64) <- f64` validator
                // crash and giving IEEE-754 totalOrder for free (C-055).
                let key_is_float = matches!(key_vt, ValType::F64);
                let ks: i32 = match key_vt {
                    ValType::I64 | ValType::F64 => 8,
                    _ => 4,
                };
                // Concrete key type for the order comparison. Prefer the
                // resolved closure return type (keeps Bool/String/Variant
                // distinct); fall back to the ValType-derived placeholder
                // (i32 → String pointer) only when inference left it generic.
                let cmp_key_ty = if !key_ty_initial.is_unresolved() {
                    key_ty_initial.clone()
                } else {
                    values::vt_to_placeholder_ty(key_vt)
                };
                // Synthesize a concrete key_ty for `emit_closure_call` sizing.
                let key_ty = values::vt_to_placeholder_ty(key_vt);
                let xs = self.scratch.alloc_i32();
                let closure = self.scratch.alloc_i32();
                let len = self.scratch.alloc_i32();
                let dst = self.scratch.alloc_i32();
                let keys = self.scratch.alloc_i32();
                let i = self.scratch.alloc_i32();
                let j = self.scratch.alloc_i32();
                let tmp_key = if key_is_str { self.scratch.alloc_i32() } else { self.scratch.alloc_i64() };
                let tmp_elem = self.scratch.alloc(elem_vt);
                self.emit_expr(&args[0]);
                wasm!(self.func, { local_set(xs); });
                self.emit_expr(&args[1]);
                wasm!(self.func, {
                    local_set(closure);
                    local_get(xs); i32_load(0); local_set(len);
                    // Alloc copy of elements
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
                // SHARE: copy the borrowed source elements into the fresh sorted
                // result — dup so the result owns them (the in-place swaps below just
                // rearrange these owned references; the source's Dec is balanced).
                self.emit_elem_copy_owned(&elem_ty);
                wasm!(self.func, {
                      local_get(i); i32_const(1); i32_add; local_set(i);
                      br(0);
                    end; end;
                });
                // Alloc keys array: len * ks
                wasm!(self.func, {
                    local_get(len); i32_const(ks); i32_mul;
                    call(self.emitter.rt.alloc); local_set(keys);
                    // Compute keys for all elements
                    i32_const(0); local_set(i);
                    block_empty; loop_empty;
                      local_get(i); local_get(len); i32_ge_u; br_if(1);
                      local_get(closure); i32_load(4); // env
                      local_get(dst); i32_const(list_data_off); i32_add;
                      local_get(i); i32_const(es); i32_mul; i32_add;
                });
                self.emit_load_at(&elem_ty, 0);
                wasm!(self.func, {
                      local_get(closure); i32_load(0); // table_idx
                });
                self.emit_closure_call(&elem_ty, &key_ty);
                // Float key → i64 totalOrder key (so it rides the i64 path).
                if key_is_float {
                    self.emit_f64_total_order_key();
                }
                if key_is_str {
                    wasm!(self.func, {
                          local_set(tmp_key);
                          local_get(keys);
                          local_get(i); i32_const(ks); i32_mul; i32_add;
                          local_get(tmp_key); i32_store(0);
                    });
                } else {
                    wasm!(self.func, {
                          local_set(tmp_key);
                          local_get(keys);
                          local_get(i); i32_const(ks); i32_mul; i32_add;
                          local_get(tmp_key); i64_store(0);
                    });
                }
                wasm!(self.func, {
                      local_get(i); i32_const(1); i32_add; local_set(i);
                      br(0);
                    end; end;
                });
                // Bubble sort: outer loop i from 0..len-1, inner loop j from 0..len-1-i.
                // Skip entirely when len < 2 (nothing to compare) — `len - 1`
                // would underflow to u32::MAX for unsigned comparison and turn
                // the loop into an infinite memory-walker.
                wasm!(self.func, {
                    block_empty;
                      local_get(len); i32_const(2); i32_lt_u; br_if(0);
                    i32_const(0); local_set(i); // i (outer)
                    block_empty; loop_empty;
                      local_get(i); local_get(len); i32_const(1); i32_sub; i32_ge_u; br_if(1);
                      i32_const(0); local_set(j); // j (inner)
                      block_empty; loop_empty;
                        // j < len - 1 - i
                        local_get(len); i32_const(1); i32_sub; local_get(i); i32_sub;
                        local_get(j); i32_le_u; br_if(1);
                        // Compare keys[j] > keys[j+1]
                        local_get(keys);
                        local_get(j); i32_const(ks); i32_mul; i32_add;
                });
                // Load keys[j] and keys[j+1] at the storage width, then ask the
                // type-directed total-order emitter whether keys[j] > keys[j+1]
                // (a descending adjacent pair → swap). One code path for every
                // key type; `emit_ord_cmp3` returns sign(a <=> b).
                if key_is_str {
                    wasm!(self.func, {
                        i32_load(0);
                        local_get(keys);
                        local_get(j); i32_const(1); i32_add; i32_const(ks); i32_mul; i32_add;
                        i32_load(0);
                    });
                } else {
                    wasm!(self.func, {
                        i64_load(0);
                        local_get(keys);
                        local_get(j); i32_const(1); i32_add; i32_const(ks); i32_mul; i32_add;
                        i64_load(0);
                    });
                }
                if key_is_float {
                    // Keys are pre-transformed i64 totalOrder keys: a plain
                    // signed `keys[j] > keys[j+1]` IS the totalOrder swap test.
                    wasm!(self.func, { i64_gt_s; });
                } else {
                    self.emit_ord_cmp3(&cmp_key_ty);
                    wasm!(self.func, { i32_const(0); i32_gt_s; });
                }
                wasm!(self.func, {
                        if_empty;
                          // Swap keys[j] and keys[j+1]
                          local_get(keys);
                          local_get(j); i32_const(ks); i32_mul; i32_add;
                });
                if key_is_str {
                    wasm!(self.func, {
                          i32_load(0); local_set(tmp_key);
                          local_get(keys);
                          local_get(j); i32_const(ks); i32_mul; i32_add;
                          local_get(keys);
                          local_get(j); i32_const(1); i32_add; i32_const(ks); i32_mul; i32_add;
                          i32_load(0); i32_store(0);
                          local_get(keys);
                          local_get(j); i32_const(1); i32_add; i32_const(ks); i32_mul; i32_add;
                          local_get(tmp_key); i32_store(0);
                    });
                } else {
                    wasm!(self.func, {
                          i64_load(0); local_set(tmp_key);
                          local_get(keys);
                          local_get(j); i32_const(ks); i32_mul; i32_add;
                          local_get(keys);
                          local_get(j); i32_const(1); i32_add; i32_const(ks); i32_mul; i32_add;
                          i64_load(0); i64_store(0);
                          local_get(keys);
                          local_get(j); i32_const(1); i32_add; i32_const(ks); i32_mul; i32_add;
                          local_get(tmp_key); i64_store(0);
                    });
                }
                wasm!(self.func, {
                          // Swap dst[j] and dst[j+1] using typed scratch local
                          // tmp_elem = dst[j]
                          local_get(dst); i32_const(list_data_off); i32_add;
                          local_get(j); i32_const(es); i32_mul; i32_add;
                });
                self.emit_load_at(&elem_ty, 0);
                wasm!(self.func, {
                          local_set(tmp_elem);
                          // dst[j] = dst[j+1]
                          local_get(dst); i32_const(list_data_off); i32_add;
                          local_get(j); i32_const(es); i32_mul; i32_add;
                          local_get(dst); i32_const(list_data_off); i32_add;
                          local_get(j); i32_const(1); i32_add; i32_const(es); i32_mul; i32_add;
                });
                self.emit_elem_copy(&elem_ty);
                wasm!(self.func, {
                          // dst[j+1] = tmp_elem
                          local_get(dst); i32_const(list_data_off); i32_add;
                          local_get(j); i32_const(1); i32_add; i32_const(es); i32_mul; i32_add;
                          local_get(tmp_elem);
                });
                self.emit_store_at(&elem_ty, 0);
                wasm!(self.func, {
                        end; // end if (swap needed)
                        local_get(j); i32_const(1); i32_add; local_set(j);
                        br(0);
                      end; end; // end inner loop
                      local_get(i); i32_const(1); i32_add; local_set(i);
                      br(0);
                    end; end; // end outer loop
                    end; // end len<2 guard block
                    local_get(dst);
                });
                self.scratch.free(tmp_elem, elem_vt);
                if key_is_str { self.scratch.free_i32(tmp_key); } else { self.scratch.free_i64(tmp_key); }
                self.scratch.free_i32(j);
                self.scratch.free_i32(i);
                self.scratch.free_i32(keys);
                self.scratch.free_i32(dst);
                self.scratch.free_i32(len);
                self.scratch.free_i32(closure);
                self.scratch.free_i32(xs);
            }
            _ => return self.emit_list_closure_call2(method, args),
        }
        true
    }
}

// Group-2 sub-match (find, find_index, any, all, each, take_end, drop_end,
// repeat, reduce) — chained from emit_list_closure_call above.
include!("calls_list_closure_p2.rs");
