//! #3290: an anonymous record literal bound by an UN-annotated `let` takes the
//! nominal record its binding is later unified with (returned, passed, stored,
//! chosen by an `if`). The checker typed the literal structurally, so with two
//! modules declaring the same field set the binding took the FIRST one on both
//! legs: native rustc E0308, wasm wall E082. `spec/integration/
//! record_literal_binding` holds the package shapes and `spec/integration/
//! modules/record_literal_binding_test.almd` the two-library shape (two path
//! dependencies on the dependency-shaped leg). `almide test` runs one leg, so
//! this net runs everything on both and demands one output.

use std::path::PathBuf;
use std::process::Command;

const EXPECTED: &str = "1.0
5.0
6.0
2
7.0
6.0
Extent { w: 2, h: 1 }
27.0";

fn integration() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("spec/integration")
}

fn almide(cwd: PathBuf, args: &[&str]) -> (bool, String, String) {
    let o = Command::new(env!("CARGO_BIN_EXE_almide"))
        .current_dir(cwd)
        .args(args)
        .output()
        .expect("spawn almide");
    (
        o.status.success(),
        String::from_utf8_lossy(&o.stdout).to_string(),
        String::from_utf8_lossy(&o.stderr).to_string(),
    )
}

fn targets() -> Vec<&'static str> {
    let wasm = Command::new("wasmtime").arg("--version").output().is_ok_and(|o| o.status.success());
    if wasm {
        vec!["rust", "wasm"]
    } else {
        eprintln!("wasmtime not on PATH — the wasm leg is skipped here (CI installs it)");
        vec!["rust"]
    }
}

#[cfg_attr(debug_assertions, ignore = "compiles the program on every leg (CI: release-shape job)")]
#[test]
fn a_literal_binding_takes_the_nominal_type_of_its_use_on_every_leg() {
    for target in targets() {
        let pkg = integration().join("record_literal_binding");
        let (ok, out, err) = almide(pkg.clone(), &["run", "src/main.almd", "--target", target]);
        assert!(ok, "{target} run failed:\n{err}");
        assert_eq!(out.trim(), EXPECTED, "{target} leg");
        let (ok, out, err) = almide(pkg, &["test", "src/record_literal_binding_test.almd", "--target", target]);
        assert!(ok, "{target} package test failed:\n{out}\n{err}");
        let (ok, out, err) = almide(integration(), &["test", "modules/record_literal_binding_test.almd", "--target", target]);
        assert!(ok, "{target} library test failed:\n{out}\n{err}");
    }
}
