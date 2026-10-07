//! The error channel of a fan arm (C-199, ADR-0024 D1, ADR-0021 D1):
//! every arm runs, the lowest-index Err is the block's.
//!
//! - #3468: a block arm whose tail is a Result call, with the fan as a fn
//!   tail or `let`-bound. Before: auto-try put the `?` into the block's tail,
//!   so native joined a Result as a payload (rustc E0308) and wasm left the
//!   frame at the first Err before the later arms ran.
//!
//! The pin, per cell: stdout, stderr and the exit code, equal on both legs.

use std::path::{Path, PathBuf};
use std::process::Command;

fn almide_bin() -> String {
    std::env::var("ALMIDE_BIN").unwrap_or_else(|_| env!("CARGO_BIN_EXE_almide").to_string())
}

fn tool_available(tool: &str) -> bool {
    let found = Command::new(tool).arg("--version").output().is_ok();
    if !found && std::env::var("ALMIDE_EXPECT_TOOLS").is_ok() {
        panic!("{tool} is required (ALMIDE_EXPECT_TOOLS is set) but was not found");
    }
    found
}

fn scratch(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("almide-fan-channel-{}-{name}", std::process::id()));
    std::fs::create_dir_all(&dir).expect("mkdir");
    dir
}

/// (stdout, stderr, exit code) of `almide run` on one target.
fn run(almd: &Path, target: &str) -> (String, String, i32) {
    let out = Command::new(almide_bin())
        .arg("run")
        .arg(almd)
        .args(["--target", target])
        .output()
        .expect("spawn almide run");
    (
        String::from_utf8_lossy(&out.stdout).into_owned(),
        String::from_utf8_lossy(&out.stderr).into_owned(),
        out.status.code().unwrap_or(-1),
    )
}

/// `typed(n)` fails with a `Bad` above 3; `loud(n)` prints, then succeeds
/// with the same error type; `num(n)` / `boom(n)` fail with a `String`.
const HELPERS: &str = "\
type Bad = | Bad(Int)

effect fn typed(n: Int) -> Result[Int, Bad] = if n > 3 then err(Bad(n)) else ok(n)

effect fn loud(n: Int) -> Result[Int, Bad] = {
  println(\"loud ${n}\")
  if n > 3 then err(Bad(n)) else ok(n)
}

effect fn num(n: Int) -> Result[Int, String] = if n > 3 then err(\"big ${n}\") else ok(n)

effect fn boom(n: Int) -> Int = if n > 3 then err(\"boom ${n}\") else ok(n)
";

/// Runs the program on both legs and asserts the expected observation on each.
fn check(name: &str, body: &str, stdout: &str, stderr: &str, code: i32) {
    if !tool_available("rustc") || !tool_available("wasmtime") {
        return;
    }
    let dir = scratch(name);
    let almd = dir.join("main.almd");
    std::fs::write(&almd, format!("{HELPERS}\n{body}")).expect("write source");
    for target in ["rust", "wasm"] {
        let (out, err, got) = run(&almd, target);
        assert_eq!(out, stdout, "{name} [{target}]: stdout (stderr: {err})");
        assert_eq!(err, stderr, "{name} [{target}]: stderr");
        assert_eq!(got, code, "{name} [{target}]: exit code");
    }
    let _ = std::fs::remove_dir_all(&dir);
}

// ── #3468: a block arm whose tail is a Result call ────────────────────

#[test]
fn the_reported_program() {
    check("tail-reported", "\
effect fn f(n: Int) -> (Int, Int) = fan {
  { println(\"a\"); num(n) },
  num(1),
}

effect fn main() -> Unit = {
  let (a, b) = f(2)!
  println(\"${a + b}\")
}
", "a\n3\n", "", 0);
}

#[test]
fn a_tail_fan_block_arm_err_still_runs_the_later_arm() {
    check("tail-err", "\
effect fn f(n: Int) -> (Int, Int) = fan {
  { println(\"a\"); num(n) },
  { println(\"b\"); num(1) },
}

effect fn main() -> Unit = {
  match f(5) {
    ok((a, b)) => println(\"ok ${a + b}\"),
    err(e) => println(\"err ${e}\"),
  }
}
", "a\nb\nerr big 5\n", "", 0);
}

#[test]
fn a_let_bound_fan_of_block_arms() {
    check("let-bound", "\
effect fn g(n: Int) -> (Int, Int) = {
  let p = fan {
    { println(\"a\"); num(n) },
    { println(\"b\"); num(1) },
  }
  p
}

effect fn main() -> Unit = {
  match g(5) {
    ok((a, b)) => println(\"ok ${a + b}\"),
    err(e) => println(\"err ${e}\"),
  }
  match g(2) {
    ok((a, b)) => println(\"ok ${a + b}\"),
    err(e) => println(\"err ${e}\"),
  }
}
", "a\nb\nerr big 5\na\nb\nok 3\n", "", 0);
}

#[test]
fn a_one_arm_tail_fan_of_a_block_arm() {
    check("one-arm", "\
effect fn f(n: Int) -> Int = fan {
  { println(\"a\"); num(n) },
}

effect fn main() -> Unit = {
  match f(5) {
    ok(v) => println(\"ok ${v}\"),
    err(e) => println(\"err ${e}\"),
  }
  println(\"${f(2)!}\")
}
", "a\nerr big 5\na\n2\n", "", 0);
}

#[test]
fn a_tail_fan_block_arm_in_main_aborts_after_every_arm() {
    check("main-abort", "\
effect fn main() -> Unit = {
  let p = fan {
    { println(\"a\"); num(5) },
    { println(\"b\"); num(1) },
  }
  println(\"${p.0 + p.1}\")
}
", "a\nb\n", "Error: big 5\n", 1);
}
