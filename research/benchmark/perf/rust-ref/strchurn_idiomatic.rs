// strchurn — the IDIOMATIC Rust a person actually writes for the #1004
// workload. Reported, never gated.
//
// It differs from `strchurn.rs` only in the two places where Almide's string
// API forces work that Rust's does not:
//   - `split` yields borrowed `&str` pieces instead of owned `String`s
//     (no N allocations, no N copies of the payload);
//   - `len()` is the O(1) byte length instead of `chars().count()`.
// The workload is pure ASCII (digits and commas), so byte length and character
// count coincide and stdout is byte-identical to the same-shape reference.
//
// The gap between this file and `strchurn.rs` is the STDLIB CONTRACT's cost;
// the gap between `strchurn.rs` and the Almide binary is CODEGEN's. Keeping
// them apart is the whole point of having two references.

fn main() {
    let n: i64 = std::env::args()
        .nth(1)
        .and_then(|s| s.parse().ok())
        .unwrap_or(1_000_000);

    let parts: Vec<String> = (0..n).map(|i| i.to_string()).collect();
    let joined: String = parts.join(",");
    let total: usize = joined.split(',').map(|s| s.len()).sum();

    println!("n: {} chars: {} sum: {}", n, joined.len(), total);
}
