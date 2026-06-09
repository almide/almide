// scale: elementwise. `scale_avx` is x86-only (measurement); elsewhere scale
// stays naive (autovec is enough — measured tie at target-cpu=native). On wasm
// this bench just runs the naive path.
use almide_kernel::scale::scale;
use std::hint::black_box;
use std::time::Instant;

fn main() {
    let n = 8192usize;
    let mut x: Vec<f32> = (0..n).map(|k| (k % 100) as f32 * 0.01).collect();
    let mut out = vec![0.0f32; n];
    let reps = 300_000u64;

    let t = Instant::now();
    for m in 0..reps {
        x[0] = black_box(m as f32);
        scale(1.0000001, black_box(&x), &mut out);
        black_box(&out);
    }
    let naive = t.elapsed();
    println!("scale naive (autovec): {naive:?}");

    #[cfg(target_arch = "x86_64")]
    {
        use almide_kernel::scale::scale_avx;
        let t = Instant::now();
        for m in 0..reps {
            x[0] = black_box(m as f32);
            unsafe { scale_avx(1.0000001, black_box(&x), &mut out) };
            black_box(&out);
        }
        let avx = t.elapsed();
        println!("scale explicit AVX:    {avx:?}");
        println!(
            "AVX/naive: {:.2}x  (ties at -C target-cpu=native; default favors AVX only \
             because autovec caps at the SSE2 baseline — so scale ships naive)",
            naive.as_secs_f64() / avx.as_secs_f64()
        );
    }
    #[cfg(not(target_arch = "x86_64"))]
    println!("(scale_avx is x86-only; elementwise stays naive elsewhere — autovec is enough)");
}
