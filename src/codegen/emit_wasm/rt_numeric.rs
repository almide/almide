//! WASM runtime: int.from_hex, float.parse, float.to_fixed, math.fpow.
//!
//! Called from `compile_runtime()` in runtime.rs.

use super::{CompiledFunc, WasmEmitter};
use wasm_encoder::{Function, Instruction, ValType};

/// __int_from_hex(s: i32) -> i32
/// Parses a hex string (e.g. "ff", "FF", "0xff") to i64, returns Result[Int, String].
/// Layout: [tag:i32][value:i64] = 12 bytes. tag=0 ok, tag=1 err (str ptr at offset 4).
pub(super) fn compile_int_from_hex(emitter: &mut WasmEmitter) {
    let type_idx = emitter.func_type_indices[&emitter.rt.int_from_hex];
    // params: 0=$s (string ptr: [len:i32][data:u8...])
    // locals:
    //   1=i32 len, 2=i32 i, 3=i64 result, 4=i32 byte, 5=i32 alloc_ptr, 6=i32 digit
    let mut f = Function::new([
        (1, ValType::I32),  // 1: len
        (1, ValType::I32),  // 2: i
        (1, ValType::I64),  // 3: result
        (1, ValType::I32),  // 4: byte
        (1, ValType::I32),  // 5: alloc_ptr
        (1, ValType::I32),  // 6: digit
    ]);

    let err_str = emitter.intern_string("invalid hex");

    // len = s.len
    wasm!(f, {
        local_get(0); i32_load(0); local_set(1);
    });

    // Empty string -> err
    wasm!(f, {
        local_get(1); i32_eqz;
        if_empty;
    });
    emit_int_from_hex_err(&mut f, emitter, err_str);
    wasm!(f, { end; });

    // i = 0, result = 0
    wasm!(f, {
        i32_const(0); local_set(2);
        i64_const(0); local_set(3);
    });

    // Skip optional "0x" or "0X" prefix
    wasm!(f, {
        local_get(1); i32_const(2); i32_ge_u;
        if_empty;
          local_get(0); i32_load8_u(4); // first byte
          i32_const(48); // '0'
          i32_eq;
          if_empty;
            local_get(0); i32_const(4); i32_add; i32_const(1); i32_add; i32_load8_u(0); // second byte
            local_set(4);
            local_get(4); i32_const(120); i32_eq; // 'x'
            local_get(4); i32_const(88); i32_eq; // 'X'
            i32_or;
            if_empty;
              i32_const(2); local_set(2); // skip "0x"
            end;
          end;
        end;
    });

    // After skipping prefix, check we still have digits
    wasm!(f, {
        local_get(2); local_get(1); i32_ge_u;
        if_empty;
    });
    emit_int_from_hex_err(&mut f, emitter, err_str);
    wasm!(f, { end; });

    // Main parse loop: while i < len
    wasm!(f, {
        block_empty; loop_empty;
        local_get(2); local_get(1); i32_ge_u; br_if(1);
    });

    // byte = s[4+i]
    wasm!(f, {
        local_get(0); i32_const(4); i32_add;
        local_get(2); i32_add;
        i32_load8_u(0); local_set(4);
    });

    // Skip underscores (common in hex literals)
    wasm!(f, {
        local_get(4); i32_const(95); i32_eq; // '_'
        if_empty;
          local_get(2); i32_const(1); i32_add; local_set(2);
          br(1); // continue loop
        end;
    });

    // Classify: '0'-'9' -> 0-9, 'a'-'f' -> 10-15, 'A'-'F' -> 10-15, else err
    // digit = -1 (sentinel for invalid)
    wasm!(f, {
        i32_const(-1); local_set(6);
        // Check '0' <= byte <= '9'
        local_get(4); i32_const(48); i32_ge_u;
        local_get(4); i32_const(57); i32_le_u;
        i32_and;
        if_empty;
          local_get(4); i32_const(48); i32_sub; local_set(6);
        else_;
          // Check 'a' <= byte <= 'f'
          local_get(4); i32_const(97); i32_ge_u;
          local_get(4); i32_const(102); i32_le_u;
          i32_and;
          if_empty;
            local_get(4); i32_const(87); i32_sub; local_set(6); // 'a'-87 = 10
          else_;
            // Check 'A' <= byte <= 'F'
            local_get(4); i32_const(65); i32_ge_u;
            local_get(4); i32_const(70); i32_le_u;
            i32_and;
            if_empty;
              local_get(4); i32_const(55); i32_sub; local_set(6); // 'A'-55 = 10
            end;
          end;
        end;
    });

    // If digit == -1 -> err
    wasm!(f, {
        local_get(6); i32_const(-1); i32_eq;
        if_empty;
    });
    emit_int_from_hex_err(&mut f, emitter, err_str);
    wasm!(f, { end; });

    // result = result * 16 + digit
    wasm!(f, {
        local_get(3); i64_const(16); i64_mul;
        local_get(6); i64_extend_i32_u;
        i64_add; local_set(3);
    });

    // i++
    wasm!(f, {
        local_get(2); i32_const(1); i32_add; local_set(2);
        br(0);
        end; end; // end loop, end block
    });

    // Return ok(result): alloc [tag=0, value=result]
    wasm!(f, {
        i32_const(12); call(emitter.rt.alloc); local_set(5);
        local_get(5); i32_const(0); i32_store(0);   // tag = 0 (ok)
        local_get(5); local_get(3); i64_store(4);    // value
        local_get(5);
        end;
    });

    emitter.add_compiled(CompiledFunc { type_idx, func: f });
}

