//! The corpus row for serde_json: the same two lines as corpus.almd.
use std::time::Instant;
const K: usize = 20;
fn main() {
    let text = std::fs::read_to_string("corpus.json").expect("corpus.json");
    let doc: serde_json::Value = serde_json::from_str(&text).expect("parse");
    let t0 = Instant::now();
    let mut parsed = 0;
    for _ in 0..K {
        let d: Result<serde_json::Value, _> = serde_json::from_str(&text);
        parsed += usize::from(d.is_ok());
    }
    let t1 = Instant::now();
    let mut written = 0;
    for _ in 0..K {
        written += serde_json::to_string(&doc).expect("write").len();
    }
    let t2 = Instant::now();
    println!("{} {}", parsed, written / K);
    println!("CORPUS parse={} stringify={}", (t1 - t0).as_nanos() / K as u128, (t2 - t1).as_nanos() / K as u128);
}
