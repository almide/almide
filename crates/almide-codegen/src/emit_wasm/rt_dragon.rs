//! Dragon4 shortest-decimal conversion for `float.to_string` (WASM target).
//!
//! Produces the SAME bytes as the native oracle
//! (`runtime/rs/src/float.rs::almide_rt_float_to_string`), which is Rust's
//! `f64` `Display` (shortest round-tripping decimal, fixed notation, never
//! scientific) plus a `.0` suffix for finite integer-valued floats.
//!
//! The previous implementation used `i64_trunc_f64_s` on the integer part,
//! which (a) TRAPPED for `|x| >= 2^63` and (b) emitted up to 15 naively
//! scaled fractional digits that were not the shortest round-tripping form
//! (`0.1*3` → `0.3`, `123.456` → `123.456000000000003`).
//!
//! This module instead implements the Steele & White / Burger–Dubois
//! "Dragon4" free-format algorithm with exact big-integer arithmetic. The
//! float is decomposed to `|x| = f · 2^e` (exact), and digits are generated
//! from the unique shortest decimal in the rounding interval. The tie-break
//! at an exact decimal midpoint rounds the final digit UP (away from zero on
//! the magnitude), matching Rust's flt2dec — this was validated byte-for-byte
//! against `format!("{}", x)` over 20M random + boundary f64 values.
//!
//! Big integers live in a heap block allocated per call: five fixed-width
//! limb arrays (R, S, M+, M−, TMP), little-endian `u32` limbs, plus a digit
//! buffer. No `i64_trunc` of the value is ever performed, so arbitrary
//! magnitude is handled without traps.

use super::{CompiledFunc, WasmEmitter};
use super::rt_string::{string_data_off, string_hdr, string_cap_off};
use wasm_encoder::ValType;
use super::TrackedFunction as Function;

// ── Big-integer layout (bytes) ──
// Each bignum: [len:i32 @ +0][limb_0 @ +4][limb_1 @ +8]... little-endian u32.
// NLIMBS limbs cover values up to ~2^(32*NLIMBS). The widest intermediate is
// S for the largest/smallest exponents scaled by 10^k; 128 limbs = 4096 bits
// is ample headroom (max needed is < ~1100 bits + log2(10^k) ≈ 2150 bits).
// `pub(super)` so the decimal→f64 parser (`rt_dec2flt`) can lay out bignums in
// the SAME format the shared `__dragon_*` helpers expect ([len:i32][u32 limbs]).
pub(super) const NLIMBS: u32 = 128;
pub(super) const BN_HDR: u32 = 4; // len word
pub(super) const BN_BYTES: u32 = BN_HDR + NLIMBS * 4; // 516
pub(super) const BN_STRIDE: u32 = (BN_BYTES + 15) & !15; // 528, 16-aligned

// Offsets of the five bignums within the scratch block.
const OFF_R: u32 = 0;
const OFF_S: u32 = BN_STRIDE;
const OFF_MP: u32 = BN_STRIDE * 2;
const OFF_MM: u32 = BN_STRIDE * 3;
const OFF_TMP: u32 = BN_STRIDE * 4;
// Digit buffer: ASCII digits, MSD first. 5e-324 needs ~768 leading zeros +
// digits; 1024 bytes is safe.
const OFF_DIGITS: u32 = BN_STRIDE * 5;
const DIGITS_CAP: u32 = 1100;
const SCRATCH_BYTES: u32 = OFF_DIGITS + DIGITS_CAP;

/// Function indices for the Dragon4 big-integer helpers.
#[derive(Default)]
pub struct DragonRuntime {
    /// dragon_norm(p) — recompute len (drop high zero limbs, keep len >= 1).
    pub norm: u32,
    /// dragon_mul_small(p, m) — p *= m  (m: u32).
    pub mul_small: u32,
    /// dragon_cmp(a, b) -> i32 — returns -1, 0, 1.
    pub cmp: u32,
    /// dragon_add(dst, src) — dst += src.
    pub add: u32,
    /// dragon_sub(dst, src) — dst -= src  (requires dst >= src).
    pub sub: u32,
    /// dragon_shl(p, bits) — p <<= bits.
    pub shl: u32,
    /// dragon_copy(dst, src) — dst = src.
    pub copy: u32,
}

/// Register all Dragon4 helper signatures. Call from `register_runtime_functions`.
pub fn register(emitter: &mut WasmEmitter) {
    let p_void = emitter.register_type(vec![ValType::I32], vec![]);
    let pp_void = emitter.register_type(vec![ValType::I32, ValType::I32], vec![]);
    let pp_i32 = emitter.register_type(vec![ValType::I32, ValType::I32], vec![ValType::I32]);

    emitter.rt.dragon.norm = emitter.register_func("__dragon_norm", p_void);
    emitter.rt.dragon.mul_small = emitter.register_func("__dragon_mul_small", pp_void);
    emitter.rt.dragon.cmp = emitter.register_func("__dragon_cmp", pp_i32);
    emitter.rt.dragon.add = emitter.register_func("__dragon_add", pp_void);
    emitter.rt.dragon.sub = emitter.register_func("__dragon_sub", pp_void);
    emitter.rt.dragon.shl = emitter.register_func("__dragon_shl", pp_void);
    emitter.rt.dragon.copy = emitter.register_func("__dragon_copy", pp_void);
}

/// Compile the `__float_to_string` driver body.
///
/// IMPORTANT: WASM function bodies must be emitted in ascending function-index
/// (= registration) order, because the code section maps the Nth body to the
/// Nth defined function. The driver `rt.float_to_string` is registered EARLY
/// (alongside int_to_string), so its body is emitted here at the matching
/// early position. The bignum helpers are registered LATE (after the regex
/// runtime), so `compile_helpers` is invoked at the end of `compile_runtime`.
pub fn compile_driver(emitter: &mut WasmEmitter) {
    compile_float_to_string(emitter);
}

/// Compile the Dragon4 bignum helper bodies. Must be called at the END of
/// `compile_runtime` to match the late registration order (see `register`).
pub fn compile_helpers(emitter: &mut WasmEmitter) {
    compile_norm(emitter);
    compile_mul_small(emitter);
    compile_cmp(emitter);
    compile_add(emitter);
    compile_sub(emitter);
    compile_shl(emitter);
    compile_copy(emitter);
}

// ───────────────────────── helpers ─────────────────────────

/// __dragon_norm(p): drop high zero limbs, keep len >= 1.
fn compile_norm(emitter: &mut WasmEmitter) {
    let type_idx = emitter.func_type_indices[&emitter.rt.dragon.norm];
    // param 0 = p ; local 1 = len
    let mut f = Function::new([(1, ValType::I32)]);
    wasm!(f, {
        local_get(0); i32_load(0); local_set(1);
        block_empty; loop_empty;
          // while len > 1 && limb[len-1] == 0 { len-- }
          local_get(1); i32_const(1); i32_le_u; br_if(1);
          // limb[len-1] at p + BN_HDR + (len-1)*4
          local_get(0); i32_const(BN_HDR as i32); i32_add;
          local_get(1); i32_const(1); i32_sub; i32_const(4); i32_mul; i32_add;
          i32_load(0);
          br_if(1); // nonzero -> stop
          local_get(1); i32_const(1); i32_sub; local_set(1);
          br(0);
        end; end;
        local_get(0); local_get(1); i32_store(0);
        end;
    });
    emitter.add_compiled(CompiledFunc::tracked_for(emitter.rt.dragon.norm, type_idx, f));
}

/// __dragon_mul_small(p, m): p *= m (m treated as u32).
fn compile_mul_small(emitter: &mut WasmEmitter) {
    let type_idx = emitter.func_type_indices[&emitter.rt.dragon.mul_small];
    // params: 0=p, 1=m | locals: 2=len, 3=i, 4=carry(i64), 5=prod(i64)
    let mut f = Function::new([
        (1, ValType::I32), (1, ValType::I32), (1, ValType::I64), (1, ValType::I64),
    ]);
    wasm!(f, {
        local_get(0); i32_load(0); local_set(2);
        i32_const(0); local_set(3);
        i64_const(0); local_set(4);
        block_empty; loop_empty;
          local_get(3); local_get(2); i32_ge_u; br_if(1);
          // prod = limb[i] * m + carry
          local_get(0); i32_const(BN_HDR as i32); i32_add; local_get(3); i32_const(4); i32_mul; i32_add;
          i32_load(0); i64_extend_i32_u;
          local_get(1); i64_extend_i32_u;
          i64_mul;
          local_get(4); i64_add;
          local_set(5);
          // limb[i] = prod & 0xFFFFFFFF
          local_get(0); i32_const(BN_HDR as i32); i32_add; local_get(3); i32_const(4); i32_mul; i32_add;
          local_get(5); i32_wrap_i64;
          i32_store(0);
          // carry = prod >> 32
          local_get(5); i64_const(32); i64_shr_u; local_set(4);
          local_get(3); i32_const(1); i32_add; local_set(3);
          br(0);
        end; end;
        // emit remaining carry limbs
        block_empty; loop_empty;
          local_get(4); i64_eqz; br_if(1);
          local_get(0); i32_const(BN_HDR as i32); i32_add; local_get(2); i32_const(4); i32_mul; i32_add;
          local_get(4); i32_wrap_i64;
          i32_store(0);
          local_get(2); i32_const(1); i32_add; local_set(2);
          local_get(4); i64_const(32); i64_shr_u; local_set(4);
          br(0);
        end; end;
        local_get(0); local_get(2); i32_store(0);
        end;
    });
    emitter.add_compiled(CompiledFunc::tracked_for(emitter.rt.dragon.mul_small, type_idx, f));
}

