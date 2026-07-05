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

    emitter.add_compiled(CompiledFunc::tracked_for(emitter.rt.int_from_hex, type_idx, f));
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

    emitter.add_compiled(CompiledFunc::tracked_for(emitter.rt.float_parse, type_idx, f));
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

/// __float_to_fixed(f: f64, decimals: i64) -> i32 (String ptr).
/// Delegates to the Dragon4-based exact fixed-precision formatter in
/// `rt_dragon::compile_float_to_fixed`, which reproduces `format!("{:.N}", f)`
/// byte-for-byte (exact binary value, round-half-to-even, no 10^N i64 overflow).
pub(super) fn compile_float_to_fixed(emitter: &mut WasmEmitter) {
    super::rt_dragon::compile_float_to_fixed(emitter);
}

/// __float_pow(base: f64, exp: f64) -> f64 — float exponentiation (`**`, math.fpow).
/// Delegates to the vendored musl-libm `__libm_pow` (e_pow.c), which handles ALL
/// special cases exactly (0/inf/nan, negative base with odd/even integer exponent,
/// |y| huge). This replaces the old integer-binexp + fractional-Taylor approximation
/// that was wrong in several ways the sweep caught: fpow(-2.0, 0.5) returned a real
/// value via abs(base) instead of NaN; fpow(0.0, -1.0) returned 0 instead of inf;
/// fpow(2.0, inf) TRAPPED (exit 134) on the i64-trunc of inf. Bit-identical native↔wasm.
pub(super) fn compile_float_pow(emitter: &mut WasmEmitter) {
    let type_idx = emitter.func_type_indices[&emitter.rt.float_pow];
    let libm_pow = emitter.rt.libm.pow;
    let mut f = Function::new([]);
    wasm!(f, {
        local_get(0); local_get(1); call(libm_pow);
        end;
    });
    emitter.add_compiled(CompiledFunc::tracked_for(emitter.rt.float_pow, type_idx, f));
}

/// __math_sin(x: f64) -> f64
/// Faithful port of libm `sin` (vendored, see emit_wasm/rt_libm.rs +
/// runtime/rs/src/libm.rs). Small-arg fast path, inf/NaN → x-x, else
/// `__libm_rem_pio2` argument reduction + the appropriate kernel by n&3. Result
/// is bit-identical to the native vendored `sin`.
pub(super) fn compile_math_sin(emitter: &mut WasmEmitter) {
    let type_idx = emitter.func_type_indices[&emitter.rt.math_sin];
    let alloc = emitter.rt.alloc;
    let rem_pio2 = emitter.rt.libm.rem_pio2;
    let k_sin = emitter.rt.libm.k_sin;
    let k_cos = emitter.rt.libm.k_cos;
    // params: 0=x. locals: 1=i32 ix, 2=i32 n, 3=i32 yp, 4=f64 y0, 5=f64 y1
    let mut f = Function::new([(2, ValType::I32), (1, ValType::I32), (2, ValType::F64)]);
    wasm!(f, {
        // ix = (to_bits(x) >> 32) & 0x7fffffff
        local_get(0); i64_reinterpret_f64; i64_const(32); i64_shr_u; i32_wrap_i64; i32_const(0x7fffffff); i32_and; local_set(1);
        // if ix <= 0x3fe921fb { if ix < 0x3e500000 { return x } return k_sin(x,0,0) }
        local_get(1); i32_const(0x3fe921fb); i32_le_u;
        if_empty;
            local_get(1); i32_const(0x3e500000); i32_lt_u;
            if_empty; local_get(0); return_; end;
            local_get(0); f64_const(0.0); i32_const(0); call(k_sin); return_;
        end;
        // if ix >= 0x7ff00000 { return x - x }
        local_get(1); i32_const(0x7ff00000); i32_ge_u;
        if_empty; local_get(0); local_get(0); f64_sub; return_; end;
        // n = rem_pio2(x, yp); y0=y[0]; y1=y[1]
        i32_const(16); call(alloc); local_set(3);
        local_get(0); local_get(3); call(rem_pio2); local_set(2);
        local_get(3); f64_load(0); local_set(4);
        local_get(3); f64_load(8); local_set(5);
        // match n & 3 { 0=>k_sin(y0,y1,1) 1=>k_cos(y0,y1) 2=>-k_sin(y0,y1,1) _=>-k_cos(y0,y1) }
        local_get(2); i32_const(3); i32_and; i32_const(0); i32_eq;
        if_f64;
            local_get(4); local_get(5); i32_const(1); call(k_sin);
        else_;
            local_get(2); i32_const(3); i32_and; i32_const(1); i32_eq;
            if_f64;
                local_get(4); local_get(5); call(k_cos);
            else_;
                local_get(2); i32_const(3); i32_and; i32_const(2); i32_eq;
                if_f64;
                    local_get(4); local_get(5); i32_const(1); call(k_sin); f64_neg;
                else_;
                    local_get(4); local_get(5); call(k_cos); f64_neg;
                end;
            end;
        end;
        end;
    });
    emitter.add_compiled(CompiledFunc::tracked_for(emitter.rt.math_sin, type_idx, f));
}

