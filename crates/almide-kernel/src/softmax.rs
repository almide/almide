//! softmax_rows — attention's core. Per row: `softmax(x)_i = exp(x_i - max) / Σ`.
//! exp is the autovec wall; this reuses the SIMD fast-exp from silu. The reduce
//! (max, sum) is order-dependent, so correctness is within-tolerance (exp approx
//! + reassoc), and each row sums to 1 by construction.

#[cfg(target_arch = "x86_64")]
use crate::silu::exp_pd;

/// Naive reference: scalar libm exp.
pub fn softmax_rows_naive(data: &[f64], rows: usize, cols: usize, out: &mut [f64]) {
    for r in 0..rows {
        let row = &data[r * cols..(r + 1) * cols];
        let o = &mut out[r * cols..(r + 1) * cols];
        let max = row.iter().copied().fold(f64::NEG_INFINITY, f64::max);
        let mut sum = 0.0;
        for j in 0..cols {
            let e = (row[j] - max).exp();
            o[j] = e;
            sum += e;
        }
        let inv = 1.0 / sum;
        for j in 0..cols {
            o[j] *= inv;
        }
    }
}

/// SIMD one row: max (scalar), exp(x-max) (SIMD fast-exp) + lane-sum, then scale.
#[cfg(target_arch = "x86_64")]
#[target_feature(enable = "avx2,fma")]
unsafe fn softmax_row_avx(row: &[f64], o: &mut [f64]) {
    use std::arch::x86_64::*;
    let cols = row.len();
    let max = row.iter().copied().fold(f64::NEG_INFINITY, f64::max);
    let vmax = _mm256_set1_pd(max);
    let mut vsum = _mm256_setzero_pd();
    let chunks = cols / 4;
    for c in 0..chunks {
        let x = _mm256_loadu_pd(row.as_ptr().add(c * 4));
        let e = exp_pd(_mm256_sub_pd(x, vmax));
        _mm256_storeu_pd(o.as_mut_ptr().add(c * 4), e);
        vsum = _mm256_add_pd(vsum, e);
    }
    // horizontal sum of vsum
    let hi = _mm256_extractf128_pd(vsum, 1);
    let lo = _mm256_castpd256_pd128(vsum);
    let s = _mm_add_pd(lo, hi);
    let s = _mm_add_sd(s, _mm_unpackhi_pd(s, s));
    let mut sum = _mm_cvtsd_f64(s);
    for j in (chunks * 4)..cols {
        let e = (row[j] - max).exp();
        o[j] = e;
        sum += e;
    }
    let inv = 1.0 / sum;
    let vinv = _mm256_set1_pd(inv);
    for c in 0..chunks {
        let e = _mm256_loadu_pd(o.as_ptr().add(c * 4));
        _mm256_storeu_pd(o.as_mut_ptr().add(c * 4), _mm256_mul_pd(e, vinv));
    }
    for j in (chunks * 4)..cols {
        o[j] *= inv;
    }
}

/// wasm simd128 one row: max (scalar), exp(x-max) f64x2 + lane-sum, scale.
#[cfg(all(target_arch = "wasm32", target_feature = "simd128"))]
fn softmax_row_wasm(row: &[f64], o: &mut [f64]) {
    use crate::silu::exp_pd_wasm;
    use std::arch::wasm32::*;
    let cols = row.len();
    let max = row.iter().copied().fold(f64::NEG_INFINITY, f64::max);
    let vmax = f64x2_splat(max);
    let mut vsum = f64x2_splat(0.0);
    let chunks = cols / 2;
    for c in 0..chunks {
        let x = unsafe { v128_load(row.as_ptr().add(c * 2) as *const v128) };
        let e = exp_pd_wasm(f64x2_sub(x, vmax));
        unsafe { v128_store(o.as_mut_ptr().add(c * 2) as *mut v128, e) };
        vsum = f64x2_add(vsum, e);
    }
    let mut sum = f64x2_extract_lane::<0>(vsum) + f64x2_extract_lane::<1>(vsum);
    for j in (chunks * 2)..cols {
        let e = (row[j] - max).exp();
        o[j] = e;
        sum += e;
    }
    let inv = 1.0 / sum;
    let vinv = f64x2_splat(inv);
    for c in 0..chunks {
        let e = unsafe { v128_load(o.as_ptr().add(c * 2) as *const v128) };
        unsafe { v128_store(o.as_mut_ptr().add(c * 2) as *mut v128, f64x2_mul(e, vinv)) };
    }
    for j in (chunks * 2)..cols {
        o[j] *= inv;
    }
}

