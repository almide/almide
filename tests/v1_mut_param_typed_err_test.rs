//! A fn with a `mut` parameter raises through its declared Result whatever
//! the error type is (#4668).
//!
//! The move-mode rewrite carried the buffer on the err arm only for a `String`
//! error. With a variant, a record, a tuple or an Int error the wasm leg ended
//! the process at the first propagated err (or refused the program, for an
//! effect fn), and the certified native render refused the fn. Three shapes
//! around it were wrong on the certified native render too:
//!
//! - a `x!` that is the ARGUMENT of the write (`list.push(acc, chk(x)!)`): the
//!   raise reads the buffer, so the place was copied ahead of the operand and
//!   the push landed in the copy — the caller saw no write, with a `String`
//!   error as well;
//! - a call site that binds the typed Result (`let r: Result[Int, Fail] =
//!   add(xs, k)`): a field with no `Default` was taken out of the pair, which
//!   is not valid Rust.
//!
//! Each program runs on wasm and on the certified native render under the
//! ownership certificate with no fallback to the standard codegen; both must
//! print the expected text.
use std::process::Command;

const FIXTURE: &str = include_str!("../spec/wasm_cross/mut_param_typed_err_propagates.almd");

fn run(source: &str, extra: &[&str]) -> (Option<i32>, String, String) {
    let dir = tempfile::tempdir().expect("scratch");
    let file = dir.path().join("prog.almd");
    std::fs::write(&file, source).expect("source");
    let out = Command::new(env!("CARGO_BIN_EXE_almide")).arg("run").arg(&file).args(extra)
        .env("ALMIDE_CERTIFY_OWNERSHIP", "fail").env("ALMIDE_VERBOSE", "1")
        .current_dir(dir.path()).output().expect("almide run");
    (out.status.code(), String::from_utf8_lossy(&out.stdout).into_owned(), String::from_utf8_lossy(&out.stderr).into_owned())
}

/// Both legs print `expected`, and the native one is the certified render.
fn assert_both(source: &str, expected: &str) {
    for extra in [&[][..], &["--target", "wasm"][..]] {
        let (code, stdout, stderr) = run(source, extra);
        assert_eq!(code, Some(0), "{extra:?}: {stderr}");
        assert!(!stderr.contains("walled"), "{extra:?} fell back to the standard codegen: {stderr}");
        assert_eq!(stdout, expected, "{extra:?}");
    }
}

#[test]
fn the_fixture_prints_the_same_on_wasm_and_the_certified_native_render() {
    let (code, wasm, stderr) = run(FIXTURE, &["--target", "wasm"]);
    assert_eq!(code, Some(0), "{stderr}");
    for line in [
        "add: err big 2 len 2",
        "push_then_check: err big 3 len 4",
        "push_checked: ok 2 len 2",
        "push_checked: err big 2 len 2",
        "guarded: err plain len 4",
        "both: err over 2",
        "tally: err 30",
    ] {
        assert!(wasm.lines().any(|l| l == line), "missing `{line}` in:\n{wasm}");
    }
    assert_both(FIXTURE, &wasm);
}

const HEAD: &str = r#"type Fail =
  | Plain
  | Why(String)

fn show(e: Fail) -> String = match e {
  Plain => "plain",
  Why(m) => m,
}

fn chk(x: Int) -> Result[Int, Fail] =
  if x > 1 then err(Why("big " + int.to_string(x))) else ok(x)
"#;

#[test]
fn a_bound_typed_result_keeps_the_write_and_the_error() {
    let source = format!(
        r#"{HEAD}
fn add(mut acc: List[Int], x: Int) -> Result[Int, Fail] = {{
  list.push(acc, x)
  let v = chk(x)!
  ok(v + list.len(acc))
}}

effect fn main() -> Unit = {{
  var xs: List[Int] = [7]
  for k in 0..<4 {{
    let r: Result[Int, Fail] = add(xs, k)
    match r {{
      ok(n) => println("ok " + int.to_string(n)),
      err(e) => println("err " + show(e)),
    }}
    println("len " + int.to_string(list.len(xs)))
  }}
}}
"#
    );
    assert_both(&source, "ok 2\nlen 2\nok 4\nlen 3\nerr big 2\nlen 4\nerr big 3\nlen 5\n");
}

#[test]
fn a_raise_in_the_argument_of_the_write_does_not_lose_the_write() {
    // The `String` error took this path before #4668 and lost the push on the
    // certified native render.
    let source = r#"fn chk(x: Int) -> Result[Int, String] =
  if x > 1 then err("big " + int.to_string(x)) else ok(x)

fn add(mut acc: List[Int], x: Int) -> Result[Int, String] = {
  list.push(acc, chk(x)!)
  ok(list.len(acc))
}

effect fn main() -> Unit = {
  var xs: List[Int] = [7]
  for k in 0..<4 {
    match add(xs, k) {
      ok(n) => println("ok " + int.to_string(n)),
      err(e) => println("err " + e),
    }
    println("len " + int.to_string(list.len(xs)))
  }
}
"#;
    assert_both(source, "ok 2\nlen 2\nok 3\nlen 3\nerr big 2\nlen 3\nerr big 3\nlen 3\n");
}

#[test]
fn an_effect_fn_with_a_typed_error_propagates_through_two_levels() {
    let source = format!(
        r#"{HEAD}
effect fn inner(mut acc: List[Int], x: Int) -> Result[Int, Fail] = {{
  list.push(acc, x)
  let v = chk(x)!
  ok(v + list.len(acc))
}}

effect fn outer(mut acc: List[Int], x: Int) -> Result[Int, Fail] = {{
  let n = inner(acc, x)!
  list.push(acc, 50)
  ok(n + 1)
}}

effect fn main() -> Unit = {{
  var xs: List[Int] = []
  for k in 0..<3 {{
    match outer(xs, k) {{
      ok(n) => println("ok " + int.to_string(n)),
      err(e) => println("err " + show(e)),
    }}
    println("len " + int.to_string(list.len(xs)))
  }}
}}
"#
    );
    assert_both(&source, "ok 2\nlen 2\nok 5\nlen 4\nerr big 2\nlen 5\n");
}
