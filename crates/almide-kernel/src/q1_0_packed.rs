//! Q1_0 in Almide's packed, f64 layout — the ABI of
//! `almide_rt_matrix_linear_q1_0_row_no_bias`.
//!
//! Each 128-element block is 18 bytes: fp16 scale (2 bytes) + 128 sign bits
//! (16 bytes, LSB-first). x is f64. Almide's own block dot is scalar on x86
//! (its SIMD path is NEON-only), so the AVX f64 path here beats Almide itself on
//! native — and quantized matmul is outside BLAS entirely.
//!
//! Two layers of correctness, same as q1_0.rs: the sign placement is a selection
//! (exhaustively provable), and the reduction is bitwise to a named order +
//! bounded error. Here we differential-test AVX == naive (the reduction order
//! matches), within tolerance for the f64 sum.

/// IEEE fp16 (little-endian bytes) → f64.
pub fn fp16_to_f64(lo: u8, hi: u8) -> f64 {
    let h = (lo as u16) | ((hi as u16) << 8);
    let sign = (h >> 15) & 1;
    let exp = (h >> 10) & 0x1f;
    let mant = h & 0x3ff;
    let val: f64 = if exp == 0 {
        // zero / subnormal: mant * 2^-24
        (mant as f64) * (1.0 / 16_777_216.0)
    } else if exp == 0x1f {
        if mant == 0 { f64::INFINITY } else { f64::NAN }
    } else {
        // normal: (1 + mant/1024) * 2^(exp-15)
        (1.0 + (mant as f64) / 1024.0) * exp2i(exp as i32 - 15)
    };
    if sign == 1 { -val } else { val }
}

#[inline]
fn exp2i(e: i32) -> f64 {
    // exact power of two without powi at runtime cost concerns; e in fp16 range
    if e >= 0 { (1u64 << e) as f64 } else { 1.0 / ((1u64 << (-e)) as f64) }
}

/// One 128-element block: `(Σ x[k]·sign(k)) · scale`, sign packed (16 bytes,
/// LSB-first), scale already decoded. Naive reference.
pub fn q1_0_block_dot_packed_naive(x: &[f64], sign: &[u8], scale: f64) -> f64 {
    let mut acc = 0.0f64;
    for k in 0..128 {
        let bit = (sign[k / 8] >> (k % 8)) & 1;
        acc += if bit == 1 { -x[k] } else { x[k] };
    }
    acc * scale
}

/// AVX f64x4 block dot: apply 4 sign bits per group via XOR of the sign bit,
/// accumulate, horizontal-reduce. The bit-unpack rustc/Almide-scalar can't reach.
#[cfg(target_arch = "x86_64")]
#[target_feature(enable = "avx")]
pub unsafe fn q1_0_block_dot_packed_avx(x: &[f64], sign: &[u8], scale: f64) -> f64 {
    use std::arch::x86_64::*;
    let negzero = f64::from_bits(0x8000_0000_0000_0000); // -0.0 = sign bit only
    let mut acc = _mm256_setzero_pd();
    for g in 0..32 {
        let bit_base = g * 4;
        let byte = sign[bit_base / 8];
        let nib = (byte >> (bit_base % 8)) & 0xF; // 4 bits for this group of 4
        // mask lane k = -0.0 if bit set else +0.0; XOR flips the sign of x[k]
        let mask = _mm256_set_pd(
            if nib & 8 != 0 { negzero } else { 0.0 },
            if nib & 4 != 0 { negzero } else { 0.0 },
            if nib & 2 != 0 { negzero } else { 0.0 },
            if nib & 1 != 0 { negzero } else { 0.0 },
        );
        let xv = _mm256_loadu_pd(x.as_ptr().add(g * 4));
        acc = _mm256_add_pd(acc, _mm256_xor_pd(xv, mask));
    }
    // horizontal reduce 4 -> 1
    let hi = _mm256_extractf128_pd(acc, 1);
    let lo = _mm256_castpd256_pd128(acc);
    let s = _mm_add_pd(lo, hi);
    let s = _mm_add_sd(s, _mm_unpackhi_pd(s, s));
    _mm_cvtsd_f64(s) * scale
}

/// wasm simd128 block dot: f64x2, 2 sign bits per group via ±0.0 mask XOR.
#[cfg(all(target_arch = "wasm32", target_feature = "simd128"))]
fn q1_0_block_dot_packed_wasm(x: &[f64], sign: &[u8], scale: f64) -> f64 {
    use std::arch::wasm32::*;
    let negzero = f64::from_bits(0x8000_0000_0000_0000);
    let mut acc = f64x2_splat(0.0);
    for g in 0..64 {
        let bit_base = g * 2;
        let byte = sign[bit_base / 8];
        let two = (byte >> (bit_base % 8)) & 0x3;
        let mask = f64x2(
            if two & 1 != 0 { negzero } else { 0.0 },
            if two & 2 != 0 { negzero } else { 0.0 },
        );
        // SAFETY: g in 0..64, g*2 in 0..128, x has 128 elems.
        let xv = unsafe { v128_load(x.as_ptr().add(g * 2) as *const v128) };
        acc = f64x2_add(acc, v128_xor(xv, mask));
    }
    (f64x2_extract_lane::<0>(acc) + f64x2_extract_lane::<1>(acc)) * scale
}

