//! #2098: synchronous scalar folds borrow read-only captures; escaping
//! closures still own their captures and mutable captures still share state.
use std::process::Command;

#[test]
fn scalar_fold_borrows_without_changing_escaping_closures() {
    let dir = tempfile::tempdir().expect("tempdir");
    let file = dir.path().join("capture.almd");
    std::fs::write(&file, r#"
fn row(v: List[Float], n: Int) -> Float =
  list.fold(0..<n, 0.0, (a, j) => a + v[j])
fn saved(v: List[Float]) -> (Int) -> Float = (i) => v[i]
effect fn main() -> Unit = {
  let v = [1.0, 2.0, 3.0]
  assert(row(v, 3) == 6.0)
  assert(row(v, 0) == 0.0)
  assert(v[2] == 3.0)
  let f = saved([7.0, 8.0])
  assert(f(1) == 8.0)
  var total = 0
  let sum = list.fold([1, 2, 3], 0, (a, x) => {
    total = total + x
    a + x
  })
  assert(sum == 6)
  assert(total == 6)
  var indexed = [1, 2, 3]
  indexed[{ let size = list.len(indexed); size - 1 }] = 42
  assert(indexed[2] == 42)
  println("ok")
}
"#).expect("source");
    let result = Command::new(env!("CARGO_BIN_EXE_almide"))
        .arg("run").arg(&file).output().expect("native run");
    assert!(result.status.success(), "{}", String::from_utf8_lossy(&result.stderr));
    assert_eq!(result.stdout, b"ok\n");
    let emitted = Command::new(env!("CARGO_BIN_EXE_almide"))
        .arg("emit").arg(&file).output().expect("emit");
    assert!(emitted.status.success());
    let rust = String::from_utf8(emitted.stdout).expect("UTF8");
    let row = rust.split("pub fn row(").nth(1).expect("row function")
        .split("\npub fn ").next().expect("body");
    assert!(!row.contains("__cap_"), "scalar fold still clones its capture: {row}");
}
