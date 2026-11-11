//! A `!` that no rewrite gave a continuation, on the certified native render.
//!
//! Two lowering paths took such a `!` as it stood. A discarded one in statement
//! position (an arm's value, a statement of an arm's block) had its wrapper
//! stripped and its call run for effect only: the callee's err was received and
//! dropped, and the run went on. One in a scalar operand position ended the
//! process on the failing side, whatever fn it stood in.
//!
//! Both are right only where an err that leaves the fn ends the process anyway:
//! the void `main`, a test block. There the discarded one is now the die match
//! the statement rewrite builds; in any other fn the native render refuses the
//! fn, so the err is never dropped.
use std::process::Command;

const HELPERS: &str = r##"fn chk(n: Int) -> Result[Unit, String] =
  if n > 0 then ok(()) else err("bad " + int.to_string(n))

effect fn fin(s: String) -> Unit = {
  println(s)
  chk(string.len(s) - 3)!
}
"##;

/// The value of a branch nested in a match arm of the void `main`, with more of
/// `main` after the match.
const NESTED_ARM_OF_MAIN: &str = r##"effect fn main() -> Unit = {
  let n = string.len("")
  println("before")
  match int.parse("1") {
    err(_) => fin("abc")!,
    ok(k) =>
      if n + k > 2 then fin("xyz")!
      else fin("ab")!,
  }
  println("after")
}
"##;

/// The same branch as the whole body of a fn whose caller propagates.
const NESTED_ARM_OF_A_FN: &str = r##"effect fn play(n: Int) -> Unit =
  match int.parse("1") {
    err(_) => fin("abc")!,
    ok(k) =>
      if n + k > 2 then fin("xyz")!
      else fin("ab")!,
  }

effect fn main() -> Unit = {
  println("before")
  play(0)!
  println("after")
}
"##;

/// A statement of an arm's block beside an arm that is itself a `!`, in a fn
/// whose caller inspects the result.
const ARM_BLOCK_STATEMENT_OF_A_FN: &str = r##"effect fn step(s: String) -> Unit =
  match s {
    "g0" => chk(1)!,
    "g1" => println("one"),
    first => {
      chk(string.len(first) - 2)!
      println(first)
    },
  }

effect fn main() -> Unit = {
  for s in ["g0", "g1", "long", "g7"] {
    match step(s) {
      ok(_) => println("fine"),
      err(e) => println("caught " + e),
    }
  }
}
"##;

/// A test block whose only failure is a discarded `!` in a nested arm.
const NESTED_ARM_OF_A_TEST: &str = r##"test "a discarded unwrap in a nested arm fails the test" {
  let n = string.len("")
  match int.parse("1") {
    err(_) => chk(1)!,
    ok(k) =>
      if n + k > 2 then chk(2)!
      else chk(0)!,
  }
}
"##;

struct Run {
    code: Option<i32>,
    stdout: String,
    stderr: String,
}

fn almide(dir: &std::path::Path, args: &[&str]) -> Run {
    let out = Command::new(env!("CARGO_BIN_EXE_almide")).args(args)
        .env("ALMIDE_CERTIFY_OWNERSHIP", "fail").env("ALMIDE_VERBOSE", "1")
        .current_dir(dir).output().expect("almide");
    Run {
        code: out.status.code(),
        stdout: String::from_utf8_lossy(&out.stdout).into_owned(),
        stderr: String::from_utf8_lossy(&out.stderr).into_owned(),
    }
}

/// `almide run` on the native leg and on the wasm leg: both exit with `code`
/// and print `stdout`, and a failing run names `error` on stderr. `unwalled`
/// also requires the certified native render.
fn runs(name: &str, body: &str, code: i32, stdout: &str, error: &str, unwalled: bool) {
    let dir = tempfile::tempdir().expect("scratch");
    let file = format!("{name}.almd");
    std::fs::write(dir.path().join(&file), format!("{HELPERS}\n{body}")).expect("source");
    for extra in [&[][..], &["--target", "wasm"][..]] {
        let mut args = vec!["run", file.as_str()];
        args.extend_from_slice(extra);
        let run = almide(dir.path(), &args);
        assert_eq!(run.code, Some(code), "{name} {extra:?}: {}", run.stderr);
        assert_eq!(run.stdout, stdout, "{name} {extra:?}");
        assert!(run.stderr.contains(error), "{name} {extra:?}: {}", run.stderr);
        assert!(!unwalled || !run.stderr.contains("walled"), "{name} {extra:?} fell back: {}", run.stderr);
    }
}

#[test]
fn a_discarded_unwrap_in_a_nested_arm_of_main_ends_the_run_with_its_error() {
    runs("nested_arm_of_main", NESTED_ARM_OF_MAIN, 1, "before\nab\n", "Error: bad -1", true);
}

#[test]
fn a_discarded_unwrap_in_a_nested_arm_of_a_fn_never_drops_its_error() {
    runs("nested_arm_of_a_fn", NESTED_ARM_OF_A_FN, 1, "before\nab\n", "Error: bad -1", false);
}

#[test]
fn a_discarded_unwrap_in_an_arm_block_of_a_fn_reaches_the_caller() {
    let stdout = "fine\none\nfine\nlong\nfine\ncaught bad 0\n";
    runs("arm_block_statement_of_a_fn", ARM_BLOCK_STATEMENT_OF_A_FN, 0, stdout, "", false);
}

#[test]
fn a_discarded_unwrap_in_a_nested_arm_of_a_test_block_fails_the_test() {
    let dir = tempfile::tempdir().expect("scratch");
    std::fs::write(dir.path().join("nested_arm_test.almd"), format!("{HELPERS}\n{NESTED_ARM_OF_A_TEST}")).expect("source");
    // The default run takes the wasm leg first and re-runs a failing file on the
    // native leg: both must fail it. A native leg that drops the err passes the
    // re-run, which the runner reports as a wasm-only failure.
    let run = almide(dir.path(), &["test", "nested_arm_test.almd"]);
    assert_eq!(run.code, Some(1), "{}{}", run.stdout, run.stderr);
    assert!(run.stderr.contains("Error: bad 0"), "{}{}", run.stdout, run.stderr);
    assert!(run.stderr.contains("1 failed"), "{}{}", run.stdout, run.stderr);
    assert!(!run.stderr.contains("native re-run PASSED"), "the native leg passed the failing test: {}", run.stderr);
}
