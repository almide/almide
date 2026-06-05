//! WASM runtime: int.from_hex, float.parse, float.to_fixed, math.fpow.
//!
//! Called from `compile_runtime()` in runtime.rs.

use super::{CompiledFunc, WasmEmitter};
use wasm_encoder::{Instruction, ValType};
use super::TrackedFunction as Function;

/// __int_from_hex(s: i32) -> i32
///
/// Byte-for-byte mirror of the native oracle (runtime/rs/src/int.rs):
///   `i64::from_str_radix(s.trim().trim_start_matches("0x"), 16).map_err(|e| e.to_string())`
/// The quirks this implies (all native-as-oracle, see contract_notes):
///   - `trim()` strips leading/trailing Unicode whitespace.
///   - `trim_start_matches("0x")` strips a lowercase "0x" prefix REPEATEDLY
///     ("0x0xff" → "ff" → 255) and is CASE-SENSITIVE ("0X10" is NOT stripped,
///     so it then fails on 'X' as an invalid digit).
///   - NO underscore skipping ("f_f" is an invalid digit).
///   - a single sign (+/-) is accepted AFTER 0x-stripping ("0x-ff" → -255).
///   - the four std `ParseIntError` strings are reproduced exactly (shared with
///     int.parse, deduped by intern_string).
/// Layout: [tag:i32][value:i64] = 12 bytes. tag=0 ok, tag=1 err (str ptr at offset 4).
pub(super) fn compile_int_from_hex(emitter: &mut WasmEmitter) {
    let type_idx = emitter.func_type_indices[&emitter.rt.int_from_hex];
    let data_off = emitter.layout_reg.fixed_offset(
        super::engine::layout::STRING, super::engine::layout::string::DATA) as i32;

    // The same four std error strings int.parse uses (intern dedups them).
    let err_empty = emitter.intern_string("cannot parse integer from empty string");
    let err_digit = emitter.intern_string("invalid digit found in string");
    let err_large = emitter.intern_string("number too large to fit in target type");
    let err_small = emitter.intern_string("number too small to fit in target type");
    let alloc = emitter.rt.alloc;

    const RADIX: i64 = 16;
    // params: 0=$s (string ptr: [len:i32][data:u8...])
    // locals:
    //   1=len, 2=i (cursor), 3=end (exclusive, after trailing trim),
    //   4=is_neg, 5=byte, 6=alloc_ptr, 7=acc (i64 magnitude, u64 semantics),
    //   8=digit (i32), 9=limit (i64 max magnitude for sign), 10=tmp (i64)
    let mut f = Function::new([
        (1, ValType::I32),  // 1: len
        (1, ValType::I32),  // 2: i
        (1, ValType::I32),  // 3: end
        (1, ValType::I32),  // 4: is_neg
        (1, ValType::I32),  // 5: byte
        (1, ValType::I32),  // 6: alloc_ptr
        (1, ValType::I64),  // 7: acc
        (1, ValType::I32),  // 8: digit
        (1, ValType::I64),  // 9: limit
        (1, ValType::I64),  // 10: tmp
        (1, ValType::I32),  // 11: scratch for trim_backward
    ]);

    // Emit an `err(<interned string>)` return: alloc [tag=1][str_ptr] and return.
    let emit_err = |f: &mut Function, err_str: u32| {
        wasm!(f, {
            i32_const(12); call(alloc); local_set(6);
            local_get(6); i32_const(1); i32_store(0);
            local_get(6); i32_const(err_str as i32); i32_store(4);
            local_get(6);
            return_;
        });
    };
    // Emit `byte = s[data_off + idx_local]`.
    let load_byte = |f: &mut Function, idx_local: u32, dst: u32| {
        wasm!(f, {
            local_get(0); i32_const(data_off); i32_add;
            local_get(idx_local); i32_add;
            i32_load8_u(0); local_set(dst);
        });
    };

    // len = s.len
    wasm!(f, { local_get(0); i32_load(0); local_set(1); });

    // Trim leading + trailing Unicode whitespace (s.trim()).
    wasm!(f, { i32_const(0); local_set(2); });
    super::rt_string::emit_trim_forward(&mut f, emitter, 2, 1);
    wasm!(f, { local_get(1); local_set(3); });
    super::rt_string::emit_trim_backward(&mut f, emitter, 3, 2, 11);

    // Strip a lowercase "0x" prefix REPEATEDLY: while (end-i >= 2 && s[i]=='0' && s[i+1]=='x') i += 2.
    wasm!(f, {
        block_empty; loop_empty;
          // need at least 2 chars remaining
          local_get(3); local_get(2); i32_sub; i32_const(2); i32_lt_s; br_if(1);
    });
    load_byte(&mut f, 2, 5);
    wasm!(f, {
          local_get(5); i32_const(48); i32_ne; br_if(1);   // s[i] != '0'
    });
    // s[i+1] == 'x' (lowercase only)
    wasm!(f, { i32_const(0); local_set(5); }); // reuse byte; compute s[i+1]
    wasm!(f, {
          local_get(0); i32_const(data_off); i32_add; local_get(2); i32_add; i32_const(1); i32_add;
          i32_load8_u(0); local_set(5);
          local_get(5); i32_const(120); i32_ne; br_if(1);   // s[i+1] != 'x'
          local_get(2); i32_const(2); i32_add; local_set(2); // i += 2
          br(0);
        end; end;
    });

    // Empty after trim + strip → "cannot parse integer from empty string"
    wasm!(f, { local_get(2); local_get(3); i32_ge_u; if_empty; });
    emit_err(&mut f, err_empty);
    wasm!(f, { end; });

    // Optional single leading sign (from_str_radix accepts +/-).
    wasm!(f, { i32_const(0); local_set(4); }); // is_neg = 0
    load_byte(&mut f, 2, 5);
    wasm!(f, {
        local_get(5); i32_const(45); i32_eq; // '-'
        if_empty;
          i32_const(1); local_set(4);
          local_get(2); i32_const(1); i32_add; local_set(2);
        else_;
          local_get(5); i32_const(43); i32_eq; // '+'
          if_empty;
            local_get(2); i32_const(1); i32_add; local_set(2);
          end;
        end;
    });

    // No digits after sign → "invalid digit found in string"
    wasm!(f, { local_get(2); local_get(3); i32_ge_u; if_empty; });
    emit_err(&mut f, err_digit);
    wasm!(f, { end; });

    // limit = is_neg ? |i64::MIN| : i64::MAX  (u64 semantics)
    wasm!(f, {
        local_get(4);
        if_empty; i64_const(i64::MIN); local_set(9);
        else_; i64_const(i64::MAX); local_set(9); end;
        i64_const(0); local_set(7);   // acc = 0
    });

    // Main parse loop: while i < end
    wasm!(f, { block_empty; loop_empty;
        local_get(2); local_get(3); i32_ge_u; br_if(1);
    });
    load_byte(&mut f, 2, 5);

    // Classify hex digit: '0'-'9' → 0-9, 'a'-'f' → 10-15, 'A'-'F' → 10-15, else -1.
    wasm!(f, {
        i32_const(-1); local_set(8);
        local_get(5); i32_const(48); i32_ge_u;
        local_get(5); i32_const(57); i32_le_u; i32_and;
        if_empty;
          local_get(5); i32_const(48); i32_sub; local_set(8);
        else_;
          local_get(5); i32_const(97); i32_ge_u;
          local_get(5); i32_const(102); i32_le_u; i32_and;
          if_empty;
            local_get(5); i32_const(87); i32_sub; local_set(8); // 'a'-87 = 10
          else_;
            local_get(5); i32_const(65); i32_ge_u;
            local_get(5); i32_const(70); i32_le_u; i32_and;
            if_empty;
              local_get(5); i32_const(55); i32_sub; local_set(8); // 'A'-55 = 10
            end;
          end;
        end;
    });

    // Invalid digit → "invalid digit found in string"
    wasm!(f, { local_get(8); i32_const(-1); i32_eq; if_empty; });
    emit_err(&mut f, err_digit);
    wasm!(f, { end; });

    // d (i64) → tmp
    wasm!(f, { local_get(8); i64_extend_i32_u; local_set(10); });

    // Overflow step 1: acc > limit/RADIX → overflow.
    wasm!(f, {
        local_get(9); i64_const(RADIX); i64_div_u;
        local_get(7); i64_ge_u; i32_eqz;
        if_empty;
    });
    wasm!(f, { local_get(4); if_empty; });
    emit_err(&mut f, err_small);
    wasm!(f, { else_; });
    emit_err(&mut f, err_large);
    wasm!(f, { end; end; });

    // acc = acc * RADIX
    wasm!(f, { local_get(7); i64_const(RADIX); i64_mul; local_set(7); });

    // Overflow step 2: acc > limit - d → overflow.
    wasm!(f, {
        local_get(9); local_get(10); i64_sub;
        local_get(7); i64_ge_u; i32_eqz;
        if_empty;
    });
    wasm!(f, { local_get(4); if_empty; });
    emit_err(&mut f, err_small);
    wasm!(f, { else_; });
    emit_err(&mut f, err_large);
    wasm!(f, { end; end; });

    // acc = acc + d; i++
    wasm!(f, {
        local_get(7); local_get(10); i64_add; local_set(7);
        local_get(2); i32_const(1); i32_add; local_set(2);
        br(0);
        end; end;
    });

    // Materialize signed value: negative → 0 - acc (wraps to i64::MIN at 2^63).
    wasm!(f, {
        local_get(4);
        if_empty; i64_const(0); local_get(7); i64_sub; local_set(7); end;
    });

    // Return ok(value): alloc [tag=0][value:i64]
    wasm!(f, {
        i32_const(12); call(alloc); local_set(6);
        local_get(6); i32_const(0); i32_store(0);
        local_get(6); local_get(7); i64_store(4);
        local_get(6);
        end;
    });

    emitter.add_compiled(CompiledFunc::tracked(type_idx, f));
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
    //
    // Mirrors native Rust `s.trim().parse::<f64>()`:
    //   - trims ASCII whitespace (leading + trailing)
    //   - optional leading sign (+/-)
    //   - inf / infinity / nan (case-insensitive, with sign)
    //   - decimal mantissa with optional '.' (".5", "5.", "5.5" all valid)
    //   - optional scientific exponent (e/E [+/-] digits) scaled by 10^exp
    //   - Err strings byte-match Rust: "cannot parse float from empty string"
    //     (empty/whitespace-only) and "invalid float literal" (malformed).
    //
    // locals:
    //   1=i32 len, 2=i32 i (cursor), 3=f64 result (mantissa),
    //   4=i32 is_neg, 5=i32 byte, 6=i32 alloc_ptr, 7=f64 frac_mult,
    //   8=i32 has_dot, 9=i32 digit_count, 10=i32 end (exclusive),
    //   11=i32 data_base (s+DATA), 12=i32 exp_neg, 13=i32 exp_val,
    //   14=i32 exp_digit_count, 15=f64 pow10, 16=i32 saw_e
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
        (1, ValType::I32),  // 10: end
        (1, ValType::I32),  // 11: data_base
        (1, ValType::I32),  // 12: exp_neg
        (1, ValType::I32),  // 13: exp_val
        (1, ValType::I32),  // 14: exp_digit_count
        (1, ValType::F64),  // 15: pow10
        (1, ValType::I32),  // 16: saw_e
        (1, ValType::I32),  // 17: sig (significand bignum ptr)
        (1, ValType::I32),  // 18: frac_count (fractional digits kept in sig)
        (1, ValType::I32),  // 19: started (seen the first significant digit)
        (1, ValType::I32),  // 20: sig_digits (significant digits accumulated)
        (1, ValType::I32),  // 21: sticky (a dropped significand digit was non-zero)
    ]);

    let empty_err = emitter.intern_string("cannot parse float from empty string");
    let invalid_err = emitter.intern_string("invalid float literal");
    let sig_stride = super::rt_dragon::BN_STRIDE as i32;
    let sig_hdr = super::rt_dragon::BN_HDR as i32;
    let mul_small = emitter.rt.dragon.mul_small;
    let bn_add_small = emitter.rt.decfloat.bn_add_small;
    let dec2flt = emitter.rt.decfloat.dec2flt;
    // Saturate the base-10 exponent magnitude well before it can overflow the i32
    // accumulator (`exp_val*10 + digit`). Any |exp10| past a few hundred already
    // rounds to ±inf or ±0 in __dec2flt, so clamping here can't change a result —
    // it only stops a huge exponent ("1e2147483648") from wrapping i32 to garbage.
    let exp_magnitude_clamp = 100_000_000_i32;
    let data_off = emitter
        .layout_reg
        .fixed_offset(super::engine::layout::STRING, super::engine::layout::string::DATA)
        as i32;

    // len = s.len ; data_base = s + DATA_OFFSET
    wasm!(f, {
        local_get(0); i32_load(0); local_set(1);
        local_get(0); i32_const(data_off); i32_add; local_set(11);
    });

    // Initialize: i = 0, end = len, defaults.
    wasm!(f, {
        i32_const(0); local_set(2);
        local_get(1); local_set(10);
        f64_const(0.0); local_set(3);
        i32_const(0); local_set(4);
        f64_const(1.0); local_set(7);
        i32_const(0); local_set(8);
        i32_const(0); local_set(9);
        i32_const(0); local_set(12);
        i32_const(0); local_set(13);
        i32_const(0); local_set(14);
        i32_const(0); local_set(16);
        // Significand bignum (len=1, limb0=0) — built digit by digit, then handed
        // to __dec2flt for correctly-rounded scaling. frac_count = fractional digits.
        i32_const(sig_stride); call(emitter.rt.alloc); local_set(17);
        local_get(17); i32_const(1); i32_store(0);
        local_get(17); i32_const(sig_hdr); i32_add; i32_const(0); i32_store(0);
        i32_const(0); local_set(18);
        i32_const(0); local_set(19);   // started
        i32_const(0); local_set(20);   // sig_digits
        i32_const(0); local_set(21);   // sticky
    });

    // Trim leading + trailing Unicode whitespace (matches native s.trim().parse),
    // codepoint-aware via the shared __is_unicode_ws helpers. i=cursor(2),
    // end=10, string ptr=0, scratch q=5.
    super::rt_string::emit_trim_forward(&mut f, emitter, 2, 10);
    super::rt_string::emit_trim_backward(&mut f, emitter, 10, 2, 5);

    // Empty after trim -> "cannot parse float from empty string"
    wasm!(f, {
        local_get(2); local_get(10); i32_ge_u;
        if_empty;
    });
    emit_float_parse_err(&mut f, emitter, empty_err);
    wasm!(f, { end; });

    // Optional leading sign at data[i].
    wasm!(f, {
        local_get(11); local_get(2); i32_add; i32_load8_u(0); local_set(5);
        local_get(5); i32_const(45); i32_eq;   // '-'
        if_empty;
          i32_const(1); local_set(4);
          local_get(2); i32_const(1); i32_add; local_set(2);
        else_;
          local_get(5); i32_const(43); i32_eq; // '+'
          if_empty;
            local_get(2); i32_const(1); i32_add; local_set(2);
          end;
        end;
    });

    // After the sign, the body [i, end) must be non-empty (rejects "+", "-").
    wasm!(f, {
        local_get(2); local_get(10); i32_ge_u;
        if_empty;
    });
    emit_float_parse_err(&mut f, emitter, invalid_err);
    wasm!(f, { end; });

    // inf / infinity (case-insensitive) -> ±inf
    emit_kw_match(&mut f, b"infinity", 2, 10, 11);
    emit_kw_match(&mut f, b"inf", 2, 10, 11);
    wasm!(f, {
        i32_or;
        if_empty;
    });
    emit_float_parse_ok_special(&mut f, emitter, f64::INFINITY);
    wasm!(f, { end; });

    // nan (case-insensitive) -> NaN (sign honored to byte-match Rust's -nan bits;
    // not observable through float.to_string but kept faithful).
    emit_kw_match(&mut f, b"nan", 2, 10, 11);
    wasm!(f, {
        if_empty;
    });
    emit_float_parse_ok_special(&mut f, emitter, f64::NAN);
    wasm!(f, { end; });

    // --- Decimal mantissa: scan [i, end). On 'e'/'E' break to exponent. ---
    wasm!(f, {
        block_empty; loop_empty;
          local_get(2); local_get(10); i32_ge_u; br_if(1);
          local_get(11); local_get(2); i32_add; i32_load8_u(0); local_set(5);

          // '.' -> set has_dot (err on second dot)
          local_get(5); i32_const(46); i32_eq;
          if_empty;
            local_get(8); if_empty;
    });
    emit_float_parse_err(&mut f, emitter, invalid_err);
    wasm!(f, {
            end;
            i32_const(1); local_set(8);
            local_get(2); i32_const(1); i32_add; local_set(2);
            br(1);
          end;

          // 'e' / 'E' -> exponent: mark and break out of mantissa loop
          local_get(5); i32_const(101); i32_eq;   // 'e'
          local_get(5); i32_const(69); i32_eq;     // 'E'
          i32_or;
          if_empty;
            i32_const(1); local_set(16);
            local_get(2); i32_const(1); i32_add; local_set(2);
            br(2);                                  // exit loop+block
          end;

          // digit '0'..'9' ?
          local_get(5); i32_const(48); i32_lt_u;
          local_get(5); i32_const(57); i32_gt_u;
          i32_or;
          if_empty;
    });
    emit_float_parse_err(&mut f, emitter, invalid_err);
    wasm!(f, {
          end;

          local_get(9); i32_const(1); i32_add; local_set(9);   // digit_count++ (all digits)
          // started |= (digit != 0) — leading zeros carry no significance.
          local_get(19); local_get(5); i32_const(48); i32_ne; i32_or; local_set(19);
          local_get(19); i32_eqz;
          if_empty;
            // Leading zero: it only advances the fractional scale (frac_count),
            // never the significand or the precision budget. So a flat decimal
            // like "0.000…0123" keeps full precision for its real digits.
            local_get(8); if_empty; local_get(18); i32_const(1); i32_add; local_set(18); end;
          else_;
            // Significant digit. Accumulate sig = sig*10 + digit while under the
            // precision cap (keeps the bignum within NLIMBS); track the exact
            // decimal exponent via frac_count. Past the cap, drop the digit but
            // OR a non-zero value into `sticky` so __dec2flt can break a tie.
            local_get(20); i32_const(super::rt_dec2flt::SIG_DIGIT_CAP); i32_lt_u;
            if_empty;
              local_get(17); i32_const(super::rt_dec2flt::DECIMAL_BASE); call(mul_small);
              local_get(17); local_get(5); i32_const(48); i32_sub; call(bn_add_small);
              local_get(20); i32_const(1); i32_add; local_set(20);   // sig_digits++
              local_get(8); if_empty; local_get(18); i32_const(1); i32_add; local_set(18); end;
            else_;
              local_get(21); local_get(5); i32_const(48); i32_ne; i32_or; local_set(21);
            end;
          end;

          local_get(2); i32_const(1); i32_add; local_set(2);
          br(0);
        end; end;
    });

    // Need at least one mantissa digit (rejects ".", "e3", ".e1").
    wasm!(f, {
        local_get(9); i32_eqz;
        if_empty;
    });
    emit_float_parse_err(&mut f, emitter, invalid_err);
    wasm!(f, { end; });

    // --- Exponent (only if we saw 'e'/'E') ---
    wasm!(f, {
        local_get(16);
        if_empty;
          // Optional exponent sign at data[i].
          local_get(2); local_get(10); i32_ge_u;
          if_empty;
    });
    // "1e" with no exponent body -> invalid.
    emit_float_parse_err(&mut f, emitter, invalid_err);
    wasm!(f, {
          end;
          local_get(11); local_get(2); i32_add; i32_load8_u(0); local_set(5);
          local_get(5); i32_const(45); i32_eq;   // '-'
          if_empty;
            i32_const(1); local_set(12);
            local_get(2); i32_const(1); i32_add; local_set(2);
          else_;
            local_get(5); i32_const(43); i32_eq; // '+'
            if_empty;
              local_get(2); i32_const(1); i32_add; local_set(2);
            end;
          end;

          // Exponent digit loop.
          block_empty; loop_empty;
            local_get(2); local_get(10); i32_ge_u; br_if(1);
            local_get(11); local_get(2); i32_add; i32_load8_u(0); local_set(5);
            local_get(5); i32_const(48); i32_lt_u;
            local_get(5); i32_const(57); i32_gt_u;
            i32_or;
            if_empty;
    });
    emit_float_parse_err(&mut f, emitter, invalid_err);
    wasm!(f, {
            end;
            // exp_val = exp_val*10 + digit, saturating at exp_magnitude_clamp so a
            // huge exponent can't wrap the i32 accumulator (see clamp definition).
            local_get(13); i32_const(exp_magnitude_clamp); i32_lt_u;
            if_empty;
              local_get(13); i32_const(10); i32_mul;
              local_get(5); i32_const(48); i32_sub; i32_add; local_set(13);
            end;
            local_get(14); i32_const(1); i32_add; local_set(14);
            local_get(2); i32_const(1); i32_add; local_set(2);
            br(0);
          end; end;
          // Exponent must have >= 1 digit ("1e+" -> invalid).
          local_get(14); i32_eqz;
          if_empty;
    });
    emit_float_parse_err(&mut f, emitter, invalid_err);
    wasm!(f, {
          end;
        end;
    });

    // i must now equal end — no trailing garbage.
    wasm!(f, {
        local_get(2); local_get(10); i32_lt_u;
        if_empty;
    });
    emit_float_parse_err(&mut f, emitter, invalid_err);
    wasm!(f, { end; });

    // exp10 = (exp_neg ? -exp_val : exp_val) - frac_count. Hand the significand +
    // decimal exponent to __dec2flt for the correctly-rounded f64 (Clinger
    // AlgorithmM, exact big-integer arithmetic); __dec2flt applies the sign.
    wasm!(f, {
        local_get(12);                          // exp_neg
        if_i32;
          i32_const(0); local_get(13); i32_sub; // -exp_val
        else_;
          local_get(13);                        // exp_val
        end;
        local_get(18); i32_sub;                 // - frac_count → exp10
        local_set(14);
        local_get(4); local_get(17); local_get(14); local_get(21); call(dec2flt); local_set(3);
        // Return ok(result): alloc 12 bytes [tag=0][f64]
        i32_const(12); call(emitter.rt.alloc); local_set(6);
        local_get(6); i32_const(0); i32_store(0);
        local_get(6); local_get(3); f64_store(4);
        local_get(6);
        end;
    });

    emitter.add_compiled(CompiledFunc::tracked(type_idx, f));
}

