//! #2582: a pattern binder off a `Box`'d variant field, passed to a callee
//! that BORROWS its parameter at a non-last use, is borrowed — never cloned
//! first. The clone deep-copied the whole subtree per call (`depth(&*(l.clone()))`),
//! 466,254 native allocations against 1,844 on wasm for the issue's program.
//!
//! The defect was not specific to an `if` condition: any non-last use of the
//! binder took the clone, and the borrow could not strip it because it sat
//! under the box deref (`Borrow(Deref(Clone(l)))`). So the cells are the
//! positions a non-last use can take — an `if` condition, a `let` initializer,
//! an `and` operand, a binop operand, a `match` subject, a `while` condition —
//! plus a control where the binder IS moved by a sibling argument after the
//! borrow, which must keep its (move-side) clone and still build.
use std::process::Command;

const PROGRAM: &str = r#"
type Tree = | Leaf | Node(Tree, Int, Tree)
fn insert(t: Tree, x: Int) -> Tree =
  match t {
    Leaf => Node(Leaf, x, Leaf),
    Node(l, v, r) => if x < v then Node(insert(l, x), v, r) else Node(l, v, insert(r, x)),
  }
fn build(xs: List[Int]) -> Tree = xs |> list.fold(Leaf, (t, x) => insert(t, x))
fn depth(t: Tree) -> Int =
  match t { Leaf => 0, Node(l, _, r) => 1 + (if depth(l) > depth(r) then depth(l) else depth(r)) }
fn in_let(t: Tree) -> Int =
  match t { Leaf => 0, Node(l, _, r) => {
    let a = depth(l)
    let b = depth(r)
    a + b + depth(l) + depth(r) } }
fn in_and(t: Tree) -> Bool =
  match t { Leaf => false, Node(l, _, _) => depth(l) > 0 and depth(l) < 100 }
fn in_binop(t: Tree) -> Int =
  match t { Leaf => 0, Node(l, _, r) => depth(l) + depth(l) + depth(r) }
fn in_subject(t: Tree) -> Int =
  match t { Leaf => 0, Node(l, _, r) => match depth(l) { 0 => depth(r), _ => depth(l) } }
fn in_while(t: Tree) -> Int =
  match t { Leaf => 0, Node(l, _, _) => {
    var i = 0
    while i < depth(l) { i = i + 1 }
    i + depth(l) } }
fn keep(t: Tree, a: Int) -> Tree = if a > 100 then Leaf else t
fn moved_after(t: Tree) -> Tree = match t { Leaf => Leaf, Node(l, _, _) => keep(l, depth(l)) }
effect fn main() -> Unit = {
  let n = 200
  let t = build(list.range(0, n) |> list.map((i) => (i * 7919 + 100) % n))
  println(int.to_string(depth(t)))
  println(int.to_string(in_let(t)))
  println(if in_and(t) then "yes" else "no")
  println(int.to_string(in_binop(t)))
  println(int.to_string(in_subject(t)))
  println(int.to_string(in_while(t)))
  println(int.to_string(depth(moved_after(t))))
}
"#;

const BORROWING: &[&str] = &["depth", "in_let", "in_and", "in_binop", "in_subject", "in_while"];

fn almide() -> String {
    std::env::var("ALMIDE_BIN").unwrap_or_else(|_| env!("CARGO_BIN_EXE_almide").to_string())
}

fn write_program() -> (tempfile::TempDir, std::path::PathBuf) {
    let directory = tempfile::tempdir().unwrap();
    let source = directory.path().join("box_binder.almd");
    std::fs::write(&source, PROGRAM).unwrap();
    (directory, source)
}

fn body_of<'a>(rust: &'a str, name: &str) -> &'a str {
    let start = rust
        .find(&format!("fn {name}("))
        .unwrap_or_else(|| panic!("no fn {name} in the emitted Rust"));
    let rest = &rust[start..];
    let end = rest.find("\n}").expect("unterminated fn") + 2;
    &rest[..end]
}

#[test]
fn a_box_binder_at_a_borrowing_call_is_not_cloned() {
    let (_dir, source) = write_program();
    let out = Command::new(almide()).arg(&source).args(["--target", "rust"]).output().unwrap();
    assert!(out.status.success(), "emit failed: {}", String::from_utf8_lossy(&out.stderr));
    let rust = String::from_utf8_lossy(&out.stdout).to_string();
    for name in BORROWING {
        let body = body_of(&rust, name);
        assert!(
            !body.contains(".clone()"),
            "`{name}` clones a box-bound binder to pass it to a borrowing callee (#2582):\n{body}"
        );
    }
}

#[test]
fn the_box_binder_program_runs_in_a_handful_of_allocations() {
    let (_dir, source) = write_program();
    let native = Command::new(almide())
        .arg("run")
        .arg(&source)
        .env("ALMIDE_ALLOC_COUNT", "1")
        .output()
        .unwrap();
    let stderr = String::from_utf8_lossy(&native.stderr).to_string();
    assert!(native.status.success(), "native run failed:\n{stderr}");
    assert_eq!(String::from_utf8_lossy(&native.stdout), "14\n50\nyes\n38\n13\n26\n13\n");
    let line = stderr
        .lines()
        .find(|l| l.starts_with("__ALMD_ALLOC"))
        .unwrap_or_else(|| panic!("no `__ALMD_ALLOC` line — the allocation lane did not arm:\n{stderr}"));
    let allocs: u64 = line
        .split_whitespace()
        .find_map(|f| f.strip_prefix("allocs="))
        .and_then(|n| n.parse().ok())
        .unwrap_or_else(|| panic!("unparsable allocation report: {line}"));
    // The output equals the wasm leg's (1,634 allocations there). Native is
    // ~3.5k with the fix; with the per-call subtree clone it was ~2.5 million.
    assert!(allocs < 20_000, "native made {allocs} allocations (~2.5M with the #2582 clone): {line}");
}
