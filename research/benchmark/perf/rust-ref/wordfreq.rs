// wordfreq — SAME-SHAPE, SAME-SEMANTICS reference for wordfreq.almd (#2157).
//
// The fair reference for the keyed-aggregation row: an owned `String` key
// cloned out of the vocabulary per draw (Almide's `let w = vocab[draw(i)]`
// owns its String — there is no borrowed element type), one map operation
// per draw through the `entry` API, and the same top-10 sort (count desc,
// word asc). `HashMap` is std's: hashbrown + SipHash-1-3, the map a Rust
// programmer reaches for. Anything cheaper — `&str` keys borrowing the
// vocabulary, a perfect hash over the 5 000 words — would be measuring a
// different program.

use std::collections::HashMap;

const VOCAB: i64 = 5000;
const M: i64 = 2147483648;

fn next(s: i64) -> i64 {
    (s.wrapping_mul(1103515245).wrapping_add(12345)) % M
}

fn word_of(i: i64) -> String {
    let mut s = next(i * 7919 + 17);
    let k = 3 + (s / 65536) % 6;
    let mut w = String::new();
    for _ in 0..k {
        s = next(s);
        w.push((97 + (s / 65536) % 26) as u8 as char);
    }
    w
}

fn draw(i: i64) -> usize {
    ((next(next((i.wrapping_mul(2654435761).wrapping_add(1)) % M)) / 65536) % VOCAB) as usize
}

fn main() {
    let n: i64 = std::env::args()
        .nth(1)
        .and_then(|s| s.parse().ok())
        .unwrap_or(2_000_000);

    let vocab: Vec<String> = (0..VOCAB).map(word_of).collect();
    let mut counts: HashMap<String, i64> = HashMap::new();
    for i in 0..n {
        let w = vocab[draw(i)].clone();
        *counts.entry(w).or_insert(0) += 1;
    }
    let mut top: Vec<(String, i64)> = counts.into_iter().collect();
    top.sort_by(|a, b| b.1.cmp(&a.1).then(a.0.cmp(&b.0)));
    for (w, c) in top.iter().take(10) {
        println!("{} {}", w, c);
    }
}
