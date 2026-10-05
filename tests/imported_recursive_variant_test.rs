//! #3422: an entry program matching a recursive variant declared in another
//! module. Box-deref resolved the entry's constructor patterns against the
//! entry's own type declarations only, so `Node(l, _)` on an imported `T` never
//! marked `l` as a box binder and rustc refused `cur = l` (E0308, `T` vs
//! `Box<T>`). A module function's match already resolved against every
//! module's decls, which is why `almide build` of a program whose match lives
//! in a module worked while `almide test` of that file (the test file is the
//! entry) and `almide run` of a matching `main` did not.
//! `spec/integration/imported_recursive_variant` holds the shapes: selective
//! and qualified imports, statement- and expression-position matches, a payload
//! through `List[R]`, and a test block in the declaring module itself.

use std::path::PathBuf;
use std::process::Command;

const EXPECTED: &str = "a 3 3";

fn project() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("spec/integration/imported_recursive_variant")
}

fn almide(args: &[&str]) -> (bool, String, String) {
    let o = Command::new(env!("CARGO_BIN_EXE_almide"))
        .current_dir(project())
        .args(args)
        .output()
        .expect("spawn almide");
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
fn an_imported_recursive_variant_matches_in_the_native_test_lane() {
    for file in ["src/walk_test.almd", "src/tree.almd"] {
        let (ok, out, err) = almide(&["test", file, "--target", "rust"]);
        assert!(ok, "native `almide test {file}` failed:\n{out}\n{err}");
    }
}

#[cfg_attr(debug_assertions, ignore = "compiles the program on every leg (CI: release-shape job)")]
#[test]
fn an_entry_main_matching_an_imported_recursive_variant_runs_on_every_leg() {
    let (ok, out, err) = almide(&["run", "src/main.almd", "--target", "rust"]);
    assert!(ok, "native run failed:\n{err}");
    assert_eq!(out.trim(), EXPECTED, "native leg");
    if !wasmtime_available() {
        eprintln!("wasmtime not on PATH — the wasm leg is skipped here (CI installs it)");
        return;
    }
    let (ok, out, err) = almide(&["run", "src/main.almd", "--target", "wasm"]);
    assert!(ok, "wasm run failed:\n{err}");
    assert_eq!(out.trim(), EXPECTED, "wasm leg");
}