/// Emit the err return for int_from_hex: alloc [tag=1][str_ptr] and return.
fn emit_int_from_hex_err(f: &mut Function, emitter: &WasmEmitter, err_str: u32) {
    wasm!(f, {
        i32_const(12); call(emitter.rt.alloc); local_set(5);
        local_get(5); i32_const(1); i32_store(0);              // tag = 1 (err)
        local_get(5); i32_const(err_str as i32); i32_store(4); // err string
        local_get(5);
        return_;
    });
}

/// __float_parse(s: i32) -> i32
/// Parses a string to f64, returns Result[Float, String].
/// Layout: [tag:i32][f64 | err_str_ptr:i32] = 12 bytes.
/// tag=0: ok, f64 at offset 4.  tag=1: err, str ptr at offset 4.
///
/// Handles: optional leading sign, integer part, optional decimal part.
/// Examples: "3.14", "-0.5", "42", "+1.0"
pub(super) fn compile_float_parse(emitter: &mut WasmEmitter) {
    let type_idx = emitter.func_type_indices[&emitter.rt.float_parse];
    // params: 0=$s (string ptr: [len:i32][data:u8...])
    // locals:
    //   1=i32 len, 2=i32 i, 3=f64 result, 4=i32 is_neg,
    //   5=i32 byte, 6=i32 alloc_ptr, 7=f64 frac_mult,
    //   8=i32 has_dot, 9=i32 digit_count
    let mut f = Function::new([
        (1, ValType::I32),  // 1: len
        (1, ValType::I32),  // 2: i
        (1, ValType::F64),  // 3: result
        (1, ValType::I32),  // 4: is_neg
        (1, ValType::I32),  // 5: byte
        (1, ValType::I32),  // 6: alloc_ptr
        (1, ValType::F64),  // 7: frac_mult
        (1, ValType::I32),  // 8: has_dot
        (1, ValType::I32),  // 9: digit_count
    ]);

    let err_str = emitter.intern_string("invalid number");

    // len = s.len
    wasm!(f, {
        local_get(0); i32_load(0); local_set(1);
    });

    // Empty string -> err
    wasm!(f, {
        local_get(1); i32_eqz;
        if_empty;
    });
    emit_float_parse_err(&mut f, emitter, err_str);
    wasm!(f, { end; });

    // Initialize: i=0, result=0.0, is_neg=0, frac_mult=1.0, has_dot=0, digit_count=0
    wasm!(f, {
        i32_const(0); local_set(2);
        f64_const(0.0); local_set(3);
        i32_const(0); local_set(4);
        f64_const(1.0); local_set(7);
        i32_const(0); local_set(8);
        i32_const(0); local_set(9);
    });

    // Check leading '-'
    wasm!(f, {
        local_get(0); i32_load8_u(4);
        i32_const(45); // '-'
        i32_eq;
        if_empty;
          i32_const(1); local_set(4);
          i32_const(1); local_set(2);
        end;
    });

    // Check leading '+' (only if not already negative)
    wasm!(f, {
        local_get(0); i32_load8_u(4);
        i32_const(43); // '+'
        i32_eq;
        local_get(4); i32_eqz;
        i32_and;
        if_empty;
          i32_const(1); local_set(2);
        end;
    });

    // Main parse loop: while i < len
    wasm!(f, {
        block_empty; loop_empty;
        local_get(2); local_get(1); i32_ge_u; br_if(1);
    });

    // byte = s[4+i]
    wasm!(f, {
        local_get(0); i32_const(4); i32_add;
        local_get(2); i32_add;
        i32_load8_u(0); local_set(5);
    });

    // Check for '.'
    wasm!(f, {
        local_get(5); i32_const(46); i32_eq; // '.'
        if_empty;
          // If we already saw a dot -> err
          local_get(8);
          if_empty;
    });
    emit_float_parse_err(&mut f, emitter, err_str);
    wasm!(f, {
          end;
          i32_const(1); local_set(8);
          // advance i and continue
          local_get(2); i32_const(1); i32_add; local_set(2);
          br(1); // continue loop
        end;
    });

    // Check digit: '0' <= byte <= '9'
    wasm!(f, {
        local_get(5); i32_const(48); i32_lt_u;
        local_get(5); i32_const(57); i32_gt_u;
        i32_or;
        if_empty;
    });
    // Not a digit -> err
    emit_float_parse_err(&mut f, emitter, err_str);
    wasm!(f, { end; });

    // It's a digit. digit_count++
    wasm!(f, {
        local_get(9); i32_const(1); i32_add; local_set(9);
    });

    // If we're past the dot: frac_mult /= 10, result += digit * frac_mult
    // Else: result = result * 10 + digit
    wasm!(f, {
        local_get(8);
        if_empty;
          // Fractional part
          local_get(7); f64_const(10.0); f64_div; local_set(7);
          local_get(3);
          local_get(5); i32_const(48); i32_sub; i64_extend_i32_u; f64_convert_i64_s;
          local_get(7); f64_mul;
          f64_add; local_set(3);
        else_;
          // Integer part
          local_get(3); f64_const(10.0); f64_mul;
          local_get(5); i32_const(48); i32_sub; i64_extend_i32_u; f64_convert_i64_s;
          f64_add; local_set(3);
        end;
    });

    // i++, continue
    wasm!(f, {
        local_get(2); i32_const(1); i32_add; local_set(2);
        br(0);
        end; end; // end loop, end block
    });

    // Must have at least 1 digit
    wasm!(f, {
        local_get(9); i32_eqz;
        if_empty;
    });
    emit_float_parse_err(&mut f, emitter, err_str);
    wasm!(f, { end; });

    // If is_neg: result = -result
    wasm!(f, {
        local_get(4);
        if_empty;
          local_get(3); f64_neg; local_set(3);
        end;
    });

    // Return ok(result): alloc 12 bytes [tag=0][f64]
    wasm!(f, {
        i32_const(12); call(emitter.rt.alloc); local_set(6);
        local_get(6); i32_const(0); i32_store(0);     // tag = 0 (ok)
        local_get(6); local_get(3); f64_store(4);      // value
        local_get(6);
        end;
    });

    emitter.add_compiled(CompiledFunc { type_idx, func: f });
}

