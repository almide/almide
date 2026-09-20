//! #2316: a local bound inside a branch inside a loop is MOVED at its last
//! use, not cloned.
//!
//! The four cells differ only in whether a loop and a branch enclose the same
//! statements, and three of them already passed — they are here as controls,
//! because the defect was not "clones in loops" but the conjunction, and a fix
//! that moved everything would look identical on the failing cell alone.
//!
//! The C3 `clone-at-last-use` certificate cannot see this: it skips any use
//! with `in_loop` (`certify_ownership.rs`), which is exactly this shape. So
//! this test is the gate, not the certifier.
use std::process::Command;

const PROGRAM: &str = r#"
type Box = { size: Int, kids: List[Int] }
type Holder = { sizes: List[Int], kids: List[Box] }
fn make(n: Int) -> Box = Box { size: n, kids: [n, n, n] }

fn one_use_branch_in_loop(xs: List[Int], mut h: Holder) -> Unit = {
  for x in xs { if x >= 0 then { let kid = make(x)
      list.push(h.kids, kid) } else () }
  ()
}
fn two_uses_branch_in_loop(xs: List[Int], mut h: Holder) -> Unit = {
  for x in xs { if x >= 0 then { let kid = make(x)
      list.push(h.sizes, kid.size)
      list.push(h.kids, kid) } else () }
  ()
}
fn two_uses_loop_only(xs: List[Int], mut h: Holder) -> Unit = {
  for x in xs { let kid = make(x)
    list.push(h.sizes, kid.size)
    list.push(h.kids, kid) }
  ()
}
fn two_uses_branch_only(x: Int, mut h: Holder) -> Unit = {
  if x >= 0 then { let kid = make(x)
    list.push(h.sizes, kid.size)
    list.push(h.kids, kid) } else ()
}
fn main() -> Unit = {
  var h = Holder { sizes: [], kids: [] }
  one_use_branch_in_loop([1], h)
  two_uses_branch_in_loop([2], h)
  two_uses_loop_only([3], h)
  two_uses_branch_only(4, h)
  println(int.to_string(list.len(h.kids)))
}
"#;

fn emitted_rust() -> String {
    let bin =
        std::env::var("ALMIDE_BIN").unwrap_or_else(|_| env!("CARGO_BIN_EXE_almide").to_string());
    let directory = tempfile::tempdir().unwrap();
    let source = directory.path().join("lastuse.almd");
    std::fs::write(&source, PROGRAM).unwrap();
    let out = Command::new(&bin)
        .arg(&source)
        .args(["--target", "rust"])
        .output()
        .unwrap();
    assert!(
        out.status.success(),
        "emit failed: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    String::from_utf8_lossy(&out.stdout).to_string()
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
fn a_local_in_a_branch_in_a_loop_is_moved_at_its_last_use() {
    let rust = emitted_rust();
    for name in [
        "one_use_branch_in_loop",
        "two_uses_branch_in_loop",
        "two_uses_loop_only",
        "two_uses_branch_only",
    ] {
        let body = body_of(&rust, name);
        assert_eq!(
            body.matches("kid.clone()").count(),
            0,
            "`kid` is cloned at its last use in {name} — the value is rebound on \
             every iteration that reaches it, so the last use is a move:\n{body}"
        );
        // The push must still receive the value, not vanish: a fix that
        // dropped the argument would also show zero clones.
        assert!(
            body.contains("h.kids, kid)"),
            "{name} must still push the owned value:\n{body}"
        );
    }
}

/// The answer, not just the shape — a move that is wrong is worse than a
/// clone that is wasteful.
#[test]
fn the_program_still_answers_correctly() {
    let bin =
        std::env::var("ALMIDE_BIN").unwrap_or_else(|_| env!("CARGO_BIN_EXE_almide").to_string());
    let directory = tempfile::tempdir().unwrap();
    let source = directory.path().join("lastuse.almd");
    std::fs::write(&source, PROGRAM).unwrap();
    let out = Command::new(&bin).arg("run").arg(&source).output().unwrap();
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    assert_eq!(String::from_utf8_lossy(&out.stdout).trim(), "4");
}
