//! Q1_0 in Almide's packed, f64 layout — the ABI of
//! `almide_rt_matrix_linear_q1_0_row_no_bias`.
//!
//! Each 128-element block is 18 bytes: fp16 scale (2 bytes) + 128 sign bits
//! (16 bytes, LSB-first). Bit 1 is `+x`, bit 0 is `-x` — the convention of
//! `from_q1_0_bytes` / `select_rows_q1_0`, so the packed GEMV IS the GEMV over
//! the decoded matrix. x is f64. Quantized matmul is outside BLAS entirely.
//!
//! ONE reduction order on every arch (#1827), so native answers the same bits on
//! x86_64, aarch64 and wasm32, and the wasm self-host (stdlib/matrix_fused.almd)
//! transcribes it: two lanes, lane 0 sums the even k and lane 1 the odd k, each
//! k-ascending from +0.0, then `(lane0 + lane1) * scale`. The SIMD paths are
//! f64x2 (SSE2 / NEON / simd128) — exactly those two lanes — and the naive
//! reference spells the same two accumulators, so every path is bit-identical.

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

/// One 128-element block: `(Σ±x[k]) · scale`, sign packed (16 bytes,
/// LSB-first, bit 1 = `+x`), scale already decoded. Naive reference — the two
/// lanes the SIMD paths keep, spelled as two accumulators.
pub fn q1_0_block_dot_packed_naive(x: &[f64], sign: &[u8], scale: f64) -> f64 {
    let mut lane = [0.0f64; 2];
    for k in 0..128 {
        let bit = (sign[k / 8] >> (k % 8)) & 1;
        lane[k % 2] += if bit == 1 { x[k] } else { -x[k] };
    }
    (lane[0] + lane[1]) * scale
}

/// The two-lane XOR mask of sign bits `two` (bit 0 → lane 0): `-0.0` flips a
/// lane whose bit is CLEAR (that weight is `-1`), `+0.0` keeps it.
#[inline]
fn lane_masks(two: u8) -> (f64, f64) {
    let negzero = f64::from_bits(0x8000_0000_0000_0000); // -0.0 = sign bit only
    (
        if two & 1 == 0 { negzero } else { 0.0 },
        if two & 2 == 0 { negzero } else { 0.0 },
    )
}

/// x86_64 block dot: SSE2 f64x2 (baseline on x86_64), 2 sign bits per group via
/// ±0.0 mask XOR — the same two lanes as NEON and simd128, so the same bits.
#[cfg(target_arch = "x86_64")]
pub fn q1_0_block_dot_packed_sse2(x: &[f64], sign: &[u8], scale: f64) -> f64 {
    use std::arch::x86_64::*;
    // SAFETY: SSE2 is part of the x86_64 baseline; the loads read x[2g..2g+2]
    // with g < 64 inside the 128-element block the caller slices.
    unsafe {
        let mut acc = _mm_setzero_pd();
        for g in 0..64 {
            let bit_base = g * 2;
            let byte = sign[bit_base / 8];
            let (m0, m1) = lane_masks((byte >> (bit_base % 8)) & 0x3);
            let mask = _mm_set_pd(m1, m0);
            let xv = _mm_loadu_pd(x.as_ptr().add(g * 2));
            acc = _mm_add_pd(acc, _mm_xor_pd(xv, mask));
        }
        let lo = _mm_cvtsd_f64(acc);
        let hi = _mm_cvtsd_f64(_mm_unpackhi_pd(acc, acc));
        (lo + hi) * scale
    }
}

