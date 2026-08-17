//! The identifier expression's negative half, cited by ALS-E17 (resolution
//! to the nearest binding is pinned by the S1 shadowing fixture and
//! `spec/wasm_cross/string_interpolation.almd`, C-246): an unresolved name
//! is check-time E003, and the diagnostic must carry the name it failed to
//! resolve — a bare "undefined" with no name would leave an LLM guessing
//! which of its identifiers was wrong.

use std::process::Command;

fn almide() -> &'static str {
    env!("CARGO_BIN_EXE_almide")
}

#[test]
fn unresolved_identifier_is_e003_naming_the_ident() {
    let dir = tempfile::tempdir().expect("tempdir");
    let file = dir.path().join("ident.almd");
    std::fs::write(
        &file,
        "effect fn main() -> Unit = {\n  println(int.to_string(missing_var))\n}\n",
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
    assert!(text.contains("E003"), "unresolved ident must be E003, got:\n{text}");
    assert!(
        text.contains("missing_var"),
        "E003 must NAME the unresolved identifier, got:\n{text}"
    );
}
