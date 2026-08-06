//! An un-annotated effect-fn binding over an `if`/`match` of `ok(..)`/`err(..)`
//! CONSTRUCTORS keeps its `Result` in the checker (#717 family).
//!
//! The checker's `effect_unwrap_rhs` used to strip `Result` from every
//! Result-typed un-annotated `let` in an effect fn, on the contract that the
//! lowering inserts the matching `?`. The lowering only does that for values
//! that PROPAGATE — a direct call, or a branch whose arms are calls (the
//! `via_if` shape `spec/lang/effect_if_value_test.almd` pins, which really does
//! short-circuit). A branch whose arms are explicit constructors gets no `?`,
//! so the bound value stays a `Result` at runtime on every backend — verified
//! observable: a statement after such a binding still runs on the err path,
//! identically on native and wasm.
//!
//! With the checker alone claiming the payload type, the type system could not
//! catch the mismatch it had itself created: `int.to_string(r)` type-checked and
//! then failed in the GENERATED RUST (`E0308`), the effect-fn tail re-yielding
//! the var was double-`Ok`-wrapped so the function did not compile at all, and
//! the wasm leg trapped on `unreachable` for scalar payloads. One agreement fix
//! closed all three; these tests pin the checker half of it.

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

fn assert_rejected(src: &str, wanted: &str) {
    let errs = errors(src);
    assert!(
        errs.iter().any(|e| e.contains(wanted)),
        "expected an error containing {wanted:?}, got {errs:?}\nsource: {src}"
    );
}

fn assert_accepted(src: &str) {
    let errs = errors(src);
    assert!(errs.is_empty(), "expected no errors, got {errs:?}\nsource: {src}");
}

#[test]
fn constructor_branch_binding_is_a_result_not_its_payload() {
    // Used to type-check (the checker believed `r: Int`) and then die in the
    // generated Rust with E0308 — the class this fix closes.
    assert_rejected(
        "effect fn f(c: Bool) -> Result[Int, String] = {\n\
         \x20 let r = if c then ok(1) else err(\"n\")\n\
         \x20 println(int.to_string(r))\n\
         \x20 r\n\
         }",
        "but got Result[Int, String]",
    );
}

#[test]
fn constructor_match_branch_binding_is_a_result_not_its_payload() {
    assert_rejected(
        "effect fn f(c: Bool) -> Result[Int, String] = {\n\
         \x20 let r = match c { true => ok(1), _ => err(\"n\") }\n\
         \x20 println(int.to_string(r))\n\
         \x20 r\n\
         }",
        "but got Result[Int, String]",
    );
}

#[test]
fn constructor_branch_binding_re_yielded_as_the_tail_is_accepted() {
    // The whole function used to fail to compile (double-`Ok` wrap).
    assert_accepted(
        "effect fn f(c: Bool) -> Result[Int, String] = {\n\
         \x20 let r = if c then ok(1) else err(\"n\")\n\
         \x20 r\n\
         }",
    );
}

#[test]
fn call_armed_branch_binding_spells_its_propagation() {
    // ADR-0008 (#1123 N+1): the #717 shape spells per-branch `!` now — the
    // explicit form stays ACCEPTED and the binding really is the payload…
    assert_accepted(
        "effect fn g(x: Int) -> Result[Int, String] = if x < 0 then err(\"neg\") else ok(x)\n\
         effect fn f(x: Int) -> Result[Int, String] = {\n\
         \x20 let v = if x > 10 then g(x)! else g(0 - 1)!\n\
         \x20 ok(v + 100)\n\
         }",
    );
    // …while the old implicit spelling is the E041 error.
    assert_rejected(
        "effect fn g(x: Int) -> Result[Int, String] = if x < 0 then err(\"neg\") else ok(x)\n\
         effect fn f(x: Int) -> Result[Int, String] = {\n\
         \x20 let v = if x > 10 then g(x) else g(0 - 1)\n\
         \x20 ok(v + 100)\n\
         }",
        "implicit propagation",
    );
}

#[test]
fn annotated_constructor_branch_binding_still_keeps_its_result() {
    // The pre-existing escape hatch, unchanged.
    assert_accepted(
        "effect fn f(c: Bool) -> Result[Int, String] = {\n\
         \x20 let r: Result[Int, String] = if c then ok(7) else err(\"x\")\n\
         \x20 match r { ok(v) => ok(v), err(e) => err(e) }\n\
         }",
    );
}
