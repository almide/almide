//! #2061: each implicit fan closure owns its read-only captures before spawning.
use std::process::Command;

#[test]
fn fan_siblings_preserve_shared_heap_inputs_on_both_targets() {
    let dir = tempfile::tempdir().unwrap();
    let source = dir.path().join("main.almd");
    std::fs::write(&source, r#"type Row = { values: List[Int], label: String }

effect fn measure(rows: List[Int]) -> (Int, Int) = fan { list.sum(rows), list.len(rows) }

effect fn label_pair(label: String) -> (String, String) = fan { label + "a", label + "b" }

effect fn main() -> Unit = {
  let rows = [1, 2, 3]
  let (sum, size) = fan { list.sum(rows), list.len(rows) }
  assert_eq((sum, size), (6, 3))
  let measured = measure(rows)!
  assert_eq(measured, (6, 3))
  assert_eq(rows, [1, 2, 3])
  let label = "x"
  let (left, right) = fan { label + "a", label + "b" }
  assert_eq((left, right), ("xa", "xb"))
  let labels = label_pair(label)!
  assert_eq(labels, ("xa", "xb"))
  assert_eq(label, "x")
  let counts = ["x": 7]
  let (count, entries) = fan { map.get(counts, "x") ?? 0, map.len(counts) }
  assert_eq(count, 7)
  assert_eq(entries, 1)
  assert_eq(map.get(counts, "x"), some(7))
  let row = Row { values: rows, label: label }
  let (row_sum, row_label) = fan { list.sum(row.values), row.label + "!" }
  assert_eq((row_sum, row_label), (6, "x!"))
  assert_eq(row.label, "x")
  let (nested, outer) = fan {
    {
      let (a, b) = fan { list.sum(rows), list.len(rows) }
      a + b
    },
    list.len(rows),
  }
  assert_eq((nested, outer), (9, 3))
  println("fan captures ok")
}
"#).unwrap();
    let bin = std::env::var("ALMIDE_BIN")
        .unwrap_or_else(|_| format!("{}/target/release/almide", env!("CARGO_MANIFEST_DIR")));
    let emitted = Command::new(&bin).arg("emit").arg(&source)
        .output().expect("emit capture matrix");
    assert!(emitted.status.success());
    let rust = String::from_utf8(emitted.stdout).unwrap();
    let user_rust = rust.split("//__ALMIDE_RT_BOUNDARY__").last().unwrap();
    assert!(!user_rust.contains(".clone().clone()"), "capture must be cloned only once");
    for target in ["rust", "wasm"] {
        let out = Command::new(&bin).arg("run").arg(&source)
            .args(["--target", target]).output().expect("run compiler");
        assert!(out.status.success(), "{target}: {}", String::from_utf8_lossy(&out.stderr));
        assert_eq!(String::from_utf8_lossy(&out.stdout).trim(), "fan captures ok", "{target}");
    }
}
