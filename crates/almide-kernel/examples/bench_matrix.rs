// Arbitrary-size transpose (what almide_rt needs): 512x512, vs Rust naive + Almide f64.
use almide_kernel::transpose::transpose_matrix;
use std::hint::black_box;
use std::time::Instant;

fn transpose_naive_f32(input: &[f32], rows: usize, cols: usize, out: &mut [f32]) {
    for i in 0..rows { for j in 0..cols { out[j * rows + i] = input[i * cols + j]; } }
}
fn transpose_naive_f64(input: &[f64], rows: usize, cols: usize, out: &mut [f64]) {
    for i in 0..rows { for j in 0..cols { out[j * rows + i] = input[i * cols + j]; } }
}

fn main() {
    let arch = if cfg!(target_arch = "wasm32") { "wasm (simd128)" } else { "native (AVX)" };
    let (rows, cols) = (512usize, 512usize);
    let mut xf: Vec<f32> = (0..rows * cols).map(|k| (k % 97) as f32 * 0.3 - 7.0).collect();
    let mut xd: Vec<f64> = (0..rows * cols).map(|k| (k % 97) as f64 * 0.3 - 7.0).collect();
    let mut out = vec![0.0f32; rows * cols];
    let mut outd = vec![0.0f64; rows * cols];
    let reps = 8000u64;

    let t = Instant::now();
    for r in 0..reps { xf[0] = black_box(r as f32); transpose_naive_f32(black_box(&xf), rows, cols, &mut out); black_box(&out); }
    let rust_naive = t.elapsed();
    let t = Instant::now();
    for r in 0..reps { xd[0] = black_box(r as f64); transpose_naive_f64(black_box(&xd), rows, cols, &mut outd); black_box(&outd); }
    let almide = t.elapsed();
    let t = Instant::now();
    for r in 0..reps { xf[0] = black_box(r as f32); transpose_matrix(black_box(&xf), rows, cols, &mut out); black_box(&out); }
    let kernel = t.elapsed();

    println!("[{arch}] transpose {rows}x{cols}");
    println!("  Rust naive (f32): {rust_naive:?}");
    println!("  Almide     (f64): {almide:?}");
    println!("  almide-kernel:    {kernel:?}");
    println!("  → vs Rust:   {:.2}x", rust_naive.as_secs_f64() / kernel.as_secs_f64());
    println!("  → vs Almide: {:.2}x", almide.as_secs_f64() / kernel.as_secs_f64());
}
