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

include!("rt_dragon_p2.rs");
include!("rt_dragon_p3.rs");
