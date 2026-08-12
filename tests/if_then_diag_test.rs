//! `if` has exactly one arm spelling: `then` (optionally followed by a brace
//! block). The Rust-style `if c { ... }` must be rejected WITH the steer —
//! it is the single most likely transcription slip for an LLM trained on
//! brace languages, and a bare "expected token" error would not name the fix.
//! ALS-E13 (docs/specs/als/expressions.md) cites this as the negative half;
//! the accepted forms are pinned by `spec/wasm_cross/if_block_forms.almd`
//! (C-242).

use std::process::Command;

fn almide() -> &'static str {
    env!("CARGO_BIN_EXE_almide")
}

#[test]
fn brace_if_without_then_steers_to_then() {
    let dir = tempfile::tempdir().expect("tempdir");
    let file = dir.path().join("ifb.almd");
    std::fs::write(
        &file,
        "effect fn main() -> Unit = {\n  let a = 5\n  if a > 3 {\n    println(\"x\")\n  }\n}\n",
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
    assert!(
        text.contains("if requires 'then'"),
        "`if c {{` must steer to `then`, got:\n{text}"
    );
}
