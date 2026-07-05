//! silu_mul — swiglu / FFN core: `out[i] = silu(a[i]) * b[i]`,
//! `silu(x) = x * sigmoid(x) = x / (1 + exp(-x))`.
//!
//! `exp` is the autovec wall: rustc/Almide emit a scalar libm call per element,
//! which can't vectorize. A SIMD fast-exp (range reduction + Taylor degree-6,
//! f64 AVX) lets almide-kernel beat the scalar path. Correctness is
//! within-tolerance (exp is approximated ~1e-7 relative, fine for an activation).

/// Naive reference: scalar libm exp.
pub fn silu_mul_naive(a: &[f64], b: &[f64], out: &mut [f64]) {
    for i in 0..a.len() {
        let x = a[i];
        let sig = 1.0 / (1.0 + (-x).exp());
        out[i] = x * sig * b[i];
    }
}

/// SIMD fast exp for f64x4: exp(x) = 2^k · exp(r), k = round(x/ln2),
/// r = x - k·ln2 ∈ [-ln2/2, ln2/2], exp(r) by Taylor degree 6.
/// SIMD fast exp, shared by silu/softmax/gelu — every op that hits the exp wall.
#[cfg(target_arch = "x86_64")]
#[target_feature(enable = "avx2,fma")]
pub(crate) unsafe fn exp_pd(x: std::arch::x86_64::__m256d) -> std::arch::x86_64::__m256d {
    use std::arch::x86_64::*;
    // Clamp to the bit-trick's valid range: `(k + 1023) << 52` needs the biased
    // exponent in (0, 2046), i.e. |x| ≲ 708 (min-normal 2^-1022 .. max 2^1023).
    // Outside it the shift WRAPS and exp returns garbage — the standard softmax
    // mask value -1e9 corrupted whole rows (nn masked attention). Clamping to
    // ±708 saturates to ~3e-308 / ~8e307, which every consumer (softmax weight,
    // silu/gelu sigmoid) treats identically to 0 / inf.
    let x = _mm256_max_pd(_mm256_set1_pd(-708.0), _mm256_min_pd(x, _mm256_set1_pd(708.0)));
    let log2e = _mm256_set1_pd(std::f64::consts::LOG2_E);
    let ln2 = _mm256_set1_pd(std::f64::consts::LN_2);
    // k = round(x * log2e)
    let kf = _mm256_round_pd::<{ _MM_FROUND_TO_NEAREST_INT | _MM_FROUND_NO_EXC }>(
        _mm256_mul_pd(x, log2e),
    );
    let r = _mm256_fnmadd_pd(kf, ln2, x); // r = x - kf*ln2
    // Taylor exp(r), Horner: ((((((1/720)r + 1/120)r + 1/24)r + 1/6)r + 1/2)r + 1)r + 1
    let mut p = _mm256_set1_pd(1.0 / 720.0);
    p = _mm256_fmadd_pd(p, r, _mm256_set1_pd(1.0 / 120.0));
    p = _mm256_fmadd_pd(p, r, _mm256_set1_pd(1.0 / 24.0));
    p = _mm256_fmadd_pd(p, r, _mm256_set1_pd(1.0 / 6.0));
    p = _mm256_fmadd_pd(p, r, _mm256_set1_pd(0.5));
    p = _mm256_fmadd_pd(p, r, _mm256_set1_pd(1.0));
    p = _mm256_fmadd_pd(p, r, _mm256_set1_pd(1.0)); // exp(r)
    // 2^k via exponent bits: ((k + 1023) << 52)
    let ki = _mm256_cvtpd_epi32(kf); // __m128i, 4×i32
    let k64 = _mm256_cvtepi32_epi64(ki); // 4×i64
    let biased = _mm256_add_epi64(k64, _mm256_set1_epi64x(1023));
    let pow2k = _mm256_castsi256_pd(_mm256_slli_epi64(biased, 52));
    _mm256_mul_pd(p, pow2k)
}

