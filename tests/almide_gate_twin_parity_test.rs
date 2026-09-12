//! #2130 step 1: the Almide gate twins answer what their `.sh` originals answer.
//!
//! Unit 0.46 ported seven gates to `tools/almide-gates/` with byte-identity
//! against the original as the stated acceptance check — and the check was
//! never built. CI runs only the `.sh` side; the twins are exercised by
//! `almide test tools/almide-gates/` and nothing ever compared the two. The
//! tool's own comment names what that costs:
//!
//! > the freshness gates compare the bash against the committed docs, never the
//! > port against either, so a reader that folded two contracts into one
//! > rendered a different index while every gate stayed green
//!
//! This is step 1 of the order that issue calls non-negotiable — build the
//! diff, watch it stay green, and only THEN promote. Nothing is promoted here
//! and no `.sh` is deleted.
//!
//! **It found drift on its first run.** `stamp`'s FATAL path (the PATH binary
//! disagreeing with the workspace build) had diverged twice: the bash had
//! learned to name the cause — a `cargo test` relinking `target/release` after
//! the last `make install` — while the port still carried the original
//! one-liner, and the bash RETURNS there while the port went on to print the
//! toolchain lines and the closing rule. Either would have shipped different
//! evidence text the day the twin was promoted.
//!
//! The two twins compared here are the ones that are hermetic and cheap.
//! `output-parity` sweeps the whole corpus and `fuzz-track-record` queries the
//! GitHub API; both need a fixture design of their own before they can join,
//! and that is tracked on #2130 rather than faked here with a comparison that
//! would be green for the wrong reason.

use std::process::Command;

fn repo_root() -> std::path::PathBuf {
    std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
}

fn almide() -> &'static str {
    env!("CARGO_BIN_EXE_almide")
}

struct Answer {
    code: Option<i32>,
    text: String,
}

fn run(program: &str, args: &[&str]) -> Answer {
    let out = Command::new(program)
        .args(args)
        .current_dir(repo_root())
        .output()
        .unwrap_or_else(|e| panic!("spawn {program}: {e}"));
    Answer {
        code: out.status.code(),
        text: format!(
            "{}{}",
            String::from_utf8_lossy(&out.stdout),
            String::from_utf8_lossy(&out.stderr)
        ),
    }
}

/// The twin, through the tool's subcommand dispatch.
fn twin(args: &[&str]) -> Answer {
    let main = repo_root().join("tools/almide-gates/src/main.almd");
    let mut argv = vec!["run", main.to_str().expect("path"), "--"];
    argv.extend_from_slice(args);
    run(almide(), &argv)
}

/// The first line that differs, named — a whole-output dump says only that
/// something moved, which is what makes drift tedious enough to ignore.
fn first_divergence(original: &str, port: &str) -> Option<String> {
    let (a, b): (Vec<_>, Vec<_>) = (original.lines().collect(), port.lines().collect());
    for i in 0..a.len().max(b.len()) {
        let (x, y) = (a.get(i).copied().unwrap_or(""), b.get(i).copied().unwrap_or(""));
        if x != y {
            return Some(format!("line {}\n  original: {x}\n  port:     {y}", i + 1));
        }
    }
    None
}

fn assert_same(label: &str, original: Answer, port: Answer) {
    assert_eq!(
        original.code, port.code,
        "{label}: the port exited {:?} where the original exited {:?}\n--- original ---\n{}\n--- port ---\n{}",
        port.code, original.code, original.text, port.text
    );
    if let Some(d) = first_divergence(&original.text, &port.text) {
        panic!("{label}: the port's output diverged from the original\n{d}");
    }
}

#[test]
fn the_contract_ledger_twin_answers_what_the_shell_gate_answers() {
    assert_same(
        "check-contracts",
        run("bash", &["scripts/check-contracts.sh"]),
        twin(&["check-contracts", "docs/contracts"]),
    );
}

/// `stamp` is sourced, not executed, so the original is invoked the way its
/// callers do. Both paths matter and which one runs depends on the machine:
/// the FATAL path when the PATH binary is stale, the toolchain listing when it
/// is not. The comparison covers whichever the machine is in — and the drift it
/// caught was on the FATAL path, which is the one a developer meets.
#[test]
fn the_toolchain_stamp_twin_answers_what_the_shell_gate_answers() {
    let root = repo_root();
    let root = root.to_str().expect("path");
    assert_same(
        "stamp",
        run("bash", &["-c", &format!("source proofs/lib/stamp.sh; stamp_toolchain '{root}'")]),
        twin(&["stamp", "."]),
    );
}

/// The comparison must be able to SEE a difference — a diff that cannot fail is
/// the same decoration as the missing check it replaces. A forged original is
/// the cheapest honest probe: it differs from the port by construction.
#[test]
fn the_comparison_names_the_first_differing_line() {
    assert_eq!(first_divergence("a\nb\nc\n", "a\nb\nc\n"), None);
    assert_eq!(
        first_divergence("a\nb\nc\n", "a\nB\nc\n").as_deref(),
        Some("line 2\n  original: b\n  port:     B")
    );
    assert_eq!(
        first_divergence("a\n", "a\nb\n").as_deref(),
        Some("line 2\n  original: \n  port:     b")
    );
}
