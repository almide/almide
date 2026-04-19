//! Diagnostic fixture harness — the Phase 2 substrate of
//! `docs/roadmap/active/diagnostics-here-try-hint.md`.
//!
//! Each directory under `tests/diagnostics/<case>/` is one fixture:
//!
//! ```
//! tests/diagnostics/bang-not/
//!   broken.almd   // fails to compile with a specific diagnostic
//!   fixed.almd    // compiles cleanly — the suggested fix
//!   meta.toml     // expected diagnostic substrings / code
//! ```
//!
//! The harness enforces two invariants per case:
//!
//! 1. `broken.almd` produces at least one error, and (if `meta.toml`
//!    declares expectations) the error payload contains the expected
//!    code / substring / hint.
//! 2. `fixed.almd` compiles cleanly — zero errors from `almide check`.
//!
//! This is the foundation the `Try:` snippet auto-apply phase will
//! build on; right now the fixtures only pin "fix exists and
//! compiles". When individual diagnostics gain populated `try:`
//! snippets, a follow-up test will mechanically apply them and
//! assert equality with `fixed.almd`.
//!
//! Running: `cargo test --test diagnostic_harness_test`.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::process::Command;

fn almide() -> &'static str {
    env!("CARGO_BIN_EXE_almide")
}

#[derive(Default, Debug)]
struct Meta {
    expects_error: Option<String>,
    expects_code: Option<String>,
    hint_substring: Option<String>,
}

fn parse_meta(path: &Path) -> Meta {
    let Ok(text) = std::fs::read_to_string(path) else { return Meta::default(); };
    let mut fields: HashMap<&str, String> = HashMap::new();
    for line in text.lines() {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') { continue; }
        let Some((key, rest)) = line.split_once('=') else { continue };
        let key = key.trim();
        let value = rest.trim().trim_matches('"').to_string();
        fields.insert(
            match key {
                "expects_error" => "expects_error",
                "expects_code" => "expects_code",
                "hint_substring" => "hint_substring",
                _ => continue,
            },
            value,
        );
    }
    Meta {
        expects_error: fields.remove("expects_error"),
        expects_code: fields.remove("expects_code"),
        hint_substring: fields.remove("hint_substring"),
    }
}

fn fixtures_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/diagnostics")
}

fn collect_cases() -> Vec<PathBuf> {
    let root = fixtures_root();
    let mut cases = Vec::new();
    if let Ok(entries) = std::fs::read_dir(&root) {
        for entry in entries.flatten() {
            if entry.file_type().map_or(false, |t| t.is_dir()) {
                cases.push(entry.path());
            }
        }
    }
    cases.sort();
    cases
}

fn run_check(file: &Path) -> (bool, String, String) {
    let out = Command::new(almide())
        .args(["check", file.to_str().unwrap()])
        .output()
        .expect("almide check");
    let stdout = String::from_utf8_lossy(&out.stdout).to_string();
    let stderr = String::from_utf8_lossy(&out.stderr).to_string();
    (out.status.success(), stdout, stderr)
}

#[test]
fn every_case_has_broken_and_fixed_pair() {
    let cases = collect_cases();
    assert!(!cases.is_empty(), "no fixtures found under tests/diagnostics/");
    for case in &cases {
        assert!(
            case.join("broken.almd").exists(),
            "missing broken.almd in {}", case.display()
        );
        assert!(
            case.join("fixed.almd").exists(),
            "missing fixed.almd in {}", case.display()
        );
    }
}

#[test]
fn broken_files_produce_expected_diagnostics() {
    let cases = collect_cases();
    for case in &cases {
        let broken = case.join("broken.almd");
        let meta = parse_meta(&case.join("meta.toml"));
        let (success, stdout, stderr) = run_check(&broken);
        let combined = format!("{}{}", stdout, stderr);
        assert!(
            !success || combined.contains("error"),
            "broken.almd in {} unexpectedly passed:\nstdout: {}\nstderr: {}",
            case.display(), stdout, stderr
        );
        if let Some(code) = &meta.expects_code {
            assert!(
                combined.contains(&format!("[{}]", code))
                || combined.contains(&format!("error[{}]", code)),
                "expected code {} not found in diagnostic for {}:\n{}",
                code, case.display(), combined
            );
        }
        if let Some(err) = &meta.expects_error {
            assert!(
                combined.contains(err),
                "expected error substring {:?} not in diagnostic for {}:\n{}",
                err, case.display(), combined
            );
        }
        if let Some(hint) = &meta.hint_substring {
            assert!(
                combined.to_lowercase().contains(&hint.to_lowercase()),
                "expected hint substring {:?} not in diagnostic for {}:\n{}",
                hint, case.display(), combined
            );
        }
    }
}

#[test]
fn fixed_files_compile_cleanly() {
    let cases = collect_cases();
    for case in &cases {
        let fixed = case.join("fixed.almd");
        let (success, stdout, stderr) = run_check(&fixed);
        assert!(
            success,
            "fixed.almd in {} failed to compile:\nstdout: {}\nstderr: {}",
            case.display(), stdout, stderr
        );
    }
}
