//! #2129: the regex engine answers what an engine OUTSIDE this project
//! answers, on both legs.
//!
//! C-032 already fuzzes the engine — with `runtime/rs/src/regex.rs`, *our own*
//! native engine, as the oracle. That certifies the two legs agree and is blind
//! to a rule both read the same way and both read wrong, which is the failure
//! shape that made this repo's own gate authors stop calling the module: a
//! silent mis-read looks exactly like a correct answer.
//!
//! Two such rules were wrong, and both were invisible to every gate here:
//!
//! - `[]]` — a `]` in the FIRST position of a class is a literal member (POSIX;
//!   PCRE, Python, Rust, Go and JS all follow it). Read as the terminator, the
//!   pattern became an empty class plus a stray `]` and never matched. No
//!   error, no match, nothing to see.
//! - `split` over a pattern that can match empty conflated the split point with
//!   the scan position, so a zero-width match ATE a character:
//!   `split("^", "abc")` answered `["a", "bc"]`.
//!
//! The table lives in `proofs/regex/reference-answers.tsv` and its subset is
//! documented in `proofs/regex/README.md` — patterns where the reference
//! engines disagree with each other are deliberately out, so a difference here
//! is a defect rather than a dialect. The shell gate
//! `scripts/check-regex-reference.sh` runs the same comparison in CI; this test
//! is the cargo-side entry so a `cargo test` run cannot miss it.

use std::io::Write;
use std::process::{Command, Stdio};

fn repo_root() -> std::path::PathBuf {
    std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
}

fn wasmtime_available() -> bool {
    Command::new("wasmtime").arg("--version").output().is_ok_and(|o| o.status.success())
}

/// Ask one leg every question in the table; answers come back one per line.
fn answers(target: Option<&str>, cases: &str) -> Vec<String> {
    let probe = repo_root().join("proofs/regex/probe.almd");
    let mut cmd = Command::new(env!("CARGO_BIN_EXE_almide"));
    cmd.arg("run").arg(&probe);
    if let Some(t) = target {
        cmd.arg("--target").arg(t);
    }
    let mut child = cmd
        .current_dir(repo_root())
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("spawn almide");
    child.stdin.as_mut().expect("stdin").write_all(cases.as_bytes()).expect("write cases");
    let out = child.wait_with_output().expect("wait almide");
    assert!(
        out.status.success(),
        "the {} leg could not answer the probe: {}",
        target.unwrap_or("native"),
        String::from_utf8_lossy(&out.stderr)
    );
    String::from_utf8_lossy(&out.stdout).lines().map(str::to_string).collect()
}

#[test]
fn both_legs_answer_what_an_outside_engine_answers() {
    if !wasmtime_available() {
        eprintln!("skipping: wasmtime not on PATH");
        return;
    }
    let table = std::fs::read_to_string(repo_root().join("proofs/regex/reference-answers.tsv"))
        .expect("proofs/regex/reference-answers.tsv");
    let rows: Vec<(&str, &str)> = table
        .lines()
        .filter(|l| !l.is_empty())
        .map(|l| l.rsplit_once('\t').expect("op/pattern/subject/answer row"))
        .collect();
    // A table that shrank is a gate that stopped looking: the subset is grow-only.
    assert!(rows.len() >= 6000, "the reference table has only {} rows", rows.len());
    let cases: String = rows.iter().map(|(c, _)| format!("{c}\n")).collect();

    for target in [None, Some("wasm")] {
        let got = answers(target, &cases);
        assert_eq!(
            got.len(),
            rows.len(),
            "the {} leg answered {} of {} cases",
            target.unwrap_or("native"),
            got.len(),
            rows.len()
        );
        let mut diffs = Vec::new();
        for ((case, want), have) in rows.iter().zip(&got) {
            if want != have {
                diffs.push(format!("  {case}\n    reference {want}\n    answered  {have}"));
            }
        }
        assert!(
            diffs.is_empty(),
            "the {} leg differs from the reference on {} of {} cases:\n{}",
            target.unwrap_or("native"),
            diffs.len(),
            rows.len(),
            diffs[..diffs.len().min(10)].join("\n")
        );
    }
}
