//! #1308 — a stray character no lexer rule matches must be a loud parse
//! error, never a silent truncation. The old lexer turned unknown chars into
//! EOF-typed placeholder tokens, so the parser stopped at the first stray
//! non-ASCII character and dropped the rest of the file with every gate
//! green (`almide check` clean, `almide run` degrading to "running 0 tests",
//! exit 0 throughout).

use std::process::Command;

fn almide() -> &'static str {
    env!("CARGO_BIN_EXE_almide")
}

fn check(dir: &std::path::Path, source: &str) -> (String, bool) {
    let file = dir.join("garbage.almd");
    std::fs::write(&file, source).expect("write fixture");
    let out = Command::new(almide())
        .arg("check")
        .arg(&file)
        .output()
        .expect("run almide check");
    let text = format!(
        "{}{}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    );
    (text, out.status.success())
}

const VALID_TAIL: &str = "fn main() -> Unit = {\n  println(\"ok\")\n}\n";

#[test]
fn trailing_garbage_is_an_error_not_a_silent_pass() {
    let dir = tempfile::tempdir().expect("tempdir");
    let (out, ok) = check(dir.path(), &format!("{VALID_TAIL}\nあああああ\n"));
    assert!(!ok, "trailing garbage must fail check, got:\n{out}");
    assert!(
        out.contains("Unexpected character 'あ' (U+3042)"),
        "must name the character and code point, got:\n{out}"
    );
}

#[test]
fn leading_garbage_is_an_error_not_a_truncated_empty_program() {
    let dir = tempfile::tempdir().expect("tempdir");
    let (out, ok) = check(dir.path(), &format!("あああああ\n\n{VALID_TAIL}"));
    assert!(!ok, "leading garbage must fail check, got:\n{out}");
    assert!(out.contains("U+3042"), "got:\n{out}");
}

#[test]
fn midfile_garbage_is_an_error_not_a_dropped_second_half() {
    // The worst old case: everything after the stray char — including test
    // blocks — vanished while check/test/run all stayed green.
    let dir = tempfile::tempdir().expect("tempdir");
    let (out, ok) = check(
        dir.path(),
        &format!("fn helper() -> Int = 1\n\n。\n\n{VALID_TAIL}"),
    );
    assert!(!ok, "mid-file garbage must fail check, got:\n{out}");
    assert!(
        out.contains("Unexpected character '。' (U+3002)"),
        "got:\n{out}"
    );
}

#[test]
fn fullwidth_space_is_named_not_swallowed() {
    let dir = tempfile::tempdir().expect("tempdir");
    let (out, ok) = check(
        dir.path(),
        &format!("fn helper() -> Int = 1\n　\n{VALID_TAIL}"),
    );
    assert!(!ok, "U+3000 outside strings must fail check, got:\n{out}");
    assert!(out.contains("U+3000"), "got:\n{out}");
}

#[test]
fn non_ascii_inside_strings_and_comments_still_fine() {
    let dir = tempfile::tempdir().expect("tempdir");
    let (out, ok) = check(
        dir.path(),
        "// コメントは自由 — 全角。も可\nfn main() -> Unit = {\n  println(\"あああああ。\")\n}\n",
    );
    assert!(ok, "non-ASCII in strings/comments must stay legal, got:\n{out}");
}

#[test]
fn leading_utf8_bom_is_stripped_not_an_error() {
    let dir = tempfile::tempdir().expect("tempdir");
    let (out, ok) = check(dir.path(), &format!("\u{feff}{VALID_TAIL}"));
    assert!(ok, "a leading BOM must be stripped, got:\n{out}");
}