/// __math_cos(x: f64) -> f64
/// Faithful port of libm `cos` (vendored). Bit-identical to native vendored `cos`.
pub(super) fn compile_math_cos(emitter: &mut WasmEmitter) {
    let type_idx = emitter.func_type_indices[&emitter.rt.math_cos];
    let alloc = emitter.rt.alloc;
    let rem_pio2 = emitter.rt.libm.rem_pio2;
    let k_sin = emitter.rt.libm.k_sin;
    let k_cos = emitter.rt.libm.k_cos;
    // params: 0=x. locals: 1=i32 ix, 2=i32 n, 3=i32 yp, 4=f64 y0, 5=f64 y1
    let mut f = Function::new([(2, ValType::I32), (1, ValType::I32), (2, ValType::F64)]);
    wasm!(f, {
        local_get(0); i64_reinterpret_f64; i64_const(32); i64_shr_u; i32_wrap_i64; i32_const(0x7fffffff); i32_and; local_set(1);
        // if ix <= 0x3fe921fb { if ix < 0x3e46a09e { if (x as i32)==0 { return 1.0 } } return k_cos(x,0) }
        local_get(1); i32_const(0x3fe921fb); i32_le_u;
        if_empty;
            local_get(1); i32_const(0x3e46a09e); i32_lt_u;
            if_empty;
                local_get(0); i32_trunc_f64_s; i32_eqz;
                if_empty; f64_const(1.0); return_; end;
            end;
            local_get(0); f64_const(0.0); call(k_cos); return_;
        end;
        local_get(1); i32_const(0x7ff00000); i32_ge_u;
        if_empty; local_get(0); local_get(0); f64_sub; return_; end;
        i32_const(16); call(alloc); local_set(3);
        local_get(0); local_get(3); call(rem_pio2); local_set(2);
        local_get(3); f64_load(0); local_set(4);
        local_get(3); f64_load(8); local_set(5);
        // match n & 3 { 0=>k_cos 1=>-k_sin 2=>-k_cos _=>k_sin }
        local_get(2); i32_const(3); i32_and; i32_const(0); i32_eq;
        if_f64;
            local_get(4); local_get(5); call(k_cos);
        else_;
            local_get(2); i32_const(3); i32_and; i32_const(1); i32_eq;
            if_f64;
                local_get(4); local_get(5); i32_const(1); call(k_sin); f64_neg;
            else_;
                local_get(2); i32_const(3); i32_and; i32_const(2); i32_eq;
                if_f64;
                    local_get(4); local_get(5); call(k_cos); f64_neg;
                else_;
                    local_get(4); local_get(5); i32_const(1); call(k_sin);
                end;
            end;
        end;
        end;
    });
    emitter.add_compiled(CompiledFunc::tracked_for(emitter.rt.math_cos, type_idx, f));
}

