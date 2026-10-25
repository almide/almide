//! E005 names a same-stem stdlib sibling that returns the expected type (#2097).
//!
//! `list.find` answers the element and `list.find_index` its position, one
//! suffix apart. "Fix the argument type" carried no information the message
//! did not; the sibling is a lookup, so the hint names it and the `try:` line
//! is the renamed call. (The original pair, `fs.read_bytes` / `read_bytes_raw`,
//! became one function when `fs.read_bytes` took `Bytes`, R-fs-01.)

use std::process::Command;

fn almide() -> &'static str {
    env!("CARGO_BIN_EXE_almide")
}

fn check(src: &str) -> (bool, String) {
    let dir = tempfile::tempdir().expect("tempdir");
    let file = dir.path().join("t.almd");
    std::fs::write(&file, src).expect("write");
    let out = Command::new(almide()).arg("check").arg(&file).output().expect("run");
    let text = format!("{}{}", String::from_utf8_lossy(&out.stdout), String::from_utf8_lossy(&out.stderr));
    (out.status.success(), text)
}

#[test]
fn a_same_stem_sibling_with_the_expected_return_is_named() {
    let (ok, text) = check(
        "fn at(i: Int?) -> Int = i ?? -1\n\
         fn main() -> Unit = {\n\
         \x20 let hit = list.find([\"a\", \"b\"], (s) => s == \"b\")\n\
         \x20 println(int.to_string(at(hit)))\n\
         }\n",
    );
    assert!(!ok, "the mismatch must still be E005:\n{text}");
    assert!(text.contains("error[E005]"), "{text}");
    assert!(text.contains("`list.find_index` returns"), "sibling not named:\n{text}");
    assert!(text.contains("list.find_index([\"a\", \"b\"]"), "try line must be the renamed call:\n{text}");
    assert!(!text.contains("Fix the argument type"), "the generic hint must give way:\n{text}");
}

#[test]
fn a_let_in_another_fn_never_names_the_origin_for_a_same_named_param() {
    // `a` binds `hit` from `list.find`; `c` has a PARAMETER `hit` of the
    // same type. The origin table is scoped with the binding, so `c`'s E005
    // must not claim `hit` came from `list.find` or offer a `let` rewrite
    // into a function that has no such `let`.
    let (ok, text) = check(
        "fn at(i: Int?) -> Int = i ?? -1\n\
         fn a() -> Unit = {\n\
         \x20 let hit = list.find([\"a\"], (s) => s == \"a\")\n\
         \x20 println(hit ?? \"none\")\n\
         }\n\
         fn c(hit: String?) -> Int = at(hit)\n\
         fn main() -> Unit = println(int.to_string(c(none)))\n",
    );
    assert!(!ok, "{text}");
    assert!(text.contains("error[E005]"), "{text}");
    assert!(!text.contains("came from `list.find`"), "another fn's let leaked into the hint:\n{text}");
    assert!(!text.contains("find_index"), "no sibling may be claimed for a parameter:\n{text}");
    assert!(text.contains("Fix the argument type"), "the generic hint must stay:\n{text}");
}

#[test]
fn a_param_shadowing_a_let_in_the_same_fn_drops_the_origin() {
    // Same fn: the `let hit` is real, but the lambda's parameter `hit` shadows
    // it at the E005 site. A non-`let` binding clears the origin.
    let (ok, text) = check(
        "fn at(i: Int?) -> Int = i ?? -1\n\
         fn main() -> Unit = {\n\
         \x20 let hit = list.find([\"a\"], (s) => s == \"a\")\n\
         \x20 let f = (hit: String?) => at(hit)\n\
         \x20 println(int.to_string(f(hit)))\n\
         }\n",
    );
    assert!(!ok, "{text}");
    assert!(text.contains("error[E005]"), "{text}");
    assert!(!text.contains("came from `list.find`"), "the shadowed let leaked into the hint:\n{text}");
    assert!(!text.contains("find_index"), "no sibling may be claimed for a parameter:\n{text}");
}

#[test]
fn without_a_sibling_the_generic_hint_stays() {
    // `list.len` has no String form: a conversion, not a rename, so no
    // sibling is claimed and the wording is what it was. (`list.sum` served
    // here until it became generic over `Numeric` and stopped mismatching.)
    let (ok, text) = check(
        "fn total(xs: List[String]) -> String = string.trim(list.len(xs))\n\
         fn main() -> Unit = println(total([\"a\"]))\n",
    );
    assert!(!ok, "{text}");
    assert!(text.contains("error[E005]"), "{text}");
    assert!(!text.contains("which is what this argument expects"), "no sibling exists here:\n{text}");
    assert!(text.contains("Fix the argument type"), "the generic hint must stay:\n{text}");
}