/// ARM NEON one row: max (scalar), exp(x-max) f64x2 + lane-sum, scale.
#[cfg(target_arch = "aarch64")]
#[target_feature(enable = "neon")]
unsafe fn softmax_row_neon(row: &[f64], o: &mut [f64]) {
    use crate::silu::exp_pd_neon;
    use std::arch::aarch64::*;
    let cols = row.len();
    let max = row.iter().copied().fold(f64::NEG_INFINITY, f64::max);
    let vmax = vdupq_n_f64(max);
    let mut vsum = vdupq_n_f64(0.0);
    let chunks = cols / 2;
    for c in 0..chunks {
        let x = vld1q_f64(row.as_ptr().add(c * 2));
        let e = exp_pd_neon(vsubq_f64(x, vmax));
        vst1q_f64(o.as_mut_ptr().add(c * 2), e);
        vsum = vaddq_f64(vsum, e);
    }
    let mut sum = vgetq_lane_f64::<0>(vsum) + vgetq_lane_f64::<1>(vsum);
    for j in (chunks * 2)..cols {
        let e = (row[j] - max).exp();
        o[j] = e;
        sum += e;
    }
    let inv = 1.0 / sum;
    let vinv = vdupq_n_f64(inv);
    for c in 0..chunks {
        let e = vld1q_f64(o.as_ptr().add(c * 2));
        vst1q_f64(o.as_mut_ptr().add(c * 2), vmulq_f64(e, vinv));
    }
    for j in (chunks * 2)..cols {
        o[j] *= inv;
    }
}

/// Per-target dispatch.
pub fn softmax_rows(data: &[f64], rows: usize, cols: usize, out: &mut [f64]) {
    #[cfg(target_arch = "x86_64")]
    {
        if is_x86_feature_detected!("avx2") && is_x86_feature_detected!("fma") {
            for r in 0..rows {
                unsafe {
                    softmax_row_avx(&data[r * cols..(r + 1) * cols], &mut out[r * cols..(r + 1) * cols]);
                }
            }
            return;
        }
    }
    #[cfg(target_arch = "aarch64")]
    {
        if std::arch::is_aarch64_feature_detected!("neon") {
            for r in 0..rows {
                unsafe {
                    softmax_row_neon(&data[r * cols..(r + 1) * cols], &mut out[r * cols..(r + 1) * cols]);
                }
            }
            return;
        }
    }
    #[cfg(all(target_arch = "wasm32", target_feature = "simd128"))]
    {
        for r in 0..rows {
            softmax_row_wasm(&data[r * cols..(r + 1) * cols], &mut out[r * cols..(r + 1) * cols]);
        }
        return;
    }
    softmax_rows_naive(data, rows, cols, out);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn softmax_matches_naive_and_sums_to_one() {
        let (rows, cols) = (8, 100);
        let data: Vec<f64> = (0..rows * cols).map(|k| (k % 50) as f64 * 0.1 - 2.5).collect();
        let mut simd = vec![0.0; rows * cols];
        let mut naive = vec![0.0; rows * cols];
        softmax_rows(&data, rows, cols, &mut simd);
        softmax_rows_naive(&data, rows, cols, &mut naive);
        for i in 0..rows * cols {
            assert!((simd[i] - naive[i]).abs() <= 1e-6, "i={i}: {} vs {}", simd[i], naive[i]);
        }
        for r in 0..rows {
            let s: f64 = simd[r * cols..(r + 1) * cols].iter().sum();
            assert!((s - 1.0).abs() < 1e-9, "row {r} sum {s}");
        }
    }
}
