//! #3105: a Unit VALUE in statement position is dropped. `lower` puts a
//! Unit value on the stack as an i32 placeholder (a `()` literal, a void call
//! under a Unit want, the `r ?? ()` join over a `Result[Unit, _]`).
//! `lower_stmt_expr`'s catch-all skipped the discard for Unit, which left the
//! i32 on the stack, and the module failed validation. The downstream canary
//! caught it: O6lvl4/ctxgate's `fs.remove(p) ?? ()` arm tails moved three test
//! files from the structural leg to the native fallback.

mod harness;
use harness::run_wasm;

/// A `?? ()` statement in each position the ctxgate files had: the fn-body
/// tail, a match arm's tail, and an arm tail behind a `guard … else ()`. The
/// guard form is the one 583d6dbb3 exposed, because the guard's else now exits
/// through the Ok channel and the rest of the arm is lowered as a statement
/// body.
const SRC: &str = r#"effect fn f(n: Int) -> Unit = if n > 0 then ok(()) else err("no")

effect fn tail(n: Int) -> Unit = {
  f(n) ?? ()
}

effect fn arm(o: Option[Int]) -> Unit = {
  match o {
    none => (),
    some(l) => {
      println("arm ${l}")
      f(l) ?? ()
    },
  }
}

effect fn guarded(o: Option[Int]) -> Unit = {
  match o {
    none => (),
    some(l) => {
      guard l > 3 else ()
      println("guarded ${l}")
      f(l - 5) ?? ()
    },
  }
}

effect fn main() -> Unit = {
  tail(1)!
  tail(0)!
  arm(some(2))!
  arm(some(-2))!
  arm(none)!
  guarded(some(9))!
  guarded(some(4))!
  guarded(some(1))!
  guarded(none)!
  println("done")
}
"#;

#[test]
fn a_unit_unwrap_or_statement_is_dropped_in_every_position() {
    let ir = almide_spine::s5::lower_to_ir("unit_statement_discard.almd", SRC).expect("lowers");
    let bytes = almide_wasm::emit_program(&ir).expect("the structural leg emits it");
    wasmparser::validate(&bytes).expect("a statement-position Unit value leaves nothing on the stack");
    let run = run_wasm(&bytes).expect("runs");
    assert_eq!(run.exit, 0, "stderr: {}", run.stderr);
    let expected = "arm 2\narm -2\nguarded 9\nguarded 4\ndone\n";
    assert_eq!(run.stdout, expected);
    let interp = almide_spine::s5::run_file("unit_statement_discard.almd", SRC).expect("interp runs");
    assert_eq!(interp.exit, 0, "interp stderr: {}", interp.stderr);
    assert_eq!(run.stdout, interp.stdout, "wasm diverged from the interp oracle");
}
