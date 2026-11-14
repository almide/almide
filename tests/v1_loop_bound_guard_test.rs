//! A `guard c else err(e)` in an arm of a BOUND branch in a loop body
//! (`for a in args { let found = match a { …, _ => { …; guard g else err(e);
//! some(r) } }; … }`). A failing guard leaves the fn with `err(e)`. It used to
//! stay a `Guard`, and the arm became an if-result that merged the arm's
//! Option with the Err, which the certified native render refused (#4640). It
//! now takes the spelling the loop error flag already carries, `if g then ()
//! else { err(e)! }`, as the same guard does as a statement of the loop body.
//!
//! Each program prints one fixed text on the native render (ownership
//! certificate required, rendered by the certified render itself) and on wasm.

use std::path::{Path, PathBuf};
use std::process::Command;

fn almide() -> String {
    std::env::var("ALMIDE_BIN").unwrap_or_else(|_| env!("CARGO_BIN_EXE_almide").to_string())
}

fn scratch(tag: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("almide-loop-guard-{}-{tag}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

/// (stdout, stderr) of `almide run <src>` on `target`, the certificate required.
fn run(src: &Path, target: Option<&str>) -> (String, String) {
    let mut cmd = Command::new(almide());
    cmd.arg("run").arg(src).env("ALMIDE_CERTIFY_OWNERSHIP", "fail");
    if let Some(t) = target {
        cmd.args(["--target", t]);
    }
    let out = cmd.output().expect("almide");
    (String::from_utf8_lossy(&out.stdout).into_owned(), String::from_utf8_lossy(&out.stderr).into_owned())
}

/// `source` prints `expected` on the native render and on wasm, and the
/// native build is the certified render's own.
fn prints(tag: &str, source: &str, expected: &str) {
    if let Err(e) = almide_mir::pipeline::try_render_rust_source(source) {
        panic!("{tag} left the certified render: {e:?}");
    }
    let dir = scratch(tag);
    let src = dir.join("prog.almd");
    std::fs::write(&src, source).unwrap();
    for target in [None, Some("wasm")] {
        let (stdout, stderr) = run(&src, target);
        assert_eq!(stdout, expected, "{tag} on {target:?}:\n{stderr}");
    }
    let _ = std::fs::remove_dir_all(&dir);
}

const BOUND_MATCH_IN_FOR: &str = r##"type Term =
  | Var(Int)
  | App(Term, Term)
  | Meta(Int)

type St = { n: Int }

fn has_meta(t: Term) -> Bool = match t {
  Meta(_) => true,
  App(a, b) => has_meta(a) or has_meta(b),
  _ => false,
}

fn show(t: Term) -> String = "${t}"

fn infer(t: Term) -> (Term, Term)! = match t {
  Var(i) => if i < 0 then err("negative") else ok((t, Var(i + 1))),
  _ => ok((t, t)),
}

fn run(mut st: St, ctx: List[(String, Term)], args: List[Term], goal: Term) -> Term! = {
  let zctx = ctx |> list.map(((x, ty)) => (x, ty))
  guard not has_meta(goal) and not list.any(zctx, ((_, ty)) => has_meta(ty)) else err("simp: the goal is not known here (${show(goal)}); give its type")
  var rules: List[Term] = []
  for a in args {
    let found = match a {
      Var(0) => some(a),
      _ => {
        let (t, ty) = infer(a)!
        guard not has_meta(t) and not has_meta(ty) else err("simp: cannot tell ${show(t)}")
        some(App(t, ty))
      },
    }
    guard found != none else err("simp: a rule must prove an equation")
    match found {
      some(rl) => list.push(rules, rl),
      none => (),
    }
  }
  st.n = st.n + 1
  ok(list.fold(rules, goal, (acc, r) => App(acc, r)))
}

fn res(r: Result[Term, String]) -> String = match r {
  ok(t) => "ok ${t}",
  err(e) => "err ${e}",
}

fn main() -> Unit = {
  var st = St { n: 0 }
  println(res(run(st, [("x", Var(0))], [Var(0), Var(2)], Var(5))))
  println(res(run(st, [("x", Meta(0))], [Var(0)], Var(5))))
  println(res(run(st, [], [Var(-1)], Var(5))))
  println(res(run(st, [], [Meta(1)], Var(5))))
}
"##;

#[test]
fn a_guard_in_a_bound_match_arm_in_a_for_loop_raises_from_the_fn() {
    prints(
        "bound-match-for",
        BOUND_MATCH_IN_FOR,
        "ok App(App(Var(5), Var(0)), App(Var(2), Var(3)))\nerr simp: the goal is not known here (Var(5)); give its type\nerr negative\nerr simp: cannot tell Meta(1)\n",
    );
}

const BOUND_IF_AND_DESTRUCTURE: &str = r##"fn parse_all(xs: List[String]) -> List[Int]! = {
  var out: List[Int] = []
  for x in xs {
    let n = if string.starts_with(x, "#") then {
      let rest = string.drop(x, 1)
      guard rest != "" else err("empty tag")
      string.len(rest) * 100
    } else {
      let v = int.parse(x)!
      guard v >= 0 else err("negative ${x}")
      v
    }
    list.push(out, n)
  }
  ok(out)
}

fn count_until(xs: List[String], stop: String) -> Int! = {
  var i = 0
  var total = 0
  while i < list.len(xs) {
    let w = list.get(xs, i) ?? ""
    let (a, b) = match string.len(w) {
      0 => {
        guard i > 0 else err("starts empty")
        (0, 0)
      },
      n => {
        guard w != stop else err("hit ${stop} at ${i}")
        (n, 1)
      },
    }
    total = total + a + b
    i = i + 1
  }
  ok(total)
}

fn show(r: Result[List[Int], String]) -> String = match r {
  ok(v) => "ok ${v}",
  err(e) => "err ${e}",
}

fn show2(r: Result[Int, String]) -> String = match r {
  ok(v) => "ok ${v}",
  err(e) => "err ${e}",
}

fn main() -> Unit = {
  println(show(parse_all(["1", "#ab", "7"])))
  println(show(parse_all(["1", "#", "7"])))
  println(show(parse_all(["1", "-3", "7"])))
  println(show(parse_all(["x"])))
  println(show2(count_until(["ab", "", "c"], "zz")))
  println(show2(count_until(["", "c"], "zz")))
  println(show2(count_until(["ab", "zz", "c"], "zz")))
}
"##;

#[test]
fn guards_in_bound_if_and_destructured_match_arms_in_loops_raise_from_the_fn() {
    prints(
        "bound-if-while",
        BOUND_IF_AND_DESTRUCTURE,
        "ok [1, 200, 7]\nerr empty tag\nerr negative -3\nerr invalid digit found in string\nok 5\nerr starts empty\nerr hit zz at 1\n",
    );
}
