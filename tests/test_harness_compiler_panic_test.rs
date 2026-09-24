//! #2533 item 3: a file whose compile PANICS inside the compiler must be
//! counted as a failed test file. The harness's worker thread used to die
//! without sending a result, so the file dropped out of the tally: the run
//! printed "All 1 test file(s) passed" / "0 tests in 1 file" and exited 5
//! (the no-tests verdict) instead of reporting a failure.
//!
//! `ALMIDE_IR_FAULT=<pass>` injects a postcondition violation after the named
//! nanopass (the release binary's negative control), which is a real compiler
//! panic on the native leg without depending on a live compiler bug.

use std::process::Command;

fn almide() -> &'static str {
    env!("CARGO_BIN_EXE_almide")
}

#[test]
fn a_compiler_panic_is_a_failed_test_file_not_a_missing_one() {
    let dir = std::env::temp_dir().join(format!("almide-2533-panic-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let file = dir.join("panics_test.almd");
    std::fs::write(&file, "test \"t\" {\n  assert_eq(1 + 1, 2)\n}\n").unwrap();

    let out = Command::new(almide())
        .args(["test", file.to_str().unwrap(), "--target", "rust"])
        .env("ALMIDE_IR_FAULT", "ConcretizeTypes")
        .output()
        .expect("run almide test");
    let stderr = String::from_utf8_lossy(&out.stderr);
    let _ = std::fs::remove_dir_all(&dir);

    assert_eq!(out.status.code(), Some(1), "a panicking file must fail the run with exit 1:\n{stderr}");
    assert!(!stderr.contains("passed"), "a panicking file must not be reported as passed:\n{stderr}");
    assert!(stderr.contains("1/1 test file(s) failed"), "the panicking file must be counted:\n{stderr}");
    assert!(stderr.contains("the compiler panicked"), "the failure must say the compiler panicked:\n{stderr}");
}
