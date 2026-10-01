//! #3165: a record literal in one module that omits a field whose default the
//! declaring module wrote over its own names. `spec/integration/
//! cross_module_field_defaults` holds every shape: a default naming the
//! module's constant, calling its fn, naming another module's constant through
//! the declaring module's import alias (which the caller binds to a different
//! module), a caller local spelled like the default's constant, a
//! record-payload case, and literals inside the declaring module beside
//! same-named locals, with a default calling the module's fn (#3167). Before
//! the fix the native leg failed with rustc E0425 (or
//! silently read a caller local) and the wasm leg walled on a type mismatch.
//! `almide test` runs one leg, so this net runs the project's `src/main.almd`
//! on both and demands one output.

use std::path::PathBuf;
use std::process::Command;

const EXPECTED: &str = "3 6 40 5 55
9 62
21 3 6 5
58 4
5 41 999 100 7";

fn project() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("spec/integration/cross_module_field_defaults")
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
fn a_cross_module_field_default_resolves_in_its_declaring_module_on_every_leg() {
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