/// Emit the err return for float_parse: alloc [tag=1][str_ptr] and return.
fn emit_float_parse_err(f: &mut Function, emitter: &WasmEmitter, err_str: u32) {
    wasm!(f, {
        i32_const(12); call(emitter.rt.alloc); local_set(6);
        local_get(6); i32_const(1); i32_store(0);          // tag = 1 (err)
        local_get(6); i32_const(err_str as i32); i32_store(4); // err string
        local_get(6);
        return_;
    });
}

/// __float_to_fixed(f: f64, decimals: i64) -> i32
/// Format a float with exactly N decimal places. Returns String ptr.
///
/// Algorithm: multiply by 10^decimals, round, then format integer part + "." + padded decimal part.
pub(super) fn compile_float_to_fixed(emitter: &mut WasmEmitter) {
    let type_idx = emitter.func_type_indices[&emitter.rt.float_to_fixed];
    // params: 0=f64 f, 1=i64 decimals
    // locals:
    //   2=i32 dec_i32,  3=f64 scale,  4=i32 loop_i,
    //   5=f64 scaled,   6=i64 int_val, 7=i32 int_str,
    //   8=i32 buf,      9=i32 count,  10=i32 digit,
    //   11=i32 result,  12=i32 copy_i, 13=i32 is_neg
    let mut f = Function::new([
        (1, ValType::I32),  // 2: dec_i32
        (1, ValType::F64),  // 3: scale
        (1, ValType::I32),  // 4: loop_i
        (1, ValType::F64),  // 5: scaled
        (1, ValType::I64),  // 6: int_val (absolute)
        (1, ValType::I32),  // 7: int_str
        (1, ValType::I32),  // 8: buf (scratch for decimal digits)
        (1, ValType::I32),  // 9: count
        (1, ValType::I32),  // 10: digit
        (1, ValType::I32),  // 11: result
        (1, ValType::I32),  // 12: copy_i
        (1, ValType::I32),  // 13: is_neg
    ]);

    // dec_i32 = decimals as i32 (clamped to [0, 20])
    wasm!(f, {
        local_get(1); i32_wrap_i64; local_set(2);
    });
    // if dec < 0: dec = 0
    f.instruction(&Instruction::LocalGet(2));
    f.instruction(&Instruction::I32Const(0));
    f.instruction(&Instruction::I32LtS);
    wasm!(f, { if_empty; i32_const(0); local_set(2); end; });
    // if dec > 20: dec = 20
    wasm!(f, {
        local_get(2); i32_const(20); i32_gt_u;
        if_empty;
          i32_const(20); local_set(2);
        end;
    });

    // If decimals == 0: return int_to_string(round_half_away_from_zero(f))
    // f64.nearest uses banker's rounding (half-to-even) which gives -2 for -2.5.
    // Standard toFixed(0) should give -3 for -2.5 (round half away from zero).
    // Formula: copysign(floor(abs(f) + 0.5), f)
    wasm!(f, {
        local_get(2); i32_eqz;
        if_empty;
          local_get(0); f64_abs; f64_const(0.5); f64_add; f64_floor;
          local_get(0); f64_copysign;
          i64_trunc_f64_s;
          call(emitter.rt.int_to_string);
          return_;
        end;
    });

    // is_neg = f < 0.0
    wasm!(f, {
        local_get(0); f64_const(0.0); f64_lt; local_set(13);
    });

    // Compute scale = 10^decimals via loop
    wasm!(f, {
        f64_const(1.0); local_set(3);
        i32_const(0); local_set(4);
        block_empty; loop_empty;
          local_get(4); local_get(2); i32_ge_u; br_if(1);
          local_get(3); f64_const(10.0); f64_mul; local_set(3);
          local_get(4); i32_const(1); i32_add; local_set(4);
          br(0);
        end; end;
    });

    // scaled = round_half_away_from_zero(abs(f) * scale)
    // Use floor(x + 0.5) instead of f64.nearest (banker's rounding)
    wasm!(f, {
        local_get(0); f64_abs;
        local_get(3); f64_mul;
        f64_const(0.5); f64_add; f64_floor;
        local_set(5);
    });

    // int_val = scaled as i64
    wasm!(f, {
        local_get(5); i64_trunc_f64_s; local_set(6);
    });

    // Extract decimal digits: we need dec_i32 digits from int_val % scale
    // Alloc scratch buf for digits (max 20)
    wasm!(f, {
        i32_const(20); call(emitter.rt.alloc); local_set(8);
        local_get(2); local_set(9); // count = dec_i32 (we fill all positions)
    });

    // Fill digits right-to-left: buf[count-1-i] = (int_val % 10) + '0'
    // Loop count times
    wasm!(f, {
        i32_const(0); local_set(4); // i = 0
        block_empty; loop_empty;
          local_get(4); local_get(9); i32_ge_u; br_if(1);
          // digit = (int_val % 10)
          local_get(6); i64_const(10); i64_rem_s; i32_wrap_i64;
          // Ensure non-negative (in case of rounding artifacts)
          local_set(10);
          local_get(10); i32_const(0); i32_lt_s;
          if_empty;
            i32_const(0); local_get(10); i32_sub; local_set(10);
          end;
          // buf[count-1-i] = digit + '0'
          local_get(8);
          local_get(9); i32_const(1); i32_sub; local_get(4); i32_sub;
          i32_add;
          local_get(10); i32_const(48); i32_add;
          i32_store8(0);
          // int_val /= 10
          local_get(6); i64_const(10); i64_div_s; local_set(6);
          local_get(4); i32_const(1); i32_add; local_set(4);
          br(0);
        end; end;
    });

    // Now int_val holds the integer part. Build integer string.
    // If is_neg and original int_val != 0, negate it.
    // Actually, we want the integer part of abs(f), which is now in int_val (after extracting decimals).
    // If is_neg, we need to prepend '-'.
    // The int_to_string handles negative, so let's just pass -(int_val) if is_neg.
    wasm!(f, {
        local_get(13);
        if_empty;
          i64_const(0); local_get(6); i64_sub; local_set(6);
        end;
        local_get(6);
        call(emitter.rt.int_to_string);
        local_set(7);
    });

    // Build result: int_str + "." + decimal_buf
    // First build decimal string from buf[0..count]
    let dot = emitter.intern_string(".");
    wasm!(f, {
        // Alloc decimal string: 4 + count bytes
        i32_const(4); local_get(9); i32_add;
        call(emitter.rt.alloc); local_set(11);
        local_get(11); local_get(9); i32_store(0); // len
        // Copy buf[0..count] to result+4
        i32_const(0); local_set(12);
        block_empty; loop_empty;
          local_get(12); local_get(9); i32_ge_u; br_if(1);
          local_get(11); i32_const(4); i32_add; local_get(12); i32_add;
          local_get(8); local_get(12); i32_add; i32_load8_u(0);
          i32_store8(0);
          local_get(12); i32_const(1); i32_add; local_set(12);
          br(0);
        end; end;
    });

    // Concat: int_str + "." + dec_str
    wasm!(f, {
        local_get(7);
        i32_const(dot as i32);
        call(emitter.rt.concat_str);
        local_get(11);
        call(emitter.rt.concat_str);
        end;
    });

    emitter.add_compiled(CompiledFunc { type_idx, func: f });
}

