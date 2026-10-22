// E1 small kernel, R*: expert Rust for idxloop.almd. Same float operations in
// the same order (no reassociation, no FMA); bounds checks removed with
// get_unchecked and the index kept in usize.
// Usage: idxloop <n> <reps>

#[inline(always)]
fn smooth_dot(xs: &[f64], r: i64) -> f64 {
    let n = xs.len();
    let mut acc = r as f64;
    if n < 3 {
        return acc;
    }
    let p = xs.as_ptr();
    for i in 1..n - 1 {
        // SAFETY: 1 <= i <= n-2, so i-1, i, i+1 and i/2 are all < n.
        unsafe {
            let s = *p.add(i - 1) * 0.25 + *p.add(i) * 0.5 + *p.add(i + 1) * 0.25;
            acc += s * *p.add(i >> 1);
        }
    }
    acc
}

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let arg = |k: usize, d: i64| args.get(k).and_then(|s| s.parse().ok()).unwrap_or(d);
    let n = arg(1, 1000);
    let reps = arg(2, 10);
    let xs: Vec<f64> = (0..n).map(|i| ((i * 7919) % 1000) as f64 / 1000.0).collect();
    let mut total = 0.0;
    for r in 0..reps {
        total += smooth_dot(&xs, r);
    }
    println!("{:.6}", total);
}
