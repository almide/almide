impl FuncCompiler<'_> {
    /// Matrix dispatch group 4 (chained from `emit_matrix_call`).
    /// Disjoint arm subset; returns true iff this group handled `method`.
    pub(super) fn emit_matrix_call_g4(&mut self, method: &str, args: &[IrExpr]) -> bool {
        match method {
            "from_q1_0_bytes" => {
                // matrix.from_q1_0_bytes(data: Bytes, offset: Int, rows: Int, cols: Int) -> Matrix
                //
                // Q1_0 block layout: 18 bytes per 128 weights —
                //   bytes[0..2]  = fp16 scale (little-endian)
                //   bytes[2..18] = 16 bytes of sign bits, LSB-first.
                // Sign bit mapping: 0 → -scale, 1 → +scale.
                //
                // The fp16 → f32 conversion inlined below handles normal
                // values and exact zeros. Subnormal fp16 scales never
                // occur in practice for Q1_0 (the calibration pipeline
                // always lands in the normal range); we treat any
                // `exp == 0` as zero to keep the loop branch-free.
                let data = self.scratch.alloc_i32();
                let off = self.scratch.alloc_i32();
                let rows = self.scratch.alloc_i32();
                let cols = self.scratch.alloc_i32();
                let dst = self.scratch.alloc_i32();
                let total = self.scratch.alloc_i32();
                let k = self.scratch.alloc_i32();
                let data_off = self.scratch.alloc_i32();
                let block_start = self.scratch.alloc_i32();
                let bits_start = self.scratch.alloc_i32();
                let scale_raw = self.scratch.alloc_i32();
                let sign = self.scratch.alloc_i32();
                let expv = self.scratch.alloc_i32();
                let mant = self.scratch.alloc_i32();
                let f32bits = self.scratch.alloc_i32();
                let byte_idx = self.scratch.alloc_i32();
                let bit_off = self.scratch.alloc_i32();
                let bit = self.scratch.alloc_i32();
                let scale = self.scratch.alloc_f64();
                let neg_scale = self.scratch.alloc_f64();

                // Evaluate args.
                self.emit_expr(&args[0]);
                wasm!(self.func, { local_set(data); });
                self.emit_expr(&args[1]);
                wasm!(self.func, { i32_wrap_i64; local_set(off); });
                self.emit_expr(&args[2]);
                wasm!(self.func, { i32_wrap_i64; local_set(rows); });
                self.emit_expr(&args[3]);
                wasm!(self.func, { i32_wrap_i64; local_set(cols); });

                // total = rows * cols; dst = alloc(8 + total*8); write header.
                wasm!(self.func, {
                    local_get(rows); local_get(cols); i32_mul; local_set(total);
                    local_get(total); i32_const(8); i32_mul;
                    i32_const(self.emitter.layout_reg.fixed_offset(super::engine::layout::LIST, super::engine::layout::list::DATA) as i32); i32_add;
                    call(self.emitter.rt.alloc); local_set(dst);
                    local_get(dst); local_get(rows); i32_store(0);
                    local_get(dst); local_get(cols); i32_store(4);
                    // data_off = data_ptr + 4 (skip bytes-len header) + offset
                    local_get(data); i32_const(self.emitter.layout_reg.fixed_offset(super::engine::layout::LIST, super::engine::layout::list::DATA) as i32); i32_add;
                    local_get(off); i32_add;
                    local_set(data_off);
                    i32_const(0); local_set(k);
                    block_empty; loop_empty;
                      local_get(k); local_get(total); i32_ge_u; br_if(1);
                      // On a fresh 128-block boundary, reload scale & neg_scale.
                      local_get(k); i32_const(127); i32_and; i32_eqz;
                      if_empty;
                        // block_start = data_off + (k / 128) * 18
                        local_get(data_off);
                        local_get(k); i32_const(7); i32_shr_u;
                        i32_const(18); i32_mul;
                        i32_add;
                        local_set(block_start);
                        // scale_raw (u16 LE) = byte[0] | byte[1] << 8
                        local_get(block_start); i32_load8_u(0);
                        local_get(block_start); i32_load8_u(1);
                        i32_const(8); i32_shl;
                        i32_or;
                        local_set(scale_raw);
                        // Decompose fp16 bits into sign / exp / mantissa.
                        local_get(scale_raw); i32_const(15); i32_shr_u;
                        i32_const(1); i32_and;
                        local_set(sign);
                        local_get(scale_raw); i32_const(10); i32_shr_u;
                        i32_const(31); i32_and;
                        local_set(expv);
                        local_get(scale_raw); i32_const(1023); i32_and;
                        local_set(mant);
                        // f32bits = (sign << 31) | ((exp + 112) << 23) | (mant << 13)
                        local_get(sign); i32_const(31); i32_shl;
                        local_get(expv); i32_const(112); i32_add; i32_const(23); i32_shl;
                        i32_or;
                        local_get(mant); i32_const(13); i32_shl;
                        i32_or;
                        local_set(f32bits);
                        // If exp == 0 force f32bits to zero (sign-only).
                        local_get(expv); i32_eqz;
                        if_empty;
                          local_get(sign); i32_const(31); i32_shl; local_set(f32bits);
                        end;
                        // scale = f64 from the reconstructed f32 bits.
                        local_get(f32bits); f32_reinterpret_i32; f64_promote_f32;
                        local_set(scale);
                        f64_const(0.0); local_get(scale); f64_sub;
                        local_set(neg_scale);
                        local_get(block_start); i32_const(2); i32_add;
                        local_set(bits_start);
                      end;
                      // byte_idx = bits_start + ((k & 127) >> 3) = bits_start + ((k >> 3) & 15)
                      local_get(bits_start);
                      local_get(k); i32_const(3); i32_shr_u; i32_const(15); i32_and;
                      i32_add;
                      local_set(byte_idx);
                      // bit_off = k & 7
                      local_get(k); i32_const(7); i32_and; local_set(bit_off);
                      // bit = (byte >> bit_off) & 1
                      local_get(byte_idx); i32_load8_u(0);
                      local_get(bit_off); i32_shr_u;
                      i32_const(1); i32_and;
                      local_set(bit);
                      // dst[8 + k*8] = bit == 1 ? scale : neg_scale
                      local_get(dst); i32_const(self.emitter.layout_reg.fixed_offset(super::engine::layout::LIST, super::engine::layout::list::DATA) as i32); i32_add;
                      local_get(k); i32_const(8); i32_mul; i32_add;
                      local_get(bit);
                      if_f64;
                        local_get(scale);
                      else_;
                        local_get(neg_scale);
                      end;
                      f64_store(0);
                      local_get(k); i32_const(1); i32_add; local_set(k);
                      br(0);
                    end; end;
                    local_get(dst);
                });
                self.scratch.free_f64(neg_scale);
                self.scratch.free_f64(scale);
                self.scratch.free_i32(bit);
                self.scratch.free_i32(bit_off);
                self.scratch.free_i32(byte_idx);
                self.scratch.free_i32(f32bits);
                self.scratch.free_i32(mant);
                self.scratch.free_i32(expv);
                self.scratch.free_i32(sign);
                self.scratch.free_i32(scale_raw);
                self.scratch.free_i32(bits_start);
                self.scratch.free_i32(block_start);
                self.scratch.free_i32(data_off);
                self.scratch.free_i32(k);
                self.scratch.free_i32(total);
                self.scratch.free_i32(dst);
                self.scratch.free_i32(cols);
                self.scratch.free_i32(rows);
                self.scratch.free_i32(off);
                self.scratch.free_i32(data);
            }
            "linear_row" | "linear_row_no_bias" => {
                // y[i,j] = sum_k x[i,k] * weight[j,k] + bias[j]
                let with_bias = method == "linear_row";
                let x = self.scratch.alloc_i32();
                let w = self.scratch.alloc_i32();
                let b = self.scratch.alloc_i32();
                let dst = self.scratch.alloc_i32();
                let xr = self.scratch.alloc_i32();
                let wr = self.scratch.alloc_i32();
                let nin = self.scratch.alloc_i32();
                let i = self.scratch.alloc_i32();
                let j = self.scratch.alloc_i32();
                let k = self.scratch.alloc_i32();
                let s = self.scratch.alloc_f64();
                self.emit_expr(&args[0]);
                wasm!(self.func, { local_set(x); });
                self.emit_expr(&args[1]);
                wasm!(self.func, { local_set(w); });
                if with_bias {
                    self.emit_expr(&args[2]);
                    wasm!(self.func, { local_set(b); });
                }
                wasm!(self.func, {
                    local_get(x); i32_load(0); local_set(xr);
                    local_get(x); i32_load(4); local_set(nin);
                    local_get(w); i32_load(0); local_set(wr);
                    local_get(xr); local_get(wr); i32_mul; i32_const(8); i32_mul;
                    i32_const(self.emitter.layout_reg.fixed_offset(super::engine::layout::LIST, super::engine::layout::list::DATA) as i32); i32_add;
                    call(self.emitter.rt.alloc); local_set(dst);
                    local_get(dst); local_get(xr); i32_store(0);
                    local_get(dst); local_get(wr); i32_store(4);
                    i32_const(0); local_set(i);
                    block_empty; loop_empty;
                      local_get(i); local_get(xr); i32_ge_u; br_if(1);
                      i32_const(0); local_set(j);
                      block_empty; loop_empty;
                        local_get(j); local_get(wr); i32_ge_u; br_if(1);
                        f64_const(0.0); local_set(s);
                        i32_const(0); local_set(k);
                        block_empty; loop_empty;
                          local_get(k); local_get(nin); i32_ge_u; br_if(1);
                          // x[i,k]
                          local_get(x); i32_const(self.emitter.layout_reg.fixed_offset(super::engine::layout::LIST, super::engine::layout::list::DATA) as i32); i32_add;
                          local_get(i); local_get(nin); i32_mul; local_get(k); i32_add;
                          i32_const(8); i32_mul; i32_add; f64_load(0);
                          // weight[j,k]
                          local_get(w); i32_const(self.emitter.layout_reg.fixed_offset(super::engine::layout::LIST, super::engine::layout::list::DATA) as i32); i32_add;
                          local_get(j); local_get(nin); i32_mul; local_get(k); i32_add;
                          i32_const(8); i32_mul; i32_add; f64_load(0);
                          f64_mul; local_get(s); f64_add; local_set(s);
                          local_get(k); i32_const(1); i32_add; local_set(k);
                          br(0);
                        end; end;
                        // dst[i,j] = s + bias[j] (if with_bias)
                        local_get(dst); i32_const(self.emitter.layout_reg.fixed_offset(super::engine::layout::LIST, super::engine::layout::list::DATA) as i32); i32_add;
                        local_get(i); local_get(wr); i32_mul; local_get(j); i32_add;
                        i32_const(8); i32_mul; i32_add;
                        local_get(s);
                });
                if with_bias {
                    wasm!(self.func, {
                        local_get(b); i32_const(self.emitter.layout_reg.fixed_offset(super::engine::layout::LIST, super::engine::layout::list::DATA) as i32); i32_add;
                        local_get(j); i32_const(8); i32_mul; i32_add;
                        f64_load(0); f64_add;
                    });
                }
                wasm!(self.func, {
                        f64_store(0);
                        local_get(j); i32_const(1); i32_add; local_set(j);
                        br(0);
                      end; end;
                      local_get(i); i32_const(1); i32_add; local_set(i);
                      br(0);
                    end; end;
                    local_get(dst);
                });
                self.scratch.free_f64(s);
                self.scratch.free_i32(k);
                self.scratch.free_i32(j);
                self.scratch.free_i32(i);
                self.scratch.free_i32(nin);
                self.scratch.free_i32(wr);
                self.scratch.free_i32(xr);
                self.scratch.free_i32(dst);
                self.scratch.free_i32(b);
                self.scratch.free_i32(w);
                self.scratch.free_i32(x);
            }
            "causal_mask_add" => {
                // matrix.causal_mask_add(m, mask_val) → Matrix; add mask_val where j > i
                let m = self.scratch.alloc_i32();
                let dst = self.scratch.alloc_i32();
                let rows = self.scratch.alloc_i32();
                let cols = self.scratch.alloc_i32();
                let r = self.scratch.alloc_i32();
                let c = self.scratch.alloc_i32();
                let mv = self.scratch.alloc_f64();
                self.emit_expr(&args[0]);
                wasm!(self.func, { local_set(m); });
                self.emit_expr(&args[1]);
                wasm!(self.func, {
                    local_set(mv);
                    local_get(m); i32_load(0); local_set(rows);
                    local_get(m); i32_load(4); local_set(cols);
                    local_get(rows); local_get(cols); i32_mul; i32_const(8); i32_mul;
                    i32_const(self.emitter.layout_reg.fixed_offset(super::engine::layout::LIST, super::engine::layout::list::DATA) as i32); i32_add;
                    call(self.emitter.rt.alloc); local_set(dst);
                    local_get(dst); local_get(rows); i32_store(0);
                    local_get(dst); local_get(cols); i32_store(4);
                    i32_const(0); local_set(r);
                    block_empty; loop_empty;
                      local_get(r); local_get(rows); i32_ge_u; br_if(1);
                      i32_const(0); local_set(c);
                      block_empty; loop_empty;
                        local_get(c); local_get(cols); i32_ge_u; br_if(1);
                        local_get(dst); i32_const(self.emitter.layout_reg.fixed_offset(super::engine::layout::LIST, super::engine::layout::list::DATA) as i32); i32_add;
                        local_get(r); local_get(cols); i32_mul; local_get(c); i32_add;
                        i32_const(8); i32_mul; i32_add;
                        local_get(m); i32_const(self.emitter.layout_reg.fixed_offset(super::engine::layout::LIST, super::engine::layout::list::DATA) as i32); i32_add;
                        local_get(r); local_get(cols); i32_mul; local_get(c); i32_add;
                        i32_const(8); i32_mul; i32_add; f64_load(0);
                        local_get(c); local_get(r); i32_gt_u;
                        if_f64; local_get(mv); else_; f64_const(0.0); end;
                        f64_add;
                        f64_store(0);
                        local_get(c); i32_const(1); i32_add; local_set(c);
                        br(0);
                      end; end;
                      local_get(r); i32_const(1); i32_add; local_set(r);
                      br(0);
                    end; end;
                    local_get(dst);
                });
                self.scratch.free_f64(mv);
                self.scratch.free_i32(c);
                self.scratch.free_i32(r);
                self.scratch.free_i32(cols);
                self.scratch.free_i32(rows);
                self.scratch.free_i32(dst);
                self.scratch.free_i32(m);
            }
            "multi_head_attention" | "masked_multi_head_attention" => {
                // Per-head loop: scores[i,j] = (sum_k q[i,col0+k]*k[j,col0+k]) * scale
                //                + (-1e9 if causal && j>i else 0)
                //                softmax row → weights @ v columns col0..col1 → out
                let causal = method == "masked_multi_head_attention";
                let q = self.scratch.alloc_i32();
                let kk = self.scratch.alloc_i32();
                let vv = self.scratch.alloc_i32();
                let dst = self.scratch.alloc_i32();
                let scores = self.scratch.alloc_i32();
                let nh = self.scratch.alloc_i32();
                let sq = self.scratch.alloc_i32();
                let sk = self.scratch.alloc_i32();
                let dm = self.scratch.alloc_i32();
                let dh = self.scratch.alloc_i32();
                let h = self.scratch.alloc_i32();
                let i = self.scratch.alloc_i32();
                let j = self.scratch.alloc_i32();
                let kki = self.scratch.alloc_i32();
                let col0 = self.scratch.alloc_i32();
                let scale = self.scratch.alloc_f64();
                let acc = self.scratch.alloc_f64();
                let max = self.scratch.alloc_f64();
                let sum = self.scratch.alloc_f64();
                let v = self.scratch.alloc_f64();
                let w = self.scratch.alloc_f64();
                self.emit_expr(&args[0]);
                wasm!(self.func, { local_set(q); });
                self.emit_expr(&args[1]);
                wasm!(self.func, { local_set(kk); });
                self.emit_expr(&args[2]);
                wasm!(self.func, { local_set(vv); });
                self.emit_expr(&args[3]);
                wasm!(self.func, {
                    i32_wrap_i64; local_set(nh);
                    local_get(q); i32_load(0); local_set(sq);
                    local_get(kk); i32_load(0); local_set(sk);
                    local_get(q); i32_load(4); local_set(dm);
                    local_get(dm); local_get(nh); i32_div_u; local_set(dh);
                    // scale = 1/sqrt(dh)
                    f64_const(1.0);
                    local_get(dh); f64_convert_i32_u; f64_sqrt;
                    f64_div; local_set(scale);
                    // Alloc dst (sq, dm), zero
                    local_get(sq); local_get(dm); i32_mul; i32_const(8); i32_mul;
                    i32_const(self.emitter.layout_reg.fixed_offset(super::engine::layout::LIST, super::engine::layout::list::DATA) as i32); i32_add;
                    call(self.emitter.rt.alloc); local_set(dst);
                    local_get(dst); local_get(sq); i32_store(0);
                    local_get(dst); local_get(dm); i32_store(4);
                    local_get(dst); i32_const(self.emitter.layout_reg.fixed_offset(super::engine::layout::LIST, super::engine::layout::list::DATA) as i32); i32_add;
                    i32_const(0);
                    local_get(sq); local_get(dm); i32_mul; i32_const(8); i32_mul;
                    memory_fill;
                    // Alloc scores (sq, sk) reused per head
                    local_get(sq); local_get(sk); i32_mul; i32_const(8); i32_mul;
                    call(self.emitter.rt.alloc); local_set(scores);
                    // Per head
                    i32_const(0); local_set(h);
                    block_empty; loop_empty;
                      local_get(h); local_get(nh); i32_ge_u; br_if(1);
                      local_get(h); local_get(dh); i32_mul; local_set(col0);
                      // scores[i,j] = (sum_k q[i,col0+k]*k[j,col0+k]) * scale + mask
                      i32_const(0); local_set(i);
                      block_empty; loop_empty;
                        local_get(i); local_get(sq); i32_ge_u; br_if(1);
                        i32_const(0); local_set(j);
                        block_empty; loop_empty;
                          local_get(j); local_get(sk); i32_ge_u; br_if(1);
                          f64_const(0.0); local_set(acc);
                          i32_const(0); local_set(kki);
                          block_empty; loop_empty;
                            local_get(kki); local_get(dh); i32_ge_u; br_if(1);
                            // q[i, col0+kki]
                            local_get(q); i32_const(self.emitter.layout_reg.fixed_offset(super::engine::layout::LIST, super::engine::layout::list::DATA) as i32); i32_add;
                            local_get(i); local_get(dm); i32_mul;
                            local_get(col0); i32_add; local_get(kki); i32_add;
                            i32_const(8); i32_mul; i32_add; f64_load(0);
                            // k[j, col0+kki]
                            local_get(kk); i32_const(self.emitter.layout_reg.fixed_offset(super::engine::layout::LIST, super::engine::layout::list::DATA) as i32); i32_add;
                            local_get(j); local_get(dm); i32_mul;
                            local_get(col0); i32_add; local_get(kki); i32_add;
                            i32_const(8); i32_mul; i32_add; f64_load(0);
                            f64_mul; local_get(acc); f64_add; local_set(acc);
                            local_get(kki); i32_const(1); i32_add; local_set(kki);
                            br(0);
                          end; end;
                          local_get(acc); local_get(scale); f64_mul; local_set(acc);
                });
                if causal {
                    wasm!(self.func, {
                          // KV-cache-aware causal mask: query row i (one of
                          // the sq new tokens) attends to key row j iff
                          // j <= (sk - sq) + i. When sq == sk this reduces
                          // to j <= i. When sq < sk (single-token gen step
                          // with cached K of length sk - sq) the cached
                          // prefix is always visible, and the new query
                          // sees its own key plus everything before.
                          local_get(j);
                          local_get(sk); local_get(sq); i32_sub;
                          local_get(i); i32_add;
                          i32_gt_u;
                          if_f64; f64_const(-1.0e9); else_; f64_const(0.0); end;
                          local_get(acc); f64_add; local_set(acc);
                    });
                }
                wasm!(self.func, {
                          // scores[i*sk + j] = acc
                          local_get(scores);
                          local_get(i); local_get(sk); i32_mul; local_get(j); i32_add;
                          i32_const(8); i32_mul; i32_add;
                          local_get(acc); f64_store(0);
                          local_get(j); i32_const(1); i32_add; local_set(j);
                          br(0);
                        end; end;
                        // Softmax row i — NaN/Inf-defensive.
                        // Initialise max with scores[i*sk] (instead of -1e308 sentinel)
                        // so a single NaN can't poison the whole row via
                        // f64_gt-returns-false-for-NaN.
                        local_get(scores);
                        local_get(i); local_get(sk); i32_mul;
                        i32_const(8); i32_mul; i32_add; f64_load(0);
                        local_set(max);
                        i32_const(1); local_set(j);
                        block_empty; loop_empty;
                          local_get(j); local_get(sk); i32_ge_u; br_if(1);
                          local_get(scores);
                          local_get(i); local_get(sk); i32_mul; local_get(j); i32_add;
                          i32_const(8); i32_mul; i32_add; f64_load(0); local_set(v);
                          local_get(v); local_get(max); f64_gt;
                          if_empty; local_get(v); local_set(max); end;
                          local_get(j); i32_const(1); i32_add; local_set(j);
                          br(0);
                        end; end;
                        f64_const(0.0); local_set(sum);
                        i32_const(0); local_set(j);
                        block_empty; loop_empty;
                          local_get(j); local_get(sk); i32_ge_u; br_if(1);
                          local_get(scores);
                          local_get(i); local_get(sk); i32_mul; local_get(j); i32_add;
                          i32_const(8); i32_mul; i32_add;
                          local_get(scores);
                          local_get(i); local_get(sk); i32_mul; local_get(j); i32_add;
                          i32_const(8); i32_mul; i32_add; f64_load(0);
                          local_get(max); f64_sub; call(self.emitter.rt.math_exp); local_set(v);
                          local_get(v); f64_store(0);
                          local_get(sum); local_get(v); f64_add; local_set(sum);
                          local_get(j); i32_const(1); i32_add; local_set(j);
                          br(0);
                        end; end;
                        // Sum guard: if sum is non-positive or non-finite, fall back
                        // to uniform distribution over sk (avoid 0/0 → NaN cascade).
                        local_get(sum); f64_const(0.0); f64_le;
                        local_get(sum); local_get(sum); f64_ne;  // NaN check
                        i32_or;
                        if_empty;
                          local_get(sk); f64_convert_i32_u; local_set(sum);
                          // Re-fill scores row with 1.0 (will become 1/sk after div)
                          i32_const(0); local_set(j);
                          block_empty; loop_empty;
                            local_get(j); local_get(sk); i32_ge_u; br_if(1);
                            local_get(scores);
                            local_get(i); local_get(sk); i32_mul; local_get(j); i32_add;
                            i32_const(8); i32_mul; i32_add;
                            f64_const(1.0); f64_store(0);
                            local_get(j); i32_const(1); i32_add; local_set(j);
                            br(0);
                          end; end;
                        end;
                        // dst[i, col0..col1] += (scores[i,j] / sum) * v[j, col0..col1]
                        i32_const(0); local_set(j);
                        block_empty; loop_empty;
                          local_get(j); local_get(sk); i32_ge_u; br_if(1);
                          local_get(scores);
                          local_get(i); local_get(sk); i32_mul; local_get(j); i32_add;
                          i32_const(8); i32_mul; i32_add; f64_load(0);
                          local_get(sum); f64_div; local_set(w);
                          // Add w * v[j, col0+kki] to dst[i, col0+kki]
                          i32_const(0); local_set(kki);
                          block_empty; loop_empty;
                            local_get(kki); local_get(dh); i32_ge_u; br_if(1);
                            local_get(dst); i32_const(self.emitter.layout_reg.fixed_offset(super::engine::layout::LIST, super::engine::layout::list::DATA) as i32); i32_add;
                            local_get(i); local_get(dm); i32_mul;
                            local_get(col0); i32_add; local_get(kki); i32_add;
                            i32_const(8); i32_mul; i32_add;
                            local_get(dst); i32_const(self.emitter.layout_reg.fixed_offset(super::engine::layout::LIST, super::engine::layout::list::DATA) as i32); i32_add;
                            local_get(i); local_get(dm); i32_mul;
                            local_get(col0); i32_add; local_get(kki); i32_add;
                            i32_const(8); i32_mul; i32_add; f64_load(0);
                            local_get(vv); i32_const(self.emitter.layout_reg.fixed_offset(super::engine::layout::LIST, super::engine::layout::list::DATA) as i32); i32_add;
                            local_get(j); local_get(dm); i32_mul;
                            local_get(col0); i32_add; local_get(kki); i32_add;
                            i32_const(8); i32_mul; i32_add; f64_load(0);
                            local_get(w); f64_mul; f64_add;
                            f64_store(0);
                            local_get(kki); i32_const(1); i32_add; local_set(kki);
                            br(0);
                          end; end;
                          local_get(j); i32_const(1); i32_add; local_set(j);
                          br(0);
                        end; end;
                        local_get(i); i32_const(1); i32_add; local_set(i);
                        br(0);
                      end; end;
                      local_get(h); i32_const(1); i32_add; local_set(h);
                      br(0);
                    end; end;
                    local_get(dst);
                });
                self.scratch.free_f64(w);
                self.scratch.free_f64(v);
                self.scratch.free_f64(sum);
                self.scratch.free_f64(max);
                self.scratch.free_f64(acc);
                self.scratch.free_f64(scale);
                self.scratch.free_i32(col0);
                self.scratch.free_i32(kki);
                self.scratch.free_i32(j);
                self.scratch.free_i32(i);
                self.scratch.free_i32(h);
                self.scratch.free_i32(dh);
                self.scratch.free_i32(dm);
                self.scratch.free_i32(sk);
                self.scratch.free_i32(sq);
                self.scratch.free_i32(nh);
                self.scratch.free_i32(scores);
                self.scratch.free_i32(dst);
                self.scratch.free_i32(vv);
                self.scratch.free_i32(kk);
                self.scratch.free_i32(q);
            }
            "to_bytes_f64_le" => {
                // Matrix → flat f64 LE bytes (row-major). Symmetric to from_bytes_f64_le.
                // Layout: matrix [rows:i32][cols:i32][f64...] → bytes [len:i32][f64...]
                // Just memcpy the data and prepend the length prefix.
                let m = self.scratch.alloc_i32();
                let total = self.scratch.alloc_i32();
                let bytes_len = self.scratch.alloc_i32();
                let dst = self.scratch.alloc_i32();
                self.emit_expr(&args[0]);
                wasm!(self.func, {
                    local_set(m);
                    local_get(m); i32_load(0); local_get(m); i32_load(4); i32_mul; local_set(total);
                    local_get(total); i32_const(8); i32_mul; local_set(bytes_len);
                    // alloc bytes buffer with header
                    local_get(bytes_len); call(self.emitter.rt.string_alloc); local_set(dst);
                    // memcpy: dst+data_off ← m+data_off, bytes_len bytes
                    local_get(dst); i32_const(self.emitter.layout_reg.fixed_offset(super::engine::layout::LIST, super::engine::layout::list::DATA) as i32); i32_add;
                    local_get(m); i32_const(self.emitter.layout_reg.fixed_offset(super::engine::layout::LIST, super::engine::layout::list::DATA) as i32); i32_add;
                    local_get(bytes_len);
                    memory_copy;
                    local_get(dst);
                });
                self.scratch.free_i32(dst);
                self.scratch.free_i32(bytes_len);
                self.scratch.free_i32(total);
                self.scratch.free_i32(m);
            }
            "to_bytes_f32_le" => {
                // Matrix → flat f32 LE bytes. Each f64 is demoted to f32 element-wise.
                let m = self.scratch.alloc_i32();
                let total = self.scratch.alloc_i32();
                let bytes_len = self.scratch.alloc_i32();
                let dst = self.scratch.alloc_i32();
                let i = self.scratch.alloc_i32();
                let src_addr = self.scratch.alloc_i32();
                let dst_addr = self.scratch.alloc_i32();
                self.emit_expr(&args[0]);
                wasm!(self.func, {
                    local_set(m);
                    local_get(m); i32_load(0); local_get(m); i32_load(4); i32_mul; local_set(total);
                    local_get(total); i32_const(4); i32_mul; local_set(bytes_len);
                    local_get(bytes_len); call(self.emitter.rt.string_alloc); local_set(dst);
                    i32_const(0); local_set(i);
                    block_empty; loop_empty;
                      local_get(i); local_get(total); i32_ge_u; br_if(1);
                      local_get(m); i32_const(self.emitter.layout_reg.fixed_offset(super::engine::layout::LIST, super::engine::layout::list::DATA) as i32); i32_add; local_get(i); i32_const(8); i32_mul; i32_add; local_set(src_addr);
                      local_get(dst); i32_const(self.emitter.layout_reg.fixed_offset(super::engine::layout::LIST, super::engine::layout::list::DATA) as i32); i32_add; local_get(i); i32_const(4); i32_mul; i32_add; local_set(dst_addr);
                      local_get(dst_addr);
                      local_get(src_addr); f64_load(0); f32_demote_f64;
                      f32_store(0);
                      local_get(i); i32_const(1); i32_add; local_set(i);
                      br(0);
                    end; end;
                    local_get(dst);
                });
                self.scratch.free_i32(dst_addr);
                self.scratch.free_i32(src_addr);
                self.scratch.free_i32(i);
                self.scratch.free_i32(dst);
                self.scratch.free_i32(bytes_len);
                self.scratch.free_i32(total);
                self.scratch.free_i32(m);
            }
            "from_bytes_f64_le" => {
                // Construct Matrix from raw f64 LE bytes — fast path for JS-supplied data.
                let buf = self.scratch.alloc_i32();
                let off = self.scratch.alloc_i32();
                let r = self.scratch.alloc_i32();
                let c = self.scratch.alloc_i32();
                let dst = self.scratch.alloc_i32();
                let total = self.scratch.alloc_i32();
                self.emit_expr(&args[0]);
                wasm!(self.func, { local_set(buf); });
                self.emit_expr(&args[1]);
                wasm!(self.func, { i32_wrap_i64; local_set(off); });
                self.emit_expr(&args[2]);
                wasm!(self.func, { i32_wrap_i64; local_set(r); });
                self.emit_expr(&args[3]);
                wasm!(self.func, {
                    i32_wrap_i64; local_set(c);
                    local_get(r); local_get(c); i32_mul; local_set(total);
                    local_get(total); i32_const(8); i32_mul; i32_const(self.emitter.layout_reg.header_size(super::engine::layout::LIST) as i32); i32_add;
                    call(self.emitter.rt.alloc); local_set(dst);
                    local_get(dst); local_get(r); i32_store(0);
                    local_get(dst); local_get(c); i32_store(4);
                    // memcpy: dst+8 ← buf+4+off, total*8 bytes
                    local_get(dst); i32_const(self.emitter.layout_reg.fixed_offset(super::engine::layout::LIST, super::engine::layout::list::DATA) as i32); i32_add;
                    local_get(buf); i32_const(self.emitter.layout_reg.fixed_offset(super::engine::layout::LIST, super::engine::layout::list::DATA) as i32); i32_add; local_get(off); i32_add;
                    local_get(total); i32_const(8); i32_mul;
                    memory_copy;
                    local_get(dst);
                });
                self.scratch.free_i32(total);
                self.scratch.free_i32(dst);
                self.scratch.free_i32(c);
                self.scratch.free_i32(r);
                self.scratch.free_i32(off);
                self.scratch.free_i32(buf);
            }
            "from_bytes_f32_le" | "from_bytes_f16_le" => {
                let is_f16 = method == "from_bytes_f16_le";
                let buf = self.scratch.alloc_i32();
                let off = self.scratch.alloc_i32();
                let r = self.scratch.alloc_i32();
                let c = self.scratch.alloc_i32();
                let dst = self.scratch.alloc_i32();
                let total = self.scratch.alloc_i32();
                let i = self.scratch.alloc_i32();
                self.emit_expr(&args[0]);
                wasm!(self.func, { local_set(buf); });
                self.emit_expr(&args[1]);
                wasm!(self.func, { i32_wrap_i64; local_set(off); });
                self.emit_expr(&args[2]);
                wasm!(self.func, { i32_wrap_i64; local_set(r); });
                self.emit_expr(&args[3]);
                wasm!(self.func, {
                    i32_wrap_i64; local_set(c);
                    local_get(r); local_get(c); i32_mul; local_set(total);
                    local_get(total); i32_const(8); i32_mul; i32_const(self.emitter.layout_reg.header_size(super::engine::layout::LIST) as i32); i32_add;
                    call(self.emitter.rt.alloc); local_set(dst);
                    local_get(dst); local_get(r); i32_store(0);
                    local_get(dst); local_get(c); i32_store(4);
                    i32_const(0); local_set(i);
                    block_empty; loop_empty;
                      local_get(i); local_get(total); i32_ge_u; br_if(1);
                      // dst[data] + i * 8
                      local_get(dst); i32_const(self.emitter.layout_reg.fixed_offset(super::engine::layout::LIST, super::engine::layout::list::DATA) as i32); i32_add;
                      local_get(i); i32_const(8); i32_mul; i32_add;
                      // src = buf + 4 + off + i * elem_bytes
                      local_get(buf); i32_const(self.emitter.layout_reg.fixed_offset(super::engine::layout::LIST, super::engine::layout::list::DATA) as i32); i32_add; local_get(off); i32_add;
                });
                let elem_bytes: i32 = if is_f16 { 2 } else { 4 };
                wasm!(self.func, {
                      local_get(i); i32_const(elem_bytes); i32_mul; i32_add;
                });
                if is_f16 {
                    wasm!(self.func, {
                        i32_load16_u(0);
                        call(self.emitter.rt.bytes_f16_to_f64);
                    });
                } else {
                    wasm!(self.func, { f32_load(0); f64_promote_f32; });
                }
                wasm!(self.func, {
                      f64_store(0);
                      local_get(i); i32_const(1); i32_add; local_set(i);
                      br(0);
                    end; end;
                    local_get(dst);
                });
                self.scratch.free_i32(i);
                self.scratch.free_i32(total);
                self.scratch.free_i32(dst);
                self.scratch.free_i32(c);
                self.scratch.free_i32(r);
                self.scratch.free_i32(off);
                self.scratch.free_i32(buf);
            }
            "conv1d" => {
                // matrix.conv1d(input, weight, bias, kernel, stride, padding)
                // input: (T, in_ch), weight: (out_ch, in_ch*kernel), bias: (out_ch,)
                // output: (T_out, out_ch) where T_out = (T + 2P - K) / S + 1
                let inp = self.scratch.alloc_i32();
                let w = self.scratch.alloc_i32();
                let b = self.scratch.alloc_i32();
                let kk = self.scratch.alloc_i32();
                let st = self.scratch.alloc_i32();
                let pd = self.scratch.alloc_i32();
                let dst = self.scratch.alloc_i32();
                let tin = self.scratch.alloc_i32();
                let ich = self.scratch.alloc_i32();
                let och = self.scratch.alloc_i32();
                let tout = self.scratch.alloc_i32();
                let t = self.scratch.alloc_i32();
                let o = self.scratch.alloc_i32();
                let cc = self.scratch.alloc_i32();
                let ki = self.scratch.alloc_i32();
                let base = self.scratch.alloc_i32();
                let tp = self.scratch.alloc_i32();
                let tc = self.scratch.alloc_i32();
                let s = self.scratch.alloc_f64();
                self.emit_expr(&args[0]);
                wasm!(self.func, { local_set(inp); });
                self.emit_expr(&args[1]);
                wasm!(self.func, { local_set(w); });
                self.emit_expr(&args[2]);
                wasm!(self.func, { local_set(b); });
                self.emit_expr(&args[3]);
                wasm!(self.func, { i32_wrap_i64; local_set(kk); });
                self.emit_expr(&args[4]);
                wasm!(self.func, { i32_wrap_i64; local_set(st); });
                self.emit_expr(&args[5]);
                wasm!(self.func, {
                    i32_wrap_i64; local_set(pd);
                    local_get(inp); i32_load(0); local_set(tin);
                    local_get(inp); i32_load(4); local_set(ich);
                    local_get(w); i32_load(0); local_set(och);
                    // tout = (tin + 2*pd - kk) / st + 1
                    local_get(tin); local_get(pd); i32_const(2); i32_mul; i32_add;
                    local_get(kk); i32_sub; local_get(st); i32_div_u;
                    i32_const(1); i32_add; local_set(tout);
                    // Alloc dst (tout, och)
                    local_get(tout); local_get(och); i32_mul; i32_const(8); i32_mul;
                    i32_const(self.emitter.layout_reg.fixed_offset(super::engine::layout::LIST, super::engine::layout::list::DATA) as i32); i32_add;
                    call(self.emitter.rt.alloc); local_set(dst);
                    local_get(dst); local_get(tout); i32_store(0);
                    local_get(dst); local_get(och); i32_store(4);
                    // For each t, o:
                    i32_const(0); local_set(t);
                    block_empty; loop_empty;
                      local_get(t); local_get(tout); i32_ge_u; br_if(1);
                      local_get(t); local_get(st); i32_mul; local_set(base);
                      i32_const(0); local_set(o);
                      block_empty; loop_empty;
                        local_get(o); local_get(och); i32_ge_u; br_if(1);
                        // s = bias[o]
                        local_get(b); i32_const(self.emitter.layout_reg.fixed_offset(super::engine::layout::LIST, super::engine::layout::list::DATA) as i32); i32_add;
                        local_get(o); i32_const(8); i32_mul; i32_add;
                        f64_load(0); local_set(s);
                        i32_const(0); local_set(cc);
                        block_empty; loop_empty;
                          local_get(cc); local_get(ich); i32_ge_u; br_if(1);
                          i32_const(0); local_set(ki);
                          block_empty; loop_empty;
                            local_get(ki); local_get(kk); i32_ge_u; br_if(1);
                            // tp = base + ki; if (tp >= pd && tp < pd+tin): use
                            local_get(base); local_get(ki); i32_add; local_set(tp);
                            local_get(tp); local_get(pd); i32_ge_u;
                            local_get(tp); local_get(pd); local_get(tin); i32_add; i32_lt_u;
                            i32_and;
                            if_empty;
                              local_get(tp); local_get(pd); i32_sub; local_set(tc);
                              // weight[o][cc*kk + ki]
                              local_get(w); i32_const(self.emitter.layout_reg.fixed_offset(super::engine::layout::LIST, super::engine::layout::list::DATA) as i32); i32_add;
                              local_get(o); local_get(ich); i32_mul; local_get(kk); i32_mul;
                              local_get(cc); local_get(kk); i32_mul; i32_add;
                              local_get(ki); i32_add;
                              i32_const(8); i32_mul; i32_add; f64_load(0);
                              // input[tc][cc]
                              local_get(inp); i32_const(self.emitter.layout_reg.fixed_offset(super::engine::layout::LIST, super::engine::layout::list::DATA) as i32); i32_add;
                              local_get(tc); local_get(ich); i32_mul; local_get(cc); i32_add;
                              i32_const(8); i32_mul; i32_add; f64_load(0);
                              f64_mul; local_get(s); f64_add; local_set(s);
                            end;
                            local_get(ki); i32_const(1); i32_add; local_set(ki);
                            br(0);
                          end; end;
                          local_get(cc); i32_const(1); i32_add; local_set(cc);
                          br(0);
                        end; end;
                        // dst[t, o] = s
                        local_get(dst); i32_const(self.emitter.layout_reg.fixed_offset(super::engine::layout::LIST, super::engine::layout::list::DATA) as i32); i32_add;
                        local_get(t); local_get(och); i32_mul; local_get(o); i32_add;
                        i32_const(8); i32_mul; i32_add;
                        local_get(s); f64_store(0);
                        local_get(o); i32_const(1); i32_add; local_set(o);
                        br(0);
                      end; end;
                      local_get(t); i32_const(1); i32_add; local_set(t);
                      br(0);
                    end; end;
                    local_get(dst);
                });
                self.scratch.free_f64(s);
                self.scratch.free_i32(tc);
                self.scratch.free_i32(tp);
                self.scratch.free_i32(base);
                self.scratch.free_i32(ki);
                self.scratch.free_i32(cc);
                self.scratch.free_i32(o);
                self.scratch.free_i32(t);
                self.scratch.free_i32(tout);
                self.scratch.free_i32(och);
                self.scratch.free_i32(ich);
                self.scratch.free_i32(tin);
                self.scratch.free_i32(dst);
                self.scratch.free_i32(pd);
                self.scratch.free_i32(st);
                self.scratch.free_i32(kk);
                self.scratch.free_i32(b);
                self.scratch.free_i32(w);
                self.scratch.free_i32(inp);
            }
            _ => return false,
        }
        true
    }
}
