//! The result slot of a loop-converted tail call on the certified native
//! render (#4640, #3550).
//!
//! A self tail call that hands a heap value on becomes a loop, and a base
//! that reads a binding of the loop body delivers its value through a result
//! slot that needs a placeholder before the first delivery. A declared
//! variant had none, so such a fn kept the heap tail-call refusal; a
//! scalar-Ok Result was kept out of the slot altogether. The slot now starts
//! at the variant's shallowest constructor (a tuple at its components'), a
//! delivery the field builders decline goes through the ordinary bind, and a
//! scalar-Ok Result keeps its declared carrier.
use std::process::Command;

/// The program on the native leg under the ownership certificate with no
/// fallback to the standard codegen, and on wasm: both print `expected`.
fn renders_and_agrees(name: &str, source: &str, expected: &str) {
    let dir = tempfile::tempdir().expect("scratch");
    let file = dir.path().join(format!("{name}.almd"));
    std::fs::write(&file, source).expect("source");
    for extra in [&[][..], &["--target", "wasm"][..]] {
        let run = Command::new(env!("CARGO_BIN_EXE_almide")).arg("run").arg(&file).args(extra)
            .env("ALMIDE_CERTIFY_OWNERSHIP", "fail").env("ALMIDE_VERBOSE", "1")
            .current_dir(dir.path()).output().expect("almide run");
        let stderr = String::from_utf8_lossy(&run.stderr);
        assert!(run.status.success(), "{name} {extra:?}: {stderr}");
        assert!(!stderr.contains("walled"), "{name} {extra:?} fell back: {stderr}");
        assert_eq!(String::from_utf8_lossy(&run.stdout), expected, "{name} {extra:?}");
    }
}

/// A tuple of a variant and a count; the base reads the match binder.
const OFFSET: &str = r##"type Level =
  | LZero
  | LSucc(Level)
  | LMax(Level, Level)

fn offset(l: Level, k: Int) -> (Level, Int) = match l {
  LSucc(a) => offset(a, k + 1),
  other => (other, k),
}

fn show(l: Level) -> String = match l {
  LZero => "0",
  LSucc(a) => "S(" + show(a) + ")",
  LMax(a, b) => "max(" + show(a) + "," + show(b) + ")",
}

effect fn main() -> Unit = {
  let (l, k) = offset(LSucc(LSucc(LMax(LZero, LSucc(LZero)))), 0)
  println(show(l) + " " + int.to_string(k))
}
"##;

const OFFSET_STDOUT: &str = "max(0,S(0)) 2\n";

/// A variant with no payload-free constructor beside a list accumulator.
const SPINE: &str = r##"type Term =
  | V(String)
  | App(Term, Term)

fn spine_acc(t: Term, args: List[Term]) -> (Term, List[Term]) = match t {
  App(f, a) => spine_acc(f, [a] + args),
  other => (other, args),
}

fn show(t: Term) -> String = match t {
  V(s) => s,
  App(f, a) => "(" + show(f) + " " + show(a) + ")",
}

effect fn main() -> Unit = {
  let (h, args) = spine_acc(App(App(V("f"), V("x")), App(V("g"), V("y"))), [])
  println(show(h) + " " + (args |> list.map((a) => show(a)) |> list.join(",")))
}
"##;

const SPINE_STDOUT: &str = "f x,(g y)\n";

/// A tail call beside non-tail self calls, the bases reading several let binds.
const LEVEL_SIMP: &str = r##"type Level = | LZero | LSucc(Level) | LMax(Level, Level) | LIMax(Level, Level) | LParam(String) | LMeta(Int)

fn is_zero(l: Level) -> Bool = match l { LZero => true, _ => false }
fn never_zero(l: Level) -> Bool = match l { LSucc(_) => true, LMax(a, b) => never_zero(a) or never_zero(b), _ => false }
fn level_leq(a: Level, b: Level) -> Bool = match a {
  LZero => true,
  LSucc(x) => match b { LSucc(y) => level_leq(x, y), _ => false },
  _ => show(a) == show(b),
}

fn level_simp(l: Level) -> Level = match l {
  LSucc(a) => LSucc(level_simp(a)),
  LMax(a, b) => {
    let x = level_simp(a)
    let y = level_simp(b)
    if is_zero(x) then y
    else if is_zero(y) then x
    else if level_leq(x, y) then y
    else if level_leq(y, x) then x
    else LMax(x, y)
  },
  LIMax(a, b) => {
    let y = level_simp(b)
    if is_zero(y) then LZero
    else if never_zero(y) then level_simp(LMax(a, y))
    else {
      let x = level_simp(a)
      if level_leq(x, y) then y else LIMax(x, y)
    }
  },
  other => other,
}

fn show(l: Level) -> String = match l {
  LZero => "0",
  LSucc(a) => "S(" + show(a) + ")",
  LMax(a, b) => "max(" + show(a) + "," + show(b) + ")",
  LIMax(a, b) => "imax(" + show(a) + "," + show(b) + ")",
  LParam(p) => p,
  LMeta(i) => "?" + int.to_string(i),
}