/// __dragon_cmp(a, b) -> i32: -1 if a<b, 0 if eq, 1 if a>b. Assumes normalized.
fn compile_cmp(emitter: &mut WasmEmitter) {
    let type_idx = emitter.func_type_indices[&emitter.rt.dragon.cmp];
    // params: 0=a, 1=b | locals: 2=la, 3=lb, 4=i, 5=va, 6=vb
    let mut f = Function::new([
        (1, ValType::I32), (1, ValType::I32), (1, ValType::I32), (1, ValType::I32), (1, ValType::I32),
    ]);
    wasm!(f, {
        local_get(0); i32_load(0); local_set(2);
        local_get(1); i32_load(0); local_set(3);
        // different lengths -> longer is larger
        local_get(2); local_get(3); i32_ne;
        if_empty;
          local_get(2); local_get(3); i32_lt_u;
          if_i32; i32_const(-1); else_; i32_const(1); end;
          return_;
        end;
        // equal lengths: compare from MSD down
        local_get(2); local_set(4); // i = len
        block_empty; loop_empty;
          local_get(4); i32_eqz; br_if(1);
          local_get(4); i32_const(1); i32_sub; local_set(4);
          local_get(0); i32_const(BN_HDR as i32); i32_add; local_get(4); i32_const(4); i32_mul; i32_add; i32_load(0); local_set(5);
          local_get(1); i32_const(BN_HDR as i32); i32_add; local_get(4); i32_const(4); i32_mul; i32_add; i32_load(0); local_set(6);
          local_get(5); local_get(6); i32_ne;
          if_empty;
            local_get(5); local_get(6); i32_lt_u;
            if_i32; i32_const(-1); else_; i32_const(1); end;
            return_;
          end;
          br(0);
        end; end;
        i32_const(0);
        end;
    });
    emitter.add_compiled(CompiledFunc::tracked_for(emitter.rt.dragon.cmp, type_idx, f));
}

/// __dragon_add(dst, src): dst += src.
fn compile_add(emitter: &mut WasmEmitter) {
    let type_idx = emitter.func_type_indices[&emitter.rt.dragon.add];
    // params: 0=dst, 1=src | locals: 2=ld, 3=ls, 4=n, 5=i, 6=carry(i64), 7=sum(i64)
    let mut f = Function::new([
        (1, ValType::I32), (1, ValType::I32), (1, ValType::I32), (1, ValType::I32),
        (1, ValType::I64), (1, ValType::I64),
    ]);
    wasm!(f, {
        local_get(0); i32_load(0); local_set(2);
        local_get(1); i32_load(0); local_set(3);
        // n = max(ld, ls)
        local_get(2); local_get(3); i32_ge_u;
        if_i32; local_get(2); else_; local_get(3); end;
        local_set(4);
        i32_const(0); local_set(5);
        i64_const(0); local_set(6);
        block_empty; loop_empty;
          local_get(5); local_get(4); i32_ge_u; br_if(1);
          // a = (i < ld) ? dst.limb[i] : 0
          local_get(5); local_get(2); i32_lt_u;
          if_i64;
            local_get(0); i32_const(BN_HDR as i32); i32_add; local_get(5); i32_const(4); i32_mul; i32_add; i32_load(0); i64_extend_i32_u;
          else_; i64_const(0); end;
          // b = (i < ls) ? src.limb[i] : 0
          local_get(5); local_get(3); i32_lt_u;
          if_i64;
            local_get(1); i32_const(BN_HDR as i32); i32_add; local_get(5); i32_const(4); i32_mul; i32_add; i32_load(0); i64_extend_i32_u;
          else_; i64_const(0); end;
          i64_add; local_get(6); i64_add; local_set(7);
          // dst.limb[i] = sum & 0xFFFFFFFF
          local_get(0); i32_const(BN_HDR as i32); i32_add; local_get(5); i32_const(4); i32_mul; i32_add;
          local_get(7); i32_wrap_i64; i32_store(0);
          local_get(7); i64_const(32); i64_shr_u; local_set(6);
          local_get(5); i32_const(1); i32_add; local_set(5);
          br(0);
        end; end;
        // final carry
        local_get(6); i64_eqz; i32_eqz;
        if_empty;
          local_get(0); i32_const(BN_HDR as i32); i32_add; local_get(4); i32_const(4); i32_mul; i32_add;
          local_get(6); i32_wrap_i64; i32_store(0);
          local_get(4); i32_const(1); i32_add; local_set(4);
        end;
        local_get(0); local_get(4); i32_store(0);
        end;
    });
    emitter.add_compiled(CompiledFunc::tracked_for(emitter.rt.dragon.add, type_idx, f));
}

/// __dragon_sub(dst, src): dst -= src. Requires dst >= src. Normalizes result.
fn compile_sub(emitter: &mut WasmEmitter) {
    let type_idx = emitter.func_type_indices[&emitter.rt.dragon.sub];
    // params: 0=dst, 1=src | locals: 2=ld, 3=ls, 4=i, 5=borrow(i64), 6=diff(i64)
    let mut f = Function::new([
        (1, ValType::I32), (1, ValType::I32), (1, ValType::I32),
        (1, ValType::I64), (1, ValType::I64),
    ]);
    wasm!(f, {
        local_get(0); i32_load(0); local_set(2);
        local_get(1); i32_load(0); local_set(3);
        i32_const(0); local_set(4);
        i64_const(0); local_set(5);
        block_empty; loop_empty;
          local_get(4); local_get(2); i32_ge_u; br_if(1);
          // diff = dst.limb[i] - (i<ls? src.limb[i] : 0) - borrow
          local_get(0); i32_const(BN_HDR as i32); i32_add; local_get(4); i32_const(4); i32_mul; i32_add; i32_load(0); i64_extend_i32_u;
          local_get(4); local_get(3); i32_lt_u;
          if_i64;
            local_get(1); i32_const(BN_HDR as i32); i32_add; local_get(4); i32_const(4); i32_mul; i32_add; i32_load(0); i64_extend_i32_u;
          else_; i64_const(0); end;
          i64_sub; local_get(5); i64_sub; local_set(6);
          // if diff < 0 (high bit set since we extended u32): diff += 2^32, borrow=1
          local_get(6); i64_const(0); i64_lt_s;
          if_empty;
            local_get(6); i64_const(0x1_0000_0000); i64_add; local_set(6);
            i64_const(1); local_set(5);
          else_;
            i64_const(0); local_set(5);
          end;
          local_get(0); i32_const(BN_HDR as i32); i32_add; local_get(4); i32_const(4); i32_mul; i32_add;
          local_get(6); i32_wrap_i64; i32_store(0);
          local_get(4); i32_const(1); i32_add; local_set(4);
          br(0);
        end; end;
        local_get(0); call(emitter.rt.dragon.norm);
        end;
    });
    emitter.add_compiled(CompiledFunc::tracked_for(emitter.rt.dragon.sub, type_idx, f));
}

/// __dragon_shl(p, bits): p <<= bits.
fn compile_shl(emitter: &mut WasmEmitter) {
    let type_idx = emitter.func_type_indices[&emitter.rt.dragon.shl];
    // params: 0=p, 1=bits | locals: 2=len, 3=limb_shift, 4=bit_shift, 5=i, 6=carry, 7=v
    let mut f = Function::new([
        (1, ValType::I32), (1, ValType::I32), (1, ValType::I32),
        (1, ValType::I32), (1, ValType::I32), (1, ValType::I32),
    ]);
    wasm!(f, {
        local_get(0); i32_load(0); local_set(2);
        local_get(1); i32_const(32); i32_div_u; local_set(3); // limb_shift
        local_get(1); i32_const(32); i32_rem_u; local_set(4); // bit_shift
        // limb shift: move limbs up by limb_shift, zero the low ones
        local_get(3); i32_eqz; i32_eqz;
        if_empty;
          // i = len; while i>0 { i--; limb[i+ls] = limb[i] }
          local_get(2); local_set(5);
          block_empty; loop_empty;
            local_get(5); i32_eqz; br_if(1);
            local_get(5); i32_const(1); i32_sub; local_set(5);
            local_get(0); i32_const(BN_HDR as i32); i32_add; local_get(5); local_get(3); i32_add; i32_const(4); i32_mul; i32_add;
            local_get(0); i32_const(BN_HDR as i32); i32_add; local_get(5); i32_const(4); i32_mul; i32_add; i32_load(0);
            i32_store(0);
            br(0);
          end; end;
          // zero low limb_shift limbs
          i32_const(0); local_set(5);
          block_empty; loop_empty;
            local_get(5); local_get(3); i32_ge_u; br_if(1);
            local_get(0); i32_const(BN_HDR as i32); i32_add; local_get(5); i32_const(4); i32_mul; i32_add;
            i32_const(0); i32_store(0);
            local_get(5); i32_const(1); i32_add; local_set(5);
            br(0);
          end; end;
          local_get(2); local_get(3); i32_add; local_set(2);
        end;
        // bit shift within limbs
        local_get(4); i32_eqz; i32_eqz;
        if_empty;
          i32_const(0); local_set(6); // carry
          local_get(3); local_set(5); // i = limb_shift
          block_empty; loop_empty;
            local_get(5); local_get(2); i32_ge_u; br_if(1);
            local_get(0); i32_const(BN_HDR as i32); i32_add; local_get(5); i32_const(4); i32_mul; i32_add; i32_load(0); local_set(7);
            // limb[i] = (v << bit_shift) | carry
            local_get(0); i32_const(BN_HDR as i32); i32_add; local_get(5); i32_const(4); i32_mul; i32_add;
            local_get(7); local_get(4); i32_shl; local_get(6); i32_or;
            i32_store(0);
            // carry = v >> (32 - bit_shift)
            local_get(7); i32_const(32); local_get(4); i32_sub; i32_shr_u; local_set(6);
            local_get(5); i32_const(1); i32_add; local_set(5);
            br(0);
          end; end;
          local_get(6); i32_eqz; i32_eqz;
          if_empty;
            local_get(0); i32_const(BN_HDR as i32); i32_add; local_get(2); i32_const(4); i32_mul; i32_add;
            local_get(6); i32_store(0);
            local_get(2); i32_const(1); i32_add; local_set(2);
          end;
        end;
        local_get(0); local_get(2); i32_store(0);
        local_get(0); call(emitter.rt.dragon.norm);
        end;
    });
    emitter.add_compiled(CompiledFunc::tracked_for(emitter.rt.dragon.shl, type_idx, f));
}

