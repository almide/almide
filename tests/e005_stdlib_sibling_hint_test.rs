//! E005 names a same-stem stdlib sibling that returns the expected type (#2097).
//!
//! `fs.read_bytes` returns `List[Int]` and `fs.read_bytes_raw` returns `Bytes`,
//! four lines apart in `stdlib/fs.almd`. "Fix the argument type" carried no
//! information the message did not; the sibling is a lookup, so the hint
//! names it and the `try:` line is the renamed call.

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
        "import fs\n\
         effect fn main() -> Unit = {\n\
         \x20 let b = fs.read_bytes(\"Cargo.toml\")!\n\
         \x20 println(int.to_string(bytes.len(bytes.slice(b, 0, 16))))\n\
         }\n",
    );
    assert!(!ok, "the mismatch must still be E005:\n{text}");
    assert!(text.contains("error[E005]"), "{text}");
    assert!(text.contains("`fs.read_bytes_raw` returns"), "sibling not named:\n{text}");
    assert!(text.contains("fs.read_bytes_raw(\"Cargo.toml\")!"), "try line must be the renamed call:\n{text}");
    assert!(!text.contains("Fix the argument type"), "the generic hint must give way:\n{text}");
}

#[test]
fn without_a_sibling_the_generic_hint_stays() {
    // `list.sum` has no Float form: a conversion, not a rename, so no
    // sibling is claimed and the wording is what it was.
    let (ok, text) = check(
        "fn total(xs: List[Float]) -> Int = list.sum(xs)\n\
         fn main() -> Unit = println(int.to_string(total([1.0, 2.0])))\n",
    );
    assert!(!ok, "{text}");
    assert!(text.contains("error[E005]"), "{text}");
    assert!(!text.contains("which is what this argument expects"), "no sibling exists here:\n{text}");
}
