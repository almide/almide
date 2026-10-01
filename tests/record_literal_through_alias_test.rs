//! #3153: a record literal written through a cross-module type alias builds
//! the record the alias names. `spec/integration/alias_record_literal` holds
//! both broken shapes: `term.T { n: 3 }` through `type T = state.T`, which the
//! checker typed as the alias and then reported E013 "no fields" at the first
//! field access, and `render.Bar { … }` in a sibling submodule through `type
//! Bar = findbar.Bar`, which passed the checker and lowered to a bare `Bar`
//! the generated Rust does not declare (rustc E0422). The second shape fails
//! only on the native leg, and `almide test` runs the wasm leg first, so this
//! net runs the project's `src/main.almd` on both legs and demands one output.

use std::path::PathBuf;
use std::process::Command;

const EXPECTED: &str = "3 4 d
4 3 d
9 6 r
6 5 7
8
x
y:false";

fn project() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("spec/integration/alias_record_literal")
}

fn run(target: &str) -> (bool, String, String) {
    let o = Command::new(env!("CARGO_BIN_EXE_almide"))
        .current_dir(project())
        .args(["run", "src/main.almd", "--target", target])
        .output()
        .expect("spawn almide run");
    (
        o.status.success(),
        String::from_utf8_lossy(&o.stdout).to_string(),
        String::from_utf8_lossy(&o.stderr).to_string(),
    )
}

fn wasmtime_available() -> bool {
    Command::new("wasmtime").arg("--version").output().is_ok_and(|o| o.status.success())
}

#[cfg_attr(debug_assertions, ignore = "compiles the program on every leg (CI: release-shape job)")]
#[test]
fn a_record_literal_through_an_alias_builds_the_record_on_every_leg() {
    let (ok, out, err) = run("rust");
    assert!(ok, "native run failed:\n{err}");
    assert_eq!(out.trim(), EXPECTED, "native leg");
    if !wasmtime_available() {
        eprintln!("wasmtime not on PATH — the wasm leg is skipped here (CI installs it)");
        return;
    }
    let (ok, out, err) = run("wasm");
    assert!(ok, "wasm run failed:\n{err}");
    assert_eq!(out.trim(), EXPECTED, "wasm leg");
}
