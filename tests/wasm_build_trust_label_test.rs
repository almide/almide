//! The one line `almide build --target wasm` prints names the leg that
//! produced the bytes AND the trust that leg has actually earned (#2184).
//!
//! The incumbent v1 leg's module carries the per-function ownership
//! certificate the kernel-proven checker re-verifies: `verified`. The
//! structural leg — the default — is trusted end to end with its certificate
//! pending (#1696, docs/contracts/proven-vs-trusted.md): saying `verified`
//! there attached the word to output no certificate covered, and #2154's
//! run-time trap shipped under it.
use std::process::Command;

const PROGRAM: &str = "effect fn main() -> Unit = println(\"hello\")\n";

fn almide() -> String {
    std::env::var("ALMIDE_BIN").unwrap_or_else(|_| format!("{}/target/release/almide", env!("CARGO_MANIFEST_DIR")))
}

fn build_line(dir: &std::path::Path, envs: &[(&str, &str)]) -> String {
    let src = dir.join("main.almd");
    std::fs::write(&src, PROGRAM).unwrap();
    let mut cmd = Command::new(almide());
    cmd.current_dir(dir).args(["build", "main.almd", "--target", "wasm", "-o", "out.wasm"]);
    for (k, v) in envs { cmd.env(k, v); }
    let out = cmd.output().unwrap();
    assert!(out.status.success(), "{}", String::from_utf8_lossy(&out.stderr));
    let stderr = String::from_utf8_lossy(&out.stderr).to_string();
    stderr.lines().find(|l| l.starts_with("Built ")).unwrap_or_else(|| panic!("no Built line:\n{stderr}")).to_string()
}

#[test]
fn the_structural_leg_is_trusted_with_its_certificate_pending() {
    let dir = tempfile::tempdir().unwrap();
    let line = build_line(dir.path(), &[]);
    assert!(line.contains("structural leg"), "{line}");
    assert!(line.contains("trusted, certificate pending"), "{line}");
    assert!(!line.contains("verified"), "the word belongs to the certified leg only:\n{line}");
}

#[test]
fn the_incumbent_leg_is_verified() {
    let dir = tempfile::tempdir().unwrap();
    let line = build_line(dir.path(), &[("ALMIDE_WASM_INCUMBENT", "1")]);
    assert!(line.contains("incumbent v1 leg"), "{line}");
    assert!(line.contains(", verified"), "{line}");
    assert!(!line.contains("certificate pending"), "{line}");
}
