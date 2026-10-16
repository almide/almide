//! A `testing` assert outside a test block reads a variable operand in place.
//!
//! The desugar (C-381) bound every operand to an `__assert_<i>` temp so the
//! failure block never re-runs it. For an operand that is already a variable
//! the temp was a second owner, and the native leg cloned the value into it
//! on every call: `assert_some(x)` over a `some(<string of n chars>)` copied
//! the n chars per call. The stdlib inspection measured it at O(n) per call
//! on native against O(1) on wasm.
use std::process::Command;

fn native_allocs(src: &str) -> (String, u64) {
    let dir = tempfile::tempdir().expect("scratch");
    let file = dir.path().join("assert_operand.almd");
    std::fs::write(&file, src).expect("source");
    let out = Command::new(env!("CARGO_BIN_EXE_almide"))
        .arg("run")
        .arg(&file)
        .env("ALMIDE_ALLOC_COUNT", "1")
        .output()
        .expect("run");
    let stderr = String::from_utf8_lossy(&out.stderr).to_string();
    assert!(out.status.success(), "{stderr}");
    let line = stderr
        .lines()
        .find(|l| l.starts_with("__ALMD_ALLOC"))
        .unwrap_or_else(|| panic!("no `__ALMD_ALLOC` line — the allocation lane did not arm:\n{stderr}"));
    let allocs = line
        .split_whitespace()
        .find_map(|f| f.strip_prefix("allocs=")?.parse().ok())
        .unwrap_or_else(|| panic!("unparsable allocation report: {line}"));
    (String::from_utf8_lossy(&out.stdout).to_string(), allocs)
}

#[test]
fn an_assert_over_a_variable_does_not_copy_it_per_call() {
    for (ty, init, call) in [
        ("Option[String]", "some(string.repeat(\"a\", 1000))", "testing.assert_some(x)"),
        ("Result[String, String]", "ok(string.repeat(\"a\", 1000))", "testing.assert_ok(x)"),
        ("Result[String, String]", "err(string.repeat(\"a\", 1000))", "testing.assert_err(x)"),
        ("String", "string.repeat(\"a\", 1000)", "testing.assert_contains(x, \"a\")"),
    ] {
        let src = format!(
            "import testing\n\neffect fn main() -> Unit = {{\n  var x: {ty} = {init}\n  var i = 0\n  while i < 2000 {{\n    let _r: Unit = {call}\n    i = i + 1\n  }}\n  println(\"ok\")\n}}\n"
        );
        let (out, allocs) = native_allocs(&src);
        assert_eq!(out, "ok\n", "{call}");
        // A copy per call is 2000 allocations; the program itself needs a handful.
        assert!(allocs < 200, "{call}: {allocs} native allocations over 2000 calls — the operand is copied per call");
    }
}
