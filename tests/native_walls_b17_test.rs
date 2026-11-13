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

// A `??` arm with a record payload and a computed-call operand: lowered as the
// match it means, the fallback only on the absent side.
const UNWRAP_OR_ARM: &str = r#"
type T = { name: String, xs: List[Int] }
fn dflt() -> T = T { name: "default", xs: [1] }
fn builtin() -> List[T] = [T { name: "a", xs: [2, 3] }, T { name: "b", xs: [] }]
fn named(n: String) -> Option[T] = list.find(builtin(), (t) => t.name == n)
fn apply(t: T, k: Int) -> T = { ...t, xs: t.xs + [k] }
fn pick(n: String, dark: Bool, load: (String) -> Option[T]) -> T = {
  let name = if dark then n + "!" else n
  let base = match named(name) {
    some(t) => t,
    none => load(name) ?? dflt(),
  }
  apply(base, 9)
}
effect fn main() -> Unit = {
  let none_loaded = (n: String) => {
    let missing: Option[T] = none
    missing
  }
  let some_loaded = (n: String) => some(T { name: "file:" + n, xs: [] })
  for (n, d) in [("a", false), ("b", true), ("zz", false)] {
    let t = pick(n, d, none_loaded)
    let u = pick(n, d, some_loaded)
    println(t.name + " " + int.to_string(list.len(t.xs)) + " / " + u.name)
  }
}
"#;

#[test]
fn a_record_unwrap_or_arm_lowers_as_its_match() {
    both(UNWRAP_OR_ARM, "a 3 / a\ndefault 2 / file:b!\ndefault 2 / file:zz\n");
}

// A `var` copied from an `Option[record]` field, then matched and reassigned in
// a `while`: the copy owns its record through the option.
const OPTION_FIELD_SLOT: &str = r#"
type Pos = { x: Int, y: Int, z: Int }
type PathNode = { pos: Pos, g: Float, h: Float, parent: Option[Pos] }
fn pos_eq(a: Pos, b: Pos) -> Bool = a.x == b.x and a.y == b.y and a.z == b.z
fn reconstruct(goal_node: PathNode, closed_nodes: List[PathNode]) -> List[Pos] = {
  var path = [goal_node.pos]
  var current_parent = goal_node.parent
  var safety = 0
  let max_safety = list.len(closed_nodes) + 1
  while safety < max_safety {
    safety = safety + 1
    match current_parent {
      none => { safety = max_safety },
      some(parent_pos) => {
        path = [parent_pos] + path
        let parent_node = list.find(closed_nodes, (n) => pos_eq(n.pos, parent_pos))
        match parent_node {
          some(pn) => { current_parent = pn.parent },
          none => { safety = max_safety },
        }
      },
    }
  }
  path
}
fn p(x: Int) -> Pos = Pos { x: x, y: 0, z: 1 }
effect fn main() -> Unit = {
  let nodes = [PathNode { pos: p(0), g: 0.0, h: 0.0, parent: none }, PathNode { pos: p(1), g: 0.0, h: 0.0, parent: some(p(0)) }, PathNode { pos: p(2), g: 0.0, h: 0.0, parent: some(p(1)) }]
  let goal = PathNode { pos: p(3), g: 0.0, h: 0.0, parent: some(p(2)) }
  println(list.join(reconstruct(goal, nodes) |> list.map((q) => int.to_string(q.x)), ","))
}
"#;

#[test]
fn a_var_copy_of_an_option_record_field_is_a_loop_slot() {
    both(OPTION_FIELD_SLOT, "0,1,2,3\n");
}

// The same with a record that holds a String: its drop recurses.
const OPTION_FIELD_SLOT_HEAP: &str = r#"
type Pos = { x: Int, tag: String }
type Node = { pos: Pos, parent: Option[Pos] }
fn p(x: Int) -> Pos = Pos { x: x, tag: "t" + int.to_string(x) }
fn next(q: Pos) -> Option[Pos] = if q.x > 0 then some(p(q.x - 1)) else none
fn walk(n0: Node) -> Int = {
  var cur = n0.parent
  var n = 0
  var go = true
  while go {
    match cur {
      none => { go = false },
      some(q) => {
        n = n + q.x + string.len(q.tag)
        cur = next(q)
      },
    }
  }
  n
}
effect fn main() -> Unit = {
  println(int.to_string(walk(Node { pos: p(9), parent: some(p(4)) })))
}
"#;

#[test]
fn a_var_copy_of_an_option_heap_record_field_is_a_loop_slot() {
    both(OPTION_FIELD_SLOT_HEAP, "20\n");
}
