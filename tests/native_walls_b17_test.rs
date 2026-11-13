//! v1 walls, round 17 (family B): shapes the native render refused, now rendered,
//! each checked against its expected output on the native render and on wasm.

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

// A `!` inside a pushed heap element, in a `for` nested in a `while` arm:
// the bang is bound on its own first, the push appends the bound value.
const NESTED_PUSH_BANG: &str = r#"
type AExpr = | EVar(String) | EApp(AExpr, List[AExpr]) | EArrow(AExpr, AExpr) | EHole
fn at(ts: List[String], i: Int) -> String = list.get(ts, i) ?? ""
fn close_at(ts: List[String], i: Int) -> Int = {
  var d = 0
  var j = i
  var r = -1
  while j < list.len(ts) and r < 0 {
    let t = at(ts, j)
    if t == "(" then { d = d + 1 } else if t == ")" then { d = d - 1 } else ()
    if d == 0 and t == ")" then { r = j } else ()
    j = j + 1
  }
  r
}
fn parse_type(ts: List[String]) -> AExpr! = {
  var depth = 0
  var arrow = -1
  for (i, t) in list.enumerate(ts) {
    if t == "(" then {
      depth = depth + 1
    } else if t == ")" then {
      depth = depth - 1
    } else if t == "=>" and depth == 0 and arrow < 0 then {
      arrow = i
    } else ()
  }
  if arrow >= 0 then ok(EArrow(parse_type(list.take(ts, arrow))!, parse_type(list.drop(ts, arrow + 1))!))
  else {
    var cur: List[AExpr] = []
    var i = 0
    while i < list.len(ts) {
      let t = at(ts, i)
      if t == "(" then {
        let e = close_at(ts, i)
        guard e > i else err("unclosed ( in a type")
        var parts: List[List[String]] = []
        var part: List[String] = []
        var d = 0
        for u in list.slice(ts, i + 1, e) {
          if u == "(" then {
            d = d + 1
          } else if u == ")" then {
            d = d - 1
          } else ()
          if u == "," and d == 0 then {
            list.push(parts, part)
            part = []
          } else list.push(part, u)
        }
        list.push(parts, part)
        var parsed: List[AExpr] = []
        for q in parts {
          list.push(parsed, parse_type(q)!)
        }
        cur = parsed
        i = e + 1
      } else {
        let base = EVar(t)
        cur = [if list.is_empty(cur) then base else EApp(base, cur)]
        i = i + 1
      }
    }
    guard list.len(cur) == 1 else err("cannot read this type")
    ok(list.first(cur) ?? EHole)
  }
}
fn show(e: AExpr) -> String = match e {
  EVar(s) => s,
  EApp(f, xs) => show(f) + "[" + list.join(xs |> list.map((x) => show(x)), ",") + "]",
  EArrow(a, b) => "(" + show(a) + "->" + show(b) + ")",
  EHole => "_",
}
effect fn main() -> Unit = {
  for s in ["a", "a list", "( a , b ) prod", "a => b list => c", "( a", "a b", "( a => b , c ) fn list"] {
    match parse_type(string.split(s, " ")) { ok(e) => println(show(e)), err(m) => println("E " + m) }
  }
}
"#;

#[test]
fn a_bang_in_a_pushed_heap_element_inside_nested_loops_is_bound_first() {
    both(
        NESTED_PUSH_BANG,
        "a\nlist[a]\nprod[a,b]\n(a->(list[b]->c))\nE unclosed ( in a type\nb[a]\nlist[fn[(a->b),c]]\n",
    );
}

// Two bangs as the arms of a bound `if`, in a loop: one `!` over the branch.
const BRANCH_OF_BANGS: &str = r#"
type LexError = { at: Int, msg: String }
fn scan(s: String) -> Result[List[String], LexError] =
  if s == "" then err(LexError { at: 0, msg: "empty" }) else ok(string.split(s, " "))
fn line_scan(s: String) -> Result[List[String], LexError] =
  if s == "" then err(LexError { at: 1, msg: "none" }) else ok(string.split(s, ","))
fn count(texts: List[String], lines: Bool) -> Int! = {
  var n = 0
  for t in texts {
    let toks = if lines then line_scan(t)! else scan(t)!
    n = n + list.len(toks)
  }
  ok(n)
}
effect fn main() -> Unit = {
  for (ts, l) in [(["a b", "c"], false), (["c,d,e"], true), (["x", ""], true)] {
    match count(ts, l) {
      ok(n) => println(int.to_string(n)),
      err(e) => println("E " + e),
    }
  }
}
"#;

#[test]
fn a_bound_branch_of_two_bangs_propagates_once() {
    both(BRANCH_OF_BANGS, "3\n3\nE LexError { at: 1, msg: \"none\" }\n");
}
