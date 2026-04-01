//! Matrix stdlib call dispatch for WASM codegen.
//!
//! Memory layout: [rows:i32][cols:i32][data:f64...]  (row-major, 8 bytes per element)
//! Total size: 8 + rows*cols*8

use super::FuncCompiler;
use almide_ir::IrExpr;

impl FuncCompiler<'_> {
    /// Dispatch a matrix stdlib method call. Returns true if handled.
    pub(super) fn emit_matrix_call(&mut self, method: &str, args: &[IrExpr]) -> bool {
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
            "add" | "sub" => {
                // element-wise add/sub
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
                if method == "add" {
                    wasm!(self.func, { f64_add; });
                } else {
                    wasm!(self.func, { f64_sub; });
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
            _ => return false,
        }
        true
    }
}
