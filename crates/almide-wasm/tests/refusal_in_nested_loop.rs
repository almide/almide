//! #4654: a refusal raised inside a loop body that sits in a branch arm is a
//! refusal (E082), not a compiler panic.
//!
//! `lower_loop_body` installs the loop's own label context and restored the
//! enclosing one only on success. When a statement of the body was refused,
//! the enclosing match arm then closed its label (`shift_loop_labels(-1)`)
//! against the body's fresh context and hit `loop label depth underflow`.
//!
//! The refused shape here is a `continue` inside a VALUE-position block (the
//! structural leg's `expr:Continue` decline, as in decline_site.rs). The
//! issue's own shape, a `guard … else err(<List[String]>)` in an `effect fn`,
//! is now a check-time E001 (#4654). If this shape starts lowering, swap in
//! any other statement the structural leg refuses: the assertion is that the
//! refusal arrives as an error.

const SRC: &str = "effect fn work(c: Bool, n: Int) -> Int = {
  var q = 0
  match c {
    true => {
      for i in 0..<2 {
        let k = {
          if n == 0 then continue
          1
        }
        q = q + i + k
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
