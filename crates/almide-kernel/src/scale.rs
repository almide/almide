//! Elementwise scale: `out[i] = a * x[i]`.
//!
//! Honest note (measured): elementwise is memory-bound. With `-C
//! target-cpu=native` the autovectorized naive loop ties explicit AVX (0.99x) —
//! rustc emits the same AVX. Default builds show explicit AVX ~1.23x *only*
//! because autovec is capped at the SSE2 baseline target; the right fix is
//! target-cpu, not shipping ceremony. So `scale` stays naive — almide-kernel's
//! edge is *data-movement* (transpose: 4.23x native, autovec can't build a
//! shuffle network), not elementwise. `scale_avx` exists only for the
//! differential test + bench. Don't ship ceremony that doesn't pay — the same
//! honesty that kept @stream out of Wyve's examples.

/// Production: `out[i] = a * x[i]`. rustc autovectorizes this loop.
pub fn scale(a: f32, x: &[f32], out: &mut [f32]) {
    assert_eq!(x.len(), out.len());
    // iterator form: no bounds checks, rustc autovectorizes fully
    for (o, &xi) in out.iter_mut().zip(x) {
        *o = a * xi;
    }
}

/// Explicit AVX — measurement only, differential-tested bitwise-equal to `scale`.
#[cfg(target_arch = "x86_64")]
#[target_feature(enable = "avx")]
pub unsafe fn scale_avx(a: f32, x: &[f32], out: &mut [f32]) {
    use std::arch::x86_64::*;
    let n = x.len();
    let va = _mm256_set1_ps(a);
    let chunks = n / 8;
    for c in 0..chunks {
        let off = c * 8;
        let vx = _mm256_loadu_ps(x.as_ptr().add(off));
        _mm256_storeu_ps(out.as_mut_ptr().add(off), _mm256_mul_ps(va, vx));
    }
    for i in (chunks * 8)..n {
        out[i] = a * x[i];
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn scale_is_scale() {
        let x: Vec<f32> = (0..100).map(|k| k as f32 * 0.5 - 7.0).collect();
        let mut out = vec![0.0f32; 100];
        scale(2.5, &x, &mut out);
        for i in 0..100 {
            assert_eq!(out[i], 2.5 * x[i]);
        }
    }

    #[cfg(target_arch = "x86_64")]
    #[test]
    fn avx_matches_naive_bitwise() {
        if !is_x86_feature_detected!("avx") {
            return;
        }
        let x: Vec<f32> = (0..1000).map(|k| (k as f32) * 0.013 - 3.3).collect();
        let mut a_out = vec![0.0f32; 1000];
        let mut b_out = vec![0.0f32; 1000];
        scale(1.7, &x, &mut a_out);
        unsafe { scale_avx(1.7, &x, &mut b_out) };
        for i in 0..1000 {
            assert_eq!(a_out[i].to_bits(), b_out[i].to_bits(), "index {i}");
        }
    }
}
