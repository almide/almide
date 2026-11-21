//! #4854: every exit the emitted bytes hold is one an ExitPlan accounts for.
//! The emitter's test hook writes the exits of a fresh function but records
//! no plan for them; the validator must refuse the function (E083) instead of
//! judging only the windows it was told about. Row `witness.e083-exit-validator`.

#[path = "../../../tests/support/novel.rs"]
mod novel;

fn emit(src: &str) -> Result<Vec<u8>, almide_wasm::EmitError> {
    let ir = almide_spine::s5::lower_to_ir("exit_unrecorded.almd", src).expect("front");
    almide_wasm::emit_program(&ir)
}

#[test]
fn novel_an_exit_no_plan_records_is_refused() {
    let f = novel::ident("parse_or_fail");
    let src = format!(
        "fn {f}(s: String) -> Result[Int, String] = {{\n  let xs = [s, s]\n  let n = int.parse(s)!\n  ok(n + list.len(xs))\n}}\n\nfn main() -> Unit = println(\"${{{f}(\\\"4\\\") ?? 0}}\")\n"
    );
    emit(&src).expect("control: the honest emitter records every exit");
    almide_wasm::test_omit_exit_record(true);
    let res = emit(&src);
    almide_wasm::test_omit_exit_record(false);
    let Err(almide_wasm::EmitError::OwnershipLowering(d)) = res else {
        panic!("an exit no plan records must be an E083 defect, got {res:?}");
    };
    let text = d.to_string();
    assert!(text.starts_with("error[E083]: an exit in the function's bytes has no exit plan"), "{text}");
    assert!(text.contains(&f), "{text}");
}
