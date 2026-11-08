fn eval_a(i: usize, j: usize) -> f64 { let ij = i + j; 1.0 / ((ij * (ij + 1) / 2 + i + 1) as f64) }
fn mul_av(v: &[f64], n: usize) -> Vec<f64> {
    let mut out = vec![0.0; n];
    for i in 0..n { let mut s = 0.0; for j in 0..n { s += eval_a(i, j) * v[j]; } out[i] = s; }
    out
}
fn mul_atv(v: &[f64], n: usize) -> Vec<f64> {
    let mut out = vec![0.0; n];
    for i in 0..n { let mut s = 0.0; for j in 0..n { s += eval_a(j, i) * v[j]; } out[i] = s; }
    out
}
fn main() {
    let n = 1200usize;
    let mut u = vec![1.0; n];
    let mut v = vec![0.0; n];
    for _ in 0..10 { v = mul_atv(&mul_av(&u, n), n); u = mul_atv(&mul_av(&v, n), n); }
    let (mut vbv, mut vv) = (0.0, 0.0);
    for i in 0..n { vbv += u[i] * v[i]; vv += v[i] * v[i]; }
    println!("{}", (vbv / vv).sqrt());
}
