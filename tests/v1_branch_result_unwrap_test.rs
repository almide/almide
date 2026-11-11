//! `(if c then f(x) else ok(d))!` — a propagating unwrap whose operand is a
//! branch of Results. Once the `!` is a match, the branch is the match subject:
//! a merge of two owned Results, read by the arms and released once by the drop
//! of its type. Two things refused it. The ownership certificate counted a
//! merge as holding a reference only under four of the drop ops, so a merge of
//! any other Result type failed verification on a correct lowering; and inside
//! a loop (a tail call turned into one) the statement match refused the branch
//! as a subject no arm dispatch can read. The certificate now counts every
//! drop of the family, and the branch subject is bound to a `let` first.
use std::process::Command;

/// The unwrap of a branch of Results: destructured in a self tail call with a
/// list accumulator, destructured in a plain body, as a call argument over a
/// Result of a JSON value, and in a loop.
const SHAPE: &str = r##"import json

fn is_sym(ts: List[String], p: Int, s: String) -> Bool = (list.get(ts, p) ?? "") == s

fn subs_of(ts: List[String], p: Int) -> Result[(List[String], Int), String] =
  if is_sym(ts, p, ")") then ok((["s"], p + 1)) else err("expected )")

fn arms(ts: List[String], p: Int, acc: List[String]) -> Result[(List[String], Int), String] =
  if is_sym(ts, p, "}") then ok((acc, p + 1))
  else {
    let (subs, p2) = (if is_sym(ts, p, "(") then subs_of(ts, p + 1) else ok(([], p)))!
    arms(ts, p2 + 1, acc + subs)
  }

fn one(ts: List[String], p: Int) -> Result[(List[String], Int), String] = {
  let (subs, p2) = (if is_sym(ts, p, "(") then subs_of(ts, p + 1) else ok(([], p)))!
  ok((subs + ["z"], p2 + 100))
}

fn show(r: Result[(List[String], Int), String]) -> String =
  match r {
    ok((xs, p)) => "${list.join(xs, "+")} ${int.to_string(p)}",
    err(e) => e,
  }

fn field(text: String) -> Result[String, String] = {
  let decoded = json.parse(text)!
  let s = value.as_string((value.as_object(decoded) |> result.flat_map((_) => option.to_result(value.get(decoded, "result"), "missing field 'result'")))!)!
  ok(s)
}

fn main() -> Unit = {
  println(show(arms(["A", "(", ")", "B", "}"], 0, [])))
  println(show(arms(["A", "(", "B", "}"], 0, [])))
  println(show(one(["(", ")", "B", "}"], 0)))
  println(show(one(["(", "B", "}"], 0)))
  println(show(one(["B", "}"], 0)))
  println(field("{\"result\":\"hello\"}") ?? "none")
  println(match field("{\"other\":1}") { ok(s) => s, err(e) => e })
  println(match field("[1]") { ok(s) => s, err(_) => "not an object" })
  var total = 0
  let ts = ["A", "(", ")", "B", "(", ")", "C", "}"]
  for i in 0..<100000 {
    total = total + (match arms(ts, i % 2, []) {
      ok((xs, p)) => list.len(xs) + p,
      err(_) => 1,
    })
  }
  println(int.to_string(total))
}
"##;

const SHAPE_STDOUT: &str = "s 5\nexpected )\ns+z 102\nexpected )\nz 100\nhello\nmissing field 'result'\nnot an object\n1000000\n";

/// The spellings that never met either refusal: the branch bound to a `let`
/// before its unwrap, and the unwrap of a plain call.
const CONTROL: &str = r##"fn is_sym(ts: List[String], p: Int, s: String) -> Bool = (list.get(ts, p) ?? "") == s

fn subs_of(ts: List[String], p: Int) -> Result[(List[String], Int), String] =
  if is_sym(ts, p, ")") then ok((["s"], p + 1)) else err("expected )")

fn bound(ts: List[String], p: Int, acc: List[String]) -> Result[(List[String], Int), String] =
  if is_sym(ts, p, "}") then ok((acc, p + 1))
  else {
    let r = if is_sym(ts, p, "(") then subs_of(ts, p + 1) else ok(([], p))
    let (subs, p2) = r!
    bound(ts, p2 + 1, acc + subs)
  }

fn call(ts: List[String], p: Int, acc: List[String]) -> Result[(List[String], Int), String] =
  if is_sym(ts, p, "}") then ok((acc, p + 1))
  else {
    let (subs, p2) = subs_of(ts, p)!
    call(ts, p2, acc + subs)
  }

fn show(r: Result[(List[String], Int), String]) -> String =
  match r {
    ok((xs, p)) => "${list.join(xs, "+")} ${int.to_string(p)}",
    err(e) => e,
  }

fn main() -> Unit = {
  println(show(bound(["A", "(", ")", "B", "}"], 0, [])))
  println(show(bound(["A", "(", "B", "}"], 0, [])))
  println(show(call([")", ")", "}"], 0, [])))
  println(show(call([")", "x"], 0, [])))
}
"##;

const CONTROL_STDOUT: &str = "s 5\nexpected )\ns+s 3\nexpected )\n";

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
fn the_unwrap_of_a_branch_of_results_is_rendered_in_a_tail_loop_a_body_and_an_argument() {
    runs_unwalled("branch_result_unwrap", SHAPE, SHAPE_STDOUT);
}

#[test]
fn a_bound_branch_and_a_plain_call_unwrap_as_before() {
    runs_unwalled("branch_result_unwrap_control", CONTROL, CONTROL_STDOUT);
}
