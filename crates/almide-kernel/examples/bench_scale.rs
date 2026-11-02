// scale: elementwise, naive (autovec is enough — the explicit AVX form measured
// a tie at target-cpu=native and was unsafe and removed, #4342).
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
}