/// wasm simd128 block dot: f64x2, 2 sign bits per group via ±0.0 mask XOR.
#[cfg(all(target_arch = "wasm32", target_feature = "simd128"))]
fn q1_0_block_dot_packed_wasm(x: &[f64], sign: &[u8], scale: f64) -> f64 {
    use std::arch::wasm32::*;
    let mut acc = f64x2_splat(0.0);
    // 128 f64 = exactly 64 two-lane windows; the cast is proved by the type.
    let (lanes, _) = x.as_chunks::<2>();
    for g in 0..64 {
        let bit_base = g * 2;
        let byte = sign[bit_base / 8];
        let (m0, m1) = lane_masks((byte >> (bit_base % 8)) & 0x3);
        let mask = f64x2(m0, m1);
        let xv = crate::simd_wasm::load_f64x2(&lanes[g]);
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
    let mut acc = vdupq_n_f64(0.0);
    for g in 0..64 {
        let bit_base = g * 2;
        let byte = sign[bit_base / 8];
        let (m0, m1) = lane_masks((byte >> (bit_base % 8)) & 0x3);
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
        q1_0_block_dot_packed_sse2(x, sign, scale)
    }
    #[cfg(not(target_arch = "x86_64"))]
    {
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
}

/// Full quantized linear, Almide's ABI: out[i,j] = Σ_blocks dot(x[i, block],
/// W[j, block]·scale). x: `[x_rows*n_in]` f64; w_bytes: packed Q1_0 with
/// `w_offset`; w_rows = out cols. Writes `[x_rows*w_rows]` row-major.
/// A packed Q1_0 weight matrix: the byte blob, where this matrix starts inside
/// it, and how many output rows it has. Each row is `blocks` × 18 bytes — two
/// bytes of fp16 scale followed by sixteen of packed signs.
#[derive(Clone, Copy)]
pub struct PackedWeights<'a> {
    pub bytes: &'a [u8],
    pub offset: usize,
    pub rows: usize,
}

pub fn linear_q1_0_packed(
    x: &[f64], x_rows: usize, n_in: usize,
    w: PackedWeights<'_>,
    out: &mut [f64],
) {
    let blocks = n_in / 128;
    for i in 0..x_rows {
        let xi = &x[i * n_in..(i + 1) * n_in];
        for j in 0..w.rows {
            out[i * w.rows + j] = packed_row_dot(xi, &w, j, blocks);
        }
    }
}

/// One output cell: Σ over the row's blocks of `dot(x[block], W[j, block]·scale)`.
fn packed_row_dot(xi: &[f64], w: &PackedWeights<'_>, j: usize, blocks: usize) -> f64 {
    let row_off = w.offset + j * blocks * 18;
    let mut sum = 0.0f64;
    for b in 0..blocks {
        let bs = row_off + b * 18;
        let scale = fp16_to_f64(w.bytes[bs], w.bytes[bs + 1]);
        let sign = &w.bytes[bs + 2..bs + 18];
        sum += q1_0_block_dot_packed(&xi[b * 128..b * 128 + 128], sign, scale);
    }
    sum
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

    /// Every path is the SAME two-lane order, so bit-identical — not merely close.
    #[test]
    fn simd_block_dot_is_bitwise_naive() {
        for seed in 0..200u32 {
            let x: Vec<f64> = (0..128)
                .map(|k| ((k as u32).wrapping_mul(2654435761).wrapping_add(seed) as f64) * 3e-10 - 1.0)
                .collect();
            let sign: Vec<u8> = (0..16).map(|b| (b * 37 + seed as usize) as u8).collect();
            let scale = 0.0123 + seed as f64 * 1e-4;
            let a = q1_0_block_dot_packed_naive(&x, &sign, scale);
            let b = q1_0_block_dot_packed(&x, &sign, scale);
            assert_eq!(a.to_bits(), b.to_bits(), "seed {seed}: {a} vs {b}");
        }
    }

    #[test]
    fn bit_one_is_plus_x() {
        let mut x = vec![0.0f64; 128];
        x[0] = 1.0;
        let mut sign = vec![0u8; 16];
        sign[0] = 1;
        assert_eq!(q1_0_block_dot_packed(&x, &sign, 1.0), 1.0);
        sign[0] = 0;
        assert_eq!(q1_0_block_dot_packed(&x, &sign, 1.0), -1.0);
    }
}
