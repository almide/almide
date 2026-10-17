//! `random.int(min, max)` over a span wider than `Int` holds.
//!
//! `max - min + 1` overflows once the span passes 2^63 - 1. Native reduced
//! the draw modulo that wrapped span and divided by zero on the full range
//! (exit 101, a Rust panic); the wasm self-host read the wrapped span as
//! "max <= min" and returned `min` on every call. Found by the ADR-0027
//! verdict campaign (`xtarget-fuzz run --focus random.int`).
use std::process::Command;

fn run(src: &str, wasm: bool) -> (String, i32) {
    let dir = tempfile::tempdir().expect("scratch");
    let file = dir.path().join("random_int.almd");
    std::fs::write(&file, src).expect("source");
    let mut cmd = Command::new(env!("CARGO_BIN_EXE_almide"));
    cmd.arg("run").arg(&file);
    if wasm {
        cmd.args(["--target", "wasm"]);
    }
    let out = cmd.output().expect("run");
    (String::from_utf8_lossy(&out.stdout).to_string(), out.status.code().unwrap_or(-1))
}

#[test]
fn a_span_wider_than_int_draws_in_range_and_not_always_min() {
    // 64 draws per span: each in [min, max], and not all equal to min (a
    // uniform draw over 2^63 values repeats min 64 times with odds 2^-4032).
    let src = r#"import random

effect fn spread(min: Int, max: Int) -> String = {
  let draws = list.range(0, 64) |> list.map((_) => random.int(min, max))
  let inside = list.all(draws, (d) => d >= min and d <= max)
  let varied = list.any(draws, (d) => d != min)
  "${inside} ${varied}"
}

effect fn main() -> Unit = {
  println(spread(-9223372036854775808, 9223372036854775807)!)
  println(spread(-9223372036854775808, 0)!)
  println(spread(0, 9223372036854775807)!)
  println(spread(-1, 9223372036854775807)!)
}
"#;
    let expected = "true true\n".repeat(4);
    for wasm in [false, true] {
        assert_eq!(run(src, wasm), (expected.clone(), 0), "wasm={wasm}");
    }
}