effect fn main() -> Unit = {
  println(show(level_simp(LIMax(LSucc(LZero), LSucc(LMax(LZero, LParam("u")))))))
  println(show(level_simp(LIMax(LParam("v"), LParam("u")))))
  println(show(level_simp(LIMax(LParam("v"), LMax(LZero, LZero)))))
  println(show(level_simp(LMax(LSucc(LZero), LSucc(LSucc(LZero))))))
  println(show(level_simp(LIMax(LIMax(LParam("a"), LSucc(LZero)), LSucc(LParam("b"))))))
}
"##;

const LEVEL_SIMP_STDOUT: &str = "S(u)\nimax(v,u)\n0\nS(S(0))\nmax(max(a,S(0)),S(b))\n";

/// The tail call inside a nested match over a let-bound self call.
const BETA: &str = r##"type Term = | Sort(Int) | Var(Int) | Const(String, List[Int]) | App(Term, Term) | Lam(String, Term, Term) | Pi(String, Term, Term) | Meta(Int)

fn subst(b: Term, a: Term) -> Term = match b {
  Var(i) => if i == 0 then a else Var(i - 1),
  App(f, x) => App(subst(f, a), subst(x, a)),
  Lam(n, t, c) => Lam(n, subst(t, a), c),
  other => other,
}

fn beta_go(t: Term, fuel: Int) -> Term = match t {
  App(f, a) => {
    let g = beta_go(f, fuel)
    match g {
      Lam(_, _, b) => if fuel > 0 then beta_go(subst(b, a), fuel - 1) else App(g, a),
      _ => App(g, beta_go(a, fuel)),
    }
  },
  Lam(x, ty, b) => Lam(x, beta_go(ty, fuel), beta_go(b, fuel)),
  Pi(x, ty, b) => Pi(x, beta_go(ty, fuel), beta_go(b, fuel)),
  other => other,
}

fn show(t: Term) -> String = match t {
  Sort(i) => "Sort" + int.to_string(i),
  Var(i) => "#" + int.to_string(i),
  Const(s, _) => s,
  App(f, a) => "(" + show(f) + " " + show(a) + ")",
  Lam(n, t, b) => "(\\" + n + ":" + show(t) + "." + show(b) + ")",
  Pi(n, t, b) => "(Pi " + n + ":" + show(t) + "." + show(b) + ")",
  Meta(i) => "?" + int.to_string(i),
}

effect fn main() -> Unit = {
  let id = Lam("x", Sort(0), Var(0))
  println(show(beta_go(App(id, Const("c", [])), 5)))
  println(show(beta_go(App(App(Lam("f", Sort(1), Lam("y", Sort(0), App(Var(0), Var(0)))), id), Const("d", [1])), 5)))
  println(show(beta_go(App(id, Const("c", [])), 0)))
  println(show(beta_go(Pi("a", App(id, Meta(3)), App(Const("k", []), App(id, Var(7)))), 2)))
}
"##;

const BETA_STDOUT: &str = "c\n(d d)\n((\\x:Sort0.#0) c)\n(Pi a:?3.(k #7))\n";

/// `Result[Int, String]`: the tail call behind `!` and `ok(..)`, bases in match arms.
const WALK: &str = r##"effect fn fetch(page: Int) -> Result[String, String] = if page > 3 then err("gone ${page}") else ok("p${page}")

fn grow(html: String, k: Int) -> Int = k + string.len(html)

effect fn walk(dir: String, page: Int, known: Int) -> Result[Int, String] = if page > 9 then ok(known)
else match fetch(page) {
  err(reason) => if page == 1 then err(reason) else ok(known),
  ok(html) => {
    let grown = grow(html, known)
    if grown <= known then ok(known)
    else {
      println("${dir}/p${page}.html")
      let deeper = walk(dir, page + 1, grown)!
      ok(deeper)
    }
  },
}

effect fn main() -> Unit = {
  match walk("d", 1, 0) {
    ok(n) => println("ok ${n}"),
    err(e) => println("err ${e}"),
  }
  match walk("d", 5, 0) {
    ok(n) => println("ok ${n}"),
    err(e) => println("err ${e}"),
  }
}
"##;

const WALK_STDOUT: &str = "d/p1.html\nd/p2.html\nd/p3.html\nok 6\nok 0\n";

/// `Result[Unit, String]`: a guard, a `!`, and the tail call in a nested arm with a grown list.
const RUN: &str = r##"effect fn step(messages: List[String], n: Int) -> Result[String, String] = if n > 5 then err("too far") else ok("m${n}/${list.len(messages)}")

