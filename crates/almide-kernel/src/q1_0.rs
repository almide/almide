//! Q1_0 block dot — the inner kernel of 1-bit quantized matmul.
//!
//! This is where almide-kernel beats Rust hardest: the work is "unpack a packed
//! sign bit per element, then a signed sum". rustc can't autovectorize it (bit
//! addressing is non-affine), and Almide's own `q1_0_block_dot` falls to *scalar*
//! on x86 (its SIMD path is NEON-only). Explicit AVX2 has the floor to itself.
//!
//! Exo-style: algorithm (the signed-sum spec) and schedule (AVX2 bit-unpack +
//! lane-parallel sum + horizontal reduce) separated. NOTE: this is a reduction,
//! so the schedule reassociates the float sum — equivalence is "within
//! tolerance", not bitwise. (Wyve/Exo require reassoc for float reductions;
//! bitwise-exact is the wrong bar. transpose was data-movement → bitwise; this
//! is a sum → reassoc. Being explicit about which bar applies is the point.)

/// Algorithm: `out = (Σ_k x[k] * sign(k)) * scale`, where `sign(k) = -1` if the
/// k-th packed bit is set, else `+1`. 128-element block; sign is 16 bytes
/// (128 bits, LSB-first within each byte).
pub fn q1_0_dot_naive(x: &[f32; 128], sign: &[u8; 16], scale: f32) -> f32 {
    let mut acc = 0.0f32;
    for k in 0..128 {
        let bit = (sign[k / 8] >> (k % 8)) & 1;
        acc += if bit == 1 { -x[k] } else { x[k] };
    }
    acc * scale
}

/// Public entry: per-target SIMD schedule (AVX2 on x86, simd128 on wasm),
/// naive algorithm as the fallback.
pub fn q1_0_dot(x: &[f32; 128], sign: &[u8; 16], scale: f32) -> f32 {
    #[cfg(target_arch = "x86_64")]
    {
        if is_x86_feature_detected!("avx2") {
            // SAFETY: guarded by the runtime feature check.
            return unsafe { q1_0_dot_avx2(x, sign, scale) };
        }
    }
    #[cfg(all(target_arch = "wasm32", target_feature = "simd128"))]
    {
        return q1_0_dot_simd128(x, sign, scale);
    }
    #[allow(unreachable_code)]
    q1_0_dot_naive(x, sign, scale)
}

/// Schedule (wasm simd128): the same bit-unpack as AVX2, 4-wide (v128 = f32x4).
/// wasm has no autovec for this and Almide's wasm path is scalar — so this is
/// where almide-kernel beats both Rust-wasm and Almide-wasm.
#[cfg(all(target_arch = "wasm32", target_feature = "simd128"))]
pub fn q1_0_dot_simd128(x: &[f32; 128], sign: &[u8; 16], scale: f32) -> f32 {
    use std::arch::wasm32::*;
    let lane_bit = i32x4(1, 2, 4, 8);
    let signbit = i32x4_splat(0x8000_0000u32 as i32);
    let mut acc = f32x4_splat(0.0);
    for blk in 0..32 {
        let bit_base = blk * 4;
        let byte = sign[bit_base / 8];
        let nib = ((byte >> (bit_base % 8)) & 0xF) as i32;
        let b = i32x4_splat(nib);
        let sel = v128_and(b, lane_bit);
        let is_set = i32x4_eq(sel, lane_bit); // all-ones where bit set
        let flip = v128_and(is_set, signbit); // 0x80000000 where set
        // SAFETY: blk in 0..32, blk*4 in 0..128, x has 128 elems.
        let xv = unsafe { v128_load(x.as_ptr().add(blk * 4) as *const v128) };
        let signed = v128_xor(xv, flip);
        acc = f32x4_add(acc, signed);
    }
    let s = f32x4_extract_lane::<0>(acc)
        + f32x4_extract_lane::<1>(acc)
        + f32x4_extract_lane::<2>(acc)
        + f32x4_extract_lane::<3>(acc);
    s * scale
}

/// Schedule (AVX2): for each byte, broadcast it, select each lane's bit, turn a
/// set bit into the float sign bit (0x80000000), XOR it onto x (flip sign), and
/// accumulate 8-wide; then horizontally reduce. This is the bit-unpack rustc
/// can't reach.
/// The bit-unpack, isolated: apply 8 packed sign bits (one byte, LSB-first) to
/// 8 lanes — lane k gets `-x` if bit k is set, else `x`. It's an XOR of the sign
/// bit, hence value-independent. This is the part proven statically and
/// exhaustively (256 byte values = all bit patterns); the reduction around it is
/// the reassociated part that stays within-tolerance.
#[cfg(target_arch = "x86_64")]
#[target_feature(enable = "avx2")]
unsafe fn apply_sign_avx2(x: std::arch::x86_64::__m256, byte: u8) -> std::arch::x86_64::__m256 {
    use std::arch::x86_64::*;
    let lane_bit = _mm256_setr_epi32(1, 2, 4, 8, 16, 32, 64, 128); // LSB-first per byte
    let signbit = _mm256_set1_epi32(0x8000_0000u32 as i32);
    let b = _mm256_set1_epi32(byte as i32);
    let sel = _mm256_and_si256(b, lane_bit);
    let is_set = _mm256_cmpeq_epi32(sel, lane_bit); // 0xFFFFFFFF where bit set
    let flip = _mm256_and_si256(is_set, signbit); // 0x80000000 where set
    _mm256_xor_ps(x, _mm256_castsi256_ps(flip))
}

