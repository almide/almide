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
//! interpreter; the three must print the same, expected text.
//!
//! The certified native render lowers every one of these exits (#4640): the
//! branch statement becomes the block's tail with the rest of the block in each
//! arm that carries on, an exit that can only raise leaves a loop as a
//! propagation, and a value leaves a loop through the loop early-return form.
//! The last test holds that render to the same text under the ownership
//! certificate, with no fallback to the standard codegen.
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

/// The program for the certified native render: `SHAPES` with the three
/// `show_unit(unit_arm_ok(..))` lines of `main` spelled as a propagated call
/// and a literal `ok` line. `unit_arm_ok` can never err, and its Unit result
/// passed as a `Result` ARGUMENT is a shape that render refuses in `main` on
/// its own, with or without a guard; the fn under test and the text printed
/// are the same.
fn certified_shapes() -> String {
    let mut replaced = 0;
    let lines: Vec<String> = SHAPES
        .lines()
        .map(|l| match l.trim().strip_prefix("println(show_unit(unit_arm_ok(").and_then(|r| r.strip_suffix(")))")) {
            Some(args) => {
                replaced += 1;
                format!("  unit_arm_ok({args})!\n  println(\"ok\")")
            }
            None => l.to_string(),
        })
        .collect();
    assert_eq!(replaced, 3, "the three unit_arm_ok lines of main");
    lines.join("\n") + "\n"
}

#[test]
fn every_else_shape_renders_on_the_certified_native_render() {
    let dir = tempfile::tempdir().expect("scratch");
    let file = dir.path().join("shapes.almd");
    std::fs::write(&file, certified_shapes()).expect("source");
    for extra in [&[][..], &["--target", "wasm"][..]] {
        let out = Command::new(env!("CARGO_BIN_EXE_almide")).arg("run").arg(&file).args(extra)
            .env("ALMIDE_CERTIFY_OWNERSHIP", "fail").env("ALMIDE_VERBOSE", "1")
            .current_dir(dir.path()).output().expect("almide run");
        let stderr = String::from_utf8_lossy(&out.stderr);
        assert_eq!(out.status.code(), Some(0), "{extra:?}: {stderr}");
        assert!(!stderr.contains("walled"), "{extra:?} fell back to the standard codegen: {stderr}");
        assert_eq!(String::from_utf8_lossy(&out.stdout), SHAPES_STDOUT, "{extra:?}");
    }
}
