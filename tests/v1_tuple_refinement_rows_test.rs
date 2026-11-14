//! A variant match whose arms refine a constructor's fields with a guard, a
//! nested String literal or a list shape, and fall through to a binding arm:
//!
//! ```text
//! EWith(EApp(EVar(tag), [x]), arms) if string.starts_with(tag, "%in:") => …,
//! EWith(s, arms) => …,
//! ```
//!
//! The regroup turns each constructor's arms into a tuple match over its
//! fields. Compiling that match column by column would copy the binding arm
//! into every branch that can reach it, so the certified native render
//! refused it (#4640). It is now an ordered chain: each row is tested in turn
//! (tag, literal and list-length tests, then the guard under the row's
//! binders) and each arm body is lowered once.
//!
//! Each program prints one fixed text on the native render (ownership
//! certificate required, rendered by the certified render itself) and on wasm.

use std::path::{Path, PathBuf};
use std::process::Command;

fn almide() -> String {
    std::env::var("ALMIDE_BIN").unwrap_or_else(|_| env!("CARGO_BIN_EXE_almide").to_string())
}

fn scratch(tag: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("almide-tuple-rows-{}-{tag}", std::process::id()));
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

const GUARDED_LIST_ROW: &str = r##"type E =
  | EVar(String)
  | ENum(Int)
  | EApp(E, List[E])
  | EWith(E, List[(String, E)])
  | EPair(E, Int, String)
  | EHole

fn show(e: E) -> String = match e {
  EVar(n) => n,
  ENum(k) => int.to_string(k),
  EApp(f, args) => show(f) + "(" + list.join(args |> list.map((a) => show(a)), ",") + ")",
  EWith(s, arms) => "with " + show(s) + "#" + int.to_string(list.len(arms)),
  EPair(a, k, t) => "<" + show(a) + "," + int.to_string(k) + "," + t + ">",
  EHole => "_",
}

fn pick(e: E) -> String = match e {
  EWith(EApp(EVar(tag), [x]), arms) if string.starts_with(tag, "%in:") and show(x) != "skip" => "in " + string.drop(tag, 4) + " " + show(x) + " " + int.to_string(list.len(arms)),
  EWith(EApp(EVar("pair"), [x, y]), arms) => "pair " + show(x) + "/" + show(y) + " " + int.to_string(list.len(arms)),
  EWith(EPair(ENum(7), k, t), _) => "seven " + int.to_string(k) + t,
  EWith(EPair(a, 3, "z"), arms) if list.len(arms) > 1 => "three " + show(a),
  EWith(EApp(f, []), _) => show(f),
  EWith(s, arms) => "with " + show(s) + list.join(arms |> list.map(((p, r)) => p + "=" + show(r)), ";"),
  _ => "other",
}

fn score(e: E) -> Int = match e {
  EWith(EApp(EVar(tag), [x]), _) if string.len(tag) > 3 => string.len(show(x)),
  EWith(EPair(ENum(n), k, "q"), arms) => n * 100 + k + list.len(arms),
  EWith(_, arms) => 0 - list.len(arms),
  _ => 99,
}

fn check(e: E) -> String! = match e {
  EWith(EApp(EVar("%bad"), [x]), _) => err("bad " + show(x)),
  EWith(EApp(EVar(tag), [x]), arms) if string.starts_with(tag, "%") => {
    guard list.len(arms) > 0 else err("no arms for " + tag)
    ok(tag + ":" + show(x))
  },
  EWith(s, _) => ok("plain " + show(s)),
  _ => ok("other"),
}

fn res(r: Result[String, String]) -> String = match r {
  ok(v) => "ok " + v,
  err(m) => "err " + m,
}

fn main() -> Unit = {
  let a1 = [("a", EHole)]
  let a2 = [("a", EHole), ("b", EVar("z"))]
  let es = [
    EWith(EApp(EVar("%in:h"), [EVar("y")]), a1),
    EWith(EApp(EVar("%in:h"), [EVar("skip")]), a1),
    EWith(EApp(EVar("%in:h"), [EVar("y"), EVar("w")]), a2),
    EWith(EApp(EVar("%in:h"), []), a2),
    EWith(EApp(EVar("pair"), [ENum(1), ENum(2)]), a2),
    EWith(EApp(EVar("pair"), [ENum(1)]), a2),
    EWith(EApp(EVar("%bad"), [EVar("q")]), a1),
    EWith(EApp(EVar("%ok"), [EVar("q")]), []),
    EWith(EPair(ENum(7), 5, "t"), a1),
    EWith(EPair(ENum(8), 3, "z"), a2),
    EWith(EPair(ENum(8), 3, "z"), a1),
    EWith(EPair(ENum(4), 2, "q"), a2),
    EWith(EApp(ENum(6), []), a1),
    EWith(EVar("v"), []),
    EApp(EVar("f"), [EHole]),
    ENum(3),
  ]
  for e in es {
    println(pick(e) + " | " + int.to_string(score(e)) + " | " + res(check(e)))
  }
}
"##;

