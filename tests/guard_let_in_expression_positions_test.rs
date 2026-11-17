//! A failing `guard let` returns from the enclosing FUNCTION in every
//! expression position (#4678, ALS-ST6 / C-266). A lambda body is the one
//! boundary: there the else returns from the lambda.
//!
//! Before #4678 a block whose value an enclosing expression used — an arm of
//! a bound `if` / `match`, a condition, a call argument, an operand, an
//! element, an interpolation part, a range end — made the else its value, and
//! the fn carried on. The front end now sends that else through the guard's
//! return channel, and the certified native render hoists the value out of
//! its expression so the exit stands on the fn's tail chain.
//!
//! The program is the cross-target fixture, one fn per position, run for a
//! succeeding (5) and a failing (-5) input. Each position is held on wasm, on
//! the certified native render under the ownership certificate with no
//! fallback to the standard codegen, and in the interpreter.
#![allow(dead_code)]
use std::process::Command;

include!("wasm_runtime_test_parts/interp_leg.rs");

const PROGRAM: &str = include_str!("../spec/wasm_cross/guard_let_in_expression_positions.almd");

/// `(position, what n = 5 prints after "after <position>", what n = -5 prints)`.
/// A position that returns prints no "after" line on the failing input.
const POSITIONS: &[(&str, &str, &str)] = &[
    ("if_then_arm", "60", "-1"),
    ("if_else_arm", "60", "-1"),
    ("match_arm", "60", "-1"),
    ("if_cond", "10", "-1"),
    ("call_single", "60", "-1"),
    ("call_second", "70", "-1"),
    ("call_nested", "70", "-1"),
    ("bin_left", "70", "-1"),
    ("bin_right", "70", "-1"),
    ("list_elem", "2", "-1"),
    ("tuple_elem", "6", "-1"),
    ("record_field", "6", "-1"),
    ("interp", "v=5", "none"),
    ("for_head", "10", "-1"),
    ("while_cond", "5", "-1"),
    ("opt_arg", "5", "-1"),
    ("raise_arg", "ok 6", "err raised"),
];

fn expected() -> String {
    let mut ok = String::from("-- 5\n");
    let mut fail = String::from("-- -5\n");
    for (name, value, left) in POSITIONS {
        ok.push_str(&format!("after {name}\n{value}\n"));
        fail.push_str(&format!("{left}\n"));
    }
    // The lambda's else is the lambda's result; its fn carries on.
    ok.push_str("after lambda_body\n60\n");
    fail.push_str("after lambda_body\n-10\n");
    ok + &fail
}

/// Name the first position whose lines differ, so a regression reads as one.
fn check(leg: &str, got: &str) {
    let want = expected();
    if got == want {
        return;
    }
    let failing = got.split("-- -5\n").nth(1).unwrap_or("");
    for (name, _, _) in POSITIONS {
        assert!(!failing.contains(&format!("after {name}\n")), "{leg}: a failing guard let in `{name}` did not return from the fn:\n{got}");
    }
    assert_eq!(got, want, "{leg}");
}

#[test]
fn every_position_returns_on_wasm_and_the_certified_native_render() {
    let dir = tempfile::tempdir().expect("scratch");
    let file = dir.path().join("prog.almd");
    std::fs::write(&file, PROGRAM).expect("source");
    for extra in [&[][..], &["--target", "wasm"][..]] {
        let out = Command::new(env!("CARGO_BIN_EXE_almide")).arg("run").arg(&file).args(extra)
            .env("ALMIDE_CERTIFY_OWNERSHIP", "fail").env("ALMIDE_VERBOSE", "1")
            .current_dir(dir.path()).output().expect("almide run");
        let stderr = String::from_utf8_lossy(&out.stderr);
        assert_eq!(out.status.code(), Some(0), "{extra:?}: {stderr}");
        assert!(!stderr.contains("walled"), "{extra:?} fell back to the standard codegen: {stderr}");
        check(&format!("{extra:?}"), &String::from_utf8_lossy(&out.stdout));
    }
}

#[test]
fn every_position_returns_in_the_interpreter() {
    match run_interp_capture(PROGRAM) {
        InterpLeg::Ran(code, stdout, stderr) => {
            assert_eq!(code, 0, "{stderr}");
            check("interp", &format!("{stdout}\n"));
        }
        InterpLeg::Skip(why) => panic!("the interpreter did not run the program: {why}"),
    }
}
