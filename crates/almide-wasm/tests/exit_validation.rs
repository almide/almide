//! E083 (#1996): the build validates each function's emitted exit
//! operations against its checked ExitPlan, and a mismatch is a compiler
//! defect with its own diagnostic — never a wall, never a source error.
//!
//! The negative half uses the emitter's test hook `test_omit_first_release`
//! (exit_plan.rs `emit_exit` skips the first release of every plan): the
//! validator must then name the local whose release is missing. Both halves
//! run in ONE test so the process-wide switch cannot race a sibling.

const SRC: &str = r#"fn helper(n: Int) -> Int = {
  let xs = [n, n, n]
  list.len(xs)
}

fn main() -> Unit = println("${helper(3)}")
"#;

/// Two functions that end in a call of each other: each exit is a
/// `return_call`, an explicit transfer in the bytes.
const TAIL_SRC: &str = r#"fn pong(n: Int) -> Int = if n == 0 then 0 else ping(n - 1)

fn ping(n: Int) -> Int = if n == 0 then 1 else pong(n - 1)

fn main() -> Unit = println("${ping(4)}")
"#;

fn emit_src(src: &str) -> Result<Vec<u8>, almide_wasm::EmitError> {
    let ir = almide_spine::s5::lower_to_ir("exit_validation.almd", src).expect("front");
    almide_wasm::emit_program(&ir)
}

fn emit() -> Result<Vec<u8>, almide_wasm::EmitError> {
    emit_src(SRC)
}

#[test]
fn an_exit_that_skips_a_planned_release_is_a_compiler_defect_named_by_local() {
    // Positive control: the honest emitter passes its own validator.
    emit().expect("the unmutated emitter implements every plan");

    almide_wasm::test_omit_first_release(true);
    let res = emit();
    almide_wasm::test_omit_first_release(false);
    let Err(almide_wasm::EmitError::OwnershipLowering(d)) = res else {
        panic!("a skipped release must be an E083 defect, got {res:?}");
    };
    let text = d.to_string();
    assert!(text.starts_with("error[E083]: an exit leaves a released credit undecremented"), "{text}");
    assert!(text.contains("--> fn `helper`"), "{text}");
    assert!(text.contains("= value: local `xs`"), "{text}");
    assert!(text.contains("= expected: release before the epilogue return"), "{text}");
    assert!(text.contains("= emitted: none"), "{text}");
    assert!(text.contains("compiler contract failure; no artifact emitted"), "{text}");
    // Never a wall: the reason text must not read like an Unsupported shape.
    assert!(!text.contains("wall"), "{text}");

    // The other direction (#4854): a transfer the bytes hold that no plan
    // recorded. The honest emitter records every exit it writes.
    emit_src(TAIL_SRC).expect("the unmutated emitter records every exit");
    almide_wasm::test_omit_exit_record(true);
    let res = emit_src(TAIL_SRC);
    almide_wasm::test_omit_exit_record(false);
    let Err(almide_wasm::EmitError::OwnershipLowering(d)) = res else {
        panic!("an unrecorded exit must be an E083 defect, got {res:?}");
    };
    let text = d.to_string();
    assert!(text.starts_with("error[E083]: an exit in the function's bytes has no exit plan"), "{text}");
    assert!(text.contains("= emitted: a return_call no plan claims") || text.contains("= emitted: a return no plan claims"), "{text}");
}
