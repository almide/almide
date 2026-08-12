//! The binding statements' negative half, cited by ALS-S1 (the accepted
//! spellings are pinned by `spec/wasm_cross/binding_stmts.almd`, C-241):
//!
//! - assigning an immutable `let` binding is E009, and the hint must STEER
//!   to `var` so the fix is mechanical;
//! - reassigning a `var` at a different type is E001 — the type is fixed at
//!   declaration, never silently widened.

use std::process::Command;

fn almide() -> &'static str {
    env!("CARGO_BIN_EXE_almide")
}

fn check(dir: &std::path::Path, source: &str) -> String {
    let file = dir.join("bind.almd");
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
fn assigning_a_let_is_e009_steering_to_var() {
    let dir = tempfile::tempdir().expect("tempdir");
    let out = check(
        dir.path(),
        "effect fn main() -> Unit = {\n  let x = 1\n  x = 2\n  println(int.to_string(x))\n}\n",
    );
    assert!(out.contains("E009"), "assign-to-let must be E009, got:\n{out}");
    assert!(
        out.contains("var x"),
        "the E009 hint must steer to `var`, got:\n{out}"
    );
}

#[test]
fn retyping_a_var_is_e001() {
    let dir = tempfile::tempdir().expect("tempdir");
    let out = check(
        dir.path(),
        "effect fn main() -> Unit = {\n  var x = 1\n  x = \"s\"\n  println(int.to_string(x))\n}\n",
    );
    assert!(
        out.contains("E001") && out.contains("expected Int but got String"),
        "var retype must be a typed E001, got:\n{out}"
    );
}
