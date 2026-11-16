//! #4764: a list built by `list.flat_map` and carried in `ok(..)` / `some(..)`
//! came back out empty on the native render.
//!
//! The v1 lowering builds a flat_map's result as a loop accumulator: a block
//! allocated empty (`DynList { len: 0 }`), then rebound each round
//! (`SetLocal acc <- concat(acc, part)`). The native render's set of empty
//! list blocks named every block allocated empty, rebound or not, and the
//! payload of the carrier read it from there: `ok(acc)` rendered as
//! `Ok(Vec::new())`. So `list.flat_map(result.unwrap_or_else(ok(<flat_map>),
//! ..), ..)` printed `[]` where wasm and v0.69.1 printed the elements. A block
//! allocated empty is that empty list where it is read only while nothing
//! rebinds it (`AddrState::still_empty_list`), the rule `known_none` keeps
//! for a mutable None.
//!
//! What this test holds:
//! - the issue's program prints the elements on native and on wasm;
//! - every carrier (Result and Option; `unwrap_or_else`, `??`, `match`) and
//!   every outer combinator over a flat_map-built payload prints them too,
//!   beside the issue's controls (a `map`-built list, a literal, no carrier);
//! - both programs stay on the certified v1 render, so the native leg runs
//!   the render that carried the bug, never the v0 fallback.

use std::path::{Path, PathBuf};
use std::process::Command;

fn almide() -> String {
    std::env::var("ALMIDE_BIN").unwrap_or_else(|_| env!("CARGO_BIN_EXE_almide").to_string())
}

fn scratch(tag: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("almide-rebound-empty-{}-{tag}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

/// (stdout, stderr) of `almide run <src>` on `target`.
fn run(src: &Path, target: Option<&str>) -> (String, String) {
    let mut cmd = Command::new(almide());
    cmd.arg("run").arg(src).env("ALMIDE_VERIFIED_DEBUG", "1");
    if let Some(t) = target {
        cmd.args(["--target", t]);
    }
    let out = cmd.output().expect("almide");
    (String::from_utf8_lossy(&out.stdout).into_owned(), String::from_utf8_lossy(&out.stderr).into_owned())
}

/// The program of #4764, verbatim.
const ISSUE: &str = r#"fn main() -> Unit = {
  let s1: Result[List[String], String] = ok(list.flat_map([1, 2, 3], (x) => ["a"]))
  let r4: List[Int] = list.flat_map(result.unwrap_or_else(s1, (y) => [y]), (t) => [7])
  println("r4 = ${r4}")
}
"#;

/// The flat_map-built payload through each carrier and outer combinator, and
/// the issue's controls.
const MATRIX: &str = r#"fn res_uoe() -> List[Int] = {
  let s1: Result[List[String], String] = ok(list.flat_map([1, 2, 3], (x) => ["a"]))
  list.flat_map(result.unwrap_or_else(s1, (y) => [y]), (t) => [7])
}

fn res_coalesce() -> List[Int] = {
  let s1: Result[List[String], String] = ok(list.flat_map([1, 2, 3], (x) => ["a"]))
  let u: List[String] = s1 ?? []
  list.map(u, (t) => 7)
}

fn res_match() -> List[Int] = {
  let s1: Result[List[String], String] = ok(list.flat_map([1, 2, 3], (x) => ["a"]))
  let u: List[String] = match s1 {
    ok(v) => v,
    err(e) => [e],
  }
  list.filter_map(u, (t) => some(7))
}

fn opt_uoe() -> Int = {
  let s1: Option[List[String]] = some(list.flat_map([1, 2, 3], (x) => ["a"]))
  let u: List[String] = option.unwrap_or_else(s1, () => [])
  list.fold(u, 0, (a, t) => a + 7)
}

fn opt_coalesce() -> List[Int] = {
  let s1: Option[List[String]] = some(list.flat_map([1, 2, 3], (x) => ["a"]))
  list.flat_map(s1 ?? [], (t) => [7])
}

