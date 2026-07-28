//! A sized numeric type may not be mixed with a canonical `Int`/`Float` VALUE
//! (#902).
//!
//! The mixed-width rule was enforced only when BOTH sides were sized, so the
//! same mistake slipped through whenever the wide side was spelled `Int`:
//! `fn add32(a: Int32, b: Int) -> Int32 = a + b` passed `check`, failed the
//! native build with a rustc E0308, and on wasm — where every scalar rides one
//! i64 — computed a value outside the declared width. Spelling that parameter
//! `Int64` WAS caught, so the rule was enforcing a spelling, not a type.
//!
//! The canonical side stays exempt when it is a LITERAL-ONLY expression, which
//! is what the permissive pair exists for: a literal adopts the sized width at
//! lowering, a value does not.

use almide::lexer::Lexer;
use almide::parser::Parser;
use almide::canonicalize;
use almide::check::Checker;
use almide::diagnostic::Level;

fn errors(input: &str) -> Vec<String> {
    let tokens = Lexer::tokenize(input);
    let mut parser = Parser::new(tokens);
    let mut prog = parser.parse().expect("parse failed");
    let canon = canonicalize::canonicalize_program(&prog, std::iter::empty());
    let mut checker = Checker::from_env(canon.env);
    checker.diagnostics = canon.diagnostics;
    checker
        .infer_program(&mut prog)
        .into_iter()
        .filter(|d| d.level == Level::Error)
        .map(|d| d.message)
        .collect()
}

fn assert_rejected(src: &str) {
    let errs = errors(src);
    assert!(
        errs.iter().any(|e| e.contains("mixes sized numeric type")),
        "expected a mixed-width rejection, got {errs:?}\nsource: {src}"
    );
}

fn assert_accepted(src: &str) {
    let errs = errors(src);
    assert!(errs.is_empty(), "unexpected rejection for {src}: {errs:?}");
}

#[test]
fn a_sized_int_parameter_may_not_meet_a_canonical_int_value() {
    assert_rejected("fn add32(a: Int32, b: Int) -> Int32 = a + b");
    assert_rejected("fn sub32(a: Int32, b: Int) -> Int32 = a - b");
    assert_rejected("fn mul8(a: Int8, b: Int) -> Int8 = a * b");
    // Either order.
    assert_rejected("fn add32r(a: Int, b: Int32) -> Int32 = a + b");
}

#[test]
fn a_sized_float_parameter_may_not_meet_a_canonical_float_value() {
    assert_rejected("fn addf(a: Float32, b: Float) -> Float32 = a + b");
    assert_rejected("fn mulf(a: Float64, b: Float) -> Float64 = a * b");
}

#[test]
fn a_canonical_int_local_is_rejected_too() {
    assert_rejected(
        "fn main() -> Unit = {\n  let a: Int32 = 1\n  let n: Int = 2\n  println(int32.to_string(a + n))\n}",
    );
}

#[test]
fn a_literal_still_adopts_the_sized_width() {
    assert_accepted("fn main() -> Unit = {\n  let x: Int32 = 1\n  println(int32.to_string(x + 2))\n}");
    // Literal-only ARITHMETIC counts as a literal: nothing in it chose a width.
    assert_accepted("fn main() -> Unit = {\n  let x: Int32 = 1\n  println(int32.to_string(x + 2 * 3))\n}");
    assert_accepted("fn main() -> Unit = {\n  let x: Int32 = 1\n  println(int32.to_string(x - -2))\n}");
    assert_accepted("fn main() -> Unit = {\n  let f: Float32 = 1.5\n  println(float32.to_string(f + 0.5))\n}");
}

#[test]
fn same_width_and_all_canonical_arithmetic_are_untouched() {
    assert_accepted("fn add32(a: Int32, b: Int32) -> Int32 = a + b");
    assert_accepted("fn addi(a: Int, b: Int) -> Int = a + b");
    assert_accepted("fn addf(a: Float, b: Float) -> Float = a + b");
    assert_accepted("fn add64(a: Int64, b: Int64) -> Int64 = a + b");
}
