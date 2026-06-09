use almide_kernel::gelu::{gelu, gelu_naive};
use std::hint::black_box; use std::time::Instant;
fn main() {
    let n = 4096*512;
    let mut d: Vec<f64> = (0..n).map(|k| (k%1000) as f64*0.01-5.0).collect();
    let mut o = vec![0.0f64; n]; let reps=60u64;
    let t=Instant::now(); for r in 0..reps { d[0]=black_box(r as f64*1e-9); gelu_naive(black_box(&d),&mut o); black_box(&o); } let nv=t.elapsed();
    let t=Instant::now(); for r in 0..reps { d[0]=black_box(r as f64*1e-9); gelu(black_box(&d),&mut o); black_box(&o); } let sv=t.elapsed();
    println!("gelu n={n}: libm {nv:?} / SIMD {sv:?} → {:.2}x", nv.as_secs_f64()/sv.as_secs_f64());
}
