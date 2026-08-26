//! MC/DC independence-pair vectors for the effect-isolation and arity
//! decisions in `almide-frontend/src/check/calls.rs` (proofs/mcdc-ledger.toml,
//! #566 rung 2). In-process checker harness, same shape as
//! condition_must_be_bool_test.rs.

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
        .map(|d| d.message)
        .collect()
}

fn has(errs: &[String], needle: &str) -> bool { errs.iter().any(|e| e.contains(needle)) }

// ── Site calls.rs:402 — `sig.is_effect && !self.env.can_call_effect` (E006) ──

#[test]
fn site402_baseline_effect_callee_from_pure_fn_is_e006() {
    let errs = errors(
        "effect fn w() -> Unit = println(\"x\")\nfn caller() -> Unit = w()\neffect fn main() -> Unit = caller()",
    );
    assert!(has(&errs, "cannot call effect function 'w'"), "{errs:?}");
}

#[test]
fn site402_c1_pure_callee_alone_is_clean() {
    // Only C1 flips: the callee stops being effect; the caller stays pure.
    let errs = errors(
        "fn w() -> Unit = ()\nfn caller() -> Unit = w()\neffect fn main() -> Unit = caller()",
    );
    assert!(!has(&errs, "cannot call effect function"), "{errs:?}");
}

#[test]
fn site402_c2_effect_caller_alone_is_clean() {
    // Only C2 flips: the caller becomes effect; the callee stays effect.
    let errs = errors(
        "effect fn w() -> Unit = println(\"x\")\neffect fn caller() -> Unit = w()\neffect fn main() -> Unit = caller()",
    );
    assert!(!has(&errs, "cannot call effect function"), "{errs:?}");
}

// ── Site calls.rs:449 — `arg_tys.len() < min_params || arg_tys.len() > sig.params.len()` (E004) ──

#[test]
fn site449_baseline_exact_arity_is_clean() {
    let errs = errors("fn f(a: Int, b: Int) -> Int = a + b\nfn main() -> Unit = println(\"${f(1, 2)}\")");
    assert!(!has(&errs, "argument"), "{errs:?}");
}

#[test]
fn site449_c1_too_few_arguments_alone_is_e004() {
    // Only C1 (len < min) is true; C2 (len > max) stays false.
    let errs = errors("fn f(a: Int, b: Int) -> Int = a + b\nfn main() -> Unit = println(\"${f(1)}\")");
    assert!(has(&errs, "argument"), "{errs:?}");
}

#[test]
fn site449_c2_too_many_arguments_alone_is_e004() {
    // Only C2 (len > max) is true; C1 (len < min) stays false.
    let errs = errors("fn f(a: Int, b: Int) -> Int = a + b\nfn main() -> Unit = println(\"${f(1, 2, 3)}\")");
    assert!(has(&errs, "argument"), "{errs:?}");
}
