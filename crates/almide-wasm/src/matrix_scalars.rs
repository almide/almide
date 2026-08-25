//! Scalar math helper bodies for the matrix kernels: the CANONICAL
//! fast-exp (#1197, transcribed op-for-op from almide-kernel silu.rs —
//! deliberately UNFUSED, `f64.nearest` rounds ties to even) and the
//! tanh-approximation gelu built on it. Both are emitted helpers so
//! every kernel shares ONE body — bit-identity across call sites is
//! structural, not a discipline.

use wasm_encoder::{BlockType, Function, ValType};

/// `$fast_exp(x) -> f64` — exp(x) = 2^k · P6(r), k = nearest(x·log2e),
/// r = x − k·ln2; clamped to ±708 (the biased exponent must stay inside
/// (0, 2046)). NaN input follows the native path: the clamps keep NaN,
/// k = trunc_sat(NaN) = 0, and the polynomial propagates NaN.
pub(crate) fn emit_fast_exp() -> Function {
    let (x, kf, r) = (0u32, 1u32, 2u32);
    let mut f = Function::new([(2, ValType::F64)]);
    let mut i = f.instructions();
    // x = fmin(fmax(x0, -708), 708)
    i.local_get(x).f64_const((-708.0f64).into()).f64_max();
    i.f64_const(708.0f64.into()).f64_min().local_set(x);
    // kf = nearest(x * log2e)
    i.local_get(x).f64_const(std::f64::consts::LOG2_E.into()).f64_mul().f64_nearest();
    i.local_set(kf);
    // r = x - kf * ln2
    i.local_get(x);
    i.local_get(kf).f64_const(std::f64::consts::LN_2.into()).f64_mul();
    i.f64_sub().local_set(r);
    // Horner, degree 6, UNFUSED (separate mul then add each step)
    i.f64_const(0.001_388_888_888_888_889f64.into());
    for c in [
        0.008_333_333_333_333_333f64,
        0.041_666_666_666_666_664,
        0.166_666_666_666_666_66,
        0.5,
        1.0,
        1.0,
    ] {
        i.local_get(r).f64_mul();
        i.f64_const(c.into()).f64_add();
    }
    // * 2^k via bits: ((trunc(kf) + 1023) << 52)
    i.local_get(kf).i64_trunc_sat_f64_s().i64_const(1023).i64_add();
    i.i64_const(52).i64_shl().f64_reinterpret_i64();
    i.f64_mul();
    i.end();
    f
}

/// `$q10_val(data: i32, off: i64, k: i64) -> f64` — one Q1_0 weight on
/// the global-k schedule: 18-byte blocks of [fp16 scale][16 sign-bit
/// bytes]; an element whose block leaves the data region is 0.0 (#1532
/// per-element bound); the sign spells `0.0 - scale` (not neg) and the
/// dequant zero normalizes -0.0 → +0.0 (dq_zero).
pub(crate) fn emit_q10_val(f16_to_f64: u32) -> Function {
    use wasm_encoder::MemArg;
    let (data, off, k, bs, v) = (0u32, 1u32, 2u32, 3u32, 4u32);
    let raw = |extra: u64| MemArg {
        offset: u64::from(almide_layout::PAYLOAD) + extra,
        align: 0,
        memory_index: 0,
    };
    let mut f = Function::new([(1, ValType::I64), (1, ValType::F64)]);
    let mut i = f.instructions();
    // bs = (k >> 7) * 18
    i.local_get(k).i64_const(7).i64_shr_u().i64_const(18).i64_mul().local_set(bs);
    // off + bs + 18 > len → 0.0
    i.local_get(off).local_get(bs).i64_add().i64_const(18).i64_add();
    i.local_get(data)
        .i32_load(MemArg { offset: 4, align: 2, memory_index: 0 })
        .i64_extend_i32_u();
    i.i64_gt_s().if_(BlockType::Empty);
    i.f64_const(0.0f64.into()).return_();
    i.end();
    // scale = f16(load16(data + off + bs))
    i.local_get(data).local_get(off).local_get(bs).i64_add().i32_wrap_i64().i32_add();
    i.i32_load16_u(raw(0)).call(f16_to_f64).local_set(v);
    // bit = (load8(data + off + bs + 2 + ((k>>3)&15)) >> (k&7)) & 1
    i.local_get(data).local_get(off).local_get(bs).i64_add().i32_wrap_i64().i32_add();
    i.local_get(k).i64_const(3).i64_shr_u().i64_const(15).i64_and().i32_wrap_i64().i32_add();
    i.i32_load8_u(raw(2));
    i.local_get(k).i64_const(7).i64_and().i32_wrap_i64().i32_shr_u();
    i.i32_const(1).i32_and().i32_const(1).i32_eq().if_(BlockType::Result(ValType::F64));
    i.local_get(v);
    i.else_();
    i.f64_const(0.0f64.into()).local_get(v).f64_sub();
    i.end();
    i.local_set(v);
    // dq_zero: v == 0.0 → +0.0
    i.local_get(v).f64_const(0.0f64.into()).f64_eq().if_(BlockType::Result(ValType::F64));
    i.f64_const(0.0f64.into());
    i.else_();
    i.local_get(v);
    i.end();
    i.end();
    f
}

/// `$gelu(x) -> f64` — the tanh approximation in the NATIVE op order
/// (all four native lanes agree): inner = √(2/π)·(x + 0.044715·((x·x)·x)),
/// t = 1 − 2/(e^{2·inner} + 1), y = (0.5·x)·(1 + t) — halve FIRST
/// (doubling first overflows near f64::MAX where native stays finite).
pub(crate) fn emit_gelu_scalar(fast_exp: u32) -> Function {
    let (x, t) = (0u32, 1u32);
    let mut f = Function::new([(1, ValType::F64)]);
    let mut i = f.instructions();
    // inner = 0.7978845608028654 * (x + 0.044715 * ((x*x)*x))
    i.local_get(x);
    i.f64_const(0.044_715f64.into());
    i.local_get(x).local_get(x).f64_mul().local_get(x).f64_mul();
    i.f64_mul().f64_add();
    i.f64_const(0.797_884_560_802_865_4f64.into()).f64_mul();
    // e2 = fast_exp(2.0 * inner)
    i.f64_const(2.0f64.into()).f64_mul().call(fast_exp).local_set(t);
    // t = 1.0 - 2.0 / (e2 + 1.0)
    i.f64_const(1.0f64.into());
    i.f64_const(2.0f64.into());
    i.local_get(t).f64_const(1.0f64.into()).f64_add();
    i.f64_div().f64_sub().local_set(t);
    // (0.5 * x) * (1.0 + t)
    i.f64_const(0.5f64.into()).local_get(x).f64_mul();
    i.f64_const(1.0f64.into()).local_get(t).f64_add();
    i.f64_mul();
    i.end();
    f
}
