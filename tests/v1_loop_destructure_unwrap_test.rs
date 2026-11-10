//! `let (s, q) = f(x)!` inside a `while` body of a Result fn. The loop's
//! propagation rewrite read a plain `let v = e!` and a bare `e!` statement, and
//! passed a destructuring bind by; its `!` then reached the lowering where no
//! early return exists and the function was refused ("unwrap `!` in a
//! call-argument position"). The destructure is now the payload bound whole and
//! destructured as the next statement, which the rewrite already carries.
use std::process::Command;

/// A destructuring unwrap first in a loop body, with a second propagation in a
/// branch after it, on the ok path and on both err paths, and in a hot loop.
const SHAPE: &str = r##"fn is_sym(ts: List[String], p: Int, s: String) -> Bool = (list.get(ts, p) ?? "") == s

fn expect(ts: List[String], p: Int, s: String) -> Result[Int, String] =
  if is_sym(ts, p, s) then ok(p + 1) else err("expected " + s)

fn sub(ts: List[String], p: Int) -> Result[(String, Int), String] =
  match list.get(ts, p) {
    some(t) => ok((t, p + 1)),
    none => err("eof"),
  }

fn subs(ts: List[String], p: Int) -> Result[(List[String], Int), String] = {
  var out: List[String] = []
  var q = p
  var going = true
  while going {
    let (s, q1) = sub(ts, q)!
    list.push(out, s)
    if is_sym(ts, q1, ",") then {
      q = q1 + 1
    } else {
      q = expect(ts, q1, ")")!
      going = false
    }
  }
  ok((out, q))
}

fn show(r: Result[(List[String], Int), String]) -> String =
  match r {
    ok((xs, p)) => "${list.join(xs, "+")} ${int.to_string(p)}",
    err(e) => e,
  }

fn main() -> Unit = {
  println(show(subs(["a", ",", "b", ")"], 0)))
  println(show(subs(["a", ",", "b"], 0)))
  println(show(subs(["a", "b"], 0)))
  println(show(subs(["a", ","], 0)))
  var total = 0
  let us = ["a", ",", "b", ",", "c", ")"]
  for i in 0..<100000 {
    total = total + (match subs(us, i % 2) {
      ok((xs, p)) => list.len(xs) + p,
      err(_) => 1,
    })
  }
  println(int.to_string(total))
}
"##;

const SHAPE_STDOUT: &str = "a+b 4\nexpected )\nexpected )\neof\n500000\n";

/// What the rewrite already carried: a plain propagating bind in the loop
/// body, and a destructuring unwrap outside any loop.
const CONTROL: &str = r##"fn is_sym(ts: List[String], p: Int, s: String) -> Bool = (list.get(ts, p) ?? "") == s

fn expect(ts: List[String], p: Int, s: String) -> Result[Int, String] =
  if is_sym(ts, p, s) then ok(p + 1) else err("expected " + s)

fn sub(ts: List[String], p: Int) -> Result[(String, Int), String] =
  match list.get(ts, p) {
    some(t) => ok((t, p + 1)),
    none => err("eof"),
  }

fn run(ts: List[String], p: Int) -> Result[Int, String] = {
  var q = p
  var going = true
  while going {
    let q1 = expect(ts, q, "a")!
    if is_sym(ts, q1, ",") then {
      q = q1 + 1
    } else {
      q = q1
      going = false
    }
  }
  ok(q)
}

fn first(ts: List[String]) -> Result[String, String] = {
  let (s, q) = sub(ts, 0)!
  ok(s + int.to_string(q))
}

fn main() -> Unit = {
  println(match run(["a", ",", "a", ")"], 0) { ok(q) => int.to_string(q), err(e) => e })
  println(match run(["a", ",", "b"], 0) { ok(q) => int.to_string(q), err(e) => e })
  println(first(["x"]) ?? "none")
  println(match first([]) { ok(s) => s, err(e) => e })
}
"##;

const CONTROL_STDOUT: &str = "3\nexpected a\nx1\neof\n";

fn runs_unwalled(name: &str, src: &str, stdout: &str) {
    let dir = tempfile::tempdir().expect("scratch");
    let file = dir.path().join(format!("{name}.almd"));
    std::fs::write(&file, src).expect("source");
    for extra in [&[][..], &["--target", "wasm"][..]] {
        let run = Command::new(env!("CARGO_BIN_EXE_almide")).arg("run").arg(&file).args(extra)
            .env("ALMIDE_CERTIFY_OWNERSHIP", "fail").env("ALMIDE_VERBOSE", "1")
            .current_dir(dir.path()).output().expect("almide run");
        let stderr = String::from_utf8_lossy(&run.stderr);
        assert!(run.status.success(), "{name} {extra:?}: {stderr}");
        assert!(!stderr.contains("walled"), "{name} {extra:?} fell back: {stderr}");
        assert_eq!(String::from_utf8_lossy(&run.stdout), stdout, "{name} {extra:?}");
    }
}

#[test]
fn a_destructuring_unwrap_in_a_loop_body_propagates_and_binds() {
    runs_unwalled("loop_destructure_unwrap", SHAPE, SHAPE_STDOUT);
}

#[test]
fn a_plain_unwrap_in_a_loop_and_a_destructure_outside_one_run_as_before() {
    runs_unwalled("loop_destructure_unwrap_control", CONTROL, CONTROL_STDOUT);
}
