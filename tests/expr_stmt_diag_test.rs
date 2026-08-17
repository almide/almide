//! The expression statement's negative half, cited by ALS-S3 (the accepted
//! forms are pinned by `spec/wasm_cross/expr_stmt_comment.almd`, C-252): a
//! statement-position call whose Result is DISCARDED is E042 (ADR-0008
//! must-use), and the hint must spell both escapes — `expr!` propagates,
//! `let _ = expr` deliberately discards.

use std::process::Command;

fn almide() -> &'static str {
    env!("CARGO_BIN_EXE_almide")
}

#[test]
fn discarded_result_statement_is_e042_with_both_escapes() {
    let dir = tempfile::tempdir().expect("tempdir");
    let file = dir.path().join("e042.almd");
    std::fs::write(
        &file,
        "import fs\n\neffect fn main() -> Unit = {\n  fs.read_text(\"/tmp/x\")\n  println(\"done\")\n}\n",
    )
    .expect("write fixture");
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
    assert!(text.contains("E042"), "discarded Result must be E042, got:\n{text}");
    assert!(
        text.contains("discards a Result"),
        "E042 must say what is being discarded, got:\n{text}"
    );
}
