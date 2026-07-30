// spectral-norm — The Computer Language Benchmarks Game, simple scalar Rust.
// Reference implementation for the Almide overhead comparison.

fn eval_a(i: i64, j: i64) -> f64 {
    let ij = i + j;
    1.0 / (ij * (ij + 1) / 2 + i + 1) as f64
}

fn multiply_av(v: &[f64], out: &mut [f64]) {
    let n = v.len() as i64;
    for i in 0..n {
        let mut sum = 0.0;
        for j in 0..n {
            sum += eval_a(i, j) * v[j as usize];
        }
        out[i as usize] = sum;
    }
}

fn multiply_atv(v: &[f64], out: &mut [f64]) {
    let n = v.len() as i64;
    for i in 0..n {
        let mut sum = 0.0;
        for j in 0..n {
            sum += eval_a(j, i) * v[j as usize];
        }
        out[i as usize] = sum;
    }
}

fn multiply_atav(v: &[f64], tmp: &mut [f64], out: &mut [f64]) {
    multiply_av(v, tmp);
    multiply_atv(tmp, out);
}

fn main() {
    let n: usize = std::env::args()
        .nth(1)
        .and_then(|s| s.parse().ok())
        .unwrap_or(100);

    let mut u = vec![1.0; n];
    let mut v = vec![0.0; n];
    let mut tmp = vec![0.0; n];

    for _ in 0..10 {
        multiply_atav(&u, &mut tmp, &mut v);
        multiply_atav(&v, &mut tmp, &mut u);
    }

    let mut vbv = 0.0;
    let mut vv = 0.0;
    for i in 0..n {
        vbv += u[i] * v[i];
        vv += v[i] * v[i];
    }
    println!("{:.9}", (vbv / vv).sqrt());
}
