//! gelu — activation: `0.5·x·(1 + tanh(K·(x + 0.044715·x³)))`, K=√(2/π).
//! tanh is the autovec wall; `tanh(y) = 1 - 2/(exp(2y)+1)` reuses the shared SIMD
//! fast-exp. within-tolerance (exp approx). The芋づる: one fast-exp, every exp/tanh
//! op falls.

#[cfg(target_arch = "x86_64")]
use crate::silu::exp_pd;

const K: f64 = 0.7978845608028654; // sqrt(2/pi)

pub fn gelu_naive(data: &[f64], out: &mut [f64]) {
    for i in 0..data.len() {
        let x = data[i];
        let inner = K * (x + 0.044715 * x * x * x);
        out[i] = 0.5 * x * (1.0 + inner.tanh());
    }
}

#[cfg(target_arch = "x86_64")]
#[target_feature(enable = "avx2,fma")]
unsafe fn gelu_avx(data: &[f64], out: &mut [f64]) {
    use std::arch::x86_64::*;
    let k = _mm256_set1_pd(K);
    let c = _mm256_set1_pd(0.044715);
    let half = _mm256_set1_pd(0.5);
    let one = _mm256_set1_pd(1.0);
    let two = _mm256_set1_pd(2.0);
    let n = data.len();
    let chunks = n / 4;
    for ci in 0..chunks {
        let off = ci * 4;
        let x = _mm256_loadu_pd(data.as_ptr().add(off));
        let x3 = _mm256_mul_pd(_mm256_mul_pd(x, x), x);
        // inner = K*(x + 0.044715*x3)
        let inner = _mm256_mul_pd(k, _mm256_fmadd_pd(c, x3, x));
        // tanh(inner) = 1 - 2/(exp(2*inner)+1)
        let e = exp_pd(_mm256_mul_pd(two, inner));
        let t = _mm256_sub_pd(one, _mm256_div_pd(two, _mm256_add_pd(e, one)));
        // 0.5*x*(1+t)
        let r = _mm256_mul_pd(_mm256_mul_pd(half, x), _mm256_add_pd(one, t));
        _mm256_storeu_pd(out.as_mut_ptr().add(off), r);
    }
    for i in (chunks * 4)..n {
        let x = data[i];
        let inner = K * (x + 0.044715 * x * x * x);
        out[i] = 0.5 * x * (1.0 + inner.tanh());
    }
}

#[cfg(all(target_arch = "wasm32", target_feature = "simd128"))]
fn gelu_wasm(data: &[f64], out: &mut [f64]) {
    use crate::silu::exp_pd_wasm;
    use std::arch::wasm32::*;
    let k = f64x2_splat(K);
    let c = f64x2_splat(0.044715);
    let half = f64x2_splat(0.5);
    let one = f64x2_splat(1.0);
    let two = f64x2_splat(2.0);
    let n = data.len();
    let chunks = n / 2;
    for ci in 0..chunks {
        let off = ci * 2;
        let x = unsafe { v128_load(data.as_ptr().add(off) as *const v128) };
        let x3 = f64x2_mul(f64x2_mul(x, x), x);
        let inner = f64x2_mul(k, f64x2_add(x, f64x2_mul(c, x3)));
        let e = exp_pd_wasm(f64x2_mul(two, inner));
        let t = f64x2_sub(one, f64x2_div(two, f64x2_add(e, one)));
        let r = f64x2_mul(f64x2_mul(half, x), f64x2_add(one, t));
        unsafe { v128_store(out.as_mut_ptr().add(off) as *mut v128, r) };
    }
    for i in (chunks * 2)..n {
        let x = data[i];
        let inner = K * (x + 0.044715 * x * x * x);
        out[i] = 0.5 * x * (1.0 + inner.tanh());
    }
}

#[cfg(target_arch = "aarch64")]
#[target_feature(enable = "neon")]
unsafe fn gelu_neon(data: &[f64], out: &mut [f64]) {
    use crate::silu::exp_pd_neon;
    use std::arch::aarch64::*;
    let k = vdupq_n_f64(K);
    let c = vdupq_n_f64(0.044715);
    let half = vdupq_n_f64(0.5);
    let one = vdupq_n_f64(1.0);
    let two = vdupq_n_f64(2.0);
    let n = data.len();
    let chunks = n / 2;
    for ci in 0..chunks {
        let off = ci * 2;
        let x = vld1q_f64(data.as_ptr().add(off));
        let x3 = vmulq_f64(vmulq_f64(x, x), x);
        let inner = vmulq_f64(k, vaddq_f64(x, vmulq_f64(c, x3)));
        let e = exp_pd_neon(vmulq_f64(two, inner));
        let t = vsubq_f64(one, vdivq_f64(two, vaddq_f64(e, one)));
        let r = vmulq_f64(vmulq_f64(half, x), vaddq_f64(one, t));
        vst1q_f64(out.as_mut_ptr().add(off), r);
    }
    for i in (chunks * 2)..n {
        let x = data[i];
        let inner = K * (x + 0.044715 * x * x * x);
        out[i] = 0.5 * x * (1.0 + inner.tanh());
    }
}

pub fn gelu(data: &[f64], out: &mut [f64]) {
    assert_eq!(data.len(), out.len());
    #[cfg(target_arch = "x86_64")]
    {
        if is_x86_feature_detected!("avx2") && is_x86_feature_detected!("fma") {
            return unsafe { gelu_avx(data, out) };
        }
    }
    #[cfg(target_arch = "aarch64")]
    {
        if std::arch::is_aarch64_feature_detected!("neon") {
            return unsafe { gelu_neon(data, out) };
        }
    }
    #[cfg(all(target_arch = "wasm32", target_feature = "simd128"))]
    {
        return gelu_wasm(data, out);
    }
    gelu_naive(data, out);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn gelu_matches_libm_within_tolerance() {
        let n = 1024;
        let data: Vec<f64> = (0..n).map(|k| (k as f64) * 0.013 - 6.5).collect();
        let mut simd = vec![0.0; n];
        let mut naive = vec![0.0; n];
        gelu(&data, &mut simd);
        gelu_naive(&data, &mut naive);
        for i in 0..n {
            let tol = 1e-6 * naive[i].abs().max(1.0);
            assert!((simd[i] - naive[i]).abs() <= tol, "i={i}: {} vs {}", simd[i], naive[i]);
        }
    }
}
