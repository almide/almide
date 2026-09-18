//! A module-level `List` whose initializer is computed from other such
//! globals and literals (`let DERIVED = BASE + ["c"]`) used to wall every fn
//! that read it on the incumbent wasm leg (#2274): only a CONST initializer
//! was materialized, and the wall named the reader, not the global. The
//! call-free shape now folds to the literal it denotes at lowering time (a
//! call-bearing initializer already took the startup-init route), and a shape
//! that cannot fold names what it is in the wall text.
use std::process::Command;

const FOLDS: &str = "let BASE: List[String] = [\"a\", \"b\"]\n\nlet DERIVED: List[String] = BASE + [\"c\"]\n\nlet TWICE: List[String] = DERIVED + DERIVED\n\nfn pick(x: String) -> List[String] = DERIVED |> list.filter((r) => r != x)\n\neffect fn main() -> Unit = {\n  let got = pick(\"a\")\n  println(list.join(got, \",\"))\n  println(int.to_string(list.len(TWICE)))\n}\n";

fn almide() -> String {
    std::env::var("ALMIDE_BIN").unwrap_or_else(|_| format!("{}/target/release/almide", env!("CARGO_MANIFEST_DIR")))
}

fn run_incumbent(dir: &std::path::Path, src: &str) -> (bool, String, String) {
    let file = dir.join("main.almd");
    std::fs::write(&file, src).unwrap();
    let out = Command::new(almide())
        .args(["run", file.to_str().unwrap(), "--target", "wasm"])
        .env("ALMIDE_WASM_INCUMBENT", "1")
        .output()
        .unwrap();
    (out.status.success(), String::from_utf8_lossy(&out.stdout).trim().to_string(), String::from_utf8_lossy(&out.stderr).to_string())
}

#[test]
fn a_call_free_computed_list_global_folds_and_runs_on_the_incumbent() {
    let dir = tempfile::tempdir().unwrap();
    let (ok, out, err) = run_incumbent(dir.path(), FOLDS);
    assert!(ok, "the incumbent must build the folded global:\n{err}");
    assert_eq!(out, "b,c\n6");
    let native = Command::new(almide()).args(["run", dir.path().join("main.almd").to_str().unwrap()]).output().unwrap();
    assert_eq!(String::from_utf8_lossy(&native.stdout).trim(), out, "native and incumbent must agree");
}
