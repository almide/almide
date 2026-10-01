//! #3164: a top-level `let` that names a LATER top-level `let`, read by a
//! function declared above both. Registration seeds the lets in source order,
//! so the function used to read `PAIRS`'s seed `List[(String, Unknown)]`; its
//! loop variable stayed partly Unknown and `run` / `build --release` panicked
//! in `ConcretizeTypes` behind a green `almide check`.
//!
//! The spec fixtures (spec/lang/top_let_forward_reference_test.almd and its
//! module twin) keep their initializers literal, because the v1 lowering walls
//! a heap global with a COMPUTED initializer; this net runs the computed chain
//! — the issue's own shape, `let TEXT = greet(..)` read through a list of
//! pairs — on every leg, from a scratch directory outside the spec corpus.

use std::path::PathBuf;
use std::process::Command;

const PROGRAM: &str = r#"
fn install() -> String = {
  var out = ""
  for f in PAIRS {
    out = out + f.0 + "=" + f.1 + ";"
  }
  out
}

fn total() -> Int = list.fold(TABLE, 0, (acc, p) => acc + p.0 + string.len(p.1))

let PAIRS = [("a", TEXT), ("b", TEXT + "!")]

let TABLE = [(1, WORD)]

let WORD = TEXT + "x"

let TEXT = greet("hello")

fn greet(s: String) -> String = s + "-"

effect fn main() -> Unit = {
  println(install())
  println(int.to_string(total()))
}
"#;

const EXPECTED: &str = "a=hello-;b=hello-!;\n8";

fn scratch() -> PathBuf {
    let dir = std::env::temp_dir().join(format!("almide-3164-{}", std::process::id()));
    std::fs::create_dir_all(&dir).expect("scratch dir");
    std::fs::write(dir.join("main.almd"), PROGRAM).expect("write program");
    dir
}

fn run(dir: &PathBuf, target: &str) -> (bool, String, String) {
    let o = Command::new(env!("CARGO_BIN_EXE_almide"))
        .current_dir(dir)
        .args(["run", "main.almd", "--target", target])
        .output()
        .expect("spawn almide run");
    (o.status.success(), String::from_utf8_lossy(&o.stdout).to_string(), String::from_utf8_lossy(&o.stderr).to_string())
}

fn wasmtime_available() -> bool {
    Command::new("wasmtime").arg("--version").output().is_ok_and(|o| o.status.success())
}

#[cfg_attr(debug_assertions, ignore = "compiles the program on every leg (CI: release-shape job)")]
#[test]
fn a_fn_above_a_computed_let_chain_runs_on_every_leg() {
    let dir = scratch();
    let (ok, out, err) = run(&dir, "rust");
    assert!(ok, "native run failed:\n{err}");
    assert_eq!(out.trim(), EXPECTED, "native leg");
    if wasmtime_available() {
        let (ok, out, err) = run(&dir, "wasm");
        assert!(ok, "wasm run failed:\n{err}");
        assert_eq!(out.trim(), EXPECTED, "wasm leg");
    } else {
        eprintln!("wasmtime not on PATH — the wasm leg is skipped here (CI installs it)");
    }
    let _ = std::fs::remove_dir_all(&dir);
}
