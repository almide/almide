// Full quantized linear (inference hot path): M=1 token, K=2048, N=2048.
use almide_kernel::q1_0::{linear_q1_0, linear_q1_0_naive};
use std::hint::black_box;
use std::time::Instant;

fn main() {
    let arch = if cfg!(target_arch = "wasm32") { "wasm (simd128)" } else { "native (AVX2)" };
    let (m, k, n) = (1usize, 2048usize, 2048usize);
    let blocks = k / 128;
    let mut x: Vec<f32> = (0..m * k).map(|i| (i % 100) as f32 * 0.01 - 0.5).collect();
    let w_sign: Vec<u8> = (0..n * blocks * 16).map(|i| (i * 37 + 11) as u8).collect();
    let w_scale: Vec<f32> = (0..n * blocks).map(|i| 0.01 + (i % 7) as f32 * 0.001).collect();
    let mut out = vec![0.0f32; m * n];

    // correctness: SIMD linear vs naive linear (reassoc reduction → tolerance)
    let mut out_n = vec![0.0f32; m * n];
    linear_q1_0(&x, m, k, &w_sign, &w_scale, n, &mut out);
    linear_q1_0_naive(&x, m, k, &w_sign, &w_scale, n, &mut out_n);
    let mut maxrel = 0.0f32;
    for i in 0..m * n {
        let mag = out_n[i].abs().max(1.0);
        maxrel = maxrel.max((out[i] - out_n[i]).abs() / mag);
    }
    println!("[{arch}] linear {m}x{k}x{n}  max rel err: {maxrel:.2e}");

    let reps = 4000u64;
    let t = Instant::now();
    for r in 0..reps { x[0] = black_box(r as f32 * 1e-6); linear_q1_0_naive(black_box(&x), m, k, &w_sign, &w_scale, n, &mut out); black_box(&out); }
    let rust_naive = t.elapsed();
    let t = Instant::now();
    for r in 0..reps { x[0] = black_box(r as f32 * 1e-6); linear_q1_0(black_box(&x), m, k, &w_sign, &w_scale, n, &mut out); black_box(&out); }
    let kernel = t.elapsed();

    println!("  Rust naive:       {rust_naive:?}");
    println!("  almide-kernel:    {kernel:?}");
    println!("  → vs Rust: {:.2}x", rust_naive.as_secs_f64() / kernel.as_secs_f64());
    let gflop = (reps as f64) * (m * n * k * 2) as f64 / kernel.as_secs_f64() / 1e9;
    println!("  almide-kernel: {gflop:.1} GFLOP/s");
}
