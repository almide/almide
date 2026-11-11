//! A call argument that is a heap-result `if` is bound to a `let` before the
//! call, and so are the call's other arguments. When the call sits in a
//! conditionally evaluated position — an `if` arm, a `match` arm, the right
//! side of `and` / `or`, a `??` fallback — those binds have to stay in that
//! position. The certified native render moved them in front of the whole
//! branch (#4640), so:
//!
//! - a recursive call in the arm ran before the base-case test and never
//!   returned (`Error: stack overflow`);
//! - a sibling argument's effects ran for the arm that was not taken;
//! - a name bound by the arm's pattern was read outside the arm, which the
//!   render refused (`use of unbound var`).
//!
//! Each shape runs on the native leg under the ownership certificate with no
//! fallback to the standard codegen, and on wasm.
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

/// The recursive call is an argument beside the heap `if`: it runs only past the base case.
const RECURSIVE_ARM: &str = r##"type T =
  | Leaf
  | Node(T, List[Int])

fn build(n: Int) -> T = if n == 0 then Leaf else Node(build(n - 1), if n % 2 == 0 then [n] else [])

fn size(t: T) -> Int = match t {
  Leaf => 0,
  Node(l, xs) => 1 + size(l) + list.len(xs),
}

effect fn main() -> Unit = {
  println(int.to_string(size(build(5))))
  println(int.to_string(size(build(0))))
}
"##;

/// A printing argument beside the heap `if`, in an `if` arm, a `match` arm, the right of
/// `and` / `or`, a `??` fallback and a loop body: it prints only where its arm runs.
const EFFECT_ORDER: &str = r##"type T =
  | Leaf
  | Node(String, List[Int])

fn noisy(s: String) -> String = {
  println("eval " + s)
  s + "!"
}

fn in_if(c: Bool, n: Int) -> T =
  if c then Leaf else Node(noisy("if"), if n % 2 == 0 then [n] else [])

fn in_match(o: Option[Int]) -> T = match o {
  none => Leaf,
  some(n) => Node(noisy("match"), if n % 2 == 0 then [n] else []),
}

fn long(tag: String, xs: List[Int]) -> Bool = string.len(tag) + list.len(xs) > 3

fn in_and(c: Bool, n: Int) -> String =
  if c and long(noisy("and"), if n % 2 == 0 then [n] else []) then "yes" else "no"

fn in_or(c: Bool, n: Int) -> String =
  if c or long(noisy("or"), if n % 2 == 0 then [n] else []) then "yes" else "no"

fn count(tag: String, xs: List[Int]) -> String = tag + int.to_string(list.len(xs))

fn in_fallback(o: Option[String], n: Int) -> String =
  o ?? count(noisy("fallback"), if n % 2 == 0 then [n] else [])

fn in_loop(n: Int) -> String = {
  var out = ""
  for i in 0..<n {
    out = out + (if i % 2 == 0 then "." else count(noisy("loop"), if i > 2 then [i] else []))
  }
  out
}

fn show(t: T) -> String = match t {
  Leaf => "leaf",
  Node(s, xs) => s + int.to_string(list.len(xs)),
}

effect fn main() -> Unit = {
  println(show(in_if(true, 2)))
  println(show(in_if(false, 2)))
  println(show(in_match(none)))
  println(show(in_match(some(3))))
  println(in_and(false, 2))
  println(in_and(true, 2))
  println(in_or(true, 2))
  println(in_or(false, 3))
  println(in_fallback(some("held"), 2))
  println(in_fallback(none, 2))
  println(in_loop(4))
}
"##;

const EFFECT_ORDER_STDOUT: &str = "leaf\neval if\nif!1\nleaf\neval match\nmatch!0\nno\neval and\nyes\nyes\neval or\nno\nheld\neval fallback\nfallback!1\neval loop\neval loop\n.loop!0.loop!1\n";

/// The heap `if` and its sibling read names the arm's pattern binds.
const PATTERN_NAMES: &str = r##"type Raw =
  | RName(String)
  | RApp(Raw, Raw)
  | RLam(String, Raw, Raw)
  | RBracket(Raw)
  | RType

fn show(r: Raw) -> String = match r {
  RName(x) => x,
  RApp(f, a) => "(" + show(f) + " " + show(a) + ")",
  RLam(x, ty, b) => "fun " + x + ":" + show(ty) + "." + show(b),
  RBracket(b) => "[" + show(b) + "]",
  RType => "Type",
}

fn replace(r: Raw, x: String, by: Raw) -> Raw = match r {
  RName(y) => if y == x then by else r,
  RApp(f, a) => RApp(replace(f, x, by), replace(a, x, by)),
  RLam(y, ty, b) => RLam(y, replace(ty, x, by), if y == x then b else replace(b, x, by)),
  RBracket(b) => RBracket(replace(b, x, by)),
  other => other,
}

effect fn main() -> Unit = {
  let t = RLam("x", RType, RApp(RName("x"), RLam("y", RName("y"), RBracket(RName("y")))))
  println(show(replace(t, "y", RName("Z"))))
  println(show(replace(t, "x", RName("Z"))))
  println(show(replace(RApp(RName("y"), RType), "y", RName("Z"))))
}
"##;

const PATTERN_NAMES_STDOUT: &str = "fun x:Type.(x fun y:Z.[y])\nfun x:Type.(x fun y:y.[y])\n(Z Type)\n";

#[test]
fn a_recursive_call_beside_a_heap_if_argument_runs_only_in_its_arm() {
    renders_and_agrees("recursive_arm", RECURSIVE_ARM, "7\n0\n");
}

#[test]
fn an_argument_beside_a_heap_if_argument_has_its_effects_only_in_its_arm() {
    renders_and_agrees("effect_order", EFFECT_ORDER, EFFECT_ORDER_STDOUT);
}

#[test]
fn a_heap_if_argument_in_a_match_arm_reads_the_names_the_arm_binds() {
    renders_and_agrees("pattern_names", PATTERN_NAMES, PATTERN_NAMES_STDOUT);
}
