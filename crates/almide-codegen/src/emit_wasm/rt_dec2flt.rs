//! Correctly-rounded decimal → f64 for `float.parse` / `string.to_float` (WASM).
//!
//! Native is `s.trim().parse::<f64>()` — IEEE-754 round-to-nearest-even. The old
//! WASM parser accumulated the mantissa in `f64` (`acc*10 + digit`, then `*10^exp`),
//! which is NOT correctly rounded: `"0.3"` → `0.30000000000000004`, long mantissas
//! drift, subnormals (`5e-324`) collapse to `0`. With the shortest-round-trip
//! `to_string` (Dragon4) these errors became observable.
//!
//! This implements Clinger's AlgorithmM with exact big-integer arithmetic
//! (reusing the `__dragon_*` bignum helpers): the value `f · 10^e` is held as a
//! fraction `num/den` of binary bignums; the 53-bit mantissa is extracted by
//! bit-by-bit long division (shift/compare/subtract — no bignum division needed),
//! rounded half-to-even, and assembled into the f64 bit pattern directly
//! (handling subnormals and overflow→inf). Proven byte-identical to
//! `str::parse::<f64>()` over 2.55M fuzzed + boundary inputs.

use super::{CompiledFunc, WasmEmitter};
use super::rt_dragon::{BN_HDR, BN_STRIDE, NLIMBS};
use wasm_encoder::ValType;
use super::TrackedFunction as Function;

// ── IEEE-754 binary64 parameters (so the bit math below reads as IEEE-754, not
//    as inscrutable powers of two) ──
/// Significand precision in bits, including the implicit leading 1.
const MANT_BITS: i32 = 53;
/// Width of the STORED mantissa field (precision minus the implicit bit).
const MANT_FIELD_BITS: i32 = MANT_BITS - 1; // 52
/// Exponent bias.
const EXP_BIAS: i32 = 1023;
/// Unbiased exponent of the smallest normal value (`1.0 · 2^MIN_NORMAL_EXP`).
const MIN_NORMAL_EXP: i32 = -1022;
/// Biased exponent field of ±inf / NaN.
const INF_EXP_FIELD: i32 = 2047;
/// Bit position of the sign.
const SIGN_BIT: i64 = 63;
/// Binary exponent (`shift`, value = m·2^shift) of the smallest subnormal:
/// `MIN_NORMAL_EXP - MANT_FIELD_BITS`.
const MIN_SUBNORMAL_SHIFT: i32 = MIN_NORMAL_EXP - MANT_FIELD_BITS; // -1074
/// `shift + EXP_FIELD_BIAS` is the biased exponent field of a normal.
const EXP_FIELD_BIAS: i32 = EXP_BIAS + MANT_FIELD_BITS; // 1075
/// `2^MANT_FIELD_BITS` — the implicit high bit of a normal significand.
const SIGNIFICAND_MSB: i64 = 1i64 << MANT_FIELD_BITS; // 2^52
/// `2^MANT_BITS` — one past the largest representable significand.
const SIGNIFICAND_LIMIT: i64 = 1i64 << MANT_BITS; // 2^53
/// Mask of the stored mantissa field.
const MANTISSA_MASK: i64 = SIGNIFICAND_MSB - 1; // 2^52 - 1
/// Bit pattern of +inf (`INF_EXP_FIELD` in the exponent, zero mantissa).
const INF_BITS: i64 = (INF_EXP_FIELD as i64) << MANT_FIELD_BITS; // 0x7FF0_0000_0000_0000
/// Extra low bits extracted past the significand to drive round-to-nearest-even.
const ROUND_GUARD_BITS: i32 = 2;

