//! Float, Int, and Math stdlib call dispatch for WASM codegen.

use super::FuncCompiler;
use crate::ir::IrExpr;
use crate::types::Ty;
use wasm_encoder::Instruction;

impl FuncCompiler<'_> {
    /// Dispatch a float stdlib method call. Returns true if handled.
    pub(super) fn emit_float_call(&mut self, method: &str, args: &[IrExpr]) -> bool {
        match method {
            "to_string" => {
                self.emit_expr(&args[0]);
                wasm!(self.func, { call(self.emitter.rt.float_to_string); });
            }
            "to_int" => {
                // truncate f64 → i64 (saturating: NaN→0, ±Inf→i64::MAX/MIN)
                self.emit_expr(&args[0]);
                self.func.instruction(&wasm_encoder::Instruction::I64TruncSatF64S);
            }
            "round" => {
                // Round half away from zero: copysign(floor(abs(x) + 0.5), x)
                let tmp = self.scratch.alloc_f64();
                self.emit_expr(&args[0]);
                wasm!(self.func, {
                    local_set(tmp);
                    local_get(tmp); // x
                    f64_abs; f64_const(0.5); f64_add; f64_floor; // floor(abs(x)+0.5)
                    local_get(tmp); // x (for sign)
                    f64_copysign; // copysign(magnitude, sign)
                });
                self.scratch.free_f64(tmp);
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
                let tmp = self.scratch.alloc_f64();
                self.emit_expr(&args[0]);
                wasm!(self.func, {
                    local_set(tmp);
                    local_get(tmp); f64_const(0.0); f64_lt;
                    if_f64;
                      f64_const(-1.0);
                    else_;
                      local_get(tmp); f64_const(0.0); f64_gt;
                      if_f64;
                        f64_const(1.0);
                      else_;
                        f64_const(0.0);
                      end;
                    end;
                });
                self.scratch.free_f64(tmp);
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
                // NaN != NaN. Store to avoid double eval.
                let tmp = self.scratch.alloc_f64();
                self.emit_expr(&args[0]);
                wasm!(self.func, {
                    local_set(tmp);
                    local_get(tmp);
                    local_get(tmp);
                    f64_ne;
                });
                self.scratch.free_f64(tmp);
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
                let s = self.scratch.alloc_i64();
                wasm!(self.func, {
                    local_set(s);
                    local_get(s); i64_const(0); i64_lt_s;
                    if_i64;
                      i64_const(0); local_get(s); i64_sub;
                    else_;
                      local_get(s);
                    end;
                });
                self.scratch.free_i64(s);
            }
            "min" => {
                self.emit_expr(&args[0]);
                self.emit_expr(&args[1]);
                let a = self.scratch.alloc_i64();
                let b = self.scratch.alloc_i64();
                wasm!(self.func, {
                    local_set(a); local_set(b);
                    local_get(b); local_get(a); i64_lt_s;
                    if_i64; local_get(b); else_; local_get(a); end;
                });
                self.scratch.free_i64(b);
                self.scratch.free_i64(a);
            }
            "max" => {
                self.emit_expr(&args[0]);
                self.emit_expr(&args[1]);
                let a = self.scratch.alloc_i64();
                let b = self.scratch.alloc_i64();
                wasm!(self.func, {
                    local_set(a); local_set(b);
                    local_get(b); local_get(a); i64_gt_s;
                    if_i64; local_get(b); else_; local_get(a); end;
                });
                self.scratch.free_i64(b);
                self.scratch.free_i64(a);
            }
            "clamp" => {
                // clamp(n, lo, hi) = max(lo, min(n, hi))
                self.emit_expr(&args[0]); // n
                self.emit_expr(&args[1]); // lo
                self.emit_expr(&args[2]); // hi
                let hi = self.scratch.alloc_i64();
                let lo = self.scratch.alloc_i64();
                let n = self.scratch.alloc_i64();
                wasm!(self.func, {
                    local_set(hi);       // hi
                    local_set(lo);   // lo
                    local_set(n);   // n
                    // min(n, hi)
                    local_get(n); local_get(hi); i64_lt_s;
                    if_i64; local_get(n); else_; local_get(hi); end;
                    // max(lo, result)
                    local_set(n); // temp = min(n, hi)
                    local_get(lo); local_get(n); i64_gt_s;
                    if_i64; local_get(lo); else_; local_get(n); end;
                });
                self.scratch.free_i64(n);
                self.scratch.free_i64(lo);
                self.scratch.free_i64(hi);
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
                let bits = self.scratch.alloc_i64();
                self.emit_expr(&args[0]);
                self.emit_expr(&args[1]);
                wasm!(self.func, { i64_add; });
                self.emit_expr(&args[2]);
                // mask = (1 << bits) - 1
                wasm!(self.func, {
                    local_set(bits);
                    i64_const(1);
                    local_get(bits);
                    i64_shl;
                    i64_const(1);
                    i64_sub;
                    i64_and;
                });
                self.scratch.free_i64(bits);
            }
            "wrap_mul" => {
                let bits = self.scratch.alloc_i64();
                self.emit_expr(&args[0]);
                self.emit_expr(&args[1]);
                wasm!(self.func, { i64_mul; });
                self.emit_expr(&args[2]);
                wasm!(self.func, {
                    local_set(bits);
                    i64_const(1);
                    local_get(bits);
                    i64_shl;
                    i64_const(1);
                    i64_sub;
                    i64_and;
                });
                self.scratch.free_i64(bits);
            }
            "to_hex" => {
                // to_hex(n: Int) → String: hex lowercase
                // Alloc temp buffer (20 bytes max), write digits in reverse, then create result
                let buf = self.scratch.alloc_i32();
                let count = self.scratch.alloc_i32();
                let digit = self.scratch.alloc_i32();
                let idx = self.scratch.alloc_i32();
                let n64 = self.scratch.alloc_i64();
                self.emit_expr(&args[0]);
                wasm!(self.func, {
                    local_set(n64);
                    local_get(n64); i64_eqz;
                    if_i32;
                      i32_const(5); call(self.emitter.rt.alloc); local_set(buf);
                      local_get(buf); i32_const(1); i32_store(0);
                      local_get(buf); i32_const(48); i32_store8(4);
                      local_get(buf);
                    else_;
                      // Alloc temp buffer for reversed digits
                      i32_const(20); call(self.emitter.rt.alloc); local_set(buf); // buf
                      i32_const(0); local_set(count); // count
                });
                wasm!(self.func, {
                      block_empty; loop_empty;
                        local_get(n64); i64_eqz; br_if(1);
                        local_get(n64); i64_const(16); i64_rem_u; i32_wrap_i64;
                        local_set(digit); // digit
                        local_get(digit); i32_const(10); i32_lt_u;
                        if_i32; local_get(digit); i32_const(48); i32_add;
                        else_; local_get(digit); i32_const(87); i32_add; end;
                        local_set(digit); // char
                        local_get(buf); local_get(count); i32_add;
                        local_get(digit); i32_store8(0);
                        local_get(count); i32_const(1); i32_add; local_set(count);
                        local_get(n64); i64_const(16); i64_div_u; local_set(n64);
                        br(0);
                      end; end;
                });
                wasm!(self.func, {
                      // Alloc result string
                      i32_const(4); local_get(count); i32_add;
                      call(self.emitter.rt.alloc); local_set(digit);
                      local_get(digit); local_get(count); i32_store(0);
                      // Copy reversed
                      i32_const(0); local_set(idx);
                      block_empty; loop_empty;
                        local_get(idx); local_get(count); i32_ge_u; br_if(1);
                        local_get(digit); i32_const(4); i32_add; local_get(idx); i32_add;
                        local_get(buf);
                        local_get(count); i32_const(1); i32_sub; local_get(idx); i32_sub;
                        i32_add; i32_load8_u(0);
                        i32_store8(0);
                        local_get(idx); i32_const(1); i32_add; local_set(idx);
                        br(0);
                      end; end;
                      local_get(digit);
                    end;
                });
                self.scratch.free_i64(n64);
                self.scratch.free_i32(idx);
                self.scratch.free_i32(digit);
                self.scratch.free_i32(count);
                self.scratch.free_i32(buf);
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
                        let s = self.scratch.alloc_i64();
                        wasm!(self.func, {
                            local_set(s);
                            local_get(s); i64_const(0); i64_lt_s;
                            if_i64; i64_const(0); local_get(s); i64_sub;
                            else_; local_get(s); end;
                        });
                        self.scratch.free_i64(s);
                    }
                }
            }
            "max" => {
                self.emit_expr(&args[0]);
                self.emit_expr(&args[1]);
                let a = self.scratch.alloc_i64();
                let b = self.scratch.alloc_i64();
                wasm!(self.func, {
                    local_set(a); local_set(b);
                    local_get(b); local_get(a); i64_gt_s;
                    if_i64; local_get(b); else_; local_get(a); end;
                });
                self.scratch.free_i64(b);
                self.scratch.free_i64(a);
            }
            "min" => {
                self.emit_expr(&args[0]);
                self.emit_expr(&args[1]);
                let a = self.scratch.alloc_i64();
                let b = self.scratch.alloc_i64();
                wasm!(self.func, {
                    local_set(a); local_set(b);
                    local_get(b); local_get(a); i64_lt_s;
                    if_i64; local_get(b); else_; local_get(a); end;
                });
                self.scratch.free_i64(b);
                self.scratch.free_i64(a);
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
            "log" => {
                self.emit_expr(&args[0]);
                if matches!(&args[0].ty, Ty::Int) {
                    wasm!(self.func, { f64_convert_i64_s; });
                }
                wasm!(self.func, { call(self.emitter.rt.math_log); });
            }
            "exp" => {
                self.emit_expr(&args[0]);
                if matches!(&args[0].ty, Ty::Int) {
                    wasm!(self.func, { f64_convert_i64_s; });
                }
                wasm!(self.func, { call(self.emitter.rt.math_exp); });
            }
            "log10" => {
                self.emit_expr(&args[0]);
                if matches!(&args[0].ty, Ty::Int) {
                    wasm!(self.func, { f64_convert_i64_s; });
                }
                wasm!(self.func, { call(self.emitter.rt.math_log10); });
            }
            "log2" => {
                self.emit_expr(&args[0]);
                if matches!(&args[0].ty, Ty::Int) {
                    wasm!(self.func, { f64_convert_i64_s; });
                }
                wasm!(self.func, { call(self.emitter.rt.math_log2); });
            }
            "sign" => {
                // math.sign(n: Int) → Int (-1, 0, 1)
                self.emit_expr(&args[0]);
                let s = self.scratch.alloc_i64();
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
                self.scratch.free_i64(s);
            }
            "pow" => {
                // pow(base: Int, exp: Int) → Int
                // Loop: result = 1; for i in 0..exp: result *= base
                self.emit_expr(&args[0]); // base
                self.emit_expr(&args[1]); // exp
                let exp = self.scratch.alloc_i64();
                let base = self.scratch.alloc_i64();
                let result = self.scratch.alloc_i64();
                let i = self.scratch.alloc_i64();
                wasm!(self.func, {
                    local_set(exp);       // exp
                    local_set(base);   // base
                    i64_const(1);
                    local_set(result);   // result = 1
                    i64_const(0);
                    local_set(i);   // i = 0
                    block_empty; loop_empty;
                      local_get(i); local_get(exp); i64_ge_s; br_if(1);
                      local_get(result); local_get(base); i64_mul; local_set(result);
                      local_get(i); i64_const(1); i64_add; local_set(i);
                      br(0);
                    end; end;
                    local_get(result);
                });
                self.scratch.free_i64(i);
                self.scratch.free_i64(result);
                self.scratch.free_i64(base);
                self.scratch.free_i64(exp);
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
                let n = self.scratch.alloc_i64();
                let result = self.scratch.alloc_i64();
                let i = self.scratch.alloc_i64();
                wasm!(self.func, {
                    local_set(n); // n
                    i64_const(1); local_set(result); // result
                    i64_const(2); local_set(i); // i
                    block_empty; loop_empty;
                      local_get(i); local_get(n); i64_gt_s; br_if(1);
                      local_get(result); local_get(i); i64_mul; local_set(result);
                      local_get(i); i64_const(1); i64_add; local_set(i);
                      br(0);
                    end; end;
                    local_get(result);
                });
                self.scratch.free_i64(i);
                self.scratch.free_i64(result);
                self.scratch.free_i64(n);
            }
            "choose" => {
                // choose(n, k) = n! / (k! * (n-k)!)
                // Iterative: result = 1; for i in 0..k: result = result * (n-i) / (i+1)
                self.emit_expr(&args[0]); // n
                self.emit_expr(&args[1]); // k
                let k = self.scratch.alloc_i64();
                let n = self.scratch.alloc_i64();
                let result = self.scratch.alloc_i64();
                let i = self.scratch.alloc_i64();
                wasm!(self.func, {
                    local_set(k);       // k
                    local_set(n);   // n
                    i64_const(1); local_set(result); // result
                    i64_const(0); local_set(i); // i
                    block_empty; loop_empty;
                      local_get(i); local_get(k); i64_ge_s; br_if(1);
                      // result = result * (n - i) / (i + 1)
                      local_get(result);
                      local_get(n); local_get(i); i64_sub;
                      i64_mul;
                      local_get(i); i64_const(1); i64_add;
                      i64_div_s;
                      local_set(result);
                      local_get(i); i64_const(1); i64_add; local_set(i);
                      br(0);
                    end; end;
                    local_get(result);
                });
                self.scratch.free_i64(i);
                self.scratch.free_i64(result);
                self.scratch.free_i64(n);
                self.scratch.free_i64(k);
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
