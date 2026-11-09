use serde_json::{json, Value};
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
    let doc = Value::Array((0..N).map(record).collect());
    let text = serde_json::to_string(&doc).unwrap();
    let back: Value = serde_json::from_str(&text).unwrap();
    let sum: i64 = back.as_array().unwrap().iter()
        .map(|r| r["meta"]["score"].as_i64().unwrap_or(0)).sum();
    println!("{} {}", sum, text.len());
}
