//! `let (b, q) = if c then { let (h, r) = f(x)!; (h, r) } else (d, p)` — a
//! destructuring bind of a branch whose arm propagates, and the same with the
//! arm itself an unwrap (`if c then f(x)! else (d, p)`). Every lift of a `!`
//! out of a bound branch reads a plain `let v = <branch>`, so the destructuring
//! spelling kept its `!` inside the arm and the function was refused
//! ("heap-result `match` outside the executable subset"). The branch is now
//! bound whole and destructured as the next statement.
use std::process::Command;

/// A destructuring bind of a branch with a propagating block arm, with an
/// unwrap arm, and with one nested in the other; each on the taken arm, the
/// other arm and the err path, and in a hot loop.
const SHAPE: &str = r##"type Token = { kind: String, text: String }

fn tok(ts: List[Token], p: Int) -> Token = list.get(ts, p) ?? { kind: "eof", text: "" }

fn is_sym(ts: List[Token], p: Int, s: String) -> Bool = tok(ts, p).kind == "sym" and tok(ts, p).text == s

fn ident(ts: List[Token], p: Int) -> Result[(String, Int), String] =
  if tok(ts, p).kind == "ident" then ok((tok(ts, p).text, p + 1)) else err("expected a name")

fn block_arm(ts: List[Token], p: Int) -> Result[(String, Int), String] = {
  let (a, p1) = ident(ts, p)!
  let (b, p2) = if is_sym(ts, p1, ":") then {
    let (h, q) = ident(ts, p1 + 1)!
    (a + ":" + h, q)
  } else (a, p1)
  ok((b + "!", p2))
}

fn unwrap_arm(ts: List[Token], p: Int) -> Result[(String, Int), String] = {
  let (a, p1) = ident(ts, p)!
  let (b, p2) = if is_sym(ts, p1, ":") then ident(ts, p1 + 1)! else ("Eq", p1)
  ok((a + "=" + b, p2))
}

fn nested(ts: List[Token], p: Int) -> Result[(String, Int), String] = {
  let (a, p1) = ident(ts, p)!
  let (b, p2) = if is_sym(ts, p1, ":") then {
    let (h, q) = ident(ts, p1 + 1)!
    let (e, q2) = if is_sym(ts, q, ":") then ident(ts, q + 1)! else ("Eq", q)
    (a + ":" + h + ":" + e, q2)
  } else (a, p1)
  ok((b, p2))
}

fn lex(s: String) -> List[Token] =
  string.split(s, " ") |> list.map((w) => if w == ":" then { kind: "sym", text: w } else { kind: "ident", text: w })

fn show(r: Result[(String, Int), String]) -> String =
  match r {
    ok((s, p)) => "${s} ${int.to_string(p)}",
    err(e) => e,
  }

fn main() -> Unit = {
  for s in ["a : b", "a b", "a :", ":", "a : b : c"] {
    let ts = lex(s)
    println(show(block_arm(ts, 0)))
    println(show(unwrap_arm(ts, 0)))
    println(show(nested(ts, 0)))
  }
  let ts = lex("a : b : c")
  var total = 0
  for i in 0..<100000 {
    total = total + (match nested(ts, i % 3) {
      ok((s, p)) => string.len(s) + p,
      err(_) => 1,
    })
  }
  println(int.to_string(total))
}
"##;

const SHAPE_STDOUT: &str = "a:b! 3\na=b 3\na:b:Eq 3\na! 1\na=Eq 1\na 1\nexpected a name\nexpected a name\nexpected a name\nexpected a name\nexpected a name\nexpected a name\na:b! 3\na=b 3\na:b:c 5\n733336\n";

/// The spellings that never met the refusal: the branch bound whole before its
/// destructure, and a destructured branch with no propagation in it.
const CONTROL: &str = r##"type Token = { kind: String, text: String }

fn tok(ts: List[Token], p: Int) -> Token = list.get(ts, p) ?? { kind: "eof", text: "" }

fn is_sym(ts: List[Token], p: Int, s: String) -> Bool = tok(ts, p).kind == "sym" and tok(ts, p).text == s

fn ident(ts: List[Token], p: Int) -> Result[(String, Int), String] =
  if tok(ts, p).kind == "ident" then ok((tok(ts, p).text, p + 1)) else err("expected a name")

fn bound_whole(ts: List[Token], p: Int) -> Result[(String, Int), String] = {
  let (a, p1) = ident(ts, p)!
  let t = if is_sym(ts, p1, ":") then {
    let (h, q) = ident(ts, p1 + 1)!
    (a + ":" + h, q)
  } else (a, p1)
  let (b, p2) = t
  ok((b + "!", p2))
}

fn no_unwrap(ts: List[Token], p: Int) -> Result[(String, Int), String] = {
  let (a, p1) = ident(ts, p)!
  let (b, p2) = if is_sym(ts, p1, ":") then (a + ":", p1 + 1) else (a, p1)
  ok((b + "!", p2))
}

fn lex(s: String) -> List[Token] =
  string.split(s, " ") |> list.map((w) => if w == ":" then { kind: "sym", text: w } else { kind: "ident", text: w })

fn show(r: Result[(String, Int), String]) -> String =
  match r {
    ok((s, p)) => "${s} ${int.to_string(p)}",
    err(e) => e,
  }

fn main() -> Unit = {
  for s in ["a : b", "a b", "a :", ":"] {
    let ts = lex(s)
    println(show(bound_whole(ts, 0)))
    println(show(no_unwrap(ts, 0)))
  }
}
"##;

const CONTROL_STDOUT: &str = "a:b! 3\na:! 2\na! 1\na! 1\nexpected a name\na:! 2\nexpected a name\nexpected a name\n";

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
fn a_destructured_branch_with_a_propagating_arm_binds_and_propagates() {
    runs_unwalled("destructure_branch_unwrap", SHAPE, SHAPE_STDOUT);
}

#[test]
fn a_branch_bound_whole_and_a_destructured_branch_without_an_unwrap_run_as_before() {
    runs_unwalled("destructure_branch_unwrap_control", CONTROL, CONTROL_STDOUT);
}
