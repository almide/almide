//! #1537: a `guard … else err(…)` inside a statement-position `if` block of an
//! effect fn, followed by TWO var reassignments, was SILENTLY DROPPED on the
//! wasm leg — the module built v1-verified, the err never returned, the rest
//! of the block was skipped, and the function fell through to its `ok(…)`
//! (native returns the err; teastia 2.1.0's `aiapp.assemble` shipped the
//! wrong answer). The eating point was `lower_branch_arm_tail`'s deferred-
//! value fallthrough: the guard desugar rewrites the block to a UNIT `if`
//! whose else arm is the Result-typed err, and a Result-typed tail in an
//! EXECUTED unit arm has no value channel — `record_elided_calls` captured
//! nothing (the else carries no Call node) and emitted nothing.
//!
//! The assertion is deliberately fix-proof, like `wasm_validation_wall_test`:
//! EITHER the wasm build fails with an honest wall (today — the strict-mode
//! backstop refuses the channel-less Result tail), OR it succeeds and the
//! run answers the same err native does (when the executed-branch early
//! return lands). What may NEVER happen: a module that runs and answers
//! `ok …` — the silent fall-through this test exists to keep dead.

use std::process::Command;

fn almide() -> &'static str {
    env!("CARGO_BIN_EXE_almide")
}

const REPRO_1537: &str = r#"type Out = { items: List[String], status: String }
effect fn assemble(flag: String) -> Result[Out, String] = {
  var items: List[String] = ["a"]
  var status = "not queried"
  if flag != "" then {
    guard flag == "y" else err("no " + flag)
    items = ["b", "c"]
    status = "checked"
  } else ()
  ok(Out { items: items, status: status })
}
effect fn main() -> Unit = {
  match assemble("x") {
    ok(o) => println("ok " + o.status),
    err(e) => println("error: " + e),
  }
}
"#;

#[test]
fn guard_err_in_statement_if_never_falls_through_on_wasm() {
    let dir = tempfile::tempdir().expect("tempdir");
    let src = dir.path().join("repro.almd");
    let wasm = dir.path().join("repro.wasm");
    std::fs::write(&src, REPRO_1537).expect("write repro");

    let build = Command::new(almide())
        .args(["build", "--target", "wasm"])
        .arg(&src)
        .arg("-o")
        .arg(&wasm)
        .output()
        .expect("run almide build");

    if !build.status.success() {
        // Today's disposition: the honest wall. It must be a NAMED wall (the
        // retired-emitter framing), not a crash or an unnamed failure.
        let err = String::from_utf8_lossy(&build.stderr);
        assert!(
            err.contains("outside the MIR-lowering subset") || err.contains("wall"),
            "wasm build failed but not as a named wall:\n{err}"
        );
        assert!(
            !wasm.exists(),
            "a walled build must not leave a wasm artifact on disk"
        );
        return;
    }

    // Post-fix disposition: the module runs and answers native's err.
    let run = Command::new("wasmtime")
        .arg(&wasm)
        .output()
        .expect("run wasmtime");
    let out = String::from_utf8_lossy(&run.stdout);
    assert!(
        out.contains("error: no x"),
        "wasm leg must answer the guard's err exactly as native does; got:\n{out}"
    );
    assert!(
        !out.contains("ok "),
        "the guard's err fell through to ok — the #1537 silent-wrong shape is back:\n{out}"
    );
}
