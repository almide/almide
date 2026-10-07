//! A `fan { }` block's Err inside an effect fn other than `main` is
//! RETURNED to the caller, not an abort of the program (#3463).
//!
//! The reading (C-199, ADR-0024 D1): every arm runs, in arm order, and the
//! lowest-index Err is the block's result. A result in a non-`main` effect
//! fn is propagated exactly as a `!` there is — the caller sees `err(msg)`
//! and decides; only `main` turns it into `Error: <msg>` + exit 1.
//!
//! Before: the wasm leg lowered every fan block's Err as the abort frame, so
//! `match f() { err(e) => … }` never ran its err arm and the program died
//! with `Error: boom 5` / exit 1, where native printed `err boom 5` / exit 0.
//!
//! The pin, per cell of the matrix: stdout, stderr and the exit code, equal
//! to the expected values on both legs (native Rust and wasm).

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
    let dir = std::env::temp_dir().join(format!("almide-3463-{}-{name}", std::process::id()));
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

/// `boom(n)` prints its arm and fails above 3; `word(n)` is a fresh heap
/// payload, so an ok arm next to the failing one owns a block the
/// propagation has to release.
const HELPERS: &str = "\
effect fn boom(n: Int) -> Int = {
  println(\"arm ${n}\")
  if n > 3 then err(\"boom ${n}\") else ok(n)
}

effect fn word(n: Int) -> String = {
  println(\"word ${n}\")
  if n > 3 then err(\"bad word ${n}\") else ok(\"w\" + int.to_string(n))
}
";

/// Runs `body` on both legs and asserts the expected observation on each.
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

/// `f(p, q)` fans two `boom` arms; `main` reports what `f` returned.
fn pair_program(p: i64, q: i64) -> String {
    format!(
        "\
effect fn f(p: Int, q: Int) -> Int = {{
  let (a, b) = fan {{ boom(p), boom(q) }}
  a + b
}}

effect fn main() -> Unit = {{
  match f({p}, {q}) {{
    ok(v) => println(\"ok ${{v}}\"),
    err(e) => println(\"err ${{e}}\"),
  }}
  println(\"after\")
}}
"
    )
}

#[test]
fn arm_zero_err_is_returned_to_the_caller() {
    check("a0", &pair_program(5, 1), "arm 5\narm 1\nerr boom 5\nafter\n", "", 0);
}

#[test]
fn arm_one_err_is_returned_to_the_caller() {
    check("a1", &pair_program(1, 6), "arm 1\narm 6\nerr boom 6\nafter\n", "", 0);
}

#[test]
fn both_err_and_the_lowest_index_is_returned() {
    check("both", &pair_program(7, 5), "arm 7\narm 5\nerr boom 7\nafter\n", "", 0);
}

#[test]
fn every_arm_ok_returns_the_values() {
    check("ok", &pair_program(1, 2), "arm 1\narm 2\nok 3\nafter\n", "", 0);
}

#[test]
fn a_one_arm_fan_returns_its_err() {
    check("single", "\
effect fn f() -> Int = {
  let a = fan { boom(9) }
  a + 1
}

effect fn main() -> Unit = {
  match f() {
    ok(v) => println(\"ok ${v}\"),
    err(e) => println(\"err ${e}\"),
  }
}
", "arm 9\nerr boom 9\n", "", 0);
}

#[test]
fn a_heap_arm_beside_the_failing_one() {
    check("heap", "\
effect fn f(p: Int, q: Int) -> String = {
  let (s, n, t) = fan { word(p), boom(q), word(1) }
  s + int.to_string(n) + t
}

effect fn main() -> Unit = {
  for (p, q) in [(1, 2), (1, 8), (4, 2), (5, 9)] {
    match f(p, q) {
      ok(v) => println(\"ok ${v}\"),
      err(e) => println(\"err ${e}\"),
    }
  }
}
", "word 1\narm 2\nword 1\nok w12w1\n\
word 1\narm 8\nword 1\nerr boom 8\n\
word 4\narm 2\nword 1\nerr bad word 4\n\
word 5\narm 9\nword 1\nerr bad word 5\n", "", 0);
}

#[test]
fn propagated_through_a_second_frame_with_bang() {
    check("depth2", "\
effect fn f(p: Int, q: Int) -> Int = {
  let (a, b) = fan { boom(p), boom(q) }
  a + b
}

effect fn g(p: Int, q: Int) -> Int = {
  let v = f(p, q)!
  println(\"g got ${v}\")
  v * 10
}

effect fn main() -> Unit = {
  for (p, q) in [(1, 2), (1, 4), (6, 2)] {
    match g(p, q) {
      ok(v) => println(\"ok ${v}\"),
      err(e) => println(\"err ${e}\"),
    }
  }
}
", "arm 1\narm 2\ng got 3\nok 30\narm 1\narm 4\nerr boom 4\narm 6\narm 2\nerr boom 6\n", "", 0);
}

#[test]
fn main_still_aborts_on_the_lowest_index_err() {
    check("main", "\
effect fn main() -> Unit = {
  let (a, b) = fan { boom(1), boom(5) }
  println(int.to_string(a + b))
}
", "arm 1\narm 5\n", "Error: boom 5\n", 1);
}

#[test]
fn a_propagated_err_reaching_main_through_bang_aborts() {
    check("main-bang", "\
effect fn f(p: Int, q: Int) -> Int = {
  let (a, b) = fan { boom(p), boom(q) }
  a + b
}

effect fn main() -> Unit = {
  println(\"first ${f(1, 2)!}\")
  println(\"second ${f(1, 7)!}\")
}
", "arm 1\narm 2\nfirst 3\narm 1\narm 7\n", "Error: boom 7\n", 1);
}

/// The issue's own program (#3463): pure arms, no output of their own.
#[test]
fn the_reported_program() {
    check("issue", "\
effect fn quiet(n: Int) -> Int = if n > 3 then err(\"boom ${n}\") else ok(n)

effect fn f() -> Int = {
  let (a, b) = fan { quiet(5), quiet(1) }
  a + b
}

effect fn main() -> Unit = {
  match f() {
    ok(v) => println(\"ok ${v}\"),
    err(e) => println(\"err ${e}\"),
  }
}
", "err boom 5\n", "", 0);
}

/// Every exit of the propagating block releases what it must, once: an
/// owned pure arm value, the ok carriers and err carriers it does not
/// return, and a borrowed (let-bound) carrier it does return shares. The
/// wasm leg's allocation counters see no block alive at exit and as many
/// frees as allocations, over every arm combination.
#[test]
fn no_block_outlives_the_run() {
    if !tool_available("rustc") || !tool_available("wasmtime") {
        return;
    }
    let src = "\
effect fn quiet(n: Int) -> Int = if n > 3 then err(\"boom ${n}\") else ok(n)
effect fn noun(n: Int) -> String = if n > 3 then err(\"bad ${n}\") else ok(\"w\" + int.to_string(n))

effect fn f(s: String, p: Int, q: Int) -> String = {
  let (a, b, c, d) = fan { s + \"!\", noun(p), quiet(q), noun(1) }
  a + b + int.to_string(c) + d
}

effect fn g(n: Int) -> Int = {
  let r: Result[Int, String] = if n > 3 then err(\"rr ${n}\") else ok(n)
  let (a, b) = fan { r, quiet(1) }
  a + b
}

effect fn main() -> Unit = {
  var k = 0
  for i in 0..<30 {
    let p = if i % 3 == 0 then 5 else 1
    let q = if i % 2 == 0 then 6 else 2
    match f(\"hi\", p, q) {
      ok(v) => { k = k + string.len(v) },
      err(e) => { k = k + string.len(e) },
    }
    match g(p) {
      ok(v) => { k = k + v },
      err(e) => { k = k + string.len(e) },
    }
  }
  println(int.to_string(k))
}
";
    let dir = scratch("live");
    let almd = dir.join("main.almd");
    std::fs::write(&almd, src).expect("write source");
    let (native, _, code) = run(&almd, "rust");
    assert_eq!((native.as_str(), code), ("270\n", 0), "native leg");
    let out = Command::new(almide_bin())
        .arg("run")
        .arg(&almd)
        .args(["--target", "wasm"])
        .env("ALMIDE_WASM_ALLOC_COUNT", "1")
        .output()
        .expect("spawn almide run");
    assert_eq!(String::from_utf8_lossy(&out.stdout), "270\n", "wasm leg");
    let err = String::from_utf8_lossy(&out.stderr);
    let line = err.lines().find(|l| l.starts_with("__ALMD_WASM_ALLOC")).unwrap_or_else(|| panic!("no counter line:\n{err}"));
    let field = |k: &str| -> u64 {
        line.split_whitespace()
            .find_map(|w| w.strip_prefix(&format!("{k}=")))
            .and_then(|v| v.parse().ok())
            .unwrap_or_else(|| panic!("no {k} in {line}"))
    };
    assert_eq!(field("live"), 0, "a block outlived the run: {line}");
    assert_eq!(field("allocs"), field("frees"), "allocations and frees differ: {line}");
    let _ = std::fs::remove_dir_all(&dir);
}

/// An arm's top-level `!` — the writer's, or the one auto-try puts on a
/// Result call arm of a TAIL or `let`-bound fan — is the arm's own marker:
/// every arm still runs, and the lowest-index Err is the block's. Before,
/// the wasm leg lowered it as the frame's own exit, so an early arm's Err
/// left the frame (or aborted `main`) before a later arm ran.
#[test]
fn an_arm_level_bang_still_runs_every_arm() {
    check("arm-bang", "\
effect fn t(p: Int, q: Int) -> (Int, Int) = fan { boom(p), boom(q) }

effect fn u(p: Int, q: Int) -> Int = {
  let r = fan { boom(p), boom(q) }
  r.0 + r.1
}

effect fn v(p: Int, q: Int) -> Int = {
  let (a, b) = fan { boom(p)!, boom(q)! }
  a + b
}

effect fn main() -> Unit = {
  match t(5, 1) {
    ok((a, b)) => println(\"t ok ${a + b}\"),
    err(e) => println(\"t err ${e}\"),
  }
  match u(1, 6) {
    ok(x) => println(\"u ok ${x}\"),
    err(e) => println(\"u err ${e}\"),
  }
  match v(7, 5) {
    ok(x) => println(\"v ok ${x}\"),
    err(e) => println(\"v err ${e}\"),
  }
  match t(1, 2) {
    ok((a, b)) => println(\"t ok ${a + b}\"),
    err(e) => println(\"t err ${e}\"),
  }
  let r = fan { boom(8), boom(2) }
  println(\"unreachable ${r.0}\")
}
", "arm 5\narm 1\nt err boom 5\narm 1\narm 6\nu err boom 6\narm 7\narm 5\nv err boom 7\n\
arm 1\narm 2\nt ok 3\narm 8\narm 2\n", "Error: boom 8\n", 1);
}

/// A pure `-> Result[_, String]` call arm beside an effect arm, in a tail
/// fan: the pure arm's Err does not skip the effect arm.
#[test]
fn a_pure_result_arm_beside_an_effect_arm() {
    check("pure-arm", "\
fn mk(s: String) -> Result[String, String] = if string.len(s) > 0 then ok(s + \"!\") else err(\"empty\")

effect fn both(s: String) -> (String, Int) = fan {
  mk(s)
  boom(1)
}

effect fn main() -> Unit = {
  for s in [\"x\", \"\"] {
    match both(s) {
      ok((a, b)) => println(\"ok ${a} ${b}\"),
      err(e) => println(\"err ${e}\"),
    }
  }
}
", "arm 1\nok x! 1\narm 1\nerr empty\n", "", 0);
}
