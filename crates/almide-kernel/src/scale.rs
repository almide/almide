//! Elementwise scale: `out[i] = a * x[i]`.
//!
//! Honest note (measured): elementwise is memory-bound. With `-C
//! target-cpu=native` the autovectorized naive loop ties explicit AVX (0.99x) —
//! rustc emits the same AVX. Default builds show explicit AVX ~1.23x *only*
//! because autovec is capped at the SSE2 baseline target; the right fix is
//! target-cpu, not shipping ceremony. So `scale` stays naive. The explicit
//! AVX `scale_avx` that measured it was `unsafe` and removed (#4342):
//! nothing but its own bench called it. Don't ship ceremony
//! that doesn't pay — the same honesty that kept @stream out of Wyve's
//! examples.

/// Production: `out[i] = a * x[i]`. rustc autovectorizes this loop.
pub fn scale(a: f32, x: &[f32], out: &mut [f32]) {
    assert_eq!(x.len(), out.len());
    // iterator form: no bounds checks, rustc autovectorizes fully
    for (o, &xi) in out.iter_mut().zip(x) {
        *o = a * xi;
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
}
