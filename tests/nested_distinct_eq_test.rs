//! #3450: structural `==` on a type nesting several DISTINCT containers.
//!
//! The structural wasm leg inlines a comparison type by type, and every level
//! holds i32 slots across its children (a list five, a record / tuple / option
//! two, a map six). `List[List[List[List[List[String]]]]]` held 25 of the 24-slot
//! pool and the build walled with `hold-depth-i32`, so the program fell back off
//! the structural leg. A comparison that would overflow the pool is now a call to
//! an outlined `(a, b) -> i32` helper on a fresh pool.
//!
//! These programs live here rather than in `spec/wasm_cross/`: a function
//! returning a list three or more levels deep walls the MIR corpus leg, so a
//! corpus fixture would grow the walled-real baseline. The record-chain half of
//! the issue is the corpus fixture `spec/wasm_cross/deep_eq_nested_distinct.almd`.

use std::process::Command;

/// Five and seven `List` levels, equal and unequal at the innermost String,
/// compared inside `main` (literals bound by `let` in the frame that compares).
const NESTED_LISTS: &str = r#"
fn verdict(b: Bool) -> String = if b then "eq" else "ne"

effect fn main() -> Unit = {
  let a: List[List[List[List[List[String]]]]] = [[[[["a"]]]]]
  println(verdict(a == [[[[["a"]]]]]))
  println(verdict(a == [[[[["b"]]]]]))
  let b: List[List[List[List[List[List[List[String]]]]]]] = [[[[[[["x", "y"], ["z"]]]]]]]
  println(verdict(b == [[[[[[["x", "y"], ["z"]]]]]]]))
  println(verdict(b == [[[[[[["x", "y"], ["w"]]]]]]]))
  println(verdict(b != [[[[[[["x", "y"]]]]]]]))
}
"#;

/// Every container kind in one stack: list, option, tuple, map, list, list.
const MIXED_CONTAINERS: &str = r#"
fn verdict(b: Bool) -> String = if b then "eq" else "ne"

effect fn main() -> Unit = {
  let m: List[Option[(Int, List[Map[String, List[List[String]]]])]] = [some((1, [["k": [["v"]]]])), none]
  println(verdict(m == [some((1, [["k": [["v"]]]])), none]))
  println(verdict(m == [some((1, [["k": [["u"]]]])), none]))
  println(verdict(m == [none, some((1, [["k": [["v"]]]]))]))
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
fn nested_lists_five_and_seven_deep_compare_on_the_structural_leg() {
    if !wasmtime_available() {
        eprintln!("skipping: wasmtime not on PATH");
        return;
    }
    assert_eq!(agree(NESTED_LISTS, "nested-lists"), "eq\nne\neq\nne\neq\n");
}

#[test]
fn a_mixed_container_stack_compares_on_the_structural_leg() {
    if !wasmtime_available() {
        eprintln!("skipping: wasmtime not on PATH");
        return;
    }
    assert_eq!(agree(MIXED_CONTAINERS, "mixed"), "eq\nne\nne\n");
}
