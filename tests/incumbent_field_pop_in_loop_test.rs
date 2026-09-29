//! #2919: `list.pop(h.f)` on a record var's list FIELD inside an executing
//! loop body or unit arm, on the INCUMBENT wasm leg (almide-mir), forced
//! with `ALMIDE_WASM_INCUMBENT=1` and compared with native.
//!
//! The two-level record+field COW rebound the var to its copy through
//! `value_of`. That rebind is frame-local, so each later iteration re-read
//! the block the first iteration had already released: the ownership witness
//! went `a(id)(d)` (the proven checker rejects it), the program trapped, and
//! an alias taken before the loop could read the popped list's contents.
//! Inside a real loop or arm the rebind is now drop-old + `SetLocal` on the
//! var's stable local.
//!
//! These are Rust tests rather than `spec/wasm_cross` fixtures because the
//! default route sends the shape to the structural leg once it lowers it.
//! These tests force the incumbent, the fallback leg that still has to be
//! right.

use std::process::Command;

/// Push and pop through the field in ONE loop body — the combined shape
/// (`i(idd)` before the fix).
const PUSH_POP_SAME_LOOP: &str = r#"
type Item = { id: Int, kids: List[Int] }
effect fn main() -> Unit = {
  var it = { id: 1, kids: [1] }
  for i in 0..<10 {
    list.push(it.kids, i)
    let _ = list.pop(it.kids)
  }
  println("${it.kids} ${it.id}")
}
"#;

/// A push loop, then a pop loop whose pop is a `match` subject (`a(id)(d)`
/// before the fix).
const POP_LOOP_AFTER_PUSH_LOOP: &str = r#"
type Item = { id: Int, kids: List[Int] }
effect fn main() -> Unit = {
  var it = { id: 1, kids: [] }
  for i in 0..<5 {
    list.push(it.kids, i)
  }
  var total = 0
  for _ in 0..<3 {
    match list.pop(it.kids) {
      some(x) => {
        total = total + x
      },
      none => (),
    }
  }
  println("${it.kids} ${total}")
}
"#;

/// The pop inside one arm of an `if` in the loop, with a snapshot taken
/// before the loop: the snapshot must keep the pre-loop list (C-033). Before
/// the fix the incumbent printed the MUTATED list for the snapshot, then
/// trapped.
const POP_IN_ARM_WITH_SNAPSHOT: &str = r#"
type Item = { id: Int, kids: List[Int] }
effect fn main() -> Unit = {
  var it = { id: 7, kids: [1, 2, 3, 4, 5] }
  let snap = it
  for i in 0..<4 {
    if i % 2 == 0 then {
      let _ = list.pop(it.kids)
    } else {
      list.push(it.kids, i * 10)
    }
  }
  println("${it.kids} ${snap.kids} ${it.id}")
}
"#;

/// Popping past empty inside the loop: `none` from the empty field, and the
/// field stays empty.
const POP_PAST_EMPTY: &str = r#"
type Item = { id: Int, kids: List[Int] }
effect fn main() -> Unit = {
  var it = { id: 2, kids: [4, 5] }
  var nones = 0
  for _ in 0..<5 {
    match list.pop(it.kids) {
      some(_) => (),
      none => {
        nones = nones + 1
      },
    }
  }
  println("${it.kids} ${nones}")
}
"#;

fn almide_bin() -> &'static str {
    env!("CARGO_BIN_EXE_almide")
}

fn wasmtime_available() -> bool {
    Command::new("wasmtime").arg("--version").output().is_ok_and(|o| o.status.success())
}

/// Run on native, then on the forced incumbent wasm leg; assert both exit 0
/// with identical stdout, and return it.
fn agree_on_incumbent(program: &str, label: &str) -> String {
    let dir = tempfile::tempdir().expect("tempdir");
    let source = dir.path().join("main.almd");
    std::fs::write(&source, program).expect("source");
    let native = Command::new(almide_bin())
        .args(["run", source.to_str().expect("path")])
        .env_remove("ALMIDE_WASM_INCUMBENT")
        .env_remove("ALMIDE_WASM_STRUCTURAL")
        .output()
        .expect("native run");
    assert!(native.status.success(), "{label}/native: {}", String::from_utf8_lossy(&native.stderr));
    let wasm = Command::new(almide_bin())
        .args(["run", source.to_str().expect("path"), "--target", "wasm"])
        .env("ALMIDE_WASM_INCUMBENT", "1")
        .env_remove("ALMIDE_WASM_STRUCTURAL")
        .output()
        .expect("incumbent run");
    assert!(
        wasm.status.success(),
        "{label}/incumbent exited {:?}: {}",
        wasm.status.code(),
        String::from_utf8_lossy(&wasm.stderr)
    );
    let native_out = String::from_utf8_lossy(&native.stdout).to_string();
    let wasm_out = String::from_utf8_lossy(&wasm.stdout).to_string();
    assert_eq!(wasm_out, native_out, "{label}: the incumbent answered differently from native");
    native_out
}

#[test]
fn push_and_pop_through_a_field_in_one_loop() {
    if !wasmtime_available() {
        return;
    }
    assert_eq!(agree_on_incumbent(PUSH_POP_SAME_LOOP, "same loop").trim(), "[1] 1");
}

#[test]
fn a_pop_loop_after_a_push_loop() {
    if !wasmtime_available() {
        return;
    }
    assert_eq!(agree_on_incumbent(POP_LOOP_AFTER_PUSH_LOOP, "pop after push").trim(), "[0, 1] 9");
}

#[test]
fn a_pop_in_a_loop_arm_leaves_the_snapshot_intact() {
    if !wasmtime_available() {
        return;
    }
    assert_eq!(
        agree_on_incumbent(POP_IN_ARM_WITH_SNAPSHOT, "pop in arm").trim(),
        "[1, 2, 3, 4, 30] [1, 2, 3, 4, 5] 7"
    );
}

#[test]
fn popping_past_empty_in_a_loop() {
    if !wasmtime_available() {
        return;
    }
    assert_eq!(agree_on_incumbent(POP_PAST_EMPTY, "past empty").trim(), "[] 3");
}
