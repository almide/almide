use almide_kernel::softmax::{softmax_rows, softmax_rows_naive};
use std::hint::black_box; use std::time::Instant;
fn main() {
    let (rows, cols) = (2048usize, 2048usize); // attention scores
    let mut data: Vec<f64> = (0..rows*cols).map(|k| (k%256) as f64 * 0.02 - 2.5).collect();
    let mut out = vec![0.0f64; rows*cols]; let reps = 40u64;
    let t = Instant::now();
    for r in 0..reps { data[0]=black_box(r as f64*1e-9); softmax_rows_naive(black_box(&data), rows, cols, &mut out); black_box(&out); }
    let nv = t.elapsed();
    let t = Instant::now();
    for r in 0..reps { data[0]=black_box(r as f64*1e-9); softmax_rows(black_box(&data), rows, cols, &mut out); black_box(&out); }
    let sv = t.elapsed();
    println!("softmax {rows}x{cols}: libm {nv:?} / SIMD {sv:?} → {:.2}x", nv.as_secs_f64()/sv.as_secs_f64());
}
