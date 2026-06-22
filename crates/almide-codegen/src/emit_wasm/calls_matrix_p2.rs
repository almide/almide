impl FuncCompiler<'_> {
    /// Matrix dispatch group 2 (chained from `emit_matrix_call`).
    /// Disjoint arm subset; returns true iff this group handled `method`.
    pub(super) fn emit_matrix_call_g2(&mut self, method: &str, args: &[IrExpr]) -> bool {
        match method {
            "neg" => {
                // matrix.neg(m): element-wise negation.
                let m = self.scratch.alloc_i32();
                let dst = self.scratch.alloc_i32();
                let total = self.scratch.alloc_i32();
                let i = self.scratch.alloc_i32();
                self.emit_expr(&args[0]);
                wasm!(self.func, {
                    local_set(m);
                    local_get(m); i32_load(0); local_get(m); i32_load(4); i32_mul; local_set(total);
                    local_get(total); i32_const(8); i32_mul; i32_const(self.emitter.layout_reg.header_size(super::engine::layout::LIST) as i32); i32_add;
                    call(self.emitter.rt.alloc); local_set(dst);
                    local_get(dst); local_get(m); i32_load(0); i32_store(0);
                    local_get(dst); local_get(m); i32_load(4); i32_store(4);
                    i32_const(0); local_set(i);
                    block_empty; loop_empty;
                      local_get(i); local_get(total); i32_ge_u; br_if(1);
                      local_get(dst); i32_const(self.emitter.layout_reg.fixed_offset(super::engine::layout::LIST, super::engine::layout::list::DATA) as i32); i32_add; local_get(i); i32_const(8); i32_mul; i32_add;
                      local_get(m); i32_const(self.emitter.layout_reg.fixed_offset(super::engine::layout::LIST, super::engine::layout::list::DATA) as i32); i32_add; local_get(i); i32_const(8); i32_mul; i32_add;
                      f64_load(0); f64_neg;
                      f64_store(0);
                      local_get(i); i32_const(1); i32_add; local_set(i);
                      br(0);
                    end; end;
                    local_get(dst);
                });
                self.scratch.free_i32(i);
                self.scratch.free_i32(total);
                self.scratch.free_i32(dst);
                self.scratch.free_i32(m);
            }
            "pow" => {
                // matrix.pow(m, exp): element-wise power via __float_pow runtime.
                let m = self.scratch.alloc_i32();
                let exp = self.scratch.alloc_f64();
                let dst = self.scratch.alloc_i32();
                let total = self.scratch.alloc_i32();
                let i = self.scratch.alloc_i32();
                let x = self.scratch.alloc_f64();
                let result = self.scratch.alloc_f64();
                self.emit_expr(&args[0]);
                wasm!(self.func, { local_set(m); });
                self.emit_expr(&args[1]);
                wasm!(self.func, {
                    local_set(exp);
                    local_get(m); i32_load(0); local_get(m); i32_load(4); i32_mul; local_set(total);
                    local_get(total); i32_const(8); i32_mul; i32_const(self.emitter.layout_reg.header_size(super::engine::layout::LIST) as i32); i32_add;
                    call(self.emitter.rt.alloc); local_set(dst);
                    local_get(dst); local_get(m); i32_load(0); i32_store(0);
                    local_get(dst); local_get(m); i32_load(4); i32_store(4);
                    i32_const(0); local_set(i);
                    block_empty; loop_empty;
                      local_get(i); local_get(total); i32_ge_u; br_if(1);
                      // Load x into a local so it's stable across the call.
                      local_get(m); i32_const(self.emitter.layout_reg.fixed_offset(super::engine::layout::LIST, super::engine::layout::list::DATA) as i32); i32_add; local_get(i); i32_const(8); i32_mul; i32_add; f64_load(0);
                      local_set(x);
                      // Compute pow(x, exp) via __float_pow runtime.
                      local_get(x); local_get(exp); call(self.emitter.rt.float_pow);
                      local_set(result);
                      // Store at dst+8+i*8
                      local_get(dst); i32_const(self.emitter.layout_reg.fixed_offset(super::engine::layout::LIST, super::engine::layout::list::DATA) as i32); i32_add; local_get(i); i32_const(8); i32_mul; i32_add;
                      local_get(result);
                      f64_store(0);
                      local_get(i); i32_const(1); i32_add; local_set(i);
                      br(0);
                    end; end;
                    local_get(dst);
                });
                self.scratch.free_f64(result);
                self.scratch.free_f64(x);
                self.scratch.free_i32(i);
                self.scratch.free_i32(total);
                self.scratch.free_i32(dst);
                self.scratch.free_f64(exp);
                self.scratch.free_i32(m);
            }
            "gelu" => {
                // matrix.gelu(m) → Matrix. tanh approximation.
                // y = 0.5 * x * (1 + tanh(sqrt(2/π) * (x + 0.044715 * x^3)))
                let m = self.scratch.alloc_i32();
                let dst = self.scratch.alloc_i32();
                let total = self.scratch.alloc_i32();
                let i = self.scratch.alloc_i32();
                let x = self.scratch.alloc_f64();
                self.emit_expr(&args[0]);
                wasm!(self.func, {
                    local_set(m);
                    local_get(m); i32_load(0); local_get(m); i32_load(4); i32_mul; local_set(total);
                    local_get(total); i32_const(8); i32_mul; i32_const(self.emitter.layout_reg.header_size(super::engine::layout::LIST) as i32); i32_add;
                    call(self.emitter.rt.alloc); local_set(dst);
                    local_get(dst); local_get(m); i32_load(0); i32_store(0);
                    local_get(dst); local_get(m); i32_load(4); i32_store(4);
                    i32_const(0); local_set(i);
                    block_empty; loop_empty;
                      local_get(i); local_get(total); i32_ge_u; br_if(1);
                      // x = m.data[i]
                      local_get(m); i32_const(self.emitter.layout_reg.fixed_offset(super::engine::layout::LIST, super::engine::layout::list::DATA) as i32); i32_add;
                      local_get(i); i32_const(8); i32_mul; i32_add;
                      f64_load(0); local_set(x);
                      // dst.data[i] = gelu(x)
                      local_get(dst); i32_const(self.emitter.layout_reg.fixed_offset(super::engine::layout::LIST, super::engine::layout::list::DATA) as i32); i32_add;
                      local_get(i); i32_const(8); i32_mul; i32_add;
                });
                // tanh(z) via runtime helper or expansion: tanh(z) = (e^(2z)-1)/(e^(2z)+1)
                // Compute inner = K * (x + 0.044715 * x^3)
                wasm!(self.func, {
                      f64_const(0.7978845608028654);
                      local_get(x);
                      f64_const(0.044715);
                      local_get(x); local_get(x); f64_mul;
                      local_get(x); f64_mul;
                      f64_mul;
                      f64_add;
                      f64_mul;
                      // stack: inner. Clamp to [-20, 20] to avoid exp overflow
                      // tanh saturates to ±1 outside this range, so no precision loss.
                      f64_const(20.0); f64_min;
                      f64_const(-20.0); f64_max;
                      // stack: clamped_inner. compute exp(2*inner)
                      f64_const(2.0); f64_mul;
                      call(self.emitter.rt.math_exp);
                      // stack: e2. compute (e2-1)/(e2+1) = tanh
                      // Need to duplicate e2 — use a scratch local
                });
                let e2 = self.scratch.alloc_f64();
                wasm!(self.func, {
                      local_set(e2);
                      local_get(e2); f64_const(1.0); f64_sub;
                      local_get(e2); f64_const(1.0); f64_add;
                      f64_div;
                      // stack: tanh_inner. compute 0.5 * x * (1 + tanh)
                      f64_const(1.0); f64_add;
                      local_get(x); f64_mul;
                      f64_const(0.5); f64_mul;
                      f64_store(0);
                      local_get(i); i32_const(1); i32_add; local_set(i);
                      br(0);
                    end; end;
                    local_get(dst);
                });
                self.scratch.free_f64(e2);
                self.scratch.free_f64(x);
                self.scratch.free_i32(i);
                self.scratch.free_i32(total);
                self.scratch.free_i32(dst);
                self.scratch.free_i32(m);
            }
            "softmax_rows" => {
                // matrix.softmax_rows(m) → Matrix. Numerically-stable row softmax.
                let m = self.scratch.alloc_i32();
                let dst = self.scratch.alloc_i32();
                let rows = self.scratch.alloc_i32();
                let cols = self.scratch.alloc_i32();
                let r = self.scratch.alloc_i32();
                let c = self.scratch.alloc_i32();
                let row_off = self.scratch.alloc_i32();
                let max = self.scratch.alloc_f64();
                let sum = self.scratch.alloc_f64();
                let v = self.scratch.alloc_f64();
                self.emit_expr(&args[0]);
                wasm!(self.func, {
                    local_set(m);
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
                      // row_off = 8 + r*cols*8 (offset to row r in data)
                      local_get(r); local_get(cols); i32_mul; i32_const(8); i32_mul;
                      i32_const(self.emitter.layout_reg.fixed_offset(super::engine::layout::LIST, super::engine::layout::list::DATA) as i32); i32_add; local_set(row_off);
                      // max = m[r, 0], scan row 1..cols (NaN-safe init).
                      local_get(m); local_get(row_off); i32_add;
                      f64_load(0); local_set(max);
                      i32_const(1); local_set(c);
                      block_empty; loop_empty;
                        local_get(c); local_get(cols); i32_ge_u; br_if(1);
                        local_get(m); local_get(row_off); i32_add;
                        local_get(c); i32_const(8); i32_mul; i32_add;
                        f64_load(0); local_set(v);
                        local_get(v); local_get(max); f64_gt;
                        if_empty; local_get(v); local_set(max); end;
                        local_get(c); i32_const(1); i32_add; local_set(c);
                        br(0);
                      end; end;
                      // exps + sum
                      f64_const(0.0); local_set(sum);
                      i32_const(0); local_set(c);
                      block_empty; loop_empty;
                        local_get(c); local_get(cols); i32_ge_u; br_if(1);
                        local_get(m); local_get(row_off); i32_add;
                        local_get(c); i32_const(8); i32_mul; i32_add;
                        f64_load(0); local_get(max); f64_sub;
                        call(self.emitter.rt.math_exp); local_set(v);
                        // dst[r, c] = v
                        local_get(dst); local_get(row_off); i32_add;
                        local_get(c); i32_const(8); i32_mul; i32_add;
                        local_get(v); f64_store(0);
                        local_get(sum); local_get(v); f64_add; local_set(sum);
                        local_get(c); i32_const(1); i32_add; local_set(c);
                        br(0);
                      end; end;
                      // Sum guard: NaN/Inf/zero fall-back to uniform.
                      local_get(sum); f64_const(0.0); f64_le;
                      local_get(sum); local_get(sum); f64_ne;
                      i32_or;
                      if_empty;
                        local_get(cols); f64_convert_i32_u; local_set(sum);
                        i32_const(0); local_set(c);
                        block_empty; loop_empty;
                          local_get(c); local_get(cols); i32_ge_u; br_if(1);
                          local_get(dst); local_get(row_off); i32_add;
                          local_get(c); i32_const(8); i32_mul; i32_add;
                          f64_const(1.0); f64_store(0);
                          local_get(c); i32_const(1); i32_add; local_set(c);
                          br(0);
                        end; end;
                      end;
                      // Normalize: dst[r,c] /= sum
                      i32_const(0); local_set(c);
                      block_empty; loop_empty;
                        local_get(c); local_get(cols); i32_ge_u; br_if(1);
                        local_get(dst); local_get(row_off); i32_add;
                        local_get(c); i32_const(8); i32_mul; i32_add;
                        local_get(dst); local_get(row_off); i32_add;
                        local_get(c); i32_const(8); i32_mul; i32_add;
                        f64_load(0);
                        local_get(sum); f64_div;
                        f64_store(0);
                        local_get(c); i32_const(1); i32_add; local_set(c);
                        br(0);
                      end; end;
                      local_get(r); i32_const(1); i32_add; local_set(r);
                      br(0);
                    end; end;
                    local_get(dst);
                });
                self.scratch.free_f64(v);
                self.scratch.free_f64(sum);
                self.scratch.free_f64(max);
                self.scratch.free_i32(row_off);
                self.scratch.free_i32(c);
                self.scratch.free_i32(r);
                self.scratch.free_i32(cols);
                self.scratch.free_i32(rows);
                self.scratch.free_i32(dst);
                self.scratch.free_i32(m);
            }
            "layer_norm_rows" => {
                // matrix.layer_norm_rows(m, gamma, beta, eps) → Matrix
                let m = self.scratch.alloc_i32();
                let gamma = self.scratch.alloc_i32();
                let beta = self.scratch.alloc_i32();
                let eps = self.scratch.alloc_f64();
                let dst = self.scratch.alloc_i32();
                let rows = self.scratch.alloc_i32();
                let cols = self.scratch.alloc_i32();
                let r = self.scratch.alloc_i32();
                let c = self.scratch.alloc_i32();
                let row_off = self.scratch.alloc_i32();
                let mean = self.scratch.alloc_f64();
                let var = self.scratch.alloc_f64();
                let inv = self.scratch.alloc_f64();
                let cnt = self.scratch.alloc_f64();
                let tmp = self.scratch.alloc_f64();
                self.emit_expr(&args[0]);
                wasm!(self.func, { local_set(m); });
                self.emit_expr(&args[1]);
                wasm!(self.func, { local_set(gamma); });
                self.emit_expr(&args[2]);
                wasm!(self.func, { local_set(beta); });
                self.emit_expr(&args[3]);
                wasm!(self.func, {
                    local_set(eps);
                    local_get(m); i32_load(0); local_set(rows);
                    local_get(m); i32_load(4); local_set(cols);
                    local_get(rows); local_get(cols); i32_mul; i32_const(8); i32_mul;
                    i32_const(self.emitter.layout_reg.fixed_offset(super::engine::layout::LIST, super::engine::layout::list::DATA) as i32); i32_add;
                    call(self.emitter.rt.alloc); local_set(dst);
                    local_get(dst); local_get(rows); i32_store(0);
                    local_get(dst); local_get(cols); i32_store(4);
                    // cnt = (f64) cols
                    local_get(cols); f64_convert_i32_u; local_set(cnt);
                    i32_const(0); local_set(r);
                    block_empty; loop_empty;
                      local_get(r); local_get(rows); i32_ge_u; br_if(1);
                      local_get(r); local_get(cols); i32_mul; i32_const(8); i32_mul;
                      i32_const(self.emitter.layout_reg.fixed_offset(super::engine::layout::LIST, super::engine::layout::list::DATA) as i32); i32_add; local_set(row_off);
                      // mean
                      f64_const(0.0); local_set(mean);
                      i32_const(0); local_set(c);
                      block_empty; loop_empty;
                        local_get(c); local_get(cols); i32_ge_u; br_if(1);
                        local_get(m); local_get(row_off); i32_add;
                        local_get(c); i32_const(8); i32_mul; i32_add;
                        f64_load(0);
                        local_get(mean); f64_add; local_set(mean);
                        local_get(c); i32_const(1); i32_add; local_set(c);
                        br(0);
                      end; end;
                      local_get(mean); local_get(cnt); f64_div; local_set(mean);
                      // var
                      f64_const(0.0); local_set(var);
                      i32_const(0); local_set(c);
                      block_empty; loop_empty;
                        local_get(c); local_get(cols); i32_ge_u; br_if(1);
                        local_get(m); local_get(row_off); i32_add;
                        local_get(c); i32_const(8); i32_mul; i32_add;
                        f64_load(0); local_get(mean); f64_sub; local_set(tmp);
                        local_get(tmp); local_get(tmp); f64_mul;
                        local_get(var); f64_add; local_set(var);
                        local_get(c); i32_const(1); i32_add; local_set(c);
                        br(0);
                      end; end;
                      local_get(var); local_get(cnt); f64_div; local_set(var);
                      // Clamp var to [0, +inf) and replace NaN with 0 so sqrt is stable.
                      local_get(var); local_get(var); f64_ne;  // NaN check
                      if_empty; f64_const(0.0); local_set(var); end;
                      local_get(var); f64_const(0.0); f64_lt;
                      if_empty; f64_const(0.0); local_set(var); end;
                      // inv = 1 / sqrt(var + eps)
                      f64_const(1.0);
                      local_get(var); local_get(eps); f64_add; f64_sqrt;
                      f64_div; local_set(inv);
                      // Apply (x - mean) * inv * gamma[c] + beta[c]
                      i32_const(0); local_set(c);
                      block_empty; loop_empty;
                        local_get(c); local_get(cols); i32_ge_u; br_if(1);
                        local_get(dst); local_get(row_off); i32_add;
                        local_get(c); i32_const(8); i32_mul; i32_add;
                        local_get(m); local_get(row_off); i32_add;
                        local_get(c); i32_const(8); i32_mul; i32_add;
                        f64_load(0); local_get(mean); f64_sub;
                        local_get(inv); f64_mul;
                        local_get(gamma); i32_const(self.emitter.layout_reg.fixed_offset(super::engine::layout::LIST, super::engine::layout::list::DATA) as i32); i32_add;
                        local_get(c); i32_const(8); i32_mul; i32_add;
                        f64_load(0); f64_mul;
                        local_get(beta); i32_const(self.emitter.layout_reg.fixed_offset(super::engine::layout::LIST, super::engine::layout::list::DATA) as i32); i32_add;
                        local_get(c); i32_const(8); i32_mul; i32_add;
                        f64_load(0); f64_add;
                        f64_store(0);
                        local_get(c); i32_const(1); i32_add; local_set(c);
                        br(0);
                      end; end;
                      local_get(r); i32_const(1); i32_add; local_set(r);
                      br(0);
                    end; end;
                    local_get(dst);
                });
                self.scratch.free_f64(tmp);
                self.scratch.free_f64(cnt);
                self.scratch.free_f64(inv);
                self.scratch.free_f64(var);
                self.scratch.free_f64(mean);
                self.scratch.free_i32(row_off);
                self.scratch.free_i32(c);
                self.scratch.free_i32(r);
                self.scratch.free_i32(cols);
                self.scratch.free_i32(rows);
                self.scratch.free_i32(dst);
                self.scratch.free_f64(eps);
                self.scratch.free_i32(beta);
                self.scratch.free_i32(gamma);
                self.scratch.free_i32(m);
            }
            "rms_norm_rows" => {
                // matrix.rms_norm_rows(m, gamma, eps) → Matrix.
                // For each row: rms = sqrt(mean(x²) + eps),
                // dst[r, c] = m[r, c] * (1/rms) * gamma[c].
                // Mirrors layer_norm_rows but drops mean + beta.
                let m = self.scratch.alloc_i32();
                let gamma = self.scratch.alloc_i32();
                let eps = self.scratch.alloc_f64();
                let dst = self.scratch.alloc_i32();
                let rows = self.scratch.alloc_i32();
                let cols = self.scratch.alloc_i32();
                let r = self.scratch.alloc_i32();
                let c = self.scratch.alloc_i32();
                let row_off = self.scratch.alloc_i32();
                let sq = self.scratch.alloc_f64();
                let inv = self.scratch.alloc_f64();
                let cnt = self.scratch.alloc_f64();
                let tmp = self.scratch.alloc_f64();
                self.emit_expr(&args[0]);
                wasm!(self.func, { local_set(m); });
                self.emit_expr(&args[1]);
                wasm!(self.func, { local_set(gamma); });
                self.emit_expr(&args[2]);
                wasm!(self.func, {
                    local_set(eps);
                    local_get(m); i32_load(0); local_set(rows);
                    local_get(m); i32_load(4); local_set(cols);
                    local_get(rows); local_get(cols); i32_mul; i32_const(8); i32_mul;
                    i32_const(self.emitter.layout_reg.fixed_offset(super::engine::layout::LIST, super::engine::layout::list::DATA) as i32); i32_add;
                    call(self.emitter.rt.alloc); local_set(dst);
                    local_get(dst); local_get(rows); i32_store(0);
                    local_get(dst); local_get(cols); i32_store(4);
                    local_get(cols); f64_convert_i32_u; local_set(cnt);
                    i32_const(0); local_set(r);
                    block_empty; loop_empty;
                      local_get(r); local_get(rows); i32_ge_u; br_if(1);
                      local_get(r); local_get(cols); i32_mul; i32_const(8); i32_mul;
                      i32_const(self.emitter.layout_reg.fixed_offset(super::engine::layout::LIST, super::engine::layout::list::DATA) as i32); i32_add; local_set(row_off);
                      // sq = Σ m[r, c]²
                      f64_const(0.0); local_set(sq);
                      i32_const(0); local_set(c);
                      block_empty; loop_empty;
                        local_get(c); local_get(cols); i32_ge_u; br_if(1);
                        local_get(m); local_get(row_off); i32_add;
                        local_get(c); i32_const(8); i32_mul; i32_add;
                        f64_load(0); local_set(tmp);
                        local_get(tmp); local_get(tmp); f64_mul;
                        local_get(sq); f64_add; local_set(sq);
                        local_get(c); i32_const(1); i32_add; local_set(c);
                        br(0);
                      end; end;
                      // NaN-guard on sq (Σx² may be NaN if any x is NaN).
                      local_get(sq); local_get(sq); f64_ne;
                      if_empty; f64_const(0.0); local_set(sq); end;
                      // inv = 1 / sqrt(sq / cnt + eps)
                      f64_const(1.0);
                      local_get(sq); local_get(cnt); f64_div;
                      local_get(eps); f64_add; f64_sqrt;
                      f64_div; local_set(inv);
                      // dst[r, c] = m[r, c] * inv * gamma[c]
                      i32_const(0); local_set(c);
                      block_empty; loop_empty;
                        local_get(c); local_get(cols); i32_ge_u; br_if(1);
                        local_get(dst); local_get(row_off); i32_add;
                        local_get(c); i32_const(8); i32_mul; i32_add;
                        local_get(m); local_get(row_off); i32_add;
                        local_get(c); i32_const(8); i32_mul; i32_add;
                        f64_load(0); local_get(inv); f64_mul;
                        local_get(gamma); i32_const(self.emitter.layout_reg.fixed_offset(super::engine::layout::LIST, super::engine::layout::list::DATA) as i32); i32_add;
                        local_get(c); i32_const(8); i32_mul; i32_add;
                        f64_load(0); f64_mul;
                        f64_store(0);
                        local_get(c); i32_const(1); i32_add; local_set(c);
                        br(0);
                      end; end;
                      local_get(r); i32_const(1); i32_add; local_set(r);
                      br(0);
                    end; end;
                    local_get(dst);
                });
                self.scratch.free_f64(tmp);
                self.scratch.free_f64(cnt);
                self.scratch.free_f64(inv);
                self.scratch.free_f64(sq);
                self.scratch.free_i32(row_off);
                self.scratch.free_i32(c);
                self.scratch.free_i32(r);
                self.scratch.free_i32(cols);
                self.scratch.free_i32(rows);
                self.scratch.free_i32(dst);
                self.scratch.free_f64(eps);
                self.scratch.free_i32(gamma);
                self.scratch.free_i32(m);
            }
            "swiglu_gate" => {
                // matrix.swiglu_gate(x, w_gate, w_up) → Matrix.
                // out[i, j] = g * sigmoid(g) * u, where
                //   g = Σ_k x[i, k] * w_gate[j, k]
                //   u = Σ_k x[i, k] * w_up[j, k]
                // Weight rows are output channels, columns are input dim —
                // mirrors `runtime/rs/src/matrix.rs::almide_rt_matrix_swiglu_gate`.
                let x = self.scratch.alloc_i32();
                let wg = self.scratch.alloc_i32();
                let wu = self.scratch.alloc_i32();
                let dst = self.scratch.alloc_i32();
                let r = self.scratch.alloc_i32();
                let d_in = self.scratch.alloc_i32();
                let d_out = self.scratch.alloc_i32();
                let i = self.scratch.alloc_i32();
                let j = self.scratch.alloc_i32();
                let k = self.scratch.alloc_i32();
                let g = self.scratch.alloc_f64();
                let u = self.scratch.alloc_f64();
                let tmp = self.scratch.alloc_f64();
                let sig = self.scratch.alloc_f64();
                self.emit_expr(&args[0]);
                wasm!(self.func, { local_set(x); });
                self.emit_expr(&args[1]);
                wasm!(self.func, { local_set(wg); });
                self.emit_expr(&args[2]);
                wasm!(self.func, {
                    local_set(wu);
                    local_get(x); i32_load(0); local_set(r);
                    local_get(x); i32_load(4); local_set(d_in);
                    local_get(wg); i32_load(0); local_set(d_out);
                    // alloc dst = r × d_out
                    local_get(r); local_get(d_out); i32_mul; i32_const(8); i32_mul;
                    i32_const(self.emitter.layout_reg.fixed_offset(super::engine::layout::LIST, super::engine::layout::list::DATA) as i32); i32_add;
                    call(self.emitter.rt.alloc); local_set(dst);
                    local_get(dst); local_get(r); i32_store(0);
                    local_get(dst); local_get(d_out); i32_store(4);
                    i32_const(0); local_set(i);
                    block_empty; loop_empty;
                      local_get(i); local_get(r); i32_ge_u; br_if(1);
                      i32_const(0); local_set(j);
                      block_empty; loop_empty;
                        local_get(j); local_get(d_out); i32_ge_u; br_if(1);
                        f64_const(0.0); local_set(g);
                        f64_const(0.0); local_set(u);
                        i32_const(0); local_set(k);
                        block_empty; loop_empty;
                          local_get(k); local_get(d_in); i32_ge_u; br_if(1);
                          // tmp = x[i, k]
                          local_get(x); i32_const(self.emitter.layout_reg.fixed_offset(super::engine::layout::LIST, super::engine::layout::list::DATA) as i32); i32_add;
                          local_get(i); local_get(d_in); i32_mul; local_get(k); i32_add;
                          i32_const(8); i32_mul; i32_add;
                          f64_load(0); local_set(tmp);
                          // g += tmp * w_gate[j, k]
                          local_get(tmp);
                          local_get(wg); i32_const(self.emitter.layout_reg.fixed_offset(super::engine::layout::LIST, super::engine::layout::list::DATA) as i32); i32_add;
                          local_get(j); local_get(d_in); i32_mul; local_get(k); i32_add;
                          i32_const(8); i32_mul; i32_add;
                          f64_load(0); f64_mul;
                          local_get(g); f64_add; local_set(g);
                          // u += tmp * w_up[j, k]
                          local_get(tmp);
                          local_get(wu); i32_const(self.emitter.layout_reg.fixed_offset(super::engine::layout::LIST, super::engine::layout::list::DATA) as i32); i32_add;
                          local_get(j); local_get(d_in); i32_mul; local_get(k); i32_add;
                          i32_const(8); i32_mul; i32_add;
                          f64_load(0); f64_mul;
                          local_get(u); f64_add; local_set(u);
                          local_get(k); i32_const(1); i32_add; local_set(k);
                          br(0);
                        end; end;
                        // sig = 1 / (1 + exp(-g)), with -g clamped to ±40 so
                        // exp() can't overflow to ∞ (σ saturates anyway).
                        f64_const(0.0); local_get(g); f64_sub;
                        f64_const(40.0); f64_min;
                        f64_const(-40.0); f64_max;
                        call(self.emitter.rt.math_exp);
                        f64_const(1.0); f64_add; local_set(tmp); // tmp = 1 + exp(-g)
                        f64_const(1.0); local_get(tmp); f64_div; local_set(sig);
                        // out[i, j] = g * sig * u
                        local_get(dst); i32_const(self.emitter.layout_reg.fixed_offset(super::engine::layout::LIST, super::engine::layout::list::DATA) as i32); i32_add;
                        local_get(i); local_get(d_out); i32_mul; local_get(j); i32_add;
                        i32_const(8); i32_mul; i32_add;
                        local_get(g); local_get(sig); f64_mul;
                        local_get(u); f64_mul;
                        f64_store(0);
                        local_get(j); i32_const(1); i32_add; local_set(j);
                        br(0);
                      end; end;
                      local_get(i); i32_const(1); i32_add; local_set(i);
                      br(0);
                    end; end;
                    local_get(dst);
                });
                self.scratch.free_f64(sig);
                self.scratch.free_f64(tmp);
                self.scratch.free_f64(u);
                self.scratch.free_f64(g);
                self.scratch.free_i32(k);
                self.scratch.free_i32(j);
                self.scratch.free_i32(i);
                self.scratch.free_i32(d_out);
                self.scratch.free_i32(d_in);
                self.scratch.free_i32(r);
                self.scratch.free_i32(dst);
                self.scratch.free_i32(wu);
                self.scratch.free_i32(wg);
                self.scratch.free_i32(x);
            }
            "attention_weights" => {
                // matrix.attention_weights(q, kt, scale) → Matrix.
                // Composes `softmax_rows(scale * (q × kt))` into a single
                // pass: dst[i, j] = scale * Σ_k q[i, k] * kt[k, j], then
                // row-softmax in place. Phase 1 allocs are freed before
                // Phase 2 so row_off/maxv/sumv/v slots reuse them —
                // keeps the scratch peak at 7 i32 + 2 f64.
                let q = self.scratch.alloc_i32();
                let kt = self.scratch.alloc_i32();
                let scale = self.scratch.alloc_f64();
                let dst = self.scratch.alloc_i32();
                let qr = self.scratch.alloc_i32();
                let inner = self.scratch.alloc_i32();
                let ktc = self.scratch.alloc_i32();
                let i = self.scratch.alloc_i32();
                let j = self.scratch.alloc_i32();
                let k = self.scratch.alloc_i32();
                let acc = self.scratch.alloc_f64();
                self.emit_expr(&args[0]);
                wasm!(self.func, { local_set(q); });
                self.emit_expr(&args[1]);
                wasm!(self.func, { local_set(kt); });
                self.emit_expr(&args[2]);
                if matches!(&args[2].ty, almide_lang::types::Ty::Int) {
                    wasm!(self.func, { f64_convert_i64_s; });
                }
                wasm!(self.func, {
                    local_set(scale);
                    local_get(q); i32_load(0); local_set(qr);
                    local_get(q); i32_load(4); local_set(inner);
                    local_get(kt); i32_load(4); local_set(ktc);
                    // alloc dst = qr × ktc
                    local_get(qr); local_get(ktc); i32_mul; i32_const(8); i32_mul;
                    i32_const(self.emitter.layout_reg.fixed_offset(super::engine::layout::LIST, super::engine::layout::list::DATA) as i32); i32_add;
                    call(self.emitter.rt.alloc); local_set(dst);
                    local_get(dst); local_get(qr); i32_store(0);
                    local_get(dst); local_get(ktc); i32_store(4);
                    // Phase 1: dst[i, j] = scale * Σ_k q[i, k] * kt[k, j]
                    i32_const(0); local_set(i);
                    block_empty; loop_empty;
                      local_get(i); local_get(qr); i32_ge_u; br_if(1);
                      i32_const(0); local_set(j);
                      block_empty; loop_empty;
                        local_get(j); local_get(ktc); i32_ge_u; br_if(1);
                        f64_const(0.0); local_set(acc);
                        i32_const(0); local_set(k);
                        block_empty; loop_empty;
                          local_get(k); local_get(inner); i32_ge_u; br_if(1);
                          // q[i, k]
                          local_get(q); i32_const(self.emitter.layout_reg.fixed_offset(super::engine::layout::LIST, super::engine::layout::list::DATA) as i32); i32_add;
                          local_get(i); local_get(inner); i32_mul; local_get(k); i32_add;
                          i32_const(8); i32_mul; i32_add;
                          f64_load(0);
                          // kt[k, j]
                          local_get(kt); i32_const(self.emitter.layout_reg.fixed_offset(super::engine::layout::LIST, super::engine::layout::list::DATA) as i32); i32_add;
                          local_get(k); local_get(ktc); i32_mul; local_get(j); i32_add;
                          i32_const(8); i32_mul; i32_add;
                          f64_load(0);
                          f64_mul;
                          local_get(acc); f64_add; local_set(acc);
                          local_get(k); i32_const(1); i32_add; local_set(k);
                          br(0);
                        end; end;
                        // dst[i, j] = scale * acc
                        local_get(dst); i32_const(self.emitter.layout_reg.fixed_offset(super::engine::layout::LIST, super::engine::layout::list::DATA) as i32); i32_add;
                        local_get(i); local_get(ktc); i32_mul; local_get(j); i32_add;
                        i32_const(8); i32_mul; i32_add;
                        local_get(acc); local_get(scale); f64_mul;
                        f64_store(0);
                        local_get(j); i32_const(1); i32_add; local_set(j);
                        br(0);
                      end; end;
                      local_get(i); i32_const(1); i32_add; local_set(i);
                      br(0);
                    end; end;
                });
                // Free Phase 1 slots so Phase 2's row_off/maxv/sumv/v
                // reuse the same physical WASM locals. `qr` stays live —
                // Phase 2's outer loop still needs it.
                self.scratch.free_f64(acc);
                self.scratch.free_i32(k);
                self.scratch.free_i32(inner);
                self.scratch.free_f64(scale);
                self.scratch.free_i32(kt);
                self.scratch.free_i32(q);
                let row_off = self.scratch.alloc_i32();
                let maxv = self.scratch.alloc_f64();
                let sumv = self.scratch.alloc_f64();
                let v = self.scratch.alloc_f64();
                wasm!(self.func, {
                    // Phase 2: row-softmax in place on dst.
                    i32_const(0); local_set(i);
                    block_empty; loop_empty;
                      local_get(i); local_get(qr); i32_ge_u; br_if(1);
                      local_get(i); local_get(ktc); i32_mul; i32_const(8); i32_mul;
                      i32_const(self.emitter.layout_reg.fixed_offset(super::engine::layout::LIST, super::engine::layout::list::DATA) as i32); i32_add; local_set(row_off);
                      // maxv = dst[i, 0]; scan 1..ktc
                      local_get(dst); local_get(row_off); i32_add;
                      f64_load(0); local_set(maxv);
                      i32_const(1); local_set(j);
                      block_empty; loop_empty;
                        local_get(j); local_get(ktc); i32_ge_u; br_if(1);
                        local_get(dst); local_get(row_off); i32_add;
                        local_get(j); i32_const(8); i32_mul; i32_add;
                        f64_load(0); local_set(v);
                        local_get(v); local_get(maxv); f64_gt;
                        if_empty; local_get(v); local_set(maxv); end;
                        local_get(j); i32_const(1); i32_add; local_set(j);
                        br(0);
                      end; end;
                      // dst[i, j] = exp(dst[i, j] - maxv); sum
                      f64_const(0.0); local_set(sumv);
                      i32_const(0); local_set(j);
                      block_empty; loop_empty;
                        local_get(j); local_get(ktc); i32_ge_u; br_if(1);
                        local_get(dst); local_get(row_off); i32_add;
                        local_get(j); i32_const(8); i32_mul; i32_add;
                        local_get(dst); local_get(row_off); i32_add;
                        local_get(j); i32_const(8); i32_mul; i32_add;
                        f64_load(0); local_get(maxv); f64_sub;
                        call(self.emitter.rt.math_exp); local_set(v);
                        local_get(v); f64_store(0);
                        local_get(sumv); local_get(v); f64_add; local_set(sumv);
                        local_get(j); i32_const(1); i32_add; local_set(j);
                        br(0);
                      end; end;
                      // Sum guard: NaN/Inf/zero → uniform distribution.
                      local_get(sumv); f64_const(0.0); f64_le;
                      local_get(sumv); local_get(sumv); f64_ne;
                      i32_or;
                      if_empty;
                        local_get(ktc); f64_convert_i32_u; local_set(sumv);
                        i32_const(0); local_set(j);
                        block_empty; loop_empty;
                          local_get(j); local_get(ktc); i32_ge_u; br_if(1);
                          local_get(dst); local_get(row_off); i32_add;
                          local_get(j); i32_const(8); i32_mul; i32_add;
                          f64_const(1.0); f64_store(0);
                          local_get(j); i32_const(1); i32_add; local_set(j);
                          br(0);
                        end; end;
                      end;
                      // normalize
                      i32_const(0); local_set(j);
                      block_empty; loop_empty;
                        local_get(j); local_get(ktc); i32_ge_u; br_if(1);
                        local_get(dst); local_get(row_off); i32_add;
                        local_get(j); i32_const(8); i32_mul; i32_add;
                        local_get(dst); local_get(row_off); i32_add;
                        local_get(j); i32_const(8); i32_mul; i32_add;
                        f64_load(0);
                        local_get(sumv); f64_div;
                        f64_store(0);
                        local_get(j); i32_const(1); i32_add; local_set(j);
                        br(0);
                      end; end;
                      local_get(i); i32_const(1); i32_add; local_set(i);
                      br(0);
                    end; end;
                    local_get(dst);
                });
                self.scratch.free_f64(v);
                self.scratch.free_f64(sumv);
                self.scratch.free_f64(maxv);
                self.scratch.free_i32(row_off);
                self.scratch.free_i32(j);
                self.scratch.free_i32(i);
                self.scratch.free_i32(qr);
                self.scratch.free_i32(ktc);
                self.scratch.free_i32(dst);
            }
            _ => return false,
        }
        true
    }
}
