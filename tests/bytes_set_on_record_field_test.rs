//! `bytes.put_*(h.f, ..)` on a record var's Bytes FIELD keeps value semantics
//! on the structural wasm leg: the record and then the field's block are
//! made unique before the in-place store (bytes_recv.rs's two-level COW).
//! Before, `var p2 = p1; bytes.put_u8(p2.buf, 1, 42)` wrote through the block
//! `p1.buf` shared and the alias read 42 where native reads 8 — a silent
//! divergence in the released 0.62.0, caught by
//! `spec/lang/rccow_value_semantics_test.almd` once the structural leg
//! served it.
//!
//! The matrix lives in `spec/wasm_cross/bytes_set_on_record_field.almd`
//! (C-033). This file carries what the fixture cannot: a nested path
//! `o.inner.buf` (the shape that printed `42 42` before) — the removed
//! `bytes.set_*` family walled it by name (`bytes-set-nonvar`); `bytes.put_u8`
//! takes its receiver as a `mut` parameter, so the nested place is written
//! back after the call and must leave the alias intact on both legs; and the shape
//! the fixture does not spell because the incumbent brick refuses its
//! enclosing loop — a `for` loop of sets through the field after a snapshot.

use std::process::Command;

const NESTED_PATH: &str = r#"
type Pack = { tag: String, buf: Bytes }
type Outer = { name: String, inner: Pack }
fn main() -> Unit = {
  let o1 = Outer { name: "o", inner: Pack { tag: "i", buf: bytes.from_list([7, 8]) } }
  var o2 = o1
  bytes.put_u8(o2.inner.buf, 1, 42)
  println("${bytes.get(o1.inner.buf, 1) ?? 0} ${bytes.get(o2.inner.buf, 1) ?? 0}")
}
"#;

const SETS_IN_LOOP_AFTER_SNAPSHOT: &str = r#"
type Pack = { tag: String, buf: Bytes }
fn main() -> Unit = {
  var acc = Pack { tag: "acc", buf: bytes.zeros(64) }
  let snap = acc
  for i in 0..<64 {
    bytes.put_u8(acc.buf, i, i * 3)
  }
  var sum = 0
  for i in 0..<64 {
    sum = sum + bytes.get(acc.buf, i) ?? 0
  }
  println("${sum} ${bytes.get(acc.buf, 63) ?? 0} ${bytes.get(snap.buf, 63) ?? 0} ${snap.tag}")
}
"#;

fn almide_bin() -> &'static str {
    env!("CARGO_BIN_EXE_almide")
}

fn wasmtime_available() -> bool {
    Command::new("wasmtime").arg("--version").output().is_ok_and(|o| o.status.success())
}

fn build(program: &str, target: &str, dir: &std::path::Path) -> std::process::Output {
    let source = dir.join("main.almd");
    std::fs::write(&source, program).expect("source");
    let artifact = dir.join(if target == "rust" { "native" } else { "m.wasm" });
    Command::new(almide_bin())
        .args(["build", source.to_str().expect("path"), "--target", target, "-o", artifact.to_str().expect("path")])
        .env_remove("ALMIDE_COMPONENT_P3")
        .output()
        .expect("build")
}

/// Build on both legs, run under stock `wasmtime`, assert agreement.
fn agree(program: &str, label: &str) -> String {
    let dir = tempfile::tempdir().expect("tempdir");
    let mut first: Option<String> = None;
    for target in ["rust", "wasm"] {
        let built = build(program, target, dir.path());
        assert!(built.status.success(), "{label}/{target} build:\n{}", String::from_utf8_lossy(&built.stderr));
        let artifact = dir.path().join(if target == "rust" { "native" } else { "m.wasm" });
        let mut command = if target == "rust" {
            Command::new(&artifact)
        } else {
            let mut c = Command::new("wasmtime");
            c.arg("run").arg(&artifact);
            c
        };
        let out = command.output().expect("run");
        assert!(out.status.success(), "{label}/{target} exited {:?}: {}", out.status.code(), String::from_utf8_lossy(&out.stderr));
        let stdout = String::from_utf8_lossy(&out.stdout).to_string();
        match &first {
            Some(native) => assert_eq!(&stdout, native, "{label}: the wasm leg answered differently from native"),
            None => first = Some(stdout),
        }
    }
    first.expect("native ran")
}

#[test]
fn sets_through_the_field_in_a_loop_leave_the_snapshot_intact_on_both_legs() {
    if !wasmtime_available() {
        return;
    }
    // 3 * (0 + 1 + ... + 63) = 6048; the snapshot keeps its zeros.
    assert_eq!(agree(SETS_IN_LOOP_AFTER_SNAPSHOT, "sets in loop").trim(), "6048 189 0 acc");
}

#[test]
fn a_nested_path_receiver_writes_back_without_reaching_the_alias() {
    if !wasmtime_available() {
        return;
    }
    // `o1` keeps its 8; only `o2`'s copy of the nested block reads 42.
    assert_eq!(agree(NESTED_PATH, "nested path").trim(), "8 42");
}
