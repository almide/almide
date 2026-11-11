//! The else of a `guard let` in a block bound by a `let` / `var`, or assigned to
//! a variable, returns from the FUNCTION (ALS-ST6), as it does at every other
//! position.
//!
//! Every leg had made the else's value the block's value and carried on past
//! the bind, or refused the program when the else had the fn's type and not
//! the block's. The front end now opens such a block into the enclosing one,
//! where the guard is the statement each leg already runs.
//!
//! The program is the cross-target fixture; here it is held to the expected
//! text on wasm, in the interpreter, and on the certified native render under
//! the ownership certificate with no fallback to the standard codegen.
#![allow(dead_code)]
use std::process::Command;

include!("wasm_runtime_test_parts/interp_leg.rs");

const PROGRAM: &str = include_str!("../spec/wasm_cross/guard_let_in_bound_block.almd");
const EXPECTED: &str = "bound 42\nok 43\nerr bad input\nok 15\nerr relabel: not positive\nhalf+1 = 5\nodd\n14\nleaving\n-7\nvalue 204\ninner\nouter\n8\n50\n-1\n";

#[test]
fn the_else_leaves_the_fn_on_wasm_and_the_certified_native_render() {
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
        assert_eq!(String::from_utf8_lossy(&out.stdout), EXPECTED, "{extra:?}");
    }
}

#[test]
fn the_else_leaves_the_fn_in_the_interpreter() {
    match run_interp_capture(PROGRAM) {
        InterpLeg::Ran(code, stdout, stderr) => {
            assert_eq!(code, 0, "{stderr}");
            assert_eq!(stdout, EXPECTED.trim());
        }
        InterpLeg::Skip(why) => panic!("the interpreter did not run the program: {why}"),
    }
}
