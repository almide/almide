//! A propagation in the arm of a branch STATEMENT of a Result fn, with more of
//! the function after the branch.
//!
//! Two things went wrong there. A loop in such an arm whose body holds a `!`
//! was refused: the loop's propagation rewrite reads a loop that is a statement
//! of a Result-typed block, and a unit arm is not one. And a `!` bound in such
//! an arm that the continuation lift did not take (a destructuring bind, or a
//! payload the lift does not admit) was rewritten where it stood into a match
//! whose err arm is the arm block's value, which a statement discards: the
//! error was dropped and the statements after the branch ran.
//!
//! The lift now also takes a branch whose arm holds a propagating loop and
//! reads a destructuring bind as it reads a plain one, and an arm that
//! propagates is no longer normalized in place.
//!
//! The same held for a `!` inside a value of the arm (`q = q + g(f()!)`), which
//! the arm's normalization hoisted to the spine first, and for a bare block
//! statement. The lift reads a `!` wherever the arm holds it and takes a block
//! statement as the one-armed branch; an arm that propagates anywhere is left
//! whole.
use std::process::Command;

/// A `while` holding a destructuring unwrap and a plain one, inside the then
/// arm of a unit `if`, on the ok path, each err path and in a hot loop.
const LOOP_IN_ARM: &str = r##"fn is_sym(ts: List[String], p: Int, s: String) -> Bool = (list.get(ts, p) ?? "") == s

fn expect(ts: List[String], p: Int, s: String) -> Result[Int, String] =
  if is_sym(ts, p, s) then ok(p + 1) else err("expected " + s)

fn ident(ts: List[String], p: Int) -> Result[(String, Int), String] =
  match list.get(ts, p) {
    some(t) => if t == "(" or t == ")" or t == "," then err("expected a name") else ok((t, p + 1)),
    none => err("eof"),
  }

fn show(r: Result[(List[String], Int), String]) -> String =
  match r {
    ok((xs, p)) => "${list.join(xs, "+")} ${int.to_string(p)}",
    err(e) => e,
  }

fn spec(ts: List[String], p: Int) -> Result[(List[String], Int), String] = {
  var names: List[String] = []
  var q = p
  if is_sym(ts, q, "(") then {
    q = q + 1
    var going = not is_sym(ts, q, ")")
    if not going then {
      q = q + 1
    } else ()
    while going {
      let (x, q1) = ident(ts, q)!
      list.push(names, x)
      q = q1
      if is_sym(ts, q, ",") then {
        q = q + 1
      } else {
        q = expect(ts, q, ")")!
        going = false
      }
    }
  } else ()
  ok((names, q))
}

fn main() -> Unit = {
  println(show(spec(["(", "a", ",", "b", ")"], 0)))
  println(show(spec(["(", ")"], 0)))
  println(show(spec(["x"], 0)))
  println(show(spec(["(", "a", "b"], 0)))
  println(show(spec(["(", "a", ",", ")"], 0)))
  var total = 0
  let us = ["(", "a", ",", "b", ",", "c", ")", "d"]
  for i in 0..<100000 {
    total = total + (match spec(us, (i % 2) * 7) {
      ok((xs, p)) => list.len(xs) + p,
      err(_) => 1,
    })
  }
  println(int.to_string(total))
}
"##;

const LOOP_IN_ARM_STDOUT: &str = "a+b 5\n 2\n 0\nexpected )\nexpected a name\n850000\n";

/// `let (x, q) = f()!` as the first statement of a unit `if` arm.
const DESTRUCTURE_IN_ARM: &str = r##"fn is_sym(ts: List[String], p: Int, s: String) -> Bool = (list.get(ts, p) ?? "") == s

fn expect(ts: List[String], p: Int, s: String) -> Result[Int, String] =
  if is_sym(ts, p, s) then ok(p + 1) else err("expected " + s)

fn ident(ts: List[String], p: Int) -> Result[(String, Int), String] =
  match list.get(ts, p) {
    some(t) => if t == "(" or t == ")" or t == "," then err("expected a name") else ok((t, p + 1)),
    none => err("eof"),
  }

