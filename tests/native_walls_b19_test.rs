//! v1 walls, round 19 (family B): shapes the native render refused or ran
//! differently, now rendered, each checked against its expected output on the
//! native render and on wasm.

fn render(src: &str) -> Result<String, String> {
    let src = src.to_string();
    std::thread::Builder::new()
        .stack_size(256 << 20)
        .spawn(move || almide_mir::pipeline::try_render_rust_source(&src).map_err(|e| e.to_string()))
        .unwrap()
        .join()
        .unwrap()
}

fn run(src: &str, target: Option<&str>) -> String {
    let dir = tempfile::tempdir().expect("scratch");
    let file = dir.path().join("p.almd");
    std::fs::write(&file, src).expect("source");
    let mut cmd = std::process::Command::new(env!("CARGO_BIN_EXE_almide"));
    cmd.arg("run").arg(&file).env("ALMIDE_CERTIFY_OWNERSHIP", "fail").env("ALMIDE_VERBOSE", "1");
    if let Some(t) = target {
        cmd.arg("--target").arg(t);
    }
    let out = cmd.output().expect("almide run");
    let err = String::from_utf8_lossy(&out.stderr);
    assert!(out.status.success(), "{err}");
    assert!(!err.contains("walled"), "{err}");
    String::from_utf8_lossy(&out.stdout).into_owned()
}

fn both(src: &str, want: &str) {
    render(src).expect("renders on the native render");
    assert_eq!(run(src, None), want);
    assert_eq!(run(src, Some("wasm")), want);
}

// A `??` over a tuple pushed straight into a loop-carried list: the element
// takes the route the `let p = pe ?? ("", "")` spelling already took.
const TUPLE_UNWRAP_OR_PUSH: &str = r#"
fn g(s: String) -> Result[(String, String), String] = if s == "" then err("bad") else ok((s, s + "!"))
fn f(xs: List[String]) -> Result[List[(String, String)], String] = {
  var out: List[(String, String)] = []
  for x in xs {
    let pe = g(x)
    guard result.is_ok(pe) else err("empty")
    list.push(out, pe ?? ("", ""))
  }
  ok(out)
}
fn main() -> Unit = { println("${f(["a", "b"])}"); println("${f(["a", "", "c"])}") }
"#;

// The premise accumulator of a statement parser: a split loop that resets
// its current list, then a `??` tuple element pushed after a guard.
const PREMISE_ACCUMULATOR: &str = r#"
type ITok = { text: String, quoted: Bool, line: Int }
type Th = { name: String, lhs: String, rhs: String, prems: List[(String, String)], line: Int }
fn at(xs: List[String], k: Int) -> String = list.get(xs, k) ?? ""
fn parse_eq(ts: List[String]) -> Result[(String, String), String] = match list.index_of(ts, "=") {
  some(k) => ok((list.take(ts, k) |> list.join(" "), list.drop(ts, k + 1) |> list.join(" "))),
  none => err("no eq"),
}
fn lemma(bw: List[String], body: List[ITok], unnamed: Int, line: Int) -> Result[Th, String] = {
  let stmt_at = match list.index_of(body |> list.map((t) => t.quoted), true) {
    some(k) => ok(k),
    none => err("line ${int.to_string(line)}: lemma: expected a statement in quotes"),
  }!
  let name = if stmt_at > 0 and at(bw, 0) != ":" then at(bw, 0) else "lemma_" + int.to_string(unnamed + 1)
  let stmt = list.get(body, stmt_at) ?? ITok { text: "", quoted: true, line: line }
  var parts: List[List[String]] = []
  var cur: List[String] = []
  var depth = 0
  for t in string.split(stmt.text, " ") {
    if t == "(" then { depth = depth + 1 } else if t == ")" then { depth = depth - 1 } else ()
    if t == "=>" and depth == 0 then {
      list.push(parts, cur)
      cur = []
    } else list.push(cur, t)
  }
  var prems: List[(String, String)] = []
  for ptoks in parts {
    let pe = parse_eq(ptoks)
    guard result.is_ok(pe) else err("line ${int.to_string(line)}: ${name}: a premise must be an equation")
    list.push(prems, pe ?? ("", ""))
  }
  let (lhs, rhs) = parse_eq(cur)!
  ok(Th { name: name, lhs: lhs, rhs: rhs, prems: prems, line: line })
}
fn show(r: Result[Th, String]) -> String = match r {
  ok(t) => t.name + ":" + (t.prems |> list.map(((a, b)) => a + "~" + b) |> list.join("|")) + "/" + t.lhs + "~" + t.rhs,
  err(e) => e,
}
fn main() -> Unit = {
  let body = [ITok { text: "x", quoted: false, line: 1 }, ITok { text: "a = b => c = d => e = f", quoted: true, line: 1 }]
  println(show(lemma(["foo"], body, 0, 1)))
  println(show(lemma(["foo"], [ITok { text: "a => e = f", quoted: true, line: 1 }], 0, 5)))
  println(show(lemma(["foo"], [ITok { text: "p q", quoted: true, line: 2 }], 2, 3)))
  println(show(lemma([":"], [], 4, 9)))
}
"#;

// A guard in an arm of a bound `match` / `if` returns its else from the fn;
// the bound value is not the else.
const BOUND_ARM_GUARD: &str = r#"
fn opt(x: Int) -> Option[Int] = if x == 0 then none else some(x)
fn fm(x: Int) -> Int = {
  let r = match opt(x) {
    none => 0,
    some(v) => {
      guard v > 0 else -1
      v * 2
    },
  }
  r + 100
}
fn fi(x: Int) -> Int = {
  var r = if x == 0 then 0 else {
    guard x > 0 else -1
    x * 2
  }
  r = r + 1
  r + 100
}
fn fs(x: Int) -> String = {
  let r = match opt(x) {
    none => "zero",
    some(v) => {
      guard v > 0 else "neg"
      "pos ${v}"
    },
  }
  r + "!"
}
fn bump(mut c: List[Int], x: Int) -> Int = {
  let r = match opt(x) {
    none => 0,
    some(v) => {
      guard v > 0 else -1
      list.push(c, v)
      v * 2
    },
  }
  r + list.len(c)
}
fn main() -> Unit = {
  println("${fm(5)} ${fm(0)} ${fm(-5)}")
  println("${fi(5)} ${fi(0)} ${fi(-5)}")
  println("${fs(5)} ${fs(0)} ${fs(-5)}")
  var c: List[Int] = []
  println("${bump(c, 5)} ${bump(c, 0)} ${bump(c, -5)} ${list.len(c)}")
}
"#;

#[test]
fn a_tuple_unwrap_or_pushed_in_a_loop_renders() {
    both(TUPLE_UNWRAP_OR_PUSH, "ok([(\"a\", \"a!\"), (\"b\", \"b!\")])\nerr(\"empty\")\n");
}

#[test]
fn a_premise_accumulator_renders() {
    both(
        PREMISE_ACCUMULATOR,
        "foo:a~b|c~d/e~f\nline 5: lemma_1: a premise must be an equation\nno eq\nline 9: lemma: expected a statement in quotes\n",
    );
}

#[test]
fn a_guard_in_a_bound_arm_returns_from_the_fn() {
    both(BOUND_ARM_GUARD, "110 100 -1\n111 101 -1\npos 5! zero! neg\n11 1 -1 1\n");
}