// ── bignum + parsing parameters ──
/// Base of the decimal mantissa/exponent.
pub(super) const DECIMAL_BASE: i32 = 10;
/// Bits per bignum limb.
const LIMB_BITS: i32 = 32;
/// Bytes per bignum limb.
const LIMB_BYTES: i32 = LIMB_BITS / 8; // 4
/// Short-circuit scaling once a bignum nears the `NLIMBS·LIMB_BITS`-bit capacity
/// (the value is then certainly inf or 0). Leaves headroom for the AlgorithmM
/// shift that follows.
const BN_OVERFLOW_GUARD_BITS: i32 = (NLIMBS as i32) * LIMB_BITS - 196; // 3900
/// Significant digits accumulated before truncating (leading zeros excluded).
/// Matches Rust `dec2flt`'s `Decimal::MAX_DIGITS`: 768 significant digits is the
/// proven bound past which every further digit can only break an exact half-way
/// tie — handled by the `sticky` flag — so keeping this many is correctly-rounded
/// for ALL inputs, including the worst-case ~767-digit subnormal ties. Comfortably
/// within `NLIMBS` even after the AlgorithmM shift (~80 + ~36 limbs < 128).
pub(super) const SIG_DIGIT_CAP: i32 = 768;
/// Scratch bignums used by AlgorithmM (num, den, u, d, shifted, two_rem).
const SCRATCH_SLOTS: i32 = 7;
// AlgorithmM scratch slot indices.
const SLOT_NUM: i32 = 0;
const SLOT_DEN: i32 = 1;
const SLOT_U: i32 = 2;
const SLOT_D: i32 = 3;
const SLOT_SHIFTED: i32 = 4;
const SLOT_TWOREM: i32 = 5;

/// Function indices for the decimal→f64 parser.
#[derive(Default)]
pub struct DecFloatRuntime {
    /// `__dec2flt(neg, sig_ptr, exp10, sticky) -> f64` — value = (-1)^neg · sig ·
    /// 10^exp10, where `sticky` is 1 iff the significand was truncated and any
    /// dropped digit was non-zero (breaks a half-way tie upward).
    pub dec2flt: u32,
    /// `__bn_add_small(p, a)` — bignum `p += a` (a: u32). Builds the significand.
    pub bn_add_small: u32,
    /// `__bn_bit_len(p) -> i32` — number of significant bits of bignum `p`.
    pub bn_bit_len: u32,
}

pub fn register(emitter: &mut WasmEmitter) {
    let pp_void = emitter.register_type(vec![ValType::I32, ValType::I32], vec![]);
    let p_i32 = emitter.register_type(vec![ValType::I32], vec![ValType::I32]);
    let p4_f64 = emitter.register_type(
        vec![ValType::I32, ValType::I32, ValType::I32, ValType::I32],
        vec![ValType::F64],
    );
    emitter.rt.decfloat.bn_add_small = emitter.register_func("__bn_add_small", pp_void);
    emitter.rt.decfloat.bn_bit_len = emitter.register_func("__bn_bit_len", p_i32);
    emitter.rt.decfloat.dec2flt = emitter.register_func("__dec2flt", p4_f64);
}

/// Compile the decimal→f64 bodies. Registered + compiled LATE (after the Dragon4
/// helpers it depends on), mirroring `rt_dragon::compile_helpers`.
pub fn compile_helpers(emitter: &mut WasmEmitter) {
    compile_bn_add_small(emitter);
    compile_bn_bit_len(emitter);
    compile_dec2flt(emitter);
}

/// Emit `return (-1)^neg · 0.0` reading `neg` from `neg_local`.
fn emit_signed_zero_return(f: &mut Function, neg_local: u32) {
    wasm!(f, {
        local_get(neg_local); if_f64; f64_const(-0.0); else_; f64_const(0.0); end;
        return_;
    });
}

/// Set the sign bit of the f64 in `bits_local` when `neg_local` is non-zero.
fn emit_apply_sign(f: &mut Function, bits_local: u32, neg_local: u32) {
    wasm!(f, {
        local_get(neg_local);
        if_empty;
          local_get(bits_local); i64_const(1); i64_const(SIGN_BIT); i64_shl; i64_or; local_set(bits_local);
        end;
    });
}