/// SIMD silu_mul (AVX2+FMA): sigmoid via fast exp, fused with x and b.
#[cfg(target_arch = "x86_64")]
#[target_feature(enable = "avx2,fma")]
unsafe fn silu_mul_avx(a: &[f64], b: &[f64], out: &mut [f64]) {
    use std::arch::x86_64::*;
    let n = a.len();
    let one = _mm256_set1_pd(1.0);
    let chunks = n / 4;
    for c in 0..chunks {
        let off = c * 4;
        let x = _mm256_loadu_pd(a.as_ptr().add(off));
        let bv = _mm256_loadu_pd(b.as_ptr().add(off));
        // sigmoid(x) = 1/(1+exp(-x))
        let neg = _mm256_sub_pd(_mm256_setzero_pd(), x);
        let e = exp_pd(neg);
        let sig = _mm256_div_pd(one, _mm256_add_pd(one, e));
        // out = x * sig * b
        let r = _mm256_mul_pd(_mm256_mul_pd(x, sig), bv);
        _mm256_storeu_pd(out.as_mut_ptr().add(off), r);
    }
    for i in (chunks * 4)..n {
        let x = a[i];
        let sig = 1.0 / (1.0 + (-x).exp());
        out[i] = x * sig * b[i];
    }
}

/// SIMD fast exp for wasm f64x2 (same algorithm, 2-wide). Shared by silu/softmax/
/// gelu on wasm. f64x2→i64 via i32 trunc then sign-extend (simd128 has no direct
/// f64→i64).
#[cfg(all(target_arch = "wasm32", target_feature = "simd128"))]
pub(crate) fn exp_pd_wasm(x: std::arch::wasm32::v128) -> std::arch::wasm32::v128 {
    use std::arch::wasm32::*;
    // Clamp to the bit-trick's valid range (see exp_pd): outside |x| ≲ 708 the
    // `(k + 1023) << 52` shift wraps and returns garbage (softmax -1e9 mask).
    let x = f64x2_max(f64x2_splat(-708.0), f64x2_min(x, f64x2_splat(708.0)));
    let log2e = f64x2_splat(std::f64::consts::LOG2_E);
    let ln2 = f64x2_splat(std::f64::consts::LN_2);
    let kf = f64x2_nearest(f64x2_mul(x, log2e));
    let r = f64x2_sub(x, f64x2_mul(kf, ln2));
    // Taylor exp(r), Horner
    let mut p = f64x2_splat(1.0 / 720.0);
    p = f64x2_add(f64x2_mul(p, r), f64x2_splat(1.0 / 120.0));
    p = f64x2_add(f64x2_mul(p, r), f64x2_splat(1.0 / 24.0));
    p = f64x2_add(f64x2_mul(p, r), f64x2_splat(1.0 / 6.0));
    p = f64x2_add(f64x2_mul(p, r), f64x2_splat(0.5));
    p = f64x2_add(f64x2_mul(p, r), f64x2_splat(1.0));
    p = f64x2_add(f64x2_mul(p, r), f64x2_splat(1.0)); // exp(r)
    // 2^k: k = kf truncated, (k+1023) << 52
    let ki32 = i32x4_trunc_sat_f64x2_zero(kf); // low 2 lanes = k
    let ki64 = i64x2_extend_low_i32x4(ki32);
    let biased = i64x2_add(ki64, i64x2_splat(1023));
    let pow2k = i64x2_shl(biased, 52); // reinterpreted as f64x2 bits
    f64x2_mul(p, pow2k)
}

#[cfg(all(target_arch = "wasm32", target_feature = "simd128"))]
fn silu_mul_wasm(a: &[f64], b: &[f64], out: &mut [f64]) {
    use std::arch::wasm32::*;
    let one = f64x2_splat(1.0);
    let n = a.len();
    let chunks = n / 2;
    for c in 0..chunks {
        let off = c * 2;
        // SAFETY: off+2 <= n.
        let x = unsafe { v128_load(a.as_ptr().add(off) as *const v128) };
        let bv = unsafe { v128_load(b.as_ptr().add(off) as *const v128) };
        let e = exp_pd_wasm(f64x2_neg(x));
        let sig = f64x2_div(one, f64x2_add(one, e));
        let r = f64x2_mul(f64x2_mul(x, sig), bv);
        unsafe { v128_store(out.as_mut_ptr().add(off) as *mut v128, r) };
    }
    for i in (chunks * 2)..n {
        let x = a[i];
        let sig = 1.0 / (1.0 + (-x).exp());
        out[i] = x * sig * b[i];
    }
}

