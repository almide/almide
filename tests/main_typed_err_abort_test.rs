//! `main`'s abort on an error that is not a `String` (#3470, C-035).
//!
//! In `main`, a failed `!` (and a `fan { … }` block whose first Err decides)
//! ends the process with `Error: <msg>` on stderr and exit 1 on both targets.
//! Native renders the message from the error value — a `String` as is, a
//! `List[String]` joined with `", "`, any other type its `almide_repr` (the
//! text `"${e}"` shows). Before: wasm lowered only the `String` case and fell
//! into `unreachable` for every other error type (`Error: wasm trap: …`), and a
//! fan block in `main` with a typed-error arm walled (`fan-block-err-ty`).
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
    let dir = std::env::temp_dir().join(format!("almide-main-typed-err-{}-{name}", std::process::id()));
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

/// `typed(n)` fails with a `Bad` above 3; `loud(n)` prints, then does the
/// same; `num(n)` fails with a `String`.
const HELPERS: &str = "\
type Bad = | Bad(Int)

effect fn typed(n: Int) -> Result[Int, Bad] = if n > 3 then err(Bad(n)) else ok(n)

effect fn loud(n: Int) -> Result[Int, Bad] = {
  println(\"loud ${n}\")
  if n > 3 then err(Bad(n)) else ok(n)
}

effect fn num(n: Int) -> Result[Int, String] = if n > 3 then err(\"big ${n}\") else ok(n)
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

// ── `!` in main ───────────────────────────────────────────────────────

#[test]
fn the_reported_program() {
    check("reported", "\
effect fn main() -> Unit = {
  let a = typed(5)!
  println(\"${a}\")
}
", "", "Error: Bad(5)\n", 1);
}

#[test]
fn the_ok_path_is_unchanged() {
    check("ok", "\
effect fn main() -> Unit = {
  let a = typed(2)!
  println(\"${a}\")
}
", "2\n", "", 0);
}

#[test]
fn a_record_error_with_a_quoted_string() {
    check("record", "\
type E = { code: Int, msg: String }

effect fn f(n: Int) -> Result[Int, E] = if n > 3 then err({ code: n, msg: \"too \\\"big\\\"\" }) else ok(n)

effect fn main() -> Unit = {
  println(\"start\")
  let a = f(5)!
  println(\"${a}\")
}
", "start\n", "Error: E { code: 5, msg: \"too \\\"big\\\"\" }\n", 1);
}

#[test]
fn a_variant_error_with_a_string_payload() {
    check("variant-str", "\
type E = | NotFound(String) | Other(Int, List[String])

effect fn f(n: Int) -> Result[Int, E] = if n > 3 then err(NotFound(\"x\\ny\")) else ok(n)

effect fn main() -> Unit = {
  let a = f(5)!
  println(\"${a}\")
}
", "", "Error: NotFound(\"x\\ny\")\n", 1);
}

#[test]
fn a_variant_error_with_nested_payloads() {
    check("variant-nested", "\
type E = | NotFound(String) | Other(Int, List[String])

effect fn f(n: Int) -> Result[Int, E] = if n > 3 then err(Other(n, [\"a\", \"b\"])) else ok(n)

effect fn main() -> Unit = {
  let a = f(5)!
  println(\"${a}\")
}
", "", "Error: Other(5, [\"a\", \"b\"])\n", 1);
}

#[test]
fn a_record_error_with_float_unsigned_and_option_fields() {
    check("record-scalars", "\
type F = { v: Float, u: UInt64, o: Option[String] }

effect fn f(n: Int) -> Result[Int, F] = if n > 3 then err({ v: 1.5, u: 18446744073709551615, o: some(\"q\") }) else ok(n)

effect fn main() -> Unit = {
  let a = f(5)!
  println(\"${a}\")
}
", "", "Error: F { v: 1.5, u: 18446744073709551615, o: some(\"q\") }\n", 1);
}

#[test]
fn an_int_error() {
    check("int", "\
effect fn f(n: Int) -> Result[Int, Int] = if n > 3 then err(n) else ok(n)

effect fn main() -> Unit = {
  let a = f(5)!
  println(\"${a}\")
}
", "", "Error: 5\n", 1);
}

#[test]
fn a_list_of_strings_error_is_joined() {
    check("list-str", "\
effect fn f(n: Int) -> Result[Int, List[String]] = if n > 3 then err([\"a\", \"b\"]) else ok(n)

effect fn main() -> Unit = {
  let a = f(5)!
  println(\"${a}\")
}
", "", "Error: a, b\n", 1);
}

#[test]
fn a_float_error() {
    check("float", "\
effect fn f(n: Int) -> Result[Int, Float] = if n > 3 then err(1.5) else ok(n)

effect fn main() -> Unit = {
  let a = f(5)!
  println(\"${a}\")
}
", "", "Error: 1.5\n", 1);
}

#[test]
fn a_typed_bang_inside_an_expression_in_a_loop() {
    check("loop", "\
effect fn main() -> Unit = {
  for i in [1, 2, 4, 6] {
    println(\"v ${typed(i)! + 1}\")
  }
}
", "v 2\nv 3\n", "Error: Bad(4)\n", 1);
}

#[test]
fn an_option_none_for_comparison() {
    check("option", "\
effect fn main() -> Unit = {
  let xs: List[Int] = []
  let a = list.first(xs)!
  println(\"${a}\")
}
", "", "Error: none\n", 1);
}

// ── a fan block in main ──────────────────────────────────────────────

#[test]
fn fan_arms_with_a_typed_bang_run_every_arm_then_abort() {
    check("fan-bang", "\
effect fn main() -> Unit = {
  let (a, b) = fan { typed(5)!, loud(1)! }
  println(\"${a + b}\")
}
", "loud 1\n", "Error: Bad(5)\n", 1);
}

#[test]
fn fan_result_arms_with_a_typed_error_abort_with_the_first() {
    check("fan-typed", "\
effect fn main() -> Unit = {
  let (a, b) = fan { loud(5), loud(7) }
  println(\"${a + b}\")
}
", "loud 5\nloud 7\n", "Error: Bad(5)\n", 1);
}

#[test]
fn fan_mixed_error_types_abort_with_the_lowest_index_err() {
    check("fan-mixed", "\
effect fn main() -> Unit = {
  let (a, b, c) = fan { loud(2), num(7), loud(5) }
  println(\"${a + b + c}\")
}
", "loud 2\nloud 5\n", "Error: big 7\n", 1);
}

#[test]
fn fan_a_typed_err_before_a_string_err_decides() {
    check("fan-typed-first", "\
effect fn main() -> Unit = {
  let (a, b) = fan { loud(5), num(7) }
  println(\"${a + b}\")
}
", "loud 5\n", "Error: Bad(5)\n", 1);
}

#[test]
fn fan_in_a_loop_aborts_on_the_first_failing_iteration() {
    check("fan-loop", "\
effect fn main() -> Unit = {
  for i in [1, 4] {
    let (a, b) = fan { loud(i), loud(i + 1) }
    println(\"sum ${a + b}\")
  }
}
", "loud 1\nloud 2\nsum 3\nloud 4\nloud 5\n", "Error: Bad(4)\n", 1);
}

#[test]
fn fan_typed_arms_all_ok() {
    check("fan-ok", "\
effect fn main() -> Unit = {
  let (a, b) = fan { loud(2), loud(3) }
  println(\"${a + b}\")
}
", "loud 2\nloud 3\n5\n", "", 0);
}

#[test]
fn a_one_arm_typed_fan() {
    check("fan-one", "\
effect fn main() -> Unit = {
  let a = fan { loud(4) }
  println(\"${a}\")
}
", "loud 4\n", "Error: Bad(4)\n", 1);
}

// ── main returning a Result with a non-String error (#3474) ───────────
//
// `effect fn main() -> Result[Unit, E]` ending in `err(e)`: the same
// `Error: <msg>` + exit 1, the message rendered as main's `!` abort renders
// it. Before: rustc E0277 natively for a user type (the wrapper used
// `Display`), and E082 `main-err-carrier:non-string-err` on wasm for every
// error type but `String`.

#[test]
fn main_returning_a_user_error() {
    check("ret-variant", "\
effect fn main() -> Result[Unit, Bad] = {
  println(\"start\")
  err(Bad(3))
}
", "start\n", "Error: Bad(3)\n", 1);
}

#[test]
fn main_returning_a_record_error() {
    check("ret-record", "\
type E = { code: Int, msg: String }

effect fn main() -> Result[Unit, E] = {
  println(\"start\")
  err({ code: 7, msg: \"no \\\"way\\\"\" })
}
", "start\n", "Error: E { code: 7, msg: \"no \\\"way\\\"\" }\n", 1);
}

#[test]
fn main_returning_an_int_error() {
    check("ret-int", "\
effect fn main() -> Result[Unit, Int] = {
  println(\"start\")
  err(3)
}
", "start\n", "Error: 3\n", 1);
}

#[test]
fn main_returning_a_float_error() {
    check("ret-float", "\
effect fn main() -> Result[Unit, Float] = {
  println(\"start\")
  err(1.0)
}
", "start\n", "Error: 1\n", 1);
}

#[test]
fn main_returning_a_list_of_strings_error_is_joined() {
    check("ret-list-str", "\
effect fn main() -> Result[Unit, List[String]] = {
  println(\"start\")
  err([\"a\", \"b\"])
}
", "start\n", "Error: a, b\n", 1);
}

#[test]
fn main_returning_a_string_error_is_unchanged() {
    check("ret-str", "\
effect fn main() -> Result[Unit, String] = {
  println(\"start\")
  err(\"boom\")
}
", "start\n", "Error: boom\n", 1);
}

#[test]
fn main_returning_a_user_error_from_its_own_bang() {
    check("ret-variant-bang", "\
effect fn main() -> Result[Unit, Bad] = {
  println(\"start\")
  let a = typed(5)!
  println(\"${a}\")
  ok(())
}
", "start\n", "Error: Bad(5)\n", 1);
}

#[test]
fn main_returning_a_user_error_from_a_guard() {
    check("ret-variant-guard", "\
effect fn main() -> Result[Unit, Bad] = {
  println(\"start\")
  guard 1 > 2 else err(Bad(9))
  println(\"unreached\")
  ok(())
}
", "start\n", "Error: Bad(9)\n", 1);
}

#[test]
fn main_returning_a_user_error_ok_path() {
    check("ret-variant-ok", "\
effect fn main() -> Result[Unit, Bad] = {
  let a = typed(2)!
  println(\"${a}\")
  ok(())
}
", "2\n", "", 0);
}

// ── a fan block in a String-channel effect fn other than main ─────────
//
// A typed-error arm's `!` (or a bare typed-error Result arm) there is the
// arm's marker, not an exit (C-199): every arm runs, and the lowest-index
// Err is the block's, converted to the String channel by its repr exactly
// as a direct `typed(5)!` there converts (ADR-0021 D2). Before: wasm left
// the frame at the first arm's `!`, before a later arm ran, and walled a
// bare typed arm (`fan-block-err-ty`).

#[test]
fn a_direct_typed_bang_in_a_string_fn_is_its_repr() {
    check("sfn-direct", "\
effect fn g() -> Result[Int, String] = {
  let a = typed(5)!
  ok(a)
}

effect fn main() -> Unit = {
  let v = g()!
  println(\"${v}\")
}
", "", "Error: Bad(5)\n", 1);
}

#[test]
fn string_fn_fan_typed_bang_arms_run_every_arm() {
    check("sfn-fan-bang", "\
effect fn g() -> Result[Int, String] = {
  let (a, b) = fan { typed(5)!, loud(1)! }
  ok(a + b)
}

effect fn main() -> Unit = {
  let v = g()!
  println(\"${v}\")
}
", "loud 1\n", "Error: Bad(5)\n", 1);
}

#[test]
fn string_fn_fan_typed_result_arms_run_every_arm() {
    check("sfn-fan-typed", "\
effect fn g() -> Result[Int, String] = {
  let (a, b) = fan { typed(5), loud(1) }
  ok(a + b)
}

effect fn main() -> Unit = {
  let v = g()!
  println(\"${v}\")
}
", "loud 1\n", "Error: Bad(5)\n", 1);
}

#[test]
fn unit_effect_fn_fan_typed_bang_arms_run_every_arm() {
    check("ufn-fan-bang", "\
effect fn g() -> Unit = {
  let (a, b) = fan { typed(5)!, loud(1)! }
  println(\"${a + b}\")
}

effect fn main() -> Unit = {
  g()!
}
", "loud 1\n", "Error: Bad(5)\n", 1);
}

#[test]
fn string_fn_fan_a_string_err_before_a_typed_err_wins() {
    check("sfn-fan-mixed", "\
effect fn g() -> Result[Int, String] = {
  let (a, b, c) = fan { loud(2), num(7)!, loud(5)! }
  ok(a + b + c)
}

effect fn main() -> Unit = {
  let v = g()!
  println(\"${v}\")
}
", "loud 2\nloud 5\n", "Error: big 7\n", 1);
}

#[test]
fn string_fn_fan_a_typed_err_before_a_string_err_wins() {
    check("sfn-fan-typed-first", "\
effect fn g() -> Result[Int, String] = {
  let (a, b, c) = fan { loud(6)!, num(7)!, loud(1)! }
  ok(a + b + c)
}

effect fn main() -> Unit = {
  let v = g()!
  println(\"${v}\")
}
", "loud 6\nloud 1\n", "Error: Bad(6)\n", 1);
}

#[test]
fn string_fn_fan_a_list_of_strings_arm_is_joined() {
    check("sfn-fan-list-str", "\
effect fn many(n: Int) -> Result[Int, List[String]] = if n > 3 then err([\"x\", \"y\"]) else ok(n)

effect fn g() -> Result[Int, String] = {
  let (a, b) = fan { many(5)!, loud(1)! }
  ok(a + b)
}

effect fn main() -> Unit = {
  let v = g()!
  println(\"${v}\")
}
", "loud 1\n", "Error: x, y\n", 1);
}

#[test]
fn string_fn_fan_typed_arms_all_ok() {
    check("sfn-fan-ok", "\
effect fn g() -> Result[Int, String] = {
  let (a, b) = fan { typed(2)!, loud(3)! }
  ok(a + b)
}

effect fn main() -> Unit = {
  let v = g()!
  println(\"${v}\")
}
", "loud 3\n5\n", "", 0);
}

#[test]
fn string_fn_fan_caught_by_the_caller() {
    check("sfn-fan-caught", "\
effect fn g() -> Result[Int, String] = {
  let (a, b) = fan { typed(5)!, loud(1)! }
  ok(a + b)
}

effect fn main() -> Unit = {
  match g() {
    ok(v) => println(\"ok ${v}\"),
    err(e) => println(\"caught ${e}\"),
  }
}
", "loud 1\ncaught Bad(5)\n", "", 0);
}

#[test]
fn string_fn_fan_in_a_loop_with_a_bound_carrier() {
    check("sfn-fan-loop", "\
effect fn g(n: Int) -> Result[Int, String] = {
  let r: Result[Int, Bad] = typed(n)
  let (a, b) = fan { r!, loud(n + 1)! }
  ok(a + b)
}

effect fn main() -> Unit = {
  for i in [1, 2, 3, 4] {
    match g(i) {
      ok(v) => println(\"ok ${v}\"),
      err(e) => println(\"err ${e}\"),
    }
  }
}
", "loud 2\nok 3\nloud 3\nok 5\nloud 4\nerr Bad(4)\nloud 5\nerr Bad(4)\n", "", 0);
}
