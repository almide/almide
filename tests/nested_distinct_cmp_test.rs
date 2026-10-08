//! The ordering and display twins of #3450 (`nested_distinct_eq_test.rs`).
//!
//! The structural wasm leg inlines the total-order compare (`list.sort`,
//! `sort_by`, `min` / `max`) and the display (`${x}`, `assert_eq`'s message)
//! type by type, each level holding i32 slots from the 24-slot pool: a list
//! compare five, a tuple three, an option two; a list display three. A sort of
//! `List[List[List[List[String]]]]`, eight nested lists under an `assert_eq`, and
//! a display of a record chain each nesting `List[List[List[_]]]` walled with
//! `hold-depth-i32` and fell back to native. A compare or display that would
//! overflow the pool is now a call to an outlined per-type helper.
//!
//! These programs live here rather than in `spec/wasm_cross/`: a function
//! returning a list three or more levels deep walls the MIR corpus leg.

use std::process::Command;

/// Sorting and extremum walks over nested lists, tuples and options.
const ORDERING: &str = r#"
type A: Ord = { xs: List[List[List[String]]] }
type B: Ord = { xs: List[List[List[A]]] }
type C: Ord = { xs: List[List[List[B]]] }

effect fn main() -> Unit = {
  let l5: List[List[List[List[List[List[String]]]]]] = [[[[[["b"]]]]], [[[[["a", "z"]]]]], [[[[["a"]]]]]]
  println("${list.sort(l5)}")
  println("${list.max(l5)}")
  println("${list.min(l5)}")
  let keyed: List[(Int, List[List[List[List[List[String]]]]])] = [(1, [[[[["q"]]]]]), (2, [[[[["p"]]]]])]
  println("${list.sort_by(keyed, ((_, v)) => v)}")
  let t: List[List[(Int, List[(String, List[(Int, List[(String, Int)])])])]] =
    [[(1, [("x", [(2, [("y", 9)])])])], [(1, [("x", [(2, [("y", 3)])])])]]
  println("${list.sort(t)}")
  let o: List[List[Option[List[Option[List[Option[List[Option[String]]]]]]]]] =
    [[some([some([some([some("v")])])])], [some([some([some([none])])])], [none]]
  println("${list.sort(o)}")
  let tup: List[((((((((Int, Int), Int), Int), Int), Int), Int), Int), Int)] =
    [((((((((1, 3), 3), 4), 5), 6), 7), 8), 9), ((((((((1, 2), 3), 4), 5), 6), 7), 8), 9)]
  println("${list.sort(tup)}")
  let c1: C = { xs: [[[{ xs: [[[{ xs: [[["s2"]]] }]]] }]]] }
  let c2: C = { xs: [[[{ xs: [[[{ xs: [[["s1"]]] }]]] }]]] }
  println("${list.sort([c1, c2])}")
}
"#;

/// Displays deeper than the pool: pure lists, every container kind, and
/// generic-instance leaves that print by their IR type (UInt64, Float32).
const DISPLAY: &str = r#"
type A = { xs: List[List[List[String]]] }
type B = { xs: List[List[List[A]]] }
type C = { xs: List[List[List[B]]] }
type W[T] = { v: T, c: C }

effect fn main() -> Unit = {
  let deep: List[List[List[List[List[List[List[List[List[Int]]]]]]]]] = [[[[[[[[[1, 2]]]]]]]]]
  println("${deep}")
  let u = (0 - 1).to_uint64()
  let h = 0.1
  let f = h.to_float32()
  let m: List[List[List[List[List[List[List[List[(UInt64, Option[List[Map[String, List[Float32]]]])]]]]]]]] =
    [[[[[[[[(u, some([["k": [f]]]))]]]]]]]]
  println("${m}")
  let r: Result[Int, List[List[List[List[List[List[List[List[String]]]]]]]]] = err([[[[[[[["x"]]]]]]]])
  println("${r}")
  let w: W[UInt64] = { v: u, c: { xs: [[[{ xs: [[[{ xs: [[["s"]]] }]]] }]]] } }
  println("${w}")
}
"#;

fn almide_bin() -> &'static str {
    env!("CARGO_BIN_EXE_almide")
}

fn wasmtime_available() -> bool {
    Command::new("wasmtime").arg("--version").output().is_ok_and(|o| o.status.success())
}

/// Run one program on both legs, assert the wasm build came from the
/// STRUCTURAL leg (a wall would route it elsewhere), and that the two legs
/// print the same bytes; returns the native stdout.
fn agree(program: &str, label: &str) -> String {
    let dir = tempfile::tempdir().expect("tempdir");
    let source = dir.path().join("main.almd");
    std::fs::write(&source, program).expect("source");
    let mut first: Option<String> = None;
    for target in ["rust", "wasm"] {
        let artifact = dir.path().join(if target == "rust" { "native" } else { "m.wasm" });
        let built = Command::new(almide_bin())
            .args(["build", source.to_str().expect("path"), "--target", target, "-o"])
            .arg(&artifact)
            .env_remove("ALMIDE_COMPONENT_P3")
            .output()
            .expect("build");
        let log = format!("{}{}", String::from_utf8_lossy(&built.stdout), String::from_utf8_lossy(&built.stderr));
        assert!(built.status.success(), "{label}/{target} build:\n{log}");
        if target == "wasm" {
            assert!(log.contains("structural leg"), "{label}: wasm build left the structural leg:\n{log}");
        }
        let mut command = if target == "rust" {
            Command::new(&artifact)
        } else {
            let mut c = Command::new("wasmtime");
            c.arg("run").arg(&artifact);
            c
        };
        let out = command.output().expect("run");
        assert!(out.status.success(), "{label}/{target} exited {:?}", out.status.code());
        let stdout = String::from_utf8_lossy(&out.stdout).to_string();
        match &first {
            Some(native) => assert_eq!(&stdout, native, "{label}: the wasm leg answered differently from native"),
            None => first = Some(stdout),
        }
    }
    first.expect("one leg ran")
}

#[test]
fn nested_orderings_sort_on_the_structural_leg() {
    if !wasmtime_available() {
        eprintln!("skipping: wasmtime not on PATH");
        return;
    }
    let out = agree(ORDERING, "ordering");
    let lines: Vec<&str> = out.lines().collect();
    assert_eq!(lines.len(), 8, "{out}");
    assert_eq!(lines[0], r#"[[[[[["a"]]]]], [[[[["a", "z"]]]]], [[[[["b"]]]]]]"#);
    assert_eq!(lines[1], r#"some([[[[["b"]]]]])"#);
    assert_eq!(lines[2], r#"some([[[[["a"]]]]])"#);
}

#[test]
fn nested_displays_print_on_the_structural_leg() {
    if !wasmtime_available() {
        eprintln!("skipping: wasmtime not on PATH");
        return;
    }
    let out = agree(DISPLAY, "display");
    let lines: Vec<&str> = out.lines().collect();
    assert_eq!(lines[0], "[[[[[[[[[1, 2]]]]]]]]]");
    assert_eq!(lines[1], r#"[[[[[[[[(18446744073709551615, some([["k": [0.1]]]))]]]]]]]]"#);
}