/// __float_pow(base: f64, exp: f64) -> f64
/// Float exponentiation. Handles:
/// - exp == 0 -> 1.0
/// - exp is non-negative integer -> binary exponentiation (exact)
/// - exp is negative integer -> 1.0 / pow(base, -exp)
/// - Fractional exp -> exp(exp * ln(base)) via sqrt reduction + Taylor series
pub(super) fn compile_float_pow(emitter: &mut WasmEmitter) {
    let type_idx = emitter.func_type_indices[&emitter.rt.float_pow];
    // params: 0=f64 base, 1=f64 exp
    // locals (all f64 or i64/i32 as needed — carefully typed):
    //   2=f64 result/y (shared),  3=i64 n (integer path only),
    //   4=f64 b/z,  5=f64 frac_check/zk/sum,
    //   6=i32 is_neg_exp,
    //   7=f64 ln_sum (fractional path), 8=f64 term (fractional path)
    let mut f = Function::new([
        (1, ValType::F64),  // 2: result (int path) / y (frac path)
        (1, ValType::I64),  // 3: n (integer exponent)
        (1, ValType::F64),  // 4: b (int path) / z (frac path)
        (1, ValType::F64),  // 5: frac_check / z^k
        (1, ValType::I32),  // 6: is_neg_exp
        (1, ValType::F64),  // 7: ln_sum (frac path) / exp_arg
        (1, ValType::F64),  // 8: term (frac path) / exp_sum
    ]);

    // Special case: exp == 0.0 -> 1.0
    wasm!(f, {
        local_get(1); f64_const(0.0); f64_eq;
        if_empty;
          f64_const(1.0); return_;
        end;
    });

    // Special case: base == 1.0 -> 1.0
    wasm!(f, {
        local_get(0); f64_const(1.0); f64_eq;
        if_empty;
          f64_const(1.0); return_;
        end;
    });

    // Special case: base == 0.0 -> 0.0 (for positive exp)
    wasm!(f, {
        local_get(0); f64_const(0.0); f64_eq;
        if_empty;
          f64_const(0.0); return_;
        end;
    });

    // Check if exp is an integer: trunc(exp) == exp
    wasm!(f, {
        local_get(1);
    });
    f.instruction(&Instruction::F64Trunc);
    wasm!(f, {
        local_set(5);
        local_get(1); local_get(5); f64_eq;
        if_f64;
    });

    // --- Integer exponent path: binary exponentiation ---
    wasm!(f, {
          local_get(1); f64_const(0.0); f64_lt; local_set(6);
          local_get(5); f64_abs; i64_trunc_f64_s; local_set(3);
          f64_const(1.0); local_set(2);
          local_get(0); local_set(4);
    });

    // Binary exponentiation loop
    wasm!(f, {
          block_empty; loop_empty;
            local_get(3); i64_eqz; br_if(1);
            local_get(3); i64_const(1); i64_and; i64_eqz;
            i32_eqz;
            if_empty;
              local_get(2); local_get(4); f64_mul; local_set(2);
            end;
            local_get(4); local_get(4); f64_mul; local_set(4);
            local_get(3); i64_const(1); i64_shr_s; local_set(3);
            br(0);
          end; end;
    });

    // If negative exp: result = 1.0 / result
    wasm!(f, {
          local_get(6);
          if_empty;
            f64_const(1.0); local_get(2); f64_div; local_set(2);
          end;
          local_get(2);
    });

    // --- Fractional exponent path: exp(exp * ln(base)) ---
    wasm!(f, {
        else_;
    });

    // Compute ln(base) via the math_log runtime function -> store in local 7
    wasm!(f, {
        local_get(0); f64_abs;
        call(emitter.rt.math_log);
        local_set(7);
    });

    // exp_arg = exp * ln(base) -> local 7 (reuse)
    wasm!(f, {
        local_get(1); local_get(7); f64_mul; local_set(7);
    });

    // exp(exp_arg) via Taylor: sum = 1, term = 1
    // local 8 = term, local 2 = sum (reuse)
    wasm!(f, {
        f64_const(1.0); local_set(8); // term = 1
        f64_const(1.0); local_set(2); // sum = 1
    });

    // 15 terms: term *= x/k, sum += term
    for k in 1..=15 {
        wasm!(f, {
            local_get(8); local_get(7); f64_mul;
            f64_const(k as f64); f64_div;
            local_set(8);
            local_get(2); local_get(8); f64_add; local_set(2);
        });
    }

    wasm!(f, {
        local_get(2); // result
        end; // end else (if/else for int vs frac)
    });

    wasm!(f, { end; }); // end function

    emitter.add_compiled(CompiledFunc { type_idx, func: f });
}

