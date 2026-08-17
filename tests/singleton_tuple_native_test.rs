//! A 1-tuple must stay a real tuple in emitted Rust — `(x)` is a
//! parenthesized scalar, not a tuple (#1267).
//!
//! The v0 walker joined tuple elements with `", "` and wrapped them in
//! parens, so a 1-tuple rendered as `let t: (i64) = (5i64);` — Rust reads
//! both as bare `i64` and the subsequent `t.0` is rustc E0610. The bug only
//! surfaced when something routed the build to the v0 codegen: `almide run`
//! tries the v1 MIR native render first, and a string interpolation of a
//! Bool (`"${true}"` → `bool.to_string`, outside the native runtime floor)
//! walls it, falling back to v0. Hence the confusing shape in the issue:
//! interpolation anywhere in the fn "scalarized" an unrelated 1-tuple.
//!
//! Pinned as a cargo gate rather than a spec test because the assertion is
//! about the NATIVE v0 leg specifically (the wasm leg always ran this
//! correctly), plus a textual pin on the v0 Rust emission so the coverage
//! survives even if the v1 wall that routes to v0 disappears.

use std::process::Command;

fn almide() -> &'static str {
    env!("CARGO_BIN_EXE_almide")
}

/// The issue #1267 repro: interpolation forces the v0 fallback, the 1-tuple
/// plus `.0` must still compile and run.
const INTERP_PLUS_SINGLETON_TUPLE: &str = r#"
effect fn main() -> Unit = {
  println("${true}")
  let t = (5,)
  println(int.to_string(t.0))
}
"#;

/// The no-interpolation control from the issue: same 1-tuple shape, no
/// forced v0 routing. Must keep working.
const SINGLETON_TUPLE_CONTROL: &str = r#"
effect fn main() -> Unit = {
  let t = (5,)
  println(int.to_string(t.0))
}
"#;

fn run_native(name: &str, src_text: &str) -> (bool, String, String) {
    let dir = tempfile::tempdir().expect("tempdir");
    let src = dir.path().join(name);
    std::fs::write(&src, src_text).expect("write fixture");
    let out = Command::new(almide())
        .args(["run", src.to_str().unwrap()])
        .output()
        .expect("run almide");
    (
        out.status.success(),
        String::from_utf8_lossy(&out.stdout).into_owned(),
        String::from_utf8_lossy(&out.stderr).into_owned(),
    )
}

#[test]
fn interpolation_plus_singleton_tuple_runs_native() {
    let (ok, stdout, stderr) = run_native("interp_1tuple.almd", INTERP_PLUS_SINGLETON_TUPLE);
    assert!(ok, "native run failed (1-tuple scalarized to bare i64 again? E0610):\n{stderr}");
    assert_eq!(stdout, "true\n5\n", "wrong output (wasm leg prints exactly this)");
}

#[test]
fn singleton_tuple_control_still_runs_native() {
    let (ok, stdout, stderr) = run_native("control_1tuple.almd", SINGLETON_TUPLE_CONTROL);
    assert!(ok, "native control run failed:\n{stderr}");
    assert_eq!(stdout, "5\n");
}

/// Pin the v0 emission itself: the `--target rust` emit path is always v0
/// (no v1 routing), so this asserts the 1-tuple repr directly and keeps
/// covering the walker even if the v1 native wall that routes the repro to
/// v0 is later lifted.
#[test]
fn v0_emission_keeps_singleton_tuple_repr() {
    let dir = tempfile::tempdir().expect("tempdir");
    let src = dir.path().join("emit_1tuple.almd");
    std::fs::write(&src, INTERP_PLUS_SINGLETON_TUPLE).expect("write fixture");
    let out = Command::new(almide())
        .args([src.to_str().unwrap(), "--target", "rust"])
        .output()
        .expect("emit rust");
    assert!(out.status.success(), "emit failed:\n{}", String::from_utf8_lossy(&out.stderr));
    let code = String::from_utf8_lossy(&out.stdout);
    assert!(
        code.contains("(5i64,)"),
        "1-tuple literal lost its trailing comma (scalarizes to i64 in Rust)"
    );
    assert!(
        code.contains("(i64,)"),
        "1-tuple type annotation lost its trailing comma (scalarizes to i64 in Rust)"
    );
    assert!(
        !code.contains("let t: (i64) ="),
        "the exact #1267 broken shape is back: `let t: (i64) = ...`"
    );
}
