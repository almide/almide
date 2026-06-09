//! 8x8 f32 transpose — Exo-style: algorithm (what) and schedule (how) separated.
//!
//! - `transpose_8x8_naive` is the **algorithm**: the spec, `out[j][i] = in[i][j]`.
//! - The **schedule** is an explicit composition of named passes (load → unpack
//!   → shuffle → permute → store), not one hand-written SIMD blob. Each pass is
//!   an independent transform; composing them *is* the schedule. To change how
//!   the transpose runs, recompose the passes — the algorithm above is untouched.
//! - **Equivalence** (Exo's program-equivalence guarantee) is pinned by the
//!   differential test: every schedule must be bitwise-exact to the naive
//!   algorithm. Static proof (Lean) is the optional stronger backstop; the
//!   differential test is the everyday guarantee.

/// Algorithm: `out[j*8 + i] = in[i*8 + j]` (the spec every schedule must match).
pub fn transpose_8x8_naive(input: &[f32; 64]) -> [f32; 64] {
    let mut out = [0.0f32; 64];
    for i in 0..8 {
        for j in 0..8 {
            out[j * 8 + i] = input[i * 8 + j];
        }
    }
    out
}

/// Public entry. Per-target schedule (AVX on x86, simd128 on wasm), naive else.
pub fn transpose_8x8(input: &[f32; 64]) -> [f32; 64] {
    #[cfg(target_arch = "x86_64")]
    {
        if is_x86_feature_detected!("avx") {
            // SAFETY: guarded by the runtime feature check.
            return unsafe { transpose_8x8_avx(input) };
        }
    }
    #[cfg(all(target_arch = "wasm32", target_feature = "simd128"))]
    {
        return transpose_8x8_simd128(input);
    }
    #[allow(unreachable_code)]
    transpose_8x8_naive(input)
}

// ============== schedule: wasm simd128 (128-bit = f32x4) ==============
// 8x8 decomposes into four 4x4 blocks; each block transposed by a 4x4 shuffle
// pass, then the off-diagonal blocks swap places. The 4x4 pass is the unit;
// the schedule composes four of them + the block swap. Bitwise-exact (no reassoc
// — it's pure data movement).

/// pass: transpose a 4x4 block held in four f32x4 rows.
#[cfg(all(target_arch = "wasm32", target_feature = "simd128"))]
#[inline(always)]
fn transpose_4x4(
    r0: std::arch::wasm32::v128,
    r1: std::arch::wasm32::v128,
    r2: std::arch::wasm32::v128,
    r3: std::arch::wasm32::v128,
) -> [std::arch::wasm32::v128; 4] {
    use std::arch::wasm32::*;
    let t0 = i32x4_shuffle::<0, 4, 1, 5>(r0, r1);
    let t1 = i32x4_shuffle::<2, 6, 3, 7>(r0, r1);
    let t2 = i32x4_shuffle::<0, 4, 1, 5>(r2, r3);
    let t3 = i32x4_shuffle::<2, 6, 3, 7>(r2, r3);
    [
        i32x4_shuffle::<0, 1, 4, 5>(t0, t2),
        i32x4_shuffle::<2, 3, 6, 7>(t0, t2),
        i32x4_shuffle::<0, 1, 4, 5>(t1, t3),
        i32x4_shuffle::<2, 3, 6, 7>(t1, t3),
    ]
}

#[cfg(all(target_arch = "wasm32", target_feature = "simd128"))]
fn transpose_8x8_simd128(input: &[f32; 64]) -> [f32; 64] {
    use std::arch::wasm32::*;
    let p = input.as_ptr();
    // each row = left half (cols 0-3) + right half (cols 4-7)
    // SAFETY: 64 f32 in, all offsets < 64.
    let ld = |off: usize| unsafe { v128_load(p.add(off) as *const v128) };
    let (l0, r0) = (ld(0), ld(4));
    let (l1, r1) = (ld(8), ld(12));
    let (l2, r2) = (ld(16), ld(20));
    let (l3, r3) = (ld(24), ld(28));
    let (l4, r4) = (ld(32), ld(36));
    let (l5, r5) = (ld(40), ld(44));
    let (l6, r6) = (ld(48), ld(52));
    let (l7, r7) = (ld(56), ld(60));
    let a = transpose_4x4(l0, l1, l2, l3); // top-left  → out rows 0-3, left
    let b = transpose_4x4(l4, l5, l6, l7); // bot-left  → out rows 0-3, right
    let c = transpose_4x4(r0, r1, r2, r3); // top-right → out rows 4-7, left
    let d = transpose_4x4(r4, r5, r6, r7); // bot-right → out rows 4-7, right
    let mut out = [0.0f32; 64];
    let q = out.as_mut_ptr();
    let st = |off: usize, v: v128| unsafe { v128_store(q.add(off) as *mut v128, v) };
    for j in 0..4 {
        st(j * 8, a[j]);
        st(j * 8 + 4, b[j]);
        st((j + 4) * 8, c[j]);
        st((j + 4) * 8 + 4, d[j]);
    }
    out
}