/// __math_sin(x: f64) -> f64
/// Taylor series: sin(x) = x - x^3/3! + x^5/5! - x^7/7! + ...
/// With range reduction to [-pi, pi] first.
pub(super) fn compile_math_sin(emitter: &mut WasmEmitter) {
    let type_idx = emitter.func_type_indices[&emitter.rt.math_sin];
    // params: 0=f64 x
    // locals: 1=f64 x_reduced, 2=f64 term, 3=f64 sum, 4=f64 x2
    let mut f = Function::new([
        (1, ValType::F64),  // 1: x_reduced
        (1, ValType::F64),  // 2: term
        (1, ValType::F64),  // 3: sum
        (1, ValType::F64),  // 4: x2 (x*x, precomputed)
    ]);

    const TWO_PI: f64 = std::f64::consts::TAU;
    const PI: f64 = std::f64::consts::PI;

    // Range reduction: x = x - floor(x / (2*pi)) * (2*pi)
    wasm!(f, {
        local_get(0);
        local_get(0); f64_const(TWO_PI); f64_div; f64_floor; f64_const(TWO_PI); f64_mul;
        f64_sub;
        local_set(1);
    });
    // If x > pi: x -= 2*pi
    wasm!(f, {
        local_get(1); f64_const(PI); f64_gt;
        if_empty;
          local_get(1); f64_const(TWO_PI); f64_sub; local_set(1);
        end;
    });
    // If x < -pi: x += 2*pi
    wasm!(f, {
        local_get(1); f64_const(-PI); f64_lt;
        if_empty;
          local_get(1); f64_const(TWO_PI); f64_add; local_set(1);
        end;
    });

    // x2 = x * x
    wasm!(f, {
        local_get(1); local_get(1); f64_mul; local_set(4);
    });

    // term = x, sum = x
    wasm!(f, {
        local_get(1); local_set(2);
        local_get(1); local_set(3);
    });

    // 8 iterations: term *= -x^2 / ((2k)*(2k+1)), sum += term
    // k=1: term *= -x2 / (2*3)
    // k=2: term *= -x2 / (4*5)
    // ...
    for k in 1..=8u32 {
        let denom = ((2 * k) * (2 * k + 1)) as f64;
        wasm!(f, {
            local_get(2); local_get(4); f64_mul; f64_neg;
            f64_const(denom); f64_div; local_set(2);
            local_get(3); local_get(2); f64_add; local_set(3);
        });
    }

    wasm!(f, { local_get(3); end; });
    emitter.add_compiled(CompiledFunc { type_idx, func: f });
}

