//! Dense matmul `C = A·B` (m×k · k×n, row-major). SIMD inner product: transpose
//! B once so each column becomes a contiguous row, then a SIMD dot per (i,j).
//!
//! This beats Almide's tiled-*scalar* mul by vectorizing the inner product. BLAS
//! lives here too — register-tiled micro-kernels (6x16, packing, prefetch) are
//! the ceiling and we don't reach it; this is the SIMD-dot rung, enough to pass
//! a scalar inner loop. Used for sdpa's QKᵀ and weights·V.

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

#[cfg(target_arch = "x86_64")]
#[target_feature(enable = "avx2,fma")]
#[allow(dead_code)]
unsafe fn matmul_avx(a: &[f64], m: usize, k: usize, b: &[f64], n: usize, out: &mut [f64]) {
    use std::arch::x86_64::*;
    // transpose B → bt (n×k), columns become contiguous rows
    let mut bt = vec![0.0f64; n * k];
    for kk in 0..k {
        let brow = &b[kk * n..kk * n + n];
        for j in 0..n {
            bt[j * k + kk] = brow[j];
        }
    }
    let chunks = k / 4;
    for i in 0..m {
        let ai = &a[i * k..i * k + k];
        for j in 0..n {
            let bj = &bt[j * k..j * k + k];
            let mut acc = _mm256_setzero_pd();
            for c in 0..chunks {
                let av = _mm256_loadu_pd(ai.as_ptr().add(c * 4));
                let bv = _mm256_loadu_pd(bj.as_ptr().add(c * 4));
                acc = _mm256_fmadd_pd(av, bv, acc);
            }
            let hi = _mm256_extractf128_pd(acc, 1);
            let lo = _mm256_castpd256_pd128(acc);
            let s = _mm_add_pd(lo, hi);
            let s = _mm_add_sd(s, _mm_unpackhi_pd(s, s));
            let mut sum = _mm_cvtsd_f64(s);
            for kk in (chunks * 4)..k {
                sum += ai[kk] * bj[kk];
            }
            out[i * n + j] = sum;
        }
    }
}

/// Register-tiled matmul: 4×4 micro-kernel. The 4×4 block of C lives in 4 ymm
/// accumulators; the k-loop loads one B row (contiguous, 4 doubles) and fma's it
/// into all 4 C rows (B reuse), broadcasting one A element per row. This is BLAS's
/// register-blocking core — C stays in registers across k, memory traffic is just
/// A column + B row. Ragged edges (m%4, n%4) fall back to scalar.
#[cfg(target_arch = "x86_64")]
#[target_feature(enable = "avx2,fma")]
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
                c0 = _mm256_fmadd_pd(_mm256_set1_pd(*a.get_unchecked((i0) * k + kk)), brow, c0);
                c1 = _mm256_fmadd_pd(_mm256_set1_pd(*a.get_unchecked((i0 + 1) * k + kk)), brow, c1);
                c2 = _mm256_fmadd_pd(_mm256_set1_pd(*a.get_unchecked((i0 + 2) * k + kk)), brow, c2);
                c3 = _mm256_fmadd_pd(_mm256_set1_pd(*a.get_unchecked((i0 + 3) * k + kk)), brow, c3);
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
        if is_x86_feature_detected!("avx2") && is_x86_feature_detected!("fma") {
            return unsafe { matmul_register_tiled(a, m, k, b, n, out) };
        }
    }
    matmul_naive(a, m, k, b, n, out);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn matmul_matches_naive() {
        // a: 5x7, b: 7x4
        let (m, k, n) = (5, 7, 4);
        let a: Vec<f64> = (0..m * k).map(|i| i as f64 * 0.1 - 1.0).collect();
        let b: Vec<f64> = (0..k * n).map(|i| i as f64 * 0.07 - 0.5).collect();
        let mut simd = vec![0.0; m * n];
        let mut naive = vec![0.0; m * n];
        matmul(&a, m, k, &b, n, &mut simd);
        matmul_naive(&a, m, k, &b, n, &mut naive);
        for i in 0..m * n {
            assert!((simd[i] - naive[i]).abs() <= 1e-12 * naive[i].abs().max(1.0), "i={i}");
        }
    }
}
