//! #2607: E022 inside a fn that returns a carrier names the real mismatch.
//!
//! The generic E022 says `!` is "only valid inside … a fn returning
//! Result/Option". That is wrong advice when the fn already returns one. A
//! Result operand in an Option fn gets its own message and the `?!` fix-it
//! (tests/diagnostics/e022-result-bang-in-option-fn). A plain value operand
//! already gets E034 at the operator, and the misleading E022 beside it is
//! dropped.

use std::process::Command;

fn check(source: &str) -> String {
    let dir = tempfile::tempdir().expect("tempdir");
    let file = dir.path().join("bang.almd");
    std::fs::write(&file, source).expect("write fixture");
    let out = Command::new(env!("CARGO_BIN_EXE_almide"))
        .arg("check")
        .arg(&file)
        .output()
        .expect("run almide check");
    format!("{}{}", String::from_utf8_lossy(&out.stdout), String::from_utf8_lossy(&out.stderr))
}

#[test]
fn a_result_operand_in_an_option_fn_names_the_carrier_mismatch() {
    let text = check(
        "fn first_num(s: String) -> Int? = {\n  let n = int.parse(s)!\n  some(n)\n}\n\
         effect fn main() -> Unit = println(int.to_string(first_num(\"3\") ?? 0))\n",
    );
    assert!(text.contains("error[E022]"), "expected E022, got:\n{text}");
    assert!(
        text.contains("`int.parse(s)` is a Result[Int, String] but `first_num` returns Option[Int]"),
        "E022 must name the operand and the fn's carrier, got:\n{text}"
    );
    assert!(!text.contains("only valid inside"), "the generic wording blames the wrong thing, got:\n{text}");
}

#[test]
fn a_plain_value_operand_in_a_result_fn_is_e034_alone() {
    let text = check(
        "fn f(s: String) -> Int! = {\n  let x = string.len(s)!\n  ok(x)\n}\n\
         effect fn main() -> Unit = println(int.to_string(f(\"a\") ?? 0))\n",
    );
    assert!(text.contains("error[E034]"), "expected E034, got:\n{text}");
    assert!(!text.contains("E022"), "no E022 beside the E034, got:\n{text}");
}

#[test]
fn a_result_fn_or_a_converted_operand_is_accepted() {
    let text = check(
        "fn first_num(s: String) -> Int? = {\n  let n = int.parse(s)?!\n  some(n)\n}\n\
         fn as_result(s: String) -> Int! = {\n  let n = int.parse(s)!\n  ok(n)\n}\n\
         effect fn main() -> Unit = println(int.to_string((first_num(\"3\") ?? 0) + (as_result(\"4\") ?? 0)))\n",
    );
    assert!(text.contains("No errors found"), "both spellings must check, got:\n{text}");
}
