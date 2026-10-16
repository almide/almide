//! An abort-desugared assert in a value position runs on both legs.
//!
//! `assert` / `assert_eq` (C-153) and the `testing` asserts (C-339, C-381)
//! desugar outside test blocks to a block that either aborts or falls through.
//! That block had no tail, so bound to a `let` (`let r: Unit = assert(...)`,
//! the shape the stdlib inspection's probes use) the wasm leg refused it with
//! E082 `expr:Block-no-tail`, while native ran it.
use std::process::Command;

fn run(src: &str, wasm: bool) -> (String, i32) {
    let dir = tempfile::tempdir().expect("scratch");
    let file = dir.path().join("assert_let.almd");
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
fn a_passing_assert_bound_to_a_let_runs_on_both_legs() {
    for call in [
        "assert(1 < 2)",
        "assert_eq(1, 1)",
        "testing.assert_gt(2, 1)",
        "testing.assert_some(some(1))",
        "testing.assert_snapshot(\"a\", \"a\")",
    ] {
        let src = format!("import testing\n\neffect fn main() -> Unit = {{\n  let r: Unit = {call}\n  println(\"ok\")\n}}\n");
        let native = run(&src, false);
        let wasm = run(&src, true);
        assert_eq!(native, ("ok\n".to_string(), 0), "{call}: native");
        assert_eq!(wasm, native, "{call}: wasm differs from native");
    }
}
