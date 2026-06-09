use almide_kernel::q1_0::{q1_0_dot, q1_0_dot_naive};
use std::hint::black_box;
use std::time::Instant;

// Almide's q1_0_block_dot is f64 and scalar on x86/wasm (its SIMD path is NEON-only).
fn q1_0_dot_almide_f64(x: &[f64; 128], sign: &[u8; 16], scale: f64) -> f64 {
    let mut acc = 0.0f64;
    for k in 0..128 {
        let bit = (sign[k / 8] >> (k % 8)) & 1;
        acc += if bit == 1 { -x[k] } else { x[k] };
    }
    acc * scale
}

fn main() {
    let arch = if cfg!(target_arch = "wasm32") { "wasm (simd128)" } else { "native (AVX2)" };
    let mut xf: [f32; 128] = std::array::from_fn(|k| (k as f32) * 0.01 - 0.6);
    let mut xd: [f64; 128] = std::array::from_fn(|k| (k as f64) * 0.01 - 0.6);
    let sign: [u8; 16] = std::array::from_fn(|b| (b * 37 + 11) as u8);
    let reps = 20_000_000u64;

    let mut s = 0.0f32;
    let t = Instant::now();
    for n in 0..reps { xf[0] = black_box(n as f32 * 1e-9); s += q1_0_dot_naive(black_box(&xf), black_box(&sign), 0.0123); }
    let rust_naive = t.elapsed();

    let mut sd = 0.0f64;
    let t = Instant::now();
    for n in 0..reps { xd[0] = black_box(n as f64 * 1e-9); sd += q1_0_dot_almide_f64(black_box(&xd), black_box(&sign), 0.0123); }
    let almide = t.elapsed();

    let mut s2 = 0.0f32;
    let t = Instant::now();
    for n in 0..reps { xf[0] = black_box(n as f32 * 1e-9); s2 += q1_0_dot(black_box(&xf), black_box(&sign), 0.0123); }
    let kernel = t.elapsed();

    println!("[{arch}]  reps={reps}");
    println!("  Rust naive (f32 scalar): {rust_naive:?}");
    println!("  Almide     (f64 scalar): {almide:?}");
    println!("  almide-kernel SIMD:      {kernel:?}");
    println!("  → vs Rust:   {:.2}x", rust_naive.as_secs_f64() / kernel.as_secs_f64());
    println!("  → vs Almide: {:.2}x", almide.as_secs_f64() / kernel.as_secs_f64());
    println!("  (sink {s} {sd} {s2})");
}
