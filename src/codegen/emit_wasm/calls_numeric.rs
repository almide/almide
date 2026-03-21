//! Float, Int, and Math stdlib call dispatch for WASM codegen.

use super::FuncCompiler;
use super::values;
use crate::ir::IrExpr;
use crate::types::Ty;
use wasm_encoder::{Instruction, ValType};

impl FuncCompiler<'_> {
    /// Dispatch a float stdlib method call. Returns true if handled.
    pub(super) fn emit_float_call(&mut self, method: &str, args: &[IrExpr]) -> bool {
        match method {
            "to_string" => {
                self.emit_expr(&args[0]);
                wasm!(self.func, { call(self.emitter.rt.float_to_string); });
            }
            "to_int" => {
                // truncate f64 → i64
                self.emit_expr(&args[0]);
                wasm!(self.func, { i64_trunc_f64_s; });
            }
            "round" => {
                // floor(x + 0.5) — standard rounding (half-up), not banker's rounding
                self.emit_expr(&args[0]);
                wasm!(self.func, { f64_const(0.5); f64_add; f64_floor; });
            }
            "floor" => {
                self.emit_expr(&args[0]);
                wasm!(self.func, { f64_floor; });
            }
            "ceil" => {
                self.emit_expr(&args[0]);
                wasm!(self.func, { f64_ceil; });
            }
            "abs" => {
                self.emit_expr(&args[0]);
                wasm!(self.func, { f64_abs; });
            }
            "sqrt" => {
                self.emit_expr(&args[0]);
                wasm!(self.func, { f64_sqrt; });
            }
            "from_int" => {
                self.emit_expr(&args[0]);
                wasm!(self.func, { f64_convert_i64_s; });
            }
            "sign" => {
                // sign(n) → -1.0, 0.0, or 1.0
                // Store f64 in mem[0] (as 8 bytes) since no f64 scratch local
                wasm!(self.func, { i32_const(0); });
                self.emit_expr(&args[0]);
                wasm!(self.func, {
                    f64_store(0);
                    i32_const(0); f64_load(0); f64_const(0.0); f64_lt;
                    if_f64;
                      f64_const(-1.0);
                    else_;
                      i32_const(0); f64_load(0); f64_const(0.0); f64_gt;
                      if_f64;
                        f64_const(1.0);
                      else_;
                        f64_const(0.0);
                      end;
                    end;
                });
            }
            "min" => {
                self.emit_expr(&args[0]);
                self.emit_expr(&args[1]);
                self.func.instruction(&Instruction::F64Min);
            }
            "max" => {
                self.emit_expr(&args[0]);
                self.emit_expr(&args[1]);
                self.func.instruction(&Instruction::F64Max);
            }
            "clamp" => {
                // clamp(n, lo, hi) = max(lo, min(n, hi))
                self.emit_expr(&args[1]); // lo
                self.emit_expr(&args[0]); // n
                self.emit_expr(&args[2]); // hi
                self.func.instruction(&Instruction::F64Min); // min(n, hi)
                self.func.instruction(&Instruction::F64Max); // max(lo, min(n, hi))
            }
            "is_nan" => {
                // NaN != NaN. Store in mem to avoid double eval.
                wasm!(self.func, { i32_const(0); });
                self.emit_expr(&args[0]);
                wasm!(self.func, {
                    f64_store(0);
                    i32_const(0); f64_load(0);
                    i32_const(0); f64_load(0);
                    f64_ne;
                });
            }
            "is_infinite" => {
                self.emit_expr(&args[0]);
                wasm!(self.func, { f64_abs; f64_const(f64::INFINITY); f64_eq; });
            }
            "parse" => {
                // float.parse(s: String) → Result[Float, String]
                self.emit_expr(&args[0]);
                wasm!(self.func, { call(self.emitter.rt.float_parse); });
            }
            "to_fixed" => {
                // float.to_fixed(n: Float, decimals: Int) → String
                self.emit_expr(&args[0]);
                self.emit_expr(&args[1]);
                wasm!(self.func, { call(self.emitter.rt.float_to_fixed); });
            }
            _ => return false,
        }
        true
    }

    /// Dispatch an int stdlib method call. Returns true if handled.
    pub(super) fn emit_int_call(&mut self, method: &str, args: &[IrExpr]) -> bool {
        match method {
            "to_string" => {
                self.emit_expr(&args[0]);
                wasm!(self.func, { call(self.emitter.rt.int_to_string); });
            }
            "parse" => {
                self.emit_expr(&args[0]);
                wasm!(self.func, { call(self.emitter.rt.int_parse); });
            }
            "abs" => {
                self.emit_expr(&args[0]);
                let s = self.match_i64_base + self.match_depth;
                wasm!(self.func, {
                    local_set(s);
                    local_get(s); i64_const(0); i64_lt_s;
                    if_i64;
                      i64_const(0); local_get(s); i64_sub;
                    else_;
                      local_get(s);
                    end;
                });
            }
            "min" => {
                self.emit_expr(&args[0]);
                self.emit_expr(&args[1]);
                let s = self.match_i64_base + self.match_depth;
                wasm!(self.func, {
                    local_set(s); local_set(s + 1);
                    local_get(s + 1); local_get(s); i64_lt_s;
                    if_i64; local_get(s + 1); else_; local_get(s); end;
                });
            }
            "max" => {
                self.emit_expr(&args[0]);
                self.emit_expr(&args[1]);
                let s = self.match_i64_base + self.match_depth;
                wasm!(self.func, {
                    local_set(s); local_set(s + 1);
                    local_get(s + 1); local_get(s); i64_gt_s;
                    if_i64; local_get(s + 1); else_; local_get(s); end;
                });
            }
            "clamp" => {
                // clamp(n, lo, hi) = max(lo, min(n, hi))
                self.emit_expr(&args[0]); // n
                self.emit_expr(&args[1]); // lo
                self.emit_expr(&args[2]); // hi
                let s = self.match_i64_base + self.match_depth;
                wasm!(self.func, {
                    local_set(s);       // hi
                    local_set(s + 1);   // lo
                    local_set(s + 2);   // n
                    // min(n, hi)
                    local_get(s + 2); local_get(s); i64_lt_s;
                    if_i64; local_get(s + 2); else_; local_get(s); end;
                    // max(lo, result)
                    local_set(s + 2); // temp = min(n, hi)
                    local_get(s + 1); local_get(s + 2); i64_gt_s;
                    if_i64; local_get(s + 1); else_; local_get(s + 2); end;
                });
            }
            "to_float" => {
                self.emit_expr(&args[0]);
                wasm!(self.func, { f64_convert_i64_s; });
            }
            "band" => {
                self.emit_expr(&args[0]);
                self.emit_expr(&args[1]);
                wasm!(self.func, { i64_and; });
            }
            "bor" => {
                self.emit_expr(&args[0]);
                self.emit_expr(&args[1]);
                wasm!(self.func, { i64_or; });
            }
            "bxor" => {
                self.emit_expr(&args[0]);
                self.emit_expr(&args[1]);
                wasm!(self.func, { i64_xor; });
            }
            "bshl" => {
                self.emit_expr(&args[0]);
                self.emit_expr(&args[1]);
                wasm!(self.func, { i64_shl; });
            }
            "bshr" => {
                self.emit_expr(&args[0]);
                self.emit_expr(&args[1]);
                wasm!(self.func, { i64_shr_s; });
            }
            "bnot" => {
                self.emit_expr(&args[0]);
                wasm!(self.func, { i64_const(-1); i64_xor; });
            }
            "to_u32" => {
                self.emit_expr(&args[0]);
                wasm!(self.func, { i64_const(0xFFFFFFFF); i64_and; });
            }
            "to_u8" => {
                self.emit_expr(&args[0]);
                wasm!(self.func, { i64_const(0xFF); i64_and; });
            }
            "wrap_add" => {
                // wrap_add(a, b, bits)
                self.emit_expr(&args[0]);
                self.emit_expr(&args[1]);
                wasm!(self.func, { i64_add; });
                self.emit_expr(&args[2]);
                // mask = (1 << bits) - 1
                wasm!(self.func, {
                    local_set(self.match_i64_base + self.match_depth);
                    i64_const(1);
                    local_get(self.match_i64_base + self.match_depth);
                    i64_shl;
                    i64_const(1);
                    i64_sub;
                    i64_and;
                });
            }
            "wrap_mul" => {
                self.emit_expr(&args[0]);
                self.emit_expr(&args[1]);
                wasm!(self.func, { i64_mul; });
                self.emit_expr(&args[2]);
                wasm!(self.func, {
                    local_set(self.match_i64_base + self.match_depth);
                    i64_const(1);
                    local_get(self.match_i64_base + self.match_depth);
                    i64_shl;
                    i64_const(1);
                    i64_sub;
                    i64_and;
                });
            }
            "to_hex" => {
                // to_hex(n: Int) → String: hex lowercase
                // Alloc temp buffer (20 bytes max), write digits in reverse, then create result
                let s = self.match_i32_base + self.match_depth;
                let s64 = self.match_i64_base + self.match_depth;
                self.emit_expr(&args[0]);
                wasm!(self.func, {
                    local_set(s64);
                    local_get(s64); i64_eqz;
                    if_i32;
                      i32_const(5); call(self.emitter.rt.alloc); local_set(s);
                      local_get(s); i32_const(1); i32_store(0);
                      local_get(s); i32_const(48); i32_store8(4);
                      local_get(s);
                    else_;
                      // Alloc temp buffer for reversed digits
                      i32_const(20); call(self.emitter.rt.alloc); local_set(s); // buf
                      i32_const(0); local_set(s + 1); // count
                });
                wasm!(self.func, {
                      block_empty; loop_empty;
                        local_get(s64); i64_eqz; br_if(1);
                        local_get(s64); i64_const(16); i64_rem_u; i32_wrap_i64;
                        local_set(s + 2); // digit
                        local_get(s + 2); i32_const(10); i32_lt_u;
                        if_i32; local_get(s + 2); i32_const(48); i32_add;
                        else_; local_get(s + 2); i32_const(87); i32_add; end;
                        local_set(s + 2); // char
                        local_get(s); local_get(s + 1); i32_add;
                        local_get(s + 2); i32_store8(0);
                        local_get(s + 1); i32_const(1); i32_add; local_set(s + 1);
                        local_get(s64); i64_const(16); i64_div_u; local_set(s64);
                        br(0);
                      end; end;
                });
                wasm!(self.func, {
                      // Alloc result string
                      i32_const(4); local_get(s + 1); i32_add;
                      call(self.emitter.rt.alloc); local_set(s + 2);
                      local_get(s + 2); local_get(s + 1); i32_store(0);
                      // Copy reversed
                      i32_const(0); local_set(s + 3);
                      block_empty; loop_empty;
                        local_get(s + 3); local_get(s + 1); i32_ge_u; br_if(1);
                        local_get(s + 2); i32_const(4); i32_add; local_get(s + 3); i32_add;
                        local_get(s);
                        local_get(s + 1); i32_const(1); i32_sub; local_get(s + 3); i32_sub;
                        i32_add; i32_load8_u(0);
                        i32_store8(0);
                        local_get(s + 3); i32_const(1); i32_add; local_set(s + 3);
                        br(0);
                      end; end;
                      local_get(s + 2);
                    end;
                });
            }
            "from_hex" => {
                self.emit_expr(&args[0]);
                wasm!(self.func, { call(self.emitter.rt.int_from_hex); });
            }
            "rotate_right" | "rotate_left" => {
                self.emit_stub_call(args);
                return true;
            }
            _ => return false,
        }
        true
    }

    /// Dispatch a math stdlib method call. Returns true if handled.
    pub(super) fn emit_math_call(&mut self, method: &str, args: &[IrExpr]) -> bool {
        match method {
            "pi" => {
                wasm!(self.func, { f64_const(std::f64::consts::PI); });
            }
            "e" => {
                wasm!(self.func, { f64_const(std::f64::consts::E); });
            }
            "sqrt" => {
                self.emit_expr(&args[0]);
                if matches!(&args[0].ty, Ty::Int) {
                    wasm!(self.func, { f64_convert_i64_s; });
                }
                wasm!(self.func, { f64_sqrt; });
            }
            "abs" => {
                self.emit_expr(&args[0]);
                match &args[0].ty {
                    Ty::Float => { wasm!(self.func, { f64_abs; }); }
                    _ => {
                        let s = self.match_i64_base + self.match_depth;
                        wasm!(self.func, {
                            local_set(s);
                            local_get(s); i64_const(0); i64_lt_s;
                            if_i64; i64_const(0); local_get(s); i64_sub;
                            else_; local_get(s); end;
                        });
                    }
                }
            }
            "max" => {
                self.emit_expr(&args[0]);
                self.emit_expr(&args[1]);
                let s = self.match_i64_base + self.match_depth;
                wasm!(self.func, {
                    local_set(s); local_set(s + 1);
                    local_get(s + 1); local_get(s); i64_gt_s;
                    if_i64; local_get(s + 1); else_; local_get(s); end;
                });
            }
            "min" => {
                self.emit_expr(&args[0]);
                self.emit_expr(&args[1]);
                let s = self.match_i64_base + self.match_depth;
                wasm!(self.func, {
                    local_set(s); local_set(s + 1);
                    local_get(s + 1); local_get(s); i64_lt_s;
                    if_i64; local_get(s + 1); else_; local_get(s); end;
                });
            }
            "fmin" => {
                self.emit_expr(&args[0]);
                self.emit_expr(&args[1]);
                self.func.instruction(&Instruction::F64Min);
            }
            "fmax" => {
                self.emit_expr(&args[0]);
                self.emit_expr(&args[1]);
                self.func.instruction(&Instruction::F64Max);
            }
            "sin" => {
                self.emit_expr(&args[0]);
                if matches!(&args[0].ty, Ty::Int) {
                    wasm!(self.func, { f64_convert_i64_s; });
                }
                wasm!(self.func, { call(self.emitter.rt.math_sin); });
            }
            "cos" => {
                self.emit_expr(&args[0]);
                if matches!(&args[0].ty, Ty::Int) {
                    wasm!(self.func, { f64_convert_i64_s; });
                }
                wasm!(self.func, { call(self.emitter.rt.math_cos); });
            }
            "tan" => {
                self.emit_expr(&args[0]);
                if matches!(&args[0].ty, Ty::Int) {
                    wasm!(self.func, { f64_convert_i64_s; });
                }
                wasm!(self.func, { call(self.emitter.rt.math_tan); });
            }
            "log" | "exp" | "log10" | "log2" => {
                self.emit_stub_call(args);
                return true;
            }
            "sign" => {
                // math.sign(n: Int) → Int (-1, 0, 1)
                self.emit_expr(&args[0]);
                let s = self.match_i64_base + self.match_depth;
                wasm!(self.func, {
                    local_set(s);
                    local_get(s); i64_const(0); i64_lt_s;
                    if_i64; i64_const(-1);
                    else_;
                      local_get(s); i64_const(0); i64_gt_s;
                      if_i64; i64_const(1);
                      else_; i64_const(0);
                      end;
                    end;
                });
            }
            "pow" => {
                // pow(base: Int, exp: Int) → Int
                // Loop: result = 1; for i in 0..exp: result *= base
                self.emit_expr(&args[0]); // base
                self.emit_expr(&args[1]); // exp
                let s = self.match_i64_base + self.match_depth;
                wasm!(self.func, {
                    local_set(s);       // exp
                    local_set(s + 1);   // base
                    i64_const(1);
                    local_set(s + 2);   // result = 1
                    i64_const(0);
                    local_set(s + 3);   // i = 0
                    block_empty; loop_empty;
                      local_get(s + 3); local_get(s); i64_ge_s; br_if(1);
                      local_get(s + 2); local_get(s + 1); i64_mul; local_set(s + 2);
                      local_get(s + 3); i64_const(1); i64_add; local_set(s + 3);
                      br(0);
                    end; end;
                    local_get(s + 2);
                });
            }
            "fpow" => {
                // fpow(base: Float, exp: Float) → Float
                self.emit_expr(&args[0]);
                self.emit_expr(&args[1]);
                wasm!(self.func, { call(self.emitter.rt.float_pow); });
            }
            "factorial" => {
                // factorial(n) → Int, loop
                self.emit_expr(&args[0]);
                let s = self.match_i64_base + self.match_depth;
                wasm!(self.func, {
                    local_set(s); // n
                    i64_const(1); local_set(s + 1); // result
                    i64_const(2); local_set(s + 2); // i
                    block_empty; loop_empty;
                      local_get(s + 2); local_get(s); i64_gt_s; br_if(1);
                      local_get(s + 1); local_get(s + 2); i64_mul; local_set(s + 1);
                      local_get(s + 2); i64_const(1); i64_add; local_set(s + 2);
                      br(0);
                    end; end;
                    local_get(s + 1);
                });
            }
            "choose" => {
                // choose(n, k) = n! / (k! * (n-k)!)
                // Iterative: result = 1; for i in 0..k: result = result * (n-i) / (i+1)
                self.emit_expr(&args[0]); // n
                self.emit_expr(&args[1]); // k
                let s = self.match_i64_base + self.match_depth;
                wasm!(self.func, {
                    local_set(s);       // k
                    local_set(s + 1);   // n
                    i64_const(1); local_set(s + 2); // result
                    i64_const(0); local_set(s + 3); // i
                    block_empty; loop_empty;
                      local_get(s + 3); local_get(s); i64_ge_s; br_if(1);
                      // result = result * (n - i) / (i + 1)
                      local_get(s + 2);
                      local_get(s + 1); local_get(s + 3); i64_sub;
                      i64_mul;
                      local_get(s + 3); i64_const(1); i64_add;
                      i64_div_s;
                      local_set(s + 2);
                      local_get(s + 3); i64_const(1); i64_add; local_set(s + 3);
                      br(0);
                    end; end;
                    local_get(s + 2);
                });
            }
            "log_gamma" => {
                self.emit_stub_call(args);
                return true;
            }
            _ => return false,
        }
        true
    }
}
