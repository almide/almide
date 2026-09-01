//! `<>` generics are the single most likely transcription slip for an LLM
//! trained on Rust/TS/Java — and CHEATSHEET's "Common mistakes" block
//! predicts exactly this spelling. The steer must fire in EVERY position the
//! mistake appears (#1736): the declaration head (`fn id<T>(x: T)` — the
//! highest-frequency form), the type annotation (`let xs: List<Int> = []`),
//! and the expression position that was already wired. A generic
//! "expected token" fallback names no fix and costs the writer a retry.

use std::process::Command;

fn almide() -> &'static str {
    env!("CARGO_BIN_EXE_almide")
}

fn check_hint(name: &str, src: &str) -> String {
    let dir = tempfile::tempdir().expect("tempdir");
    let file = dir.path().join(name);
    std::fs::write(&file, src).expect("write fixture");
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
fn declaration_head_angle_generics_steer_to_brackets() {
    let text = check_hint("gd.almd", "fn id<T>(x: T) -> T = x\n");
    assert!(
        text.contains("Use [] for generics"),
        "`fn id<T>` must steer to `[]`, got:\n{text}"
    );
}

#[test]
fn type_annotation_angle_generics_steer_to_brackets() {
    let text = check_hint(
        "ga.almd",
        "fn main() -> Unit = {\n  let xs: List<Int> = []\n  println(int.to_string(list.len(xs)))\n}\n",
    );
    assert!(
        text.contains("Use [] for generics"),
        "`List<Int>` in an annotation must steer to `[]`, got:\n{text}"
    );
}
