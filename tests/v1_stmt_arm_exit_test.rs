//! An early exit from the arm of a branch STATEMENT, and out of a loop, on the
//! certified native render (#4640).
//!
//! The MIR has no mid-function return, so a `guard c else E` whose block is
//! not the fn's tail was refused: in an arm of an `if` / `match` statement
//! with code after the branch, and — for an else that yields a value — in a
//! loop body under a branch or inside an arm. The render now makes the branch
//! the block's tail with the rest of the block in each arm that carries on
//! (`lower/arm_exit.rs`), carries a raising exit out of a loop as a
//! propagation, and a value exit through the loop early-return form.
//!
//! One shape rendered WRONGLY before: a bare `err(e)` arm beside an arm that
//! propagates. The continuation lift appended the rest of the block to the
//! `err(e)` arm as well, so the error was dropped and the fn returned its tail
//! (`err_arm(none)` below printed `after err arm` / `ok 0`).
//!
//! Each program runs on the native leg under the ownership certificate, with
//! no fallback to the standard codegen, and on wasm; both print the expected
//! text.
use std::process::Command;

/// Guards a user wrote in statement arms: a scalar, a String and a Result
/// else, two guards in one arm, a `guard let` two branches deep, two exiting
/// branches in a row, and a bare `err(e)` arm beside a propagating arm.
const IN_ARMS: &str = r##"fn mk(n: Int) -> Result[Int, String] =
  if n > 0 then ok(n) else err("no " + int.to_string(n))

fn show(r: Result[Int, String]) -> String = match r {
  ok(k) => "ok " + int.to_string(k),
  err(e) => "err " + e,
}

fn show_text(r: Result[String, String]) -> String = match r {
  ok(s) => "ok " + s,
  err(e) => "err " + e,
}

fn value_else(c: Bool, n: Int) -> Int = {
  var q = 1
  if c then {
    guard n > 0 else -1
    q = q + n
  } else ()
  println("after value")
  q * 10
}

fn text_else(o: Int?, s: String) -> String = {
  var out = "t:"
  match o {
    some(k) => {
      guard k < 5 else "too big " + int.to_string(k) + " for " + s
      out = out + s
    },
    none => (),
  }
  out + "."
}

fn result_else(c: Bool, n: Int) -> Result[Int, String] = {
  var q = 0
  if c then {
    guard n != 3 else ok(-3)
    guard n > 0 else err("neg " + int.to_string(n))
    q = q + n
  } else {
    q = 100
  }
  println("after result")
  ok(q)
}

fn nested(a: Bool, b: Bool, n: Int) -> Result[String, String] = {
  var log = ["s"]
  if a then {
    if b then {
      guard let v = mk(n) else err(e) => err("inner: " + e)
      log = log + [int.to_string(v)]
    } else ()
    log = log + ["mid"]
  } else ()
  log = log + ["end"]
  ok(list.join(log, " "))
}

fn two_branches(a: Bool, n: Int) -> Int = {
  var q = 0
  if a then {
    guard n > 0 else -1
    q = q + 1
  } else ()
  match n {
    7 => {
      guard a else -7
      q = q + 7
    },
    _ => (),
  }
  q
}

fn err_arm(o: Int?) -> Result[Int, String] = {
  var q = 0
  match o {
    some(k) => {
      let v = mk(k)!
      q = q + v
    },
    none => err("early"),
  }
  println("after err arm")
  ok(q)
}

fn main() -> Unit = {
  for n in [2, 0] {
    println(int.to_string(value_else(true, n)))
    println(text_else(some(n + 3), "x"))
    println(show(result_else(true, n + 1)))
    println(show_text(nested(true, true, n)))
    println(int.to_string(two_branches(true, n * 7 / 2)))
    println(show(err_arm(some(n))))
  }
  println(int.to_string(value_else(false, 0)))
  println(text_else(none, "y"))
  println(show(result_else(true, -2)))
  println(show(result_else(false, 0)))
  println(show_text(nested(true, false, 0)))
  println(show_text(nested(false, true, 0)))
  println(int.to_string(two_branches(false, 7)))
  println(int.to_string(two_branches(false, 0)))
  println(show(err_arm(none)))
}
"##;