#[cfg(target_arch = "x86_64")]
#[target_feature(enable = "avx2")]
pub unsafe fn q1_0_dot_avx2(x: &[f32; 128], sign: &[u8; 16], scale: f32) -> f32 {
    use std::arch::x86_64::*;
    let mut acc = _mm256_setzero_ps();
    for blk in 0..16 {
        let xv = _mm256_loadu_ps(x.as_ptr().add(blk * 8));
        let signed = apply_sign_avx2(xv, sign[blk]);
        acc = _mm256_add_ps(acc, signed);
    }
    // horizontal reduce 8 -> 1
    let lo = _mm256_castps256_ps128(acc);
    let hi = _mm256_extractf128_ps(acc, 1);
    let s = _mm_add_ps(lo, hi);
    let s = _mm_hadd_ps(s, s);
    let s = _mm_hadd_ps(s, s);
    _mm_cvtss_f32(s) * scale
}

/// Full quantized linear — the inference hot path. `out[m,n] = Σ_k x[m,k]·W[n,k]`
/// with W in Q1_0. x: `[m*k]` row-major f32; w_sign: `[n*blocks*16]` u8;
/// w_scale: `[n*blocks]` f32; k a multiple of 128. Uses the per-target
/// `q1_0_dot` schedule (AVX2 / simd128 / naive) for every block dot.
pub fn linear_q1_0(
    x: &[f32], m: usize, k: usize,
    w_sign: &[u8], w_scale: &[f32], n: usize,
    out: &mut [f32],
) {
    let blocks = k / 128;
    for mi in 0..m {
        for ni in 0..n {
            let mut acc = 0.0f32;
            for b in 0..blocks {
                let xb: &[f32; 128] =
                    (&x[mi * k + b * 128..mi * k + b * 128 + 128]).try_into().unwrap();
                let sb: &[u8; 16] = (&w_sign[(ni * blocks + b) * 16..(ni * blocks + b) * 16 + 16])
                    .try_into()
                    .unwrap();
                acc += q1_0_dot(xb, sb, w_scale[ni * blocks + b]);
            }
            out[mi * n + ni] = acc;
        }
    }
}

/// Naive reference for `linear_q1_0` (scalar block dot — the Rust baseline).
pub fn linear_q1_0_naive(
    x: &[f32], m: usize, k: usize,
    w_sign: &[u8], w_scale: &[f32], n: usize,
    out: &mut [f32],
) {
    let blocks = k / 128;
    for mi in 0..m {
        for ni in 0..n {
            let mut acc = 0.0f32;
            for b in 0..blocks {
                let xb: &[f32; 128] =
                    (&x[mi * k + b * 128..mi * k + b * 128 + 128]).try_into().unwrap();
                let sb: &[u8; 16] = (&w_sign[(ni * blocks + b) * 16..(ni * blocks + b) * 16 + 16])
                    .try_into()
                    .unwrap();
                acc += q1_0_dot_naive(xb, sb, w_scale[ni * blocks + b]);
            }
            out[mi * n + ni] = acc;
        }
    }
}

