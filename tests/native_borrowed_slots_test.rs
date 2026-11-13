//! A loop-converted self tail call whose heap slot only ever re-points at its
//! own payloads keeps borrowing its parameter on the native render: the
//! caller hands `&p` over (no copy of the whole tree per call, as the
//! standard codegen's `p: &Term`) and each round re-points the reference.
//! A slot whose value leaves the frame, or that takes a value from elsewhere,
//! keeps its owned copy.

fn render(src: &str) -> Result<String, String> {
    let src = src.to_string();
    std::thread::Builder::new()
        .stack_size(256 << 20)
        .spawn(move || almide_mir::pipeline::try_render_rust_source(&src).map_err(|e| e.to_string()))
        .unwrap()
        .join()
        .unwrap()
}

/// The rendered header line of the fn whose name contains `fn_name`.
fn header_of(out: &str, fn_name: &str) -> String {
    out.lines()
        .find(|l| l.trim_start().starts_with("fn ") && l.contains(fn_name))
        .unwrap_or_else(|| panic!("{fn_name} is rendered:\n{out}"))
        .to_string()
}

/// The type the header gives parameter `k`.
fn param_ty(header: &str, k: usize) -> String {
    let inner = &header[header.find('(').unwrap() + 1..header.rfind(')').unwrap()];
    inner.split(", ").nth(k).unwrap().split(": ").nth(1).unwrap().to_string()
}

fn run_native(src: &str) -> String {
    let dir = tempfile::tempdir().expect("scratch");
    let file = dir.path().join("p.almd");
    std::fs::write(&file, src).expect("source");
    let out = std::process::Command::new(env!("CARGO_BIN_EXE_almide"))
        .arg("run")
        .arg(&file)
        .env("ALMIDE_CERTIFY_OWNERSHIP", "fail")
        .output()
        .expect("almide run");
    assert!(out.status.success(), "{}", String::from_utf8_lossy(&out.stderr));
    String::from_utf8_lossy(&out.stdout).into_owned()
}

const MATCHES: &str = r#"
type Term =
  | Var(Int)
  | App(Term, Term)
  | Const(String)

fn matches(p: Term, t: Term, b: Map[Int, Term]) -> Map[Int, Term]? = match p {
  Var(j) => match map.get(b, j) {
    some(prev) => if prev == t then some(b) else none,
    none => some(map.set(b, j, t)),
  },
  App(f, x) => match t {
    App(g, y) => match matches(f, g, b) {
      some(b2) => matches(x, y, b2),
      none => none,
    },
    _ => none,
  },
  other => if other == t then some(b) else none,
}

fn spine(k: Int, t: Term) -> Term = if k == 0 then t else spine(k - 1, App(Const("f"), t))

effect fn main() -> Unit = {
  let p = spine(20000, Var(0))
  let t = spine(20000, Const("c"))
  let u = spine(20000, Const("d"))
  var total = 0
  for i in 0..<3 {
    total = total + map.len(matches(p, t, map.new()) ?? map.new())
    total = total + map.len(matches(App(Var(1), Var(1)), App(Const("a"), Const("b")), map.new()) ?? map.new())
    total = total + map.len(matches(App(Var(1), Var(1)), App(Const("a"), Const("a")), map.new()) ?? map.new())
  }
  println(int.to_string(total))
  println(match matches(p, u, map.new()) { some(b) => match map.get(b, 0) { some(Const(s)) => s, _ => "?" }, none => "none" })
}
"#;

#[test]
fn a_slot_that_only_re_points_at_its_own_payloads_borrows_its_parameter() {
    let out = render(MATCHES).unwrap_or_else(|e| panic!("matches walls: {e}"));
    let header = header_of(&out, "matches");
    assert!(param_ty(&header, 0).starts_with('&'), "the pattern stays borrowed: {header}");
    assert!(param_ty(&header, 1).starts_with('&'), "the term stays borrowed: {header}");
    assert_eq!(run_native(MATCHES), "6\nd\n");
}

#[test]
fn a_slot_whose_value_leaves_the_frame_keeps_its_copy() {
    let src = r#"
type Tree =
  | Leaf(Int)
  | Node(Tree, Tree)

fn rightmost(t: Tree) -> Tree = match t {
  Node(_, r) => rightmost(r),
  _ => t,
}

fn spine(k: Int, t: Tree) -> Tree = if k == 0 then t else spine(k - 1, Node(Leaf(k), t))

effect fn main() -> Unit = {
  let t = spine(20000, Leaf(7))
  println(match rightmost(t) { Leaf(v) => int.to_string(v), _ => "?" })
  println(match t { Node(Leaf(v), _) => int.to_string(v), _ => "?" })
}
"#;
    let out = render(src).unwrap_or_else(|e| panic!("rightmost walls: {e}"));
    let header = header_of(&out, "rightmost");
    assert!(!param_ty(&header, 0).starts_with('&'), "a returned slot owns its value: {header}");
    // `spine` wraps last with `Leaf(1)`: the outermost node.
    assert_eq!(run_native(src), "7\n1\n");
}

#[test]
fn a_slot_that_takes_a_value_from_elsewhere_keeps_its_copy() {
    let src = r#"
type Level =
  | LZero
  | LSucc(Level)

fn peel(l: Level) -> Level = match l {
  LSucc(x) => x,
  _ => l,
}

fn depth(a: Level, n: Int) -> Int = match a {
  LSucc(_) => depth(peel(a), n + 1),
  LZero => n,
}

fn deep(n: Int, l: Level) -> Level = if n == 0 then l else deep(n - 1, LSucc(l))

effect fn main() -> Unit = println(int.to_string(depth(deep(20000, LZero), 0)))
"#;
    let out = render(src).unwrap_or_else(|e| panic!("depth walls: {e}"));
    let header = header_of(&out, "depth");
    assert!(!param_ty(&header, 0).starts_with('&'), "a slot fed by a call result owns it: {header}");
    assert_eq!(run_native(src), "20000\n");
}
