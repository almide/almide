// strchurn — SAME-SHAPE, SAME-SEMANTICS reference for strchurn.almd (#1004).
//
// This is the fair codegen reference: it does exactly the work the Almide
// stdlib contracts oblige the program to do.
//   - `split` collects OWNED `String`s, because `almide_rt_string_split`
//     returns `Vec<String>` (Almide has no borrowed-slice type).
//   - length is `chars().count()`, because `string.len` is defined as a
//     CHARACTER count (`almide_rt_string_len` = `s.chars().count()`).
// Anything cheaper here would be measuring a different program.
//
// `strchurn_idiomatic.rs` drops both obligations (borrowed `&str`, byte
// `len()`); the delta between the two references is the API's semantic cost
// and is reported separately from codegen. See ../string-gap-1004.md.

fn main() {
    let n: i64 = std::env::args()
        .nth(1)
        .and_then(|s| s.parse().ok())
        .unwrap_or(1_000_000);

    let parts: Vec<String> = (0..n).map(|i| i.to_string()).collect();
    let joined: String = parts.join(",");
    let back: Vec<String> = joined.split(',').map(|s| s.to_string()).collect();
    let total: i64 = back.iter().map(|s| s.chars().count() as i64).sum();

    println!("n: {} chars: {} sum: {}", n, joined.chars().count(), total);
}