/// __math_cos(x: f64) -> f64
/// Taylor series: cos(x) = 1 - x^2/2! + x^4/4! - x^6/6! + ...
/// With range reduction to [-pi, pi] first.
pub(super) fn compile_math_cos(emitter: &mut WasmEmitter) {
    let type_idx = emitter.func_type_indices[&emitter.rt.math_cos];
    // params: 0=f64 x
    // locals: 1=f64 x_reduced, 2=f64 term, 3=f64 sum, 4=f64 x2
    let mut f = Function::new([
        (1, ValType::F64),  // 1: x_reduced
        (1, ValType::F64),  // 2: term
        (1, ValType::F64),  // 3: sum
        (1, ValType::F64),  // 4: x2
    ]);

    const TWO_PI: f64 = std::f64::consts::TAU;
    const PI: f64 = std::f64::consts::PI;

    // Range reduction: x = x - floor(x / (2*pi)) * (2*pi)
    wasm!(f, {
        local_get(0);
        local_get(0); f64_const(TWO_PI); f64_div; f64_floor; f64_const(TWO_PI); f64_mul;
        f64_sub;
        local_set(1);
    });
    // If x > pi: x -= 2*pi
    wasm!(f, {
        local_get(1); f64_const(PI); f64_gt;
        if_empty;
          local_get(1); f64_const(TWO_PI); f64_sub; local_set(1);
        end;
    });
    // If x < -pi: x += 2*pi
    wasm!(f, {
        local_get(1); f64_const(-PI); f64_lt;
        if_empty;
          local_get(1); f64_const(TWO_PI); f64_add; local_set(1);
        end;
    });

    // x2 = x * x
    wasm!(f, {
        local_get(1); local_get(1); f64_mul; local_set(4);
    });

    // term = 1.0, sum = 1.0
    wasm!(f, {
        f64_const(1.0); local_set(2);
        f64_const(1.0); local_set(3);
    });

    // 8 iterations: term *= -x^2 / ((2k-1)*(2k)), sum += term
    // k=1: term *= -x2 / (1*2)
    // k=2: term *= -x2 / (3*4)
    // ...
    for k in 1..=8u32 {
        let denom = ((2 * k - 1) * (2 * k)) as f64;
        wasm!(f, {
            local_get(2); local_get(4); f64_mul; f64_neg;
            f64_const(denom); f64_div; local_set(2);
            local_get(3); local_get(2); f64_add; local_set(3);
        });
    }

    wasm!(f, { local_get(3); end; });
    emitter.add_compiled(CompiledFunc { type_idx, func: f });
}

/// __math_tan(x: f64) -> f64
/// tan(x) = sin(x) / cos(x)
pub(super) fn compile_math_tan(emitter: &mut WasmEmitter) {
    let type_idx = emitter.func_type_indices[&emitter.rt.math_tan];
    let mut f = Function::new([]);

    wasm!(f, {
        local_get(0); call(emitter.rt.math_sin);
        local_get(0); call(emitter.rt.math_cos);
        f64_div;
        end;
    });
    emitter.add_compiled(CompiledFunc { type_idx, func: f });
}

