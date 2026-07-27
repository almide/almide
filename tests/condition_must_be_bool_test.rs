//! `if` / `while` conditions must be `Bool` — Almide has no truthiness (#896).
//!
//! Before this, the checker inferred the condition and discarded the type, so
//! `if 1 then …` ran with C-style truthiness (and `while i { … }` terminated on
//! zero), while `if "s" then …` passed `almide check` clean and then died in
//! codegen as "produced invalid Rust — this is an Almide bug". Both are now the
//! ordinary E001 the language reference always claimed they were.

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

#[test]
fn if_rejects_an_int_condition() {
    assert_rejected(
        "fn main() -> Unit = { if 1 then println(\"y\") }",
        "if condition: expected Bool but got Int",
    );
}

#[test]
fn if_rejects_a_string_condition() {
    // This one used to reach codegen and ICE rather than diagnose.
    assert_rejected(
        "fn main() -> Unit = { if \"hello\" then println(\"y\") }",
        "if condition: expected Bool but got String",
    );
}

#[test]
fn if_rejects_a_non_bool_condition_with_an_else_branch() {
    assert_rejected(
        "fn pick(n: Int) -> String = if n then \"a\" else \"b\"",
        "if condition: expected Bool but got Int",
    );
}

#[test]
fn while_rejects_an_int_condition() {
    assert_rejected(
        "fn main() -> Unit = { var i = 2\n  while i { i = i - 1 } }",
        "while condition: expected Bool but got Int",
    );
}

#[test]
fn bool_conditions_still_check() {
    for src in [
        "fn main() -> Unit = { if true then println(\"y\") }",
        "fn pick(n: Int) -> String = if n > 0 then \"a\" else \"b\"",
        "fn main() -> Unit = { var i = 2\n  while i > 0 { i = i - 1 } }",
        // A comparison behind a call, so the condition's type arrives from
        // inference rather than syntactically.
        "fn pos(n: Int) -> Bool = n > 0\nfn main() -> Unit = { if pos(1) then println(\"y\") }",
    ] {
        assert!(errors(src).is_empty(), "unexpected errors for {src}: {:?}", errors(src));
    }
}
