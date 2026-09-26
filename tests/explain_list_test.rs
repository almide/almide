//! `almide explain --list --json` (#2149) as a drift gate.
//!
//! The listing is one row per documented code. This test keeps it honest
//! against the compiler itself:
//!
//! 1. The row set equals the set of codes the compiler EMITS: every
//!    `with_code("EXXX")` under `crates/`, plus the codes the build path
//!    prints as a literal `error[EXXX]` header without a `Diagnostic`
//!    (E081–E083 today). A new code without a doc, or a doc whose code
//!    nothing emits any more, fails here.
//! 2. Every row is complete: a title, a severity from the closed set, a
//!    `since` version, a stated fix-it verdict.
//! 3. Every severity is what the binary emits: each fixture under
//!    `tests/diagnostics/` is checked, and a diagnostic whose level the
//!    registry row does not name fails.

use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};
use std::process::Command;

fn root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
}

fn list_rows() -> Vec<serde_json::Value> {
    // From a temp dir: the listing must come from the binary alone.
    let tmp = tempfile::tempdir().expect("tempdir");
    let out = Command::new(env!("CARGO_BIN_EXE_almide"))
        .args(["explain", "--list", "--json"])
        .current_dir(tmp.path())
        .output()
        .expect("almide explain --list --json");
    assert!(out.status.success(), "explain --list failed: {}", String::from_utf8_lossy(&out.stderr));
    let v: serde_json::Value = serde_json::from_slice(&out.stdout).expect("explain --list --json is JSON");
    v.as_array().expect("a JSON array").clone()
}

/// Codes the compiler emits, read from source.
fn emitted_codes() -> BTreeSet<String> {
    fn walk(dir: &Path, codes: &mut BTreeSet<String>) {
        let Ok(entries) = std::fs::read_dir(dir) else { return };
        for entry in entries.flatten() {
            let path = entry.path();
            let name = path.file_name().and_then(|n| n.to_str()).unwrap_or("");
            if path.is_dir() {
                if name == "target" || name == "tests" { continue; }
                walk(&path, codes);
            } else if name.ends_with(".rs") {
                let text = std::fs::read_to_string(&path).unwrap_or_default();
                for line in text.lines() {
                    if line.trim_start().starts_with("//") { continue; }
                    for (open, close) in [("with_code(\"E", '"'), ("error[E", ']')] {
                        let mut rest = line;
                        while let Some(i) = rest.find(open) {
                            rest = &rest[i + open.len()..];
                            let digits: String = rest.chars().take_while(|c| c.is_ascii_digit()).collect();
                            // Almide codes are three digits; rustc's are four
                            // (`error[E0428]` in a comment-free string) and
                            // are not ours.
                            if digits.len() == 3 && rest[digits.len()..].starts_with(close) {
                                codes.insert(format!("E{digits}"));
                            }
                        }
                    }
                }
            }
        }
    }
    let mut codes = BTreeSet::new();
    walk(&root().join("crates"), &mut codes);
    walk(&root().join("src"), &mut codes);
    codes
}

#[test]
fn explain_list_rows_equal_the_emitted_codes() {
    let rows = list_rows();
    let listed: BTreeSet<String> = rows.iter().map(|r| r["code"].as_str().unwrap().to_string()).collect();
    assert_eq!(listed.len(), rows.len(), "duplicate codes in explain --list");
    let emitted = emitted_codes();
    let unlisted: Vec<&String> = emitted.difference(&listed).collect();
    let dead: Vec<&String> = listed.difference(&emitted).collect();
    assert!(
        unlisted.is_empty() && dead.is_empty(),
        "explain --list ({} rows) drifted from the {} emitted codes — emitted but not listed \
         (add docs/diagnostics/<CODE>.md and a codes.toml row): {:?}; listed but never emitted: {:?}",
        rows.len(), emitted.len(), unlisted, dead
    );
    eprintln!("explain --list: {} rows = {} emitted codes", rows.len(), emitted.len());
}

#[test]
fn every_explain_list_row_is_complete() {
    let mut bad = Vec::new();
    for r in list_rows() {
        let f = |k: &str| r[k].as_str().unwrap_or("").to_string();
        let since_ok = {
            let s = f("since");
            let parts: Vec<&str> = s.split('.').collect();
            parts.len() == 3 && parts.iter().all(|p| !p.is_empty() && p.chars().all(|c| c.is_ascii_digit()))
        };
        if f("mnemonic").is_empty()
            || !matches!(f("severity").as_str(), "error" | "warning" | "error|warning")
            || !since_ok
            || !matches!(f("verdict").as_str(), "mechanical" | "conditional" | "not-fixable")
        {
            bad.push(r.to_string());
        }
    }
    assert!(bad.is_empty(), "incomplete explain --list rows:\n{}", bad.join("\n"));
}

#[test]
fn every_severity_matches_what_the_binary_emits() {
    let severity: BTreeMap<String, String> = list_rows()
        .iter()
        .map(|r| (r["code"].as_str().unwrap().to_string(), r["severity"].as_str().unwrap().to_string()))
        .collect();
    let mut seen = 0usize;
    let mut wrong: BTreeSet<String> = BTreeSet::new();
    let dir = root().join("tests/diagnostics");
    for entry in std::fs::read_dir(&dir).expect("tests/diagnostics").flatten() {
        let broken = entry.path().join("broken.almd");
        if !broken.exists() { continue; }
        let out = Command::new(env!("CARGO_BIN_EXE_almide"))
            .args(["check", "--json"])
            .arg(&broken)
            .output()
            .expect("almide check --json");
        for d in String::from_utf8_lossy(&out.stdout).lines()
            .filter_map(|l| serde_json::from_str::<serde_json::Value>(l).ok())
        {
            let (Some(code), Some(level)) = (d["code"].as_str(), d["level"].as_str()) else { continue };
            if code.is_empty() { continue; }
            seen += 1;
            let declared = severity.get(code).map(String::as_str).unwrap_or("");
            if !declared.split('|').any(|s| s == level) {
                wrong.insert(format!("{code}: emitted as {level}, codes.toml says {declared:?}"));
            }
        }
    }
    assert!(seen > 500, "only {seen} coded diagnostics across the fixtures — the scan went vacuous");
    assert!(wrong.is_empty(), "docs/diagnostics/codes.toml severity drifted:\n{}", wrong.into_iter().collect::<Vec<_>>().join("\n"));
}
