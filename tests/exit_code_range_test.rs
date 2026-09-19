//! C-350: `process.exit(code)` accepts 0..=125 on every target, and any other
//! code is a defined abort — one stderr line and exit 1, byte-identical.
//!
//! The two `spec/wasm_cross` fixtures carry the promise across the stock WASI
//! runtime, which is where it was broken (#2303). This test carries the values
//! a fixture cannot: one program per code, over the whole boundary, including
//! the two that used to be answered WRONG rather than refused —
//!
//! | code | native before | stock runtime before | every target now |
//! |---|---|---|---|
//! | 125 | 125 | 125 | 125 |
//! | 126 | 126 | 1 + a host error | abort |
//! | 200 | 200 | 1 + a host error | abort |
//! | 256 | **0** (POSIX keeps 8 bits) | 1 + a host error | abort |
//! | -1 | **255** (same) | 1 + a host error | abort |
//!
//! The code is read through an `effect fn` so the folder cannot see it: a
//! constant takes a different path (#1117), and then the test would pass with
//! the rule absent.
use std::process::Command;

const MSG: &str = "Error: exit code must be in 0..=125";

fn almide() -> String {
    std::env::var("ALMIDE_BIN")
        .unwrap_or_else(|_| format!("{}/target/release/almide", env!("CARGO_MANIFEST_DIR")))
}

fn program(code: i64) -> String {
    format!(
        "import process\n\n\
         effect fn chosen() -> Int = {code}\n\n\
         effect fn main() -> Unit = {{\n  \
           println(\"before-exit\")\n  \
           let c = chosen()!\n  \
           process.exit(c)\n\
         }}\n"
    )
}

/// (exit code, stdout, stderr) on one leg.
fn run(dir: &std::path::Path, src: &str, wasm: bool) -> (i32, String, String) {
    let file = dir.join("main.almd");
    std::fs::write(&file, src).unwrap();
    let mut cmd = Command::new(almide());
    cmd.args(["run", file.to_str().unwrap()]);
    if wasm {
        cmd.args(["--target", "wasm"]);
    }
    let out = cmd.output().unwrap();
    (
        out.status.code().unwrap_or(-1),
        String::from_utf8_lossy(&out.stdout).trim().to_string(),
        String::from_utf8_lossy(&out.stderr).trim().to_string(),
    )
}

/// Both legs, asserted equal to each other and to the expectation.
fn both_legs(code: i64, want_exit: i32, want_err: &str) {
    let dir = tempfile::tempdir().unwrap();
    let src = program(code);
    let native = run(dir.path(), &src, false);
    let wasm = run(dir.path(), &src, true);
    assert_eq!(native, wasm, "process.exit({code}): the legs must agree");
    assert_eq!(
        native,
        (want_exit, "before-exit".to_string(), want_err.to_string()),
        "process.exit({code})"
    );
}

#[test]
fn a_code_inside_the_range_is_the_exit_code_on_every_leg() {
    for code in [0, 1, 3, 124, 125] {
        both_legs(code, code as i32, "");
    }
}

#[test]
fn a_code_outside_the_range_aborts_identically_on_every_leg() {
    // 126 is the first refused code, 200 the issue's own repro, and 256 / -1
    // are the two POSIX already answered wrong instead of refusing.
    for code in [126, 127, 128, 200, 255, 256, -1] {
        both_legs(code, 1, MSG);
    }
}

/// The abort is reached through the ordinary termination convention: stdout
/// written before the call still arrives, and nothing after it runs.
#[test]
fn the_abort_keeps_the_output_written_before_it() {
    let dir = tempfile::tempdir().unwrap();
    let src = "import process\n\n\
               effect fn chosen() -> Int = 200\n\n\
               effect fn main() -> Unit = {\n  \
                 println(\"one\")\n  \
                 println(\"two\")\n  \
                 let c = chosen()!\n  \
                 process.exit(c)\n  \
               }\n";
    let (exit, out, err) = run(dir.path(), src, false);
    assert_eq!((exit, out.as_str(), err.as_str()), (1, "one\ntwo", MSG));
}
