//! A functional record update moves the field it rebuilds (#3404).
//!
//! `fn add(b: Box, i: Int) -> Box = { ...b, m: map.set(b.m, k, v), n: b.n + 1 }`
//! called as `b = add(b, i)` in a loop used to copy the map twice per call:
//! `add(b.clone(), i)` at the call site (the loop variable is not rebound by
//! the loop, so its read inside the loop always cloned) and
//! `map_set(b.m.clone(), …)` in the body (`b` has three occurrences, so the
//! field read was not its last use). 20 000 iterations took 22 s. Two rules
//! remove both copies, and each is pinned here by the emitted Rust:
//!
//! 1. **Reassignment moves** (`insert_clones_reassign`): in `x = e`, a single
//!    read of `x` in `e` outside every closure, loop and chain is `x`'s last
//!    use before the statement overwrites it, so it moves even inside a loop.
//! 2. **Partial move in an update** (`rewrite_spread`): when the update's base
//!    moves (owned, last use), a field the literal names and the initializers
//!    read exactly once moves out — Rust takes only the unnamed fields from
//!    the base, so `b.m` moved and `b.n` read after it is legal.
//!
//! The negative cells say where the copy is still required: `b` read after
//! the call, `b` read after the update, `b.m` read twice in the update, and a
//! reassignment that reads `b` twice. If one of the positive assertions fails,
//! one of the two rules is gone — fix the pass, don't relax the assertion.
//!
//! Skips cleanly when the `almide` binary is unavailable (CI builds it in the
//! build step; locally run `cargo build --release` first).

use std::path::Path;
use std::process::Command;

fn almide_bin() -> String {
    if let Ok(bin) = std::env::var("ALMIDE_BIN") {
        return bin;
    }
    let cargo_bin = Path::new(env!("CARGO_MANIFEST_DIR")).join("target/release/almide");
    if cargo_bin.exists() {
        return cargo_bin.to_str().unwrap().to_string();
    }
    "almide".to_string()
}

fn tool_available() -> bool {
    Command::new(almide_bin()).arg("--version").output().is_ok()
}

const PROGRAM: &str = r#"type Box = { m: Map[String, List[Int]], n: Int, tag: String }

fn add(b: Box, i: Int) -> Box = { ...b, m: map.set(b.m, int.to_string(i), [i]), n: b.n + 1 }

fn add_tag(b: Box, i: Int) -> Box = { ...b, m: map.set(b.m, int.to_string(i), [i]), tag: b.tag + "!" }

fn add_keep(b: Box, i: Int) -> (Box, Box) = {
  let c = { ...b, m: map.set(b.m, int.to_string(i), [i]), n: b.n + 1 }
  (c, b)
}

fn add_reread(b: Box, i: Int) -> Box = { ...b, m: map.set(b.m, int.to_string(i), [i]), n: map.len(b.m) }

fn both(a: Box, b: Box) -> Box = { ...a, n: a.n + b.n }

fn grow(n: Int) -> Box = {
  var b = Box { m: [:], n: 0, tag: "t" }
  var i = 0
  while i < n {
    b = add(b, i)
    i = i + 1
  }
  b
}

fn keep_old(b: Box) -> Int = {
  let c = add(b, 1)
  c.n + b.n
}

fn twice(n: Int) -> Box = {
  var b = Box { m: [:], n: 1, tag: "t" }
  for _ in 0..<n {
    b = both(b, b)
  }
  b
}

fn main() -> Unit = {
  println(int.to_string(grow(3).n + keep_old(grow(1)) + twice(2).n))
  println(int.to_string(add_tag(grow(1), 2).n + add_keep(grow(1), 2).0.n + add_reread(grow(1), 2).n))
}
"#;

fn emitted() -> String {
    let dir = std::env::temp_dir().join(format!("almide-record-update-move-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let src = dir.join("prog.almd");
    std::fs::write(&src, PROGRAM).unwrap();
    let output = Command::new(almide_bin())
        .args([src.to_str().unwrap(), "--target", "rust"])
        .output()
        .expect("failed to spawn almide");
    std::fs::remove_dir_all(&dir).ok();
    assert!(output.status.success(), "--target rust emit failed:\n{}", String::from_utf8_lossy(&output.stderr));
    String::from_utf8_lossy(&output.stdout).to_string()
}

/// The body of `pub fn <name>(` up to the first column-zero `}`.
fn body<'a>(rust: &'a str, name: &str) -> &'a str {
    let needle = format!("pub fn {name}(");
    let start = rust.find(&needle).unwrap_or_else(|| panic!("emitted Rust has no `{needle}`"));
    let rest = &rust[start..];
    let end = rest.find("\n}").map(|i| i + 2).unwrap_or(rest.len());
    &rest[..end]
}

