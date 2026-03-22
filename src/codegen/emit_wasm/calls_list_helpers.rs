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
        let xs_ptr = self.scratch.alloc_i32();
        let n = self.scratch.alloc_i32();
        let start = self.scratch.alloc_i32();
        let end = self.scratch.alloc_i32();
        let new_len = self.scratch.alloc_i32();
        let dst = self.scratch.alloc_i32();
        let i = self.scratch.alloc_i32();

        self.emit_expr(xs);
        wasm!(self.func, { local_set(xs_ptr); }); // xs_ptr = xs
        // Compute start and end
        if is_take {
            // take(xs, n): start=0, end=min(n, len)
            self.emit_expr(end_arg.unwrap());
            wasm!(self.func, {
                i32_wrap_i64; local_set(n); // n
                i32_const(0); local_set(start); // start = 0
                // end = min(n, len)
                local_get(n); local_get(xs_ptr); i32_load(0);
                i32_lt_u;
                if_i32; local_get(n); else_; local_get(xs_ptr); i32_load(0); end;
                local_set(end); // end
            });
        } else {
            // drop(xs, n): start=min(n, len), end=len
            self.emit_expr(start_arg.unwrap());
            wasm!(self.func, {
                i32_wrap_i64; local_set(n); // n
                // start = min(n, len)
                local_get(n); local_get(xs_ptr); i32_load(0);
                i32_lt_u;
                if_i32; local_get(n); else_; local_get(xs_ptr); i32_load(0); end;
                local_set(start); // start
                local_get(xs_ptr); i32_load(0); local_set(end); // end = len
            });
        }
        // new_len = end - start
        wasm!(self.func, {
            local_get(end); local_get(start); i32_sub; local_set(new_len);
            // alloc
            i32_const(4); local_get(new_len); i32_const(elem_size as i32); i32_mul; i32_add;
            call(self.emitter.rt.alloc); local_set(dst);
            local_get(dst); local_get(new_len); i32_store(0);
            // copy loop
            i32_const(0); local_set(i); // i
            block_empty; loop_empty;
              local_get(i); local_get(new_len); i32_ge_u; br_if(1);
              // dst[4 + i*es]
              local_get(dst); i32_const(4); i32_add;
              local_get(i); i32_const(elem_size as i32); i32_mul; i32_add;
              // src[4 + (start+i)*es]
              local_get(xs_ptr); i32_const(4); i32_add;
              local_get(start); local_get(i); i32_add;
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
              local_get(i); i32_const(1); i32_add; local_set(i);
              br(0);
            end; end;
            local_get(dst);
        });

        self.scratch.free_i32(i);
        self.scratch.free_i32(dst);
        self.scratch.free_i32(new_len);
        self.scratch.free_i32(end);
        self.scratch.free_i32(start);
        self.scratch.free_i32(n);
        self.scratch.free_i32(xs_ptr);
    }

    fn emit_memcpy_loop(&mut self, _i_local: u32, _dst_local: u32, _start_local: u32, _elem_size: usize) {
        // Generic memcpy for list.slice — complex, use inline for now
        // This is a placeholder; slice uses the same pattern as take/drop
    }

    /// Emit list.sort (insertion sort for List[Int] and List[String]).
    pub(super) fn emit_list_sort(&mut self, args: &[IrExpr]) {
        let elem_ty = self.list_elem_ty(&args[0].ty);
        match &elem_ty {
            Ty::Int => self.emit_list_sort_int(args),
            Ty::String => self.emit_list_sort_string(args),
            _ => self.emit_stub_call(args),
        }
    }

    /// Insertion sort for List[Int] (elements are i64, 8 bytes each).
    fn emit_list_sort_int(&mut self, args: &[IrExpr]) {
        let xs_ptr = self.scratch.alloc_i32();
        let len = self.scratch.alloc_i32();
        let dst = self.scratch.alloc_i32();
        let i = self.scratch.alloc_i32();
        let j = self.scratch.alloc_i32();
        let key = self.scratch.alloc_i64();

        // Copy list first
        self.emit_expr(&args[0]);
        wasm!(self.func, {
            local_set(xs_ptr);
            local_get(xs_ptr); i32_load(0); local_set(len);
            i32_const(4); local_get(len); i32_const(8); i32_mul; i32_add;
            call(self.emitter.rt.alloc); local_set(dst);
            local_get(dst); local_get(len); i32_store(0);
        });
        // Copy all elements
        wasm!(self.func, {
            i32_const(0); local_set(i);
            block_empty; loop_empty;
              local_get(i); local_get(len); i32_ge_u; br_if(1);
              local_get(dst); i32_const(4); i32_add;
              local_get(i); i32_const(8); i32_mul; i32_add;
              local_get(xs_ptr); i32_const(4); i32_add;
              local_get(i); i32_const(8); i32_mul; i32_add;
              i64_load(0); i64_store(0);
              local_get(i); i32_const(1); i32_add; local_set(i);
              br(0);
            end; end;
        });
        // Insertion sort outer loop
        wasm!(self.func, {
            i32_const(1); local_set(i);
            block_empty; loop_empty;
              local_get(i); local_get(len); i32_ge_u; br_if(1);
              local_get(dst); i32_const(4); i32_add;
              local_get(i); i32_const(8); i32_mul; i32_add;
              i64_load(0); local_set(key);
              local_get(i); i32_const(1); i32_sub; local_set(j);
        });
        // Inner loop: shift elements right
        wasm!(self.func, {
              block_empty; loop_empty;
                local_get(j); i32_const(0); i32_lt_s; br_if(1);
                local_get(dst); i32_const(4); i32_add;
                local_get(j); i32_const(8); i32_mul; i32_add;
                i64_load(0); local_get(key); i64_le_s; br_if(1);
                local_get(dst); i32_const(4); i32_add;
                local_get(j); i32_const(1); i32_add; i32_const(8); i32_mul; i32_add;
                local_get(dst); i32_const(4); i32_add;
                local_get(j); i32_const(8); i32_mul; i32_add;
                i64_load(0); i64_store(0);
                local_get(j); i32_const(1); i32_sub; local_set(j);
                br(0);
              end; end;
        });
        // Place key and continue
        wasm!(self.func, {
              local_get(dst); i32_const(4); i32_add;
              local_get(j); i32_const(1); i32_add; i32_const(8); i32_mul; i32_add;
              local_get(key); i64_store(0);
              local_get(i); i32_const(1); i32_add; local_set(i);
              br(0);
            end; end;
            local_get(dst);
        });

        self.scratch.free_i64(key);
        self.scratch.free_i32(j);
        self.scratch.free_i32(i);
        self.scratch.free_i32(dst);
        self.scratch.free_i32(len);
        self.scratch.free_i32(xs_ptr);
    }

    /// Insertion sort for List[String] (elements are i32 pointers, 4 bytes each).
    /// Comparison uses __str_cmp which returns negative/0/positive.
    fn emit_list_sort_string(&mut self, args: &[IrExpr]) {
        let xs_ptr = self.scratch.alloc_i32();
        let len = self.scratch.alloc_i32();
        let dst = self.scratch.alloc_i32();
        let i = self.scratch.alloc_i32();
        let j = self.scratch.alloc_i32();
        let str_key = self.scratch.alloc_i32();

        // Copy list first
        self.emit_expr(&args[0]);
        wasm!(self.func, {
            local_set(xs_ptr);
            local_get(xs_ptr); i32_load(0); local_set(len); // len
            i32_const(4); local_get(len); i32_const(4); i32_mul; i32_add;
            call(self.emitter.rt.alloc); local_set(dst); // dst
            local_get(dst); local_get(len); i32_store(0);
        });
        // Copy all elements (i32 pointers)
        wasm!(self.func, {
            i32_const(0); local_set(i);
            block_empty; loop_empty;
              local_get(i); local_get(len); i32_ge_u; br_if(1);
              local_get(dst); i32_const(4); i32_add;
              local_get(i); i32_const(4); i32_mul; i32_add;
              local_get(xs_ptr); i32_const(4); i32_add;
              local_get(i); i32_const(4); i32_mul; i32_add;
              i32_load(0); i32_store(0);
              local_get(i); i32_const(1); i32_add; local_set(i);
              br(0);
            end; end;
        });
        // Insertion sort outer loop
        wasm!(self.func, {
            i32_const(1); local_set(i); // i = 1
            block_empty; loop_empty;
              local_get(i); local_get(len); i32_ge_u; br_if(1);
              // key = dst[4 + i*4]
              local_get(dst); i32_const(4); i32_add;
              local_get(i); i32_const(4); i32_mul; i32_add;
              i32_load(0); local_set(str_key); // key
              local_get(i); i32_const(1); i32_sub; local_set(j); // j = i - 1
        });
        // Inner loop: shift elements right while dst[j] > key
        wasm!(self.func, {
              block_empty; loop_empty;
                local_get(j); i32_const(0); i32_lt_s; br_if(1);
                // Compare: str_cmp(dst[j], key) <= 0 means stop
                local_get(dst); i32_const(4); i32_add;
                local_get(j); i32_const(4); i32_mul; i32_add;
                i32_load(0); // dst[j]
                local_get(str_key); // key
                call(self.emitter.rt.string.cmp);
                i32_const(0); i32_le_s; br_if(1); // if dst[j] <= key, stop
                // Shift: dst[j+1] = dst[j]
                local_get(dst); i32_const(4); i32_add;
                local_get(j); i32_const(1); i32_add; i32_const(4); i32_mul; i32_add;
                local_get(dst); i32_const(4); i32_add;
                local_get(j); i32_const(4); i32_mul; i32_add;
                i32_load(0); i32_store(0);
                local_get(j); i32_const(1); i32_sub; local_set(j);
                br(0);
              end; end;
        });
        // Place key at dst[j+1]
        wasm!(self.func, {
              local_get(dst); i32_const(4); i32_add;
              local_get(j); i32_const(1); i32_add; i32_const(4); i32_mul; i32_add;
              local_get(str_key); i32_store(0);
              local_get(i); i32_const(1); i32_add; local_set(i);
              br(0);
            end; end;
            local_get(dst);
        });

        self.scratch.free_i32(str_key);
        self.scratch.free_i32(j);
        self.scratch.free_i32(i);
        self.scratch.free_i32(dst);
        self.scratch.free_i32(len);
        self.scratch.free_i32(xs_ptr);
    }

    /// Emit list.index_of(xs, x) → Option[Int].
    pub(super) fn emit_list_index_of(&mut self, args: &[IrExpr]) {
        let elem_ty = self.list_elem_ty(&args[0].ty);
        let elem_size = values::byte_size(&elem_ty);
        let xs_ptr = self.scratch.alloc_i32();
        let i = self.scratch.alloc_i32();
        let found_ptr = self.scratch.alloc_i32();
        let result = self.scratch.alloc_i32();
        let search_val_i64 = self.scratch.alloc_i64();
        let search_val_i32 = self.scratch.alloc_i32();

        self.emit_expr(&args[0]);
        wasm!(self.func, { local_set(xs_ptr); });
        // Store search value
        match values::ty_to_valtype(&elem_ty) {
            Some(ValType::I64) => {
                self.emit_expr(&args[1]);
                wasm!(self.func, { local_set(search_val_i64); });
            }
            _ => {
                self.emit_expr(&args[1]);
                wasm!(self.func, { local_set(search_val_i32); });
            }
        }
        wasm!(self.func, {
            i32_const(0); local_set(i); // i
            i32_const(0); local_set(result); // result (default: none)
            block_empty; loop_empty;
              local_get(i);
              local_get(xs_ptr); i32_load(0); // len
              i32_ge_u; br_if(1);
        });
        // Compare element
        match values::ty_to_valtype(&elem_ty) {
            Some(ValType::I64) => {
                wasm!(self.func, {
                    local_get(xs_ptr); i32_const(4); i32_add;
                    local_get(i); i32_const(8); i32_mul; i32_add;
                    i64_load(0);
                    local_get(search_val_i64); i64_eq;
                    if_empty;
                      // Found: store some(i) and break
                      i32_const(8); call(self.emitter.rt.alloc); local_set(found_ptr);
                      local_get(found_ptr); local_get(i); i64_extend_i32_u; i64_store(0);
                      local_get(found_ptr); local_set(result); br(2);
                    end;
                });
            }
            _ => {
                wasm!(self.func, {
                    local_get(xs_ptr); i32_const(4); i32_add;
                    local_get(i); i32_const(elem_size as i32); i32_mul; i32_add;
                    i32_load(0);
                    local_get(search_val_i32);
                });
                // String eq or i32 eq
                if matches!(&elem_ty, Ty::String) {
                    wasm!(self.func, { call(self.emitter.rt.string.eq); });
                } else {
                    wasm!(self.func, { i32_eq; });
                }
                wasm!(self.func, {
                    if_empty;
                      i32_const(8); call(self.emitter.rt.alloc); local_set(found_ptr);
                      local_get(found_ptr); local_get(i); i64_extend_i32_u; i64_store(0);
                      local_get(found_ptr); local_set(result); br(2);
                    end;
                });
            }
        }
        wasm!(self.func, {
              local_get(i); i32_const(1); i32_add; local_set(i);
              br(0);
            end; end;
            local_get(result); // result (none if not found)
        });

        self.scratch.free_i32(search_val_i32);
        self.scratch.free_i64(search_val_i64);
        self.scratch.free_i32(result);
        self.scratch.free_i32(found_ptr);
        self.scratch.free_i32(i);
        self.scratch.free_i32(xs_ptr);
    }

    /// Emit list.unique(xs) → List[A]: O(n²) dedup.
    pub(super) fn emit_list_unique(&mut self, args: &[IrExpr]) {
        let elem_ty = self.list_elem_ty(&args[0].ty);
        let es = values::byte_size(&elem_ty) as i32;
        let src = self.scratch.alloc_i32();
        let src_len = self.scratch.alloc_i32();
        let dst = self.scratch.alloc_i32();
        let i = self.scratch.alloc_i32();
        let j = self.scratch.alloc_i32();
        let found = self.scratch.alloc_i32();

        self.emit_expr(&args[0]);
        wasm!(self.func, {
            local_set(src);
            local_get(src); i32_load(0); local_set(src_len); // src_len
            i32_const(4); local_get(src_len); i32_const(es); i32_mul; i32_add;
            call(self.emitter.rt.alloc); local_set(dst); // dst
            local_get(dst); i32_const(0); i32_store(0);
            i32_const(0); local_set(i); // i
            block_empty; loop_empty;
              local_get(i); local_get(src_len); i32_ge_u; br_if(1);
              // Check if src[i] already in dst
              i32_const(0); local_set(j); // j
              i32_const(0); local_set(found); // found
              block_empty; loop_empty;
                local_get(j); local_get(dst); i32_load(0); i32_ge_u; br_if(1);
                local_get(src); i32_const(4); i32_add;
                local_get(i); i32_const(es); i32_mul; i32_add;
                i32_load(0);
                local_get(dst); i32_const(4); i32_add;
                local_get(j); i32_const(es); i32_mul; i32_add;
                i32_load(0);
        });
        match &elem_ty {
            Ty::String => { wasm!(self.func, { call(self.emitter.rt.string.eq); }); }
            _ => { wasm!(self.func, { i32_eq; }); }
        }
        wasm!(self.func, {
                if_empty; i32_const(1); local_set(found); br(2); end;
                local_get(j); i32_const(1); i32_add; local_set(j);
                br(0);
              end; end;
              local_get(found); i32_eqz;
              if_empty;
                local_get(dst); i32_const(4); i32_add;
                local_get(dst); i32_load(0); i32_const(es); i32_mul; i32_add;
                local_get(src); i32_const(4); i32_add;
                local_get(i); i32_const(es); i32_mul; i32_add;
        });
        self.emit_elem_copy(&elem_ty);
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

        self.scratch.free_i32(found);
        self.scratch.free_i32(j);
        self.scratch.free_i32(i);
        self.scratch.free_i32(dst);
        self.scratch.free_i32(src_len);
        self.scratch.free_i32(src);
    }

    /// Emit list.enumerate(xs) → List[(Int, A)].
    pub(super) fn emit_list_enumerate(&mut self, args: &[IrExpr]) {
        let elem_ty = if let Ty::Applied(_, a) = &args[0].ty {
            a.first().cloned().unwrap_or(Ty::Int)
        } else { Ty::Int };
        let elem_size = values::byte_size(&elem_ty);
        let tuple_size = 8 + elem_size; // Int(8) + elem

        let src_ptr = self.scratch.alloc_i32();
        let len_local = self.scratch.alloc_i32();
        let idx_local = self.scratch.alloc_i32();
        let dst_ptr = self.scratch.alloc_i32();
        let tuple_ptr = self.scratch.alloc_i32();

        // Store src
        self.emit_expr(&args[0]);
        wasm!(self.func, {
            local_set(src_ptr);
            // len
            local_get(src_ptr);
            i32_load(0);
            local_set(len_local);
            // Alloc dst: [len] + len * ptr_size(4)
            i32_const(4);
            local_get(len_local);
            i32_const(4); // each entry is a tuple ptr (i32)
            i32_mul;
            i32_add;
            call(self.emitter.rt.alloc);
            local_set(dst_ptr);
            // Store len in dst
            local_get(dst_ptr);
            local_get(len_local);
            i32_store(0);
            // Loop: create tuples
            i32_const(0);
            local_set(idx_local);
            block_empty;
            loop_empty;
        });
        let depth_guard = self.depth_push_n(2);

        wasm!(self.func, {
            local_get(idx_local);
            local_get(len_local);
            i32_ge_u;
            br_if(1);
            // Alloc tuple: [index:i64][element]
            i32_const(tuple_size as i32);
            call(self.emitter.rt.alloc);
            local_set(tuple_ptr); // tuple_ptr
            // tuple.index = idx (as i64)
            local_get(tuple_ptr);
            local_get(idx_local);
            i64_extend_i32_u;
            i64_store(0);
            // tuple.element = src[idx]
            local_get(tuple_ptr);
            // Load src element
            local_get(src_ptr);
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
            local_get(dst_ptr);
            i32_const(4);
            i32_add;
            local_get(idx_local);
            i32_const(4); // tuple ptrs are i32
            i32_mul;
            i32_add;
            local_get(tuple_ptr);
            i32_store(0);
            // idx++
            local_get(idx_local);
            i32_const(1);
            i32_add;
            local_set(idx_local);
            br(0);
        });

        self.depth_pop(depth_guard);
        wasm!(self.func, {
            end;
            end;
            // Return dst
            local_get(dst_ptr);
        });

        self.scratch.free_i32(tuple_ptr);
        self.scratch.free_i32(dst_ptr);
        self.scratch.free_i32(idx_local);
        self.scratch.free_i32(len_local);
        self.scratch.free_i32(src_ptr);
    }
}