fn show(r: Result[(List[String], Int), String]) -> String =
  match r {
    ok((xs, p)) => "${list.join(xs, "+")} ${int.to_string(p)}",
    err(e) => e,
  }

fn spec(ts: List[String], p: Int) -> Result[(List[String], Int), String] = {
  var names: List[String] = []
  var q = p
  if is_sym(ts, q, "(") then {
    let (x, q1) = ident(ts, q + 1)!
    list.push(names, x)
    q = q1
  } else ()
  ok((names, q))
}

fn main() -> Unit = {
  println(show(spec(["(", "a", ",", "b", ")"], 0)))
  println(show(spec(["(", ")"], 0)))
  println(show(spec(["x"], 0)))
  println(show(spec(["(", "a", "b"], 0)))
  println(show(spec(["(", "a", ",", ")"], 0)))
}
"##;

const DESTRUCTURE_IN_ARM_STDOUT: &str = "a 2\nexpected a name\n 0\na 2\na 2\n";

/// A plain bound `!` in the arm whose ok payload (a list of lists) the lift
/// does not admit: the render may refuse the function, never drop the error.
const UNADMITTED_IN_ARM: &str = r##"fn is_sym(ts: List[String], p: Int, s: String) -> Bool = (list.get(ts, p) ?? "") == s

fn expect(ts: List[String], p: Int, s: String) -> Result[Int, String] =
  if is_sym(ts, p, s) then ok(p + 1) else err("expected " + s)

fn ident(ts: List[String], p: Int) -> Result[(String, Int), String] =
  match list.get(ts, p) {
    some(t) => if t == "(" or t == ")" or t == "," then err("expected a name") else ok((t, p + 1)),
    none => err("eof"),
  }

fn show(r: Result[(List[String], Int), String]) -> String =
  match r {
    ok((xs, p)) => "${list.join(xs, "+")} ${int.to_string(p)}",
    err(e) => e,
  }

fn rows(ts: List[String], p: Int) -> Result[List[List[Int]], String] =
  if is_sym(ts, p, "a") then ok([[p, p + 1]]) else err("expected rows")

fn spec(ts: List[String], p: Int) -> Result[(List[String], Int), String] = {
  var names: List[String] = []
  var q = p
  if is_sym(ts, q, "(") then {
    let ns = rows(ts, q + 1)!
    list.push(names, int.to_string(list.len(ns)))
    q = q + 2
  } else ()
  ok((names, q))
}

fn main() -> Unit = {
  println(show(spec(["(", "a", ",", "b", ")"], 0)))
  println(show(spec(["(", ")"], 0)))
  println(show(spec(["x"], 0)))
  println(show(spec(["(", "a", "b"], 0)))
  println(show(spec(["(", "a", ",", ")"], 0)))
}
"##;

const UNADMITTED_IN_ARM_STDOUT: &str = "1 2\nexpected rows\n 0\n1 2\n1 2\n";

/// What already ran: a plain bound `!` of a scalar in the arm, and the same
/// loop as a statement of the function body.
const CONTROL: &str = r##"fn is_sym(ts: List[String], p: Int, s: String) -> Bool = (list.get(ts, p) ?? "") == s

fn expect(ts: List[String], p: Int, s: String) -> Result[Int, String] =
  if is_sym(ts, p, s) then ok(p + 1) else err("expected " + s)

fn ident(ts: List[String], p: Int) -> Result[(String, Int), String] =
  match list.get(ts, p) {
    some(t) => if t == "(" or t == ")" or t == "," then err("expected a name") else ok((t, p + 1)),
    none => err("eof"),
  }

fn show(r: Result[(List[String], Int), String]) -> String =
  match r {
    ok((xs, p)) => "${list.join(xs, "+")} ${int.to_string(p)}",
    err(e) => e,
  }

fn spec(ts: List[String], p: Int) -> Result[(List[String], Int), String] = {
  var names: List[String] = []
  var q = p
  if is_sym(ts, q, "(") then {
    let q1 = expect(ts, q + 1, "a")!
    list.push(names, "a")
    q = q1
  } else ()
  var going = is_sym(ts, q, ",")
  while going {
    let (x, q1) = ident(ts, q + 1)!
    list.push(names, x)
    q = q1
    going = is_sym(ts, q, ",")
  }
  ok((names, q))
}

