impl FuncCompiler<'_> {
    /// Matrix dispatch group 3 (chained from `emit_matrix_call`).
    /// Disjoint arm subset; returns true iff this group handled `method`.
    pub(super) fn emit_matrix_call_g3(&mut self, method: &str, args: &[IrExpr]) -> bool {
        match method {
            "select_rows_q1_0" => {
                // matrix.select_rows_q1_0(data, offset, cols, row_ids) -> Matrix
                // Extract the listed rows directly from a packed Q1_0 byte
                // buffer. Eliminates the 2.5 GB full-matrix decode that
                // embedding lookup would otherwise need every forward call.
                let data = self.scratch.alloc_i32();
                let off = self.scratch.alloc_i32();
                let cols = self.scratch.alloc_i32();
                let ids = self.scratch.alloc_i32();
                let n_rows = self.scratch.alloc_i32();
                let n_bpr = self.scratch.alloc_i32();
                let data_base = self.scratch.alloc_i32();
                let dst = self.scratch.alloc_i32();
                let i = self.scratch.alloc_i32();
                let rid = self.scratch.alloc_i32();
                let b = self.scratch.alloc_i32();
                let k_local = self.scratch.alloc_i32();
                let row_src_off = self.scratch.alloc_i32();
                let block_start = self.scratch.alloc_i32();
                let bits_start = self.scratch.alloc_i32();
                let scale_raw = self.scratch.alloc_i32();
                let sign = self.scratch.alloc_i32();
                let expv = self.scratch.alloc_i32();
                let mant = self.scratch.alloc_i32();
                let f32bits = self.scratch.alloc_i32();
                let bit = self.scratch.alloc_i32();
                let scale = self.scratch.alloc_f64();
                let neg_scale = self.scratch.alloc_f64();

                self.emit_expr(&args[0]);
                wasm!(self.func, { local_set(data); });
                self.emit_expr(&args[1]);
                wasm!(self.func, { i32_wrap_i64; local_set(off); });
                self.emit_expr(&args[2]);
                wasm!(self.func, { i32_wrap_i64; local_set(cols); });
                self.emit_expr(&args[3]);
                wasm!(self.func, { local_set(ids); });

                wasm!(self.func, {
                    local_get(ids); i32_load(0); local_set(n_rows);
                    local_get(cols); i32_const(7); i32_shr_u; local_set(n_bpr);
                    local_get(data); i32_const(self.emitter.layout_reg.fixed_offset(super::engine::layout::LIST, super::engine::layout::list::DATA) as i32); i32_add;
                    local_get(off); i32_add;
                    local_set(data_base);
                    // dst = alloc(8 + n_rows*cols*8)
                    local_get(n_rows); local_get(cols); i32_mul;
                    i32_const(8); i32_mul;
                    i32_const(self.emitter.layout_reg.fixed_offset(super::engine::layout::LIST, super::engine::layout::list::DATA) as i32); i32_add;
                    call(self.emitter.rt.alloc); local_set(dst);
                    local_get(dst); local_get(n_rows); i32_store(0);
                    local_get(dst); local_get(cols); i32_store(4);
                    i32_const(0); local_set(i);
                    block_empty; loop_empty;
                      local_get(i); local_get(n_rows); i32_ge_u; br_if(1);
                      // rid = row_ids[i] as i32
                      local_get(ids); i32_const(self.emitter.layout_reg.fixed_offset(super::engine::layout::LIST, super::engine::layout::list::DATA) as i32); i32_add;
                      local_get(i); i32_const(8); i32_mul; i32_add;
                      i64_load(0); i32_wrap_i64;
                      local_set(rid);
                      // row_src_off = rid * n_bpr * 18
                      local_get(rid); local_get(n_bpr); i32_mul;
                      i32_const(18); i32_mul;
                      local_set(row_src_off);
                      i32_const(0); local_set(b);
                      block_empty; loop_empty;
                        local_get(b); local_get(n_bpr); i32_ge_u; br_if(1);
                        local_get(data_base);
                        local_get(row_src_off); i32_add;
                        local_get(b); i32_const(18); i32_mul;
                        i32_add;
                        local_set(block_start);
                        local_get(block_start); i32_load8_u(0);
                        local_get(block_start); i32_load8_u(1);
                        i32_const(8); i32_shl;
                        i32_or;
                        local_set(scale_raw);
                        local_get(scale_raw); i32_const(15); i32_shr_u; i32_const(1); i32_and;
                        local_set(sign);
                        local_get(scale_raw); i32_const(10); i32_shr_u; i32_const(31); i32_and;
                        local_set(expv);
                        local_get(scale_raw); i32_const(1023); i32_and;
                        local_set(mant);
                        local_get(sign); i32_const(31); i32_shl;
                        local_get(expv); i32_const(112); i32_add; i32_const(23); i32_shl;
                        i32_or;
                        local_get(mant); i32_const(13); i32_shl;
                        i32_or;
                        local_set(f32bits);
                        local_get(expv); i32_eqz;
                        if_empty;
                          local_get(sign); i32_const(31); i32_shl; local_set(f32bits);
                        end;
                        local_get(f32bits); f32_reinterpret_i32; f64_promote_f32;
                        local_set(scale);
                        f64_const(0.0); local_get(scale); f64_sub; local_set(neg_scale);
                        local_get(block_start); i32_const(2); i32_add; local_set(bits_start);
                        i32_const(0); local_set(k_local);
                        block_empty; loop_empty;
                          local_get(k_local); i32_const(128); i32_ge_u; br_if(1);
                          local_get(bits_start);
                          local_get(k_local); i32_const(3); i32_shr_u;
                          i32_add;
                          i32_load8_u(0);
                          local_get(k_local); i32_const(7); i32_and; i32_shr_u;
                          i32_const(1); i32_and;
                          local_set(bit);
                          // dst[8 + (i*cols + b*128 + k_local)*8] = bit? scale : neg_scale
                          local_get(dst); i32_const(self.emitter.layout_reg.fixed_offset(super::engine::layout::LIST, super::engine::layout::list::DATA) as i32); i32_add;
                          local_get(i); local_get(cols); i32_mul;
                          local_get(b); i32_const(128); i32_mul; i32_add;
                          local_get(k_local); i32_add;
                          i32_const(8); i32_mul;
                          i32_add;
                          local_get(bit);
                          if_f64;
                            local_get(scale);
                          else_;
                            local_get(neg_scale);
                          end;
                          f64_store(0);
                          local_get(k_local); i32_const(1); i32_add; local_set(k_local);
                          br(0);
                        end; end;
                        local_get(b); i32_const(1); i32_add; local_set(b);
                        br(0);
                      end; end;
                      local_get(i); i32_const(1); i32_add; local_set(i);
                      br(0);
                    end; end;
                    local_get(dst);
                });
                self.scratch.free_f64(neg_scale);
                self.scratch.free_f64(scale);
                self.scratch.free_i32(bit);
                self.scratch.free_i32(f32bits);
                self.scratch.free_i32(mant);
                self.scratch.free_i32(expv);
                self.scratch.free_i32(sign);
                self.scratch.free_i32(scale_raw);
                self.scratch.free_i32(bits_start);
                self.scratch.free_i32(block_start);
                self.scratch.free_i32(row_src_off);
                self.scratch.free_i32(k_local);
                self.scratch.free_i32(b);
                self.scratch.free_i32(rid);
                self.scratch.free_i32(i);
                self.scratch.free_i32(dst);
                self.scratch.free_i32(data_base);
                self.scratch.free_i32(n_bpr);
                self.scratch.free_i32(n_rows);
                self.scratch.free_i32(ids);
                self.scratch.free_i32(cols);
                self.scratch.free_i32(off);
                self.scratch.free_i32(data);
            }
            "silu_mul" => {
                // matrix.silu_mul(a, b) -> Matrix: y[i, j] = silu(a[i,j]) * b[i,j]
                // silu(x) = x / (1 + exp(-x))
                let a = self.scratch.alloc_i32();
                let b = self.scratch.alloc_i32();
                let dst = self.scratch.alloc_i32();
                let rows = self.scratch.alloc_i32();
                let cols = self.scratch.alloc_i32();
                let total = self.scratch.alloc_i32();
                let k = self.scratch.alloc_i32();
                let xv = self.scratch.alloc_f64();
                let sig = self.scratch.alloc_f64();
                self.emit_expr(&args[0]);
                wasm!(self.func, { local_set(a); });
                self.emit_expr(&args[1]);
                wasm!(self.func, { local_set(b); });
                let math_exp = self.emitter.rt.math_exp;
                wasm!(self.func, {
                    local_get(a); i32_load(0); local_set(rows);
                    local_get(a); i32_load(4); local_set(cols);
                    local_get(rows); local_get(cols); i32_mul; local_set(total);
                    local_get(total); i32_const(8); i32_mul; i32_const(self.emitter.layout_reg.header_size(super::engine::layout::LIST) as i32); i32_add;
                    call(self.emitter.rt.alloc); local_set(dst);
                    local_get(dst); local_get(rows); i32_store(0);
                    local_get(dst); local_get(cols); i32_store(4);
                    i32_const(0); local_set(k);
                    block_empty; loop_empty;
                      local_get(k); local_get(total); i32_ge_u; br_if(1);
                      // xv = a[k]
                      local_get(a); i32_const(self.emitter.layout_reg.fixed_offset(super::engine::layout::LIST, super::engine::layout::list::DATA) as i32); i32_add;
                      local_get(k); i32_const(8); i32_mul; i32_add;
                      f64_load(0);
                      local_set(xv);
                      // sig = 1 / (1 + exp(-xv))
                      f64_const(0.0); local_get(xv); f64_sub;
                      call(math_exp);
                      f64_const(1.0); f64_add;
                      local_set(sig);
                      f64_const(1.0); local_get(sig); f64_div; local_set(sig);
                      // result = xv * sig * b[k]
                      local_get(dst); i32_const(self.emitter.layout_reg.fixed_offset(super::engine::layout::LIST, super::engine::layout::list::DATA) as i32); i32_add;
                      local_get(k); i32_const(8); i32_mul; i32_add;
                      local_get(xv); local_get(sig); f64_mul;
                      local_get(b); i32_const(self.emitter.layout_reg.fixed_offset(super::engine::layout::LIST, super::engine::layout::list::DATA) as i32); i32_add;
                      local_get(k); i32_const(8); i32_mul; i32_add;
                      f64_load(0);
                      f64_mul;
                      f64_store(0);
                      local_get(k); i32_const(1); i32_add; local_set(k);
                      br(0);
                    end; end;
                    local_get(dst);
                });
                self.scratch.free_f64(sig);
                self.scratch.free_f64(xv);
                self.scratch.free_i32(k);
                self.scratch.free_i32(total);
                self.scratch.free_i32(cols);
                self.scratch.free_i32(rows);
                self.scratch.free_i32(dst);
                self.scratch.free_i32(b);
                self.scratch.free_i32(a);
            }
            "linear_q1_0_row_no_bias" => {
                // matrix.linear_q1_0_row_no_bias(x, w_bytes, w_offset, w_rows, w_cols) -> Matrix
                // y[i, j] = Σ_k x[i, k] * W[j, k]; W packed Q1_0.
                //
                // SIMD inner loop: process 2 weights per iteration using
                // f64x2. The 128-weight block is now 64 pair iterations,
                // each packing {w0,w1} as a v128 from 2 sign-bits (same
                // byte — pair_idx*2 aligns to even lane positions so the
                // two bits always live within one byte). Accumulator is
                // v128 across all blocks for (i,j), reduced to scalar once
                // at the end.
                let x = self.scratch.alloc_i32();
                let w_bytes = self.scratch.alloc_i32();
                let w_off = self.scratch.alloc_i32();
                let out = self.scratch.alloc_i32();
                let n_in = self.scratch.alloc_i32();
                let x_rows = self.scratch.alloc_i32();
                let dst = self.scratch.alloc_i32();
                let w_data = self.scratch.alloc_i32();
                let n_bpr = self.scratch.alloc_i32();   // blocks per row
                let i = self.scratch.alloc_i32();
                let j = self.scratch.alloc_i32();
                let b = self.scratch.alloc_i32();
                let pair_idx = self.scratch.alloc_i32();
                let block_start = self.scratch.alloc_i32();
                let bits_start = self.scratch.alloc_i32();
                let scale_raw = self.scratch.alloc_i32();
                let sign = self.scratch.alloc_i32();
                let expv = self.scratch.alloc_i32();
                let mant = self.scratch.alloc_i32();
                let f32bits = self.scratch.alloc_i32();
                let byte_idx = self.scratch.alloc_i32();
                let byte_val = self.scratch.alloc_i32();
                let bit_shift = self.scratch.alloc_i32();
                let bit0 = self.scratch.alloc_i32();
                let bit1 = self.scratch.alloc_i32();
                let scale = self.scratch.alloc_f64();
                let neg_scale = self.scratch.alloc_f64();
                let w0 = self.scratch.alloc_f64();
                let w1 = self.scratch.alloc_f64();
                let sum_v = self.scratch.alloc_v128();
                let w_v = self.scratch.alloc_v128();
                let x_v = self.scratch.alloc_v128();

                self.emit_expr(&args[0]);
                wasm!(self.func, { local_set(x); });
                self.emit_expr(&args[1]);
                wasm!(self.func, { local_set(w_bytes); });
                self.emit_expr(&args[2]);
                wasm!(self.func, { i32_wrap_i64; local_set(w_off); });
                self.emit_expr(&args[3]);
                wasm!(self.func, { i32_wrap_i64; local_set(out); });
                self.emit_expr(&args[4]);
                wasm!(self.func, { i32_wrap_i64; local_set(n_in); });

                wasm!(self.func, {
                    local_get(x); i32_load(0); local_set(x_rows);
                    // w_data = w_bytes + 4 + w_off
                    local_get(w_bytes); i32_const(self.emitter.layout_reg.fixed_offset(super::engine::layout::LIST, super::engine::layout::list::DATA) as i32); i32_add;
                    local_get(w_off); i32_add;
                    local_set(w_data);
                    // n_bpr = n_in / 128
                    local_get(n_in); i32_const(7); i32_shr_u; local_set(n_bpr);
                    // dst = alloc(8 + x_rows*out*8); header
                    local_get(x_rows); local_get(out); i32_mul;
                    i32_const(8); i32_mul;
                    i32_const(self.emitter.layout_reg.fixed_offset(super::engine::layout::LIST, super::engine::layout::list::DATA) as i32); i32_add;
                    call(self.emitter.rt.alloc); local_set(dst);
                    local_get(dst); local_get(x_rows); i32_store(0);
                    local_get(dst); local_get(out); i32_store(4);
                    // for i in 0..x_rows
                    i32_const(0); local_set(i);
                    block_empty; loop_empty;
                      local_get(i); local_get(x_rows); i32_ge_u; br_if(1);
                      // for j in 0..out
                      i32_const(0); local_set(j);
                      block_empty; loop_empty;
                        local_get(j); local_get(out); i32_ge_u; br_if(1);
                        // sum_v = f64x2(0.0, 0.0)
                        f64_const(0.0); f64x2_splat;
                        local_set(sum_v);
                        // for b in 0..n_bpr
                        i32_const(0); local_set(b);
                        block_empty; loop_empty;
                          local_get(b); local_get(n_bpr); i32_ge_u; br_if(1);
                          // block_start = w_data + j*n_bpr*18 + b*18
                          local_get(w_data);
                          local_get(j); local_get(n_bpr); i32_mul;
                          local_get(b); i32_add;
                          i32_const(18); i32_mul;
                          i32_add;
                          local_set(block_start);
                          // scale_raw = block[0] | block[1]<<8
                          local_get(block_start); i32_load8_u(0);
                          local_get(block_start); i32_load8_u(1);
                          i32_const(8); i32_shl;
                          i32_or;
                          local_set(scale_raw);
                          local_get(scale_raw); i32_const(15); i32_shr_u; i32_const(1); i32_and;
                          local_set(sign);
                          local_get(scale_raw); i32_const(10); i32_shr_u; i32_const(31); i32_and;
                          local_set(expv);
                          local_get(scale_raw); i32_const(1023); i32_and;
                          local_set(mant);
                          // f32bits: normal case (exp + 112) << 23
                          local_get(sign); i32_const(31); i32_shl;
                          local_get(expv); i32_const(112); i32_add; i32_const(23); i32_shl;
                          i32_or;
                          local_get(mant); i32_const(13); i32_shl;
                          i32_or;
                          local_set(f32bits);
                          // If exp == 0 force zero (sign bit only).
                          local_get(expv); i32_eqz;
                          if_empty;
                            local_get(sign); i32_const(31); i32_shl; local_set(f32bits);
                          end;
                          local_get(f32bits); f32_reinterpret_i32; f64_promote_f32;
                          local_set(scale);
                          f64_const(0.0); local_get(scale); f64_sub; local_set(neg_scale);
                          local_get(block_start); i32_const(2); i32_add; local_set(bits_start);
                          // for pair_idx in 0..64
                          i32_const(0); local_set(pair_idx);
                          block_empty; loop_empty;
                            local_get(pair_idx); i32_const(64); i32_ge_u; br_if(1);
                            // byte_idx = pair_idx >> 2  (each byte holds 4 pairs = 8 bits)
                            local_get(pair_idx); i32_const(2); i32_shr_u;
                            local_set(byte_idx);
                            // bit_shift = (pair_idx & 3) << 1
                            local_get(pair_idx); i32_const(3); i32_and;
                            i32_const(1); i32_shl;
                            local_set(bit_shift);
                            // byte_val = block[bits_start + byte_idx]
                            local_get(bits_start); local_get(byte_idx); i32_add;
                            i32_load8_u(0);
                            local_set(byte_val);
                            // bit0 = (byte_val >> bit_shift) & 1
                            local_get(byte_val); local_get(bit_shift); i32_shr_u;
                            i32_const(1); i32_and;
                            local_set(bit0);
                            // bit1 = (byte_val >> (bit_shift + 1)) & 1
                            local_get(byte_val);
                            local_get(bit_shift); i32_const(1); i32_add; i32_shr_u;
                            i32_const(1); i32_and;
                            local_set(bit1);
                            // w0 = bit0 ? scale : neg_scale
                            local_get(bit0);
                            if_f64; local_get(scale); else_; local_get(neg_scale); end;
                            local_set(w0);
                            // w1 = bit1 ? scale : neg_scale
                            local_get(bit1);
                            if_f64; local_get(scale); else_; local_get(neg_scale); end;
                            local_set(w1);
                            // w_v = f64x2{w0, w1}
                            local_get(w0); f64x2_splat;
                            local_get(w1); f64x2_replace_lane(1);
                            local_set(w_v);
                            // x_addr = x + 8 + (i*n_in + b*128 + pair_idx*2) * 8
                            local_get(x); i32_const(self.emitter.layout_reg.fixed_offset(super::engine::layout::LIST, super::engine::layout::list::DATA) as i32); i32_add;
                            local_get(i); local_get(n_in); i32_mul;
                            local_get(b); i32_const(128); i32_mul; i32_add;
                            local_get(pair_idx); i32_const(1); i32_shl; i32_add;
                            i32_const(8); i32_mul;
                            i32_add;
                            v128_load(0);
                            local_set(x_v);
                            // sum_v = sum_v + x_v * w_v
                            local_get(sum_v);
                            local_get(x_v); local_get(w_v); f64x2_mul;
                            f64x2_add;
                            local_set(sum_v);
                            local_get(pair_idx); i32_const(1); i32_add; local_set(pair_idx);
                            br(0);
                          end; end;
                          local_get(b); i32_const(1); i32_add; local_set(b);
                          br(0);
                        end; end;
                        // dst[i, j] = sum_v[0] + sum_v[1]
                        local_get(dst); i32_const(self.emitter.layout_reg.fixed_offset(super::engine::layout::LIST, super::engine::layout::list::DATA) as i32); i32_add;
                        local_get(i); local_get(out); i32_mul;
                        local_get(j); i32_add;
                        i32_const(8); i32_mul;
                        i32_add;
                        local_get(sum_v); f64x2_extract_lane(0);
                        local_get(sum_v); f64x2_extract_lane(1);
                        f64_add;
                        f64_store(0);
                        local_get(j); i32_const(1); i32_add; local_set(j);
                        br(0);
                      end; end;
                      local_get(i); i32_const(1); i32_add; local_set(i);
                      br(0);
                    end; end;
                    local_get(dst);
                });
                self.scratch.free_v128(x_v);
                self.scratch.free_v128(w_v);
                self.scratch.free_v128(sum_v);
                self.scratch.free_f64(w1);
                self.scratch.free_f64(w0);
                self.scratch.free_f64(neg_scale);
                self.scratch.free_f64(scale);
                self.scratch.free_i32(bit1);
                self.scratch.free_i32(bit0);
                self.scratch.free_i32(bit_shift);
                self.scratch.free_i32(byte_val);
                self.scratch.free_i32(byte_idx);
                self.scratch.free_i32(f32bits);
                self.scratch.free_i32(mant);
                self.scratch.free_i32(expv);
                self.scratch.free_i32(sign);
                self.scratch.free_i32(scale_raw);
                self.scratch.free_i32(bits_start);
                self.scratch.free_i32(block_start);
                self.scratch.free_i32(pair_idx);
                self.scratch.free_i32(b);
                self.scratch.free_i32(j);
                self.scratch.free_i32(i);
                self.scratch.free_i32(n_bpr);
                self.scratch.free_i32(w_data);
                self.scratch.free_i32(dst);
                self.scratch.free_i32(x_rows);
                self.scratch.free_i32(n_in);
                self.scratch.free_i32(out);
                self.scratch.free_i32(w_off);
                self.scratch.free_i32(w_bytes);
                self.scratch.free_i32(x);
            }
            "select_rows" => {
                // matrix.select_rows(m: Matrix, row_ids: List[Int]) -> Matrix
                //
                // Gather a subset of rows without going through
                // `to_lists` — critical for embedding lookup on the
                // large token_embd matrix (151 k rows × 2 k cols × 8 B
                // = 2.5 GB) where to_lists would double peak memory.
                let m = self.scratch.alloc_i32();
                let ids = self.scratch.alloc_i32();
                let cols = self.scratch.alloc_i32();
                let src_rows = self.scratch.alloc_i32();
                let n_out = self.scratch.alloc_i32();
                let row_bytes = self.scratch.alloc_i32();
                let dst = self.scratch.alloc_i32();
                let k = self.scratch.alloc_i32();
                let rid = self.scratch.alloc_i32();
                self.emit_expr(&args[0]);
                wasm!(self.func, { local_set(m); });
                self.emit_expr(&args[1]);
                wasm!(self.func, { local_set(ids); });
                wasm!(self.func, {
                    local_get(m); i32_load(0); local_set(src_rows);
                    local_get(m); i32_load(4); local_set(cols);
                    local_get(ids); i32_load(0); local_set(n_out);
                    local_get(cols); i32_const(8); i32_mul; local_set(row_bytes);
                    // out = alloc(8 + n_out*row_bytes)
                    local_get(n_out); local_get(row_bytes); i32_mul;
                    i32_const(self.emitter.layout_reg.fixed_offset(super::engine::layout::LIST, super::engine::layout::list::DATA) as i32); i32_add;
                    call(self.emitter.rt.alloc); local_set(dst);
                    local_get(dst); local_get(n_out); i32_store(0);
                    local_get(dst); local_get(cols); i32_store(4);
                    i32_const(0); local_set(k);
                    block_empty; loop_empty;
                      local_get(k); local_get(n_out); i32_ge_u; br_if(1);
                      // rid = (i64)ids[4 + k*8] as i32
                      local_get(ids); i32_const(self.emitter.layout_reg.fixed_offset(super::engine::layout::LIST, super::engine::layout::list::DATA) as i32); i32_add;
                      local_get(k); i32_const(8); i32_mul; i32_add;
                      i64_load(0);
                      i32_wrap_i64;
                      local_set(rid);
                      // if rid < 0 or rid >= src_rows, write zeros; else copy
                      local_get(rid); i32_const(0); i32_lt_s;
                      local_get(rid); local_get(src_rows); i32_ge_u;
                      i32_or;
                      if_empty;
                        // memory.fill 0 (dst + 8 + k*row_bytes, 0, row_bytes)
                        local_get(dst); i32_const(self.emitter.layout_reg.fixed_offset(super::engine::layout::LIST, super::engine::layout::list::DATA) as i32); i32_add;
                        local_get(k); local_get(row_bytes); i32_mul; i32_add;
                        i32_const(0);
                        local_get(row_bytes);
                        memory_fill;
                      else_;
                        // memcpy(dst + 8 + k*row_bytes, m + 8 + rid*row_bytes, row_bytes)
                        local_get(dst); i32_const(self.emitter.layout_reg.fixed_offset(super::engine::layout::LIST, super::engine::layout::list::DATA) as i32); i32_add;
                        local_get(k); local_get(row_bytes); i32_mul; i32_add;
                        local_get(m); i32_const(self.emitter.layout_reg.fixed_offset(super::engine::layout::LIST, super::engine::layout::list::DATA) as i32); i32_add;
                        local_get(rid); local_get(row_bytes); i32_mul; i32_add;
                        local_get(row_bytes);
                        memory_copy;
                      end;
                      local_get(k); i32_const(1); i32_add; local_set(k);
                      br(0);
                    end; end;
                    local_get(dst);
                });
                self.scratch.free_i32(rid);
                self.scratch.free_i32(k);
                self.scratch.free_i32(dst);
                self.scratch.free_i32(row_bytes);
                self.scratch.free_i32(n_out);
                self.scratch.free_i32(src_rows);
                self.scratch.free_i32(cols);
                self.scratch.free_i32(ids);
                self.scratch.free_i32(m);
            }
            "rope_rotate" | "rope_rotate_at" => {
                // matrix.rope_rotate(x, n_heads, head_dim, theta_base) -> Matrix
                // matrix.rope_rotate_at(x, n_heads, head_dim, theta_base, start_pos) -> Matrix
                //
                // Standard RoPE: pair each head's (x[2i], x[2i+1]) and rotate
                // by `(start_pos + row_idx) * inv_freq[i]`. `rope_rotate_at`
                // is the KV-cache variant — cached rows sit at positions
                // 0..start_pos, the one new row gets start_pos. Calling
                // `rope_rotate` is equivalent to `rope_rotate_at(..., 0)`.
                let x = self.scratch.alloc_i32();
                let n_heads = self.scratch.alloc_i32();
                let head_dim = self.scratch.alloc_i32();
                let half = self.scratch.alloc_i32();
                let start_pos = self.scratch.alloc_i32();
                let theta = self.scratch.alloc_f64();
                let log_theta = self.scratch.alloc_f64();
                let head_dim_f = self.scratch.alloc_f64();
                let dst = self.scratch.alloc_i32();
                let rows = self.scratch.alloc_i32();
                let cols = self.scratch.alloc_i32();
                let total_bytes = self.scratch.alloc_i32();
                let p = self.scratch.alloc_i32();
                let h = self.scratch.alloc_i32();
                let i_pair = self.scratch.alloc_i32();
                let j0 = self.scratch.alloc_i32();
                let pair_off = self.scratch.alloc_i32();
                let pos_f = self.scratch.alloc_f64();
                let two_i_f = self.scratch.alloc_f64();
                let angle = self.scratch.alloc_f64();
                let s = self.scratch.alloc_f64();
                let c = self.scratch.alloc_f64();
                let x0 = self.scratch.alloc_f64();
                let x1 = self.scratch.alloc_f64();
                let inv_freq = self.scratch.alloc_f64();

                self.emit_expr(&args[0]);
                wasm!(self.func, { local_set(x); });
                self.emit_expr(&args[1]);
                wasm!(self.func, { i32_wrap_i64; local_set(n_heads); });
                self.emit_expr(&args[2]);
                wasm!(self.func, { i32_wrap_i64; local_set(head_dim); });
                self.emit_expr(&args[3]);
                wasm!(self.func, { local_set(theta); });
                if method == "rope_rotate_at" {
                    self.emit_expr(&args[4]);
                    wasm!(self.func, { i32_wrap_i64; local_set(start_pos); });
                } else {
                    wasm!(self.func, { i32_const(0); local_set(start_pos); });
                }

                let math_log = self.emitter.rt.math_log;
                let math_exp = self.emitter.rt.math_exp;
                let math_sin = self.emitter.rt.math_sin;
                let math_cos = self.emitter.rt.math_cos;

                wasm!(self.func, {
                    local_get(head_dim); i32_const(1); i32_shr_u; local_set(half);
                    local_get(head_dim); f64_convert_i32_u; local_set(head_dim_f);
                    local_get(theta); call(math_log); local_set(log_theta);
                    local_get(x); i32_load(0); local_set(rows);
                    local_get(x); i32_load(4); local_set(cols);
                    // total_bytes = 8 + rows*cols*8
                    local_get(rows); local_get(cols); i32_mul; i32_const(8); i32_mul;
                    i32_const(self.emitter.layout_reg.fixed_offset(super::engine::layout::LIST, super::engine::layout::list::DATA) as i32); i32_add; local_set(total_bytes);
                    local_get(total_bytes); call(self.emitter.rt.alloc); local_set(dst);
                    // memory.copy(dst, x, total_bytes)
                    local_get(dst); local_get(x); local_get(total_bytes); memory_copy;
                    i32_const(0); local_set(p);
                    block_empty; loop_empty;
                      local_get(p); local_get(rows); i32_ge_u; br_if(1);
                      local_get(p); local_get(start_pos); i32_add;
                      f64_convert_i32_u; local_set(pos_f);
                      i32_const(0); local_set(h);
                      block_empty; loop_empty;
                        local_get(h); local_get(n_heads); i32_ge_u; br_if(1);
                        i32_const(0); local_set(i_pair);
                        block_empty; loop_empty;
                          local_get(i_pair); local_get(half); i32_ge_u; br_if(1);
                          // j0 = h*head_dim + 2*i
                          local_get(h); local_get(head_dim); i32_mul;
                          local_get(i_pair); i32_const(1); i32_shl;
                          i32_add;
                          local_set(j0);
                          // pair_off = 8 + p*cols*8 + j0*8
                          i32_const(8);
                          local_get(p); local_get(cols); i32_mul; i32_const(8); i32_mul;
                          i32_add;
                          local_get(j0); i32_const(8); i32_mul;
                          i32_add;
                          local_set(pair_off);
                          // x0, x1 from source
                          local_get(x); local_get(pair_off); i32_add; f64_load(0); local_set(x0);
                          local_get(x); local_get(pair_off); i32_add; f64_load(8); local_set(x1);
                          // two_i_f = (2i) as f64
                          local_get(i_pair); i32_const(1); i32_shl;
                          f64_convert_i32_u; local_set(two_i_f);
                          // inv_freq = exp(-(two_i_f / head_dim_f) * log_theta)
                          f64_const(0.0);
                          local_get(two_i_f); local_get(head_dim_f); f64_div;
                          local_get(log_theta); f64_mul;
                          f64_sub;
                          call(math_exp); local_set(inv_freq);
                          // angle = pos_f * inv_freq
                          local_get(pos_f); local_get(inv_freq); f64_mul; local_set(angle);
                          local_get(angle); call(math_sin); local_set(s);
                          local_get(angle); call(math_cos); local_set(c);
                          // store new_x0 = x0*c - x1*s
                          local_get(dst); local_get(pair_off); i32_add;
                          local_get(x0); local_get(c); f64_mul;
                          local_get(x1); local_get(s); f64_mul;
                          f64_sub;
                          f64_store(0);
                          // store new_x1 = x0*s + x1*c  (offset +8)
                          local_get(dst); local_get(pair_off); i32_add;
                          local_get(x0); local_get(s); f64_mul;
                          local_get(x1); local_get(c); f64_mul;
                          f64_add;
                          f64_store(8);
                          local_get(i_pair); i32_const(1); i32_add; local_set(i_pair);
                          br(0);
                        end; end;
                        local_get(h); i32_const(1); i32_add; local_set(h);
                        br(0);
                      end; end;
                      local_get(p); i32_const(1); i32_add; local_set(p);
                      br(0);
                    end; end;
                    local_get(dst);
                });
                self.scratch.free_f64(inv_freq);
                self.scratch.free_f64(x1);
                self.scratch.free_f64(x0);
                self.scratch.free_f64(c);
                self.scratch.free_f64(s);
                self.scratch.free_f64(angle);
                self.scratch.free_f64(two_i_f);
                self.scratch.free_f64(pos_f);
                self.scratch.free_i32(pair_off);
                self.scratch.free_i32(j0);
                self.scratch.free_i32(i_pair);
                self.scratch.free_i32(h);
                self.scratch.free_i32(p);
                self.scratch.free_i32(total_bytes);
                self.scratch.free_i32(cols);
                self.scratch.free_i32(rows);
                self.scratch.free_i32(dst);
                self.scratch.free_f64(head_dim_f);
                self.scratch.free_f64(log_theta);
                self.scratch.free_f64(theta);
                self.scratch.free_i32(start_pos);
                self.scratch.free_i32(half);
                self.scratch.free_i32(head_dim);
                self.scratch.free_i32(n_heads);
                self.scratch.free_i32(x);
            }
            "append_rows" => {
                // matrix.append_rows(base: Matrix, extra: Matrix) -> Matrix
                // Row-wise concat. base.cols is taken as the output cols
                // (Almide already guarantees same-width matrices at the
                // call-site — we don't need a runtime reshape).
                let base = self.scratch.alloc_i32();
                let extra = self.scratch.alloc_i32();
                let r_base = self.scratch.alloc_i32();
                let r_extra = self.scratch.alloc_i32();
                let cols_l = self.scratch.alloc_i32();
                let r_total = self.scratch.alloc_i32();
                let dst = self.scratch.alloc_i32();
                let row_bytes = self.scratch.alloc_i32();
                let base_bytes = self.scratch.alloc_i32();
                let extra_bytes = self.scratch.alloc_i32();

                self.emit_expr(&args[0]);
                wasm!(self.func, { local_set(base); });
                self.emit_expr(&args[1]);
                wasm!(self.func, { local_set(extra); });

                wasm!(self.func, {
                    local_get(base); i32_load(0); local_set(r_base);
                    local_get(extra); i32_load(0); local_set(r_extra);
                    local_get(base); i32_load(4); local_set(cols_l);
                    local_get(r_base); local_get(r_extra); i32_add; local_set(r_total);
                    // row_bytes = cols * 8
                    local_get(cols_l); i32_const(8); i32_mul; local_set(row_bytes);
                    local_get(r_base); local_get(row_bytes); i32_mul; local_set(base_bytes);
                    local_get(r_extra); local_get(row_bytes); i32_mul; local_set(extra_bytes);
                    // alloc = 8 + total rows * cols * 8
                    local_get(r_total); local_get(row_bytes); i32_mul;
                    i32_const(self.emitter.layout_reg.fixed_offset(super::engine::layout::LIST, super::engine::layout::list::DATA) as i32); i32_add;
                    call(self.emitter.rt.alloc); local_set(dst);
                    local_get(dst); local_get(r_total); i32_store(0);
                    local_get(dst); local_get(cols_l); i32_store(4);
                    // memory.copy(dst+8, base+8, base_bytes)
                    local_get(dst); i32_const(self.emitter.layout_reg.fixed_offset(super::engine::layout::LIST, super::engine::layout::list::DATA) as i32); i32_add;
                    local_get(base); i32_const(self.emitter.layout_reg.fixed_offset(super::engine::layout::LIST, super::engine::layout::list::DATA) as i32); i32_add;
                    local_get(base_bytes);
                    memory_copy;
                    // memory.copy(dst+8+base_bytes, extra+8, extra_bytes)
                    local_get(dst); i32_const(self.emitter.layout_reg.fixed_offset(super::engine::layout::LIST, super::engine::layout::list::DATA) as i32); i32_add; local_get(base_bytes); i32_add;
                    local_get(extra); i32_const(self.emitter.layout_reg.fixed_offset(super::engine::layout::LIST, super::engine::layout::list::DATA) as i32); i32_add;
                    local_get(extra_bytes);
                    memory_copy;
                    local_get(dst);
                });

                self.scratch.free_i32(extra_bytes);
                self.scratch.free_i32(base_bytes);
                self.scratch.free_i32(row_bytes);
                self.scratch.free_i32(dst);
                self.scratch.free_i32(r_total);
                self.scratch.free_i32(cols_l);
                self.scratch.free_i32(r_extra);
                self.scratch.free_i32(r_base);
                self.scratch.free_i32(extra);
                self.scratch.free_i32(base);
            }
            _ => return false,
        }
        true
    }
}
