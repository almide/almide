//! `return EXPR` is reported once and the parse continues with EXPR.
//!
//! `return` is not Almide (the last expression is the value), and the hint
//! says so. But the parser used to stop at it: the rest of the statement was
//! dropped, so `fn f(x: Int) -> Int = { return x + 1 }` also reported
//! `expected Int but got Unit`. Inside a loop it lost its place entirely:
//! `Expected top-level declaration … (got None 'none')`. Three errors for one
//! mistake send a model repairing the program after the wrong two. The
//! `return` is now skipped after its diagnostic, and EXPR is parsed as the
//! value it was meant to be.
use std::process::Command;

fn errors(src: &str) -> Vec<String> {
    let dir = tempfile::tempdir().expect("scratch");
    let file = dir.path().join("ret.almd");
    std::fs::write(&file, src).expect("source");
    let out = Command::new(env!("CARGO_BIN_EXE_almide")).arg("check").arg(&file).output().expect("check");
    assert!(!out.status.success(), "a `return` is still an error");
    let text = format!("{}{}", String::from_utf8_lossy(&out.stdout), String::from_utf8_lossy(&out.stderr));
    text.lines().filter(|l| l.starts_with("error")).map(str::to_string).collect()
}

const MAIN: &str = "\neffect fn main() -> Unit = println(int.to_string(f(3)))\n";

#[test]
fn a_tail_return_is_one_error() {
    let errs = errors(&format!("fn f(x: Int) -> Int = {{\n  return x + 1\n}}\n{MAIN}"));
    assert_eq!(errs.len(), 1, "{errs:#?}");
    assert!(errs[0].contains("'return' is not needed"), "{errs:#?}");
}

#[test]
fn a_return_in_a_branch_is_one_error() {
    let errs = errors(&format!("fn f(x: Int) -> Int = {{\n  if x > 0 then return 1 else return 2\n}}\n{MAIN}"));
    assert_eq!(errs.len(), 2, "one per `return`, nothing else: {errs:#?}");
    assert!(errs.iter().all(|e| e.contains("'return' is not needed")), "{errs:#?}");
}

#[test]
fn a_return_in_a_loop_does_not_lose_the_parse() {
    let errs = errors(
        "fn first_hit(xs: List[String], f: (String) -> Bool) -> Option[String] = {\n  for x in xs {\n    if f(x) then return some(x)\n  }\n  none\n}\n\neffect fn main() -> Unit = println(first_hit([\"a\"], (s) => true) ?? \"-\")\n",
    );
    assert!(errs[0].contains("'return' is not needed"), "{errs:#?}");
    assert!(!errs.iter().any(|e| e.contains("Expected top-level declaration")), "the parser keeps its place: {errs:#?}");
}

/// `let mut x = …` (Rust) is reported once and binds as the `var` it means,
/// so a later read of `x` is not also `undefined variable 'x'`.
#[test]
fn let_mut_is_one_error_and_binds_the_var() {
    let errs = errors(&format!("fn f(x: Int) -> Int = {{\n  let mut y = x\n  y = y + 1\n  y\n}}\n{MAIN}"));
    assert_eq!(errs.len(), 1, "{errs:#?}");
    assert!(errs[0].contains("'let mut' is not valid"), "{errs:#?}");
}