const IN_ARMS_STDOUT: &str = "after value\n30\ntoo big 5 for x\nok -3\nok s 2 mid end\n8\nafter err arm\nok 2\n-1\nt:x.\nafter result\nok 1\nerr inner: no 0\n-1\nerr no 0\nafter value\n10\nt:.\nerr neg -2\nafter result\nok 100\nok s mid end\nok s end\n-7\n0\nerr early\n";

/// Exits out of a loop: a scalar from a branch in a `for`, `none` from an
/// Option fn, an `ok(..)` out of a `while`, a raise out of a branch in a
/// `for`, a loop inside an arm with code after the branch, and a String out
/// of a `for`.
const OUT_OF_LOOPS: &str = r##"fn mk(n: Int) -> Result[Int, String] =
  if n > 0 then ok(n) else err("no " + int.to_string(n))

fn pick(n: Int) -> Int? = if n > 0 then some(n) else none

fn show(r: Result[Int, String]) -> String = match r {
  ok(k) => "ok " + int.to_string(k),
  err(e) => "err " + e,
}

fn first_big(xs: List[Int]) -> Int = {
  var seen = 0
  for x in xs {
    if x > 0 then {
      guard x < 10 else x
      seen = seen + 1
    } else ()
  }
  println("scanned")
  0 - seen
}

fn all_picked(xs: List[Int]) -> String? = {
  var acc = "picked"
  for x in xs {
    if x != 7 then {
      guard let v = pick(x) else none
      acc = acc + " " + int.to_string(v)
    } else ()
  }
  some(acc)
}

fn sum_until(xs: List[Int]) -> Result[Int, String] = {
  var total = 0
  var i = 0
  while i < list.len(xs) {
    let x = list.get(xs, i) ?? 0
    guard let v = mk(x) else ok(0 - total)
    total = total + v
    i = i + 1
  }
  println("summed all")
  ok(total)
}

fn raise_in_loop(xs: List[Int]) -> Result[Int, String] = {
  var total = 0
  for x in xs {
    if x != 5 then {
      guard let v = mk(x) else err(e) => err("item: " + e)
      total = total + v
    } else ()
  }
  println("raised none")
  ok(total)
}

fn loop_in_arm(c: Bool, xs: List[Int]) -> Result[Int, String] = {
  var total = 0
  if c then {
    for x in xs {
      guard let v = mk(x) else ok(-1)
      total = total + v
    }
    println("summed")
  } else ()
  println("after arm")
  ok(total)
}

fn text_out_of_loop(xs: List[Int]) -> String = {
  var out = "s"
  for x in xs {
    guard let v = pick(x) else "stopped at " + out
    out = out + int.to_string(v)
  }
  out + "."
}

fn main() -> Unit = {
  println(int.to_string(first_big([1, 0, 2])))
  println(int.to_string(first_big([1, 30, 2])))
  println(all_picked([1, 7, 2]) ?? "nothing")
  println(all_picked([1, 0, 2]) ?? "nothing")
  println(show(sum_until([1, 2, 3])))
  println(show(sum_until([1, 2, 0, 3])))
  println(show(raise_in_loop([1, 5, 2])))
  println(show(raise_in_loop([1, 0, 2])))
  println(show(loop_in_arm(true, [1, 2])))
  println(show(loop_in_arm(true, [1, 0, 2])))
  println(show(loop_in_arm(false, [0])))
  println(text_out_of_loop([1, 2]))
  println(text_out_of_loop([1, 0, 2]))
}
"##;

const OUT_OF_LOOPS_STDOUT: &str = "scanned\n-2\n30\npicked 1 2\nnothing\nsummed all\nok 6\nok -3\nraised none\nok 3\nerr item: no 0\nsummed\nafter arm\nok 3\nok -1\nafter arm\nok 0\ns12.\nstopped at s1\n";

/// Run on the native leg under the ownership certificate — the certified
/// render, no fallback — and on the wasm leg; both print `stdout`.
fn renders(name: &str, src: &str, stdout: &str) {
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
fn an_exit_in_a_statement_arm_leaves_the_fn_on_the_certified_render() {
    renders("in_arms", IN_ARMS, IN_ARMS_STDOUT);
}

#[test]
fn an_exit_out_of_a_loop_leaves_the_fn_on_the_certified_render() {
    renders("out_of_loops", OUT_OF_LOOPS, OUT_OF_LOOPS_STDOUT);
}
