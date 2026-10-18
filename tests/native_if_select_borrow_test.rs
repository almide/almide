//! A read-only `if` that selects between two live list or String values
//! borrows the chosen one on native instead of copying it (#3532).
//!
//! The v1 render realized each arm's `Dup` as `.clone()` (`.to_vec()` from a
//! borrowed param), and the let-bound form was lifted into a helper that took
//! both lists and returned an owned copy, so a loop that picked one of two
//! lists per iteration copied a whole list every time: O(n) per `list.len`
//! on native, flat on wasm. The pins below hold the emitted shape of the
//! issue's program, and run programs where a source is written while the
//! selection is still live, which must keep their copy and their output.

use std::process::Command;

/// The issue's program: both selections are only read, and both sources
/// outlive them.
const SELECT: &str = r#"effect fn main() -> Unit = {
  let x: List[Int] = list.range(0, 20000)
  let x2: List[Int] = list.range(0, 20000)
  var acc = 0
  var i = 0
  while i < 20000 {
    let y: List[Int] = if i % 2 == 0 then x else x2
    let r = list.len(y)
    acc = acc + r
    acc = acc + list.len(if i % 2 == 0 then x else x2)
    i = i + 1
  }
  println("${acc + list.len(x) + list.len(x2)}")
}
"#;

/// A String selection, and a selection inside a fn whose sources are params.
const SELECT_STR_AND_PARAMS: &str = r#"fn pick(c: Bool, a: List[Int], b: List[Int]) -> Int = {
  var n = 0
  var i = 0
  while i < 3 {
    let y = if c then a else b
    n = n + list.len(y)
    i = i + 1
  }
  n
}

effect fn main() -> Unit = {
  let s = "alpha"
  let t = "be"
  var total = 0
  var i = 0
  while i < 4 {
    let w = if i % 2 == 0 then s else t
    total = total + string.len(w)
    i = i + 1
  }
  println(int.to_string(total))
  println(int.to_string(pick(true, [1, 2, 3], [4]) + pick(false, [1, 2, 3], [4])))
}
"#;

/// A source is rebound and written in place while the selection is live:
/// the selection must still read the value it chose.
const SOURCE_WRITTEN_WHILE_LIVE: &str = r#"effect fn main() -> Unit = {
  var x = list.range(0, 3)
  let x2 = list.range(0, 2)
  var i = 0
  while i < 3 {
    let y = if i % 2 == 0 then x else x2
    x = list.range(0, 4 + i)
    println(int.to_string(list.len(y)))
    i = i + 1
  }
  var z = list.range(7, 9)
  var j = 0
  while j < 2 {
    let w = if j == 0 then z else x2
    z[0] = 100
    println(int.to_string(w[0]))
    j = j + 1
  }
}
"#;

/// The body of `fn main` in a render.
fn main_body(rust: &str) -> &str {
    let start = rust.find("fn main()").unwrap_or_else(|| panic!("no fn main:\n{rust}"));
    let rest = &rust[start..];
    let end = rest.find("\n}\n").map_or(rest.len(), |e| e + 3);
    &rest[..end]
}

fn render(src: &str) -> String {
    almide_mir::pipeline::try_render_rust_source(src)
        .unwrap_or_else(|e| panic!("the v1 native render walled: {e:?}"))
}

/// `(stdout, stderr)` of `almide run` on `target`, with the route note on.
fn run(program: &str, target: &str) -> (String, String) {
    let dir = tempfile::tempdir().expect("tempdir");
    let source = dir.path().join("main.almd");
    std::fs::write(&source, program).expect("source");
    let out = Command::new(env!("CARGO_BIN_EXE_almide"))
        .env("ALMIDE_VERIFIED_DEBUG", "1")
        .args(["run", source.to_str().expect("path"), "--target", target])
        .output()
        .expect("spawn almide run");
    let stderr = String::from_utf8_lossy(&out.stderr).to_string();
    assert!(out.status.success(), "{target} run:\n{stderr}");
    (String::from_utf8_lossy(&out.stdout).to_string(), stderr)
}

fn same_on_both_legs(program: &str, expected: &str) {
    let (native, note) = run(program, "rust");
    assert!(
        note.contains("native: v1 trust-spine render") && !note.contains("walled"),
        "the native route left the v1 trust spine:\n{note}"
    );
    assert_eq!(native, expected, "native output");
    assert_eq!(run(program, "wasm").0, expected, "wasm output");
}

#[test]
fn a_read_only_selection_of_live_lists_borrows_on_native() {
    let rust = render(SELECT);
    let main = main_body(&rust);
    assert!(!main.contains(".clone()") && !main.contains(".to_vec()"), "the selection copies a list:\n{main}");
    assert!(!rust.contains("__almd_lift_"), "the read-only selection was lifted into a copying helper:\n{rust}");
}

#[test]
fn a_selection_of_strings_or_params_borrows_on_native() {
    let rust = render(SELECT_STR_AND_PARAMS);
    for f in ["fn main()", "fn almd_pick("] {
        let start = rust.find(f).unwrap_or_else(|| panic!("no {f}:\n{rust}"));
        let body = &rust[start..];
        let body = &body[..body.find("\n}\n").map_or(body.len(), |e| e + 3)];
        assert!(
            !body.contains(".clone()") && !body.contains(".to_vec()") && !body.contains("AlmideString::from("),
            "{f}: the selection copies:\n{body}"
        );
    }
}

#[test]
fn a_selection_keeps_its_value_when_a_source_is_written_while_it_is_live() {
    same_on_both_legs(SOURCE_WRITTEN_WHILE_LIVE, "3\n2\n5\n7\n0\n");
}

#[test]
fn the_selections_answer_the_same_on_both_legs() {
    same_on_both_legs(SELECT, "800040000\n");
    same_on_both_legs(SELECT_STR_AND_PARAMS, "14\n12\n");
}