/// __dragon_copy(dst, src): dst = src (len + limbs).
fn compile_copy(emitter: &mut WasmEmitter) {
    let type_idx = emitter.func_type_indices[&emitter.rt.dragon.copy];
    // params: 0=dst, 1=src | local 2=len, 3=i
    let mut f = Function::new([(1, ValType::I32), (1, ValType::I32)]);
    wasm!(f, {
        local_get(1); i32_load(0); local_set(2);
        local_get(0); local_get(2); i32_store(0);
        i32_const(0); local_set(3);
        block_empty; loop_empty;
          local_get(3); local_get(2); i32_ge_u; br_if(1);
          local_get(0); i32_const(BN_HDR as i32); i32_add; local_get(3); i32_const(4); i32_mul; i32_add;
          local_get(1); i32_const(BN_HDR as i32); i32_add; local_get(3); i32_const(4); i32_mul; i32_add; i32_load(0);
          i32_store(0);
          local_get(3); i32_const(1); i32_add; local_set(3);
          br(0);
        end; end;
        end;
    });
    emitter.add_compiled(CompiledFunc::tracked_for(emitter.rt.dragon.copy, type_idx, f));
}

// ───────────────────────── driver ─────────────────────────

/// __float_to_string(f: f64) -> i32 (String ptr).
///
/// Mirrors the validated Rust prototype: decompose to f·2^e, run Dragon4
/// to get the shortest decimal digits + decimal exponent k, then render in
/// fixed notation with the `.0` integer-suffix rule.
fn compile_float_to_string(emitter: &mut WasmEmitter) {
    let type_idx = emitter.func_type_indices[&emitter.rt.float_to_string];
    let dr = DragonRefs::new(emitter);

    // Locals (after the f64 param 0):
    //  1  base        i32  scratch block base ptr
    //  2  bits        i64  raw bits of |x|
    //  3  raw_exp     i32
    //  4  raw_mant    i64
    //  5  f           i64  mantissa
    //  6  e           i32  binary exponent
    //  7  k           i32  decimal exponent
    //  8  even        i32  mantissa parity (1 = even)
    //  9  low_bnd     i32  asymmetric-low-boundary flag
    // 10  dlen        i32  number of generated digits
    // 11  neg         i32  sign flag
    // 12  d           i32  current digit value
    // 13  low_t       i32
    // 14  high_t      i32
    // 15  round_up    i32
    // 16  i           i32  general loop counter
    // 17  (reserved)  f64  spare f64 scratch (k is computed on the value stack)
    // 18  result      i32  string ptr
    // 19  out_len     i32
    // 20  dp          i32  digit-buffer ptr (base + OFF_DIGITS)
    // 21  m           i32  digit count (alias of dlen for render)
    // 22  carry_i     i32  carry index for round-up
    // 23  cmp_tmp     i32  scratch for a bignum comparison result
    let mut f = Function::new([
        (1, ValType::I32), // 1 base
        (1, ValType::I64), // 2 bits
        (1, ValType::I32), // 3 raw_exp
        (1, ValType::I64), // 4 raw_mant
        (1, ValType::I64), // 5 f
        (1, ValType::I32), // 6 e
        (1, ValType::I32), // 7 k
        (1, ValType::I32), // 8 even
        (1, ValType::I32), // 9 low_bnd
        (1, ValType::I32), // 10 dlen
        (1, ValType::I32), // 11 neg
        (1, ValType::I32), // 12 d
        (1, ValType::I32), // 13 low_t
        (1, ValType::I32), // 14 high_t
        (1, ValType::I32), // 15 round_up
        (1, ValType::I32), // 16 i
        (1, ValType::F64), // 17 approx
        (1, ValType::I32), // 18 result
        (1, ValType::I32), // 19 out_len
        (1, ValType::I32), // 20 dp
        (1, ValType::I32), // 21 m (digit count, alias of dlen for render)
        (1, ValType::I32), // 22 carry_i / render index
        (1, ValType::I32), // 23 cmp_tmp
    ]);

    // ── Special cases: NaN / inf / zero ──
    // bits = reinterpret(x); exp field; mant field.
    wasm!(f, {
        local_get(0); i64_reinterpret_f64; local_set(2);
    });
    // NaN: exp==0x7FF && mant!=0  → "NaN"
    let s_nan = emitter.intern_string("NaN");
    let s_inf = emitter.intern_string("inf");
    let s_ninf = emitter.intern_string("-inf");
    let s_zero = emitter.intern_string("0.0");
    let s_nzero = emitter.intern_string("-0.0");
    wasm!(f, {
        // exp = (bits >> 52) & 0x7FF
        local_get(2); i64_const(52); i64_shr_u; i32_wrap_i64; i32_const(0x7FF); i32_and; local_set(3);
        // raw_mant = bits & 0xF_FFFF_FFFF_FFFF
        local_get(2); i64_const(0x000F_FFFF_FFFF_FFFF); i64_and; local_set(4);
        // if exp == 0x7FF: NaN or inf
        local_get(3); i32_const(0x7FF); i32_eq;
        if_empty;
          local_get(4); i64_eqz; i32_eqz;
          if_empty;
            i32_const(s_nan as i32); return_;
          end;
          // inf: sign bit
          local_get(2); i64_const(0); i64_lt_s;
          if_i32; i32_const(s_ninf as i32); else_; i32_const(s_inf as i32); end;
          return_;
        end;
    });
    // zero: x == 0.0 (covers +0 and -0). Sign from bit 63.
    wasm!(f, {
        local_get(0); f64_const(0.0); f64_eq;
        if_empty;
          local_get(2); i64_const(0); i64_lt_s;
          if_i32; i32_const(s_nzero as i32); else_; i32_const(s_zero as i32); end;
          return_;
        end;
    });

    // neg = sign bit
    wasm!(f, {
        local_get(2); i64_const(0); i64_lt_s; local_set(11);
    });

    // Work with |x|: clear sign bit.
    wasm!(f, {
        local_get(2); i64_const(0x7FFF_FFFF_FFFF_FFFF); i64_and; local_set(2);
        // re-extract exp/mant from |bits| (exp/mant already sign-independent, but recompute mant cleanly)
        local_get(2); i64_const(52); i64_shr_u; i32_wrap_i64; i32_const(0x7FF); i32_and; local_set(3);
        local_get(2); i64_const(0x000F_FFFF_FFFF_FFFF); i64_and; local_set(4);
    });

    // Decompose: if exp==0 (subnormal): f=raw_mant, e=-1074
    //            else:                  f=raw_mant + 2^52, e=exp-1075
    wasm!(f, {
        local_get(3); i32_eqz;
        if_empty;
          local_get(4); local_set(5);
          i32_const(-1074); local_set(6);
        else_;
          local_get(4); i64_const(0x10_0000_0000_0000); i64_add; local_set(5);
          local_get(3); i32_const(1075); i32_sub; local_set(6);
        end;
        // even = (f & 1) == 0
        local_get(5); i64_const(1); i64_and; i64_eqz; local_set(8);
        // low_bnd = raw_mant == 0 && exp > 1
        local_get(4); i64_eqz; local_get(3); i32_const(1); i32_gt_s; i32_and; local_set(9);
    });

    // Allocate scratch block.
    wasm!(f, {
        i32_const(SCRATCH_BYTES as i32); call(emitter.rt.alloc); local_set(1);
        local_get(1); i32_const(OFF_DIGITS as i32); i32_add; local_set(20);
    });

    // ── Initialize R, S, MP, MM as bignums set to f and 1 ──
    // Helper: set bignum at (base+off) to the i64 value on stack-top is awkward;
    // we inline "set to f" by storing low/high limbs, and "set to 1".
    // R = f (then shifted). S/MP/MM start as small ints.
    dr.set_u64(&mut f, OFF_R, 5);   // R = f
    if true {
        // e >= 0 branch vs e < 0 branch
        wasm!(f, {
            local_get(6); i32_const(0); i32_ge_s;
            if_empty;
        });
        // R = f << (e+1); S = 2; MP = 2^e; MM = 2^e
        dr.shl_imm_local(&mut f, OFF_R, 6, 1); // R <<= e+1
        dr.set_small(&mut f, OFF_S, 2);
        dr.set_small(&mut f, OFF_MP, 1);
        dr.shl_local(&mut f, OFF_MP, 6);        // MP <<= e
        dr.set_small(&mut f, OFF_MM, 1);
        dr.shl_local(&mut f, OFF_MM, 6);        // MM <<= e
        wasm!(f, {
            else_;
        });
        // R = f << 1; S = 1 << (1 - e); MP = 1; MM = 1
        dr.shl_const(&mut f, OFF_R, 1);
        dr.set_small(&mut f, OFF_S, 1);
        dr.shl_one_minus_e(&mut f, OFF_S, 6);   // S <<= (1 - e)
        dr.set_small(&mut f, OFF_MP, 1);
        dr.set_small(&mut f, OFF_MM, 1);
        wasm!(f, {
            end;
        });
    }
    // low boundary: MP <<= 1; R <<= 1; S <<= 1
    wasm!(f, {
        local_get(9);
        if_empty;
    });
    dr.shl_const(&mut f, OFF_MP, 1);
    dr.shl_const(&mut f, OFF_R, 1);
    dr.shl_const(&mut f, OFF_S, 1);
    wasm!(f, { end; });

    // ── Estimate k = ceil(log10(f) + e*log10(2)) ──
    // approx = log10((f64)f) + e * log10(2)
    // The fixup loops below correct any off-by-one in this estimate, so its
    // only requirement is to be close — exactness is not needed.
    wasm!(f, {
        local_get(5); f64_convert_i64_u;
        call(emitter.rt.math_log10);
        local_get(6); f64_convert_i32_s; f64_const(LOG10_2); f64_mul;
        f64_add;
        f64_ceil;
        // k = (i32) ceil(approx). k is in [-324, 309]; trunc-toward-zero is safe.
    });
    f.instruction(&wasm_encoder::Instruction::I32TruncF64S);
    wasm!(f, { local_set(7); });

    // ── Scale: if k>=0 S *= 10^k else R,MP,MM *= 10^(-k) ──
    // loop k times
    wasm!(f, {
        local_get(7); i32_const(0); i32_ge_s;
        if_empty;
          i32_const(0); local_set(16);
          block_empty; loop_empty;
            local_get(16); local_get(7); i32_ge_s; br_if(1);
    });
    dr.mul10(&mut f, OFF_S);
    wasm!(f, {
            local_get(16); i32_const(1); i32_add; local_set(16);
            br(0);
          end; end;
        else_;
          i32_const(0); local_set(16);
          block_empty; loop_empty;
            local_get(16); i32_const(0); local_get(7); i32_sub; i32_ge_s; br_if(1);
    });
    dr.mul10(&mut f, OFF_R);
    dr.mul10(&mut f, OFF_MP);
    dr.mul10(&mut f, OFF_MM);
    wasm!(f, {
            local_get(16); i32_const(1); i32_add; local_set(16);
            br(0);
          end; end;
        end;
    });

    // ── Fixup loop 1: while (R+MP) cmp S too_big: S*=10; k++ ──
    // too_big = even ? (R+MP >= S) : (R+MP > S)
    wasm!(f, {
        block_empty; loop_empty;
    });
    dr.copy(&mut f, OFF_TMP, OFF_R);
    dr.add(&mut f, OFF_TMP, OFF_MP);
    dr.cmp_set(&mut f, OFF_TMP, OFF_S); // cmp result -> local 23
    // too_big = even ? (c >= 0) : (c > 0)
    dr.pred_ge_gt(&mut f);
    wasm!(f, {
        i32_eqz; br_if(1); // not too_big -> break
    });
    dr.mul10(&mut f, OFF_S);
    wasm!(f, {
        local_get(7); i32_const(1); i32_add; local_set(7);
        br(0);
        end; end;
    });

    // ── Fixup loop 2: while (R+MP)*10 cmp S too_small: R,MP,MM*=10; k-- ──
    // too_small = even ? ((R+MP)*10 <= S) : ((R+MP)*10 < S)
    wasm!(f, {
        block_empty; loop_empty;
    });
    dr.copy(&mut f, OFF_TMP, OFF_R);
    dr.add(&mut f, OFF_TMP, OFF_MP);
    dr.mul10(&mut f, OFF_TMP);
    dr.cmp_set(&mut f, OFF_TMP, OFF_S);
    // too_small = even ? (c <= 0) : (c < 0)
    dr.pred_le_lt(&mut f);
    wasm!(f, {
        i32_eqz; br_if(1);
    });
    dr.mul10(&mut f, OFF_R);
    dr.mul10(&mut f, OFF_MP);
    dr.mul10(&mut f, OFF_MM);
    wasm!(f, {
        local_get(7); i32_const(1); i32_sub; local_set(7);
        br(0);
        end; end;
    });

    // ── Digit generation loop ──
    wasm!(f, {
        i32_const(0); local_set(10); // dlen = 0
        block_empty; loop_empty;     // [outer block (1)] [loop (0)]
    });
    dr.mul10(&mut f, OFF_R);
    dr.mul10(&mut f, OFF_MP);
    dr.mul10(&mut f, OFF_MM);
    // d = 0; while cmp(R,S) >= 0 { R -= S; d++ }
    wasm!(f, {
        i32_const(0); local_set(12);
        block_empty; loop_empty;
    });
    dr.cmp(&mut f, OFF_R, OFF_S);
    wasm!(f, {
          i32_const(0); i32_lt_s; br_if(1); // c < 0 -> stop
    });
    dr.sub(&mut f, OFF_R, OFF_S);
    wasm!(f, {
          local_get(12); i32_const(1); i32_add; local_set(12);
          br(0);
        end; end;
    });
    // low_t = even ? cmp(R,MM)<=0 : cmp(R,MM)<0
    dr.cmp_set(&mut f, OFF_R, OFF_MM);
    dr.pred_le_lt(&mut f);
    wasm!(f, { local_set(13); });
    // high_t = even ? cmp(R+MP,S)>=0 : cmp(R+MP,S)>0
    dr.copy(&mut f, OFF_TMP, OFF_R);
    dr.add(&mut f, OFF_TMP, OFF_MP);
    dr.cmp_set(&mut f, OFF_TMP, OFF_S);
    dr.pred_ge_gt(&mut f);
    wasm!(f, { local_set(14); });
    // if !low_t && !high_t: emit d, continue
    wasm!(f, {
        local_get(13); i32_eqz; local_get(14); i32_eqz; i32_and;
        if_empty;
          // digits[dlen] = '0'+d ; dlen++
          local_get(20); local_get(10); i32_add;
          local_get(12); i32_const(48); i32_add; i32_store8(0);
          local_get(10); i32_const(1); i32_add; local_set(10);
          br(1); // continue outer loop
        end;
    });
    // terminate: round_up = ?
    // if low_t && !high_t: round_up=0
    // elif high_t && !low_t: round_up=1
    // else: round_up = (2R cmp S) >= 0
    wasm!(f, {
        local_get(13); local_get(14); i32_eqz; i32_and;
        if_empty;
          i32_const(0); local_set(15);
        else_;
          local_get(14); local_get(13); i32_eqz; i32_and;
          if_empty;
            i32_const(1); local_set(15);
          else_;
    });
    // 2R cmp S
    dr.copy(&mut f, OFF_TMP, OFF_R);
    dr.shl_const(&mut f, OFF_TMP, 1);
    dr.cmp(&mut f, OFF_TMP, OFF_S);
    wasm!(f, {
            i32_const(0); i32_ge_s; local_set(15);
          end;
        end;
    });
    // digits[dlen] = '0'+d ; dlen++
    wasm!(f, {
        local_get(20); local_get(10); i32_add;
        local_get(12); i32_const(48); i32_add; i32_store8(0);
        local_get(10); i32_const(1); i32_add; local_set(10);
    });
    // if round_up: propagate carry from the last digit
    wasm!(f, {
        local_get(15);
        if_empty;
          // i = dlen
          local_get(10); local_set(22);
          block_empty; loop_empty;
            // if i == 0: prepend '1', k++, break
            local_get(22); i32_eqz;
            if_empty;
              // shift digits right by 1: memmove digits[0..dlen] -> digits[1..dlen+1]
    });
    // memory.copy(dst=dp+1, src=dp, len=dlen)
    wasm!(f, {
              local_get(20); i32_const(1); i32_add;
              local_get(20);
              local_get(10);
              memory_copy;
              local_get(20); i32_const(49); i32_store8(0); // '1'
              local_get(10); i32_const(1); i32_add; local_set(10);
              local_get(7); i32_const(1); i32_add; local_set(7);
              br(2); // break carry loop
            end;
            // i--
            local_get(22); i32_const(1); i32_sub; local_set(22);
            // if digits[i] == '9': set '0', continue loop; else digits[i]++, break.
            // Depths from inside this if/else: if-block=0, loop=1, block=2.
            local_get(20); local_get(22); i32_add; i32_load8_u(0);
            i32_const(57); i32_eq; // '9'
            if_empty;
              local_get(20); local_get(22); i32_add; i32_const(48); i32_store8(0);
              br(1); // continue carry loop
            else_;
              local_get(20); local_get(22); i32_add;
              local_get(20); local_get(22); i32_add; i32_load8_u(0); i32_const(1); i32_add;
              i32_store8(0);
              br(2); // break carry loop
            end;
          end; end;
        end;
    });
    // break outer digit loop
    wasm!(f, {
        br(1);
        end; end; // end loop, end outer block
    });

    // ── Render: dlen=m digits in buffer (dp), exponent k, sign neg ──
    // Compute out_len, alloc string, fill.
    // Cases:
    //   k <= 0:  "[-]0." + (-k) zeros + digits        len = neg + 2 + (-k) + m
    //   k >= m:  "[-]" + digits + (k-m) zeros + ".0"   len = neg + m + (k-m) + 2
    //   else:    "[-]" + digits[0..k] + "." + digits[k..m]  len = neg + m + 1
    //
    // result buffer: string header then data. We compute total data len, alloc,
    // then write characters sequentially using a cursor.
    wasm!(f, {
        local_get(10); local_set(21); // m = dlen (alias)
    });

    // Compute out_len into local 19.
    wasm!(f, {
        // start with neg
        local_get(11);
        // + branch
        local_get(7); i32_const(0); i32_le_s;
        if_i32;
          // 2 + (-k) + m
          i32_const(2); i32_const(0); local_get(7); i32_sub; i32_add; local_get(21); i32_add;
        else_;
          local_get(7); local_get(21); i32_ge_s;
          if_i32;
            // m + (k-m) + 2  == k + 2
            local_get(7); i32_const(2); i32_add;
          else_;
            // m + 1
            local_get(21); i32_const(1); i32_add;
          end;
        end;
        i32_add;
        local_set(19);
    });

    // Alloc string: header + out_len, set len & cap.
    wasm!(f, {
        local_get(19); i32_const(string_hdr() as i32); i32_add;
        call(emitter.rt.alloc); local_set(18);
        local_get(18); local_get(19); i32_store(0);
        local_get(18); local_get(19); i32_store(string_cap_off() as u32, 0);
    });

    // Write characters. Use local 16 as write cursor (byte offset from data start),
    // local 22 as a scratch index.
    wasm!(f, {
        i32_const(0); local_set(16); // cursor
        // sign
        local_get(11);
        if_empty;
          local_get(18); i32_const(string_data_off() as i32); i32_add; local_get(16); i32_add;
          i32_const(45); i32_store8(0); // '-'
          local_get(16); i32_const(1); i32_add; local_set(16);
        end;
    });

    // Branch on k.
    wasm!(f, {
        local_get(7); i32_const(0); i32_le_s;
        if_empty;
          // "0." then (-k) zeros then digits
          local_get(18); i32_const(string_data_off() as i32); i32_add; local_get(16); i32_add;
          i32_const(48); i32_store8(0); // '0'
          local_get(16); i32_const(1); i32_add; local_set(16);
          local_get(18); i32_const(string_data_off() as i32); i32_add; local_get(16); i32_add;
          i32_const(46); i32_store8(0); // '.'
          local_get(16); i32_const(1); i32_add; local_set(16);
          // (-k) zeros
          i32_const(0); local_set(22);
          block_empty; loop_empty;
            local_get(22); i32_const(0); local_get(7); i32_sub; i32_ge_s; br_if(1);
            local_get(18); i32_const(string_data_off() as i32); i32_add; local_get(16); i32_add;
            i32_const(48); i32_store8(0);
            local_get(16); i32_const(1); i32_add; local_set(16);
            local_get(22); i32_const(1); i32_add; local_set(22);
            br(0);
          end; end;
          // digits
          i32_const(0); local_set(22);
          block_empty; loop_empty;
            local_get(22); local_get(21); i32_ge_s; br_if(1);
            local_get(18); i32_const(string_data_off() as i32); i32_add; local_get(16); i32_add;
            local_get(20); local_get(22); i32_add; i32_load8_u(0);
            i32_store8(0);
            local_get(16); i32_const(1); i32_add; local_set(16);
            local_get(22); i32_const(1); i32_add; local_set(22);
            br(0);
          end; end;
        else_;
          local_get(7); local_get(21); i32_ge_s;
          if_empty;
            // digits then (k-m) zeros then ".0"
            i32_const(0); local_set(22);
            block_empty; loop_empty;
              local_get(22); local_get(21); i32_ge_s; br_if(1);
              local_get(18); i32_const(string_data_off() as i32); i32_add; local_get(16); i32_add;
              local_get(20); local_get(22); i32_add; i32_load8_u(0);
              i32_store8(0);
              local_get(16); i32_const(1); i32_add; local_set(16);
              local_get(22); i32_const(1); i32_add; local_set(22);
              br(0);
            end; end;
            // (k - m) zeros
            i32_const(0); local_set(22);
            block_empty; loop_empty;
              local_get(22); local_get(7); local_get(21); i32_sub; i32_ge_s; br_if(1);
              local_get(18); i32_const(string_data_off() as i32); i32_add; local_get(16); i32_add;
              i32_const(48); i32_store8(0);
              local_get(16); i32_const(1); i32_add; local_set(16);
              local_get(22); i32_const(1); i32_add; local_set(22);
              br(0);
            end; end;
            // ".0"
            local_get(18); i32_const(string_data_off() as i32); i32_add; local_get(16); i32_add;
            i32_const(46); i32_store8(0);
            local_get(16); i32_const(1); i32_add; local_set(16);
            local_get(18); i32_const(string_data_off() as i32); i32_add; local_get(16); i32_add;
            i32_const(48); i32_store8(0);
            local_get(16); i32_const(1); i32_add; local_set(16);
          else_;
            // digits[0..k] "." digits[k..m]
            i32_const(0); local_set(22);
            block_empty; loop_empty;
              local_get(22); local_get(7); i32_ge_s; br_if(1);
              local_get(18); i32_const(string_data_off() as i32); i32_add; local_get(16); i32_add;
              local_get(20); local_get(22); i32_add; i32_load8_u(0);
              i32_store8(0);
              local_get(16); i32_const(1); i32_add; local_set(16);
              local_get(22); i32_const(1); i32_add; local_set(22);
              br(0);
            end; end;
            local_get(18); i32_const(string_data_off() as i32); i32_add; local_get(16); i32_add;
            i32_const(46); i32_store8(0);
            local_get(16); i32_const(1); i32_add; local_set(16);
            // digits[k..m]  (continue from local 22 = k)
            block_empty; loop_empty;
              local_get(22); local_get(21); i32_ge_s; br_if(1);
              local_get(18); i32_const(string_data_off() as i32); i32_add; local_get(16); i32_add;
              local_get(20); local_get(22); i32_add; i32_load8_u(0);
              i32_store8(0);
              local_get(16); i32_const(1); i32_add; local_set(16);
              local_get(22); i32_const(1); i32_add; local_set(22);
              br(0);
            end; end;
          end;
        end;
    });

    wasm!(f, { local_get(18); end; });
    emitter.add_compiled(CompiledFunc::tracked_for(emitter.rt.float_to_string, type_idx, f));
}

