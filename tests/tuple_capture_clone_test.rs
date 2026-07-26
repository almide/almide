//! A TUPLE with a heap element captured by a `move` closure and used AFTER it
//! must compile on the native target.
//!
//! `capture_clone_wrap` (the capture projection of the copy-ness classifier,
//! `almide_ir::top_let_storage`) excluded tuples — "the capture path moves
//! tuples whole", with widening deferred as "a reviewed future delta". The
//! fuzzer delivered the review: `list.fold(…, (acc, b) => r4)` followed by
//! `println("${r4}")` over a `(Bool, String)` passed `almide check` and then
//! failed rustc with E0382 (borrow of moved value) — a check-vs-build
//! acceptance gap on the native leg (differential fuzz, seed
//! 1785015406589852000 index 746). The cell now agrees with `clone_free`'s:
//! a tuple with any non-clone-free element clone-wraps; an all-Copy tuple
//! (`(Int, Int)` is Copy in Rust) still skips the wrap.
//!
//! Pinned as a cargo gate rather than a spec test because the fix lives in the
//! RUST-target pass pipeline (`CaptureClonePass` runs for `Target::Rust`
//! only); the capturing-fold shape walls on the wasm leg, and parking a
//! walling file in spec/ would trade away a wasm-covered spec file for a
//! native-only assertion this harness makes directly.

use std::process::Command;

fn almide() -> &'static str {
    env!("CARGO_BIN_EXE_almide")
}

const HEAP_TUPLE_CAPTURE: &str = r#"
fn fold_to_tuple(xs: List[Bool], seed: (Bool, String)) -> ((Bool, String), (Bool, String)) = {
  let folded = list.fold(xs, (false, "init"), (acc, b) => seed)
  (folded, seed)
}

fn main() -> Unit = {
  // The capture (`seed`) is used AFTER the closure — both by the second tuple
  // component and by the interpolation below. Before the fix the closure's
  // `move` consumed it and this program failed rustc while `check` passed.
  let (folded, kept) = fold_to_tuple([true, false], (true, "αβγ"))
  println("folded=${folded} kept=${kept}")

  // The all-Copy cell must stay unwrapped and keep compiling: (Int, Int) is
  // Copy in Rust, the move costs nothing, and no clone is needed.
  let pair = (1, 2)
  let doubled = list.map([10, 20], (x) => pair)
  println("n=${int.to_string(list.len(doubled))} pair=${pair}")
}
"#;

#[test]
fn heap_tuple_capture_survives_a_later_use() {
    let dir = tempfile::tempdir().expect("tempdir");
    let src = dir.path().join("tuple_capture.almd");
    std::fs::write(&src, HEAP_TUPLE_CAPTURE).expect("write fixture");
    let out = Command::new(almide())
        .args(["run", src.to_str().unwrap()])
        .output()
        .expect("run almide");
    assert!(
        out.status.success(),
        "native run failed (the E0382 acceptance gap is back?):\n{}",
        String::from_utf8_lossy(&out.stderr)
    );
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(
        stdout.contains(r#"folded=(true, "αβγ") kept=(true, "αβγ")"#),
        "wrong folded/kept output:\n{stdout}"
    );
    assert!(
        stdout.contains("n=2 pair=(1, 2)"),
        "wrong all-copy-tuple output:\n{stdout}"
    );
}
