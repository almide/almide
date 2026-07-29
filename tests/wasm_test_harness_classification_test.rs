//! The standalone wasm test harness classifies per CAUSE (#957).
//!
//! A SKIP means "the program is correct but outside the verified renderer's
//! subset" — an honest wall. A compile error is neither: the file is broken
//! on EVERY target. Before this lock, `almide test broken.almd --target wasm`
//! reported `0 passed, 0 failed, 1 skipped` and exited 0 — a wasm-leg suite
//! ran green over files that did not even type-check, and a typo'd module fn
//! read as "wall brick not yet supported" instead of "your test is wrong".
//!
//! The locks:
//!   1. a type error in the entry file    → FAIL, exit 1, never a skip
//!   2. a type error in an imported module → FAIL, exit 1, never a skip
//!   3. `// wasm:skip` (a genuine skip)    → SKIP, exit 0
//!
//! Skips cleanly when the `almide` binary is unavailable (the release binary
//! is built by CI's build step; locally run `make install` / `cargo build
//! --release` first). wasmtime is NOT required: every case here resolves
//! before execution.

use std::path::Path;
use std::process::Command;

fn almide_bin() -> String {
    if let Ok(bin) = std::env::var("ALMIDE_BIN") {
        return bin;
    }
    let cargo_bin = Path::new(env!("CARGO_MANIFEST_DIR")).join("target/release/almide");
    if cargo_bin.exists() {
        return cargo_bin.to_str().unwrap().to_string();
    }
    "almide".to_string()
}

fn tool_available() -> bool {
    Command::new(almide_bin()).arg("--version").output().is_ok()
}

/// (stderr, exit_code) of `almide test <file> --target wasm` — the harness
/// reports on stderr.
fn wasm_test(file: &Path) -> (String, i32) {
    let output = Command::new(almide_bin())
        .args(["test", file.to_str().unwrap(), "--target", "wasm"])
        .output()
        .expect("failed to spawn almide");
    (
        String::from_utf8_lossy(&output.stderr).to_string(),
        output.status.code().unwrap_or(-1),
    )
}

fn scratch_dir(name: &str) -> std::path::PathBuf {
    let dir = std::env::temp_dir().join(format!("almide_957_{}_{}", name, std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

#[test]
fn entry_type_error_is_a_fail_not_a_skip() {
    if !tool_available() {
        return;
    }
    let dir = scratch_dir("entry");
    let file = dir.join("e002_test.almd");
    std::fs::write(&file, "test \"calls an undefined fn\" {\n  assert_eq(string.no_such_fn(\"x\"), 1)\n}\n").unwrap();

    let (stderr, code) = wasm_test(&file);
    assert_eq!(code, 1, "a compile error must fail the wasm leg, got exit {}: {}", code, stderr);
    assert!(stderr.contains("FAIL"), "verdict must be FAIL: {}", stderr);
    assert!(stderr.contains("no_such_fn"), "the diagnostic must name the cause: {}", stderr);
    assert!(!stderr.contains("skipped"), "the skip ledger must never absorb diagnostics: {}", stderr);
    std::fs::remove_dir_all(&dir).ok();
}

#[test]
fn imported_module_type_error_is_a_fail_not_a_skip() {
    if !tool_available() {
        return;
    }
    let dir = scratch_dir("module");
    std::fs::write(dir.join("helper.almd"), "pub fn broken() -> Int = \"not an int\"\n").unwrap();
    let entry = dir.join("entry_test.almd");
    std::fs::write(&entry, "import helper\n\ntest \"uses broken module\" {\n  assert_eq(helper.broken(), 1)\n}\n").unwrap();

    let (stderr, code) = wasm_test(&entry);
    assert_eq!(code, 1, "an imported module's compile error must fail the wasm leg, got exit {}: {}", code, stderr);
    assert!(stderr.contains("FAIL"), "verdict must be FAIL: {}", stderr);
    assert!(!stderr.contains("skipped"), "the skip ledger must never absorb diagnostics: {}", stderr);
    std::fs::remove_dir_all(&dir).ok();
}

#[test]
fn wasm_skip_marker_stays_a_green_skip() {
    if !tool_available() {
        return;
    }
    let dir = scratch_dir("marker");
    let file = dir.join("marked_test.almd");
    std::fs::write(&file, "// wasm:skip\ntest \"marked skip\" {\n  assert_eq(1, 1)\n}\n").unwrap();

    let (stderr, code) = wasm_test(&file);
    assert_eq!(code, 0, "a genuine skip must stay green, got exit {}: {}", code, stderr);
    assert!(stderr.contains("SKIP"), "verdict must be SKIP: {}", stderr);
    assert!(stderr.contains("1 skipped"), "the skip must be counted: {}", stderr);
    std::fs::remove_dir_all(&dir).ok();
}