// ===================== schedule: AVX 3-pass shuffle network =====================
// The "how", separated from the algorithm. Each pass is a named, independent
// transform on the 8 row-vectors. The schedule is their composition.

#[cfg(target_arch = "x86_64")]
mod avx {
    use std::arch::x86_64::*;

    #[inline(always)]
    pub unsafe fn load_rows(input: &[f32; 64]) -> [__m256; 8] {
        let p = input.as_ptr();
        [
            _mm256_loadu_ps(p),
            _mm256_loadu_ps(p.add(8)),
            _mm256_loadu_ps(p.add(16)),
            _mm256_loadu_ps(p.add(24)),
            _mm256_loadu_ps(p.add(32)),
            _mm256_loadu_ps(p.add(40)),
            _mm256_loadu_ps(p.add(48)),
            _mm256_loadu_ps(p.add(56)),
        ]
    }

    /// pass 1: unpack adjacent rows within each 128-bit lane.
    #[inline(always)]
    pub unsafe fn pass_unpack(r: [__m256; 8]) -> [__m256; 8] {
        [
            _mm256_unpacklo_ps(r[0], r[1]),
            _mm256_unpackhi_ps(r[0], r[1]),
            _mm256_unpacklo_ps(r[2], r[3]),
            _mm256_unpackhi_ps(r[2], r[3]),
            _mm256_unpacklo_ps(r[4], r[5]),
            _mm256_unpackhi_ps(r[4], r[5]),
            _mm256_unpacklo_ps(r[6], r[7]),
            _mm256_unpackhi_ps(r[6], r[7]),
        ]
    }

    /// pass 2: shuffle 64-bit groups across the unpacked pairs.
    #[inline(always)]
    pub unsafe fn pass_shuffle(t: [__m256; 8]) -> [__m256; 8] {
        [
            _mm256_shuffle_ps(t[0], t[2], 0x44),
            _mm256_shuffle_ps(t[0], t[2], 0xEE),
            _mm256_shuffle_ps(t[1], t[3], 0x44),
            _mm256_shuffle_ps(t[1], t[3], 0xEE),
            _mm256_shuffle_ps(t[4], t[6], 0x44),
            _mm256_shuffle_ps(t[4], t[6], 0xEE),
            _mm256_shuffle_ps(t[5], t[7], 0x44),
            _mm256_shuffle_ps(t[5], t[7], 0xEE),
        ]
    }

    /// pass 3: permute 128-bit lanes to finish the transpose.
    #[inline(always)]
    pub unsafe fn pass_permute(s: [__m256; 8]) -> [__m256; 8] {
        [
            _mm256_permute2f128_ps(s[0], s[4], 0x20),
            _mm256_permute2f128_ps(s[1], s[5], 0x20),
            _mm256_permute2f128_ps(s[2], s[6], 0x20),
            _mm256_permute2f128_ps(s[3], s[7], 0x20),
            _mm256_permute2f128_ps(s[0], s[4], 0x31),
            _mm256_permute2f128_ps(s[1], s[5], 0x31),
            _mm256_permute2f128_ps(s[2], s[6], 0x31),
            _mm256_permute2f128_ps(s[3], s[7], 0x31),
        ]
    }

    #[inline(always)]
    pub unsafe fn store_rows(rows: [__m256; 8]) -> [f32; 64] {
        let mut out = [0.0f32; 64];
        let q = out.as_mut_ptr();
        _mm256_storeu_ps(q, rows[0]);
        _mm256_storeu_ps(q.add(8), rows[1]);
        _mm256_storeu_ps(q.add(16), rows[2]);
        _mm256_storeu_ps(q.add(24), rows[3]);
        _mm256_storeu_ps(q.add(32), rows[4]);
        _mm256_storeu_ps(q.add(40), rows[5]);
        _mm256_storeu_ps(q.add(48), rows[6]);
        _mm256_storeu_ps(q.add(56), rows[7]);
        out
    }
}

/// The AVX schedule: load → unpack → shuffle → permute → store, composed.
/// This single line *is* the schedule — recompose the passes to change it.
#[cfg(target_arch = "x86_64")]
#[target_feature(enable = "avx")]
unsafe fn transpose_8x8_avx(input: &[f32; 64]) -> [f32; 64] {
    use avx::*;
    store_rows(pass_permute(pass_shuffle(pass_unpack(load_rows(input)))))
}