/// SIMD fast exp for ARM NEON (float64x2_t, 2-wide). Shared by silu/softmax/gelu
/// on aarch64. NEON has native FMA and f64→i64 (vcvtq_s64_f64), so it's closer to
/// the AVX version than wasm's.
#[cfg(target_arch = "aarch64")]
#[target_feature(enable = "neon")]
pub(crate) unsafe fn exp_pd_neon(
    x: std::arch::aarch64::float64x2_t,
) -> std::arch::aarch64::float64x2_t {
    use std::arch::aarch64::*;
    // Clamp to the bit-trick's valid range (see exp_pd): outside |x| ≲ 708 the
    // `(k + 1023) << 52` shift wraps and returns garbage (softmax -1e9 mask).
    let x = vmaxq_f64(vdupq_n_f64(-708.0), vminq_f64(x, vdupq_n_f64(708.0)));
    let log2e = vdupq_n_f64(std::f64::consts::LOG2_E);
    let ln2 = vdupq_n_f64(std::f64::consts::LN_2);
    let kf = vrndnq_f64(vmulq_f64(x, log2e)); // round to nearest
    let r = vsubq_f64(x, vmulq_f64(kf, ln2));
    // Taylor exp(r), Horner via fma: vfmaq_f64(acc, p, r) = acc + p*r
    let mut p = vdupq_n_f64(1.0 / 720.0);
    p = vfmaq_f64(vdupq_n_f64(1.0 / 120.0), p, r);
    p = vfmaq_f64(vdupq_n_f64(1.0 / 24.0), p, r);
    p = vfmaq_f64(vdupq_n_f64(1.0 / 6.0), p, r);
    p = vfmaq_f64(vdupq_n_f64(0.5), p, r);
    p = vfmaq_f64(vdupq_n_f64(1.0), p, r);
    p = vfmaq_f64(vdupq_n_f64(1.0), p, r); // exp(r)
    // 2^k: k = round(kf) as i64, (k+1023) << 52, reinterpret as f64
    let k = vcvtq_s64_f64(kf);
    let biased = vaddq_s64(k, vdupq_n_s64(1023));
    let pow2k = vshlq_n_s64::<52>(biased);
    vmulq_f64(p, vreinterpretq_f64_s64(pow2k))
}

#[cfg(target_arch = "aarch64")]
#[target_feature(enable = "neon")]
unsafe fn silu_mul_neon(a: &[f64], b: &[f64], out: &mut [f64]) {
    use std::arch::aarch64::*;
    let one = vdupq_n_f64(1.0);
    let n = a.len();
    let chunks = n / 2;
    for c in 0..chunks {
        let off = c * 2;
        let x = vld1q_f64(a.as_ptr().add(off));
        let bv = vld1q_f64(b.as_ptr().add(off));
        let e = exp_pd_neon(vnegq_f64(x));
        let sig = vdivq_f64(one, vaddq_f64(one, e));
        let r = vmulq_f64(vmulq_f64(x, sig), bv);
        vst1q_f64(out.as_mut_ptr().add(off), r);
    }
    for i in (chunks * 2)..n {
        let x = a[i];
        let sig = 1.0 / (1.0 + (-x).exp());
        out[i] = x * sig * b[i];
    }
}

/// Per-target dispatch.
pub fn silu_mul(a: &[f64], b: &[f64], out: &mut [f64]) {
    assert_eq!(a.len(), b.len());
    assert_eq!(a.len(), out.len());
    #[cfg(target_arch = "x86_64")]
    {
        if is_x86_feature_detected!("avx2") && is_x86_feature_detected!("fma") {
            return unsafe { silu_mul_avx(a, b, out) };
        }
    }
    #[cfg(target_arch = "aarch64")]
    {
        if std::arch::is_aarch64_feature_detected!("neon") {
            return unsafe { silu_mul_neon(a, b, out) };
        }
    }
    #[cfg(all(target_arch = "wasm32", target_feature = "simd128"))]
    {
        return silu_mul_wasm(a, b, out);
    }
    silu_mul_naive(a, b, out);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn silu_mul_matches_libm_within_tolerance() {
        let n = 1024;
        let a: Vec<f64> = (0..n).map(|k| (k as f64) * 0.013 - 6.5).collect();
        let b: Vec<f64> = (0..n).map(|k| (k as f64) * 0.007 - 3.0).collect();
        let mut simd = vec![0.0; n];
        let mut naive = vec![0.0; n];
        silu_mul(&a, &b, &mut simd);
        silu_mul_naive(&a, &b, &mut naive);
        for i in 0..n {
            let tol = 1e-6 * naive[i].abs().max(1.0);
            assert!(
                (simd[i] - naive[i]).abs() <= tol,
                "i={i}: simd {} vs libm {} (a={})",
                simd[i],
                naive[i],
                a[i]
            );
        }
    }
}