/// __math_tan(x: f64) -> f64
/// Faithful port of libm `tan` (vendored). Bit-identical to native vendored `tan`.
pub(super) fn compile_math_tan(emitter: &mut WasmEmitter) {
    let type_idx = emitter.func_type_indices[&emitter.rt.math_tan];
    let alloc = emitter.rt.alloc;
    let rem_pio2 = emitter.rt.libm.rem_pio2;
    let k_tan = emitter.rt.libm.k_tan;
    // params: 0=x. locals: 1=i32 ix, 2=i32 n, 3=i32 yp, 4=f64 y0, 5=f64 y1
    let mut f = Function::new([(2, ValType::I32), (1, ValType::I32), (2, ValType::F64)]);
    wasm!(f, {
        local_get(0); i64_reinterpret_f64; i64_const(32); i64_shr_u; i32_wrap_i64; i32_const(0x7fffffff); i32_and; local_set(1);
        // if ix <= 0x3fe921fb { if ix < 0x3e400000 { return x } return k_tan(x,0,0) }
        local_get(1); i32_const(0x3fe921fb); i32_le_u;
        if_empty;
            local_get(1); i32_const(0x3e400000); i32_lt_u;
            if_empty; local_get(0); return_; end;
            local_get(0); f64_const(0.0); i32_const(0); call(k_tan); return_;
        end;
        local_get(1); i32_const(0x7ff00000); i32_ge_u;
        if_empty; local_get(0); local_get(0); f64_sub; return_; end;
        i32_const(16); call(alloc); local_set(3);
        local_get(0); local_get(3); call(rem_pio2); local_set(2);
        local_get(3); f64_load(0); local_set(4);
        local_get(3); f64_load(8); local_set(5);
        // k_tan(y0, y1, n & 1)
        local_get(4); local_get(5); local_get(2); i32_const(1); i32_and; call(k_tan);
        end;
    });
    emitter.add_compiled(CompiledFunc::tracked_for(emitter.rt.math_tan, type_idx, f));
}

/// __math_log(x: f64) -> f64 — natural logarithm.
/// Delegates to the vendored musl-libm `__libm_log` (e_log.c) so the result is
/// bit-identical native↔wasm AND deterministic across platforms (the StrictMath
/// decision). The old sqrt-reduction + Taylor approximation it replaced was not
/// correctly rounded and disagreed with native `f64::ln` in the last ULP; it also
/// returned NaN for log(0.0) where IEEE-754 wants -inf. See emit_wasm/rt_libm.rs.
pub(super) fn compile_math_log(emitter: &mut WasmEmitter) {
    let type_idx = emitter.func_type_indices[&emitter.rt.math_log];
    let libm_log = emitter.rt.libm.log;
    let mut f = Function::new([]);
    wasm!(f, {
        local_get(0); call(libm_log);
        end;
    });
    emitter.add_compiled(CompiledFunc::tracked_for(emitter.rt.math_log, type_idx, f));
}

/// __math_log10(x: f64) -> f64 — common logarithm.
/// Delegates to the vendored musl-libm `__libm_log10` (e_log10.c). Exact powers
/// of 10 come out exact by the extra-precision split, so the old `ln(x)/ln(10)` +
/// rounding-fudge heuristic is gone; result is bit-identical native↔wasm.
pub(super) fn compile_math_log10(emitter: &mut WasmEmitter) {
    let type_idx = emitter.func_type_indices[&emitter.rt.math_log10];
    let libm_log10 = emitter.rt.libm.log10;
    let mut f = Function::new([]);
    wasm!(f, {
        local_get(0); call(libm_log10);
        end;
    });
    emitter.add_compiled(CompiledFunc::tracked_for(emitter.rt.math_log10, type_idx, f));
}

/// __math_log2(x: f64) -> f64 — binary logarithm.
/// Delegates to the vendored musl-libm `__libm_log2` (e_log2.c). Bit-identical
/// native↔wasm; exact powers of 2 are exact via the extra-precision split.
pub(super) fn compile_math_log2(emitter: &mut WasmEmitter) {
    let type_idx = emitter.func_type_indices[&emitter.rt.math_log2];
    let libm_log2 = emitter.rt.libm.log2;
    let mut f = Function::new([]);
    wasm!(f, {
        local_get(0); call(libm_log2);
        end;
    });
    emitter.add_compiled(CompiledFunc::tracked_for(emitter.rt.math_log2, type_idx, f));
}

/// __math_exp(x: f64) -> f64 — e^x.
/// Delegates to the vendored musl-libm `__libm_exp` (e_exp.c). Bit-identical
/// native↔wasm AND deterministic across platforms. The old k*ln2 reduction +
/// 20-term Taylor approximation it replaced was not correctly rounded and clamped
/// the subnormal underflow band too early (exp(-745) → 0 instead of 5e-324);
/// the vendored fn handles overflow → inf / underflow → subnormal exactly.
pub(super) fn compile_math_exp(emitter: &mut WasmEmitter) {
    let type_idx = emitter.func_type_indices[&emitter.rt.math_exp];
    let libm_exp = emitter.rt.libm.exp;
    let mut f = Function::new([]);
    wasm!(f, {
        local_get(0); call(libm_exp);
        end;
    });
    emitter.add_compiled(CompiledFunc::tracked_for(emitter.rt.math_exp, type_idx, f));
}
