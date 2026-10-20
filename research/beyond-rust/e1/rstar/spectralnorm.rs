// E1 spectralnorm, R*: expert Rust, same algorithm and the same float
// operations per output element in the same order (no reassociation, no FMA),
// so the output is byte-identical to R0/A0.
//
// What the expert does that the idiomatic version does not:
// - four output rows per pass ([f64; 4] lanes) so the divide and the
//   multiply-add run in SIMD lanes; each lane still sums j = 0..n in order;
// - A(i,j)'s denominator ij*(ij+1)/2 + i + 1 computed in f64, which is exact
//   (every intermediate < 2^53) for n <= 2^20; larger n falls back to i64;
// - no bounds checks (get_unchecked) and buffers reused across iterations.
// Usage: spectralnorm <n>

const EXACT_F64_MAX_N: usize = 1 << 20;

#[inline(always)]
fn den_i64(i: usize, j: usize) -> f64 {
    let (i, j) = (i as i64, j as i64);
    let ij = i + j;
    (ij * (ij + 1) / 2 + i + 1) as f64
}

#[inline(always)]
fn den_f64(i: f64, j: f64) -> f64 {
    let ij = i + j;
    ij * (ij + 1.0) * 0.5 + i + 1.0
}

/// out[i] = sum_j A(i,j) * v[j]   (transpose = false)
/// out[i] = sum_j A(j,i) * v[j]   (transpose = true)
#[inline(always)]
fn mul(v: &[f64], out: &mut [f64], transpose: bool) {
    let n = v.len();
    debug_assert_eq!(out.len(), n);
    let exact = n <= EXACT_F64_MAX_N;
    let mut i = 0;
    while i + 4 <= n {
        let mut sum = [0.0f64; 4];
        if exact {
            let fi = [i as f64, (i + 1) as f64, (i + 2) as f64, (i + 3) as f64];
            for j in 0..n {
                let vj = unsafe { *v.get_unchecked(j) };
                let fj = j as f64;
                for l in 0..4 {
                    let d = if transpose { den_f64(fj, fi[l]) } else { den_f64(fi[l], fj) };
                    sum[l] = sum[l] + (1.0 / d) * vj;
                }
            }
        } else {
            for j in 0..n {
                let vj = unsafe { *v.get_unchecked(j) };
                for l in 0..4 {
                    let d = if transpose { den_i64(j, i + l) } else { den_i64(i + l, j) };
                    sum[l] = sum[l] + (1.0 / d) * vj;
                }
            }
        }
        unsafe { out.get_unchecked_mut(i..i + 4).copy_from_slice(&sum) };
        i += 4;
    }
    while i < n {
        let mut s = 0.0;
        for j in 0..n {
            let d = if transpose { den_i64(j, i) } else { den_i64(i, j) };
            s = s + (1.0 / d) * unsafe { *v.get_unchecked(j) };
        }
        out[i] = s;
        i += 1;
    }
}

fn main() {
    let n: usize = std::env::args().nth(1).and_then(|s| s.parse().ok()).unwrap_or(100);
    let mut u = vec![1.0f64; n];
    let mut v = vec![0.0f64; n];
    let mut tmp = vec![0.0f64; n];
    for _ in 0..10 {
        mul(&u, &mut tmp, false);
        mul(&tmp, &mut v, true);
        mul(&v, &mut tmp, false);
        mul(&tmp, &mut u, true);
    }
    let mut vbv = 0.0;
    let mut vv = 0.0;
    for i in 0..n {
        vbv = vbv + u[i] * v[i];
        vv = vv + v[i] * v[i];
    }
    println!("{:.9}", (vbv / vv).sqrt());
}
