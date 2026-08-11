//! `guard let` inside a loop body must LOWER, not panic the compiler (#1204).
//!
//! From the day the construct landed until this fix, `for … { guard let x = …
//! else { continue } … }` crashed with
//! `internal error: entered unreachable code: guard let is desugared by the
//! enclosing block, not lower_stmt` — on released 0.56.0 as well as develop.
//! The rewrite that turns `[pre…, GuardLet, rest…]` into its bind + early-exit
//! `match` ran for BLOCK statement lists only; a loop body was mapped straight
//! through `lower_stmt`, whose `GuardLet` arm is the `unreachable!()`.
//!
//! Why no test caught it: `spec/lang/guard_let_test.almd` covers Some/None,
//! Ok/Err and nested guards — all at fn-body top level. `guard let` exists FOR
//! early exit and `continue` IS a loop's early exit, so the loop form is the
//! construct's most natural use and had zero coverage.
//!
//! This lives here rather than in `spec/lang/` on purpose: the property under
//! test is a COMPILER one (lowering is total on this shape), and the loop form
//! walls honestly on the wasm leg (break/continue in a `for` body is a declared
//! frontier). Putting it in the Almide corpus would push a file off the wasm
//! leg and add a walled fn, moving two shrink-only ratchets in the wrong
//! direction to assert something neither of them is about.

use almide::canonicalize;
use almide::check::Checker;
use almide::lexer::Lexer;
use almide::parser::Parser;

/// Parse → check → LOWER. Returns the IR function count; panics exactly where
/// the compiler would, which is what these tests are watching for.
fn lower_ok(src: &str) -> usize {
    let tokens = Lexer::tokenize(src);
    let mut parser = Parser::new(tokens);
    let mut prog = parser.parse().expect("parse failed");
    let canon = canonicalize::canonicalize_program(&prog, std::iter::empty());
    let mut checker = Checker::from_env(canon.env);
    checker.diagnostics = canon.diagnostics;
    let diags = checker.infer_program(&mut prog);
    let errs: Vec<_> = diags
        .iter()
        .filter(|d| d.level == almide::diagnostic::Level::Error)
        .map(|d| d.message.clone())
        .collect();
    assert!(errs.is_empty(), "unexpected check errors: {errs:?}");
    let ir = almide::lower::lower_program(&prog, &checker.env, &checker.type_map);
    ir.functions.len()
}

#[test]
fn guard_let_in_a_for_body_lowers() {
    assert!(
        lower_ok(
            "fn firsts(rows: List[List[String]]) -> String = {\n\
             \x20 var out = \"\"\n\
             \x20 for row in rows {\n\
             \x20   guard let h = list.first(row) else { continue }\n\
             \x20   out = out + h\n\
             \x20 }\n\
             \x20 out\n\
             }\n\
             fn main() -> Unit = println(firsts([[\"p\"], []]))\n"
        ) > 0
    );
}

#[test]
fn guard_let_in_a_while_body_lowers() {
    assert!(
        lower_ok(
            "fn scan(xs: List[Int]) -> Int = {\n\
             \x20 var i = 0\n\
             \x20 var total = 0\n\
             \x20 while i < list.len(xs) {\n\
             \x20   guard let v = list.get(xs, i) else { break }\n\
             \x20   total = total + v\n\
             \x20   i = i + 1\n\
             \x20 }\n\
             \x20 total\n\
             }\n\
             fn main() -> Unit = println(int.to_string(scan([1, 2])))\n"
        ) > 0
    );
}

#[test]
fn two_guard_lets_in_one_loop_body_lower() {
    // The rewrite recurses: everything after the first guard becomes the Ok/Some
    // arm's body, so a second guard inside it must be rewritten in turn.
    assert!(
        lower_ok(
            "fn pairs(rows: List[List[Int]]) -> Int = {\n\
             \x20 var total = 0\n\
             \x20 for row in rows {\n\
             \x20   guard let a = list.get(row, 0) else { continue }\n\
             \x20   guard let b = list.get(row, 1) else { continue }\n\
             \x20   total = total + a + b\n\
             \x20 }\n\
             \x20 total\n\
             }\n\
             fn main() -> Unit = println(int.to_string(pairs([[1, 2]])))\n"
        ) > 0
    );
}