/// __math_log(x: f64) -> f64
/// Natural logarithm via IEEE 754 exponent extraction + sqrt reduction + Taylor.
/// Decompose x = m * 2^e (1 <= m < 2), then ln(x) = e*ln(2) + ln(m).
/// ln(m) computed via 10 rounds of sqrt reduction + 7-term Taylor series.
/// Since m is in [1,2), sqrt reduction gives values very close to 1, ensuring
/// fast convergence and full f64 precision without large multiplier amplification.
pub(super) fn compile_math_log(emitter: &mut WasmEmitter) {
    let type_idx = emitter.func_type_indices[&emitter.rt.math_log];
    // params: 0=f64 x
    // locals:
    //   1=i64 bits,    2=i64 exp_raw,  3=f64 exp_f64 (e),
    //   4=f64 m (mantissa), 5=f64 y (reduced m), 6=f64 z (y-1),
    //   7=f64 z^k,     8=f64 sum
    let mut f = Function::new([
        (1, ValType::I64),  // 1: bits
        (1, ValType::I64),  // 2: exp_raw
        (1, ValType::F64),  // 3: exp as f64
        (1, ValType::F64),  // 4: mantissa m
        (1, ValType::F64),  // 5: y (sqrt-reduced m)
        (1, ValType::F64),  // 6: z = y - 1
        (1, ValType::F64),  // 7: z^k
        (1, ValType::F64),  // 8: sum
    ]);

    // Special case: x <= 0 -> NaN
    wasm!(f, {
        local_get(0); f64_const(0.0); f64_le;
        if_empty;
          f64_const(f64::NAN); return_;
        end;
    });

    // Special case: x == 1.0 -> 0.0
    wasm!(f, {
        local_get(0); f64_const(1.0); f64_eq;
        if_empty;
          f64_const(0.0); return_;
        end;
    });

    // Extract IEEE 754 bits
    // bits = reinterpret(x)
    wasm!(f, {
        local_get(0); i64_reinterpret_f64; local_set(1);
    });

    // exp_raw = (bits >> 52) & 0x7FF
    // Using arithmetic shift (i64_shr_s) + mask; mask eliminates sign extension.
    wasm!(f, {
        local_get(1); i64_const(52); i64_shr_s;
        i64_const(0x7FF); i64_and;
        local_set(2);
    });

    // e = exp_raw - 1023 (as f64)
    wasm!(f, {
        local_get(2); i64_const(1023); i64_sub;
        f64_convert_i64_s; local_set(3);
    });

    // m = reinterpret((bits & 0x000FFFFFFFFFFFFF) | (1023 << 52))
    // This sets exponent to 0 (biased 1023), giving m in [1.0, 2.0)
    wasm!(f, {
        local_get(1);
        i64_const(0x000FFFFFFFFFFFFF_u64 as i64); i64_and;
        i64_const(0x3FF0000000000000_u64 as i64); i64_or;
        f64_reinterpret_i64;
        local_set(4);
    });

    // If m == 1.0, ln(m) = 0.0, so ln(x) = e * ln(2)
    wasm!(f, {
        local_get(4); f64_const(1.0); f64_eq;
        if_empty;
          local_get(3); f64_const(std::f64::consts::LN_2); f64_mul;
          return_;
        end;
    });

    // y = m
    wasm!(f, {
        local_get(4); local_set(5);
    });

    // 10 rounds of sqrt: y = sqrt^10(m)
    // m is in [1, 2), so after 10 sqrts y is extremely close to 1.
    // Multiplier is 2^10 = 1024.
    for _ in 0..10 {
        wasm!(f, {
            local_get(5); f64_sqrt; local_set(5);
        });
    }

    // z = y - 1
    wasm!(f, {
        local_get(5); f64_const(1.0); f64_sub; local_set(6);
        local_get(6); local_set(7);  // z^1
        local_get(6); local_set(8);  // sum = z
    });

    // Taylor: ln(1+z) = z - z^2/2 + z^3/3 - z^4/4 + ... + z^7/7
    // With m in [1,2) and 10 sqrt rounds, z ~ 10^-4, so 7 terms is overkill.
    // k=2: sum -= z^2/2
    wasm!(f, { local_get(7); local_get(6); f64_mul; local_set(7); });
    wasm!(f, { local_get(8); local_get(7); f64_const(2.0); f64_div; f64_sub; local_set(8); });
    // k=3: sum += z^3/3
    wasm!(f, { local_get(7); local_get(6); f64_mul; local_set(7); });
    wasm!(f, { local_get(8); local_get(7); f64_const(3.0); f64_div; f64_add; local_set(8); });
    // k=4: sum -= z^4/4
    wasm!(f, { local_get(7); local_get(6); f64_mul; local_set(7); });
    wasm!(f, { local_get(8); local_get(7); f64_const(4.0); f64_div; f64_sub; local_set(8); });
    // k=5: sum += z^5/5
    wasm!(f, { local_get(7); local_get(6); f64_mul; local_set(7); });
    wasm!(f, { local_get(8); local_get(7); f64_const(5.0); f64_div; f64_add; local_set(8); });
    // k=6: sum -= z^6/6
    wasm!(f, { local_get(7); local_get(6); f64_mul; local_set(7); });
    wasm!(f, { local_get(8); local_get(7); f64_const(6.0); f64_div; f64_sub; local_set(8); });
    // k=7: sum += z^7/7
    wasm!(f, { local_get(7); local_get(6); f64_mul; local_set(7); });
    wasm!(f, { local_get(8); local_get(7); f64_const(7.0); f64_div; f64_add; local_set(8); });

    // ln(m) = 1024 * sum
    // ln(x) = e * ln(2) + ln(m)
    wasm!(f, {
        local_get(3); f64_const(std::f64::consts::LN_2); f64_mul;
        local_get(8); f64_const(1024.0); f64_mul;
        f64_add;
        end;
    });

    emitter.add_compiled(CompiledFunc { type_idx, func: f });
}

/// __math_log10(x: f64) -> f64
/// Common logarithm. Computes ln(x) / ln(10), with rounding correction
/// so that exact powers of 10 return exact integer results.
pub(super) fn compile_math_log10(emitter: &mut WasmEmitter) {
    let type_idx = emitter.func_type_indices[&emitter.rt.math_log10];
    // params: 0=f64 x
    // locals: 1=f64 result, 2=f64 rounded
    let mut f = Function::new([
        (1, ValType::F64),  // 1: result
        (1, ValType::F64),  // 2: rounded
    ]);

    // result = ln(x) / ln(10)
    wasm!(f, {
        local_get(0);
        call(emitter.rt.math_log);
        f64_const(std::f64::consts::LN_10);
        f64_div;
        local_set(1);
    });

    // rounded = nearest(result)
    wasm!(f, {
        local_get(1); f64_nearest; local_set(2);
    });

    // if |result - rounded| < 1e-12: return rounded, else return result
    wasm!(f, {
        local_get(1); local_get(2); f64_sub; f64_abs;
        f64_const(1e-12); f64_lt;
        if_f64;
          local_get(2);
        else_;
          local_get(1);
        end;
        end;
    });

    emitter.add_compiled(CompiledFunc { type_idx, func: f });
}

