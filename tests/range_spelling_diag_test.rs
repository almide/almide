//! The range surface has exactly two live spellings — `..<` (end-exclusive)
//! and `...` (end-inclusive). The retired spellings must STEER (E031 with the
//! `almide fix` migration hint), never parse silently into the wrong bound:
//! `..` historically meant end-exclusive and `..=` end-inclusive, so a silent
//! re-reading in either direction would be an off-by-one an LLM cannot see.
//! ALS-E10 (docs/specs/als/expressions.md) cites this as the retirement half
//! of the ruling; the live spellings are pinned by
//! `spec/wasm_cross/range_first_class.almd` (C-238).

use std::process::Command;

fn almide() -> &'static str {
    env!("CARGO_BIN_EXE_almide")
}

fn check(dir: &std::path::Path, source: &str) -> String {
    let file = dir.join("range.almd");
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
fn dotdot_is_retired_with_a_steer() {
    let dir = tempfile::tempdir().expect("tempdir");
    let out = check(
        dir.path(),
        "effect fn main() -> Unit = {\n  for i in 0..5 {\n    println(int.to_string(i))\n  }\n}\n",
    );
    assert!(out.contains("E031"), "`..` must be E031, got:\n{out}");
    assert!(
        out.contains("'..<'") || out.contains("..<"),
        "the `..` rejection must steer to `..<`, got:\n{out}"
    );
}

#[test]
fn dotdot_eq_is_retired_with_a_steer() {
    let dir = tempfile::tempdir().expect("tempdir");
    let out = check(
        dir.path(),
        "effect fn main() -> Unit = {\n  for i in 0..=5 {\n    println(int.to_string(i))\n  }\n}\n",
    );
    assert!(out.contains("E031"), "`..=` must be E031, got:\n{out}");
    assert!(
        out.contains("'...'") || out.contains("..."),
        "the `..=` rejection must steer to `...`, got:\n{out}"
    );
}
