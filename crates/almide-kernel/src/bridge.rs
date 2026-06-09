//! ABI glue — call almide-kernel from Almide's runtime matrix type.
//!
//! Almide's lightweight `AlmideMatrix` is `Vec<Vec<f64>>` (nested, f64). This
//! bridges it to the flat f64 kernels: nested→flat in, kernel, flat→nested out.
//! This is exactly the body `almide_rt_matrix_transpose` would have once
//! almide-kernel is a dependency — written and tested here, standalone, without
//! touching Almide's prelude-injected build.
//!
//! MEASURED, and it's a load-bearing finding: through the **nested** ABI
//! (`Vec<Vec<f64>>`) this path is **0.44× — slower than naive**. The nested→flat
//! and flat→nested conversions (two full copies + cols Vec allocs) cost more than
//! the SIMD transpose saves. The kernel itself wins (flat f64: 3.13× vs Almide,
//! see bench_matrix) — the loss is entirely the nested ABI.
//!
//! So the production lesson is about Almide's *type*, not the kernel: to get the
//! SIMD win, Almide must hand the kernel a **flat** buffer (its SmallF32 / a flat
//! f64 matrix), not `Vec<Vec<f64>>`. With a flat ABI there's no conversion and
//! the 3.13× lands. Wiring almide-kernel pays off only if the matrix is flat.

use crate::transpose_f64::transpose_matrix_f64;

/// Almide's lightweight matrix ABI (row-major, nested, f64).
pub type AlmideMatrix = Vec<Vec<f64>>;

/// `almide_rt_matrix_transpose` realized through almide-kernel.
/// Bitwise-exact to a naive transpose (data movement, f64 throughout — no
/// rounding). Drop-in replacement for the runtime's transpose.
pub fn almide_matrix_transpose(m: &AlmideMatrix) -> AlmideMatrix {
    let rows = m.len();
    let cols = if rows > 0 { m[0].len() } else { 0 };
    if rows == 0 || cols == 0 {
        return Vec::new();
    }
    // nested -> flat (one alloc)
    let mut flat = vec![0.0f64; rows * cols];
    for (i, row) in m.iter().enumerate() {
        debug_assert_eq!(row.len(), cols, "ragged AlmideMatrix");
        flat[i * cols..i * cols + cols].copy_from_slice(row);
    }
    // kernel
    let mut out_flat = vec![0.0f64; rows * cols];
    transpose_matrix_f64(&flat, rows, cols, &mut out_flat);
    // flat (cols x rows) -> nested
    (0..cols)
        .map(|j| out_flat[j * rows..j * rows + rows].to_vec())
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn naive(m: &AlmideMatrix) -> AlmideMatrix {
        let rows = m.len();
        let cols = if rows > 0 { m[0].len() } else { 0 };
        (0..cols).map(|j| (0..rows).map(|i| m[i][j]).collect()).collect()
    }

    #[test]
    fn bridge_matches_naive_bitwise() {
        for &(rows, cols) in &[(8, 8), (16, 16), (13, 8), (8, 13), (37, 41), (1, 5), (100, 7)] {
            let m: AlmideMatrix = (0..rows)
                .map(|i| (0..cols).map(|j| (i * cols + j) as f64 * 0.3 - 5.0).collect())
                .collect();
            let via_kernel = almide_matrix_transpose(&m);
            let reference = naive(&m);
            assert_eq!(via_kernel.len(), cols);
            for j in 0..cols {
                for i in 0..rows {
                    assert_eq!(
                        via_kernel[j][i].to_bits(),
                        reference[j][i].to_bits(),
                        "{rows}x{cols} at [{j}][{i}]"
                    );
                }
            }
        }
    }
}