#[test]
fn guarded_and_literal_rows_lower_each_arm_once() {
    prints(
        "guarded-rows",
        GUARDED_LIST_ROW,
        "in h y 1 | 1 | ok %in:h:y\n\
         with %in:h(skip)a=_ | 4 | ok %in:h:skip\n\
         with %in:h(y,w)a=_;b=z | -2 | ok plain %in:h(y,w)\n\
         %in:h | -2 | ok plain %in:h()\n\
         pair 1/2 2 | -2 | ok plain pair(1,2)\n\
         with pair(1)a=_;b=z | 1 | ok plain pair(1)\n\
         with %bad(q)a=_ | 1 | err bad q\n\
         with %ok(q) | 0 | err no arms for %ok\n\
         seven 5t | -1 | ok plain <7,5,t>\n\
         three 8 | -2 | ok plain <8,3,z>\n\
         with <8,3,z>a=_ | -1 | ok plain <8,3,z>\n\
         with <4,2,q>a=_;b=z | 404 | ok plain <4,2,q>\n\
         6 | -1 | ok plain 6()\n\
         with v | 0 | ok plain v\n\
         other | 99 | ok other\n\
         other | 99 | ok other\n",
    );
}

const NESTED_LITERAL_RESULT: &str = r##"type Raw =
  | RName(String)
  | RApp(Raw, Raw)
  | RLam(String, Raw, Raw)
  | RMatch(Raw, List[Arm])
  | RType
type Arm = { ctor: String, body: Raw }

fn calls(r: Raw) -> Raw! = match r {
  RName(n) => if n == "bad" then err("bad name") else ok(r),
  RApp(f, a) => ok(RApp(calls(f)!, calls(a)!)),
  other => ok(other),
}

fn go(body: Raw) -> Raw! = match body {
  RMatch(RName(v), arms) => if list.is_empty(arms) then ok(RName(v)) else ok(RApp(RName(v), RType)),
  RMatch(RApp(RName("%with"), e), arms) => {
    guard list.len(arms) > 0 else err("no arms")
    ok(RLam("w", e, calls(e)!))
  },
  RMatch(e, arms) => {
    let n = list.len(arms)
    ok(RApp(calls(e)!, RName(int.to_string(n))))
  },
  _ => calls(body),
}

fn show(r: Result[Raw, String]) -> String = match r {
  ok(v) => "ok ${v}",
  err(e) => "err ${e}",
}

fn main() -> Unit = {
  let a = [Arm { ctor: "c", body: RType }]
  println(show(go(RMatch(RName("x"), []))))
  println(show(go(RMatch(RName("x"), a))))
  println(show(go(RMatch(RApp(RName("%with"), RName("y")), a))))
  println(show(go(RMatch(RApp(RName("%with"), RName("y")), []))))
  println(show(go(RMatch(RApp(RName("f"), RName("y")), a))))
  println(show(go(RMatch(RApp(RName("f"), RName("bad")), a))))
  println(show(go(RApp(RName("g"), RType))))
}
"##;

#[test]
fn a_nested_string_literal_row_falls_through_to_the_binding_row() {
    prints(
        "nested-literal",
        NESTED_LITERAL_RESULT,
        "ok RName(\"x\")\n\
         ok RApp(RName(\"x\"), RType)\n\
         ok RLam(\"w\", RName(\"y\"), RName(\"y\"))\n\
         err no arms\n\
         ok RApp(RApp(RName(\"f\"), RName(\"y\")), RName(\"1\"))\n\
         err bad name\n\
         ok RApp(RName(\"g\"), RType)\n",
    );
}

const BOUND_FIELDS_MOVED_OUT: &str = r##"type E =
  | EVar(String)
  | ENum(Int)
  | EApp(E, List[E])
  | EWith(E, List[(String, E)])
  | EOpt(Option[String], Float)
  | EHole

fn show(e: E) -> String = match e {
  EVar(n) => n,
  ENum(k) => int.to_string(k),
  EApp(f, args) => show(f) + "(" + list.join(args |> list.map((a) => show(a)), ",") + ")",
  EWith(s, arms) => "with " + show(s) + "#" + int.to_string(list.len(arms)),
  EOpt(o, f) => "opt " + (o ?? "-") + " " + float.to_string(f),
  EHole => "_",
}

fn inner(e: E) -> E = match e {
  EWith(EApp(EVar("k"), [x]), _) => x,
  EWith(EApp(EVar(n), [_, y]), _) if n != "no" => y,
  EWith(EOpt(o, f), arms) if f > 1.5 => EOpt(o, if list.len(arms) > 0 then f + 1.0 else f),
  EWith(s, _) => s,
  _ => EHole,
}

fn main() -> Unit = {
  let a1 = [("a", EHole)]
  let es = [
    EWith(EApp(EVar("k"), [ENum(1)]), a1),
    EWith(EApp(EVar("k"), [ENum(1), ENum(2)]), a1),
    EWith(EApp(EVar("no"), [ENum(1), ENum(2)]), a1),
    EWith(EOpt(some("s"), 2.5), a1),
    EWith(EOpt(none, 0.5), a1),
    EVar("x"),
  ]
  for e in es {
    println(show(inner(e)))
  }
}
"##;

#[test]
fn a_bound_field_returned_whole_is_copied_out_of_its_borrow() {
    prints(
        "moved-out",
        BOUND_FIELDS_MOVED_OUT,
        "1\n2\nno(1,2)\nopt s 3.5\nopt - 0.5\n_\n",
    );
}
