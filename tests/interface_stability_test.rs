//! Interface stability (the grain CRC pattern, Survey 4 law 5): the
//! module interface must be a function of SIGNATURES only. This is the
//! edit-locality `typing_modular` theorem (crates/almide-edit-belt,
//! Typing.lean) run as a unit test over the REAL compiler — a
//! signature-preserving body edit must leave `almide compile --json`
//! byte-identical, and a signature edit must change it (so byte-equality
//! cannot pass vacuously by printing nothing).
//!
//! If the body-invariance assertion ever fails, that is an L1-class bug
//! (a body detail leaked into the interface other modules typecheck
//! against): file it, do not paper over it here.

use std::fs;
use std::path::Path;
use std::process::Command;

fn almide() -> &'static str {
    env!("CARGO_BIN_EXE_almide")
}

/// Compile `source` as module `m` in its own tempdir, return the
/// interface JSON bytes.
fn interface_json(dir: &Path, source: &str) -> String {
    let path = dir.join("m.almd");
    fs::write(&path, source).expect("write module source");
    let out = Command::new(almide())
        .args(["compile", path.to_str().unwrap(), "--json"])
        .output()
        .expect("run almide compile --json");
    assert!(
        out.status.success(),
        "compile --json failed:\n{}",
        String::from_utf8_lossy(&out.stderr)
    );
    String::from_utf8(out.stdout).expect("interface JSON is utf8")
}

/// Version A: the baseline module — a pure fn, a fn calling a sibling,
/// an effect fn with a statement body.
const VERSION_A: &str = r#"fn double(x: Int) -> Int = x * 2

fn describe(n: Int) -> String = "value: ${double(n)}"

effect fn shout(s: String) -> Int = {
  println(s)
  1
}
"#;

/// Version B: every SIGNATURE identical, every BODY different — new
/// arithmetic, new locals, a match, a different sibling-call graph.
const VERSION_B: &str = r#"fn double(x: Int) -> Int = x + x + 0

fn describe(n: Int) -> String = match n {
  0 => "value: 0",
  _ => "value: ${n + n}",
}

effect fn shout(s: String) -> Int = {
  let t = s + "!"
  println(t)
  2 - 1
}
"#;

/// Version C: one signature changed (double's return type).
const VERSION_C: &str = r#"fn double(x: Int) -> String = "${x * 2}"

fn describe(n: Int) -> String = "value: ${double(n)}"

effect fn shout(s: String) -> Int = {
  println(s)
  1
}
"#;

#[test]
fn body_edit_leaves_interface_byte_identical() {
    let da = tempfile::tempdir().expect("tempdir");
    let db = tempfile::tempdir().expect("tempdir");
    let a = interface_json(da.path(), VERSION_A);
    let b = interface_json(db.path(), VERSION_B);
    assert_eq!(
        a, b,
        "signature-preserving body edit changed the module interface — \
         a body detail leaked into what other modules typecheck against \
         (L1-class bug: file it, do not adjust this test)"
    );
}

#[test]
fn signature_edit_changes_interface() {
    let da = tempfile::tempdir().expect("tempdir");
    let dc = tempfile::tempdir().expect("tempdir");
    let a = interface_json(da.path(), VERSION_A);
    let c = interface_json(dc.path(), VERSION_C);
    assert_ne!(
        a, c,
        "return-type change did not move the interface — the equality \
         assertion above would be vacuous"
    );
}
