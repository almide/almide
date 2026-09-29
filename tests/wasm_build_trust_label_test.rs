//! The one line `almide build --target wasm` prints names the leg that
//! produced the bytes AND the trust that leg has actually earned (#2184).
//!
//! The structural leg — the only wasm leg since #2752 — is trusted end to end
//! with its certificate pending (#1696, docs/contracts/proven-vs-trusted.md): saying `verified`
//! there attached the word to output no certificate covered, and #2154's
//! run-time trap shipped under it.
use std::process::Command;

const PROGRAM: &str = "effect fn main() -> Unit = println(\"hello\")\n";

fn almide() -> String {
    std::env::var("ALMIDE_BIN").unwrap_or_else(|_| format!("{}/target/release/almide", env!("CARGO_MANIFEST_DIR")))
}

fn build_line(dir: &std::path::Path) -> String {
    let src = dir.join("main.almd");
    std::fs::write(&src, PROGRAM).unwrap();
    let mut cmd = Command::new(almide());
    cmd.current_dir(dir).args(["build", "main.almd", "--target", "wasm", "-o", "out.wasm"]);
    let out = cmd.output().unwrap();
    assert!(out.status.success(), "{}", String::from_utf8_lossy(&out.stderr));
    let stderr = String::from_utf8_lossy(&out.stderr).to_string();
    stderr.lines().find(|l| l.starts_with("Built ")).unwrap_or_else(|| panic!("no Built line:\n{stderr}")).to_string()
}

#[test]
fn the_structural_leg_is_trusted_with_its_certificate_pending() {
    let dir = tempfile::tempdir().unwrap();
    let line = build_line(dir.path());
    assert!(line.contains("structural leg"), "{line}");
    assert!(line.contains("trusted, certificate pending"), "{line}");
    assert!(!line.contains("verified"), "no certificate covers these bytes yet:\n{line}");
}
