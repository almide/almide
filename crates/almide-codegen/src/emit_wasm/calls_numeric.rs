//! Float, Int, and Math stdlib call dispatch for WASM codegen.

use super::FuncCompiler;
use almide_ir::IrExpr;
use almide_lang::types::Ty;
use wasm_encoder::Instruction;

/// Bit width at and above which a `bits`-wide mask saturates to "all ones".
/// `int.wrap_*` / `int.rotate_*` model an N-bit register; for `bits >= 64` the
/// register is the full i64, so the mask is `u64::MAX`. This mirrors the native
/// guard `if bits >= 64 { u64::MAX } else { (1u64 << bits) - 1 }`
/// (runtime/rs/src/int.rs). Without it, wasm `i64.shl` masks the shift amount
/// `mod 64`, so `1 << 64` wraps to `1` and the mask collapses to `0` — every
/// `bits >= 64` result silently became garbage (Cluster-2 finding #1).
const FULL_WIDTH_BITS: i64 = 64;
/// All-ones i64 = `u64::MAX` reinterpreted: the saturated mask for `bits >= 64`.
const FULL_WIDTH_MASK: i64 = -1;

impl FuncCompiler<'_> {
    /// Push the `bits`-wide low mask onto the wasm stack, matching the native
    /// `if bits >= 64 { u64::MAX } else { (1u64 << bits) - 1 }`. `bits_local`
    /// must already hold the (i64) bit width. The `1 << bits` is still evaluated
    /// for the `bits >= 64` branch but `select` discards it, so the wasm shift's
    /// mod-64 wrap never reaches the result.
    fn emit_wrap_mask(&mut self, bits_local: u32) {
        // wasm `select` pops [val_if_true, val_if_false, cond] and yields
        // val_if_true when cond != 0. cond = (bits >= FULL_WIDTH_BITS).
        wasm!(self.func, {
            // val_if_true = u64::MAX  (saturated mask for bits >= 64)
            i64_const(FULL_WIDTH_MASK);
            // val_if_false = (1 << bits) - 1   (valid only for 0 <= bits < 64)
            i64_const(1); local_get(bits_local); i64_shl; i64_const(1); i64_sub;
            // cond
            local_get(bits_local); i64_const(FULL_WIDTH_BITS); i64_ge_s;
            select;
        });
    }

    /// Emit Rust-`f64::max`/`f64::min` semantics for two already-emitted args.
    /// Differs from the raw `f64.max`/`f64.min` wasm instructions, which follow
    /// IEEE-754-2019 `maximum`/`minimum` and PROPAGATE NaN; Rust (and libm
    /// `fmax`/`fmin`) IGNORE a NaN operand and return the other. Logic, matching
    /// `runtime/rs/src/float.rs` / `math.rs` bit-for-bit (incl. signed-zero
    /// order: `max(0,-0) = 0`, `max(-0,0) = -0`):
    ///   max = if a.is_nan {b} else if b.is_nan {a} else if a <  b {b} else {a}
    ///   min = if a.is_nan {b} else if b.is_nan {a} else if b <  a {b} else {a}
    /// (Cluster-2 finding #4.)
    fn emit_float_min_max(&mut self, args: &[IrExpr], is_max: bool) {
        let a = self.scratch.alloc_f64();
        let b = self.scratch.alloc_f64();
        self.emit_expr(&args[0]);
        wasm!(self.func, { local_set(a); });
        self.emit_expr(&args[1]);
        wasm!(self.func, { local_set(b); });
        wasm!(self.func, {
            // if a != a (a is NaN) { b }
            local_get(a); local_get(a); f64_ne;
            if_f64;
              local_get(b);
            else_;
              // else if b != b (b is NaN) { a }
              local_get(b); local_get(b); f64_ne;
              if_f64;
                local_get(a);
              else_;
                // strict comparison; for max: a < b ? b : a; for min: b < a ? b : a
                local_get(a); local_get(b);
        });
        if is_max {
            wasm!(self.func, { f64_lt; }); // a < b
        } else {
            wasm!(self.func, { f64_gt; }); // a > b  ≡  b < a
        }
        wasm!(self.func, {
                if_f64;
                  local_get(b);
                else_;
                  local_get(a);
                end;
              end;
            end;
        });
        self.scratch.free_f64(b);
        self.scratch.free_f64(a);
    }

    /// Dispatch a float stdlib method call. Returns true if handled.
    pub(super) fn emit_float_call(&mut self, method: &str, args: &[IrExpr]) -> bool {
        use super::stdlib_dispatch::StdlibOp;

        // Declarative table: simple runtime-call patterns.
        let op: Option<StdlibOp> = match method {
            "to_string" => Some(StdlibOp::Call1(self.emitter.rt.float_to_string)),
            _ => None,
        };
        if let Some(op) = op {
            self.emit_stdlib_op(op, args);
            return true;
        }

        match method {
            "to_int" => {
                // truncate f64 → i64 (saturating: NaN→0, ±Inf→i64::MAX/MIN)
                self.emit_expr(&args[0]);
                self.func.instruction(&Instruction::I64TruncSatF64S);
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
                // float.sign == Rust `f64::signum`: copysign(1.0, x) for every
                // non-NaN x (so sign(0.0)=1, sign(-0.0)=-1, sign(±inf)=±1), and
                // NaN for NaN. The old three-way (returns 0 for ±0.0/NaN)
                // diverged from native (Cluster-2 finding #3). NaN is detected by
                // `x != x`; copysign propagates the sign bit of zeros/infs too.
                let tmp = self.scratch.alloc_f64();
                self.emit_expr(&args[0]);
                wasm!(self.func, {
                    local_set(tmp);
                    // if x != x { x (NaN) } else { copysign(1.0, x) }
                    local_get(tmp); local_get(tmp); f64_ne;
                    if_f64;
                      local_get(tmp);
                    else_;
                      f64_const(1.0); local_get(tmp); f64_copysign;
                    end;
                });
                self.scratch.free_f64(tmp);
            }
            "min" => {
                // Rust `f64::min` (NaN-ignoring), NOT the NaN-propagating f64.min.
                self.emit_float_min_max(args, false);
            }
            "max" => {
                // Rust `f64::max` (NaN-ignoring), NOT the NaN-propagating f64.max.
                self.emit_float_min_max(args, true);
            }
            "clamp" => {
                // float.clamp == Rust `f64::clamp`: `if n < lo {lo} else if
                // n > hi {hi} else {n}` (well-defined when lo <= hi and neither
                // is NaN — the same precondition native `f64::clamp` asserts).
                // Emitted directly rather than via NaN-ignoring min/max so it is
                // bit-identical to native for every valid (n, lo, hi).
                let n = self.scratch.alloc_f64();
                let lo = self.scratch.alloc_f64();
                let hi = self.scratch.alloc_f64();
                self.emit_expr(&args[0]);
                wasm!(self.func, { local_set(n); });
                self.emit_expr(&args[1]);
                wasm!(self.func, { local_set(lo); });
                self.emit_expr(&args[2]);
                // ALS-T6: lo > hi OR a NaN bound aborts in the T6 form —
                // `!(lo <= hi)` covers both in one IEEE comparison (native
                // f64::clamp panics raw on either; the un-checked chain here
                // silently returned a value — fuzz seed-20260718 index 5's
                // float twin).
                let clamp_msg = self.emitter.intern_string("Error: clamp requires min <= max\n") as i32;
                wasm!(self.func, {
                    local_set(hi);
                    local_get(lo); local_get(hi); f64_le; i32_eqz;
                    if_empty;
                      i32_const(clamp_msg);
                      call(self.emitter.rt.div_trap);
                    end;
                    local_get(n); local_get(lo); f64_lt;
                    if_f64;
                      local_get(lo);
                    else_;
                      local_get(n); local_get(hi); f64_gt;
                      if_f64;
                        local_get(hi);
                      else_;
                        local_get(n);
                      end;
                    end;
                });
                self.scratch.free_f64(hi);
                self.scratch.free_f64(lo);
                self.scratch.free_f64(n);
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
            "to_bits" => {
                // float.to_bits(f: Float) → Int: reinterpret f64 as i64
                self.emit_expr(&args[0]);
                wasm!(self.func, { i64_reinterpret_f64; });
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
                // ALS-T6: an inverted range (lo > hi) aborts in the T6 form —
                // native i64::clamp leaked the raw Rust panic while this chain
                // silently returned a value (fuzz seed-20260718 index 5).
                let clamp_msg = self.emitter.intern_string("Error: clamp requires min <= max\n") as i32;
                wasm!(self.func, {
                    local_set(hi);       // hi
                    local_set(lo);   // lo
                    local_set(n);   // n
                    local_get(lo); local_get(hi); i64_gt_s;
                    if_empty;
                      i32_const(clamp_msg);
                      call(self.emitter.rt.div_trap);
                    end;
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
            "count_leading_zeros" => {
                self.emit_expr(&args[0]);
                wasm!(self.func, { i64_clz; });
            }
            "count_trailing_zeros" => {
                self.emit_expr(&args[0]);
                wasm!(self.func, { i64_ctz; });
            }
            "pop_count" => {
                self.emit_expr(&args[0]);
                wasm!(self.func, { i64_popcnt; });
            }
            "bit_reverse" => {
                // No WASM instruction; use loop: reverse 64 bits
                let src = self.scratch.alloc_i64();
                let dst = self.scratch.alloc_i64();
                let i = self.scratch.alloc_i64();
                self.emit_expr(&args[0]);
                wasm!(self.func, {
                    local_set(src);
                    i64_const(0); local_set(dst);
                    i64_const(0); local_set(i);
                    block_empty; loop_empty;
                        local_get(i); i64_const(64); i64_ge_s; br_if(1);
                        // dst = (dst << 1) | (src & 1)
                        local_get(dst); i64_const(1); i64_shl;
                        local_get(src); i64_const(1); i64_and;
                        i64_or; local_set(dst);
                        // src >>= 1
                        local_get(src); i64_const(1); i64_shr_u; local_set(src);
                        local_get(i); i64_const(1); i64_add; local_set(i);
                        br(0);
                    end; end;
                    local_get(dst);
                });
                self.scratch.free_i64(i);
                self.scratch.free_i64(dst);
                self.scratch.free_i64(src);
            }
            "byte_swap" => {
                // Swap bytes of i64: reverse 8 bytes
                let src = self.scratch.alloc_i64();
                let dst = self.scratch.alloc_i64();
                let i = self.scratch.alloc_i64();
                self.emit_expr(&args[0]);
                wasm!(self.func, {
                    local_set(src);
                    i64_const(0); local_set(dst);
                    i64_const(0); local_set(i);
                    block_empty; loop_empty;
                        local_get(i); i64_const(8); i64_ge_s; br_if(1);
                        // dst = (dst << 8) | (src & 0xFF)
                        local_get(dst); i64_const(8); i64_shl;
                        local_get(src); i64_const(0xFF); i64_and;
                        i64_or; local_set(dst);
                        // src >>= 8
                        local_get(src); i64_const(8); i64_shr_u; local_set(src);
                        local_get(i); i64_const(1); i64_add; local_set(i);
                        br(0);
                    end; end;
                    local_get(dst);
                });
                self.scratch.free_i64(i);
                self.scratch.free_i64(dst);
                self.scratch.free_i64(src);
            }
            "bit_width" => {
                // bit_width(n) = if n == 0 then 0 else 64 - clz(n)
                self.emit_expr(&args[0]);
                let n = self.scratch.alloc_i64();
                wasm!(self.func, {
                    local_set(n);
                    local_get(n); i64_eqz;
                    if_i64;
                        i64_const(0);
                    else_;
                        i64_const(64); local_get(n); i64_clz; i64_sub;
                    end;
                });
                self.scratch.free_i64(n);
            }
            "log2_floor" => {
                // log2_floor(n) = if n <= 0 then -1 else 63 - clz(n)
                self.emit_expr(&args[0]);
                let n = self.scratch.alloc_i64();
                wasm!(self.func, {
                    local_set(n);
                    local_get(n); i64_const(0); i64_le_s;
                    if_i64;
                        i64_const(-1);
                    else_;
                        i64_const(63); local_get(n); i64_clz; i64_sub;
                    end;
                });
                self.scratch.free_i64(n);
            }
            "log2_ceil" => {
                // log2_ceil(n) = if n <= 0 then 0 else if n == 1 then 0 else 64 - clz(n-1)
                self.emit_expr(&args[0]);
                let n = self.scratch.alloc_i64();
                wasm!(self.func, {
                    local_set(n);
                    local_get(n); i64_const(1); i64_le_s;
                    if_i64;
                        i64_const(0);
                    else_;
                        i64_const(64);
                        local_get(n); i64_const(1); i64_sub; i64_clz;
                        i64_sub;
                    end;
                });
                self.scratch.free_i64(n);
            }
            "next_power_of_two" => {
                // next_power_of_two(n) = if n <= 1 then 1 else 1 << (64 - clz(n-1))
                self.emit_expr(&args[0]);
                let n = self.scratch.alloc_i64();
                wasm!(self.func, {
                    local_set(n);
                    local_get(n); i64_const(1); i64_le_s;
                    if_i64;
                        i64_const(1);
                    else_;
                        i64_const(1);
                        i64_const(64);
                        local_get(n); i64_const(1); i64_sub; i64_clz;
                        i64_sub;
                        i64_shl;
                    end;
                });
                self.scratch.free_i64(n);
            }
            "prev_power_of_two" => {
                // prev_power_of_two(n) = if n <= 0 then 0 else 1 << (63 - clz(n))
                self.emit_expr(&args[0]);
                let n = self.scratch.alloc_i64();
                wasm!(self.func, {
                    local_set(n);
                    local_get(n); i64_const(0); i64_le_s;
                    if_i64;
                        i64_const(0);
                    else_;
                        i64_const(1);
                        i64_const(63); local_get(n); i64_clz; i64_sub;
                        i64_shl;
                    end;
                });
                self.scratch.free_i64(n);
            }
            "wrap_add" => {
                // wrap_add(a, b, bits): (a + b) & wrap_mask(bits)
                let bits = self.scratch.alloc_i64();
                self.emit_expr(&args[0]);
                self.emit_expr(&args[1]);
                wasm!(self.func, { i64_add; });
                self.emit_expr(&args[2]);
                wasm!(self.func, { local_set(bits); });
                self.emit_wrap_mask(bits);
                wasm!(self.func, { i64_and; });
                self.scratch.free_i64(bits);
            }
            "wrap_mul" => {
                // wrap_mul(a, b, bits): (a * b) & wrap_mask(bits)
                let bits = self.scratch.alloc_i64();
                self.emit_expr(&args[0]);
                self.emit_expr(&args[1]);
                wasm!(self.func, { i64_mul; });
                self.emit_expr(&args[2]);
                wasm!(self.func, { local_set(bits); });
                self.emit_wrap_mask(bits);
                wasm!(self.func, { i64_and; });
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
                      i32_const(1); call(self.emitter.rt.string_alloc); local_set(buf);
                      local_get(buf); i32_const(1); i32_store(0);
                      local_get(buf); i32_const(1); i32_store(self.emitter.layout_reg.fixed_offset(super::engine::layout::STRING, super::engine::layout::string::CAP) as i32 as u32, 0);
                      local_get(buf); i32_const(48); i32_store8(self.emitter.layout_reg.fixed_offset(super::engine::layout::STRING, super::engine::layout::string::DATA) as i32 as u32);
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
                      // Alloc result string: [len][cap][data...]
                      i32_const(self.emitter.layout_reg.header_size(super::engine::layout::STRING) as i32);
                      local_get(count); i32_add;
                      call(self.emitter.rt.alloc); local_set(digit);
                      local_get(digit); local_get(count); i32_store(0); // len
                      local_get(digit); local_get(count);
                      i32_store(self.emitter.layout_reg.fixed_offset(super::engine::layout::STRING, super::engine::layout::string::CAP) as u32); // cap
                      // Copy reversed
                      i32_const(0); local_set(idx);
                      block_empty; loop_empty;
                        local_get(idx); local_get(count); i32_ge_u; br_if(1);
                        local_get(digit);
                        i32_const(self.emitter.layout_reg.fixed_offset(super::engine::layout::STRING, super::engine::layout::string::DATA) as i32);
                        i32_add; local_get(idx); i32_add;
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
            "bits_to_float" => {
                // int.bits_to_float(bits: Int) → Float: reinterpret i64 as f64
                self.emit_expr(&args[0]);
                wasm!(self.func, { f64_reinterpret_i64; });
            }
            "bits_to_f32" => {
                // int.bits_to_f32(bits: Int) → Float:
                //   take low 32 bits of i64, reinterpret as f32, promote to f64.
                self.emit_expr(&args[0]);
                wasm!(self.func, {
                    i32_wrap_i64;
                    f32_reinterpret_i32;
                    f64_promote_f32;
                });
            }
            "rotate_right" | "rotate_left" => {
                // rotate_{left,right}(a, n, bits)
                // mask = wrap_mask(bits); v = a & mask
                // rotate_left:  ((v << n) | (v >> (bits - n))) & mask
                // rotate_right: ((v >> n) | (v << (bits - n))) & mask
                // TOTAL like int `/`/`%` (C-001): a width `bits <= 0` has no
                // register to rotate (and native's `n % bits` would divide by
                // zero), so abort with `Error: rotate width must be positive` +
                // exit 1 — byte-identical to the native runtime guard — instead
                // of a divergent native panic (101) / wasm trap (134).
                let is_left = method == "rotate_left";
                let rot_width_msg = self
                    .emitter
                    .intern_string("Error: rotate width must be positive\n")
                    as i32;
                let div_trap = self.emitter.rt.div_trap;
                let a = self.scratch.alloc_i64();
                let n = self.scratch.alloc_i64();
                let bits = self.scratch.alloc_i64();
                let mask = self.scratch.alloc_i64();
                let v = self.scratch.alloc_i64();
                self.emit_expr(&args[0]);
                wasm!(self.func, { local_set(a); });
                self.emit_expr(&args[1]);
                // n = n % bits (needs bits first)
                wasm!(self.func, { local_set(n); });
                self.emit_expr(&args[2]);
                wasm!(self.func, {
                    local_set(bits);
                    // if bits <= 0 { abort("rotate width must be positive") }
                    local_get(bits); i64_const(0); i64_le_s;
                    if_empty;
                      i32_const(rot_width_msg); call(div_trap);
                    end;
                });
                // mask = wrap_mask(bits)  (u64::MAX for bits >= 64)
                self.emit_wrap_mask(bits);
                wasm!(self.func, {
                    local_set(mask);
                    // v = a & mask
                    local_get(a); local_get(mask); i64_and; local_set(v);
                    // n = n % bits
                    local_get(n); local_get(bits); i64_rem_s; local_set(n);
                });
                if is_left {
                    wasm!(self.func, {
                        // (v << n) | (v >> (bits - n))
                        local_get(v); local_get(n); i64_shl;
                        local_get(v); local_get(bits); local_get(n); i64_sub; i64_shr_u;
                        i64_or;
                        local_get(mask); i64_and;
                    });
                } else {
                    wasm!(self.func, {
                        // (v >> n) | (v << (bits - n))
                        local_get(v); local_get(n); i64_shr_u;
                        local_get(v); local_get(bits); local_get(n); i64_sub; i64_shl;
                        i64_or;
                        local_get(mask); i64_and;
                    });
                }
                self.scratch.free_i64(v);
                self.scratch.free_i64(mask);
                self.scratch.free_i64(bits);
                self.scratch.free_i64(n);
                self.scratch.free_i64(a);
            }
            _ => return false,
        }
        true
    }

    /// Dispatch a math stdlib method call. Returns true if handled.
    ///
    /// Simple patterns (unary runtime calls, constants, builtin WASM instrs)
    /// live in a declarative table via [`StdlibOp`]. Custom patterns (loops,
    /// conditionals) are inlined below.
    pub(super) fn emit_math_call(&mut self, method: &str, args: &[IrExpr]) -> bool {
        use super::stdlib_dispatch::StdlibOp;

        // ── Declarative table: simple runtime-call patterns ──
        let op: Option<StdlibOp> = match method {
            "sin"   => Some(StdlibOp::FloatUnaryCall(self.emitter.rt.math_sin)),
            "cos"   => Some(StdlibOp::FloatUnaryCall(self.emitter.rt.math_cos)),
            "tan"   => Some(StdlibOp::FloatUnaryCall(self.emitter.rt.math_tan)),
            "log"   => Some(StdlibOp::FloatUnaryCall(self.emitter.rt.math_log)),
            "exp"   => Some(StdlibOp::FloatUnaryCall(self.emitter.rt.math_exp)),
            "log10" => Some(StdlibOp::FloatUnaryCall(self.emitter.rt.math_log10)),
            "log2"  => Some(StdlibOp::FloatUnaryCall(self.emitter.rt.math_log2)),
            "atan"  => Some(StdlibOp::FloatUnaryCall(self.emitter.rt.math_atan)),
            "tanh"  => Some(StdlibOp::FloatUnaryCall(self.emitter.rt.math_tanh)),
            _ => None,
        };
        if let Some(op) = op {
            self.emit_stdlib_op(op, args);
            return true;
        }

        match method {
            "pi" => {
                wasm!(self.func, { f64_const(std::f64::consts::PI); });
            }
            "e" => {
                wasm!(self.func, { f64_const(std::f64::consts::E); });
            }
            "sqrt" => {
                // f64_convert (if Int) → f64_sqrt (builtin)
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
                // math.fmin == native `f64::min` (NaN-ignoring), NOT f64.min.
                self.emit_float_min_max(args, false);
            }
            "fmax" => {
                // math.fmax == native `f64::max` (NaN-ignoring), NOT f64.max.
                self.emit_float_min_max(args, true);
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
                // pow(base: Int, exp: Int) → Int. TOTAL like int `/`/`%` (C-001):
                // a NEGATIVE exponent has no integer result, so abort with
                // `Error: negative exponent` + exit 1 (byte-identical to the
                // native `almide_rt_math_pow`). Non-negative: exponentiation by
                // squaring with WRAPPING i64 multiply — O(log exp), matching
                // native bit-for-bit and never hanging on a huge exponent (the
                // old `for i in 0..exp` loop ran `exp` iterations).
                let neg_exp_msg =
                    self.emitter.intern_string("Error: negative exponent\n") as i32;
                let div_trap = self.emitter.rt.div_trap;
                self.emit_expr(&args[0]); // base
                self.emit_expr(&args[1]); // exp
                let exp = self.scratch.alloc_i64();
                let base = self.scratch.alloc_i64();
                let result = self.scratch.alloc_i64();
                wasm!(self.func, {
                    local_set(exp);
                    local_set(base);
                    // if exp < 0 { abort("negative exponent") }
                    local_get(exp); i64_const(0); i64_lt_s;
                    if_empty;
                      i32_const(neg_exp_msg); call(div_trap);
                    end;
                    i64_const(1); local_set(result);   // result = 1
                    // exponentiation by squaring; exp treated as an unsigned count
                    block_empty; loop_empty;
                      local_get(exp); i64_eqz; br_if(1);          // while exp != 0
                      // if exp & 1 == 1 { result *= base }
                      local_get(exp); i64_const(1); i64_and; i64_const(1); i64_eq;
                      if_empty;
                        local_get(result); local_get(base); i64_mul; local_set(result);
                      end;
                      // exp >>= 1  (logical: exp is a non-negative count here)
                      local_get(exp); i64_const(1); i64_shr_u; local_set(exp);
                      // if exp != 0 { base *= base }   (avoid a final useless square)
                      local_get(exp); i64_eqz;
                      if_empty;
                      else_;
                        local_get(base); local_get(base); i64_mul; local_set(base);
                      end;
                      br(0);
                    end; end;
                    local_get(result);
                });
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
                // Lanczos approximation (g=7, n=9 coefficients)
                // log_gamma(x) = 0.5*ln(2π) + (x+0.5)*ln(t) - t + ln(Ag(x))
                // where t = x+g+0.5, Ag(x) = c0 + c1/(x+1) + ... + c8/(x+8).
                // Bit-identical to native `almide_rt_math_log_gamma`: SAME Lanczos
                // coeffs, SAME `t = x + LANCZOS_G_OFFSET`, SAME `HALF_LN_2PI`
                // constant, and BOTH `ln` calls route through the vendored
                // musl-libm `__libm_log` (`math_log`), never the platform `f64::ln`
                // — so the old ~1-ULP drift (Cluster-2 finding #7) is gone.
                /// t = x + g + 0.5 with Lanczos g = 7 (mirrors native LANCZOS_G_OFFSET).
                const LANCZOS_G_OFFSET: f64 = 7.5;
                /// 0.5·ln(2π), pinned bit-pattern (mirrors native HALF_LN_2PI).
                const HALF_LN_2PI: f64 = 0.9189385332046727;
                let x = self.scratch.alloc_f64();
                let t = self.scratch.alloc_f64();
                let ag = self.scratch.alloc_f64();
                self.emit_expr(&args[0]);
                // Lanczos computes Γ(x+1), shift by -1 to get Γ(x)
                wasm!(self.func, { f64_const(1.0); f64_sub; local_set(x); });
                let coeffs: [f64; 9] = [
                    0.99999999999980993, 676.5203681218851, -1259.1392167224028,
                    771.32342877765313, -176.61502916214059, 12.507343278686905,
                    -0.13857109526572012, 9.9843695780195716e-6, 1.5056327351493116e-7,
                ];
                wasm!(self.func, { f64_const(coeffs[0]); local_set(ag); });
                for (i, &c) in coeffs[1..].iter().enumerate() {
                    wasm!(self.func, {
                        local_get(ag);
                        f64_const(c);
                        local_get(x); f64_const((i + 1) as f64); f64_add;
                        f64_div;
                        f64_add;
                        local_set(ag);
                    });
                }
                wasm!(self.func, {
                    local_get(x); f64_const(LANCZOS_G_OFFSET); f64_add; local_set(t);
                });
                // Reuse ag as final result: ag = ln(ag)
                wasm!(self.func, {
                    local_get(ag); call(self.emitter.rt.math_log); local_set(ag);
                });
                // result = 0.5*ln(2π) + (x+0.5)*ln(t) - t + ag
                // Reuse t slot: compute ln(t) and store back
                wasm!(self.func, {
                    f64_const(HALF_LN_2PI);
                    local_get(x); f64_const(0.5); f64_add;
                    local_get(t); call(self.emitter.rt.math_log);
                    f64_mul;
                    f64_add;
                    local_get(t); f64_sub;
                    local_get(ag); f64_add;
                });
                self.scratch.free_f64(ag);
                self.scratch.free_f64(t);
                self.scratch.free_f64(x);
            }
            _ => return false,
        }
        true
    }
}
