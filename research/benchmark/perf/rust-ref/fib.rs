// fib — the ordinary Rust for research/benchmark/perf/fib/fib.almd: the same
// recursion, the same `n:reps` argument and the same sum. LLVM does not
// inline a recursive call, so this runs one call per node.

fn fib(n: i64) -> i64 {
    if n < 2 { n } else { fib(n - 1) + fib(n - 2) }
}

fn main() {
    let arg = std::env::args().nth(1).unwrap_or_else(|| "20:10".to_string());
    let mut parts = arg.split(':').map(|s| s.parse::<i64>().ok());
    let n = parts.next().flatten().unwrap_or(20);
    let reps = parts.next().flatten().unwrap_or(10);
    let total = (0..reps).fold(0i64, |acc, i| acc + fib(n + i % 3));
    println!("fib n={n} reps={reps} sum={total}");
}