/// __math_log2(x: f64) -> f64
/// Binary logarithm. Computes ln(x) / ln(2), with rounding correction
/// so that exact powers of 2 return exact integer results.
pub(super) fn compile_math_log2(emitter: &mut WasmEmitter) {
    let type_idx = emitter.func_type_indices[&emitter.rt.math_log2];
    // params: 0=f64 x
    // locals: 1=f64 result, 2=f64 rounded
    let mut f = Function::new([
        (1, ValType::F64),  // 1: result
        (1, ValType::F64),  // 2: rounded
    ]);

    // result = ln(x) / ln(2)
    wasm!(f, {
        local_get(0);
        call(emitter.rt.math_log);
        f64_const(std::f64::consts::LN_2);
        f64_div;
        local_set(1);
    });

    // rounded = nearest(result)
    wasm!(f, {
        local_get(1); f64_nearest; local_set(2);
    });

    // if |result - rounded| < 1e-12: return rounded, else return result
    wasm!(f, {
        local_get(1); local_get(2); f64_sub; f64_abs;
        f64_const(1e-12); f64_lt;
        if_f64;
          local_get(2);
        else_;
          local_get(1);
        end;
        end;
    });

    emitter.add_compiled(CompiledFunc { type_idx, func: f });
}

/// __math_exp(x: f64) -> f64
/// e^x via range reduction + Taylor series.
/// Split x = n + frac where n = floor(x). Then e^x = e^n * e^frac.
/// e^n via repeated squaring of e, e^frac via Taylor series (15 terms).
pub(super) fn compile_math_exp(emitter: &mut WasmEmitter) {
    let type_idx = emitter.func_type_indices[&emitter.rt.math_exp];
    // params: 0=f64 x
    // locals: 1=f64 int_part, 2=f64 frac, 3=i64 n, 4=i32 is_neg,
    //         5=f64 base_pow (e^|n|), 6=f64 term, 7=f64 sum
    let mut f = Function::new([
        (1, ValType::F64),  // 1: int_part
        (1, ValType::F64),  // 2: frac
        (1, ValType::I64),  // 3: n (absolute)
        (1, ValType::I32),  // 4: is_neg
        (1, ValType::F64),  // 5: base_pow
        (1, ValType::F64),  // 6: term
        (1, ValType::F64),  // 7: sum
    ]);

    // Special case: x == 0.0 -> 1.0
    wasm!(f, {
        local_get(0); f64_const(0.0); f64_eq;
        if_empty;
          f64_const(1.0); return_;
        end;
    });

    // int_part = trunc(x)
    wasm!(f, {
        local_get(0);
    });
    f.instruction(&Instruction::F64Trunc);
    wasm!(f, {
        local_set(1);
    });

    // frac = x - int_part
    wasm!(f, {
        local_get(0); local_get(1); f64_sub; local_set(2);
    });

    // is_neg = int_part < 0
    wasm!(f, {
        local_get(1); f64_const(0.0); f64_lt; local_set(4);
    });

    // n = |int_part| as i64
    wasm!(f, {
        local_get(1); f64_abs; i64_trunc_f64_s; local_set(3);
    });

    // Compute e^|n| via binary exponentiation: base_pow = 1.0, base = e
    // base_pow local_set(5), reuse local_get(1) as base (e)
    wasm!(f, {
        f64_const(1.0); local_set(5);
        f64_const(std::f64::consts::E); local_set(1); // reuse local 1 as base
        block_empty; loop_empty;
          local_get(3); i64_eqz; br_if(1);
          // if n & 1: base_pow *= base
          local_get(3); i64_const(1); i64_and; i64_eqz;
          i32_eqz;
          if_empty;
            local_get(5); local_get(1); f64_mul; local_set(5);
          end;
          local_get(1); local_get(1); f64_mul; local_set(1);
          local_get(3); i64_const(1); i64_shr_s; local_set(3);
          br(0);
        end; end;
    });

    // If is_neg: base_pow = 1.0 / base_pow
    wasm!(f, {
        local_get(4);
        if_empty;
          f64_const(1.0); local_get(5); f64_div; local_set(5);
        end;
    });

    // Compute e^frac via Taylor series: sum = 1, term = 1
    wasm!(f, {
        f64_const(1.0); local_set(6); // term = 1
        f64_const(1.0); local_set(7); // sum = 1
    });

    // 20 terms: term *= frac/k, sum += term
    for k in 1..=20 {
        wasm!(f, {
            local_get(6); local_get(2); f64_mul;
            f64_const(k as f64); f64_div;
            local_set(6);
            local_get(7); local_get(6); f64_add; local_set(7);
        });
    }

    // result = base_pow * sum
    wasm!(f, {
        local_get(5); local_get(7); f64_mul;
        end;
    });

    emitter.add_compiled(CompiledFunc { type_idx, func: f });
}