fn main() -> Unit = {
  println(show(spec(["(", "a", ",", "b", ")"], 0)))
  println(show(spec(["(", ")"], 0)))
  println(show(spec(["x"], 0)))
  println(show(spec(["(", "a", "b"], 0)))
  println(show(spec(["(", "a", ",", ")"], 0)))
}
"##;

const CONTROL_STDOUT: &str = "a+b 4\nexpected a\n 0\na 2\nexpected a name\n";

/// A `!` inside a VALUE of a branch statement's arm — an operand, a call
/// argument, a list element, a condition, an interpolated text, a record field —
/// with more of the function after the branch. Normalizing the arm where it
/// stood brought the `!` to the arm's statement spine and rewrote it there: the
/// error became the arm's discarded value and `after …` printed.
const VALUE_IN_ARM: &str = r##"type Box = { v: Int, k: Int }

fn mk(n: Int) -> Result[Int, String] =
  if n > 0 then ok(n) else err("no " + int.to_string(n))

fn twice(v: Int) -> Int = v * 2

fn note(k: Int) -> Unit = println("note " + int.to_string(k))

fn show(r: Result[Int, String]) -> String =
  match r {
    ok(k) => "ok " + int.to_string(k),
    err(e) => "err " + e,
  }

fn in_operand(c: Bool, n: Int) -> Result[Int, String] = {
  var q = 0
  if c then {
    q = q + twice(mk(n)!)
  } else ()
  println("after operand")
  ok(q)
}

fn in_argument(c: Bool, n: Int) -> Result[Int, String] = {
  var q = 1
  if c then {
    note(twice(mk(n)!))
  } else {
    q = q + 7
  }
  println("after argument")
  ok(q)
}

fn in_list(c: Bool, n: Int) -> Result[Int, String] = {
  var q = 0
  match c {
    true => {
      let xs = [mk(n)!, mk(1)!]
      q = q + list.len(xs)
    },
    false => (),
  }
  println("after list")
  ok(q)
}

fn in_condition(c: Bool, n: Int) -> Result[Int, String] = {
  var q = 0
  if c then {
    if twice(mk(n)!) > 100 then {
      q = q + 1
    } else ()
    q = q + 2
  } else ()
  println("after condition")
  ok(q)
}

fn in_text(c: Bool, n: Int) -> Result[Int, String] = {
  if c then {
    println("v ${int.to_string(twice(mk(n)!))}")
  } else ()
  println("after text")
  ok(0)
}

fn in_field(c: Bool, n: Int) -> Result[Int, String] = {
  var q = 0
  if c then {
    let b = Box { v: mk(n)!, k: 1 }
    q = q + b.v + b.k
  } else ()
  println("after field")
  ok(q)
}

fn main() -> Unit = {
  for n in [3, 0] {
    println(show(in_operand(true, n)))
    println(show(in_argument(true, n)))
    println(show(in_list(true, n)))
    println(show(in_condition(true, n)))
    println(show(in_text(true, n)))
    println(show(in_field(true, n)))
  }
  println(show(in_operand(false, 0)))
  println(show(in_argument(false, 0)))
}
"##;

const VALUE_IN_ARM_STDOUT: &str = "after operand\nok 6\nnote 6\nafter argument\nok 1\nafter list\nok 2\nafter condition\nok 2\nv 6\nafter text\nok 0\nafter field\nok 4\nerr no 0\nerr no 0\nerr no 0\nerr no 0\nerr no 0\nerr no 0\nafter operand\nok 0\nafter argument\nok 8\n";

/// A bare block statement holding a `!`: the one-armed branch.
const BARE_BLOCK: &str = r##"fn chk(n: Int) -> Result[Unit, String] =
  if n > 0 then ok(()) else err("no " + int.to_string(n))

fn show(r: Result[Int, String]) -> String =
  match r {
    ok(k) => "ok " + int.to_string(k),
    err(e) => "err " + e,
  }

fn in_block(n: Int) -> Result[Int, String] = {
  {
    chk(n)!
    println("in block")
  }
  println("after block")
  ok(1)
}

