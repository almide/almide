// E1 small kernel, R0: idiomatic safe Rust for idxloop.almd.
// Usage: idxloop <n> <reps>

fn smooth_dot(xs: &[f64], r: i64) -> f64 {
    let n = xs.len();
    let mut acc = r as f64;
    for i in 1..n.saturating_sub(1) {
        let s = xs[i - 1] * 0.25 + xs[i] * 0.5 + xs[i + 1] * 0.25;
        acc += s * xs[i / 2];
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
