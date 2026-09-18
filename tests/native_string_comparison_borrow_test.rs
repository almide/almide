//! String comparison borrows stable operands without changing effect order.
use std::process::Command;

#[test]
fn string_comparison_borrows_fields_and_literals() {
    let dir = tempfile::tempdir().unwrap();
    let source = dir.path().join("main.almd");
    std::fs::write(&source, r#"type Tok = { kind: String, text: String }
fn same(t: Tok, want: String) -> Bool = t.kind == want
fn is_newline(t: Tok) -> Bool = t.kind == "newline"
fn before(t: Tok, want: String) -> Bool = t.kind < want
fn different(t: Tok, want: String) -> Bool = t.kind != want
fn change(mut t: Tok) -> String = { t.kind = "changed"; "newline" }
effect fn main() -> Unit = {
  let t = Tok { kind: "newline", text: "kept" }
  assert_eq(same(t, "newline"), true)
  assert_eq(is_newline(t), true)
  assert_eq(before(t, "z"), true)
  assert_eq(different(t, "x"), true)
  assert_eq(t.text, "kept")
  var changing = Tok { kind: "newline", text: "kept" }
  assert_eq(changing.kind == change(changing), true)
  assert_eq(changing.kind, "changed")
  println("comparisons ok")
}
"#).unwrap();
    let bin = std::env::var("ALMIDE_BIN").unwrap_or_else(|_| format!("{}/target/release/almide", env!("CARGO_MANIFEST_DIR")));
    let emitted = Command::new(&bin).arg("emit").arg(&source).output().unwrap();
    assert!(emitted.status.success(), "{}", String::from_utf8_lossy(&emitted.stderr));
    let rust = String::from_utf8(emitted.stdout).unwrap();
    for name in ["same", "is_newline", "before", "different"] {
        let body = rust.split(&format!("pub fn {name}(")).nth(1).unwrap().split("\n}").next().unwrap();
        assert!(!body.contains(".clone()") && !body.contains(".to_string()"), "{name}: {body}");
    }
    for target in ["rust", "wasm"] {
        let out = Command::new(&bin).arg("run").arg(&source).args(["--target", target]).output().unwrap();
        assert!(out.status.success(), "{target}: {}", String::from_utf8_lossy(&out.stderr));
        assert_eq!(String::from_utf8_lossy(&out.stdout).trim(), "comparisons ok");
    }
}