/// The q1_0 reduction in the EXACT order the SIMD schedule uses — this is the
/// *specification* of the reduction order (floats are order-dependent, so the
/// order is part of the spec, not an accident). 8 lane accumulators over the 16
/// blocks, then the horizontal tree `(l0+l4 + l1+l5) + (l2+l6 + l3+l7)` that
/// matches `_mm_add_ps` + two `_mm_hadd_ps`. The SIMD kernel is proven
/// bitwise-exact to THIS (layer 1). Its distance from the idealized exact sum is
/// bounded by `n·u·Σ|xₖ|` (n=128, u=2⁻²⁴) — layer 2, the error is *bounded*, not
/// hoped (a reduction reassociation theorem; provable in Lean).
pub fn q1_0_dot_tree_order(x: &[f32; 128], sign: &[u8; 16], scale: f32) -> f32 {
    let mut lanes = [0.0f32; 8];
    for blk in 0..16 {
        for k in 0..8 {
            let bit = (sign[blk] >> k) & 1;
            lanes[k] += if bit == 1 { -x[blk * 8 + k] } else { x[blk * 8 + k] };
        }
    }
    // horizontal reduce in the SIMD order: lo+hi, then two hadds
    let t0 = lanes[0] + lanes[4];
    let t1 = lanes[1] + lanes[5];
    let t2 = lanes[2] + lanes[6];
    let t3 = lanes[3] + lanes[7];
    ((t0 + t1) + (t2 + t3)) * scale
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make(seed: u32) -> ([f32; 128], [u8; 16], f32) {
        let x: [f32; 128] = std::array::from_fn(|k| {
            let h = (k as u32).wrapping_mul(2654435761).wrapping_add(seed.wrapping_mul(40503));
            (h as f32) * 0.000030517578 - 1.0
        });
        let sign: [u8; 16] = std::array::from_fn(|b| {
            ((b as u32).wrapping_mul(2246822519).wrapping_add(seed) & 0xff) as u8
        });
        (x, sign, 0.0123 + (seed as f32) * 0.0001)
    }

    /// Static, EXHAUSTIVE proof of the bit-unpack (Exo-style, but finite so no
    /// solver). The sign apply is an XOR of the sign bit — value-independent — so
    /// x=1 reveals the sign per lane, and 256 byte values cover ALL bit patterns.
    /// 256 runs prove `apply_sign` correct for every byte and every input. The
    /// reduction is the only part left to tolerance; the sign placement is proven.
    #[cfg(target_arch = "x86_64")]
    #[test]
    fn avx2_bit_unpack_total_proof() {
        if !is_x86_feature_detected!("avx2") {
            return;
        }
        use std::arch::x86_64::*;
        for byte in 0..=255u8 {
            let signed: [f32; 8] = unsafe {
                let xv = _mm256_set1_ps(1.0);
                let s = apply_sign_avx2(xv, byte);
                let mut out = [0.0f32; 8];
                _mm256_storeu_ps(out.as_mut_ptr(), s);
                out
            };
            for k in 0..8 {
                let expected = if (byte >> k) & 1 == 1 { -1.0f32 } else { 1.0f32 };
                assert_eq!(signed[k].to_bits(), expected.to_bits(), "byte {byte} lane {k}");
            }
        }
    }

    #[test]
    fn naive_signed_sum_is_correct() {
        let mut x = [1.0f32; 128];
        x[0] = 5.0;
        let mut sign = [0u8; 16];
        sign[0] = 0b0000_0001; // only element 0 negated
        // (-5) + 1*127 = 122, * scale
        assert_eq!(q1_0_dot_naive(&x, &sign, 2.0), 122.0 * 2.0);
    }

    /// Layer 1 of the reduction proof: the SIMD kernel is BITWISE-EXACT to the
    /// specified tree-order reduction. No tolerance — the order is the spec and
    /// the kernel implements it exactly. (Distance from the idealized exact sum
    /// is then a bounded reassociation error, layer 2.)
    #[cfg(target_arch = "x86_64")]
    #[test]
    fn avx2_is_bitwise_exact_to_tree_order() {
        if !is_x86_feature_detected!("avx2") {
            return;
        }
        for seed in 0..500u32 {
            let (x, sign, scale) = make(seed);
            let simd = unsafe { q1_0_dot_avx2(&x, &sign, scale) };
            let spec = q1_0_dot_tree_order(&x, &sign, scale);
            assert_eq!(simd.to_bits(), spec.to_bits(), "seed {seed}: simd {simd} vs tree-order {spec}");
        }
    }

    /// Layer 2 (empirical witness of the bound): tree-order vs the f64 "exact"
    /// reference stays within n·u·Σ|x|. The theorem is provable in Lean; this
    /// test witnesses it holds with margin.
    #[test]
    fn tree_order_error_is_within_the_proven_bound() {
        for seed in 0..500u32 {
            let (x, sign, scale) = make(seed);
            let tree = q1_0_dot_tree_order(&x, &sign, scale) as f64;
            // exact reference in f64 (negligible rounding at this size)
            let mut exact = 0.0f64;
            for k in 0..128 {
                let bit = (sign[k / 8] >> (k % 8)) & 1;
                exact += if bit == 1 { -(x[k] as f64) } else { x[k] as f64 };
            }
            exact *= scale as f64;
            let sum_abs: f64 = x.iter().map(|v| v.abs() as f64).sum::<f64>() * (scale.abs() as f64);
            let bound = 128.0 * 2f64.powi(-24) * sum_abs; // n·u·Σ|x|
            assert!(
                (tree - exact).abs() <= bound,
                "seed {seed}: err {} > bound {bound}",
                (tree - exact).abs()
            );
        }
    }

    #[test]
    fn avx2_matches_naive_within_tolerance() {
        // reduction: the schedule reassociates the float sum, so compare within
        // relative tolerance — bitwise-exact is the wrong bar for a reduction.
        #[cfg(target_arch = "x86_64")]
        {
            if !is_x86_feature_detected!("avx2") {
                return;
            }
            for seed in 0..200u32 {
                let (x, sign, scale) = make(seed);
                let a = q1_0_dot_naive(&x, &sign, scale);
                let b = unsafe { q1_0_dot_avx2(&x, &sign, scale) };
                // A reduction's reassoc error scales with the sum of term
                // magnitudes, not |result| (the result can cancel near zero).
                // Absolute tolerance vs that sum is the right bar for an f32
                // reassociated reduction.
                let mag: f32 = x.iter().map(|v| v.abs()).sum::<f32>() * scale.abs();
                let diff = (a - b).abs();
                assert!(
                    diff <= 1e-5 * mag.max(1.0),
                    "seed {seed}: naive {a} vs avx2 {b} (diff {diff}, mag {mag})"
                );
            }
        }
    }
}