fn opt_match() -> List[Int] = {
  let s1: Option[List[String]] = some(list.flat_map([1, 2, 3], (x) => ["a"]))
  let u: List[String] = match s1 {
    some(v) => v,
    none => [],
  }
  list.flat_map(u, (t) => [7])
}

fn control_map_inner() -> List[Int] = {
  let s1: Result[List[String], String] = ok(list.map([1, 2, 3], (x) => "a"))
  list.flat_map(result.unwrap_or_else(s1, (y) => [y]), (t) => [7])
}

fn control_literal() -> List[Int] = {
  let s1: Result[List[String], String] = ok(["a", "a", "a"])
  list.flat_map(result.unwrap_or_else(s1, (y) => [y]), (t) => [7])
}

fn control_no_carrier() -> List[Int] = {
  let a = list.flat_map([1, 2, 3], (x) => ["a"])
  list.flat_map(a, (t) => [7])
}

fn control_map_outer() -> List[Int] = {
  let s1: Result[List[String], String] = ok(list.flat_map([1, 2, 3], (x) => ["a"]))
  list.map(result.unwrap_or_else(s1, (y) => [y]), (t) => 7)
}

fn main() -> Unit = {
  println("res_uoe ${res_uoe()}")
  println("res_coalesce ${res_coalesce()}")
  println("res_match ${res_match()}")
  println("opt_uoe ${opt_uoe()}")
  println("opt_coalesce ${opt_coalesce()}")
  println("opt_match ${opt_match()}")
  println("control_map_inner ${control_map_inner()}")
  println("control_literal ${control_literal()}")
  println("control_no_carrier ${control_no_carrier()}")
  println("control_map_outer ${control_map_outer()}")
}
"#;

const MATRIX_OUT: &str = "res_uoe [7, 7, 7]
res_coalesce [7, 7, 7]
res_match [7, 7, 7]
opt_uoe 21
opt_coalesce [7, 7, 7]
opt_match [7, 7, 7]
control_map_inner [7, 7, 7]
control_literal [7, 7, 7]
control_no_carrier [7, 7, 7]
control_map_outer [7, 7, 7]
";

#[test]
fn both_programs_stay_on_the_certified_render() {
    for (tag, source) in [("issue", ISSUE), ("matrix", MATRIX)] {
        let rust = almide_mir::pipeline::try_render_rust_source(source).unwrap_or_else(|e| panic!("{tag} left the certified render: {e:?}"));
        assert!(!rust.contains("Ok(Vec::new())") && !rust.contains("Some(Vec::new())"),
            "{tag}: a rebound accumulator's payload renders as an empty list:\n{rust}");
    }
}

#[test]
fn a_flat_map_built_payload_keeps_its_elements_on_native() {
    for (tag, source, want) in [("issue", ISSUE, "r4 = [7, 7, 7]\n"), ("matrix", MATRIX, MATRIX_OUT)] {
        let dir = scratch(tag);
        let src = dir.join("prog.almd");
        std::fs::write(&src, source).unwrap();
        let (stdout, stderr) = run(&src, None);
        assert!(stderr.contains("native: v1 trust-spine render"), "{tag} did not ship the v1 render:\n{stderr}");
        assert_eq!(stdout, want, "{tag} on native:\n{stderr}");
        let _ = std::fs::remove_dir_all(&dir);
    }
}

#[test]
fn a_flat_map_built_payload_prints_the_same_on_wasm() {
    if Command::new("wasmtime").arg("--version").output().is_err() {
        eprintln!("skip: wasmtime is not installed");
        return;
    }
    for (tag, source, want) in [("issue-wasm", ISSUE, "r4 = [7, 7, 7]\n"), ("matrix-wasm", MATRIX, MATRIX_OUT)] {
        let dir = scratch(tag);
        let src = dir.join("prog.almd");
        std::fs::write(&src, source).unwrap();
        let (stdout, stderr) = run(&src, Some("wasm"));
        assert_eq!(stdout, want, "{tag} on wasm:\n{stderr}");
        let _ = std::fs::remove_dir_all(&dir);
    }
}
