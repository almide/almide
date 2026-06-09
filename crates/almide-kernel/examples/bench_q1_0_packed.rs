// Quantized linear in Almide's ABI (packed f64). almide-kernel AVX f64 vs the
// naive scalar = Almide's own x86 path. 1x2048x2048 (inference hot path).
use almide_kernel::q1_0_packed::{fp16_to_f64, linear_q1_0_packed, q1_0_block_dot_packed_naive};
use std::hint::black_box;
use std::time::Instant;

fn linear_naive(x: &[f64], xr: usize, n_in: usize, w: &[u8], woff: usize, wr: usize, out: &mut [f64]) {
    let blocks = n_in / 128;
    for i in 0..xr {
        for j in 0..wr {
            let row_off = woff + j * blocks * 18;
            let mut sum = 0.0;
            for b in 0..blocks {
                let bs = row_off + b * 18;
                let scale = fp16_to_f64(w[bs], w[bs + 1]);
                sum += q1_0_block_dot_packed_naive(&x[i * n_in + b * 128..i * n_in + b * 128 + 128], &w[bs + 2..bs + 18], scale);
            }
            out[i * wr + j] = sum;
        }
    }
}

fn main() {
    let arch = if cfg!(target_arch = "wasm32") { "wasm" } else { "native (AVX f64)" };
    let (m, k, n) = (1usize, 2048usize, 2048usize);
    let blocks = k / 128;
    let mut x: Vec<f64> = (0..m * k).map(|i| (i % 100) as f64 * 0.01 - 0.5).collect();
    let w: Vec<u8> = (0..n * blocks * 18).map(|i| ((i * 37 + 11) % 256) as u8).collect();
    let mut out = vec![0.0f64; m * n];
    let reps = 1500u64;

    let t = Instant::now();
    for r in 0..reps { x[0] = black_box(r as f64 * 1e-9); linear_naive(black_box(&x), m, k, &w, 0, n, &mut out); black_box(&out); }
    let naive = t.elapsed();
    let t = Instant::now();
    for r in 0..reps { x[0] = black_box(r as f64 * 1e-9); linear_q1_0_packed(black_box(&x), m, k, &w, 0, n, &mut out); black_box(&out); }
    let kernel = t.elapsed();

    println!("[{arch}] linear_q1_0 packed f64, {m}x{k}x{n}");
    println!("  Almide scalar (=naive): {naive:?}");
    println!("  almide-kernel (AVX f64): {kernel:?}");
    println!("  → vs Almide scalar: {:.2}x", naive.as_secs_f64() / kernel.as_secs_f64());
}