// ───────────────────────── float.to_fixed ─────────────────────────
//
// The EXACT binary expansion of any finite f64 terminates: |x| = m·2^e with
// e >= -1074, so there are at most 1074 nonzero fractional digits. The digit
// generator detects R == 0 and stops doing bignum work past that point, padding
// the remaining `N` positions with '0' — so the work is bounded by the real
// expansion regardless of how large `decimals` is.

/// __float_to_fixed(f: f64, decimals: i64) -> i32 (String ptr).
///
/// Reproduces native `format!("{:.N}", f)` EXACTLY: the decimal is the f64's
/// exact binary value `m·2^e` rounded to N fractional places, round-half-to-EVEN
/// on the exact value (so 2.5@0 -> "2", 3.5@0 -> "4", 2.675@2 -> "2.67" because
/// 2.675 is really 2.67499...). It rides the Dragon4 big-integer machinery: the
/// value is the exact rational R/S (S a power of two), digits are generated
/// MSD-first by `R*=10; d=floor(R/S); R-=d*S`, and the cutoff is rounded by the
/// half-even `2R vs S` test — identical exact arithmetic to Rust's flt2dec, so
/// there is no `10^N` i64 overflow and no multiply-then-round error.
///
/// Special cases match Rust: NaN -> "NaN", +inf -> "inf", -inf -> "-inf";
/// the sign bit is honored (-0.0@2 -> "-0.00"). N<0 is clamped to 0.
pub(super) fn compile_float_to_fixed(emitter: &mut WasmEmitter) {
    let type_idx = emitter.func_type_indices[&emitter.rt.float_to_fixed];
    let dr = DragonRefs::new(emitter);

    // Locals (after f64 param 0, i64 param 1 = decimals):
    //   2  base    i32  scratch block base (DragonRefs.ptr reads this)
    //   3  bits    i64  raw bits of |f|
    //   4  raw_exp i32
    //   5  raw_mant i64
    //   6  mant    i64  significand m (with implicit bit)
    //   7  e       i32  binary exponent
    //   8  neg     i32  sign flag
    //   9  n       i32  decimals (clamped >= 0)
    //  10  k       i32  decimal exponent (count of integer digits; <=0 ⇒ |x|<1)
    //  11  cnt     i32  number of meaningful generated digits = k + n (>=0)
    //  12  i       i32  general loop counter
    //  13  d       i32  current digit
    //  14  dp      i32  digit-buffer ptr
    //  15  dlen    i32  digits written into dp
    //  16  result  i32  string ptr
    //  17  out_len i32
    //  18  cursor  i32  write cursor (byte offset from data start)
    //  19  round   i32  round-up flag
    //  20  (reserved/unused)
    //  21  total   i32  total digit slots = max(k,0) + n
    //  22  start   i32  leading-zero slots before the first significant digit
    let mut f = Function::new([
        (1, ValType::I32),  // 2 base
        (1, ValType::I64),  // 3 bits
        (1, ValType::I32),  // 4 raw_exp
        (1, ValType::I64),  // 5 raw_mant
        (1, ValType::I64),  // 6 mant
        (1, ValType::I32),  // 7 e
        (1, ValType::I32),  // 8 neg
        (1, ValType::I32),  // 9 n
        (1, ValType::I32),  // 10 k
        (1, ValType::I32),  // 11 cnt
        (1, ValType::I32),  // 12 i
        (1, ValType::I32),  // 13 d
        (1, ValType::I32),  // 14 dp
        (1, ValType::I32),  // 15 dlen
        (1, ValType::I32),  // 16 result
        (1, ValType::I32),  // 17 out_len
        (1, ValType::I32),  // 18 cursor
        (1, ValType::I32),  // 19 round
        (1, ValType::I32),  // 20 (reserved/unused)
        (1, ValType::I32),  // 21 total (digit slots = max(k,0)+n)
        (1, ValType::I32),  // 22 start (leading-zero slots before the first real digit)
    ]);
    const BASE: u32 = 2; const BITS: u32 = 3; const RAW_EXP: u32 = 4; const RAW_MANT: u32 = 5;
    const MANT: u32 = 6; const E: u32 = 7; const NEG: u32 = 8; const N: u32 = 9;
    const K: u32 = 10; const CNT: u32 = 11; const I: u32 = 12; const D: u32 = 13;
    const DP: u32 = 14; const DLEN: u32 = 15; const RESULT: u32 = 16; const OUT_LEN: u32 = 17;
    const CURSOR: u32 = 18; const ROUND: u32 = 19;
    const TOTAL: u32 = 21; const START: u32 = 22;

    let s_nan = emitter.intern_string("NaN");
    let s_inf = emitter.intern_string("inf");
    let s_ninf = emitter.intern_string("-inf");

    // bits = reinterpret(f); n = clamp(decimals, 0, ..)
    wasm!(f, {
        local_get(0); i64_reinterpret_f64; local_set(BITS);
        local_get(1); i32_wrap_i64; local_set(N);
        local_get(N); i32_const(0); i32_lt_s; if_empty; i32_const(0); local_set(N); end;
    });

    // NaN / inf special cases (raw exp == 0x7FF).
    wasm!(f, {
        local_get(BITS); i64_const(52); i64_shr_u; i32_wrap_i64; i32_const(0x7FF); i32_and; local_set(RAW_EXP);
        local_get(BITS); i64_const(0x000F_FFFF_FFFF_FFFF); i64_and; local_set(RAW_MANT);
        local_get(RAW_EXP); i32_const(0x7FF); i32_eq;
        if_empty;
            local_get(RAW_MANT); i64_eqz; i32_eqz;
            if_empty; i32_const(s_nan as i32); return_; end;
            local_get(BITS); i64_const(0); i64_lt_s;
            if_i32; i32_const(s_ninf as i32); else_; i32_const(s_inf as i32); end;
            return_;
        end;
    });

    // neg = sign bit; work with |f| bits.
    wasm!(f, {
        local_get(BITS); i64_const(0); i64_lt_s; local_set(NEG);
        local_get(BITS); i64_const(0x7FFF_FFFF_FFFF_FFFF); i64_and; local_set(BITS);
        local_get(BITS); i64_const(52); i64_shr_u; i32_wrap_i64; i32_const(0x7FF); i32_and; local_set(RAW_EXP);
        local_get(BITS); i64_const(0x000F_FFFF_FFFF_FFFF); i64_and; local_set(RAW_MANT);
    });

    // Decompose: subnormal (raw_exp==0): mant=raw_mant, e=-1074; else mant=raw_mant+2^52, e=raw_exp-1075.
    wasm!(f, {
        local_get(RAW_EXP); i32_eqz;
        if_empty;
            local_get(RAW_MANT); local_set(MANT);
            i32_const(-1074); local_set(E);
        else_;
            local_get(RAW_MANT); i64_const(0x10_0000_0000_0000); i64_add; local_set(MANT);
            local_get(RAW_EXP); i32_const(1075); i32_sub; local_set(E);
        end;
    });

    // Allocate the Dragon4 scratch block (we use only R, S, TMP).
    wasm!(f, {
        i32_const(SCRATCH_BYTES as i32); call(emitter.rt.alloc); local_set(BASE);
        // Digit buffer: up to ~310 integer digits + n fraction digits + slack (for a
        // round-up carry that prepends one leading digit). The generation loop stops
        // producing significant digits once R hits 0, so n only sizes the buffer.
        i32_const(340); local_get(N); i32_add; i32_const(8); i32_add; call(emitter.rt.alloc); local_set(DP);
        i32_const(0); local_set(DLEN);
        i32_const(0); local_set(K);
    });
    // base must be in local 1 for DragonRefs.ptr; alias via a copy is impossible
    // (ptr() hardcodes local_get(1)). So we keep base in BASE and patch ptr via a
    // dedicated helper below that reads BASE instead. To keep DragonRefs usable we
    // simply move base into local 1 here is NOT possible (1 is the decimals param).
    // Instead this routine uses the bignum offsets with an explicit base in BASE.

    // ── Setup R/S exactly: value = R/S = mant·2^e ──
    // R bignum = mant (two limbs).
    set_bn_u64(&mut f, BASE, OFF_R, MANT);
    set_bn_small(&mut f, BASE, OFF_S, 1);
    wasm!(f, {
        local_get(E); i32_const(0); i32_ge_s;
        if_empty;
            // e >= 0: R <<= e ; S = 1
            local_get(BASE); i32_const(OFF_R as i32); i32_add; local_get(E); call(dr.shl);
        else_;
            // e < 0: S <<= (-e)
            local_get(BASE); i32_const(OFF_S as i32); i32_add; i32_const(0); local_get(E); i32_sub; call(dr.shl);
        end;
    });

    // ── Position: scale so value/10^k ∈ [0.1, 1), tracking k ──
    // if value == 0 (mant == 0): k stays 0, R stays 0 → all digits zero.
    wasm!(f, {
        local_get(MANT); i64_eqz; i32_eqz;
        if_empty;
            // value >= 1 ?  cmp(R, S) >= 0
            local_get(BASE); i32_const(OFF_R as i32); i32_add; local_get(BASE); i32_const(OFF_S as i32); i32_add; call(dr.cmp);
            i32_const(0); i32_ge_s;
            if_empty;
                // while cmp(R, S) >= 0 { S *= 10; k++ }
                block_empty; loop_empty;
                    local_get(BASE); i32_const(OFF_R as i32); i32_add; local_get(BASE); i32_const(OFF_S as i32); i32_add; call(dr.cmp);
                    i32_const(0); i32_lt_s; br_if(1);
                    local_get(BASE); i32_const(OFF_S as i32); i32_add; i32_const(10); call(dr.mul_small);
                    local_get(K); i32_const(1); i32_add; local_set(K);
                    br(0);
                end; end;
            else_;
                // while cmp(R*10, S) < 0 { R *= 10; k-- }
                block_empty; loop_empty;
                    // TMP = R; TMP *= 10; cmp(TMP, S) >= 0 → stop
                    local_get(BASE); i32_const(OFF_TMP as i32); i32_add; local_get(BASE); i32_const(OFF_R as i32); i32_add; call(dr.copy);
                    local_get(BASE); i32_const(OFF_TMP as i32); i32_add; i32_const(10); call(dr.mul_small);
                    local_get(BASE); i32_const(OFF_TMP as i32); i32_add; local_get(BASE); i32_const(OFF_S as i32); i32_add; call(dr.cmp);
                    i32_const(0); i32_ge_s; br_if(1);
                    local_get(BASE); i32_const(OFF_R as i32); i32_add; i32_const(10); call(dr.mul_small);
                    local_get(K); i32_const(1); i32_sub; local_set(K);
                    br(0);
                end; end;
            end;
        end;
    });

    // Digit-slot accounting. We materialize EVERY rendered digit into the buffer
    // (integer digits when k>0, plus all N fraction digits incl. leading zeros), so
    // the round-half-even carry can propagate uniformly and the render is a copy.
    //   cnt   = k + n        real (significant) digits from position k-1 down to -n.
    //   total = max(k,0) + n total digit slots (k integer digits when k>0, then n frac).
    //   start = max(-k,0)    leading-zero slots before the first significant digit
    //                        (only when k<=0; = total - max(cnt,0)).
    wasm!(f, {
        local_get(K); local_get(N); i32_add; local_set(CNT);
        local_get(K); i32_const(0); i32_gt_s; if_i32; local_get(K); else_; i32_const(0); end;
        local_get(N); i32_add; local_set(TOTAL);
        i32_const(0); local_get(K); i32_sub; i32_const(0); i32_gt_s;
        if_i32; i32_const(0); local_get(K); i32_sub; else_; i32_const(0); end;
        local_set(START);
    });

    // ── Generate `total` digit slots ──
    //   slot < start  : leading zero (only when k <= 0; no bignum work)
    //   R != 0        : R*=10; d = floor(R/S) via repeated subtraction; R -= d*S
    //   R == 0        : the exact expansion has ended → digit 0 (no bignum work)
    wasm!(f, {
        i32_const(0); local_set(DLEN);
        i32_const(0); local_set(I);
        block_empty; loop_empty;
            local_get(I); local_get(TOTAL); i32_ge_s; br_if(1);
            local_get(I); local_get(START); i32_lt_s;
            if_empty;
                // leading-zero slot (only when k <= 0): digit is 0, no bignum work.
                i32_const(0); local_set(D);
            else_;
                // Once R has been driven to 0, the EXACT expansion has ended and every
                // remaining digit is 0 — skip the bignum work. R is zero iff its len is
                // 1 and limb0 is 0. (This replaces a digit-count cap: it is exact and
                // bounds the work to the real expansion regardless of how large N is.)
                local_get(BASE); i32_const(OFF_R as i32); i32_add; i32_load(0); i32_const(1); i32_eq;
                local_get(BASE); i32_const((OFF_R + BN_HDR) as i32); i32_add; i32_load(0); i32_eqz;
                i32_and;
                if_empty;
                    i32_const(0); local_set(D);
                else_;
                    local_get(BASE); i32_const(OFF_R as i32); i32_add; i32_const(10); call(dr.mul_small);
                    i32_const(0); local_set(D);
                    block_empty; loop_empty;
                        local_get(BASE); i32_const(OFF_R as i32); i32_add; local_get(BASE); i32_const(OFF_S as i32); i32_add; call(dr.cmp);
                        i32_const(0); i32_lt_s; br_if(1);
                        local_get(BASE); i32_const(OFF_R as i32); i32_add; local_get(BASE); i32_const(OFF_S as i32); i32_add; call(dr.sub);
                        local_get(D); i32_const(1); i32_add; local_set(D);
                        br(0);
                    end; end;
                end;
            end;
            local_get(DP); local_get(DLEN); i32_add; local_get(D); i32_const(48); i32_add; i32_store8(0);
            local_get(DLEN); i32_const(1); i32_add; local_set(DLEN);
            local_get(I); i32_const(1); i32_add; local_set(I);
            br(0);
        end; end;
    });

    // ── Round half-to-even at position -n using `2R vs S` ──
    // When cnt <= 0 the digit loop ran 0 real steps, so R still holds the WHOLE value
    // and the cutoff -n is `-cnt` decades ABOVE it; scale S up by 10^(-cnt) so the
    // comparison is taken at position -n. (When cnt > 0, R is already the residue
    // below -n and -cnt <= 0, so no scaling.) Tie breaks to even via the last slot.
    wasm!(f, {
        local_get(CNT); i32_const(0); i32_lt_s;
        if_empty;
            // S *= 10, (-cnt) times.
            i32_const(0); local_set(I);
            block_empty; loop_empty;
                local_get(I); i32_const(0); local_get(CNT); i32_sub; i32_ge_s; br_if(1);
                local_get(BASE); i32_const(OFF_S as i32); i32_add; i32_const(10); call(dr.mul_small);
                local_get(I); i32_const(1); i32_add; local_set(I);
                br(0);
            end; end;
        end;
        // TMP = 2R; cmp(TMP, S).
        local_get(BASE); i32_const(OFF_TMP as i32); i32_add; local_get(BASE); i32_const(OFF_R as i32); i32_add; call(dr.copy);
        local_get(BASE); i32_const(OFF_TMP as i32); i32_add; i32_const(1); call(dr.shl);
        local_get(BASE); i32_const(OFF_TMP as i32); i32_add; local_get(BASE); i32_const(OFF_S as i32); i32_add; call(dr.cmp);
        local_set(D);
        i32_const(0); local_set(ROUND);
        local_get(D); i32_const(0); i32_gt_s;
        if_empty;
            i32_const(1); local_set(ROUND);                  // 2R > S → up
        else_;
            local_get(D); i32_eqz;
            if_empty;
                // exact half: round to even — up iff the last slot's digit is odd.
                // (total >= n >= 0; when total == 0, n == 0 and k <= 0, the units digit
                // is the implicit '0' → even → keep.)
                local_get(TOTAL); i32_eqz;
                if_empty;
                    i32_const(0); local_set(ROUND);
                else_;
                    local_get(DP); local_get(TOTAL); i32_const(1); i32_sub; i32_add; i32_load8_u(0);
                    i32_const(1); i32_and; local_set(ROUND);
                end;
            end;
        end;
    });

    // ── Apply round-up carry over digits[0..total]; overflow prepends '1', k++. ──
    wasm!(f, {
        local_get(ROUND);
        if_empty;
            local_get(TOTAL); local_set(I);
            block_empty; loop_empty;
                local_get(I); i32_eqz;
                if_empty;
                    // carry out of the most-significant slot: shift right by 1, set
                    // digits[0]='1', total++, k++ (a new leading integer digit).
                    local_get(DP); i32_const(1); i32_add; local_get(DP); local_get(TOTAL); memory_copy;
                    local_get(DP); i32_const(49); i32_store8(0);
                    local_get(TOTAL); i32_const(1); i32_add; local_set(TOTAL);
                    local_get(K); i32_const(1); i32_add; local_set(K);
                    br(2);
                end;
                local_get(I); i32_const(1); i32_sub; local_set(I);
                local_get(DP); local_get(I); i32_add; i32_load8_u(0); i32_const(57); i32_eq;
                if_empty;
                    local_get(DP); local_get(I); i32_add; i32_const(48); i32_store8(0);
                    br(1);
                else_;
                    local_get(DP); local_get(I); i32_add;
                    local_get(DP); local_get(I); i32_add; i32_load8_u(0); i32_const(1); i32_add;
                    i32_store8(0);
                    br(2);
                end;
            end; end;
        end;
    });
    // digits[0..total] now hold the rounded result: when k>0 the first k are integer
    // digits and the next n are the fraction; when k<=0 all `total`(=n) are fraction.

    // ── Compute out_len & render `[-]int.frac` ──
    // Layout cases mirror Rust format!("{:.n}"):
    //   k <= 0:  "[-]0." + (-k zeros) + (k+n digits)            (frac total = n)
    //   k >= 1, n == 0: "[-]" + (k int digits)                  (no point)
    //   k >= 1, n >= 1: "[-]" + (k int digits) + "." + (n frac digits)
    // Note dlen == (k>0 ? k : 0) + n after carry handling.
    wasm!(f, {
        // out_len = neg + body
        local_get(NEG);
        local_get(K); i32_const(0); i32_le_s;
        if_i32;
            // 2 ("0.") + (-k) + (k+n) == 2 + n ; but if n==0 then k<=0 means value<1 rounded:
            // Rust prints "0" with no point when n==0 and value<1 (e.g. 0.5@0="0").
            local_get(N); i32_eqz;
            if_i32;
                i32_const(1);                 // just "0"
            else_;
                i32_const(2); local_get(N); i32_add;   // "0." + n frac
            end;
        else_;
            local_get(N); i32_eqz;
            if_i32;
                local_get(K);                 // k integer digits
            else_;
                local_get(K); i32_const(1); i32_add; local_get(N); i32_add;  // k + "." + n
            end;
        end;
        i32_add; local_set(OUT_LEN);
    });

    // alloc string [len][cap][data...]
    wasm!(f, {
        local_get(OUT_LEN); i32_const(string_hdr() as i32); i32_add;
        call(emitter.rt.alloc); local_set(RESULT);
        local_get(RESULT); local_get(OUT_LEN); i32_store(0);
        local_get(RESULT); local_get(OUT_LEN); i32_store(string_cap_off() as u32, 0);
        i32_const(0); local_set(CURSOR);
        // sign
        local_get(NEG);
        if_empty;
            local_get(RESULT); i32_const(string_data_off() as i32); i32_add; local_get(CURSOR); i32_add; i32_const(45); i32_store8(0);
            local_get(CURSOR); i32_const(1); i32_add; local_set(CURSOR);
        end;
    });

    // branch on k
    wasm!(f, {
        local_get(K); i32_const(0); i32_le_s;
        if_empty;
            // value < 1 (after rounding). If n == 0 → just "0".
            local_get(RESULT); i32_const(string_data_off() as i32); i32_add; local_get(CURSOR); i32_add; i32_const(48); i32_store8(0);
            local_get(CURSOR); i32_const(1); i32_add; local_set(CURSOR);
            local_get(N); i32_eqz;
            if_empty;
                // done: just "0"
            else_;
                // '.' then all `total`(==n) fraction digits — the leading zeros are
                // already materialized in the buffer, so this is a straight copy.
                local_get(RESULT); i32_const(string_data_off() as i32); i32_add; local_get(CURSOR); i32_add; i32_const(46); i32_store8(0);
                local_get(CURSOR); i32_const(1); i32_add; local_set(CURSOR);
                i32_const(0); local_set(I);
                block_empty; loop_empty;
                    local_get(I); local_get(TOTAL); i32_ge_s; br_if(1);
                    local_get(RESULT); i32_const(string_data_off() as i32); i32_add; local_get(CURSOR); i32_add;
                    local_get(DP); local_get(I); i32_add; i32_load8_u(0); i32_store8(0);
                    local_get(CURSOR); i32_const(1); i32_add; local_set(CURSOR);
                    local_get(I); i32_const(1); i32_add; local_set(I);
                    br(0);
                end; end;
            end;
        else_;
            // value >= 1: k integer digits = digits[0..k], then (n>0) "." + digits[k..k+n]
            i32_const(0); local_set(I);
            block_empty; loop_empty;
                local_get(I); local_get(K); i32_ge_s; br_if(1);
                local_get(RESULT); i32_const(string_data_off() as i32); i32_add; local_get(CURSOR); i32_add;
                local_get(DP); local_get(I); i32_add; i32_load8_u(0); i32_store8(0);
                local_get(CURSOR); i32_const(1); i32_add; local_set(CURSOR);
                local_get(I); i32_const(1); i32_add; local_set(I);
                br(0);
            end; end;
            local_get(N); i32_eqz;
            if_empty; else_;
                local_get(RESULT); i32_const(string_data_off() as i32); i32_add; local_get(CURSOR); i32_add; i32_const(46); i32_store8(0);
                local_get(CURSOR); i32_const(1); i32_add; local_set(CURSOR);
                // digits[k .. total]  (I continues from k)
                block_empty; loop_empty;
                    local_get(I); local_get(TOTAL); i32_ge_s; br_if(1);
                    local_get(RESULT); i32_const(string_data_off() as i32); i32_add; local_get(CURSOR); i32_add;
                    local_get(DP); local_get(I); i32_add; i32_load8_u(0); i32_store8(0);
                    local_get(CURSOR); i32_const(1); i32_add; local_set(CURSOR);
                    local_get(I); i32_const(1); i32_add; local_set(I);
                    br(0);
                end; end;
            end;
        end;
    });

    wasm!(f, { local_get(RESULT); end; });
    emitter.add_compiled(CompiledFunc::tracked_for(emitter.rt.float_to_fixed, type_idx, f));
}

