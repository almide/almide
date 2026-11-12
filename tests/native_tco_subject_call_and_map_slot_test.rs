//! #3550 on the certified native render: two heap self tail calls the loop
//! conversion used to decline, so the function walled onto the standard
//! codegen.
//!
//! - A tail call in an arm of a `match` whose subject is itself a self call
//!   (`match f(l, ..) { some(b2) => f(r, .., b2), none => none }`): the
//!   subject stays a real call inside the loop body, as a recursing base does.
//! - A result slot holding a `Map` (a `mut` record param with a map field
//!   returns it beside the result): its placeholder is an empty map.

fn render(src: &str) -> Result<String, String> {
    let src = src.to_string();
    std::thread::Builder::new()
        .stack_size(256 << 20)
        .spawn(move || almide_mir::pipeline::try_render_rust_source(&src).map_err(|e| e.to_string()))
        .unwrap()
        .join()
        .unwrap()
}

/// The body of the rendered fn whose name contains `fn_name`.
fn body_of(out: &str, fn_name: &str) -> (String, String) {
    let lines: Vec<&str> = out.lines().collect();
    let at = lines
        .iter()
        .position(|l| l.trim_start().starts_with("fn ") && l.contains(fn_name))
        .unwrap_or_else(|| panic!("{fn_name} is rendered:\n{out}"));
    let name = lines[at].trim_start()["fn ".len()..].split('(').next().unwrap().trim().to_string();
    let end = (at + 1..lines.len()).find(|&k| lines[k].starts_with("fn ")).unwrap_or(lines.len());
    (name, lines[at + 1..end].join("\n"))
}

#[test]
fn a_tail_call_in_an_arm_of_a_match_on_a_self_call_renders_as_a_loop() {
    let src = r#"
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

effect fn main() -> Unit = {
  println(int.to_string(map.len(matches(App(Var(0), Var(1)), App(Const("a"), Const("b")), map.new()) ?? map.new())))
}
"#;
    let out = render(src).unwrap_or_else(|e| panic!("the subject-call tail walls: {e}"));
    let (name, body) = body_of(&out, "matches");
    assert!(body.contains("while") || body.contains("loop"), "matches is a loop:\n{body}");
    // The subject's call stays: one real self call in the loop body.
    assert_eq!(body.matches(&format!("{name}(")).count(), 1, "only the subject calls itself:\n{body}");
}

#[test]
fn a_tail_call_returning_a_mut_record_with_a_map_renders_as_a_loop() {
    let src = r#"
type Level =
  | LZero
  | LSucc(Level)
  | LMeta(Int)

type Sol = { levels: Map[Int, Level] }

fn unify_level(a: Level, b: Level, mut sol: Sol) -> Bool = match a {
  LMeta(m) => {
    sol.levels = map.set(sol.levels, m, b)
    true
  },
  LSucc(x) => match b {
    LSucc(y) => unify_level(x, y, sol),
    _ => false,
  },
  LZero => b == LZero,
}

effect fn main() -> Unit = {
  var s: Sol = { levels: map.new() }
  println(bool.to_string(unify_level(LSucc(LSucc(LMeta(1))), LSucc(LSucc(LSucc(LZero))), s)))
  println(int.to_string(map.len(s.levels)))
}
"#;
    let out = render(src).unwrap_or_else(|e| panic!("the map result slot walls: {e}"));
    let (name, body) = body_of(&out, "unify_level");
    assert!(body.contains("while") || body.contains("loop"), "unify_level is a loop:\n{body}");
    assert!(!body.contains(&format!("{name}(")), "unify_level no longer calls itself:\n{body}");
}
