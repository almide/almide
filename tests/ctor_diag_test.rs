//! The Option/Result constructor surface has two check-time rulings that the
//! ALS-E9 section cites as its negative half (the accepted spellings are
//! pinned by `spec/wasm_cross/option_result_ctors.almd`, C-237):
//!
//! - `none` is a bare VALUE, not a function — `none()` is a type error, and
//!   the message must show the mismatch so the fix (drop the parens) is
//!   readable from the diagnostic alone.
//! - `ok(e)` / `err(e)` demand a Result-typed expectation: an un-annotated
//!   `let` binding is the ADR-0008 explicit-propagation rejection (E041),
//!   whose hint steers to `!` / `??` / `?` / match.

use std::process::Command;

fn almide() -> &'static str {
    env!("CARGO_BIN_EXE_almide")
}

fn check(dir: &std::path::Path, source: &str) -> String {
    let file = dir.join("ctor.almd");
    std::fs::write(&file, source).expect("write fixture");
    let out = Command::new(almide())
        .arg("check")
        .arg(&file)
        .output()
        .expect("run almide check");
    format!(
        "{}{}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    )
}

#[test]
fn calling_none_is_a_type_error() {
    let dir = tempfile::tempdir().expect("tempdir");
    let out = check(
        dir.path(),
        "effect fn main() -> Unit = {\n  let n: Int? = none()\n  println(int.to_string(n ?? 0))\n}\n",
    );
    assert!(
        out.contains("E001"),
        "`none()` must be a check-time type error, got:\n{out}"
    );
}

#[test]
fn unannotated_ok_binding_is_e041() {
    let dir = tempfile::tempdir().expect("tempdir");
    let out = check(
        dir.path(),
        "effect fn main() -> Unit = {\n  let o = ok(3)\n  println(int.to_string(o ?? 0))\n}\n",
    );
    assert!(
        out.contains("E041") || out.contains("E034"),
        "un-annotated `ok(3)` binding must be rejected at check time, got:\n{out}"
    );
}