/// Set bignum at (base+off) to the i64 value in `loc` (two u32 limbs). Standalone
/// twin of `DragonRefs::set_u64` for callers that keep the scratch base in a local
/// OTHER than 1 (to_fixed keeps it in BASE because local 1 is its `decimals` param).
fn set_bn_u64(f: &mut Function, base: u32, off: u32, loc: u32) {
    wasm!(f, {
        local_get(base); i32_const((off + BN_HDR) as i32); i32_add; local_get(loc); i32_wrap_i64; i32_store(0);
        local_get(base); i32_const((off + BN_HDR + 4) as i32); i32_add; local_get(loc); i64_const(32); i64_shr_u; i32_wrap_i64; i32_store(0);
        local_get(base); i32_const(off as i32); i32_add;
        local_get(loc); i64_const(32); i64_shr_u; i64_eqz; if_i32; i32_const(1); else_; i32_const(2); end;
        i32_store(0);
    });
}
/// Set bignum at (base+off) to a small u32 constant (1 limb).
fn set_bn_small(f: &mut Function, base: u32, off: u32, v: u32) {
    wasm!(f, {
        local_get(base); i32_const(off as i32); i32_add; i32_const(1); i32_store(0);
        local_get(base); i32_const((off + BN_HDR) as i32); i32_add; i32_const(v as i32); i32_store(0);
    });
}

