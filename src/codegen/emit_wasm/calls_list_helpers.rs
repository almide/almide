//! List stdlib helper methods for WASM codegen.
//!
//! Utility functions used by both calls_list.rs and calls_list_closure.rs:
//! list_elem_ty, emit_elem_copy, emit_elem_store, emit_list_slice_impl, emit_memcpy_loop.

use super::FuncCompiler;
use super::values;
use crate::ir::IrExpr;
use crate::types::Ty;
use wasm_encoder::ValType;

impl FuncCompiler<'_> {
    pub(super) fn list_elem_ty(&self, ty: &Ty) -> Ty {
        if let Ty::Applied(_, args) = ty {
            args.first().cloned().unwrap_or(Ty::Int)
        } else { Ty::Int }
    }

    /// Copy one element from [stack: dst_addr, src_addr] based on type.
    pub(super) fn emit_elem_copy(&mut self, ty: &Ty) {
        match values::ty_to_valtype(ty) {
            Some(ValType::I64) => { wasm!(self.func, { i64_load(0); i64_store(0); }); }
            Some(ValType::F64) => { wasm!(self.func, { f64_load(0); f64_store(0); }); }
            _ => { wasm!(self.func, { i32_load(0); i32_store(0); }); }
        }
    }

    /// Store one element: [stack: dst_addr, value].
    pub(super) fn emit_elem_store(&mut self, ty: &Ty) {
        match values::ty_to_valtype(ty) {
            Some(ValType::I64) => { wasm!(self.func, { i64_store(0); }); }
            Some(ValType::F64) => { wasm!(self.func, { f64_store(0); }); }
            _ => { wasm!(self.func, { i32_store(0); }); }
        }
    }

    /// Emit take/drop as list slice. For take: start=0,end=n. For drop: start=n,end=len.
    fn emit_list_slice_impl(
        &mut self, xs: &IrExpr, start_arg: Option<&IrExpr>, end_arg: Option<&IrExpr>,
        elem_size: usize, is_take: bool,
    ) {
        let s = self.match_i32_base + self.match_depth;
        wasm!(self.func, { i32_const(0); });
        self.emit_expr(xs);
        wasm!(self.func, { i32_store(0); }); // mem[0] = xs
        // Compute start and end
        if is_take {
            // take(xs, n): start=0, end=min(n, len)
            self.emit_expr(end_arg.unwrap());
            wasm!(self.func, {
                i32_wrap_i64; local_set(s); // n
                i32_const(0); local_set(s + 1); // start = 0
                // end = min(n, len)
                local_get(s); i32_const(0); i32_load(0); i32_load(0);
                i32_lt_u;
                if_i32; local_get(s); else_; i32_const(0); i32_load(0); i32_load(0); end;
                local_set(s + 2); // end
            });
        } else {
            // drop(xs, n): start=min(n, len), end=len
            self.emit_expr(start_arg.unwrap());
            wasm!(self.func, {
                i32_wrap_i64; local_set(s); // n
                // start = min(n, len)
                local_get(s); i32_const(0); i32_load(0); i32_load(0);
                i32_lt_u;
                if_i32; local_get(s); else_; i32_const(0); i32_load(0); i32_load(0); end;
                local_set(s + 1); // start
                i32_const(0); i32_load(0); i32_load(0); local_set(s + 2); // end = len
            });
        }
        // new_len = end - start
        wasm!(self.func, {
            local_get(s + 2); local_get(s + 1); i32_sub; local_set(s + 3);
            // alloc
            i32_const(4); local_get(s + 3); i32_const(elem_size as i32); i32_mul; i32_add;
            call(self.emitter.rt.alloc); local_set(s + 4);
            local_get(s + 4); local_get(s + 3); i32_store(0);
            // copy loop
            i32_const(0); local_set(s + 5); // i
            block_empty; loop_empty;
              local_get(s + 5); local_get(s + 3); i32_ge_u; br_if(1);
              // dst[4 + i*es]
              local_get(s + 4); i32_const(4); i32_add;
              local_get(s + 5); i32_const(elem_size as i32); i32_mul; i32_add;
              // src[4 + (start+i)*es]
              i32_const(0); i32_load(0); i32_const(4); i32_add;
              local_get(s + 1); local_get(s + 5); i32_add;
              i32_const(elem_size as i32); i32_mul; i32_add;
        });
        // Copy one element
        let elem_ty = if is_take {
            self.list_elem_ty(&end_arg.unwrap().ty)
        } else {
            self.list_elem_ty(&start_arg.unwrap().ty)
        };
        // Actually use xs type
        self.emit_elem_copy(&self.list_elem_ty(&xs.ty));
        wasm!(self.func, {
              local_get(s + 5); i32_const(1); i32_add; local_set(s + 5);
              br(0);
            end; end;
            local_get(s + 4);
        });
    }

    fn emit_memcpy_loop(&mut self, _i_local: u32, _dst_local: u32, _start_local: u32, _elem_size: usize) {
        // Generic memcpy for list.slice — complex, use inline for now
        // This is a placeholder; slice uses the same pattern as take/drop
    }

    /// Emit list.sort (insertion sort for List[Int]).
    pub(super) fn emit_list_sort(&mut self, args: &[IrExpr]) {
        let elem_ty = self.list_elem_ty(&args[0].ty);
        if !matches!(&elem_ty, Ty::Int) {
            self.emit_stub_call(args);
            return;
        }
        let s = self.match_i32_base + self.match_depth;
        let s64 = self.match_i64_base + self.match_depth;
        // Copy list first
        wasm!(self.func, { i32_const(0); });
        self.emit_expr(&args[0]);
        wasm!(self.func, {
            i32_store(0);
            i32_const(0); i32_load(0); i32_load(0); local_set(s);
            i32_const(4); local_get(s); i32_const(8); i32_mul; i32_add;
            call(self.emitter.rt.alloc); local_set(s + 1);
            local_get(s + 1); local_get(s); i32_store(0);
        });
        // Copy all elements
        wasm!(self.func, {
            i32_const(0); local_set(s + 2);
            block_empty; loop_empty;
              local_get(s + 2); local_get(s); i32_ge_u; br_if(1);
              local_get(s + 1); i32_const(4); i32_add;
              local_get(s + 2); i32_const(8); i32_mul; i32_add;
              i32_const(0); i32_load(0); i32_const(4); i32_add;
              local_get(s + 2); i32_const(8); i32_mul; i32_add;
              i64_load(0); i64_store(0);
              local_get(s + 2); i32_const(1); i32_add; local_set(s + 2);
              br(0);
            end; end;
        });
        // Insertion sort outer loop
        wasm!(self.func, {
            i32_const(1); local_set(s + 2);
            block_empty; loop_empty;
              local_get(s + 2); local_get(s); i32_ge_u; br_if(1);
              local_get(s + 1); i32_const(4); i32_add;
              local_get(s + 2); i32_const(8); i32_mul; i32_add;
              i64_load(0); local_set(s64);
              local_get(s + 2); i32_const(1); i32_sub; local_set(s + 3);
        });
        // Inner loop: shift elements right
        wasm!(self.func, {
              block_empty; loop_empty;
                local_get(s + 3); i32_const(0); i32_lt_s; br_if(1);
                local_get(s + 1); i32_const(4); i32_add;
                local_get(s + 3); i32_const(8); i32_mul; i32_add;
                i64_load(0); local_get(s64); i64_le_s; br_if(1);
                local_get(s + 1); i32_const(4); i32_add;
                local_get(s + 3); i32_const(1); i32_add; i32_const(8); i32_mul; i32_add;
                local_get(s + 1); i32_const(4); i32_add;
                local_get(s + 3); i32_const(8); i32_mul; i32_add;
                i64_load(0); i64_store(0);
                local_get(s + 3); i32_const(1); i32_sub; local_set(s + 3);
                br(0);
              end; end;
        });
        // Place key and continue
        wasm!(self.func, {
              local_get(s + 1); i32_const(4); i32_add;
              local_get(s + 3); i32_const(1); i32_add; i32_const(8); i32_mul; i32_add;
              local_get(s64); i64_store(0);
              local_get(s + 2); i32_const(1); i32_add; local_set(s + 2);
              br(0);
            end; end;
            local_get(s + 1);
        });
    }

    /// Emit list.index_of(xs, x) → Option[Int].
    pub(super) fn emit_list_index_of(&mut self, args: &[IrExpr]) {
        let elem_ty = self.list_elem_ty(&args[0].ty);
        let elem_size = values::byte_size(&elem_ty);
        let s = self.match_i32_base + self.match_depth;
        let s64 = self.match_i64_base + self.match_depth;
        wasm!(self.func, { i32_const(0); });
        self.emit_expr(&args[0]);
        wasm!(self.func, { i32_store(0); });
        // Store search value
        match values::ty_to_valtype(&elem_ty) {
            Some(ValType::I64) => {
                self.emit_expr(&args[1]);
                wasm!(self.func, { local_set(s64); });
            }
            _ => {
                wasm!(self.func, { i32_const(4); });
                self.emit_expr(&args[1]);
                wasm!(self.func, { i32_store(0); });
            }
        }
        wasm!(self.func, {
            i32_const(0); local_set(s); // i
            i32_const(0); local_set(s + 2); // result (default: none)
            block_empty; loop_empty;
              local_get(s);
              i32_const(0); i32_load(0); i32_load(0); // len
              i32_ge_u; br_if(1);
        });
        // Compare element
        match values::ty_to_valtype(&elem_ty) {
            Some(ValType::I64) => {
                wasm!(self.func, {
                    i32_const(0); i32_load(0); i32_const(4); i32_add;
                    local_get(s); i32_const(8); i32_mul; i32_add;
                    i64_load(0);
                    local_get(s64); i64_eq;
                    if_empty;
                      // Found: store some(i) and break
                      i32_const(8); call(self.emitter.rt.alloc); local_set(s + 1);
                      local_get(s + 1); local_get(s); i64_extend_i32_u; i64_store(0);
                      local_get(s + 1); local_set(s + 2); br(2);
                    end;
                });
            }
            _ => {
                wasm!(self.func, {
                    i32_const(0); i32_load(0); i32_const(4); i32_add;
                    local_get(s); i32_const(elem_size as i32); i32_mul; i32_add;
                    i32_load(0);
                    i32_const(4); i32_load(0);
                });
                // String eq or i32 eq
                if matches!(&elem_ty, Ty::String) {
                    wasm!(self.func, { call(self.emitter.rt.string.eq); });
                } else {
                    wasm!(self.func, { i32_eq; });
                }
                wasm!(self.func, {
                    if_empty;
                      i32_const(8); call(self.emitter.rt.alloc); local_set(s + 1);
                      local_get(s + 1); local_get(s); i64_extend_i32_u; i64_store(0);
                      local_get(s + 1); local_set(s + 2); br(2);
                    end;
                });
            }
        }
        wasm!(self.func, {
              local_get(s); i32_const(1); i32_add; local_set(s);
              br(0);
            end; end;
            local_get(s + 2); // result (none if not found)
        });
    }

    /// Emit list.unique(xs) → List[A]: O(n²) dedup.
    pub(super) fn emit_list_unique(&mut self, args: &[IrExpr]) {
        let elem_ty = self.list_elem_ty(&args[0].ty);
        let es = values::byte_size(&elem_ty) as i32;
        let s = self.match_i32_base + self.match_depth;
        self.emit_expr(&args[0]);
        wasm!(self.func, {
            local_set(s);
            local_get(s); i32_load(0); local_set(s + 1); // src_len
            i32_const(4); local_get(s + 1); i32_const(es); i32_mul; i32_add;
            call(self.emitter.rt.alloc); local_set(s + 2); // dst
            local_get(s + 2); i32_const(0); i32_store(0);
            i32_const(0); local_set(s + 3); // i
            block_empty; loop_empty;
              local_get(s + 3); local_get(s + 1); i32_ge_u; br_if(1);
              // Check if src[i] already in dst
              i32_const(0); local_set(s + 4); // j
              i32_const(0); local_set(s + 5); // found
              block_empty; loop_empty;
                local_get(s + 4); local_get(s + 2); i32_load(0); i32_ge_u; br_if(1);
                local_get(s); i32_const(4); i32_add;
                local_get(s + 3); i32_const(es); i32_mul; i32_add;
                i32_load(0);
                local_get(s + 2); i32_const(4); i32_add;
                local_get(s + 4); i32_const(es); i32_mul; i32_add;
                i32_load(0);
        });
        match &elem_ty {
            Ty::String => { wasm!(self.func, { call(self.emitter.rt.string.eq); }); }
            _ => { wasm!(self.func, { i32_eq; }); }
        }
        wasm!(self.func, {
                if_empty; i32_const(1); local_set(s + 5); br(2); end;
                local_get(s + 4); i32_const(1); i32_add; local_set(s + 4);
                br(0);
              end; end;
              local_get(s + 5); i32_eqz;
              if_empty;
                local_get(s + 2); i32_const(4); i32_add;
                local_get(s + 2); i32_load(0); i32_const(es); i32_mul; i32_add;
                local_get(s); i32_const(4); i32_add;
                local_get(s + 3); i32_const(es); i32_mul; i32_add;
        });
        self.emit_elem_copy(&elem_ty);
        wasm!(self.func, {
                local_get(s + 2);
                local_get(s + 2); i32_load(0); i32_const(1); i32_add;
                i32_store(0);
              end;
              local_get(s + 3); i32_const(1); i32_add; local_set(s + 3);
              br(0);
            end; end;
            local_get(s + 2);
        });
    }

    /// Emit list.enumerate(xs) → List[(Int, A)].
    pub(super) fn emit_list_enumerate(&mut self, args: &[IrExpr]) {
        let elem_ty = if let Ty::Applied(_, a) = &args[0].ty {
            a.first().cloned().unwrap_or(Ty::Int)
        } else { Ty::Int };
        let elem_size = values::byte_size(&elem_ty);
        let tuple_size = 8 + elem_size; // Int(8) + elem

        let s = self.match_i32_base + self.match_depth;
        let len_local = s;
        let idx_local = s + 1;

        // Store src → mem[0]
        wasm!(self.func, { i32_const(0); });
        self.emit_expr(&args[0]);
        wasm!(self.func, {
            i32_store(0);
            // len
            i32_const(0);
            i32_load(0);
            i32_load(0);
            local_set(len_local);
            // Alloc dst: [len] + len * ptr_size(4)
            i32_const(8);
            i32_const(4);
            local_get(len_local);
            i32_const(4); // each entry is a tuple ptr (i32)
            i32_mul;
            i32_add;
            call(self.emitter.rt.alloc);
            i32_store(0);
            // dst = mem[8]
            // Store len in dst
            i32_const(8);
            i32_load(0);
            local_get(len_local);
            i32_store(0);
            // Loop: create tuples
            i32_const(0);
            local_set(idx_local);
            block_empty;
            loop_empty;
        });
        let saved = self.depth;
        self.depth += 2;

        wasm!(self.func, {
            local_get(idx_local);
            local_get(len_local);
            i32_ge_u;
            br_if(1);
            // Alloc tuple: [index:i64][element]
            i32_const(tuple_size as i32);
            call(self.emitter.rt.alloc);
            drop;
            // Re-alloc tuple
            i32_const(12);
            i32_const(tuple_size as i32);
            call(self.emitter.rt.alloc);
            i32_store(0); // mem[12] = tuple_ptr
            // tuple.index = idx (as i64)
            i32_const(12);
            i32_load(0); // tuple_ptr
            local_get(idx_local);
            i64_extend_i32_u;
            i64_store(0);
            // tuple.element = src[idx]
            i32_const(12);
            i32_load(0); // tuple_ptr
            // Load src element
            i32_const(0);
            i32_load(0); // src_ptr
            i32_const(4);
            i32_add;
            local_get(idx_local);
            i32_const(elem_size as i32);
            i32_mul;
            i32_add;
        });
        self.emit_load_at(&elem_ty, 0);
        self.emit_store_at(&elem_ty, 8); // store at tuple offset 8

        wasm!(self.func, {
            // dst[idx] = tuple_ptr
            i32_const(8);
            i32_load(0); // dst_ptr
            i32_const(4);
            i32_add;
            local_get(idx_local);
            i32_const(4); // tuple ptrs are i32
            i32_mul;
            i32_add;
            i32_const(12);
            i32_load(0); // tuple_ptr
            i32_store(0);
            // idx++
            local_get(idx_local);
            i32_const(1);
            i32_add;
            local_set(idx_local);
            br(0);
        });

        self.depth = saved;
        wasm!(self.func, {
            end;
            end;
            // Return dst
            i32_const(8);
            i32_load(0);
        });
    }
}