#[test]
fn the_update_moves_the_rebuilt_field_and_the_loop_moves_the_box() {
    if !tool_available() { return; }
    let rust = emitted();
    let add = body(&rust, "add");
    assert!(add.contains("almide_rt_map_set(b.m,"), "the map field must move into map.set:\n{add}");
    assert!(!add.contains(".clone()"), "nothing in `add` needs a copy:\n{add}");
    let add_tag = body(&rust, "add_tag");
    assert!(!add_tag.contains(".clone()"), "both rebuilt fields move:\n{add_tag}");
    let grow = body(&rust, "grow");
    assert!(grow.contains("b = add(b, i)"), "the reassignment must move `b`:\n{grow}");
    assert!(!grow.contains("b.clone()"), "no copy of `b` in the growth loop:\n{grow}");
}

#[test]
fn the_copy_stays_where_the_old_value_is_still_read() {
    if !tool_available() { return; }
    let rust = emitted();
    let keep_old = body(&rust, "keep_old");
    assert!(keep_old.contains("add(b.clone(), 1i64)"), "`b` is read after the call:\n{keep_old}");
    let add_keep = body(&rust, "add_keep");
    assert!(add_keep.contains("b.m.clone()"), "`b` is returned after the update:\n{add_keep}");
    let add_reread = body(&rust, "add_reread");
    assert!(add_reread.contains("b.m.clone()"), "`b.m` is read again after map.set:\n{add_reread}");
    let twice = body(&rust, "twice");
    assert!(!twice.contains("b = both(b, "), "a reassignment reading `b` twice must not move it:\n{twice}");
}

/// Rule 1 on a String accumulator: `s = s + piece` in a loop body moves `s`
/// into the concat, which extends the owned buffer in place. Through 0.66.0
/// it emitted `AlmideConcat::concat(s.clone(), …)`, copying the whole
/// accumulated string per iteration: 200 000 appends took 0.69 s and doubled
/// N cost about 3×. `s = s + s` reads `s` twice and must keep its copy.
const STRING_PROGRAM: &str = r#"fn in_for(n: Int) -> String = { var s = ""; for i in 0..<n { s = s + "ab" }; s }
fn in_while(n: Int) -> String = { var s = ""; var i = 0; while i < n { s = s + "ab"; i = i + 1 }; s }
fn nested(n: Int) -> String = { var s = ""; for i in 0..<n { for j in 0..<2 { s = s + "c" } }; s }
effect fn in_effect(n: Int) -> String = { var s = ""; for i in 0..<n { s = s + int.to_string(i) }; s }
fn doubled(n: Int) -> String = { var s = "x"; for i in 0..<n { s = s + s }; s }
effect fn main() -> Unit = {
  println(in_for(2) + in_while(1) + nested(2) + in_effect(3)! + doubled(2))
}
"#;

#[test]
fn a_string_accumulator_moves_into_its_own_concat() {
    if !tool_available() { return; }
    let dir = std::env::temp_dir().join(format!("almide-string-append-move-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let src = dir.join("prog.almd");
    std::fs::write(&src, STRING_PROGRAM).unwrap();
    let emit = Command::new(almide_bin()).args([src.to_str().unwrap(), "--target", "rust"]).output().expect("spawn almide");
    let run = Command::new(almide_bin()).args(["run", src.to_str().unwrap()]).output().expect("spawn almide run");
    std::fs::remove_dir_all(&dir).ok();
    assert!(emit.status.success(), "--target rust emit failed:\n{}", String::from_utf8_lossy(&emit.stderr));
    let rust = String::from_utf8_lossy(&emit.stdout).to_string();
    for name in ["in_for", "in_while", "nested", "in_effect"] {
        let b = body(&rust, name);
        assert!(b.contains("s = AlmideConcat::concat(s, "), "`{name}` must move `s` into the concat:\n{b}");
        assert!(!b.contains("s.clone()"), "no copy of the accumulator in `{name}`:\n{b}");
    }
    let doubled = body(&rust, "doubled");
    assert!(doubled.contains("s.clone()"), "`s = s + s` reads `s` twice and keeps one copy:\n{doubled}");
    assert!(run.status.success(), "run failed:\n{}", String::from_utf8_lossy(&run.stderr));
    assert_eq!(String::from_utf8_lossy(&run.stdout), "abababcccc012xxxx\n");
}

#[test]
fn the_program_prints_what_value_semantics_say() {
    if !tool_available() { return; }
    let dir = std::env::temp_dir().join(format!("almide-record-update-move-run-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let src = dir.join("prog.almd");
    std::fs::write(&src, PROGRAM).unwrap();
    let output = Command::new(almide_bin()).args(["run", src.to_str().unwrap()]).output().expect("spawn almide run");
    std::fs::remove_dir_all(&dir).ok();
    assert!(output.status.success(), "run failed:\n{}", String::from_utf8_lossy(&output.stderr));
    // grow(3).n = 3; keep_old(grow(1)) = 2 + 1; twice(2).n = 4  → 10
    // add_tag(grow(1), 2).n = 1; add_keep(..).0.n = 2; add_reread(..).n = map.len of the OLD map = 1 → 4
    assert_eq!(String::from_utf8_lossy(&output.stdout), "10\n4\n");
}
