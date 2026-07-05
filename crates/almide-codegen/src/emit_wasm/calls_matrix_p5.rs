impl FuncCompiler<'_> {
    /// Matrix dispatch group 5 (chained from `emit_matrix_call`).
    /// Disjoint arm subset; returns true iff this group handled `method`.
    pub(super) fn emit_matrix_call_g5(&mut self, method: &str, args: &[IrExpr]) -> bool {
        match method {
            "split_cols_even" => {
                // matrix.split_cols_even(m, n) → List[Matrix]
                // Slice m columns into n equal chunks.
                let m = self.scratch.alloc_i32();
                let n = self.scratch.alloc_i32();
                let rows = self.scratch.alloc_i32();
                let cols = self.scratch.alloc_i32();
                let chunk = self.scratch.alloc_i32();
                let list_ptr = self.scratch.alloc_i32();
                let h = self.scratch.alloc_i32();
                let r = self.scratch.alloc_i32();
                let c = self.scratch.alloc_i32();
                let sub = self.scratch.alloc_i32();
                self.emit_expr(&args[0]);
                wasm!(self.func, { local_set(m); });
                self.emit_expr(&args[1]);
                wasm!(self.func, {
                    i32_wrap_i64; local_set(n);
                    local_get(m); i32_load(0); local_set(rows);
                    local_get(m); i32_load(4); local_set(cols);
                    local_get(cols); local_get(n); i32_div_u; local_set(chunk);
                    // Alloc list: 4 + n*4
                    local_get(n); i32_const(4); i32_mul; i32_const(self.emitter.layout_reg.fixed_offset(super::engine::layout::LIST, super::engine::layout::list::DATA) as i32); i32_add;
                    call(self.emitter.rt.alloc); local_set(list_ptr);
                    local_get(list_ptr); local_get(n); i32_store(0);
                    i32_const(0); local_set(h);
                    block_empty; loop_empty;
                      local_get(h); local_get(n); i32_ge_u; br_if(1);
                      // Alloc sub-matrix (rows, chunk)
                      local_get(rows); local_get(chunk); i32_mul; i32_const(8); i32_mul;
                      i32_const(self.emitter.layout_reg.fixed_offset(super::engine::layout::LIST, super::engine::layout::list::DATA) as i32); i32_add;
                      call(self.emitter.rt.alloc); local_set(sub);
                      local_get(sub); local_get(rows); i32_store(0);
                      local_get(sub); local_get(chunk); i32_store(4);
                      // Copy rows: sub[r][c] = m[r][h*chunk + c]
                      i32_const(0); local_set(r);
                      block_empty; loop_empty;
                        local_get(r); local_get(rows); i32_ge_u; br_if(1);
                        i32_const(0); local_set(c);
                        block_empty; loop_empty;
                          local_get(c); local_get(chunk); i32_ge_u; br_if(1);
                          local_get(sub); i32_const(self.emitter.layout_reg.fixed_offset(super::engine::layout::LIST, super::engine::layout::list::DATA) as i32); i32_add;
                          local_get(r); local_get(chunk); i32_mul; local_get(c); i32_add;
                          i32_const(8); i32_mul; i32_add;
                          local_get(m); i32_const(self.emitter.layout_reg.fixed_offset(super::engine::layout::LIST, super::engine::layout::list::DATA) as i32); i32_add;
                          local_get(r); local_get(cols); i32_mul;
                          local_get(h); local_get(chunk); i32_mul; i32_add;
                          local_get(c); i32_add;
                          i32_const(8); i32_mul; i32_add;
                          f64_load(0); f64_store(0);
                          local_get(c); i32_const(1); i32_add; local_set(c);
                          br(0);
                        end; end;
                        local_get(r); i32_const(1); i32_add; local_set(r);
                        br(0);
                      end; end;
                      // list[h] = sub
                      local_get(list_ptr); i32_const(self.emitter.layout_reg.fixed_offset(super::engine::layout::LIST, super::engine::layout::list::DATA) as i32); i32_add;
                      local_get(h); i32_const(4); i32_mul; i32_add;
                      local_get(sub); i32_store(0);
                      local_get(h); i32_const(1); i32_add; local_set(h);
                      br(0);
                    end; end;
                    local_get(list_ptr);
                });
                self.scratch.free_i32(sub);
                self.scratch.free_i32(c);
                self.scratch.free_i32(r);
                self.scratch.free_i32(h);
                self.scratch.free_i32(list_ptr);
                self.scratch.free_i32(chunk);
                self.scratch.free_i32(cols);
                self.scratch.free_i32(rows);
                self.scratch.free_i32(n);
                self.scratch.free_i32(m);
            }
            "concat_cols" | "concat_cols_many" => {
                // matrix.concat_cols(matrices: List[Matrix]) → Matrix
                // (concat_cols_many is the deprecated original name.)
                // All must have same rows; result has sum of cols.
                let lst = self.scratch.alloc_i32();
                let n = self.scratch.alloc_i32();
                let dst = self.scratch.alloc_i32();
                let first = self.scratch.alloc_i32();
                let rows = self.scratch.alloc_i32();
                let total_cols = self.scratch.alloc_i32();
                let i = self.scratch.alloc_i32();
                let sub = self.scratch.alloc_i32();
                let sub_cols = self.scratch.alloc_i32();
                let col_off = self.scratch.alloc_i32();
                let r = self.scratch.alloc_i32();
                let c = self.scratch.alloc_i32();
                self.emit_expr(&args[0]);
                wasm!(self.func, {
                    local_set(lst);
                    local_get(lst); i32_load(0); local_set(n);
                    // first = lst[0]
                    local_get(lst); i32_const(self.emitter.layout_reg.fixed_offset(super::engine::layout::LIST, super::engine::layout::list::DATA) as i32); i32_add; i32_load(0); local_set(first);
                    local_get(first); i32_load(0); local_set(rows);
                    // Sum total_cols
                    i32_const(0); local_set(total_cols);
                    i32_const(0); local_set(i);
                    block_empty; loop_empty;
                      local_get(i); local_get(n); i32_ge_u; br_if(1);
                      local_get(lst); i32_const(self.emitter.layout_reg.fixed_offset(super::engine::layout::LIST, super::engine::layout::list::DATA) as i32); i32_add;
                      local_get(i); i32_const(4); i32_mul; i32_add;
                      i32_load(0); local_set(sub);
                      local_get(total_cols); local_get(sub); i32_load(4); i32_add;
                      local_set(total_cols);
                      local_get(i); i32_const(1); i32_add; local_set(i);
                      br(0);
                    end; end;
                    // Alloc dst (rows, total_cols)
                    local_get(rows); local_get(total_cols); i32_mul; i32_const(8); i32_mul;
                    i32_const(self.emitter.layout_reg.fixed_offset(super::engine::layout::LIST, super::engine::layout::list::DATA) as i32); i32_add;
                    call(self.emitter.rt.alloc); local_set(dst);
                    local_get(dst); local_get(rows); i32_store(0);
                    local_get(dst); local_get(total_cols); i32_store(4);
                    // Fill: for each submatrix, copy its rows into dst at col_off
                    i32_const(0); local_set(col_off);
                    i32_const(0); local_set(i);
                    block_empty; loop_empty;
                      local_get(i); local_get(n); i32_ge_u; br_if(1);
                      local_get(lst); i32_const(self.emitter.layout_reg.fixed_offset(super::engine::layout::LIST, super::engine::layout::list::DATA) as i32); i32_add;
                      local_get(i); i32_const(4); i32_mul; i32_add;
                      i32_load(0); local_set(sub);
                      local_get(sub); i32_load(4); local_set(sub_cols);
                      i32_const(0); local_set(r);
                      block_empty; loop_empty;
                        local_get(r); local_get(rows); i32_ge_u; br_if(1);
                        i32_const(0); local_set(c);
                        block_empty; loop_empty;
                          local_get(c); local_get(sub_cols); i32_ge_u; br_if(1);
                          // dst[r, col_off + c] = sub[r, c]
                          local_get(dst); i32_const(self.emitter.layout_reg.fixed_offset(super::engine::layout::LIST, super::engine::layout::list::DATA) as i32); i32_add;
                          local_get(r); local_get(total_cols); i32_mul;
                          local_get(col_off); i32_add; local_get(c); i32_add;
                          i32_const(8); i32_mul; i32_add;
                          local_get(sub); i32_const(self.emitter.layout_reg.fixed_offset(super::engine::layout::LIST, super::engine::layout::list::DATA) as i32); i32_add;
                          local_get(r); local_get(sub_cols); i32_mul; local_get(c); i32_add;
                          i32_const(8); i32_mul; i32_add;
                          f64_load(0); f64_store(0);
                          local_get(c); i32_const(1); i32_add; local_set(c);
                          br(0);
                        end; end;
                        local_get(r); i32_const(1); i32_add; local_set(r);
                        br(0);
                      end; end;
                      local_get(col_off); local_get(sub_cols); i32_add; local_set(col_off);
                      local_get(i); i32_const(1); i32_add; local_set(i);
                      br(0);
                    end; end;
                    local_get(dst);
                });
                self.scratch.free_i32(c);
                self.scratch.free_i32(r);
                self.scratch.free_i32(col_off);
                self.scratch.free_i32(sub_cols);
                self.scratch.free_i32(sub);
                self.scratch.free_i32(i);
                self.scratch.free_i32(total_cols);
                self.scratch.free_i32(rows);
                self.scratch.free_i32(first);
                self.scratch.free_i32(dst);
                self.scratch.free_i32(n);
                self.scratch.free_i32(lst);
            }
            "gather_rows" => {
                // matrix.gather_rows(m, indices: List[Int]) → Matrix
                let m = self.scratch.alloc_i32();
                let indices = self.scratch.alloc_i32();
                let cols = self.scratch.alloc_i32();
                let n_rows_src = self.scratch.alloc_i32();
                let n = self.scratch.alloc_i32();
                let dst = self.scratch.alloc_i32();
                let i = self.scratch.alloc_i32();
                let idx = self.scratch.alloc_i32();
                self.emit_expr(&args[0]);
                wasm!(self.func, { local_set(m); });
                self.emit_expr(&args[1]);
                wasm!(self.func, {
                    local_set(indices);
                    local_get(m); i32_load(0); local_set(n_rows_src);
                    local_get(m); i32_load(4); local_set(cols);
                    local_get(indices); i32_load(0); local_set(n);
                    // Alloc dst (n, cols)
                    local_get(n); local_get(cols); i32_mul; i32_const(8); i32_mul;
                    i32_const(self.emitter.layout_reg.fixed_offset(super::engine::layout::LIST, super::engine::layout::list::DATA) as i32); i32_add;
                    call(self.emitter.rt.alloc); local_set(dst);
                    local_get(dst); local_get(n); i32_store(0);
                    local_get(dst); local_get(cols); i32_store(4);
                    i32_const(0); local_set(i);
                    block_empty; loop_empty;
                      local_get(i); local_get(n); i32_ge_u; br_if(1);
                      // idx = indices[i] (i64 → i32)
                      local_get(indices); i32_const(self.emitter.layout_reg.fixed_offset(super::engine::layout::LIST, super::engine::layout::list::DATA) as i32); i32_add;
                      local_get(i); i32_const(8); i32_mul; i32_add;
                      i64_load(0); i32_wrap_i64; local_set(idx);
                      // bounds clamp: if idx >= n_rows_src: zero the row, else memcpy
                      local_get(idx); local_get(n_rows_src); i32_ge_u;
                      if_empty;
                        // Zero
                        local_get(dst); i32_const(self.emitter.layout_reg.fixed_offset(super::engine::layout::LIST, super::engine::layout::list::DATA) as i32); i32_add;
                        local_get(i); local_get(cols); i32_mul; i32_const(8); i32_mul; i32_add;
                        i32_const(0);
                        local_get(cols); i32_const(8); i32_mul;
                        memory_fill;
                      else_;
                        // memcpy: dst[i] ← m[idx]
                        local_get(dst); i32_const(self.emitter.layout_reg.fixed_offset(super::engine::layout::LIST, super::engine::layout::list::DATA) as i32); i32_add;
                        local_get(i); local_get(cols); i32_mul; i32_const(8); i32_mul; i32_add;
                        local_get(m); i32_const(self.emitter.layout_reg.fixed_offset(super::engine::layout::LIST, super::engine::layout::list::DATA) as i32); i32_add;
                        local_get(idx); local_get(cols); i32_mul; i32_const(8); i32_mul; i32_add;
                        local_get(cols); i32_const(8); i32_mul;
                        memory_copy;
                      end;
                      local_get(i); i32_const(1); i32_add; local_set(i);
                      br(0);
                    end; end;
                    local_get(dst);
                });
                self.scratch.free_i32(idx);
                self.scratch.free_i32(i);
                self.scratch.free_i32(dst);
                self.scratch.free_i32(n);
                self.scratch.free_i32(n_rows_src);
                self.scratch.free_i32(cols);
                self.scratch.free_i32(indices);
                self.scratch.free_i32(m);
            }
            "dot_row" | "row_dot" => {
                // matrix.row_dot(m, r, vec) → Float
                let m = self.scratch.alloc_i32();
                let row = self.scratch.alloc_i32();
                let vec = self.scratch.alloc_i32();
                let cols = self.scratch.alloc_i32();
                let vlen = self.scratch.alloc_i32();
                let n = self.scratch.alloc_i32();
                let i = self.scratch.alloc_i32();
                let s = self.scratch.alloc_f64();
                self.emit_expr(&args[0]);
                wasm!(self.func, { local_set(m); });
                self.emit_expr(&args[1]);
                wasm!(self.func, { i32_wrap_i64; local_set(row); });
                self.emit_expr(&args[2]);
                wasm!(self.func, {
                    local_set(vec);
                    local_get(m); i32_load(4); local_set(cols);
                    local_get(vec); i32_load(0); local_set(vlen);
                    // n = min(cols, vlen)
                    local_get(cols); local_get(vlen); i32_lt_u;
                    if_i32; local_get(cols); else_; local_get(vlen); end;
                    local_set(n);
                    f64_const(0.0); local_set(s);
                    i32_const(0); local_set(i);
                    block_empty; loop_empty;
                      local_get(i); local_get(n); i32_ge_u; br_if(1);
                      // m[row, i]
                      local_get(m); i32_const(self.emitter.layout_reg.fixed_offset(super::engine::layout::LIST, super::engine::layout::list::DATA) as i32); i32_add;
                      local_get(row); local_get(cols); i32_mul; local_get(i); i32_add;
                      i32_const(8); i32_mul; i32_add; f64_load(0);
                      // vec[i]
                      local_get(vec); i32_const(self.emitter.layout_reg.fixed_offset(super::engine::layout::LIST, super::engine::layout::list::DATA) as i32); i32_add;
                      local_get(i); i32_const(8); i32_mul; i32_add; f64_load(0);
                      f64_mul; local_get(s); f64_add; local_set(s);
                      local_get(i); i32_const(1); i32_add; local_set(i);
                      br(0);
                    end; end;
                    local_get(s);
                });
                self.scratch.free_f64(s);
                self.scratch.free_i32(i);
                self.scratch.free_i32(n);
                self.scratch.free_i32(vlen);
                self.scratch.free_i32(cols);
                self.scratch.free_i32(vec);
                self.scratch.free_i32(row);
                self.scratch.free_i32(m);
            }
            _ => return false,
        }
        true
    }
}
