//! `almide check --timings` — the front-end phase accounting (#1311).
//!
//! The `almide-timings` line is not decoration: it is the INPUT to the
//! per-phase throughput ratchet (`scripts/check-edit-loop-scale.sh`, via
//! `research/benchmark/editloop/scale.py`). That gate computes phase SHARES from
//! these keys, so a renamed key or a phase that silently reports zero would turn
//! a shrink-only ratchet into a number generator. The gate has its own blindness
//! floors for exactly that, but they only fire during a 40-run ladder on a
//! 30k-line corpus — these assertions fire in `cargo test`, in milliseconds,
//! against the same contract.
//!
//! What is asserted here is the SHAPE and the SIGN, never a duration: a wall
//! clock on a shared runner is the one thing this whole subsystem refuses to
//! anchor (see the script header for the measured 3x load swing).

use std::process::Command;

fn almide() -> &'static str {
    env!("CARGO_BIN_EXE_almide")
}

const SRC: &str = "fn twice(n: Int) -> Int = n * 2\n\
                   fn main() -> Unit = {\n  println(int.to_string(twice(21)))\n}\n";

fn check_with(args: &[&str]) -> String {
    let dir = tempfile::tempdir().expect("tempdir");
    let file = dir.path().join("t.almd");
    std::fs::write(&file, SRC).expect("write fixture");
    let mut cmd = Command::new(almide());
    cmd.arg("check");
    for a in args {
        cmd.arg(a);
    }
    let out = cmd.arg(&file).output().expect("run almide check");
    format!(
        "{}{}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    )
}

/// The machine-readable line the ratchet parses: present, one JSON object, with
/// every key the harness reads.
#[test]
fn timings_emits_the_machine_readable_line_with_every_key() {
    let out = check_with(&["--timings"]);
    let line = out
        .lines()
        .find(|l| l.starts_with("almide-timings "))
        .unwrap_or_else(|| panic!("no `almide-timings` line in:\n{out}"));
    for key in [
        "lex_ns", "parse_ns", "check_ns", "total_ns", "lines", "bytes", "sources",
    ] {
        assert!(
            line.contains(&format!("\"{key}\":")),
            "the ratchet reads `{key}` off this line; got:\n{line}"
        );
    }
}

/// Every accounted phase must report real work. A phase reading 0 is the
/// instrumentation hole the ladder's blindness floor exists for — but a zero
/// also silently poisons the SHARE it appears in, so it is worth catching here
/// too, where the failure names itself in one second instead of forty runs.
#[test]
fn every_phase_reports_nonzero_time() {
    let out = check_with(&["--timings"]);
    let line = out
        .lines()
        .find(|l| l.starts_with("almide-timings "))
        .unwrap_or_else(|| panic!("no `almide-timings` line in:\n{out}"));
    for key in ["lex_ns", "parse_ns", "check_ns"] {
        let v = json_u64(line, key);
        assert!(
            v > 0,
            "`{key}` reported 0 — the phase_scope for it is gone, and every share \
             computed from it would be fiction. Line:\n{line}"
        );
    }
    // The three must fit inside the process's own front-end span; if they do
    // not, the spans are nesting and the same wall time is billed twice.
    let sum = json_u64(line, "lex_ns") + json_u64(line, "parse_ns") + json_u64(line, "check_ns");
    let total = json_u64(line, "total_ns");
    assert!(
        sum <= total,
        "phases sum to {sum}ns but the whole front end took {total}ns — double counting"
    );
}

/// The denominator of every lines/sec number. It counts what the front end
/// ACTUALLY lexed — this 3-line fixture plus the auto-imported bundled stdlib —
/// so it can never be smaller than the fixture, and a collapse to ~0 means the
/// counter stopped being fed.
#[test]
fn source_counters_cover_the_bundled_stdlib_not_just_the_entry() {
    let out = check_with(&["--timings"]);
    let line = out
        .lines()
        .find(|l| l.starts_with("almide-timings "))
        .expect("almide-timings line");
    assert!(
        json_u64(line, "sources") > 1,
        "only the entry was counted; the bundled stdlib every check pays for is missing:\n{line}"
    );
    assert!(
        json_u64(line, "lines") > 100,
        "line counter reads implausibly low for entry + bundled stdlib:\n{line}"
    );
}

/// Off by default. The neighbouring wall-clock ratchet times this exact command,
/// so instrumentation that ran unasked would be measuring itself.
#[test]
fn plain_check_prints_no_timings() {
    let out = check_with(&[]);
    assert!(
        !out.contains("almide-timings"),
        "`almide check` without --timings must stay silent; got:\n{out}"
    );
    assert!(out.contains("No errors found"), "fixture must check clean:\n{out}");
}

fn json_u64(line: &str, key: &str) -> u64 {
    let needle = format!("\"{key}\":");
    let rest = &line[line.find(&needle).unwrap_or_else(|| panic!("{key} not in {line}"))
        + needle.len()..];
    let digits: String = rest.chars().take_while(|c| c.is_ascii_digit()).collect();
    digits.parse().unwrap_or_else(|_| panic!("`{key}` is not a number in {line}"))
}
