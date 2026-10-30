//! Dense matmul `C = A·B` (m×k · k×n, row-major). On x86_64 with AVX a
//! register-tiled 4×4 micro-kernel; elsewhere the tiled-scalar reference.
//!
//! Every path computes each cell in ONE order: `s = 0.0`, then for `kk`
//! ascending `s = s + a[i][kk] * b[kk][j]` — the product rounded, then the
//! sum rounded, never a fused multiply-add (C-303, #4344). That is the order
//! the wasm leg (`crates/almide-wasm/src/matrix_prod.rs`) and the self-hosted
//! bodies spell, so the product is bitwise the same on every target and on
//! every CPU, with or without FMA hardware.
//!
//! This beats Almide's tiled-*scalar* mul by keeping a C block in registers.
//! BLAS lives here too — 6x16 micro-kernels, packing and prefetch are the
//! ceiling and we don't reach it. Used for sdpa's QKᵀ and weights·V. (The
//! transpose-B SIMD-dot rung that preceded the register tile was dead code and
//! left with #4342.)

/// Row-major shape of the product `C(m×n) = A(m×k)·B(k×n)`. Bundles the three
/// extents so the x86 kernel's edge pass stays inside the parameter budget.
#[cfg(target_arch = "x86_64")]
#[derive(Clone, Copy)]
struct Dims {
    m: usize,
    k: usize,
    n: usize,
}

/// Naive tiled-scalar reference (≈ Almide's mul).
pub fn matmul_naive(a: &[f64], m: usize, k: usize, b: &[f64], n: usize, out: &mut [f64]) {
    out.fill(0.0);
    const T: usize = 32;
    for i0 in (0..m).step_by(T) {
        for k0 in (0..k).step_by(T) {
            for j0 in (0..n).step_by(T) {
                let tile = Tile {
                    rows: i0..(i0 + T).min(m),
                    depth: k0..(k0 + T).min(k),
                    cols: j0..(j0 + T).min(n),
                };
                accumulate_tile(a, k, b, n, out, &tile);
            }
        }
    }
}

/// One cache tile of the tiled reference: the (rows × depth × cols) index box
/// the three outer loops carved out.
struct Tile {
    rows: std::ops::Range<usize>,
    depth: std::ops::Range<usize>,
    cols: std::ops::Range<usize>,
}

/// `C[tile.rows, tile.cols] += A[tile.rows, tile.depth] · B[tile.depth, tile.cols]`.
///
/// The column span is taken as a slice pair so the innermost loop walks two
/// contiguous rows — same operation order as the index form it replaced, so the
/// result stays bit-identical to what the Wyve proof pins.
fn accumulate_tile(a: &[f64], k: usize, b: &[f64], n: usize, out: &mut [f64], tile: &Tile) {
    let (c0, c1) = (tile.cols.start, tile.cols.end);
    for i in tile.rows.clone() {
        for kk in tile.depth.clone() {
            let aik = a[i * k + kk];
            let orow = &mut out[i * n + c0..i * n + c1];
            let brow = &b[kk * n + c0..kk * n + c1];
            for (o, bv) in orow.iter_mut().zip(brow) {
                *o += aik * bv;
            }
        }
    }
}

/// Register-tiled matmul: 4×4 micro-kernel. The 4×4 block of C lives in 4 ymm
/// accumulators; the k-loop loads one B row (contiguous, 4 doubles), multiplies
/// it by one broadcast A element per C row and adds the product into that row
/// (B reuse). This is BLAS's register-blocking core — C stays in registers
/// across k, memory traffic is just A column + B row. Ragged edges (m%4, n%4)
/// fall back to scalar.
///
/// Multiply, then add: `_mm256_mul_pd` then `_mm256_add_pd(acc, product)`, two
/// roundings per step exactly like the scalar `s += x * y`. It used
/// `_mm256_fmadd_pd` (one rounding), and on an FMA CPU the native product
/// differed from the wasm one in the last bit (#4344). Only AVX is needed, so
/// the target feature no longer enables `fma`: nothing in the body can be
/// contracted into one.
#[cfg(target_arch = "x86_64")]
#[target_feature(enable = "avx")]
unsafe fn matmul_register_tiled(a: &[f64], m: usize, k: usize, b: &[f64], n: usize, out: &mut [f64]) {
    use std::arch::x86_64::*;
    let mt = m / 4 * 4;
    let nt = n / 4 * 4;
    for i0 in (0..mt).step_by(4) {
        for j0 in (0..nt).step_by(4) {
            let mut c0 = _mm256_setzero_pd();
            let mut c1 = _mm256_setzero_pd();
            let mut c2 = _mm256_setzero_pd();
            let mut c3 = _mm256_setzero_pd();
            for kk in 0..k {
                let brow = _mm256_loadu_pd(b.as_ptr().add(kk * n + j0));
                c0 = _mm256_add_pd(c0, _mm256_mul_pd(_mm256_set1_pd(*a.get_unchecked((i0) * k + kk)), brow));
                c1 = _mm256_add_pd(c1, _mm256_mul_pd(_mm256_set1_pd(*a.get_unchecked((i0 + 1) * k + kk)), brow));
                c2 = _mm256_add_pd(c2, _mm256_mul_pd(_mm256_set1_pd(*a.get_unchecked((i0 + 2) * k + kk)), brow));
                c3 = _mm256_add_pd(c3, _mm256_mul_pd(_mm256_set1_pd(*a.get_unchecked((i0 + 3) * k + kk)), brow));
            }
            _mm256_storeu_pd(out.as_mut_ptr().add((i0) * n + j0), c0);
            _mm256_storeu_pd(out.as_mut_ptr().add((i0 + 1) * n + j0), c1);
            _mm256_storeu_pd(out.as_mut_ptr().add((i0 + 2) * n + j0), c2);
            _mm256_storeu_pd(out.as_mut_ptr().add((i0 + 3) * n + j0), c3);
        }
    }
    scalar_edges(a, b, out, Dims { m, k, n }, mt, nt);
}