/// Emit `return (-1)^neg · inf`, using `bits_local` as scratch.
fn emit_signed_inf_return(f: &mut Function, bits_local: u32, neg_local: u32) {
    wasm!(f, { i64_const(INF_BITS); local_set(bits_local); });
    emit_apply_sign(f, bits_local, neg_local);
    wasm!(f, { local_get(bits_local); f64_reinterpret_i64; return_; });
}

/// `__bn_add_small(p, a)`: p += a (a: u32). Carry-propagates; appends a limb if needed.
fn compile_bn_add_small(emitter: &mut WasmEmitter) {
    let type_idx = emitter.func_type_indices[&emitter.rt.decfloat.bn_add_small];
    // params: 0=p, 1=a | locals: 2=len, 3=i, 4=carry, 5=addr, 6=limb, 7=sum
    let mut f = Function::new([(6, ValType::I32)]);
    wasm!(f, {
        local_get(0); i32_load(0); local_set(2);   // len
        i32_const(0); local_set(3);                 // i
        local_get(1); local_set(4);                 // carry = a
        block_empty; loop_empty;
          local_get(4); i32_eqz; br_if(1);          // carry == 0 → done
          local_get(3); local_get(2); i32_ge_u; br_if(1); // i >= len → extend below
          local_get(0); i32_const(BN_HDR as i32); i32_add; local_get(3); i32_const(LIMB_BYTES); i32_mul; i32_add; local_set(5); // &limb[i]
          local_get(5); i32_load(0); local_set(6);  // limb
          local_get(6); local_get(4); i32_add; local_set(7); // sum (wraps)
          local_get(5); local_get(7); i32_store(0);
          local_get(7); local_get(6); i32_lt_u; local_set(4); // carry = unsigned overflow
          local_get(3); i32_const(1); i32_add; local_set(3);
          br(0);
        end; end;
        // leftover carry → append a new limb
        local_get(4); i32_eqz;
        if_empty;
        else_;
          local_get(0); i32_const(BN_HDR as i32); i32_add; local_get(2); i32_const(LIMB_BYTES); i32_mul; i32_add;
          local_get(4); i32_store(0);
          local_get(0); local_get(2); i32_const(1); i32_add; i32_store(0);
        end;
        end;
    });
    emitter.add_compiled(CompiledFunc::tracked_for(emitter.rt.decfloat.bn_add_small, type_idx, f));
}

/// `__bn_bit_len(p) -> i32`: significant bit count (0 for a zero bignum).
fn compile_bn_bit_len(emitter: &mut WasmEmitter) {
    let type_idx = emitter.func_type_indices[&emitter.rt.decfloat.bn_bit_len];
    // params: 0=p | locals: 1=i, 2=limb
    let mut f = Function::new([(2, ValType::I32)]);
    wasm!(f, {
        local_get(0); i32_load(0); local_set(1);    // i = len
        block_empty; loop_empty;
          local_get(1); i32_eqz; br_if(1);          // i == 0 → 0
          local_get(1); i32_const(1); i32_sub; local_set(1); // i--
          local_get(0); i32_const(BN_HDR as i32); i32_add; local_get(1); i32_const(LIMB_BYTES); i32_mul; i32_add;
          i32_load(0); local_set(2);                // limb
          local_get(2); i32_eqz;
          if_empty;
          else_;
            // i*LIMB_BITS + (LIMB_BITS - clz(limb))  — bit index of the top set bit
            local_get(1); i32_const(LIMB_BITS); i32_mul;
            i32_const(LIMB_BITS); local_get(2); i32_clz; i32_sub;
            i32_add;
            return_;
          end;
          br(0);
        end; end;
        i32_const(0);
        end;
    });
    emitter.add_compiled(CompiledFunc::tracked_for(emitter.rt.decfloat.bn_bit_len, type_idx, f));
}

