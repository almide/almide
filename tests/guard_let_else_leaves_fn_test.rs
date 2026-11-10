//! The else of a `guard let` leaves the FUNCTION with its value, whatever the
//! else's shape and wherever the guard stands (#4655).
//!
//! The front end desugars `guard let` into a match whose failing arm is the
//! else. Where that match is the fn's value the arm is the fn's result; in a
//! branch arm of a statement (an `if`, a `match` arm, a loop body) the arm's
//! value was dropped and the fn ran on past the failed guard — unless the else
//! was a bare `err(e)`. An `ok(x)`, a `none`, a plain value, a Unit block, a
//! branching else and a block that logs before its `err(e)` all fell through
//! on wasm (exit 0, wrong output) and failed to build natively.
//!
//! One program holds every shape (`guard_let_else_leaves_fn/shapes.almd`, its
//! output beside it). It runs on wasm, on the native route and in the
//! interpreter; the three must print the same, expected text. The certified
//! native render does not lower an early exit from a branch arm that has code
//! after it, so the native leg is the standard codegen it falls back to —
//! which is why this is a test here and not a `spec/wasm_cross` fixture.
#![allow(dead_code)]
use std::process::Command;

include!("wasm_runtime_test_parts/interp_leg.rs");

const SHAPES: &str = include_str!("guard_let_else_leaves_fn/shapes.almd");
const SHAPES_STDOUT: &str = include_str!("guard_let_else_leaves_fn/shapes.stdout");

fn run(extra: &[&str]) -> (Option<i32>, String, String) {
    let dir = tempfile::tempdir().expect("scratch");
    let file = dir.path().join("shapes.almd");
    std::fs::write(&file, SHAPES).expect("source");
    let out = Command::new(env!("CARGO_BIN_EXE_almide")).arg("run").arg(&file).args(extra)
        .current_dir(dir.path()).output().expect("almide run");
    (out.status.code(), String::from_utf8_lossy(&out.stdout).into_owned(), String::from_utf8_lossy(&out.stderr).into_owned())
}

#[test]
fn every_else_shape_leaves_the_fn_on_wasm() {
    let (code, stdout, stderr) = run(&["--target", "wasm"]);
    assert_eq!(code, Some(0), "{stderr}");
    assert_eq!(stdout, SHAPES_STDOUT);
}

#[test]
fn every_else_shape_leaves_the_fn_on_native() {
    let (code, stdout, stderr) = run(&[]);
    assert_eq!(code, Some(0), "{stderr}");
    assert_eq!(stdout, SHAPES_STDOUT);
}

#[test]
fn every_else_shape_leaves_the_fn_in_the_interpreter() {
    match run_interp_capture(SHAPES) {
        InterpLeg::Ran(code, stdout, stderr) => {
            assert_eq!(code, 0, "{stderr}");
            assert_eq!(stdout, SHAPES_STDOUT.trim());
        }
        InterpLeg::Skip(why) => panic!("the interpreter did not run the program: {why}"),
    }
}
