//! f64 transpose — Almide's matrices are f64, and transpose is data movement, so
//! an f64 kernel keeps the values exact (no f32 rounding). AVX uses f64x4
//! (256-bit), so 8x8 decomposes into four 4x4 blocks (same shape as the wasm
//! f32 path). Bitwise-exact.

/// Algorithm: `out[j*8+i] = in[i*8+j]` in f64.
pub fn transpose_8x8_f64_naive(input: &[f64; 64]) -> [f64; 64] {
    let mut out = [0.0f64; 64];
    for i in 0..8 {
        for j in 0..8 {
            out[j * 8 + i] = input[i * 8 + j];
        }
    }
    out
}

/// Public entry: AVX f64x4 when available, naive otherwise.
pub fn transpose_8x8_f64(input: &[f64; 64]) -> [f64; 64] {
    #[cfg(target_arch = "x86_64")]
    {
        if is_x86_feature_detected!("avx") {
            // SAFETY: guarded by the runtime feature check.
            return unsafe { transpose_8x8_f64_avx(input) };
        }
    }
    transpose_8x8_f64_naive(input)
}

#[cfg(target_arch = "x86_64")]
#[inline(always)]
unsafe fn transpose_4x4_f64(
    r0: std::arch::x86_64::__m256d,
    r1: std::arch::x86_64::__m256d,
    r2: std::arch::x86_64::__m256d,
    r3: std::arch::x86_64::__m256d,
) -> [std::arch::x86_64::__m256d; 4] {
    use std::arch::x86_64::*;
    let t0 = _mm256_unpacklo_pd(r0, r1);
    let t1 = _mm256_unpackhi_pd(r0, r1);
    let t2 = _mm256_unpacklo_pd(r2, r3);
    let t3 = _mm256_unpackhi_pd(r2, r3);
    [
        _mm256_permute2f128_pd(t0, t2, 0x20),
        _mm256_permute2f128_pd(t1, t3, 0x20),
        _mm256_permute2f128_pd(t0, t2, 0x31),
        _mm256_permute2f128_pd(t1, t3, 0x31),
    ]
}

#[cfg(target_arch = "x86_64")]
#[target_feature(enable = "avx")]
unsafe fn transpose_8x8_f64_avx(input: &[f64; 64]) -> [f64; 64] {
    use std::arch::x86_64::*;
    let p = input.as_ptr();
    let ld = |off: usize| _mm256_loadu_pd(p.add(off));
    // each row = left (cols 0-3) + right (cols 4-7)
    let (l0, r0) = (ld(0), ld(4));
    let (l1, r1) = (ld(8), ld(12));
    let (l2, r2) = (ld(16), ld(20));
    let (l3, r3) = (ld(24), ld(28));
    let (l4, r4) = (ld(32), ld(36));
    let (l5, r5) = (ld(40), ld(44));
    let (l6, r6) = (ld(48), ld(52));
    let (l7, r7) = (ld(56), ld(60));
    let a = transpose_4x4_f64(l0, l1, l2, l3); // top-left  → rows 0-3, left
    let b = transpose_4x4_f64(l4, l5, l6, l7); // bot-left  → rows 0-3, right
    let c = transpose_4x4_f64(r0, r1, r2, r3); // top-right → rows 4-7, left
    let d = transpose_4x4_f64(r4, r5, r6, r7); // bot-right → rows 4-7, right
    let mut out = [0.0f64; 64];
    let q = out.as_mut_ptr();
    for j in 0..4 {
        _mm256_storeu_pd(q.add(j * 8), a[j]);
        _mm256_storeu_pd(q.add(j * 8 + 4), b[j]);
        _mm256_storeu_pd(q.add((j + 4) * 8), c[j]);
        _mm256_storeu_pd(q.add((j + 4) * 8 + 4), d[j]);
    }
    out
}

/// Arbitrary-size f64 transpose: 8x8 tiles through the SIMD kernel, scalar edges.
pub fn transpose_matrix_f64(input: &[f64], rows: usize, cols: usize, out: &mut [f64]) {
    assert_eq!(input.len(), rows * cols);
    assert_eq!(out.len(), rows * cols);
    let rt = rows / 8 * 8;
    let ct = cols / 8 * 8;
    let mut tile = [0.0f64; 64];
    let mut ti = 0;
    while ti < rt {
        let mut tj = 0;
        while tj < ct {
            for r in 0..8 {
                let base = (ti + r) * cols + tj;
                tile[r * 8..r * 8 + 8].copy_from_slice(&input[base..base + 8]);
            }
            let t = transpose_8x8_f64(&tile);
            for r in 0..8 {
                let base = (tj + r) * rows + ti;
                out[base..base + 8].copy_from_slice(&t[r * 8..r * 8 + 8]);
            }
            tj += 8;
        }
        ti += 8;
    }
    for i in 0..rows {
        for j in 0..cols {
            if i >= rt || j >= ct {
                out[j * rows + i] = input[i * cols + j];
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn f64_8x8_bitwise() {
        let input: [f64; 64] = std::array::from_fn(|k| (k as f64) * 1.5 - 7.0);
        let simd = transpose_8x8_f64(&input);
        let naive = transpose_8x8_f64_naive(&input);
        for k in 0..64 {
            assert_eq!(simd[k].to_bits(), naive[k].to_bits(), "at {k}");
        }
    }

    /// Static equivalence (all inputs) for the f64 permutation kernel: the
    /// index array extracts the whole permutation in one run.
    #[test]
    fn f64_schedule_is_transpose_permutation_all_inputs() {
        let index: [f64; 64] = std::array::from_fn(|k| k as f64);
        let out = transpose_8x8_f64(&index);
        for p in 0..64 {
            let source = out[p] as usize;
            let (i, j) = (p % 8, p / 8);
            assert_eq!(source, i * 8 + j, "f64 permutation wrong at {p}");
        }
    }

    #[test]
    fn f64_matrix_arbitrary() {
        for &(rows, cols) in &[(8, 8), (16, 24), (13, 8), (8, 13), (37, 41), (1, 5)] {
            let input: Vec<f64> = (0..rows * cols).map(|k| k as f64 * 0.5 - 1.0).collect();
            let mut out = vec![0.0f64; rows * cols];
            transpose_matrix_f64(&input, rows, cols, &mut out);
            for i in 0..rows {
                for j in 0..cols {
                    assert_eq!(out[j * rows + i].to_bits(), input[i * cols + j].to_bits());
                }
            }
        }
    }
}
