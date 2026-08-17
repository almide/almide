//! The unary-negation surface has exactly one accepted spelling: the keyword
//! `not`. Prefix `!` is reserved (postfix `!` is unwrap, ADR-0008), and the
//! rejection must STEER — the hint names `not` so the fix is mechanical.
//! ALS-E7 (docs/specs/als/expressions.md) cites this test as the negative
//! half of the ruling; the accepted spellings are pinned by
//! `spec/wasm_cross/grouping_unary.almd` (C-235).
//!
//! Two positions measured distinct on 0.57.0: a bare expression statement and
//! an interpolation segment take different lexer paths but must agree on the
//! steer. Merging the assertions would pass if either path lost its hint.

use std::process::Command;

fn almide() -> &'static str {
    env!("CARGO_BIN_EXE_almide")
}

fn check(dir: &std::path::Path, source: &str) -> String {
    let file = dir.join("neg.almd");
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
fn prefix_bang_in_expression_steers_to_not() {
    let dir = tempfile::tempdir().expect("tempdir");
    let out = check(
        dir.path(),
        "effect fn main() -> Unit = {\n  let b = true\n  let nb = !b\n  println(\"x\")\n}\n",
    );
    assert!(
        out.contains("'!' is not valid"),
        "prefix ! must be rejected, got:\n{out}"
    );
    assert!(
        out.contains("Use 'not' for boolean negation"),
        "the rejection must steer to `not`, got:\n{out}"
    );
}

#[test]
fn prefix_bang_in_interpolation_steers_to_not() {
    let dir = tempfile::tempdir().expect("tempdir");
    let out = check(
        dir.path(),
        "effect fn main() -> Unit = {\n  let b = true\n  println(\"${!b}\")\n}\n",
    );
    assert!(
        out.contains("'!' is not valid"),
        "prefix ! inside ${{}} must be rejected, got:\n{out}"
    );
    assert!(
        out.contains("Use 'not' for boolean negation"),
        "the interpolation rejection must steer to `not`, got:\n{out}"
    );
}
