//! `(x) => f(x)!` over a heap payload as a callback in a test block, on the
//! certified native render.
//!
//! In a test block a lambda's `!` is a plain unwrap, so the lambda's value is
//! the payload. Its tail was lowered as a function's tail `!` is — the callee's
//! Result handed back as it is — and the test binary did not compile: the
//! callback returned a Result where the list builder reads the payload. The
//! native render now refuses such a test file instead of emitting it.
use std::process::Command;

const HEAP_PAYLOAD_CALLBACK: &str = r##"fn mk(n: Int) -> Result[List[Int], String] =
  if n > 0 then ok([n, n + 1]) else err("E")

test "a callback that unwraps a list" {
  let ys = [2, 1] |> list.map((x) => mk(x)!)
  assert_eq(list.len(ys), 2)
}
"##;

#[test]
fn a_test_block_callback_unwrapping_a_heap_payload_never_reaches_the_rust_compiler_ill_typed() {
    let dir = tempfile::tempdir().expect("scratch");
    std::fs::write(dir.path().join("callback_test.almd"), HEAP_PAYLOAD_CALLBACK).expect("source");
    let run = Command::new(env!("CARGO_BIN_EXE_almide")).args(["test", "--target", "rust", "callback_test.almd"])
        .current_dir(dir.path()).output().expect("almide test");
    let stderr = String::from_utf8_lossy(&run.stderr);
    assert!(!stderr.contains("error[E"), "the emitted test binary did not compile: {stderr}");
    // Either the file runs and passes, or the native render refused it by name.
    assert!(run.status.success() || stderr.contains("native v1 test render"), "{stderr}");
    // The wasm leg runs it.
    let wasm = Command::new(env!("CARGO_BIN_EXE_almide")).args(["test", "--target", "wasm", "callback_test.almd"])
        .current_dir(dir.path()).output().expect("almide test");
    assert!(wasm.status.success(), "{}", String::from_utf8_lossy(&wasm.stderr));
}
