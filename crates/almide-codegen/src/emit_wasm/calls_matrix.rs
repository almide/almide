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
                // element-wise add/sub/div
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
                    block_empty; loop_empty;
                      local_get(i); local_get(total); i32_ge_u; br_if(1);
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
                      local_get(i); i32_const(1); i32_add; local_set(i);
                      br(0);
                    end; end;
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
                    block_empty; loop_empty;
                      local_get(i); local_get(total); i32_ge_u; br_if(1);
                      local_get(dst); i32_const(8); i32_add; local_get(i); i32_const(8); i32_mul; i32_add;
                      local_get(m); i32_const(8); i32_add; local_get(i); i32_const(8); i32_mul; i32_add;
                      f64_load(0); local_get(s); f64_mul;
                      f64_store(0);
                      local_get(i); i32_const(1); i32_add; local_set(i);
                      br(0);
                    end; end;
                    local_get(dst);
                });
                self.scratch.free_f64(s);
                self.scratch.free_i32(i);
                self.scratch.free_i32(total);
                self.scratch.free_i32(dst);
                self.scratch.free_i32(m);
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
