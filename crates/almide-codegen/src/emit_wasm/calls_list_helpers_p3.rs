impl FuncCompiler<'_> {
    /// Emit list.index_of(xs, x) → Option[Int].
    pub(super) fn emit_list_index_of(&mut self, args: &[IrExpr]) {
        let elem_ty = self.resolve_list_elem(&args[0], None);
        let elem_size = values::byte_size(&elem_ty);
        let search_vt = values::ty_to_valtype(&elem_ty).unwrap_or(ValType::I32);
        let xs_ptr = self.scratch.alloc_i32();
        let i = self.scratch.alloc_i32();
        let found_ptr = self.scratch.alloc_i32();
        let result = self.scratch.alloc_i32();
        // Hold the search value in a valtype-matched register so the per-element
        // comparison loads and compares at the correct width (i64 for Int, f64 for
        // Float, i32 pointer for String/compound). The element load below uses
        // `emit_load_at(elem_ty)` and the compare uses `emit_eq_typed(elem_ty)`,
        // so both sides agree on width and on STRUCTURAL (deep) equality — matching
        // native `position(|v| *v == x)`, not pointer identity.
        let search_val = self.scratch.alloc(search_vt);

        self.emit_expr(&args[0]);
        wasm!(self.func, { local_set(xs_ptr); });
        self.emit_expr(&args[1]);
        wasm!(self.func, { local_set(search_val); });
        wasm!(self.func, {
            i32_const(0); local_set(i); // i
            i32_const(0); local_set(result); // result (default: none)
            block_empty; loop_empty;
              local_get(i);
              local_get(xs_ptr); i32_load(0); // len
              i32_ge_u; br_if(1);
              local_get(xs_ptr); i32_const(self.emitter.layout_reg.fixed_offset(LIST, ll::DATA) as i32); i32_add;
              local_get(i); i32_const(elem_size as i32); i32_mul; i32_add;
        });
        self.emit_load_at(&elem_ty, 0);
        wasm!(self.func, { local_get(search_val); });
        self.emit_eq_typed(&elem_ty);
        wasm!(self.func, {
              if_empty;
                // Found: store some(i) and break
                i32_const(self.emitter.layout_reg.header_size(LIST) as i32); call(self.emitter.rt.alloc); local_set(found_ptr);
                local_get(found_ptr); local_get(i); i64_extend_i32_u; i64_store(0);
                local_get(found_ptr); local_set(result); br(2);
              end;
              local_get(i); i32_const(1); i32_add; local_set(i);
              br(0);
            end; end;
            local_get(result); // result (none if not found)
        });

        self.scratch.free(search_val, search_vt);
        self.scratch.free_i32(result);
        self.scratch.free_i32(found_ptr);
        self.scratch.free_i32(i);
        self.scratch.free_i32(xs_ptr);
    }

    /// Emit list.unique(xs) → List[A]: O(n²) dedup.
    pub(super) fn emit_list_unique(&mut self, args: &[IrExpr]) {
        let elem_ty = self.resolve_list_elem(&args[0], None);
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
            i32_const(self.emitter.layout_reg.header_size(LIST) as i32); local_get(src_len); i32_const(es); i32_mul; i32_add;
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
                local_get(src); i32_const(self.emitter.layout_reg.fixed_offset(LIST, ll::DATA) as i32); i32_add;
                local_get(i); i32_const(es); i32_mul; i32_add;
        });
        self.emit_load_at(&elem_ty, 0);
        wasm!(self.func, {
                local_get(dst); i32_const(self.emitter.layout_reg.fixed_offset(LIST, ll::DATA) as i32); i32_add;
                local_get(j); i32_const(es); i32_mul; i32_add;
        });
        self.emit_load_at(&elem_ty, 0);
        // Structural eq: collapse all value-equal elements (String + compound),
        // matching native unique-by-`==`, not by pointer identity.
        self.emit_eq_typed(&elem_ty);
        wasm!(self.func, {
                if_empty; i32_const(1); local_set(found); br(2); end;
                local_get(j); i32_const(1); i32_add; local_set(j);
                br(0);
              end; end;
              local_get(found); i32_eqz;
              if_empty;
                local_get(dst); i32_const(self.emitter.layout_reg.fixed_offset(LIST, ll::DATA) as i32); i32_add;
                local_get(dst); i32_load(0); i32_const(es); i32_mul; i32_add;
                local_get(src); i32_const(self.emitter.layout_reg.fixed_offset(LIST, ll::DATA) as i32); i32_add;
                local_get(i); i32_const(es); i32_mul; i32_add;
        });
        // SHARE: a unique element copied from the borrowed source list into the fresh
        // result — dup it so the result owns its reference (else the source's
        // scope-end Dec deep-frees the element the result now holds).
        self.emit_elem_copy_owned(&elem_ty);
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
        let elem_ty = self.resolve_list_elem(&args[0], None);
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
            i32_const(self.emitter.layout_reg.header_size(LIST) as i32);
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
            i32_const(self.emitter.layout_reg.fixed_offset(LIST, ll::DATA) as i32);
            i32_add;
            local_get(idx_local);
            i32_const(elem_size as i32);
            i32_mul;
            i32_add;
        });
        self.emit_load_at(&elem_ty, 0);
        // SHARE: the fresh tuple holds a second reference to the element.
        if crate::pass_perceus::is_heap_type(&elem_ty) {
            wasm!(self.func, { call(self.emitter.rt.rc_inc); });
        }
        self.emit_store_at(&elem_ty, 8); // store at tuple offset 8

        wasm!(self.func, {
            // dst[idx] = tuple_ptr
            local_get(dst_ptr);
            i32_const(self.emitter.layout_reg.fixed_offset(LIST, ll::DATA) as i32);
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
