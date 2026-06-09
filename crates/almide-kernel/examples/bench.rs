use almide_kernel::transpose::{transpose_8x8, transpose_8x8_naive};
use std::hint::black_box;
use std::time::Instant;

// Almide's almide_rt_matrix_transpose: naive, f64, Vec<Vec<f64>>-shaped (here flat f64).
fn transpose_8x8_almide_f64(input: &[f64; 64]) -> [f64; 64] {
    let mut out = [0.0f64; 64];
    for i in 0..8 {
        for j in 0..8 {
            out[j * 8 + i] = input[i * 8 + j];
        }
    }
    out
}

fn main() {
    let arch = if cfg!(target_arch = "wasm32") { "wasm (simd128)" } else { "native (AVX)" };
    // bitwise-exact check (transpose is data movement → no reassoc)
    let probe: [f32; 64] = std::array::from_fn(|k| (k as f32) * 1.7 - 13.0);
    let ok = transpose_8x8(&probe)
        .iter()
        .zip(transpose_8x8_naive(&probe).iter())
        .all(|(a, b)| a.to_bits() == b.to_bits());
    println!("[{arch}] bitwise-exact vs naive: {}", if ok { "OK" } else { "MISMATCH" });

    let mut xf: [f32; 64] = std::array::from_fn(|k| (k as f32) * 0.5 - 3.0);
    let mut xd: [f64; 64] = std::array::from_fn(|k| (k as f64) * 0.5 - 3.0);
    let reps = 120_000_000u64;

    let t = Instant::now();
    for n in 0..reps { xf[0] = black_box(n as f32); black_box(&transpose_8x8_naive(black_box(&xf))); }
    let rust_naive = t.elapsed();

    let t = Instant::now();
    for n in 0..reps { xd[0] = black_box(n as f64); black_box(&transpose_8x8_almide_f64(black_box(&xd))); }
    let almide = t.elapsed();

    let t = Instant::now();
    for n in 0..reps { xf[0] = black_box(n as f32); black_box(&transpose_8x8(black_box(&xf))); }
    let kernel = t.elapsed();

    println!("  Rust naive (f32): {rust_naive:?}");
    println!("  Almide     (f64): {almide:?}");
    println!("  almide-kernel:    {kernel:?}");
    println!("  → vs Rust:   {:.2}x", rust_naive.as_secs_f64() / kernel.as_secs_f64());
    println!("  → vs Almide: {:.2}x", almide.as_secs_f64() / kernel.as_secs_f64());
}
