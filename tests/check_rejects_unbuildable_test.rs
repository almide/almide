//! Programs that used to pass `almide check` and then fail to build (#899).
//!
//! `check` is offered as an editor/CI gate, so "check passes" has to mean "this
//! builds". These three shapes broke that promise in three different ways: a
//! rustc `E0282` on a generated generic call, the ConcretizeTypes COMPILER-BUG
//! gate, and a rustc type error on a chained range. All three are now rejected
//! at check (or parse) time.

use almide::lexer::Lexer;
use almide::parser::Parser;
use almide::canonicalize;
use almide::check::Checker;
use almide::diagnostic::Level;

/// Every reason this source is rejected before codegen — a parse error, or the
/// checker's errors. `Vec::is_empty()` means the program was accepted.
fn rejections(input: &str) -> Vec<String> {
    let tokens = Lexer::tokenize(input);
    let mut parser = Parser::new(tokens);
    let mut prog = match parser.parse() {
        Ok(p) => p,
        Err(e) => return vec![e],
    };
    // The parser recovers from a bad statement rather than aborting, so its own
    // diagnostics live here even when `parse()` returns Ok.
    let parse_errors: Vec<String> = parser
        .errors
        .iter()
        .filter(|d| d.level == Level::Error)
        .map(|d| d.message.clone())
        .collect();
    if !parse_errors.is_empty() {
        return parse_errors;
    }
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
    let errs = rejections(src);
    assert!(
        errs.iter().any(|e| e.contains(wanted)),
        "expected a rejection containing {wanted:?}, got {errs:?}\nsource: {src}"
    );
}

fn assert_accepted(src: &str) {
    let errs = rejections(src);
    assert!(errs.is_empty(), "unexpected rejection for {src}: {errs:?}");
}

#[test]
fn an_unconstrained_none_in_argument_position_is_rejected() {
    assert_rejected(
        "fn main() -> Unit = println(if option.is_none(none) then \"y\" else \"n\")",
        "cannot infer a concrete type",
    );
}

#[test]
fn an_unconstrained_err_in_argument_position_is_rejected() {
    assert_rejected(
        "fn main() -> Unit = println(if result.is_err(err(\"fail\")) then \"y\" else \"n\")",
        "cannot infer a concrete type",
    );
}

#[test]
fn a_constructor_whose_context_pins_it_is_accepted() {
    // The slot is decidable in every one of these, so the argument-position
    // check must stay quiet.
    assert_accepted("fn main() -> Unit = { let x: Option[Int] = none\n  println(if option.is_none(x) then \"y\" else \"n\") }");
    assert_accepted("fn get() -> Option[Int] = none\nfn main() -> Unit = println(if option.is_none(get()) then \"y\" else \"n\")");
    assert_accepted("fn main() -> Unit = println(if option.is_none(some(1)) then \"y\" else \"n\")");
    assert_accepted("fn main() -> Unit = println(int.to_string(option.unwrap_or(none, 7)))");
}

#[test]
fn a_constructor_in_match_arm_position_is_still_accepted() {
    // `err(e) => err(e)` keeps a loose Ok slot the checker never pins and
    // codegen resolves from the sibling arm — deliberately not this check's
    // business, and a regression here would break the stdlib suite.
    assert_accepted(
        "fn main() -> Unit = {\n\
         \x20 let outer: Result[Int, String] = ok(42)\n\
         \x20 let inner: Result[String, String] = match outer {\n\
         \x20   ok(n) => ok(int.to_string(n)),\n\
         \x20   err(e) => err(e),\n\
         \x20 }\n\
         \x20 println(result.unwrap_or(inner, \"?\"))\n\
         }",
    );
}

#[test]
fn a_chained_range_is_rejected() {
    assert_rejected(
        "fn main() -> Unit = {\n  let xs = 0..1..2\n  println(int.to_string(list.len(xs)))\n}",
        "Chained range operators are not allowed",
    );
    assert_rejected(
        "fn main() -> Unit = {\n  let xs = 0..=1..=2\n  println(int.to_string(list.len(xs)))\n}",
        "Chained range operators are not allowed",
    );
}

#[test]
fn ordinary_ranges_still_parse() {
    assert_accepted("fn main() -> Unit = println(int.to_string(list.len(0..3)))");
    assert_accepted("fn main() -> Unit = println(int.to_string(list.len(0..=3)))");
    assert_accepted("fn main() -> Unit = { for i in 0..3 { println(int.to_string(i)) } }");
    // A range whose bounds are themselves expressions must keep working — the
    // rejection looks at the bound's SHAPE, not at the tokens around it.
    assert_accepted("fn main() -> Unit = { let n = 2\n  println(int.to_string(list.len(n - 1..n + 2)))\n }");
}
