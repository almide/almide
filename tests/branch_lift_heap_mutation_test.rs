//! #2062: list element writes must survive heap-branch outlining.
//! Run both actual compiler targets: the legacy v1 test harness cannot lower
//! this shape, while the structural wasm run path supports it.
use std::process::Command;

#[test]
fn branch_outlining_preserves_enclosing_list_writes() {
    let dir = tempfile::tempdir().unwrap();
    let source = dir.path().join("main.almd");
    std::fs::write(&source, r#"effect fn main() -> Unit = {
  var xs = [0]
  for n in 1..<3 {
    let label = if n > 0 then {
      xs[0] = n
      "yes"
    } else "no"
    assert_eq(label, "yes")
  }
  assert_eq(xs, [2])
  println(int.to_string(xs[0]))
}

"#).unwrap();
    let bin = std::env::var("ALMIDE_BIN")
        .unwrap_or_else(|_| format!("{}/target/release/almide", env!("CARGO_MANIFEST_DIR")));
    for target in ["rust", "wasm"] {
        let out = Command::new(&bin)
            .arg("run").arg(&source).args(["--target", target])
            .output().expect("run compiler");
        assert!(out.status.success(), "{target}: {}", String::from_utf8_lossy(&out.stderr));
        assert_eq!(String::from_utf8_lossy(&out.stdout).trim(), "2", "{target}");
    }
}
