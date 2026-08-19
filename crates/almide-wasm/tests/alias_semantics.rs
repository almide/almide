//! The dedicated referee for List VALUE SEMANTICS in the wasm backend.
//!
//! The corpus's alias fixtures (alias_cow etc.) are still behind refused
//! walls, so the burn-up gate does not yet witness the bind-deep-copy
//! invariant — a mutation that drops `$block_copy` at binds stayed green
//! against the corpus (2026-08-19). This test IS the witness until those
//! fixtures become claimable: a bound alias must not observe a later
//! in-place push.

mod harness;
use harness::run_wasm;

const SRC: &str = r#"fn main() -> Unit = {
  var xs: List[Int] = []
  list.push(xs, 1)
  let ys = xs
  list.push(xs, 2)
  println(int.to_string(list.len(ys)))
  println(int.to_string(list.len(xs)))
}
"#;

#[test]
fn bound_alias_does_not_observe_in_place_push() {
    let ir = almide_spine::s5::lower_to_ir("alias_semantics.almd", SRC).expect("lowers");
    let bytes = almide_wasm::emit_program(&ir).expect("emits");
    let run = run_wasm(&bytes).expect("runs");
    assert_eq!(run.exit, 0);
    let wasm_out = run.stdout;

    // The oracle: the interpreter IS the definition (ARCHITECTURE.md §6.6).
    let interp = almide_spine::s5::run_file("alias_semantics.almd", SRC).expect("interp runs");
    assert_eq!(interp.exit, 0, "oracle run failed: {}", interp.stderr);
    assert_eq!(
        wasm_out, interp.stdout,
        "alias observed mutation — List bind deep-copy invariant broken"
    );
    // Belt and braces: pin the expected semantics explicitly, so a bug in
    // BOTH legs cannot silently agree.
    assert_eq!(wasm_out, "1\n2\n");
}
