//! Matrix stdlib call dispatch for WASM codegen.
//!
//! Memory layout: [rows:i32][cols:i32][data:f64...]  (row-major, 8 bytes per element)
//! Total size: 8 + rows*cols*8

use super::FuncCompiler;
use almide_ir::{IrExpr, IrExprKind, CallTarget};
use almide_lang::types::Ty;
use almide_base::intern::sym;

impl FuncCompiler<'_> {
    /// Dispatch a matrix stdlib method call. Returns true if handled.
    pub(super) fn emit_matrix_call(&mut self, method: &str, args: &[IrExpr]) -> bool {
        // WASM matrix runtime is f64-only. The _f32 variants of primitive ops
        // are preview API surfaces for the native path; on WASM they dispatch
        // to their f64 equivalents (storage & arithmetic identical at this
        // layer).
        let method = match method {
            "zeros_f32" => "zeros",
            "ones_f32" => "ones",
            "mul_f32" => "mul",
            _ => method,
        };
        // mul_scaled / mul_f32_scaled: alpha * A * B — emit as scale(mul(a, b), alpha).
        if method == "mul_scaled" || method == "mul_f32_scaled" {
            let mul_call = IrExpr {
                kind: IrExprKind::Call {
                    target: CallTarget::Module { module: sym("matrix"), func: sym("mul") },
                    args: vec![args[0].clone(), args[2].clone()],
                    type_args: vec![],
                },
                ty: Ty::Matrix,
                span: None,
            };
            let scale_args = vec![mul_call, args[1].clone()];
            return self.emit_matrix_call("scale", &scale_args);
        }
        // mul_f32_t / mul_f32_t_scaled: A @ B^T — emit transpose(B) then mul.
        if method == "mul_f32_t" {
            let transpose_call = IrExpr {
                kind: IrExprKind::Call {
                    target: CallTarget::Module { module: sym("matrix"), func: sym("transpose") },
                    args: vec![args[1].clone()],
                    type_args: vec![],
                },
                ty: Ty::Matrix,
                span: None,
            };
            return self.emit_matrix_call("mul", &[args[0].clone(), transpose_call]);
        }
        if method == "mul_f32_t_scaled" {
            let transpose_call = IrExpr {
                kind: IrExprKind::Call {
                    target: CallTarget::Module { module: sym("matrix"), func: sym("transpose") },
                    args: vec![args[2].clone()],
                    type_args: vec![],
                },
                ty: Ty::Matrix,
                span: None,
            };
            return self.emit_matrix_call("mul_scaled", &[args[0].clone(), args[1].clone(), transpose_call]);
        }
        match method {
            "zeros" | "ones" => {
                // matrix.zeros(rows, cols) / matrix.ones(rows, cols) → Matrix
                let rows = self.scratch.alloc_i32();
                let cols = self.scratch.alloc_i32();
                let ptr = self.scratch.alloc_i32();
                let total = self.scratch.alloc_i32();
                self.emit_expr(&args[0]);
                wasm!(self.func, { i32_wrap_i64; local_set(rows); });
                self.emit_expr(&args[1]);
                wasm!(self.func, {
                    i32_wrap_i64; local_set(cols);
                    // total_bytes = 8 + rows*cols*8
                    local_get(rows); local_get(cols); i32_mul; local_set(total);
                    local_get(total); i32_const(8); i32_mul; i32_const(8); i32_add;
                    call(self.emitter.rt.alloc);
                    local_set(ptr);
                    // store rows, cols
                    local_get(ptr); local_get(rows); i32_store(0);
                    local_get(ptr); local_get(cols); i32_store(4);
                });
                if method == "zeros" {
                    // zero-fill data
                    wasm!(self.func, {
                        local_get(ptr); i32_const(8); i32_add;
                        i32_const(0);
                        local_get(total); i32_const(8); i32_mul;
                        memory_fill;
                    });
                } else {
                    // fill with 1.0
                    let i = self.scratch.alloc_i32();
                    wasm!(self.func, {
                        i32_const(0); local_set(i);
                        block_empty; loop_empty;
                          local_get(i); local_get(total); i32_ge_u; br_if(1);
                          local_get(ptr); i32_const(8); i32_add;
                          local_get(i); i32_const(8); i32_mul; i32_add;
                          f64_const(1.0);
                          f64_store(0);
                          local_get(i); i32_const(1); i32_add; local_set(i);
                          br(0);
                        end; end;
                    });
                    self.scratch.free_i32(i);
                }
                wasm!(self.func, { local_get(ptr); });
                self.scratch.free_i32(total);
                self.scratch.free_i32(ptr);
                self.scratch.free_i32(cols);
                self.scratch.free_i32(rows);
            }
            "rows" => {
                self.emit_expr(&args[0]);
                wasm!(self.func, { i32_load(0); i64_extend_i32_u; });
            }
            "cols" => {
                self.emit_expr(&args[0]);
                wasm!(self.func, { i32_load(4); i64_extend_i32_u; });
            }
            "shape" => {
                // Returns (Int, Int) as a tuple: [fst:i64][snd:i64]
                let m = self.scratch.alloc_i32();
                let t = self.scratch.alloc_i32();
                self.emit_expr(&args[0]);
                wasm!(self.func, {
                    local_set(m);
                    i32_const(16); call(self.emitter.rt.alloc); local_set(t);
                    local_get(t);
                    local_get(m); i32_load(0); i64_extend_i32_u;
                    i64_store(0);
                    local_get(t);
                    local_get(m); i32_load(4); i64_extend_i32_u;
                    i64_store(8);
                    local_get(t);
                });
                self.scratch.free_i32(t);
                self.scratch.free_i32(m);
            }
            "get" => {
                // matrix.get(m, row, col) → Float (f64)
                let m = self.scratch.alloc_i32();
                let row = self.scratch.alloc_i32();
                let col = self.scratch.alloc_i32();
                self.emit_expr(&args[0]);
                wasm!(self.func, { local_set(m); });
                self.emit_expr(&args[1]);
                wasm!(self.func, { i32_wrap_i64; local_set(row); });
                self.emit_expr(&args[2]);
                wasm!(self.func, {
                    i32_wrap_i64; local_set(col);
                    // offset = 8 + (row * cols + col) * 8
                    local_get(m); i32_const(8); i32_add;
                    local_get(row); local_get(m); i32_load(4); i32_mul;
                    local_get(col); i32_add;
                    i32_const(8); i32_mul;
                    i32_add;
                    f64_load(0);
                });
                self.scratch.free_i32(col);
                self.scratch.free_i32(row);
                self.scratch.free_i32(m);
            }
            "transpose" => {
                let src = self.scratch.alloc_i32();
                let dst = self.scratch.alloc_i32();
                let r = self.scratch.alloc_i32();
                let c = self.scratch.alloc_i32();
                let rows = self.scratch.alloc_i32();
                let cols = self.scratch.alloc_i32();
                self.emit_expr(&args[0]);
                wasm!(self.func, {
                    local_set(src);
                    local_get(src); i32_load(0); local_set(rows);
                    local_get(src); i32_load(4); local_set(cols);
                    // alloc dst: [cols, rows, data...]
                    local_get(cols); local_get(rows); i32_mul; i32_const(8); i32_mul;
                    i32_const(8); i32_add;
                    call(self.emitter.rt.alloc); local_set(dst);
                    local_get(dst); local_get(cols); i32_store(0);
                    local_get(dst); local_get(rows); i32_store(4);
                    // loop: dst[c][r] = src[r][c]
                    i32_const(0); local_set(r);
                    block_empty; loop_empty;
                      local_get(r); local_get(rows); i32_ge_u; br_if(1);
                      i32_const(0); local_set(c);
                      block_empty; loop_empty;
                        local_get(c); local_get(cols); i32_ge_u; br_if(1);
                        // dst offset: 8 + (c * rows + r) * 8
                        local_get(dst); i32_const(8); i32_add;
                        local_get(c); local_get(rows); i32_mul; local_get(r); i32_add;
                        i32_const(8); i32_mul; i32_add;
                        // src offset: 8 + (r * cols + c) * 8
                        local_get(src); i32_const(8); i32_add;
                        local_get(r); local_get(cols); i32_mul; local_get(c); i32_add;
                        i32_const(8); i32_mul; i32_add;
                        f64_load(0);
                        f64_store(0);
                        local_get(c); i32_const(1); i32_add; local_set(c);
                        br(0);
                      end; end;
                      local_get(r); i32_const(1); i32_add; local_set(r);
                      br(0);
                    end; end;
                    local_get(dst);
                });
                self.scratch.free_i32(cols);
                self.scratch.free_i32(rows);
                self.scratch.free_i32(c);
                self.scratch.free_i32(r);
                self.scratch.free_i32(dst);
                self.scratch.free_i32(src);
            }
            "add" | "sub" | "div" => {
                // element-wise add/sub/div with f64x2 SIMD inner loop + scalar tail
                let a = self.scratch.alloc_i32();
                let b = self.scratch.alloc_i32();
                let dst = self.scratch.alloc_i32();
                let total = self.scratch.alloc_i32();
                let i = self.scratch.alloc_i32();
                self.emit_expr(&args[0]);
                wasm!(self.func, { local_set(a); });
                self.emit_expr(&args[1]);
                wasm!(self.func, {
                    local_set(b);
                    local_get(a); i32_load(0); local_get(a); i32_load(4); i32_mul; local_set(total);
                    local_get(total); i32_const(8); i32_mul; i32_const(8); i32_add;
                    call(self.emitter.rt.alloc); local_set(dst);
                    local_get(dst); local_get(a); i32_load(0); i32_store(0);
                    local_get(dst); local_get(a); i32_load(4); i32_store(4);
                    i32_const(0); local_set(i);
                    // SIMD loop: 2 elements per iter
                    block_empty; loop_empty;
                      local_get(i); i32_const(1); i32_add; local_get(total); i32_ge_u; br_if(1);
                      local_get(dst); i32_const(8); i32_add; local_get(i); i32_const(8); i32_mul; i32_add;
                      local_get(a); i32_const(8); i32_add; local_get(i); i32_const(8); i32_mul; i32_add; v128_load(0);
                      local_get(b); i32_const(8); i32_add; local_get(i); i32_const(8); i32_mul; i32_add; v128_load(0);
                });
                match method {
                    "add" => { wasm!(self.func, { f64x2_add; }); }
                    "sub" => { wasm!(self.func, { f64x2_sub; }); }
                    "div" => { wasm!(self.func, { f64x2_div; }); }
                    _ => unreachable!(),
                }
                wasm!(self.func, {
                      v128_store(0);
                      local_get(i); i32_const(2); i32_add; local_set(i);
                      br(0);
                    end; end;
                    // Scalar tail (1 element if total is odd)
                    local_get(i); local_get(total); i32_lt_u;
                    if_empty;
                      local_get(dst); i32_const(8); i32_add; local_get(i); i32_const(8); i32_mul; i32_add;
                      local_get(a); i32_const(8); i32_add; local_get(i); i32_const(8); i32_mul; i32_add; f64_load(0);
                      local_get(b); i32_const(8); i32_add; local_get(i); i32_const(8); i32_mul; i32_add; f64_load(0);
                });
                match method {
                    "add" => { wasm!(self.func, { f64_add; }); }
                    "sub" => { wasm!(self.func, { f64_sub; }); }
                    "div" => { wasm!(self.func, { f64_div; }); }
                    _ => unreachable!(),
                }
                wasm!(self.func, {
                      f64_store(0);
                    end;
                    local_get(dst);
                });
                self.scratch.free_i32(i);
                self.scratch.free_i32(total);
                self.scratch.free_i32(dst);
                self.scratch.free_i32(b);
                self.scratch.free_i32(a);
            }
            "mul" => {
                // Tiled matrix multiplication for cache locality.
                // Loop order: i-k-j (inner loop scans B row = contiguous memory).
                // Tile size 32 fits 3 tiles in L1 cache (32×32×8 bytes × 3 = 24KB).
                let a = self.scratch.alloc_i32();
                let b = self.scratch.alloc_i32();
                let dst = self.scratch.alloc_i32();
                let ra = self.scratch.alloc_i32();
                let ca = self.scratch.alloc_i32();
                let cb = self.scratch.alloc_i32();
                let i0 = self.scratch.alloc_i32();
                let k0 = self.scratch.alloc_i32();
                let j0 = self.scratch.alloc_i32();
                let i = self.scratch.alloc_i32();
                let k = self.scratch.alloc_i32();
                let j = self.scratch.alloc_i32();
                let i1 = self.scratch.alloc_i32();
                let k1 = self.scratch.alloc_i32();
                let j1 = self.scratch.alloc_i32();
                let a_ik = self.scratch.alloc_f64();
                const TILE: i32 = 32;
                self.emit_expr(&args[0]);
                wasm!(self.func, { local_set(a); });
                self.emit_expr(&args[1]);
                wasm!(self.func, {
                    local_set(b);
                    local_get(a); i32_load(0); local_set(ra);
                    local_get(a); i32_load(4); local_set(ca);
                    local_get(b); i32_load(4); local_set(cb);
                    // alloc + zero result
                    local_get(ra); local_get(cb); i32_mul; i32_const(8); i32_mul;
                    i32_const(8); i32_add;
                    call(self.emitter.rt.alloc); local_set(dst);
                    local_get(dst); local_get(ra); i32_store(0);
                    local_get(dst); local_get(cb); i32_store(4);
                    local_get(dst); i32_const(8); i32_add;
                    i32_const(0);
                    local_get(ra); local_get(cb); i32_mul; i32_const(8); i32_mul;
                    memory_fill;
                    // Tiled loops: i0, k0, j0 (tile starts)
                    i32_const(0); local_set(i0);
                    block_empty; loop_empty;
                      local_get(i0); local_get(ra); i32_ge_u; br_if(1);
                      // i1 = min(i0+TILE, ra)
                      local_get(i0); i32_const(TILE); i32_add; local_set(i1);
                      local_get(i1); local_get(ra); i32_gt_u;
                      if_empty; local_get(ra); local_set(i1); end;

                      i32_const(0); local_set(k0);
                      block_empty; loop_empty;
                        local_get(k0); local_get(ca); i32_ge_u; br_if(1);
                        local_get(k0); i32_const(TILE); i32_add; local_set(k1);
                        local_get(k1); local_get(ca); i32_gt_u;
                        if_empty; local_get(ca); local_set(k1); end;

                        i32_const(0); local_set(j0);
                        block_empty; loop_empty;
                          local_get(j0); local_get(cb); i32_ge_u; br_if(1);
                          local_get(j0); i32_const(TILE); i32_add; local_set(j1);
                          local_get(j1); local_get(cb); i32_gt_u;
                          if_empty; local_get(cb); local_set(j1); end;

                          // Inner tile: i, k, j
                          local_get(i0); local_set(i);
                          block_empty; loop_empty;
                            local_get(i); local_get(i1); i32_ge_u; br_if(1);

                            local_get(k0); local_set(k);
                            block_empty; loop_empty;
                              local_get(k); local_get(k1); i32_ge_u; br_if(1);
                              // a_ik = A[i][k]
                              local_get(a); i32_const(8); i32_add;
                              local_get(i); local_get(ca); i32_mul; local_get(k); i32_add;
                              i32_const(8); i32_mul; i32_add; f64_load(0);
                              local_set(a_ik);

                              // SIMD inner loop: j steps by 2 (f64x2)
                              // j1_even = j1 & ~1 (round down to even)
                              local_get(j0); local_set(j);
                              block_empty; loop_empty;
                                // if j+1 >= j1, exit SIMD loop
                                local_get(j); i32_const(1); i32_add; local_get(j1); i32_gt_u; br_if(1);
                                // addr_c = dst + 8 + (i*cb + j)*8
                                local_get(dst); i32_const(8); i32_add;
                                local_get(i); local_get(cb); i32_mul; local_get(j); i32_add;
                                i32_const(8); i32_mul; i32_add;
                                // v_c = load C[i][j..j+2]
                                local_get(dst); i32_const(8); i32_add;
                                local_get(i); local_get(cb); i32_mul; local_get(j); i32_add;
                                i32_const(8); i32_mul; i32_add;
                                v128_load(0);
                                // v_a = splat(a_ik)
                                local_get(a_ik); f64x2_splat;
                                // v_b = load B[k][j..j+2]
                                local_get(b); i32_const(8); i32_add;
                                local_get(k); local_get(cb); i32_mul; local_get(j); i32_add;
                                i32_const(8); i32_mul; i32_add;
                                v128_load(0);
                                // v_c += v_a * v_b
                                f64x2_mul; f64x2_add;
                                // store C[i][j..j+2]
                                v128_store(0);

                                local_get(j); i32_const(2); i32_add; local_set(j);
                                br(0);
                              end; end;
                              // Scalar remainder: if j < j1, process 1 more element
                              local_get(j); local_get(j1); i32_lt_u;
                              if_empty;
                                local_get(dst); i32_const(8); i32_add;
                                local_get(i); local_get(cb); i32_mul; local_get(j); i32_add;
                                i32_const(8); i32_mul; i32_add;
                                local_get(dst); i32_const(8); i32_add;
                                local_get(i); local_get(cb); i32_mul; local_get(j); i32_add;
                                i32_const(8); i32_mul; i32_add; f64_load(0);
                                local_get(a_ik);
                                local_get(b); i32_const(8); i32_add;
                                local_get(k); local_get(cb); i32_mul; local_get(j); i32_add;
                                i32_const(8); i32_mul; i32_add; f64_load(0);
                                f64_mul; f64_add; f64_store(0);
                              end;
                              local_get(k); i32_const(1); i32_add; local_set(k);
                              br(0);
                            end; end;
                            local_get(i); i32_const(1); i32_add; local_set(i);
                            br(0);
                          end; end;

                          local_get(j0); i32_const(TILE); i32_add; local_set(j0);
                          br(0);
                        end; end;
                        local_get(k0); i32_const(TILE); i32_add; local_set(k0);
                        br(0);
                      end; end;
                      local_get(i0); i32_const(TILE); i32_add; local_set(i0);
                      br(0);
                    end; end;
                    local_get(dst);
                });
                self.scratch.free_f64(a_ik);
                self.scratch.free_i32(j1);
                self.scratch.free_i32(k1);
                self.scratch.free_i32(i1);
                self.scratch.free_i32(j);
                self.scratch.free_i32(k);
                self.scratch.free_i32(i);
                self.scratch.free_i32(j0);
                self.scratch.free_i32(k0);
                self.scratch.free_i32(i0);
                self.scratch.free_i32(cb);
                self.scratch.free_i32(ca);
                self.scratch.free_i32(ra);
                self.scratch.free_i32(dst);
                self.scratch.free_i32(b);
                self.scratch.free_i32(a);
            }
            "scale" => {
                // matrix.scale(m, s) → Matrix
                let m = self.scratch.alloc_i32();
                let dst = self.scratch.alloc_i32();
                let total = self.scratch.alloc_i32();
                let i = self.scratch.alloc_i32();
                let s = self.scratch.alloc_f64();
                self.emit_expr(&args[0]);
                wasm!(self.func, { local_set(m); });
                self.emit_expr(&args[1]);
                // scalar could be Int or Float — convert to f64
                if matches!(&args[1].ty, almide_lang::types::Ty::Int) {
                    wasm!(self.func, { f64_convert_i64_s; });
                }
                wasm!(self.func, {
                    local_set(s);
                    local_get(m); i32_load(0); local_get(m); i32_load(4); i32_mul; local_set(total);
                    local_get(total); i32_const(8); i32_mul; i32_const(8); i32_add;
                    call(self.emitter.rt.alloc); local_set(dst);
                    local_get(dst); local_get(m); i32_load(0); i32_store(0);
                    local_get(dst); local_get(m); i32_load(4); i32_store(4);
                    i32_const(0); local_set(i);
                    // SIMD f64x2 inner loop
                    block_empty; loop_empty;
                      local_get(i); i32_const(1); i32_add; local_get(total); i32_ge_u; br_if(1);
                      local_get(dst); i32_const(8); i32_add; local_get(i); i32_const(8); i32_mul; i32_add;
                      local_get(m); i32_const(8); i32_add; local_get(i); i32_const(8); i32_mul; i32_add;
                      v128_load(0); local_get(s); f64x2_splat; f64x2_mul;
                      v128_store(0);
                      local_get(i); i32_const(2); i32_add; local_set(i);
                      br(0);
                    end; end;
                    // Scalar tail (1 element if total is odd)
                    local_get(i); local_get(total); i32_lt_u;
                    if_empty;
                      local_get(dst); i32_const(8); i32_add; local_get(i); i32_const(8); i32_mul; i32_add;
                      local_get(m); i32_const(8); i32_add; local_get(i); i32_const(8); i32_mul; i32_add;
                      f64_load(0); local_get(s); f64_mul;
                      f64_store(0);
                    end;
                    local_get(dst);
                });
                self.scratch.free_f64(s);
                self.scratch.free_i32(i);
                self.scratch.free_i32(total);
                self.scratch.free_i32(dst);
                self.scratch.free_i32(m);
            }
            "fma" => {
                // matrix.fma(a, ka, b, kb) → a*ka + b*kb in one pass.
                // Layout: [rows:i32][cols:i32][f64 × rows*cols], same as
                // scale/add. Total i = rows*cols, single allocation, two
                // f64 loads + 2 muls + 1 add per element. Equivalent to
                // add(scale(a, ka), scale(b, kb)) but skips two intermediate
                // matrices and an extra pass.
                let a = self.scratch.alloc_i32();
                let b = self.scratch.alloc_i32();
                let dst = self.scratch.alloc_i32();
                let total = self.scratch.alloc_i32();
                let i = self.scratch.alloc_i32();
                let ka = self.scratch.alloc_f64();
                let kb = self.scratch.alloc_f64();
                self.emit_expr(&args[0]);
                wasm!(self.func, { local_set(a); });
                self.emit_expr(&args[1]);
                if matches!(&args[1].ty, almide_lang::types::Ty::Int) {
                    wasm!(self.func, { f64_convert_i64_s; });
                }
                wasm!(self.func, { local_set(ka); });
                self.emit_expr(&args[2]);
                wasm!(self.func, { local_set(b); });
                self.emit_expr(&args[3]);
                if matches!(&args[3].ty, almide_lang::types::Ty::Int) {
                    wasm!(self.func, { f64_convert_i64_s; });
                }
                wasm!(self.func, {
                    local_set(kb);
                    // total = rows * cols
                    local_get(a); i32_load(0); local_get(a); i32_load(4); i32_mul; local_set(total);
                    // alloc 8 + total*8
                    local_get(total); i32_const(8); i32_mul; i32_const(8); i32_add;
                    call(self.emitter.rt.alloc); local_set(dst);
                    // copy header
                    local_get(dst); local_get(a); i32_load(0); i32_store(0);
                    local_get(dst); local_get(a); i32_load(4); i32_store(4);
                    i32_const(0); local_set(i);

                    // SIMD f64x2 inner loop: process 2 f64 elements per iteration.
                    // Loop invariant: i + 1 < total (room for 2 lanes).
                    block_empty; loop_empty;
                      // exit when i+1 >= total (no room for full lane)
                      local_get(i); i32_const(1); i32_add; local_get(total); i32_ge_u; br_if(1);
                      // dst addr = dst + 8 + i*8
                      local_get(dst); i32_const(8); i32_add; local_get(i); i32_const(8); i32_mul; i32_add;
                      // v_a = load a[i..i+2]
                      local_get(a); i32_const(8); i32_add; local_get(i); i32_const(8); i32_mul; i32_add;
                      v128_load(0);
                      // v_a *= splat(ka)
                      local_get(ka); f64x2_splat;
                      f64x2_mul;
                      // v_b = load b[i..i+2]
                      local_get(b); i32_const(8); i32_add; local_get(i); i32_const(8); i32_mul; i32_add;
                      v128_load(0);
                      // v_b *= splat(kb)
                      local_get(kb); f64x2_splat;
                      f64x2_mul;
                      // v_a + v_b
                      f64x2_add;
                      // store dst[i..i+2]
                      v128_store(0);
                      // i += 2
                      local_get(i); i32_const(2); i32_add; local_set(i);
                      br(0);
                    end; end;

                    // Scalar tail: process one trailing element if total is odd.
                    local_get(i); local_get(total); i32_lt_u;
                    if_empty;
                      local_get(dst); i32_const(8); i32_add; local_get(i); i32_const(8); i32_mul; i32_add;
                      local_get(a); i32_const(8); i32_add; local_get(i); i32_const(8); i32_mul; i32_add; f64_load(0);
                      local_get(ka); f64_mul;
                      local_get(b); i32_const(8); i32_add; local_get(i); i32_const(8); i32_mul; i32_add; f64_load(0);
                      local_get(kb); f64_mul;
                      f64_add;
                      f64_store(0);
                    end;

                    local_get(dst);
                });
                self.scratch.free_f64(kb);
                self.scratch.free_f64(ka);
                self.scratch.free_i32(i);
                self.scratch.free_i32(total);
                self.scratch.free_i32(dst);
                self.scratch.free_i32(b);
                self.scratch.free_i32(a);
            }
            "fma3" => {
                // matrix.fma3(a, ka, b, kb, c, kc) → a*ka + b*kb + c*kc in one pass.
                // Target of MatrixFusionPass tree-fuse rule (nested fma collapse).
                // SIMD f64x2 inner loop + scalar tail, same shape as fma but with
                // a third input/coefficient pair.
                let a = self.scratch.alloc_i32();
                let b = self.scratch.alloc_i32();
                let c = self.scratch.alloc_i32();
                let dst = self.scratch.alloc_i32();
                let total = self.scratch.alloc_i32();
                let i = self.scratch.alloc_i32();
                let ka = self.scratch.alloc_f64();
                let kb = self.scratch.alloc_f64();
                let kc = self.scratch.alloc_f64();
                self.emit_expr(&args[0]);
                wasm!(self.func, { local_set(a); });
                self.emit_expr(&args[1]);
                if matches!(&args[1].ty, almide_lang::types::Ty::Int) { wasm!(self.func, { f64_convert_i64_s; }); }
                wasm!(self.func, { local_set(ka); });
                self.emit_expr(&args[2]);
                wasm!(self.func, { local_set(b); });
                self.emit_expr(&args[3]);
                if matches!(&args[3].ty, almide_lang::types::Ty::Int) { wasm!(self.func, { f64_convert_i64_s; }); }
                wasm!(self.func, { local_set(kb); });
                self.emit_expr(&args[4]);
                wasm!(self.func, { local_set(c); });
                self.emit_expr(&args[5]);
                if matches!(&args[5].ty, almide_lang::types::Ty::Int) { wasm!(self.func, { f64_convert_i64_s; }); }
                wasm!(self.func, {
                    local_set(kc);
                    local_get(a); i32_load(0); local_get(a); i32_load(4); i32_mul; local_set(total);
                    local_get(total); i32_const(8); i32_mul; i32_const(8); i32_add;
                    call(self.emitter.rt.alloc); local_set(dst);
                    local_get(dst); local_get(a); i32_load(0); i32_store(0);
                    local_get(dst); local_get(a); i32_load(4); i32_store(4);
                    i32_const(0); local_set(i);

                    block_empty; loop_empty;
                      local_get(i); i32_const(1); i32_add; local_get(total); i32_ge_u; br_if(1);
                      local_get(dst); i32_const(8); i32_add; local_get(i); i32_const(8); i32_mul; i32_add;
                      local_get(a); i32_const(8); i32_add; local_get(i); i32_const(8); i32_mul; i32_add;
                      v128_load(0);
                      local_get(ka); f64x2_splat;
                      f64x2_mul;
                      local_get(b); i32_const(8); i32_add; local_get(i); i32_const(8); i32_mul; i32_add;
                      v128_load(0);
                      local_get(kb); f64x2_splat;
                      f64x2_mul;
                      f64x2_add;
                      local_get(c); i32_const(8); i32_add; local_get(i); i32_const(8); i32_mul; i32_add;
                      v128_load(0);
                      local_get(kc); f64x2_splat;
                      f64x2_mul;
                      f64x2_add;
                      v128_store(0);
                      local_get(i); i32_const(2); i32_add; local_set(i);
                      br(0);
                    end; end;

                    // Scalar tail
                    local_get(i); local_get(total); i32_lt_u;
                    if_empty;
                      local_get(dst); i32_const(8); i32_add; local_get(i); i32_const(8); i32_mul; i32_add;
                      local_get(a); i32_const(8); i32_add; local_get(i); i32_const(8); i32_mul; i32_add; f64_load(0);
                      local_get(ka); f64_mul;
                      local_get(b); i32_const(8); i32_add; local_get(i); i32_const(8); i32_mul; i32_add; f64_load(0);
                      local_get(kb); f64_mul;
                      f64_add;
                      local_get(c); i32_const(8); i32_add; local_get(i); i32_const(8); i32_mul; i32_add; f64_load(0);
                      local_get(kc); f64_mul;
                      f64_add;
                      f64_store(0);
                    end;

                    local_get(dst);
                });
                self.scratch.free_f64(kc);
                self.scratch.free_f64(kb);
                self.scratch.free_f64(ka);
                self.scratch.free_i32(i);
                self.scratch.free_i32(total);
                self.scratch.free_i32(dst);
                self.scratch.free_i32(c);
                self.scratch.free_i32(b);
                self.scratch.free_i32(a);
            }
            "from_lists" => {
                // matrix.from_lists(rows: List[List[Float]]) → Matrix
                // Input: List of Lists. List layout: [len:i32][elem0...]. Each inner list: [len:i32][f64...]
                let src = self.scratch.alloc_i32();
                let dst = self.scratch.alloc_i32();
                let nrows = self.scratch.alloc_i32();
                let ncols = self.scratch.alloc_i32();
                let r = self.scratch.alloc_i32();
                let c = self.scratch.alloc_i32();
                let row_ptr = self.scratch.alloc_i32();
                self.emit_expr(&args[0]);
                wasm!(self.func, {
                    local_set(src);
                    local_get(src); i32_load(0); local_set(nrows);
                    // cols from first row (or 0)
                    local_get(nrows); i32_eqz;
                    if_i32;
                      i32_const(0);
                    else_;
                      local_get(src); i32_const(4); i32_add; i32_load(0); // ptr to first row
                      i32_load(0); // len of first row
                    end;
                    local_set(ncols);
                    // alloc matrix
                    local_get(nrows); local_get(ncols); i32_mul; i32_const(8); i32_mul;
                    i32_const(8); i32_add;
                    call(self.emitter.rt.alloc); local_set(dst);
                    local_get(dst); local_get(nrows); i32_store(0);
                    local_get(dst); local_get(ncols); i32_store(4);
                    // copy data
                    i32_const(0); local_set(r);
                    block_empty; loop_empty;
                      local_get(r); local_get(nrows); i32_ge_u; br_if(1);
                      // row_ptr = *(src + 4 + r*4)  (pointer to inner list)
                      local_get(src); i32_const(4); i32_add;
                      local_get(r); i32_const(4); i32_mul; i32_add;
                      i32_load(0); local_set(row_ptr);
                      i32_const(0); local_set(c);
                      block_empty; loop_empty;
                        local_get(c); local_get(ncols); i32_ge_u; br_if(1);
                        // dst[r][c] = row_ptr->data[c]
                        local_get(dst); i32_const(8); i32_add;
                        local_get(r); local_get(ncols); i32_mul; local_get(c); i32_add;
                        i32_const(8); i32_mul; i32_add;
                        local_get(row_ptr); i32_const(4); i32_add;
                        local_get(c); i32_const(8); i32_mul; i32_add;
                        f64_load(0); f64_store(0);
                        local_get(c); i32_const(1); i32_add; local_set(c);
                        br(0);
                      end; end;
                      local_get(r); i32_const(1); i32_add; local_set(r);
                      br(0);
                    end; end;
                    local_get(dst);
                });
                self.scratch.free_i32(row_ptr);
                self.scratch.free_i32(c);
                self.scratch.free_i32(r);
                self.scratch.free_i32(ncols);
                self.scratch.free_i32(nrows);
                self.scratch.free_i32(dst);
                self.scratch.free_i32(src);
            }
            "to_lists" => {
                // matrix.to_lists(m) → List[List[Float]]
                let m = self.scratch.alloc_i32();
                let outer = self.scratch.alloc_i32();
                let nrows = self.scratch.alloc_i32();
                let ncols = self.scratch.alloc_i32();
                let r = self.scratch.alloc_i32();
                let c = self.scratch.alloc_i32();
                let row_ptr = self.scratch.alloc_i32();
                self.emit_expr(&args[0]);
                wasm!(self.func, {
                    local_set(m);
                    local_get(m); i32_load(0); local_set(nrows);
                    local_get(m); i32_load(4); local_set(ncols);
                    // alloc outer list: [len:i32][ptr0:i32][ptr1:i32]...
                    local_get(nrows); i32_const(4); i32_mul; i32_const(4); i32_add;
                    call(self.emitter.rt.alloc); local_set(outer);
                    local_get(outer); local_get(nrows); i32_store(0);
                    // create each row list
                    i32_const(0); local_set(r);
                    block_empty; loop_empty;
                      local_get(r); local_get(nrows); i32_ge_u; br_if(1);
                      // alloc row: [len:i32][f64...]
                      local_get(ncols); i32_const(8); i32_mul; i32_const(4); i32_add;
                      call(self.emitter.rt.alloc); local_set(row_ptr);
                      local_get(row_ptr); local_get(ncols); i32_store(0);
                      // copy data
                      i32_const(0); local_set(c);
                      block_empty; loop_empty;
                        local_get(c); local_get(ncols); i32_ge_u; br_if(1);
                        local_get(row_ptr); i32_const(4); i32_add;
                        local_get(c); i32_const(8); i32_mul; i32_add;
                        local_get(m); i32_const(8); i32_add;
                        local_get(r); local_get(ncols); i32_mul; local_get(c); i32_add;
                        i32_const(8); i32_mul; i32_add;
                        f64_load(0); f64_store(0);
                        local_get(c); i32_const(1); i32_add; local_set(c);
                        br(0);
                      end; end;
                      // store row ptr in outer list
                      local_get(outer); i32_const(4); i32_add;
                      local_get(r); i32_const(4); i32_mul; i32_add;
                      local_get(row_ptr); i32_store(0);
                      local_get(r); i32_const(1); i32_add; local_set(r);
                      br(0);
                    end; end;
                    local_get(outer);
                });
                self.scratch.free_i32(row_ptr);
                self.scratch.free_i32(c);
                self.scratch.free_i32(r);
                self.scratch.free_i32(ncols);
                self.scratch.free_i32(nrows);
                self.scratch.free_i32(outer);
                self.scratch.free_i32(m);
            }
            "map" => {
                // matrix.map(m, f) → Matrix: apply f(Float) -> Float to each element
                let m = self.scratch.alloc_i32();
                let closure = self.scratch.alloc_i32();
                let dst = self.scratch.alloc_i32();
                let total = self.scratch.alloc_i32();
                let i = self.scratch.alloc_i32();
                self.emit_expr(&args[0]);
                wasm!(self.func, { local_set(m); });
                self.emit_expr(&args[1]);
                wasm!(self.func, {
                    local_set(closure);
                    local_get(m); i32_load(0); local_get(m); i32_load(4); i32_mul; local_set(total);
                    local_get(total); i32_const(8); i32_mul; i32_const(8); i32_add;
                    call(self.emitter.rt.alloc); local_set(dst);
                    local_get(dst); local_get(m); i32_load(0); i32_store(0);
                    local_get(dst); local_get(m); i32_load(4); i32_store(4);
                    i32_const(0); local_set(i);
                    block_empty; loop_empty;
                      local_get(i); local_get(total); i32_ge_u; br_if(1);
                      // dst[i] = f(m[i])
                      local_get(dst); i32_const(8); i32_add;
                      local_get(i); i32_const(8); i32_mul; i32_add;
                      // call closure: env, arg, table_idx
                      local_get(closure); i32_load(4); // env
                      local_get(m); i32_const(8); i32_add;
                      local_get(i); i32_const(8); i32_mul; i32_add;
                      f64_load(0); // element value
                      local_get(closure); i32_load(0); // table_idx
                });
                self.emit_closure_call(&almide_lang::types::Ty::Float, &almide_lang::types::Ty::Float);
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
                self.scratch.free_i32(closure);
                self.scratch.free_i32(m);
            }
            "broadcast_add_row" => {
                // matrix.broadcast_add_row(m, bias) → Matrix
                // m: (R, C), bias: List[Float] of length C. result[r,c] = m[r,c] + bias[c]
                let m = self.scratch.alloc_i32();
                let bias = self.scratch.alloc_i32();
                let dst = self.scratch.alloc_i32();
                let rows = self.scratch.alloc_i32();
                let cols = self.scratch.alloc_i32();
                let r = self.scratch.alloc_i32();
                let c = self.scratch.alloc_i32();
                self.emit_expr(&args[0]);
                wasm!(self.func, { local_set(m); });
                self.emit_expr(&args[1]);
                wasm!(self.func, {
                    local_set(bias);
                    local_get(m); i32_load(0); local_set(rows);
                    local_get(m); i32_load(4); local_set(cols);
                    local_get(rows); local_get(cols); i32_mul; i32_const(8); i32_mul;
                    i32_const(8); i32_add;
                    call(self.emitter.rt.alloc); local_set(dst);
                    local_get(dst); local_get(rows); i32_store(0);
                    local_get(dst); local_get(cols); i32_store(4);
                    i32_const(0); local_set(r);
                    block_empty; loop_empty;
                      local_get(r); local_get(rows); i32_ge_u; br_if(1);
                      i32_const(0); local_set(c);
                      block_empty; loop_empty;
                        local_get(c); local_get(cols); i32_ge_u; br_if(1);
                        // dst[r,c] = m[r,c] + bias[c]
                        local_get(dst); i32_const(8); i32_add;
                        local_get(r); local_get(cols); i32_mul; local_get(c); i32_add;
                        i32_const(8); i32_mul; i32_add;
                        local_get(m); i32_const(8); i32_add;
                        local_get(r); local_get(cols); i32_mul; local_get(c); i32_add;
                        i32_const(8); i32_mul; i32_add;
                        f64_load(0);
                        local_get(bias); i32_const(4); i32_add;
                        local_get(c); i32_const(8); i32_mul; i32_add;
                        f64_load(0);
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
                self.scratch.free_i32(c);
                self.scratch.free_i32(r);
                self.scratch.free_i32(cols);
                self.scratch.free_i32(rows);
                self.scratch.free_i32(dst);
                self.scratch.free_i32(bias);
                self.scratch.free_i32(m);
            }
            "slice_rows" => {
                // matrix.slice_rows(m, start, end) → Matrix
                let m = self.scratch.alloc_i32();
                let start = self.scratch.alloc_i32();
                let end_ = self.scratch.alloc_i32();
                let dst = self.scratch.alloc_i32();
                let cols = self.scratch.alloc_i32();
                let nrows = self.scratch.alloc_i32();
                self.emit_expr(&args[0]);
                wasm!(self.func, { local_set(m); });
                self.emit_expr(&args[1]);
                wasm!(self.func, { i32_wrap_i64; local_set(start); });
                self.emit_expr(&args[2]);
                wasm!(self.func, {
                    i32_wrap_i64; local_set(end_);
                    local_get(m); i32_load(4); local_set(cols);
                    // nrows = end - start (clamped >= 0)
                    local_get(end_); local_get(start); i32_sub; local_set(nrows);
                    local_get(nrows); i32_const(0); i32_lt_s;
                    if_empty; i32_const(0); local_set(nrows); end;
                    // alloc + header
                    local_get(nrows); local_get(cols); i32_mul; i32_const(8); i32_mul;
                    i32_const(8); i32_add;
                    call(self.emitter.rt.alloc); local_set(dst);
                    local_get(dst); local_get(nrows); i32_store(0);
                    local_get(dst); local_get(cols); i32_store(4);
                    // memcpy: dst.data, m.data + start*cols*8, nrows*cols*8
                    local_get(dst); i32_const(8); i32_add;
                    local_get(m); i32_const(8); i32_add;
                    local_get(start); local_get(cols); i32_mul; i32_const(8); i32_mul; i32_add;
                    local_get(nrows); local_get(cols); i32_mul; i32_const(8); i32_mul;
                    memory_copy;
                    local_get(dst);
                });
                self.scratch.free_i32(nrows);
                self.scratch.free_i32(cols);
                self.scratch.free_i32(dst);
                self.scratch.free_i32(end_);
                self.scratch.free_i32(start);
                self.scratch.free_i32(m);
            }
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
                    local_get(total); i32_const(8); i32_mul; i32_const(8); i32_add;
                    call(self.emitter.rt.alloc); local_set(dst);
                    local_get(dst); local_get(m); i32_load(0); i32_store(0);
                    local_get(dst); local_get(m); i32_load(4); i32_store(4);
                    i32_const(0); local_set(i);
                    block_empty; loop_empty;
                      local_get(i); local_get(total); i32_ge_u; br_if(1);
                      local_get(dst); i32_const(8); i32_add; local_get(i); i32_const(8); i32_mul; i32_add;
                      local_get(m); i32_const(8); i32_add; local_get(i); i32_const(8); i32_mul; i32_add;
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
                    local_get(total); i32_const(8); i32_mul; i32_const(8); i32_add;
                    call(self.emitter.rt.alloc); local_set(dst);
                    local_get(dst); local_get(m); i32_load(0); i32_store(0);
                    local_get(dst); local_get(m); i32_load(4); i32_store(4);
                    i32_const(0); local_set(i);
                    block_empty; loop_empty;
                      local_get(i); local_get(total); i32_ge_u; br_if(1);
                      // Load x into a local so it's stable across the call.
                      local_get(m); i32_const(8); i32_add; local_get(i); i32_const(8); i32_mul; i32_add; f64_load(0);
                      local_set(x);
                      // Compute pow(x, exp) via __float_pow runtime.
                      local_get(x); local_get(exp); call(self.emitter.rt.float_pow);
                      local_set(result);
                      // Store at dst+8+i*8
                      local_get(dst); i32_const(8); i32_add; local_get(i); i32_const(8); i32_mul; i32_add;
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
                    local_get(total); i32_const(8); i32_mul; i32_const(8); i32_add;
                    call(self.emitter.rt.alloc); local_set(dst);
                    local_get(dst); local_get(m); i32_load(0); i32_store(0);
                    local_get(dst); local_get(m); i32_load(4); i32_store(4);
                    i32_const(0); local_set(i);
                    block_empty; loop_empty;
                      local_get(i); local_get(total); i32_ge_u; br_if(1);
                      // x = m.data[i]
                      local_get(m); i32_const(8); i32_add;
                      local_get(i); i32_const(8); i32_mul; i32_add;
                      f64_load(0); local_set(x);
                      // dst.data[i] = gelu(x)
                      local_get(dst); i32_const(8); i32_add;
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
                    i32_const(8); i32_add;
                    call(self.emitter.rt.alloc); local_set(dst);
                    local_get(dst); local_get(rows); i32_store(0);
                    local_get(dst); local_get(cols); i32_store(4);
                    i32_const(0); local_set(r);
                    block_empty; loop_empty;
                      local_get(r); local_get(rows); i32_ge_u; br_if(1);
                      // row_off = 8 + r*cols*8 (offset to row r in data)
                      local_get(r); local_get(cols); i32_mul; i32_const(8); i32_mul;
                      i32_const(8); i32_add; local_set(row_off);
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
                    i32_const(8); i32_add;
                    call(self.emitter.rt.alloc); local_set(dst);
                    local_get(dst); local_get(rows); i32_store(0);
                    local_get(dst); local_get(cols); i32_store(4);
                    // cnt = (f64) cols
                    local_get(cols); f64_convert_i32_u; local_set(cnt);
                    i32_const(0); local_set(r);
                    block_empty; loop_empty;
                      local_get(r); local_get(rows); i32_ge_u; br_if(1);
                      local_get(r); local_get(cols); i32_mul; i32_const(8); i32_mul;
                      i32_const(8); i32_add; local_set(row_off);
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
                        local_get(gamma); i32_const(4); i32_add;
                        local_get(c); i32_const(8); i32_mul; i32_add;
                        f64_load(0); f64_mul;
                        local_get(beta); i32_const(4); i32_add;
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
                    i32_const(8); i32_add;
                    call(self.emitter.rt.alloc); local_set(dst);
                    local_get(dst); local_get(rows); i32_store(0);
                    local_get(dst); local_get(cols); i32_store(4);
                    local_get(cols); f64_convert_i32_u; local_set(cnt);
                    i32_const(0); local_set(r);
                    block_empty; loop_empty;
                      local_get(r); local_get(rows); i32_ge_u; br_if(1);
                      local_get(r); local_get(cols); i32_mul; i32_const(8); i32_mul;
                      i32_const(8); i32_add; local_set(row_off);
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
                        local_get(gamma); i32_const(4); i32_add;
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
                    i32_const(8); i32_add;
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
                          local_get(x); i32_const(8); i32_add;
                          local_get(i); local_get(d_in); i32_mul; local_get(k); i32_add;
                          i32_const(8); i32_mul; i32_add;
                          f64_load(0); local_set(tmp);
                          // g += tmp * w_gate[j, k]
                          local_get(tmp);
                          local_get(wg); i32_const(8); i32_add;
                          local_get(j); local_get(d_in); i32_mul; local_get(k); i32_add;
                          i32_const(8); i32_mul; i32_add;
                          f64_load(0); f64_mul;
                          local_get(g); f64_add; local_set(g);
                          // u += tmp * w_up[j, k]
                          local_get(tmp);
                          local_get(wu); i32_const(8); i32_add;
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
                        local_get(dst); i32_const(8); i32_add;
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
                    i32_const(8); i32_add;
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
                          local_get(q); i32_const(8); i32_add;
                          local_get(i); local_get(inner); i32_mul; local_get(k); i32_add;
                          i32_const(8); i32_mul; i32_add;
                          f64_load(0);
                          // kt[k, j]
                          local_get(kt); i32_const(8); i32_add;
                          local_get(k); local_get(ktc); i32_mul; local_get(j); i32_add;
                          i32_const(8); i32_mul; i32_add;
                          f64_load(0);
                          f64_mul;
                          local_get(acc); f64_add; local_set(acc);
                          local_get(k); i32_const(1); i32_add; local_set(k);
                          br(0);
                        end; end;
                        // dst[i, j] = scale * acc
                        local_get(dst); i32_const(8); i32_add;
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
                      i32_const(8); i32_add; local_set(row_off);
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
                    local_get(total); i32_const(8); i32_mul; i32_const(8); i32_add;
                    call(self.emitter.rt.alloc); local_set(dst);
                    local_get(dst); local_get(rows); i32_store(0);
                    local_get(dst); local_get(cols); i32_store(4);
                    i32_const(0); local_set(k);
                    block_empty; loop_empty;
                      local_get(k); local_get(total); i32_ge_u; br_if(1);
                      // xv = a[k]
                      local_get(a); i32_const(8); i32_add;
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
                      local_get(dst); i32_const(8); i32_add;
                      local_get(k); i32_const(8); i32_mul; i32_add;
                      local_get(xv); local_get(sig); f64_mul;
                      local_get(b); i32_const(8); i32_add;
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
                let k_local = self.scratch.alloc_i32();
                let block_start = self.scratch.alloc_i32();
                let bits_start = self.scratch.alloc_i32();
                let scale_raw = self.scratch.alloc_i32();
                let sign = self.scratch.alloc_i32();
                let expv = self.scratch.alloc_i32();
                let mant = self.scratch.alloc_i32();
                let f32bits = self.scratch.alloc_i32();
                let byte_idx = self.scratch.alloc_i32();
                let bit = self.scratch.alloc_i32();
                let sum = self.scratch.alloc_f64();
                let scale = self.scratch.alloc_f64();
                let neg_scale = self.scratch.alloc_f64();

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
                    local_get(w_bytes); i32_const(4); i32_add;
                    local_get(w_off); i32_add;
                    local_set(w_data);
                    // n_bpr = n_in / 128
                    local_get(n_in); i32_const(7); i32_shr_u; local_set(n_bpr);
                    // dst = alloc(8 + x_rows*out*8); header
                    local_get(x_rows); local_get(out); i32_mul;
                    i32_const(8); i32_mul;
                    i32_const(8); i32_add;
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
                        f64_const(0.0); local_set(sum);
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
                          // for local_k in 0..128
                          i32_const(0); local_set(k_local);
                          block_empty; loop_empty;
                            local_get(k_local); i32_const(128); i32_ge_u; br_if(1);
                            // byte_idx = bits_start + (local_k >> 3)
                            local_get(bits_start);
                            local_get(k_local); i32_const(3); i32_shr_u;
                            i32_add;
                            local_set(byte_idx);
                            local_get(byte_idx); i32_load8_u(0);
                            local_get(k_local); i32_const(7); i32_and; i32_shr_u;
                            i32_const(1); i32_and;
                            local_set(bit);
                            // x_val = x[i, b*128 + local_k]
                            local_get(x); i32_const(8); i32_add;
                            local_get(i); local_get(n_in); i32_mul;
                            local_get(b); i32_const(128); i32_mul;
                            local_get(k_local);
                            i32_add; i32_add;
                            i32_const(8); i32_mul;
                            i32_add;
                            f64_load(0);
                            // multiply by w_val = bit ? scale : neg_scale
                            local_get(bit);
                            if_f64;
                              local_get(scale);
                            else_;
                              local_get(neg_scale);
                            end;
                            f64_mul;
                            local_get(sum); f64_add; local_set(sum);
                            local_get(k_local); i32_const(1); i32_add; local_set(k_local);
                            br(0);
                          end; end;
                          local_get(b); i32_const(1); i32_add; local_set(b);
                          br(0);
                        end; end;
                        // dst[i, j] = sum
                        local_get(dst); i32_const(8); i32_add;
                        local_get(i); local_get(out); i32_mul;
                        local_get(j); i32_add;
                        i32_const(8); i32_mul;
                        i32_add;
                        local_get(sum);
                        f64_store(0);
                        local_get(j); i32_const(1); i32_add; local_set(j);
                        br(0);
                      end; end;
                      local_get(i); i32_const(1); i32_add; local_set(i);
                      br(0);
                    end; end;
                    local_get(dst);
                });
                self.scratch.free_f64(neg_scale);
                self.scratch.free_f64(scale);
                self.scratch.free_f64(sum);
                self.scratch.free_i32(bit);
                self.scratch.free_i32(byte_idx);
                self.scratch.free_i32(f32bits);
                self.scratch.free_i32(mant);
                self.scratch.free_i32(expv);
                self.scratch.free_i32(sign);
                self.scratch.free_i32(scale_raw);
                self.scratch.free_i32(bits_start);
                self.scratch.free_i32(block_start);
                self.scratch.free_i32(k_local);
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
                    i32_const(8); i32_add;
                    call(self.emitter.rt.alloc); local_set(dst);
                    local_get(dst); local_get(n_out); i32_store(0);
                    local_get(dst); local_get(cols); i32_store(4);
                    i32_const(0); local_set(k);
                    block_empty; loop_empty;
                      local_get(k); local_get(n_out); i32_ge_u; br_if(1);
                      // rid = (i64)ids[4 + k*8] as i32
                      local_get(ids); i32_const(4); i32_add;
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
                        local_get(dst); i32_const(8); i32_add;
                        local_get(k); local_get(row_bytes); i32_mul; i32_add;
                        i32_const(0);
                        local_get(row_bytes);
                        memory_fill;
                      else_;
                        // memcpy(dst + 8 + k*row_bytes, m + 8 + rid*row_bytes, row_bytes)
                        local_get(dst); i32_const(8); i32_add;
                        local_get(k); local_get(row_bytes); i32_mul; i32_add;
                        local_get(m); i32_const(8); i32_add;
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
            "rope_rotate" => {
                // matrix.rope_rotate(x: Matrix, n_heads: Int, head_dim: Int, theta_base: Float) -> Matrix
                //
                // Standard RoPE: pair each head's (x[2i], x[2i+1]) and rotate
                // by `row_idx * inv_freq[i]` where `inv_freq[i] = theta_base ^ (-2i/head_dim)`.
                //
                // Uses math.log + math.exp to compute the per-pair inverse
                // frequency (WASM has no direct `pow`), and math.sin / cos
                // for the rotation. We pre-copy the input matrix bytes so
                // any columns outside the rotated region (n_heads*head_dim
                // < cols) keep their original values.
                let x = self.scratch.alloc_i32();
                let n_heads = self.scratch.alloc_i32();
                let head_dim = self.scratch.alloc_i32();
                let half = self.scratch.alloc_i32();
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
                    i32_const(8); i32_add; local_set(total_bytes);
                    local_get(total_bytes); call(self.emitter.rt.alloc); local_set(dst);
                    // memory.copy(dst, x, total_bytes)
                    local_get(dst); local_get(x); local_get(total_bytes); memory_copy;
                    i32_const(0); local_set(p);
                    block_empty; loop_empty;
                      local_get(p); local_get(rows); i32_ge_u; br_if(1);
                      local_get(p); f64_convert_i32_u; local_set(pos_f);
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
                self.scratch.free_i32(half);
                self.scratch.free_i32(head_dim);
                self.scratch.free_i32(n_heads);
                self.scratch.free_i32(x);
            }
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
                    i32_const(8); i32_add;
                    call(self.emitter.rt.alloc); local_set(dst);
                    local_get(dst); local_get(rows); i32_store(0);
                    local_get(dst); local_get(cols); i32_store(4);
                    // data_off = data_ptr + 4 (skip bytes-len header) + offset
                    local_get(data); i32_const(4); i32_add;
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
                      local_get(dst); i32_const(8); i32_add;
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
                    i32_const(8); i32_add;
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
                          local_get(x); i32_const(8); i32_add;
                          local_get(i); local_get(nin); i32_mul; local_get(k); i32_add;
                          i32_const(8); i32_mul; i32_add; f64_load(0);
                          // weight[j,k]
                          local_get(w); i32_const(8); i32_add;
                          local_get(j); local_get(nin); i32_mul; local_get(k); i32_add;
                          i32_const(8); i32_mul; i32_add; f64_load(0);
                          f64_mul; local_get(s); f64_add; local_set(s);
                          local_get(k); i32_const(1); i32_add; local_set(k);
                          br(0);
                        end; end;
                        // dst[i,j] = s + bias[j] (if with_bias)
                        local_get(dst); i32_const(8); i32_add;
                        local_get(i); local_get(wr); i32_mul; local_get(j); i32_add;
                        i32_const(8); i32_mul; i32_add;
                        local_get(s);
                });
                if with_bias {
                    wasm!(self.func, {
                        local_get(b); i32_const(4); i32_add;
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
                    i32_const(8); i32_add;
                    call(self.emitter.rt.alloc); local_set(dst);
                    local_get(dst); local_get(rows); i32_store(0);
                    local_get(dst); local_get(cols); i32_store(4);
                    i32_const(0); local_set(r);
                    block_empty; loop_empty;
                      local_get(r); local_get(rows); i32_ge_u; br_if(1);
                      i32_const(0); local_set(c);
                      block_empty; loop_empty;
                        local_get(c); local_get(cols); i32_ge_u; br_if(1);
                        local_get(dst); i32_const(8); i32_add;
                        local_get(r); local_get(cols); i32_mul; local_get(c); i32_add;
                        i32_const(8); i32_mul; i32_add;
                        local_get(m); i32_const(8); i32_add;
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
                    i32_const(8); i32_add;
                    call(self.emitter.rt.alloc); local_set(dst);
                    local_get(dst); local_get(sq); i32_store(0);
                    local_get(dst); local_get(dm); i32_store(4);
                    local_get(dst); i32_const(8); i32_add;
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
                            local_get(q); i32_const(8); i32_add;
                            local_get(i); local_get(dm); i32_mul;
                            local_get(col0); i32_add; local_get(kki); i32_add;
                            i32_const(8); i32_mul; i32_add; f64_load(0);
                            // k[j, col0+kki]
                            local_get(kk); i32_const(8); i32_add;
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
                          // Add -1e9 if j > i
                          local_get(j); local_get(i); i32_gt_u;
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
                            local_get(dst); i32_const(8); i32_add;
                            local_get(i); local_get(dm); i32_mul;
                            local_get(col0); i32_add; local_get(kki); i32_add;
                            i32_const(8); i32_mul; i32_add;
                            local_get(dst); i32_const(8); i32_add;
                            local_get(i); local_get(dm); i32_mul;
                            local_get(col0); i32_add; local_get(kki); i32_add;
                            i32_const(8); i32_mul; i32_add; f64_load(0);
                            local_get(vv); i32_const(8); i32_add;
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
                    // alloc 4 + bytes_len
                    local_get(bytes_len); i32_const(4); i32_add;
                    call(self.emitter.rt.alloc); local_set(dst);
                    local_get(dst); local_get(bytes_len); i32_store(0);
                    // memcpy: dst+4 ← m+8, bytes_len bytes
                    local_get(dst); i32_const(4); i32_add;
                    local_get(m); i32_const(8); i32_add;
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
                    local_get(bytes_len); i32_const(4); i32_add;
                    call(self.emitter.rt.alloc); local_set(dst);
                    local_get(dst); local_get(bytes_len); i32_store(0);
                    i32_const(0); local_set(i);
                    block_empty; loop_empty;
                      local_get(i); local_get(total); i32_ge_u; br_if(1);
                      local_get(m); i32_const(8); i32_add; local_get(i); i32_const(8); i32_mul; i32_add; local_set(src_addr);
                      local_get(dst); i32_const(4); i32_add; local_get(i); i32_const(4); i32_mul; i32_add; local_set(dst_addr);
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
                    local_get(total); i32_const(8); i32_mul; i32_const(8); i32_add;
                    call(self.emitter.rt.alloc); local_set(dst);
                    local_get(dst); local_get(r); i32_store(0);
                    local_get(dst); local_get(c); i32_store(4);
                    // memcpy: dst+8 ← buf+4+off, total*8 bytes
                    local_get(dst); i32_const(8); i32_add;
                    local_get(buf); i32_const(4); i32_add; local_get(off); i32_add;
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
                    local_get(total); i32_const(8); i32_mul; i32_const(8); i32_add;
                    call(self.emitter.rt.alloc); local_set(dst);
                    local_get(dst); local_get(r); i32_store(0);
                    local_get(dst); local_get(c); i32_store(4);
                    i32_const(0); local_set(i);
                    block_empty; loop_empty;
                      local_get(i); local_get(total); i32_ge_u; br_if(1);
                      // dst[data] + i * 8
                      local_get(dst); i32_const(8); i32_add;
                      local_get(i); i32_const(8); i32_mul; i32_add;
                      // src = buf + 4 + off + i * elem_bytes
                      local_get(buf); i32_const(4); i32_add; local_get(off); i32_add;
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
                    i32_const(8); i32_add;
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
                        local_get(b); i32_const(4); i32_add;
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
                              local_get(w); i32_const(8); i32_add;
                              local_get(o); local_get(ich); i32_mul; local_get(kk); i32_mul;
                              local_get(cc); local_get(kk); i32_mul; i32_add;
                              local_get(ki); i32_add;
                              i32_const(8); i32_mul; i32_add; f64_load(0);
                              // input[tc][cc]
                              local_get(inp); i32_const(8); i32_add;
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
                        local_get(dst); i32_const(8); i32_add;
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
                    local_get(n); i32_const(4); i32_mul; i32_const(4); i32_add;
                    call(self.emitter.rt.alloc); local_set(list_ptr);
                    local_get(list_ptr); local_get(n); i32_store(0);
                    i32_const(0); local_set(h);
                    block_empty; loop_empty;
                      local_get(h); local_get(n); i32_ge_u; br_if(1);
                      // Alloc sub-matrix (rows, chunk)
                      local_get(rows); local_get(chunk); i32_mul; i32_const(8); i32_mul;
                      i32_const(8); i32_add;
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
                          local_get(sub); i32_const(8); i32_add;
                          local_get(r); local_get(chunk); i32_mul; local_get(c); i32_add;
                          i32_const(8); i32_mul; i32_add;
                          local_get(m); i32_const(8); i32_add;
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
                      local_get(list_ptr); i32_const(4); i32_add;
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
                    local_get(lst); i32_const(4); i32_add; i32_load(0); local_set(first);
                    local_get(first); i32_load(0); local_set(rows);
                    // Sum total_cols
                    i32_const(0); local_set(total_cols);
                    i32_const(0); local_set(i);
                    block_empty; loop_empty;
                      local_get(i); local_get(n); i32_ge_u; br_if(1);
                      local_get(lst); i32_const(4); i32_add;
                      local_get(i); i32_const(4); i32_mul; i32_add;
                      i32_load(0); local_set(sub);
                      local_get(total_cols); local_get(sub); i32_load(4); i32_add;
                      local_set(total_cols);
                      local_get(i); i32_const(1); i32_add; local_set(i);
                      br(0);
                    end; end;
                    // Alloc dst (rows, total_cols)
                    local_get(rows); local_get(total_cols); i32_mul; i32_const(8); i32_mul;
                    i32_const(8); i32_add;
                    call(self.emitter.rt.alloc); local_set(dst);
                    local_get(dst); local_get(rows); i32_store(0);
                    local_get(dst); local_get(total_cols); i32_store(4);
                    // Fill: for each submatrix, copy its rows into dst at col_off
                    i32_const(0); local_set(col_off);
                    i32_const(0); local_set(i);
                    block_empty; loop_empty;
                      local_get(i); local_get(n); i32_ge_u; br_if(1);
                      local_get(lst); i32_const(4); i32_add;
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
                          local_get(dst); i32_const(8); i32_add;
                          local_get(r); local_get(total_cols); i32_mul;
                          local_get(col_off); i32_add; local_get(c); i32_add;
                          i32_const(8); i32_mul; i32_add;
                          local_get(sub); i32_const(8); i32_add;
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
                    i32_const(8); i32_add;
                    call(self.emitter.rt.alloc); local_set(dst);
                    local_get(dst); local_get(n); i32_store(0);
                    local_get(dst); local_get(cols); i32_store(4);
                    i32_const(0); local_set(i);
                    block_empty; loop_empty;
                      local_get(i); local_get(n); i32_ge_u; br_if(1);
                      // idx = indices[i] (i64 → i32)
                      local_get(indices); i32_const(4); i32_add;
                      local_get(i); i32_const(8); i32_mul; i32_add;
                      i64_load(0); i32_wrap_i64; local_set(idx);
                      // bounds clamp: if idx >= n_rows_src: zero the row, else memcpy
                      local_get(idx); local_get(n_rows_src); i32_ge_u;
                      if_empty;
                        // Zero
                        local_get(dst); i32_const(8); i32_add;
                        local_get(i); local_get(cols); i32_mul; i32_const(8); i32_mul; i32_add;
                        i32_const(0);
                        local_get(cols); i32_const(8); i32_mul;
                        memory_fill;
                      else_;
                        // memcpy: dst[i] ← m[idx]
                        local_get(dst); i32_const(8); i32_add;
                        local_get(i); local_get(cols); i32_mul; i32_const(8); i32_mul; i32_add;
                        local_get(m); i32_const(8); i32_add;
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
                      local_get(m); i32_const(8); i32_add;
                      local_get(row); local_get(cols); i32_mul; local_get(i); i32_add;
                      i32_const(8); i32_mul; i32_add; f64_load(0);
                      // vec[i]
                      local_get(vec); i32_const(4); i32_add;
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