fn main() -> Unit = {
  println(show(in_block(2)))
  println(show(in_block(0)))
}
"##;

const BARE_BLOCK_STDOUT: &str = "in block\nafter block\nok 1\nerr no 0\n";

/// A branch that ends a bare block statement and propagates.
const BLOCK_ENDING_BRANCH: &str = r##"fn chk(n: Int) -> Result[Unit, String] =
  if n > 0 then ok(()) else err("no " + int.to_string(n))

fn mk(n: Int) -> Result[Int, String] =
  if n > 0 then ok(n) else err("no " + int.to_string(n))

fn show(r: Result[Int, String]) -> String =
  match r {
    ok(k) => "ok " + int.to_string(k),
    err(e) => "err " + e,
  }

fn in_branch_ending_a_block(c: Bool, n: Int) -> Result[Int, String] = {
  var q = 0
  {
    println("in block")
    if c then {
      q = q + mk(n)!
      chk(n - 1)!
    } else ()
  }
  println("after block")
  ok(q)
}

fn main() -> Unit = {
  println(show(in_branch_ending_a_block(true, 2)))
  println(show(in_branch_ending_a_block(true, 1)))
  println(show(in_branch_ending_a_block(true, 0)))
  println(show(in_branch_ending_a_block(false, 0)))
}
"##;

const BLOCK_ENDING_BRANCH_STDOUT: &str =
    "in block\nafter block\nok 2\nin block\nerr no 0\nin block\nerr no 0\nin block\nafter block\nok 0\n";

/// Run on the native leg under the ownership certificate and on the wasm leg;
/// both print `stdout`. `unwalled` also requires the certified native render.
fn runs(name: &str, src: &str, stdout: &str, unwalled: bool) {
    let dir = tempfile::tempdir().expect("scratch");
    let file = dir.path().join(format!("{name}.almd"));
    std::fs::write(&file, src).expect("source");
    for extra in [&[][..], &["--target", "wasm"][..]] {
        let run = Command::new(env!("CARGO_BIN_EXE_almide")).arg("run").arg(&file).args(extra)
            .env("ALMIDE_CERTIFY_OWNERSHIP", "fail").env("ALMIDE_VERBOSE", "1")
            .current_dir(dir.path()).output().expect("almide run");
        let stderr = String::from_utf8_lossy(&run.stderr);
        assert!(run.status.success(), "{name} {extra:?}: {stderr}");
        assert!(!unwalled || !stderr.contains("walled"), "{name} {extra:?} fell back: {stderr}");
        assert_eq!(String::from_utf8_lossy(&run.stdout), stdout, "{name} {extra:?}");
    }
}

#[test]
fn a_loop_that_propagates_inside_a_branch_arm_renders_and_propagates() {
    runs("loop_in_arm", LOOP_IN_ARM, LOOP_IN_ARM_STDOUT, true);
}

#[test]
fn a_destructuring_unwrap_in_a_branch_arm_propagates_its_error() {
    runs("destructure_in_arm", DESTRUCTURE_IN_ARM, DESTRUCTURE_IN_ARM_STDOUT, true);
}

#[test]
fn an_unadmitted_unwrap_in_a_branch_arm_never_drops_its_error() {
    runs("unadmitted_in_arm", UNADMITTED_IN_ARM, UNADMITTED_IN_ARM_STDOUT, false);
}

#[test]
fn a_scalar_unwrap_in_an_arm_and_a_loop_in_the_body_run_as_before() {
    runs("arm_propagation_control", CONTROL, CONTROL_STDOUT, true);
}

#[test]
fn a_propagation_inside_a_value_of_a_branch_arm_propagates_its_error() {
    runs("value_in_arm", VALUE_IN_ARM, VALUE_IN_ARM_STDOUT, true);
}

#[test]
fn a_bare_block_statement_propagates_its_error() {
    runs("bare_block", BARE_BLOCK, BARE_BLOCK_STDOUT, true);
}

#[test]
fn a_branch_ending_a_bare_block_statement_propagates_its_error() {
    runs("block_ending_branch", BLOCK_ENDING_BRANCH, BLOCK_ENDING_BRANCH_STDOUT, true);
}