/// Bundle of helper func indices + emit-time conveniences for the driver.
struct DragonRefs {
    mul_small: u32,
    cmp: u32,
    add: u32,
    sub: u32,
    shl: u32,
    copy: u32,
}

impl DragonRefs {
    fn new(emitter: &WasmEmitter) -> DragonRefs {
        DragonRefs {
            mul_small: emitter.rt.dragon.mul_small,
            cmp: emitter.rt.dragon.cmp,
            add: emitter.rt.dragon.add,
            sub: emitter.rt.dragon.sub,
            shl: emitter.rt.dragon.shl,
            copy: emitter.rt.dragon.copy,
        }
    }
    // base ptr is in local 1; absolute ptr of bignum at offset `off` = base + off.
    fn ptr(&self, f: &mut Function, off: u32) {
        wasm!(f, { local_get(1); i32_const(off as i32); i32_add; });
    }
    /// bignum[off] = the i64 in local `loc` (split into two u32 limbs).
    fn set_u64(&self, f: &mut Function, off: u32, loc: u32) {
        // len: if hi != 0 -> 2 else 1
        wasm!(f, {
            // limb0 = (val & 0xFFFFFFFF)
            local_get(1); i32_const((off + BN_HDR) as i32); i32_add;
            local_get(loc); i32_wrap_i64; i32_store(0);
            // limb1 = (val >> 32)
            local_get(1); i32_const((off + BN_HDR + 4) as i32); i32_add;
            local_get(loc); i64_const(32); i64_shr_u; i32_wrap_i64; i32_store(0);
            // len = (hi != 0) ? 2 : 1
            local_get(1); i32_const(off as i32); i32_add;
            local_get(loc); i64_const(32); i64_shr_u; i64_eqz;
            if_i32; i32_const(1); else_; i32_const(2); end;
            i32_store(0);
        });
    }
    /// bignum[off] = small constant (1 limb).
    fn set_small(&self, f: &mut Function, off: u32, v: u32) {
        wasm!(f, {
            local_get(1); i32_const(off as i32); i32_add; i32_const(1); i32_store(0); // len = 1
            local_get(1); i32_const((off + BN_HDR) as i32); i32_add; i32_const(v as i32); i32_store(0);
        });
    }
    fn copy(&self, f: &mut Function, dst: u32, src: u32) {
        self.ptr(f, dst); self.ptr(f, src);
        wasm!(f, { call(self.copy); });
    }
    fn add(&self, f: &mut Function, dst: u32, src: u32) {
        self.ptr(f, dst); self.ptr(f, src);
        wasm!(f, { call(self.add); });
    }
    fn sub(&self, f: &mut Function, dst: u32, src: u32) {
        self.ptr(f, dst); self.ptr(f, src);
        wasm!(f, { call(self.sub); });
    }
    fn cmp(&self, f: &mut Function, a: u32, b: u32) {
        self.ptr(f, a); self.ptr(f, b);
        wasm!(f, { call(self.cmp); });
    }
    /// cmp(a, b) and store the -1/0/1 result into local 23 (`cmp_tmp`).
    fn cmp_set(&self, f: &mut Function, a: u32, b: u32) {
        self.cmp(f, a, b);
        wasm!(f, { local_set(23); });
    }
    /// Push the boolean `even ? (cmp_tmp >= 0) : (cmp_tmp > 0)`.
    /// (Used where the rounding interval is closed for even mantissas: the
    /// "upper" predicate.)  = (c > 0) | (even & (c == 0)).
    fn pred_ge_gt(&self, f: &mut Function) {
        wasm!(f, {
            local_get(23); i32_const(0); i32_gt_s;
            local_get(8); local_get(23); i32_eqz; i32_and;
            i32_or;
        });
    }
    /// Push the boolean `even ? (cmp_tmp <= 0) : (cmp_tmp < 0)`.
    /// (The "lower" predicate.)  = (c < 0) | (even & (c == 0)).
    fn pred_le_lt(&self, f: &mut Function) {
        wasm!(f, {
            local_get(23); i32_const(0); i32_lt_s;
            local_get(8); local_get(23); i32_eqz; i32_and;
            i32_or;
        });
    }
    fn mul10(&self, f: &mut Function, off: u32) {
        self.ptr(f, off);
        wasm!(f, { i32_const(10); call(self.mul_small); });
    }
    /// shl by a constant bit count.
    fn shl_const(&self, f: &mut Function, off: u32, bits: u32) {
        self.ptr(f, off);
        wasm!(f, { i32_const(bits as i32); call(self.shl); });
    }
    /// shl by the value in local `loc` (an i32).
    fn shl_local(&self, f: &mut Function, off: u32, loc: u32) {
        self.ptr(f, off);
        wasm!(f, { local_get(loc); call(self.shl); });
    }
    /// shl by (local + imm).
    fn shl_imm_local(&self, f: &mut Function, off: u32, loc: u32, imm: i32) {
        self.ptr(f, off);
        wasm!(f, { local_get(loc); i32_const(imm); i32_add; call(self.shl); });
    }
    /// shl by (1 - local).
    fn shl_one_minus_e(&self, f: &mut Function, off: u32, loc: u32) {
        self.ptr(f, off);
        wasm!(f, { i32_const(1); local_get(loc); i32_sub; call(self.shl); });
    }
}

/// log10(2), used to estimate the decimal exponent from the binary one.
const LOG10_2: f64 = core::f64::consts::LOG10_2;
