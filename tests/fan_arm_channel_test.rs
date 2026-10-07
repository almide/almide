//! The error channel of a fan arm (C-199, ADR-0024 D1, ADR-0021 D1):
//! every arm runs, the lowest-index Err is the block's.
//!
//! - #3468: a block arm whose tail is a Result call, with the fan as a fn
//!   tail or `let`-bound. Before: auto-try put the `?` into the block's tail,
//!   so native joined a Result as a payload (rustc E0308) and wasm left the
//!   frame at the first Err before the later arms ran.
//! - #3467: an arm `f(x)!` (or a bare Result arm, or a block arm with an
//!   inner `!`) over the enclosing fn's own TYPED error. Before: wasm read the
//!   `!` as an exit from the frame, so a later arm never ran; native joined
//!   into `Result<_, String>` and failed rustc E0277. An arm whose error
//!   cannot become the fn's error type (a `String` one in a `-> Result[_,
//!   Bad]` fn) is now E022 at check time, as the same `!` is (#2635).
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

/// `main` reports what `f` returned.
const REPORT_F: &str = "\
effect fn main() -> Unit = {
  match f(5) {
    ok(v) => println(\"ok ${v}\"),
    err(e) => println(\"err ${e}\"),
  }
  match f(2) {
    ok(v) => println(\"ok ${v}\"),
    err(e) => println(\"err ${e}\"),
  }
}
";

// ── #3467: a typed error ──────────────────────────────────────────────

#[test]
fn a_typed_bang_arm_runs_every_arm_then_returns_its_err() {
    check("typed-bang", &format!("\
effect fn f(p: Int) -> Result[Int, Bad] = {{
  let (a, b) = fan {{ typed(p)!, loud(1)! }}
  ok(a + b)
}}
{REPORT_F}"), "loud 1\nerr Bad(5)\nloud 1\nok 3\n", "", 0);
}

#[test]
fn a_bare_typed_result_arm() {
    check("typed-bare", &format!("\
effect fn f(p: Int) -> Result[Int, Bad] = {{
  let (a, b) = fan {{ typed(p), loud(1) }}
  ok(a + b)
}}
{REPORT_F}"), "loud 1\nerr Bad(5)\nloud 1\nok 3\n", "", 0);
}

#[test]
fn a_typed_bang_inside_a_block_arm() {
    check("typed-inner", &format!("\
effect fn f(p: Int) -> Result[Int, Bad] = {{
  let (a, b) = fan {{
    {{ let x = typed(p)!; println(\"arm0 after\"); x }},
    {{ println(\"arm1 runs\"); let y = typed(1)!; y + 1 }},
  }}
  ok(a + b)
}}
{REPORT_F}"), "arm1 runs\nerr Bad(5)\narm0 after\narm1 runs\nok 4\n", "", 0);
}

#[test]
fn both_typed_arms_fail_and_the_lowest_index_err_wins() {
    check("typed-both", "\
effect fn f(p: Int) -> Result[Int, Bad] = {
  let (a, b) = fan { loud(p)!, loud(p + 2)! }
  ok(a + b)
}

effect fn main() -> Unit = {
  match f(5) {
    ok(v) => println(\"ok ${v}\"),
    err(e) => println(\"err ${e}\"),
  }
}
", "loud 5\nloud 7\nerr Bad(5)\n", "", 0);
}

#[test]
fn a_typed_fan_as_the_fn_tail() {
    check("typed-tail", "\
effect fn f(p: Int) -> Result[(Int, Int), Bad] = fan { typed(p)!, loud(1) }

effect fn main() -> Unit = {
  match f(5) {
    ok((a, b)) => println(\"ok ${a + b}\"),
    err(e) => println(\"err ${e}\"),
  }
}
", "loud 1\nerr Bad(5)\n", "", 0);
}

/// The reported program's shape, with the second arm failing with `String`
/// in a `-> Result[_, Bad]` fn: the arm's Err leaves the fn as the block's
/// Err, and nothing converts a `String` into a `Bad` — rejected at check
/// time on both targets, as `loud(1)!` there already is (#2635).
#[test]
fn a_string_arm_in_a_typed_fn_is_a_check_error() {
    if !tool_available("rustc") || !tool_available("wasmtime") {
        return;
    }
    let dir = scratch("typed-mixed");
    let almd = dir.join("main.almd");
    std::fs::write(&almd, format!("{HELPERS}
effect fn noisy(n: Int) -> Int = {{
  println(\"noisy ${{n}}\")
  ok(n)
}}

effect fn f(p: Int) -> Result[Int, Bad] = {{
  let (a, b) = fan {{ typed(p)!, noisy(1) }}
  ok(a + b)
}}
{REPORT_F}")).expect("write source");
    for target in ["rust", "wasm"] {
        let (out, err, got) = run(&almd, target);
        assert_eq!(out, "", "[{target}]: nothing runs");
        assert!(
            err.contains("error[E022]: this fan arm's error cannot leave the block: the fn's error type is `Bad`, but this effect call fails with `String`"),
            "[{target}]: the arm is named, with both error types:\n{err}"
        );
        assert_eq!(got, 1, "[{target}]: exit code");
    }
    let _ = std::fs::remove_dir_all(&dir);
}

/// The threaded form joins into the fn's own error type: the emitted Rust
/// compiles warning-free and still spawns its arms.
#[test]
fn the_typed_threaded_form_compiles_warning_free() {
    if !tool_available("rustc") {
        return;
    }
    let dir = scratch("typed-threaded");
    let almd = dir.join("main.almd");
    std::fs::write(&almd, format!("{HELPERS}
effect fn f(p: Int) -> Result[Int, Bad] = {{
  let (a, b) = fan {{ typed(p)!, {{ println(\"arm1\"); typed(1)! + 1 }} }}
  ok(a + b)
}}
{REPORT_F}")).expect("write source");
    let emitted = Command::new(almide_bin()).arg(&almd).args(["--target", "rust"]).output().expect("spawn almide");
    assert!(emitted.status.success(), "emit failed:\n{}", String::from_utf8_lossy(&emitted.stderr));
    let rust = String::from_utf8_lossy(&emitted.stdout).into_owned();
    let user = rust.rsplit("//__ALMIDE_RT_BOUNDARY__").next().unwrap();
    assert!(user.contains("__almide_s.spawn"), "the arms keep their threads:\n{user}");
    assert!(user.contains("-> Result<_, Bad>"), "the join propagates into the fn's error type:\n{user}");
    let rs = dir.join("main.rs");
    std::fs::write(&rs, &rust).expect("write rust");
    let bin = dir.join("main");
    let rustc = Command::new("rustc")
        .args(["--edition", "2021", "-D", "warnings", "-A", "non_snake_case", "-A", "unused_macros", "-o"])
        .arg(&bin)
        .arg(&rs)
        .output()
        .expect("spawn rustc");
    assert!(rustc.status.success(), "the generated Rust does not compile warning-free:\n{}", String::from_utf8_lossy(&rustc.stderr));
    let out = Command::new(&bin).output().expect("run binary");
    assert_eq!(String::from_utf8_lossy(&out.stdout), "arm1\nerr Bad(5)\narm1\nok 4\n");
    assert_eq!(out.status.code(), Some(0));
    let _ = std::fs::remove_dir_all(&dir);
}
