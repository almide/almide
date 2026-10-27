//! #3960: a method call on a clock value (`Duration`, `Compute`) that names no
//! function passed `almide check` as `Unknown` and then failed the build with
//! an internal type-resolution error. The clock types exist only in the
//! checker, so the "undeclared type" root-cause path swallowed the E002 that
//! any other concrete receiver gets. The checker must refuse it.

use almide::canonicalize;
use almide::check::Checker;
use almide::diagnostic::Level;
use almide::lexer::Lexer;
use almide::parser::Parser;

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
        .map(|d| format!("{} [{}]", d.message, d.code.unwrap_or("")))
        .collect()
}

#[test]
fn cmp_on_a_clock_value_is_an_undefined_method() {
    for (module, ty) in [("duration", "Duration"), ("compute", "Compute")] {
        let src = format!(
            "fn main() -> Unit = {{\n  let a = {module}.ms(5)\n  let r = a.cmp({module}.s(1))\n  println(\"${{r}}\")\n}}\n"
        );
        let errs = errors(&src);
        assert!(
            errs.iter().any(|e| e.contains(&format!("undefined method 'cmp' on {ty}")) && e.contains("E002")),
            "{ty}.cmp must be E002, got {errs:?}"
        );
    }
}

#[test]
fn the_clock_compare_function_still_checks() {
    for module in ["duration", "compute"] {
        let src = format!(
            "fn main() -> Unit = {{\n  let r = {module}.compare({module}.ms(5), {module}.s(1))\n  println(\"${{r}}\")\n}}\n"
        );
        let errs = errors(&src);
        assert!(errs.is_empty(), "{module}.compare must check, got {errs:?}");
    }
}