effect fn run(model: String, messages: List[String], n: Int, yolo: Bool) -> Result[Unit, String] = {
  guard n < 4 else { println("limit") ; ok(()) }
  let resp = step(messages, n)!
  println(resp)
  match list.first(messages) {
    none => { println("finished ${model}") ; ok(()) },
    some(tu) => {
      let go = if yolo then true else string.len(tu) > 0
      if not go then { println("stopped") ; ok(()) }
      else {
        let a = "a" + tu
        run(model, messages + [a, resp], n + 1, yolo)
      }
    },
  }
}

effect fn main() -> Unit = {
  match run("m", ["x"], 0, true) { ok(_) => println("ok"), err(e) => println("err ${e}") }
  match run("m", [], 0, true) { ok(_) => println("ok"), err(e) => println("err ${e}") }
  match run("m", [""], 2, false) { ok(_) => println("ok"), err(e) => println("err ${e}") }
}
"##;

const RUN_STDOUT: &str = "m0/1\nm1/3\nm2/5\nm3/7\nlimit\nok\nm0/0\nfinished m\nok\nm2/1\nstopped\nok\n";

/// A `var` Result slot a user-written loop rebinds from its arms.
const USER_SLOT: &str = r##"fn f(n: Int) -> Result[Int, String] = {
  var r: Result[Int, String] = err("none")
  var i = 0
  while i < n {
    if i % 3 == 1 then { r = ok(i * 2) } else { r = err("e${i}") }
    i = i + 1
  }
  r
}
fn g(n: Int) -> Result[Int, String] = {
  var r: Result[Int, String] = err("")
  var i = 0
  while i < n {
    r = ok(i)
    i = i + 1
  }
  r
}
effect fn main() -> Unit = {
  for n in 0..<5 {
    match f(n) { ok(v) => println("ok ${v}"), err(e) => println("err ${e}") }
    match g(n) { ok(v) => println("ok ${v}"), err(e) => println("err ${e}") }
  }
}
"##;

const USER_SLOT_STDOUT: &str = "err none\nerr \nerr e0\nok 0\nok 2\nok 1\nerr e2\nok 2\nerr e3\nok 3\n";

/// Scalar arguments only, 300000 deep: the fns recursed frame by frame on this render and overflowed
/// where wasm loops. And `let d = f(..)!; ok(d)`, on both exits.
const DEEP: &str = r##"fn count(n: Int, k: Int) -> Result[Int, String] = {
  let cur = (k * 31 + n) % 1009
  if n <= 0 then ok(cur) else count(n - 1, cur)
}

fn parity(n: Int, k: Int) -> Result[Bool, String] = {
  let cur = (k * 31 + n) % 1009
  if n <= 0 then ok(cur % 2 == 0)
  else if cur == 1008 and n < 0 then err("never ${cur}")
  else parity(n - 1, cur)
}

effect fn step(k: Int) -> Result[Int, String] = if k < 0 then err("neg") else ok(k % 5)

effect fn chain(k: Int, acc: Int) -> Result[Int, String] = {
  let s = step(k)!
  if k >= 20000 then ok(acc + s)
  else {
    let deeper = chain(k + 1, acc + s)!
    ok(deeper)
  }
}

effect fn main() -> Unit = {
  println(match count(300000, 1) { ok(v) => "ok ${v}", err(e) => "err " + e })
  println(match parity(300000, 1) { ok(v) => (if v then "ok T" else "ok F"), err(e) => "err " + e })
  println(match chain(0, 0) { ok(v) => "ok ${v}", err(e) => "err " + e })
  println(match chain(0 - 3, 0) { ok(v) => "ok ${v}", err(e) => "err " + e })
}
"##;

const DEEP_STDOUT: &str = "ok 162\nok T\nok 40000\nerr neg\n";

#[test]
fn a_tuple_of_a_variant_result_loops_from_the_shallowest_constructor() {
    renders_and_agrees("offset", OFFSET, OFFSET_STDOUT);
}

#[test]
fn a_variant_without_a_unit_constructor_takes_a_leaf_placeholder() {
    renders_and_agrees("spine", SPINE, SPINE_STDOUT);
}

#[test]
fn a_variant_result_beside_non_tail_self_calls_keeps_the_other_calls() {
    renders_and_agrees("level_simp", LEVEL_SIMP, LEVEL_SIMP_STDOUT);
}

#[test]
fn a_variant_result_tail_call_in_a_nested_match_arm_loops() {
    renders_and_agrees("beta", BETA, BETA_STDOUT);
}

#[test]
fn a_scalar_ok_result_accumulator_carries_ok_and_err_bases() {
    renders_and_agrees("walk", WALK, WALK_STDOUT);
}

#[test]
fn a_unit_ok_result_accumulator_runs_every_exit() {
    renders_and_agrees("run", RUN, RUN_STDOUT);
}

#[test]
fn a_user_written_scalar_ok_result_slot_is_rebound_in_a_loop() {
    renders_and_agrees("user_slot", USER_SLOT, USER_SLOT_STDOUT);
}

#[test]
fn a_scalar_ok_result_tail_call_with_scalar_arguments_runs_in_constant_stack() {
    renders_and_agrees("deep", DEEP, DEEP_STDOUT);
}
