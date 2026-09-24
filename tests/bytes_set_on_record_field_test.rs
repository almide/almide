//! `bytes.set_*(h.f, ..)` on a record var's Bytes FIELD keeps value semantics
//! on the structural wasm leg: the record and then the field's block are
//! made unique before the in-place store (bytes_recv.rs's two-level COW).
//! Before, `var p2 = p1; bytes.set_at(p2.buf, 1, 42)` wrote through the block
//! `p1.buf` shared and the alias read 42 where native reads 8 — a silent
//! divergence in the released 0.62.0, caught by
//! `spec/lang/rccow_value_semantics_test.almd` once the structural leg
//! served it.
//!
//! The matrix lives in `spec/wasm_cross/bytes_set_on_record_field.almd`
//! (C-033). This file carries what the fixture cannot: the DECLARED OMISSION
//! stays a wall (a nested path `o.inner.buf` is refused by name, not written
//! through the alias — the shape that printed `42 42` before), pinned so a
//! later change that starts accepting it has to say so here; and the shape
//! the fixture does not spell because the incumbent brick refuses its
//! enclosing loop — a `for` loop of sets through the field after a snapshot.

use std::process::Command;

const NESTED_PATH: &str = r#"
type Pack = { tag: String, buf: Bytes }
type Outer = { name: String, inner: Pack }
fn main() -> Unit = {
  let o1 = Outer { name: "o", inner: Pack { tag: "i", buf: bytes.from_list([7, 8]) } }
  var o2 = o1
  bytes.set_at(o2.inner.buf, 1, 42)
  println("${bytes.read_u8(o1.inner.buf, 1)} ${bytes.read_u8(o2.inner.buf, 1)}")
}
"#;

const SETS_IN_LOOP_AFTER_SNAPSHOT: &str = r#"
type Pack = { tag: String, buf: Bytes }
fn main() -> Unit = {
  var acc = Pack { tag: "acc", buf: bytes.new(64) }
  let snap = acc
  for i in 0..<64 {
    bytes.set_at(acc.buf, i, i * 3)
  }
  var sum = 0
  for i in 0..<64 {
    sum = sum + bytes.read_u8(acc.buf, i)
  }
  println("${sum} ${bytes.read_u8(acc.buf, 63)} ${bytes.read_u8(snap.buf, 63)} ${snap.tag}")
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
        .env_remove("ALMIDE_WASM_INCUMBENT")
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
fn a_nested_path_receiver_is_refused_by_name_rather_than_written_through_the_alias() {
    let dir = tempfile::tempdir().expect("tempdir");
    let built = build(NESTED_PATH, "wasm", dir.path());
    assert!(
        !built.status.success(),
        "a nested receiver path has an owner the field gate does not model and must wall, not build"
    );
    let stderr = String::from_utf8_lossy(&built.stderr);
    assert!(
        stderr.contains("bytes-set-nonvar"),
        "the refusal must name the receiver shape (bytes-set-nonvar), got:\n{stderr}"
    );
}
