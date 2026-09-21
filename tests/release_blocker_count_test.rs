//! `scripts/count-release-blockers.sh` counts DISTINCT open blocker issues (#2400).
//!
//! The taxonomy lets one issue carry several blocker classes (`I-unsound` +
//! `I-divergence` is the common pair). The first version of the script summed the
//! per-label row counts, so four open blockers printed as 7 — every number a human
//! read off it (the pre-tag check, the workflow line, anything quoted into a seal)
//! was inflated, while `--gate` stayed correct because it only tests `> 0`.
//!
//! The script takes its listing from stdin under `--from-stdin` — the same
//! `label \t number \t title` shape it asks `gh` for — so this test feeds it a forged
//! overlap and reads the printed total, and pins the exit-code contract the
//! release workflow relies on (`--gate`: 1 with a blocker open, 0 with none).

use std::io::Write;
use std::path::PathBuf;
use std::process::{Command, Output, Stdio};

fn run(args: &[&str], listing: &str) -> Output {
    let script =
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("scripts/count-release-blockers.sh");
    let mut child = Command::new("bash")
        .arg(&script)
        .arg("--from-stdin")
        .args(args)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("spawn bash");
    child
        .stdin
        .take()
        .unwrap()
        .write_all(listing.as_bytes())
        .unwrap();
    child.wait_with_output().expect("wait for the script")
}

fn stdout(out: &Output) -> String {
    String::from_utf8_lossy(&out.stdout).into_owned()
}

fn total_line(out: &Output) -> String {
    stdout(out)
        .lines()
        .find(|l| l.starts_with("release-blockers: "))
        .map(str::to_owned)
        .unwrap_or_else(|| panic!("no `release-blockers:` line in:\n{}", stdout(out)))
}

/// The live tracker as measured on 2026-09-21 (#2400): four open issues — #2395 /
/// #2397 / #2398 under both I-unsound and I-divergence, #2396 under I-divergence
/// alone. 3 × I-unsound + 4 × I-divergence = seven memberships, and the summing
/// script printed `release-blockers: 7` for those four issues.
const OVERLAP: &str = "I-unsound\t2395\tvalue.keys walks memory\n\
                       I-unsound\t2397\tfold returns freed memory\n\
                       I-unsound\t2398\tcaptured String reads a wrong byte\n\
                       I-divergence\t2395\tvalue.keys walks memory\n\
                       I-divergence\t2396\tone-label blocker\n\
                       I-divergence\t2397\tfold returns freed memory\n\
                       I-divergence\t2398\tcaptured String reads a wrong byte\n";

#[test]
fn an_issue_under_two_blocker_labels_is_counted_once() {
    let out = run(&[], OVERLAP);
    assert!(
        out.status.success(),
        "report mode must exit 0:\n{}",
        stdout(&out)
    );
    assert_eq!(
        total_line(&out),
        "release-blockers: 4",
        "the total is the size of the issue SET, not the sum of the label rows (7):\n{}",
        stdout(&out)
    );
    let text = stdout(&out);
    // The breakdown survives so a reader can see the overlap — and the
    // memberships line says it in numbers.
    assert!(text.contains("I-unsound (3):"), "{text}");
    assert!(text.contains("I-divergence (4):"), "{text}");
    assert!(
        text.contains("label-memberships: 7 across 4 issues (3 carry more than one blocker label)"),
        "{text}"
    );
    // A label with no open issue prints nothing, as before.
    assert!(!text.contains("I-miscompile"), "{text}");
    assert!(!text.contains("regression"), "{text}");
}

#[test]
fn a_forged_summing_total_would_have_read_seven() {
    // The negative: the pre-#2400 arithmetic on this very listing. If the script
    // ever regresses to summing, the first test's `4` reads `7` — this pins that
    // the two numbers genuinely differ on the forged input, so the assertion above
    // is discriminating rather than vacuous.
    let memberships = OVERLAP.lines().count();
    let mut distinct: Vec<&str> = OVERLAP
        .lines()
        .map(|l| l.split('\t').nth(1).unwrap())
        .collect();
    distinct.sort_unstable();
    distinct.dedup();
    assert_eq!(memberships, 7);
    assert_eq!(distinct.len(), 4);
    assert_ne!(memberships, distinct.len());
}

#[test]
fn gate_exit_code_follows_the_distinct_count() {
    // One issue under two labels is still one open blocker: the gate refuses.
    let out = run(
        &["--gate"],
        "I-unsound\t10\tone issue\nI-divergence\t10\tone issue\n",
    );
    assert_eq!(
        out.status.code(),
        Some(1),
        "an open blocker must fail --gate:\n{}",
        stdout(&out)
    );
    assert_eq!(total_line(&out), "release-blockers: 1");
    assert!(
        String::from_utf8_lossy(&out.stderr).contains("must not ship over an open blocker"),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );

    // No blocker open: the gate passes with 0 printed.
    let out = run(&["--gate"], "");
    assert_eq!(out.status.code(), Some(0), "{}", stdout(&out));
    assert_eq!(total_line(&out), "release-blockers: 0");

    // Report mode never fails, whatever is open (the -rc branch of the workflow).
    let out = run(&[], OVERLAP);
    assert_eq!(out.status.code(), Some(0));
}

#[test]
fn an_unknown_argument_is_refused_not_ignored() {
    // A typo in the workflow (`--gates`) must not silently run as report mode
    // and let a final tag through.
    let out = run(&["--gates"], OVERLAP);
    assert_eq!(
        out.status.code(),
        Some(2),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
}