/// The (i, j) cells the 4×4 tiling did not cover: the bottom `m % 4` rows in
/// full, plus the right `n % 4` columns of every tiled row. Plain scalar dots —
/// no intrinsics, so this half of the kernel is safe code.
#[cfg(target_arch = "x86_64")]
fn scalar_edges(a: &[f64], b: &[f64], out: &mut [f64], d: Dims, mt: usize, nt: usize) {
    for i in 0..d.m {
        let first_col = if i < mt { nt } else { 0 };
        for j in first_col..d.n {
            let mut s = 0.0;
            for kk in 0..d.k {
                s += a[i * d.k + kk] * b[kk * d.n + j];
            }
            out[i * d.n + j] = s;
        }
    }
}

/// Per-target dispatch.
pub fn matmul(a: &[f64], m: usize, k: usize, b: &[f64], n: usize, out: &mut [f64]) {
    #[cfg(target_arch = "x86_64")]
    {
        if is_x86_feature_detected!("avx") {
            return unsafe { matmul_register_tiled(a, m, k, b, n, out) };
        }
    }
    matmul_naive(a, m, k, b, n, out);
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The cell order every leg spells (C-303): `0.0`, then `s + a·b` for
    /// `kk` ascending, product and sum each rounded once.
    fn reference(a: &[f64], m: usize, k: usize, b: &[f64], n: usize) -> Vec<f64> {
        let mut out = vec![0.0; m * n];
        for i in 0..m {
            for j in 0..n {
                let mut s = 0.0f64;
                for kk in 0..k {
                    s += a[i * k + kk] * b[kk * n + j];
                }
                out[i * n + j] = s;
            }
        }
        out
    }

    fn bits(v: &[f64]) -> Vec<u64> {
        v.iter().map(|x| x.to_bits()).collect()
    }

    /// Bitwise, not within a tolerance: the dispatched kernel (the AVX tile on
    /// an AVX CPU), the tiled-scalar fallback and the reference agree on every
    /// bit, for shapes with and without ragged edges and depths that cross the
    /// fallback's 32-wide k tiles.
    #[test]
    fn matmul_is_bitwise_the_reference_order() {
        for &(m, k, n) in &[(5, 7, 4), (4, 4, 4), (8, 33, 12), (13, 65, 9), (1, 3, 1), (16, 100, 20)] {
            let a: Vec<f64> = (0..m * k).map(|i| ((i * 37 % 101) as f64 - 50.0) * 0.013 + 1.0 / (i as f64 + 3.0)).collect();
            let b: Vec<f64> = (0..k * n).map(|i| ((i * 53 % 89) as f64 - 44.0) * 0.021 - 1.0 / (i as f64 + 7.0)).collect();
            let want = reference(&a, m, k, &b, n);
            let mut got = vec![f64::NAN; m * n];
            matmul(&a, m, k, &b, n, &mut got);
            assert_eq!(bits(&got), bits(&want), "dispatched kernel, {m}x{k}x{n}");
            let mut naive = vec![f64::NAN; m * n];
            matmul_naive(&a, m, k, &b, n, &mut naive);
            assert_eq!(bits(&naive), bits(&want), "tiled-scalar fallback, {m}x{k}x{n}");
        }
    }

    /// #4344's probe: row `[-1, 1 + 2^-52]` times column `[1, 1 - 2^-53]`.
    /// Unfused, the second product rounds to exactly 1.0 and the cell is 0.0;
    /// a fused multiply-add leaves `2^-53 - 2^-105`. 4x2 times 2x4 puts every
    /// cell in the AVX tile; 5x2 times 2x5 adds the scalar edges.
    #[test]
    fn no_fused_multiply_add_in_any_cell() {
        let p = f64::from_bits(4607182418800017409); // 1 + 2^-52
        let q = f64::from_bits(4607182418800017407); // 1 - 2^-53
        for &(m, n) in &[(4, 4), (5, 5), (8, 4)] {
            let a: Vec<f64> = (0..m).flat_map(|_| [-1.0, p]).collect();
            let b: Vec<f64> = (0..n).map(|_| 1.0).chain((0..n).map(|_| q)).collect();
            let mut out = vec![f64::NAN; m * n];
            matmul(&a, m, 2, &b, n, &mut out);
            assert!(out.iter().all(|c| c.to_bits() == 0), "{m}x2x{n}: {out:?}");
        }
    }

    /// The summation order, not only the fusion: `[1, 1e16, -1e16]` summed
    /// left to right is `(1 + 1e16) - 1e16 = 0` (the 1 rounds away), any other
    /// association that pairs the two large terms first gives 1.
    #[test]
    fn cells_sum_in_ascending_k_order() {
        let (m, k, n) = (4, 3, 4);
        let a: Vec<f64> = (0..m).flat_map(|_| [1.0, 1e16, -1e16]).collect();
        let b = vec![1.0; k * n];
        let mut out = vec![f64::NAN; m * n];
        matmul(&a, m, k, &b, n, &mut out);
        assert!(out.iter().all(|c| c.to_bits() == 0), "{out:?}");
    }
}
