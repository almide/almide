// sonic-rs: its own Value for build, to_string, from_str into Value.
#[path = "../common.rs"]
mod common;
use sonic_rs::{json, JsonContainerTrait, JsonValueTrait, Value};

const N: i64 = 50_000;

fn record(i: i64) -> Value {
    json!({
        "id": i,
        "name": format!("user_{}", i),
        "tags": [format!("t{}", i % 7), format!("t{}", i % 11)],
        "meta": { "score": (i * 37) % 1000, "active": i % 2 == 0 },
    })
}

fn main() {
    let mut ph = common::Phases::start();
    let doc: Value = (0..N).map(record).collect::<Vec<Value>>().into();
    ph.mark();
    let text = sonic_rs::to_string(&doc).unwrap();
    ph.mark();
    let back: Value = sonic_rs::from_str(&text).unwrap();
    ph.mark();
    let sum: i64 = back.as_array().unwrap().iter().map(|r| r["meta"]["score"].as_i64().unwrap_or(0)).sum();
    ph.mark();
    println!("{} {}", sum, text.len());
    ph.report();
}