/// Transpose an arbitrary `rows × cols` row-major f32 matrix into `out`
/// (`cols × rows`). This is what `almide_rt` actually needs — Almide's matrices
/// aren't 8x8. Full 8x8 tiles go through the SIMD kernel; the ragged right/bottom
/// edges are scalar. Bitwise-exact (data movement, no reassoc).
pub fn transpose_matrix(input: &[f32], rows: usize, cols: usize, out: &mut [f32]) {
    assert_eq!(input.len(), rows * cols);
    assert_eq!(out.len(), rows * cols);
    let rt = rows / 8 * 8; // tiled extent (multiple of 8)
    let ct = cols / 8 * 8;
    let mut tile = [0.0f32; 64];
    let mut ti = 0;
    while ti < rt {
        let mut tj = 0;
        while tj < ct {
            for r in 0..8 {
                let base = (ti + r) * cols + tj;
                tile[r * 8..r * 8 + 8].copy_from_slice(&input[base..base + 8]);
            }
            let t = transpose_8x8(&tile);
            for r in 0..8 {
                let base = (tj + r) * rows + ti;
                out[base..base + 8].copy_from_slice(&t[r * 8..r * 8 + 8]);
            }
            tj += 8;
        }
        ti += 8;
    }
    // ragged edges: any (i, j) not covered by a full tile
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
    fn matrix_transpose_arbitrary_sizes() {
        // includes ragged sizes (not multiples of 8) and non-square
        for &(rows, cols) in &[(8, 8), (16, 16), (13, 8), (8, 13), (37, 41), (1, 5), (100, 7)] {
            let input: Vec<f32> = (0..rows * cols).map(|k| k as f32 * 0.5 - 1.0).collect();
            let mut out = vec![0.0f32; rows * cols];
            transpose_matrix(&input, rows, cols, &mut out);
            for i in 0..rows {
                for j in 0..cols {
                    assert_eq!(out[j * rows + i].to_bits(), input[i * cols + j].to_bits(), "{rows}x{cols} at {i},{j}");
                }
            }
        }
    }

    /// Static equivalence (all inputs) for arbitrary-size transpose: the index
    /// array proves the SIMD-tiled + ragged-scalar path is the transpose
    /// permutation everywhere, for every size, in one run each.
    #[test]
    fn matrix_schedule_is_transpose_permutation_all_inputs() {
        for &(rows, cols) in &[(16, 16), (13, 8), (8, 13), (37, 41), (100, 7), (256, 256)] {
            let index: Vec<f32> = (0..rows * cols).map(|k| k as f32).collect();
            let mut out = vec![0.0f32; rows * cols];
            transpose_matrix(&index, rows, cols, &mut out);
            for i in 0..rows {
                for j in 0..cols {
                    let source = out[j * rows + i] as usize;
                    assert_eq!(source, i * cols + j, "{rows}x{cols} permutation wrong at {i},{j}");
                }
            }
        }
    }

    #[test]
    fn naive_is_a_transpose() {
        let input: [f32; 64] = std::array::from_fn(|k| k as f32);
        let out = transpose_8x8_naive(&input);
        for i in 0..8 {
            for j in 0..8 {
                assert_eq!(out[j * 8 + i], input[i * 8 + j]);
            }
        }
        assert_eq!(transpose_8x8_naive(&out), input); // transpose twice == identity
    }

    #[test]
    fn schedule_matches_algorithm_bitwise() {
        // Every schedule must be bitwise-exact to the algorithm (the spec).
        for seed in 0..100u32 {
            let input: [f32; 64] = std::array::from_fn(|k| {
                let h = (k as u32).wrapping_mul(2654435761).wrapping_add(seed.wrapping_mul(40503));
                (h as f32) * 0.0009765625 - 11.0
            });
            let scheduled = transpose_8x8(&input);
            let algorithm = transpose_8x8_naive(&input);
            for k in 0..64 {
                assert_eq!(
                    scheduled[k].to_bits(),
                    algorithm[k].to_bits(),
                    "seed {seed} index {k}"
                );
            }
        }
    }

    /// Exo-style static equivalence, specialized for a permutation kernel.
    ///
    /// transpose is a permutation: it moves positions, never touching values. So
    /// running it on the index array `input[k] = k` extracts the *entire*
    /// permutation in one shot. If the extracted permutation equals the transpose
    /// spec (`out position j*8+i ← in position i*8+j`), the SIMD schedule is
    /// correct for **every possible input** — not 100 samples, ALL of them.
    ///
    /// This is the static-equivalence guarantee Exo gets from effect analysis +
    /// SMT, but a permutation is decidable without a solver: one index run is a
    /// total proof. f32 represents 0..64 exactly (< 2^24), so `out[p] as usize`
    /// recovers the source index losslessly.
    #[test]
    fn schedule_is_the_transpose_permutation_for_all_inputs() {
        let index_input: [f32; 64] = std::array::from_fn(|k| k as f32);
        let out = transpose_8x8(&index_input);
        for p in 0..64 {
            let source = out[p] as usize; // output position p was filled from input[source]
            let (i, j) = (p % 8, p / 8); // p = j*8 + i
            let spec_source = i * 8 + j; // transpose: out[j*8+i] = in[i*8+j]
            assert_eq!(source, spec_source, "permutation wrong at output position {p}");
        }
    }
}
