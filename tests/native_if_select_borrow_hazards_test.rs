//! The shapes where a read-only selection's borrow (#3532) meets something
//! else that touches the chosen value: each program builds and answers alike
//! on the v1 native render, the walker (`--no-verified`) and wasm.
//!
//! - A String join read by `s = s + w`, where `w` may be `s` itself: the v1
//!   self-append window renders that rebind as `s.push_str(w)`, which writes
//!   `s` while a borrow of it is live (E0502), so such a join keeps its copy.
//! - A selection over a binder another selection bound by reference: the
//!   walker spells that binder's arm as the reference it already is, never
//!   `&y`, so both arms have one type whichever order the passes ran in.
//! - Selections of a record (read by a consumed field), an Option, a Map and
//!   a tuple — heap shapes other than List and String.

use std::process::Command;

/// `s = s + w` over a join that may choose `s`.
const SELF_APPEND: &str = r#"effect fn main() -> Unit = {
  var s = int.to_string(12)
  let t = int.to_string(345)
  var i = 0
  while i < 3 {
    let w = if i % 2 == 0 then s else t
    s = s + w
    i = i + 1
  }
  println(s)
}
"#;

/// `y` is bound by reference; `z` and a call argument select over it.
const SELECT_OF_SELECTED: &str = r#"fn walk(n: Int) -> Int = {
  let x = list.range(0, 3)
  let x2 = list.range(0, 2)
  let y = if n > 2 then x else x2
  var acc = 0
  var i = 0
  while i < n {
    let z = if i % 2 == 0 then x else y
    acc = acc + list.len(z) + list.len(if i % 3 == 0 then x2 else y)
    i = i + 1
  }
  acc + list.len(y)
}

effect fn main() -> Unit = {
  println(int.to_string(walk(4)))
  println(int.to_string(walk(1)))
}
"#;

/// Record, Option, Map and tuple selections (the record's field is moved out).
const OTHER_HEAP_SHAPES: &str = r#"type P = { name: String, n: Int }

fn rec_pick(k: Int) -> String = {
  let a = P { name: int.to_string(k), n: 1 }
  let b = P { name: int.to_string(k + 1), n: 2 }
  var out = ""
  var i = 0
  while i < 3 {
    let y = if i % 2 == 0 then a else b
    let s = y.name
    out = out + s + int.to_string(y.n)
    i = i + 1
  }
  out
}

fn opt_pick(k: Int) -> Int = {
  let a: Option[List[Int]] = some(list.range(0, k))
  let b: Option[List[Int]] = none
  var acc = 0
  var i = 0
  while i < 3 {
    let y = if i % 2 == 0 then a else b
    acc = acc + list.len(y ?? [])
    i = i + 1
  }
  acc
}

fn map_pick(k: Int) -> Int = {
  let a = map.from_list([("x", k), ("y", 2)])
  let b = map.from_list([("z", 3)])
  var acc = 0
  var i = 0
  while i < 3 {
    let y = if i % 2 == 0 then a else b
    acc = acc + map.len(y) + (map.get(y, "x") ?? 0)
    i = i + 1
  }
  acc
}

fn tup_pick(k: Int) -> String = {
  let a = (int.to_string(k), k)
  let b = (int.to_string(k * 2), k * 2)
  var out = ""
  var i = 0
  while i < 3 {
    let y = if i % 2 == 0 then a else b
    out = out + y.0
    i = i + 1
  }
  out
}

effect fn main() -> Unit = {
  println(rec_pick(5))
  println(int.to_string(opt_pick(4)))
  println(int.to_string(map_pick(7)))
  println(tup_pick(3))
}
"#;

/// An all-scalar record selection, which the v1 render holds as a list block.
const SCALAR_RECORD: &str = r#"type Q = { a: Int, b: Int }

effect fn main() -> Unit = {
  let p = Q { a: 1, b: 2 }
  let q = Q { a: 10, b: 20 }
  var acc = 0
  var i = 0
  while i < 3 {
    let y = if i % 2 == 0 then p else q
    acc = acc + y.a * 100 + y.b
    i = i + 1
  }
  println(int.to_string(acc))
}
"#;

fn write(program: &str) -> (tempfile::TempDir, std::path::PathBuf) {
    let dir = tempfile::tempdir().expect("tempdir");
    let source = dir.path().join("main.almd");
    std::fs::write(&source, program).expect("source");
    (dir, source)
}

/// `almide run` on the v1 native route, the walker leg and wasm: every leg
/// builds and prints `expected`. `on_v1` says whether the native route must
/// stay on the v1 render (a program outside its subset falls back).
/// A leg's name, its extra CLI flags and its environment.
type Leg<'a> = (&'a str, &'a [&'a str], &'a [(&'a str, &'a str)]);

fn every_leg(program: &str, expected: &str, on_v1: bool) {
    let (_dir, source) = write(program);
    let runs: [Leg; 3] = [
        ("native", &[], &[("ALMIDE_VERIFIED_DEBUG", "1")]),
        ("native walker", &["--no-verified"], &[("ALMIDE_NO_VERIFIED_OK", "1")]),
        ("wasm", &["--target", "wasm"], &[]),
    ];
    for (label, args, envs) in runs {
        let mut cmd = Command::new(env!("CARGO_BIN_EXE_almide"));
        cmd.arg("run").arg(&source).args(args);
        for (k, v) in envs {
            cmd.env(k, v);
        }
        let out = cmd.output().expect("spawn almide run");
        let stderr = String::from_utf8_lossy(&out.stderr);
        assert!(out.status.success(), "{label}: the build or run failed:\n{stderr}");
        assert_eq!(String::from_utf8_lossy(&out.stdout), expected, "{label}");
        if label == "native" && on_v1 {
            assert!(stderr.contains("native: v1 trust-spine render") && !stderr.contains("walled"), "left the v1 render:\n{stderr}");
        }
    }
}

/// The walker's render of `program` (`--target rust`).
fn walker_render(program: &str) -> String {
    let (_dir, source) = write(program);
    let out = Command::new(env!("CARGO_BIN_EXE_almide"))
        .args([source.to_str().expect("path"), "--target", "rust"])
        .output()
        .expect("spawn almide --target rust");
    assert!(out.status.success(), "{}", String::from_utf8_lossy(&out.stderr));
    String::from_utf8_lossy(&out.stdout).to_string()
}

#[test]
fn a_string_join_read_by_a_self_append_of_its_source_builds() {
    every_leg(SELF_APPEND, "12123451212345\n", true);
}

#[test]
fn a_selection_over_a_binder_bound_by_reference_builds() {
    every_leg(SELECT_OF_SELECTED, "25\n7\n", true);
    // A binder another selection names in an arm is decided before either is
    // rewritten: it stays a value (its arm reads it bare), so no arm is ever
    // `&y` over a `y` that is itself a reference.
    let rust = walker_render(SELECT_OF_SELECTED);
    assert!(
        !(rust.contains("let y: _ = ") && rust.contains("&y }")),
        "an arm borrows a binder that is already a reference:\n{rust}"
    );
}

#[test]
fn record_option_map_and_tuple_selections_build() {
    every_leg(OTHER_HEAP_SHAPES, "516251\n8\n19\n363\n", false);
}

#[test]
fn a_scalar_record_selection_borrows_on_v1() {
    every_leg(SCALAR_RECORD, "1224\n", true);
    let rust = almide_mir::pipeline::try_render_rust_source(SCALAR_RECORD).expect("renders on v1");
    let main = &rust[rust.find("fn main()").expect("main")..];
    assert!(!main.contains(".clone()"), "the record selection copies:\n{main}");
}
