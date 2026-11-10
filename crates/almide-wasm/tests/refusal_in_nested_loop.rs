//! #4654: a refusal raised inside a loop body that sits in a branch arm is a
//! refusal (E082), not a compiler panic.
//!
//! `lower_loop_body` installs the loop's own label context and restored the
//! enclosing one only on success. When a statement of the body was refused,
//! the enclosing match arm then closed its label (`shift_loop_labels(-1)`)
//! against the body's fresh context and hit `loop label depth underflow`.
//!
//! The refused shape here is a `guard … else err(<List[String]>)` in an
//! `effect fn`, whose error channel is `String` on the wasm leg. If that shape
//! starts lowering (or the checker starts rejecting it), swap in any other
//! statement the structural leg refuses: the assertion is that the refusal
//! arrives as an error.

const SRC: &str = "effect fn work(c: Bool, n: Int) -> Int = {
  var q = 0
  match c {
    true => {
      for i in 0..<2 {
        guard n > 0 else err([\"a\", \"b\"])
        q = q + i
      }
    },
    false => (),
  }
  q
}

effect fn main() -> Unit = {
  println(int.to_string(work(true, 3)!))
}
";

#[test]
fn a_refusal_in_a_loop_inside_a_match_arm_is_an_error() {
    let ir = almide_spine::s5::lower_to_ir("refusal_in_nested_loop.almd", SRC).expect("front");
    let err = almide_wasm::emit_program(&ir).expect_err("the shape must be refused");
    assert!(matches!(err, almide_wasm::EmitError::Unsupported(_)), "{err:?}");
}
