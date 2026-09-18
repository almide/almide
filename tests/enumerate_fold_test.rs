//! #2098: scalar enumerate/fold optimization agrees with native, including
//! empty inputs and tuple-escaping callbacks that must retain ordinary lowering.
use std::process::Command;

#[test]
fn enumerate_fold_preserves_scalar_and_escaping_results() {
    let dir = tempfile::tempdir().expect("tempdir");
    let source = dir.path().join("main.almd");
    std::fs::write(&source, r#"
fn calc(xs: List[Float]) -> Float =
  list.fold(list.enumerate(xs), 0.0, (acc, p) => acc + float.from_int(p.0) * p.1)
fn item(p: (Int, Int)) -> Int = p.1
fn main() -> Unit = {
  assert(calc([]) == 0.0)
  assert(calc([2.0, 3.0, 4.0]) == 11.0)
  assert(list.fold(list.enumerate([true, false, true]), 0, (a, p) => if p.1 then a + p.0 else a) == 2)
  assert(list.fold(list.enumerate([10, 20]), [], (a, p) => a + [p]) == [(0, 10), (1, 20)])
  assert(list.fold(list.enumerate(["a", "b"]), "", (a, p) => a + p.1) == "ab")
  assert(list.fold(list.enumerate([10, 20]), 0, (a, p) => a + item(p)) == 30)
  var xs = [1, 2, 3]
  let snapshot_sum = list.fold(list.enumerate(xs), 0, (a, p) => {
    xs[2] = 99
    a + p.1
  })
  assert(snapshot_sum == 6)
  assert(xs[2] == 99)
  println("ok")
}
"#).expect("source");
    let mut expected = None;
    for target in ["rust", "wasm"] {
        let artifact = dir.path().join(if target == "rust" { "native" } else { "module.wasm" });
        let built = Command::new(env!("CARGO_BIN_EXE_almide"))
            .args(["build", source.to_str().expect("path"), "--target", target, "-o", artifact.to_str().expect("path")])
            .env_remove("ALMIDE_WASM_INCUMBENT").env_remove("ALMIDE_COMPONENT_P3")
            .output().expect("build");
        assert!(built.status.success(), "{}", String::from_utf8_lossy(&built.stderr));
        let mut command = if target == "rust" { Command::new(&artifact) } else {
            let mut c = Command::new("wasmtime"); c.arg("run").arg(&artifact); c
        };
        let out = command.output().expect("run");
        assert!(out.status.success(), "{}", String::from_utf8_lossy(&out.stderr));
        let actual = (out.stdout, out.stderr);
        if let Some(expected) = &expected { assert_eq!(&actual, expected); }
        else { expected = Some(actual); }
    }
}