/// ARM NEON block dot: f64x2, 2 sign bits per group via ±0.0 mask XOR (integer
/// veor on the bit patterns). This is the quant hot path on Apple Silicon/mobile.
#[cfg(target_arch = "aarch64")]
#[target_feature(enable = "neon")]
unsafe fn q1_0_block_dot_packed_neon(x: &[f64], sign: &[u8], scale: f64) -> f64 {
    use std::arch::aarch64::*;
    let negzero = f64::from_bits(0x8000_0000_0000_0000);
    let mut acc = vdupq_n_f64(0.0);
    for g in 0..64 {
        let bit_base = g * 2;
        let byte = sign[bit_base / 8];
        let two = (byte >> (bit_base % 8)) & 0x3;
        // mask = [lane0: bit0?−0.0:0.0, lane1: bit1?−0.0:0.0]
        let m0 = if two & 1 != 0 { negzero } else { 0.0 };
        let m1 = if two & 2 != 0 { negzero } else { 0.0 };
        let mask = vsetq_lane_f64::<1>(m1, vdupq_n_f64(m0));
        let xv = vld1q_f64(x.as_ptr().add(g * 2));
        // XOR the sign bit (integer veor on bit patterns)
        let signed = vreinterpretq_f64_u64(veorq_u64(
            vreinterpretq_u64_f64(xv),
            vreinterpretq_u64_f64(mask),
        ));
        acc = vaddq_f64(acc, signed);
    }
    (vgetq_lane_f64::<0>(acc) + vgetq_lane_f64::<1>(acc)) * scale
}

/// Block dot, per-target dispatch.
#[inline]
pub fn q1_0_block_dot_packed(x: &[f64], sign: &[u8], scale: f64) -> f64 {
    #[cfg(target_arch = "x86_64")]
    {
        if is_x86_feature_detected!("avx") {
            return unsafe { q1_0_block_dot_packed_avx(x, sign, scale) };
        }
    }
    #[cfg(target_arch = "aarch64")]
    {
        if std::arch::is_aarch64_feature_detected!("neon") {
            return unsafe { q1_0_block_dot_packed_neon(x, sign, scale) };
        }
    }
    #[cfg(all(target_arch = "wasm32", target_feature = "simd128"))]
    {
        return q1_0_block_dot_packed_wasm(x, sign, scale);
    }
    q1_0_block_dot_packed_naive(x, sign, scale)
}

/// Full quantized linear, Almide's ABI: out[i,j] = Σ_blocks dot(x[i, block],
/// W[j, block]·scale). x: `[x_rows*n_in]` f64; w_bytes: packed Q1_0 with
/// `w_offset`; w_rows = out cols. Writes `[x_rows*w_rows]` row-major.
pub fn linear_q1_0_packed(
    x: &[f64], x_rows: usize, n_in: usize,
    w_bytes: &[u8], w_offset: usize, w_rows: usize,
    out: &mut [f64],
) {
    let blocks = n_in / 128;
    for i in 0..x_rows {
        let xi = &x[i * n_in..(i + 1) * n_in];
        for j in 0..w_rows {
            let row_off = w_offset + j * blocks * 18;
            let mut sum = 0.0f64;
            for b in 0..blocks {
                let bs = row_off + b * 18;
                let scale = fp16_to_f64(w_bytes[bs], w_bytes[bs + 1]);
                let sign = &w_bytes[bs + 2..bs + 18];
                sum += q1_0_block_dot_packed(&xi[b * 128..b * 128 + 128], sign, scale);
            }
            out[i * w_rows + j] = sum;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fp16_roundtrip_known_values() {
        assert_eq!(fp16_to_f64(0x00, 0x3c), 1.0); // 0x3c00 = 1.0
        assert_eq!(fp16_to_f64(0x00, 0x40), 2.0); // 0x4000 = 2.0
        assert_eq!(fp16_to_f64(0x00, 0xbc), -1.0); // 0xbc00 = -1.0
        assert_eq!(fp16_to_f64(0x00, 0x00), 0.0);
    }

    #[cfg(target_arch = "x86_64")]
    #[test]
    fn avx_block_dot_matches_naive() {
        if !is_x86_feature_detected!("avx") {
            return;
        }
        for seed in 0..200u32 {
            let x: Vec<f64> = (0..128)
                .map(|k| ((k as u32).wrapping_mul(2654435761).wrapping_add(seed) as f64) * 3e-10 - 1.0)
                .collect();
            let sign: Vec<u8> = (0..16).map(|b| (b * 37 + seed as usize) as u8).collect();
            let scale = 0.0123 + seed as f64 * 1e-4;
            let a = q1_0_block_dot_packed_naive(&x, &sign, scale);
            let b = unsafe { q1_0_block_dot_packed_avx(&x, &sign, scale) };
            let mag: f64 = x.iter().map(|v| v.abs()).sum::<f64>() * scale.abs();
            assert!((a - b).abs() <= 1e-12 * mag.max(1.0), "seed {seed}: {a} vs {b}");
        }
    }
}
