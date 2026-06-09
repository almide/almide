// silu_mul: x*sigmoid(x)*b. SIMD fast-exp vs scalar libm (=Almide's path).
use almide_kernel::silu::{silu_mul, silu_mul_naive};
use std::hint::black_box;
use std::time::Instant;
fn main() {
    let arch = if cfg!(target_arch = "wasm32") { "wasm" } else { "native (AVX f64 fast-exp)" };
    let n = 4096 * 512; // an FFN-ish chunk
    let mut a: Vec<f64> = (0..n).map(|k| (k % 1000) as f64 * 0.01 - 5.0).collect();
    let b: Vec<f64> = (0..n).map(|k| (k % 777) as f64 * 0.01 - 3.0).collect();
    let mut out = vec![0.0f64; n];
    let reps = 60u64;
    let t = Instant::now();
    for r in 0..reps { a[0] = black_box(r as f64 * 1e-9); silu_mul_naive(black_box(&a), &b, &mut out); black_box(&out); }
    let naive = t.elapsed();
    let t = Instant::now();
    for r in 0..reps { a[0] = black_box(r as f64 * 1e-9); silu_mul(black_box(&a), &b, &mut out); black_box(&out); }
    let simd = t.elapsed();
    println!("[{arch}] silu_mul, n={n}");
    println!("  scalar libm (=Almide): {naive:?}");
    println!("  almide-kernel SIMD:    {simd:?}");
    println!("  → {:.2}x", naive.as_secs_f64() / simd.as_secs_f64());
}
