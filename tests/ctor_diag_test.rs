//! The Option/Result constructor surface has two check-time rulings that the
//! ALS-E9 section cites as its negative half (the accepted spellings are
//! pinned by `spec/wasm_cross/option_result_ctors.almd`, C-237):
//!
//! - `none` is a bare VALUE, not a function — `none()` is E002, the same code
//!   `let f = none; f()` already produced, and the fix (drop the parens) must
//!   be readable from the diagnostic alone. It used to fall through to the
//!   generic E001, which printed the two sides of a unification — `expected
//!   Option[?0] but got fn() -> Option[Int]` — and, where the expected type
//!   was an inference slot, dragged a red-herring E025 along telling the
//!   reader to add a type annotation (#2134).
//! - `ok(e)` / `err(e)` demand a Result-typed expectation: an un-annotated
//!   `let` binding is the ADR-0008 explicit-propagation rejection (E041),
//!   whose hint steers to `!` / `??` / `?` / match.

use std::process::Command;

fn almide() -> &'static str {
    env!("CARGO_BIN_EXE_almide")
}

fn check(dir: &std::path::Path, source: &str) -> String {
    let file = dir.join("ctor.almd");
    std::fs::write(&file, source).expect("write fixture");
    let out = Command::new(almide())
        .arg("check")
        .arg(&file)
        .output()
        .expect("run almide check");
    format!(
        "{}{}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    )
}

#[test]
fn calling_none_is_a_type_error() {
    let dir = tempfile::tempdir().expect("tempdir");
    let out = check(
        dir.path(),
        "effect fn main() -> Unit = {\n  let n: Int? = none()\n  println(int.to_string(n ?? 0))\n}\n",
    );
    assert!(
        out.contains("E002"),
        "`none()` must be the not-a-function diagnostic, got:\n{out}"
    );
    assert!(
        out.contains("drop the parentheses"),
        "the fix must be readable from the diagnostic alone, got:\n{out}"
    );
    // The red herring: an expected type that is an inference slot used to
    // produce a second, misleading error pointing at type annotations.
    assert!(
        !out.contains("E025"),
        "`none()` must not cascade into an inference-annotation error, got:\n{out}"
    );
}

#[test]
fn a_constructor_used_as_a_value_says_its_argument_is_missing() {
    let dir = tempfile::tempdir().expect("tempdir");
    let out = check(dir.path(), "fn c(n: Int) -> Int? = some\nfn main() -> Unit = println(\"x\")\n");
    assert!(out.contains("E001"), "got:\n{out}");
    assert!(
        out.contains("the arguments were never supplied"),
        "the arity mistake must be named — no name-distance suggestion can reach it, got:\n{out}"
    );
}

#[test]
fn a_function_name_where_its_result_is_expected_says_to_call_it() {
    let dir = tempfile::tempdir().expect("tempdir");
    let out = check(
        dir.path(),
        "fn gen() -> Int = 7\nfn g() -> Int = gen\nfn main() -> Unit = println(\"x\")\n",
    );
    assert!(out.contains("E001"), "got:\n{out}");
    assert!(
        out.contains("the call was never made"),
        "the other direction of the same shape must be named too, got:\n{out}"
    );
}

#[test]
fn unannotated_ok_binding_is_e041() {
    let dir = tempfile::tempdir().expect("tempdir");
    let out = check(
        dir.path(),
        "effect fn main() -> Unit = {\n  let o = ok(3)\n  println(int.to_string(o ?? 0))\n}\n",
    );
    assert!(
        out.contains("E041") || out.contains("E034"),
        "un-annotated `ok(3)` binding must be rejected at check time, got:\n{out}"
    );
}

/// The other half of the same arm: any VALUE in callee position, not just
/// `none`. A parenthesised literal and a string both reach the computed-callee
/// path, where the old code compared the callee's type against a function type
/// and printed the unification instead of the mistake.
#[test]
fn any_value_in_callee_position_is_the_not_a_function_diagnostic() {
    let dir = tempfile::tempdir().expect("tempdir");
    for (src, ty) in [("(1)(2)", "Int"), ("\"abc\"()", "String"), ("[1, 2](3)", "List[Int]")] {
        let out = check(
            dir.path(),
            &format!("fn main() -> Unit = {{\n  let _ = {src}\n  println(\"x\")\n}}\n"),
        );
        assert!(out.contains("E002"), "{src}: got:\n{out}");
        assert!(
            out.contains(&format!("this expression is not a function — it has type {ty}")),
            "{src}: the type the caller actually has must be named, got:\n{out}"
        );
        assert!(
            out.contains("Only functions and closures can be called"),
            "{src}: got:\n{out}"
        );
    }
}
