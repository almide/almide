//! #3315: calling another module's function-valued top-level `let`
//! (`fnlet.thing(1)` where `fnlet` declares `let thing = inc1`) is an indirect
//! call through the let's value. It was lowered as a call to a function
//! `fnlet.thing` that does not exist, so both legs stopped with an IR verify
//! ICE after a clean `almide check`. `spec/integration/modules/
//! module_fn_value_let_test.almd` holds the shapes; the dependency-shaped leg
//! (`tests/dep_shaped_leg_test.rs`) re-homes `fnlet` as a path dependency.
//! `almide test` runs one leg, so this net runs the file on both.

use std::path::PathBuf;
use std::process::Command;

fn integration() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("spec/integration")
}

fn run_test(target: &str) -> (bool, String) {
    let o = Command::new(env!("CARGO_BIN_EXE_almide"))
        .current_dir(integration())
        .args(["test", "modules/module_fn_value_let_test.almd", "--target", target])
        .output()
        .expect("spawn almide test");
    (
        o.status.success(),
        format!("{}\n{}", String::from_utf8_lossy(&o.stdout), String::from_utf8_lossy(&o.stderr)),
    )
}

fn wasmtime_available() -> bool {
    Command::new("wasmtime").arg("--version").output().is_ok_and(|o| o.status.success())
}

#[cfg_attr(debug_assertions, ignore = "compiles the program on every leg (CI: release-shape job)")]
#[test]
fn a_module_fn_valued_let_is_called_through_its_value_on_every_leg() {
    let (ok, out) = run_test("rust");
    assert!(ok, "native leg failed:\n{out}");
    if !wasmtime_available() {
        eprintln!("wasmtime not on PATH — the wasm leg is skipped here (CI installs it)");
        return;
    }
    let (ok, out) = run_test("wasm");
    assert!(ok, "wasm leg failed:\n{out}");
}
