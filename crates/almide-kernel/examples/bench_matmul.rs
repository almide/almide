use almide_kernel::matmul::{matmul, matmul_naive};
use std::hint::black_box; use std::time::Instant;
fn main() {
    let arch = if cfg!(target_arch="wasm32") {"wasm"} else {"native (AVX f64)"};
    let (m,k,n) = (256usize, 256usize, 256usize);
    let mut a: Vec<f64> = (0..m*k).map(|i| (i%100) as f64*0.01).collect();
    let b: Vec<f64> = (0..k*n).map(|i| (i%97) as f64*0.01).collect();
    let mut out = vec![0.0f64; m*n]; let reps=80u64;
    let t=Instant::now(); for r in 0..reps { a[0]=black_box(r as f64*1e-9); matmul_naive(black_box(&a),m,k,&b,n,&mut out); black_box(&out); } let nv=t.elapsed();
    let t=Instant::now(); for r in 0..reps { a[0]=black_box(r as f64*1e-9); matmul(black_box(&a),m,k,&b,n,&mut out); black_box(&out); } let sv=t.elapsed();
    println!("[{arch}] matmul {m}x{k}x{n}: tiled-scalar(=Almide) {nv:?} / SIMD {sv:?} → {:.2}x", nv.as_secs_f64()/sv.as_secs_f64());
}