/// Push 1 if `data[i..end]` (i in `i_local`, end in `end_local`, base in
/// `base_local`) case-insensitively equals `kw`, else 0. Does not advance `i`.
/// ASCII-lowercases each scanned byte via `| 0x20`. `kw` must be lowercase ASCII.
fn emit_kw_match(f: &mut Function, kw: &[u8], i_local: u32, end_local: u32, base_local: u32) {
    // result = (end - i) == kw.len()
    wasm!(f, {
        local_get(end_local); local_get(i_local); i32_sub;
        i32_const(kw.len() as i32); i32_eq;
    });
    // AND each byte matches (lowercased). Stack holds the running i32 result.
    for (k, &c) in kw.iter().enumerate() {
        let lower = (c | 0x20) as i32;
        wasm!(f, {
            local_get(base_local); local_get(i_local); i32_add;
            i32_const(k as i32); i32_add; i32_load8_u(0);
            i32_const(0x20); i32_or;            // ASCII lowercase
            i32_const(lower); i32_eq;
            i32_and;
        });
    }
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

/// Return ok(special) for inf/nan. ORs in the f64 sign bit when is_neg (local 4)
/// is set, matching Rust's "-inf"/"-nan" bit pattern (the sign of NaN is not
/// observable through float.to_string but is kept faithful).
fn emit_float_parse_ok_special(f: &mut Function, emitter: &WasmEmitter, value: f64) {
    let pos_bits = (value.to_bits() & 0x7FFF_FFFF_FFFF_FFFF) as i64; // magnitude bits
    wasm!(f, {
        i32_const(12); call(emitter.rt.alloc); local_set(6);
        local_get(6); i32_const(0); i32_store(0);          // tag = 0 (ok)
        local_get(6);
        i64_const(pos_bits);
        local_get(4);
        if_i64;
          i64_const(0x8000000000000000_u64 as i64);
        else_;
          i64_const(0);
        end;
        i64_or;
        f64_reinterpret_i64;
        f64_store(4);
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

    // is_neg = SIGN BIT of f (catches -0.0, which compares `< 0.0` as false but
    // native renders with the sign, e.g. format!("{:.1}", -0.0) == "-0.0", and
    // format!("{:.0}", -0.4) == "-0"). Both the decimals==0 fast path and the
    // general path prepend '-' from this rather than relying on int_to_string,
    // which would drop the sign when the integer magnitude rounds to 0.
    wasm!(f, {
        local_get(0); i64_reinterpret_f64;
        i64_const(0x8000000000000000_u64 as i64); i64_and;
        i64_eqz; i32_eqz;
        local_set(13);
    });

    let minus = emitter.intern_string("-");

    // If decimals == 0: return ("-" if is_neg) + int_to_string(round_half_to_even(abs(f)))
    // Native is Rust `format!("{:.0}")`, which rounds the exact binary value
    // round-HALF-to-EVEN (banker's). `f64.nearest` IS roundTiesToEven, so it
    // matches: nearest(2.5)=2, nearest(3.5)=4, nearest(2.5 of -2.5)=2 → "-2".
    wasm!(f, {
        local_get(2); i32_eqz;
        if_empty;
          local_get(0); f64_abs; f64_nearest;
          i64_trunc_f64_s;
          call(emitter.rt.int_to_string);
          local_set(7);
          local_get(13);
          if_i32;
            i32_const(minus as i32); local_get(7);
            call(emitter.rt.concat_str);
          else_;
            local_get(7);
          end;
          return_;
        end;
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

    // scaled = round_half_to_even(abs(f) * scale)
    // Native Rust `format!("{:.N}")` rounds round-HALF-to-EVEN; `f64.nearest` is
    // exactly roundTiesToEven, so use it instead of the old round-half-up
    // `floor(x + 0.5)`. (NOTE: multiplying by `scale` first can manufacture a
    // spurious exact .5 tie that the exact-decimal native formatter does not see
    // — e.g. 0.35*10 == 3.5 exactly though 0.35's true value is 0.34999…; those
    // residuals need the deferred exact-decimal/Ryu rewrite. The corpus uses
    // values whose scaled product does not hit such a manufactured tie.)
    wasm!(f, {
        local_get(0); f64_abs;
        local_get(3); f64_mul;
        f64_nearest;
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

    // Now int_val holds the (non-negative) integer part of abs(f). Build the
    // integer string from it directly — never negate, so int_to_string can't
    // swallow the sign of values whose integer part is 0 (e.g. -0.5 has int part
    // 0 → "0", and a negate to -0 would still render "0", losing the '-'). The
    // sign is prepended explicitly at the final concat below.
    wasm!(f, {
        local_get(6);
        call(emitter.rt.int_to_string);
        local_set(7);
    });

    // Build result: int_str + "." + decimal_buf
    // First build decimal string from buf[0..count]
    let dot = emitter.intern_string(".");
    wasm!(f, {
        // Alloc decimal string: STRING_HEADER_SIZE + count bytes
        i32_const(emitter.layout_reg.header_size(super::engine::layout::STRING) as i32); local_get(9); i32_add;
        call(emitter.rt.alloc); local_set(11);
        local_get(11); local_get(9); i32_store(0); // len
        local_get(11); local_get(9); i32_store(emitter.layout_reg.fixed_offset(super::engine::layout::STRING, super::engine::layout::string::CAP) as i32 as u32, 0); // cap = len
        // Copy buf[0..count] to result+STRING_DATA_OFFSET
        i32_const(0); local_set(12);
        block_empty; loop_empty;
          local_get(12); local_get(9); i32_ge_u; br_if(1);
          local_get(11); i32_const(emitter.layout_reg.fixed_offset(super::engine::layout::STRING, super::engine::layout::string::DATA) as i32); i32_add; local_get(12); i32_add;
          local_get(8); local_get(12); i32_add; i32_load8_u(0);
          i32_store8(0);
          local_get(12); i32_const(1); i32_add; local_set(12);
          br(0);
        end; end;
    });

    // Concat: ("-" if is_neg) + int_str + "." + dec_str
    wasm!(f, {
        // body = int_str + "." + dec_str (reuse local 7 for the running result)
        local_get(7);
        i32_const(dot as i32);
        call(emitter.rt.concat_str);
        local_get(11);
        call(emitter.rt.concat_str);
        local_set(7);
        // Prepend '-' iff the input's sign bit was set.
        local_get(13);
        if_i32;
          i32_const(minus as i32);
          local_get(7);
          call(emitter.rt.concat_str);
        else_;
          local_get(7);
        end;
        end;
    });

    emitter.add_compiled(CompiledFunc::tracked(type_idx, f));
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

    emitter.add_compiled(CompiledFunc::tracked(type_idx, f));
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
    emitter.add_compiled(CompiledFunc::tracked(type_idx, f));
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
    emitter.add_compiled(CompiledFunc::tracked(type_idx, f));
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
    emitter.add_compiled(CompiledFunc::tracked(type_idx, f));
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

    emitter.add_compiled(CompiledFunc::tracked(type_idx, f));
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

    emitter.add_compiled(CompiledFunc::tracked(type_idx, f));
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

    emitter.add_compiled(CompiledFunc::tracked(type_idx, f));
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

    // Saturate: above ~709 the result is +Inf in IEEE-754; below ~-745 it is 0.
    // Without this clamp the Taylor path's `i64.trunc_f64_s` traps on huge or
    // NaN inputs (encoder softmax can momentarily push scores way out of range).
    wasm!(f, {
        // x != x  →  NaN check (NaN compares unequal to itself)
        local_get(0); local_get(0); f64_ne;
        if_empty;
          local_get(0); return_;  // propagate NaN unchanged
        end;
        local_get(0); f64_const(709.78); f64_gt;
        if_empty;
          f64_const(f64::INFINITY); return_;
        end;
        local_get(0); f64_const(-745.13); f64_lt;
        if_empty;
          f64_const(0.0); return_;
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

    emitter.add_compiled(CompiledFunc::tracked(type_idx, f));
}