/// `__dec2flt(neg, sig_ptr, exp10) -> f64`. Clinger AlgorithmM.
fn compile_dec2flt(emitter: &mut WasmEmitter) {
    let type_idx = emitter.func_type_indices[&emitter.rt.decfloat.dec2flt];
    // params: 0=neg, 1=sig_ptr, 2=exp10, 3=sticky
    // i32 locals: base, NUM, DEN, U, D, SHIFTED, TWOREM, e2, prec, shift, maxbits,
    //             bit, e, c, exp_field | i64 locals: m, bits, (reserved)
    let mut f = Function::new([(15, ValType::I32), (3, ValType::I64)]);
    const NEG: u32 = 0;
    const SIG: u32 = 1;
    const EXP10: u32 = 2;
    const STICKY: u32 = 3;
    const BASE: u32 = 4;
    const NUM: u32 = 5;
    const DEN: u32 = 6;
    const U: u32 = 7;
    const D: u32 = 8;
    const SHIFTED: u32 = 9;
    const TWOREM: u32 = 10;
    const E2: u32 = 11;
    const PREC: u32 = 12;
    const SHIFT: u32 = 13;
    const MAXBITS: u32 = 14;
    const BIT: u32 = 15;
    const E: u32 = 16;
    const C: u32 = 17;
    const EXP_FIELD: u32 = 18;
    const M: u32 = 19;
    const BITS: u32 = 20;

    let d_mul = emitter.rt.dragon.mul_small;
    let d_cmp = emitter.rt.dragon.cmp;
    let d_sub = emitter.rt.dragon.sub;
    let d_shl = emitter.rt.dragon.shl;
    let d_copy = emitter.rt.dragon.copy;
    let bn_blen = emitter.rt.decfloat.bn_bit_len;
    let alloc = emitter.rt.alloc;
    let stride = BN_STRIDE as i32;
    let hdr = BN_HDR as i32;
    let slot = |i: i32| stride * i;

    // scratch = alloc(SCRATCH_SLOTS · BN_STRIDE); compute slot pointers.
    wasm!(f, {
        i32_const(stride * SCRATCH_SLOTS); call(alloc); local_set(BASE);
        local_get(BASE); i32_const(slot(SLOT_NUM)); i32_add; local_set(NUM);
        local_get(BASE); i32_const(slot(SLOT_DEN)); i32_add; local_set(DEN);
        local_get(BASE); i32_const(slot(SLOT_U)); i32_add; local_set(U);
        local_get(BASE); i32_const(slot(SLOT_D)); i32_add; local_set(D);
        local_get(BASE); i32_const(slot(SLOT_SHIFTED)); i32_add; local_set(SHIFTED);
        local_get(BASE); i32_const(slot(SLOT_TWOREM)); i32_add; local_set(TWOREM);
    });

    // Zero significand → ±0.
    wasm!(f, {
        local_get(SIG); i32_load(0); i32_const(1); i32_eq;
        local_get(SIG); i32_const(hdr); i32_add; i32_load(0); i32_eqz;
        i32_and;
        if_empty;
    });
    emit_signed_zero_return(&mut f, NEG);
    wasm!(f, { end; });

    // num = sig ; den = 1.
    wasm!(f, {
        local_get(NUM); local_get(SIG); call(d_copy);
        local_get(DEN); i32_const(1); i32_store(0);
        local_get(DEN); i32_const(hdr); i32_add; i32_const(1); i32_store(0);
    });

    // Scale: exp10 >= 0 → num *= 10^exp10 ; else den *= 10^(-exp10) (×10, |exp10| times).
    // Guard each step against bignum overflow: if num would exceed the cap the value
    // is astronomically large → ±inf; if den would, it is astronomically small → ±0.
    // This also bounds huge exponents ("1e999999" → inf) without an unbounded loop.
    wasm!(f, {
        local_get(EXP10); i32_const(0); i32_ge_s;
        if_empty;
          local_get(EXP10); local_set(E);
          block_empty; loop_empty;
            local_get(E); i32_eqz; br_if(1);
            local_get(NUM); call(bn_blen); i32_const(BN_OVERFLOW_GUARD_BITS); i32_gt_s;
            if_empty;
    });
    emit_signed_inf_return(&mut f, BITS, NEG);
    wasm!(f, {
            end;
            local_get(NUM); i32_const(DECIMAL_BASE); call(d_mul);
            local_get(E); i32_const(1); i32_sub; local_set(E);
            br(0);
          end; end;
        else_;
          i32_const(0); local_get(EXP10); i32_sub; local_set(E);
          block_empty; loop_empty;
            local_get(E); i32_eqz; br_if(1);
            local_get(DEN); call(bn_blen); i32_const(BN_OVERFLOW_GUARD_BITS); i32_gt_s;
            if_empty;
    });
    emit_signed_zero_return(&mut f, NEG);
    wasm!(f, {
            end;
            local_get(DEN); i32_const(DECIMAL_BASE); call(d_mul);
            local_get(E); i32_const(1); i32_sub; local_set(E);
            br(0);
          end; end;
        end;
    });

    // e2 = bit_len(num) - bit_len(den).
    wasm!(f, {
        local_get(NUM); call(bn_blen);
        local_get(DEN); call(bn_blen);
        i32_sub; local_set(E2);
    });

    // ── AlgorithmM outer loop ──
    wasm!(f, {
        block_empty; loop_empty;
          // prec = MANT_BITS ; shift = e2 - MANT_FIELD_BITS
          i32_const(MANT_BITS); local_set(PREC);
          local_get(E2); i32_const(MANT_FIELD_BITS); i32_sub; local_set(SHIFT);
          // Subnormal: if shift < MIN_SUBNORMAL_SHIFT, pin shift and shrink precision.
          local_get(SHIFT); i32_const(MIN_SUBNORMAL_SHIFT); i32_lt_s;
          if_empty;
            i32_const(MANT_BITS);
            i32_const(MIN_SUBNORMAL_SHIFT); local_get(SHIFT); i32_sub;   // (MIN_SUBNORMAL_SHIFT - shift)
            i32_sub; local_set(PREC);
            i32_const(MIN_SUBNORMAL_SHIFT); local_set(SHIFT);
            local_get(PREC); i32_const(0); i32_le_s;
            if_empty; i32_const(0); local_set(PREC); end;
          end;
          // u = num ; d = den ; scale by 2^shift
          local_get(U); local_get(NUM); call(d_copy);
          local_get(D); local_get(DEN); call(d_copy);
          local_get(SHIFT); i32_const(0); i32_ge_s;
          if_empty;
            local_get(D); local_get(SHIFT); call(d_shl);          // d <<= shift
          else_;
            local_get(U); i32_const(0); local_get(SHIFT); i32_sub; call(d_shl); // u <<= -shift
          end;
          // maxbits = max(prec + ROUND_GUARD_BITS, 1)
          local_get(PREC); i32_const(ROUND_GUARD_BITS); i32_add; local_set(MAXBITS);
          local_get(MAXBITS); i32_const(1); i32_lt_s;
          if_empty; i32_const(1); local_set(MAXBITS); end;
          // m = 0 ; bit-by-bit long division (U holds the remainder)
          i64_const(0); local_set(M);
          local_get(MAXBITS); local_set(BIT);
          block_empty; loop_empty;
            local_get(BIT); i32_eqz; br_if(1);
            local_get(BIT); i32_const(1); i32_sub; local_set(BIT);
            local_get(SHIFTED); local_get(D); call(d_copy);          // shifted = d
            local_get(SHIFTED); local_get(BIT); call(d_shl);         // shifted <<= bit
            local_get(U); local_get(SHIFTED); call(d_cmp); i32_const(0); i32_ge_s; // u >= shifted ?
            if_empty;
              local_get(U); local_get(SHIFTED); call(d_sub);         // u -= shifted
              local_get(M); i64_const(1); local_get(BIT); i64_extend_i32_u; i64_shl; i64_or; local_set(M); // m |= 1<<bit
            end;
            br(0);
          end; end;
          // width correction for normals: keep m in [2^(MANT_BITS-1), 2^MANT_BITS)
          local_get(PREC); i32_const(MANT_BITS); i32_eq;
          if_empty;
            local_get(M); i64_const(SIGNIFICAND_LIMIT); i64_ge_u;            // m >= 2^53
            if_empty; local_get(E2); i32_const(1); i32_add; local_set(E2); br(2); end;
            local_get(M); i64_const(SIGNIFICAND_MSB); i64_lt_u;             // m < 2^52
            local_get(E2); i32_const(MIN_NORMAL_EXP); i32_gt_s; i32_and;
            if_empty; local_get(E2); i32_const(1); i32_sub; local_set(E2); br(2); end;
          end;
          // round half-even: two_rem = U<<1. Round up iff 2·rem > den, or 2·rem ==
          // den AND (the dropped-digit sticky bit is set OR m is odd). The sticky
          // bit breaks an otherwise-exact half-way tie upward (truncated input).
          local_get(TWOREM); local_get(U); call(d_copy);
          local_get(TWOREM); i32_const(1); call(d_shl);
          local_get(TWOREM); local_get(D); call(d_cmp); local_set(C);
          local_get(C); i32_const(0); i32_gt_s;          // 2·rem > den
          local_get(C); i32_eqz;                          // 2·rem == den (tie)
          local_get(STICKY);                              // sticky ...
          local_get(M); i64_const(1); i64_and; i64_const(1); i64_eq;  // ... OR m odd
          i32_or;
          i32_and;
          i32_or;
          if_empty; local_get(M); i64_const(1); i64_add; local_set(M); end;
          // m == 2^MANT_BITS after rounding (normal) → renormalize
          local_get(PREC); i32_const(MANT_BITS); i32_eq;
          local_get(M); i64_const(SIGNIFICAND_LIMIT); i64_eq;
          i32_and;
          if_empty;
            local_get(M); i64_const(1); i64_shr_u; local_set(M);
            local_get(E2); i32_const(1); i32_add; local_set(E2);
            local_get(SHIFT); i32_const(1); i32_add; local_set(SHIFT);
          end;
          // ── assemble f64 from (m, shift) ──
          // m == 0 → ±0
          local_get(M); i64_eqz;
          if_empty;
    });
    emit_signed_zero_return(&mut f, NEG);
    wasm!(f, {
          end;
          // bits = (m >= 2^MANT_FIELD_BITS) ? normal : subnormal
          local_get(M); i64_const(SIGNIFICAND_MSB); i64_ge_u;
          if_empty;
            // normal: exp_field = shift + EXP_FIELD_BIAS
            local_get(SHIFT); i32_const(EXP_FIELD_BIAS); i32_add; local_set(EXP_FIELD);
            local_get(EXP_FIELD); i32_const(INF_EXP_FIELD); i32_ge_s;
            if_empty;
    });
    emit_signed_inf_return(&mut f, BITS, NEG);
    wasm!(f, {
            end;
            local_get(EXP_FIELD); i64_extend_i32_u; i64_const(MANT_FIELD_BITS as i64); i64_shl;
            local_get(M); i64_const(MANTISSA_MASK); i64_and;
            i64_or; local_set(BITS);
          else_;
            // subnormal: exponent field is 0, the whole value is in the mantissa.
            local_get(M); local_set(BITS);
          end;
    });
    emit_apply_sign(&mut f, BITS, NEG);
    wasm!(f, {
          local_get(BITS); f64_reinterpret_i64; return_;
        end; end;
        // unreachable (the loop always returns)
        f64_const(0.0);
        end;
    });

    emitter.add_compiled(CompiledFunc::tracked_for(emitter.rt.decfloat.dec2flt, type_idx, f));
}
