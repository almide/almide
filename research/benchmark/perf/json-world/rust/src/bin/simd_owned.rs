// simd-json: OwnedValue for build, Writable::encode, to_owned_value (in place on the text's bytes).
#[path = "../common.rs"]
mod common;
use simd_json::prelude::*;
use simd_json::{json, OwnedValue};

const N: i64 = 50_000;

fn record(i: i64) -> OwnedValue {
    json!({
        "id": i,
        "name": format!("user_{}", i),
        "tags": [format!("t{}", i % 7), format!("t{}", i % 11)],
        "meta": { "score": (i * 37) % 1000, "active": i % 2 == 0 },
    })
}

fn main() {
    let mut ph = common::Phases::start();
    let doc = OwnedValue::Array(Box::new((0..N).map(record).collect()));
    ph.mark();
    let text = doc.encode();
    ph.mark();
    let len = text.len();
    let mut bytes = text.into_bytes();
    let back = simd_json::to_owned_value(&mut bytes).unwrap();
    ph.mark();
    let sum: i64 = back
        .as_array()
        .unwrap()
        .iter()
        .map(|r| r.get("meta").and_then(|m| m.get("score")).and_then(|s| s.as_i64()).unwrap_or(0))
        .sum();
    ph.mark();
    println!("{} {}", sum, len);
    ph.report();
}
