//! Pin for `io.print`: raw stdout append (no newline), interleaving with
//! `println` in PROGRAM order, and the always-ok unit carrier under `!`.
//! No corpus fixture exercises it (the DDD gauntlet found the wall,
//! 2026-08-26), so this is the witness.

mod harness;
use harness::run_wasm;

const SRC: &str = r#"import io

effect fn main() -> Unit = {
  io.print("a")
  io.print("${1 + 1}")!
  println("|")
  io.print("tail")
}
"#;

#[test]
fn io_print_appends_raw_in_program_order() {
    let ir = almide_spine::s5::lower_to_ir("io_print.almd", SRC).expect("lowers");
    let bytes = almide_wasm::emit_program(&ir).expect("emits");
    let run = run_wasm(&bytes).expect("runs");
    assert_eq!(run.exit, 0, "stderr: {}", run.stderr);
    let wasm_out = run.stdout;

    // The interpreter currently ABSTAINS on this prim (prim.fd_write is
    // not implemented on that leg), so the pin's authority is the
    // explicit expectation. If the interp grows the prim, it must agree.
    let interp = almide_spine::s5::run_file("io_print.almd", SRC).expect("interp runs");
    if interp.exit == 0 {
        assert_eq!(wasm_out, interp.stdout, "io.print diverged from the oracle");
    } else {
        assert!(
            interp.stderr.contains("fd_write"),
            "interp failed for a NEW reason (not the known fd_write abstention): {}",
            interp.stderr
        );
    }
    assert_eq!(wasm_out, "a2|\ntail");
}
