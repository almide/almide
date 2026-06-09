// Two ABIs, same kernel. nested Vec<Vec<f64>> vs flat Vec<f64>. 512x512.
use almide_kernel::bridge::{almide_matrix_transpose, AlmideMatrix};
use almide_kernel::transpose_f64::transpose_matrix_f64;
use std::hint::black_box;
use std::time::Instant;

fn almide_naive_nested(m: &AlmideMatrix) -> AlmideMatrix {
    let (rows, cols) = (m.len(), m[0].len());
    (0..cols).map(|j| (0..rows).map(|i| m[i][j]).collect()).collect()
}
fn almide_naive_flat(input: &[f64], rows: usize, cols: usize, out: &mut [f64]) {
    for i in 0..rows { for j in 0..cols { out[j * rows + i] = input[i * cols + j]; } }
}

fn main() {
    let arch = if cfg!(target_arch = "wasm32") { "wasm" } else { "native (AVX f64x4)" };
    let (rows, cols) = (512usize, 512usize);
    println!("[{arch}] transpose {rows}x{cols}, f64");

    // ---- nested ABI (Vec<Vec<f64>>) ----
    let mut mn: AlmideMatrix = (0..rows).map(|i| (0..cols).map(|j| (i*cols+j) as f64 * 0.3).collect()).collect();
    let reps = 3000u64;
    let t = Instant::now();
    for r in 0..reps { mn[0][0] = black_box(r as f64); black_box(almide_naive_nested(black_box(&mn))); }
    let n_almide = t.elapsed();
    let t = Instant::now();
    for r in 0..reps { mn[0][0] = black_box(r as f64); black_box(almide_matrix_transpose(black_box(&mn))); }
    let n_kernel = t.elapsed();
    println!("  nested ABI: Almide {n_almide:?}  kernel {n_kernel:?}  → {:.2}x", n_almide.as_secs_f64()/n_kernel.as_secs_f64());

    // ---- flat ABI (Vec<f64>) ----
    let mut mf: Vec<f64> = (0..rows*cols).map(|k| k as f64 * 0.3).collect();
    let mut out = vec![0.0f64; rows*cols];
    let t = Instant::now();
    for r in 0..reps { mf[0] = black_box(r as f64); almide_naive_flat(black_box(&mf), rows, cols, &mut out); black_box(&out); }
    let f_almide = t.elapsed();
    let t = Instant::now();
    for r in 0..reps { mf[0] = black_box(r as f64); transpose_matrix_f64(black_box(&mf), rows, cols, &mut out); black_box(&out); }
    let f_kernel = t.elapsed();
    println!("  flat ABI:   Almide {f_almide:?}  kernel {f_kernel:?}  → {:.2}x", f_almide.as_secs_f64()/f_kernel.as_secs_f64());
}
