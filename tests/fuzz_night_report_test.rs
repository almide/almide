//! #2390: the nightly fuzz report says what its numbers are conditional on.
//!
//! Two silent under-counts lived in the report. A shard the runner reclaimed
//! uploads nothing, so its findings left the night's total and the total was
//! published unqualified — `findings=23` for a night that found 24, with the
//! qualifier (`shards=6/8`) stopping in the step log. And the issue body listed
//! at most 20 findings (`grep | head -60`, three lines each) without saying it
//! was a cap: 23 in the headline, 20 in the list, nothing to explain the gap.
//!
//! The workflow cannot run here, so these forge what it would hand the scripts:
//! a `download-artifact` shard layout with two shards absent, a findings dir
//! with more entries than the cap, and `fuzz-night:` lines as the verdict job
//! logs carry them (including the pre-#2390 shape, so history stays scoreable).

use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

fn repo_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
}

fn scratch(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("fuzz-night-report-{name}-{}", std::process::id()));
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).expect("scratch dir");
    dir
}

struct Run {
    code: Option<i32>,
    stdout: String,
    stderr: String,
}

fn bash(args: &[&str], env: &[(&str, &str)]) -> Run {
    let mut cmd = Command::new("bash");
    cmd.args(args).current_dir(repo_root());
    for (k, v) in env {
        cmd.env(k, v);
    }
    let out = cmd.output().expect("spawn bash");
    Run {
        code: out.status.code(),
        stdout: String::from_utf8_lossy(&out.stdout).into_owned(),
        stderr: String::from_utf8_lossy(&out.stderr).into_owned(),
    }
}

/// A shard that finished its budget: the fuzzer's own summary block is present.
fn complete_shard(root: &Path, shard: u32, generated: u32, elapsed_s: u32) {
    let d = root.join(format!("fuzz-shard-424242-{shard}"));
    fs::create_dir_all(&d).unwrap();
    fs::write(
        d.join("fuzz-output.txt"),
        format!(
            "  seed     = {}\n  [  120s] generated=300 clean=290 rejects=10 findings=0 walls=0 skipped=0 | 150.0 prog/min\n\n=== campaign summary ===\n  elapsed          = {elapsed_s}.0s\n  generated        = {generated}\n",
            6_787_872 + shard
        ),
    )
    .unwrap();
}

/// A shard the runner reclaimed mid-budget but AFTER its upload step ran —
/// only progress lines, no summary block. (A shard reclaimed before upload has
/// no directory at all; that is the `missing` case.)
fn truncated_shard(root: &Path, shard: u32, generated: u32, elapsed_s: u32) {
    let d = root.join(format!("fuzz-shard-424242-{shard}"));
    fs::create_dir_all(&d).unwrap();
    fs::write(
        d.join("fuzz-output.txt"),
        format!(
            "  seed     = {}\n  [  {elapsed_s}s] generated={generated} clean=100 rejects=1 findings=0 walls=0 skipped=0 | 150.0 prog/min\n",
            6_787_872 + shard
        ),
    )
    .unwrap();
}

fn finding(root: &Path, name: &str, kind: &str, index: u32) {
    let d = root.join(name);
    fs::create_dir_all(&d).unwrap();
    fs::write(
        d.join("meta.txt"),
        format!(
            "seed        = 541579861880\nindex       = {index}\nrung        = 1\nkind        = {kind}\nsummary     = stdout differs ({name})\nreproduce   = xtarget-fuzz replay --seed 541579861880 --index {index} --family all\n"
        ),
    )
    .unwrap();
}

fn field<'a>(line: &'a str, key: &str) -> &'a str {
    line.split_whitespace()
        .find_map(|kv| kv.strip_prefix(&format!("{key}=")))
        .unwrap_or_else(|| panic!("no {key}= on: {line}"))
}

#[test]
fn a_preempted_shard_is_named_and_the_count_is_conditional_in_the_heading() {
    let dir = scratch("preempted");
    let shards = dir.join("shards");
    // Run 33848741367's shape: 8 planned, shards 3 and 8 never uploaded.
    for s in [1, 2, 4, 5] {
        complete_shard(&shards, s, 1000, 300);
    }
    truncated_shard(&shards, 6, 400, 120);
    truncated_shard(&shards, 7, 450, 130);
    let outputs = dir.join("outputs.txt");
    fs::write(&outputs, "").unwrap();

    let r = bash(
        &["scripts/fuzz-night-verdict.sh", shards.to_str().unwrap(), "5", "8", "23", "0"],
        &[("GITHUB_OUTPUT", outputs.to_str().unwrap())],
    );
    assert_eq!(r.code, Some(0), "verdict must not fail on coverage loss\n{}{}", r.stdout, r.stderr);

    // The heading carries the denominator.
    assert!(
        r.stdout.lines().next().unwrap().contains("findings=23 of 6/8 shards"),
        "heading:\n{}",
        r.stdout
    );
    assert!(r.stdout.contains("(shards 3, 8) did not report"), "{}", r.stdout);
    assert!(r.stdout.contains("NOT in this count"), "{}", r.stdout);

    // The record line names the missing shards and states delivered vs planned.
    let line = r.stderr.lines().find(|l| l.starts_with("fuzz-night: ")).expect("record line");
    assert_eq!(field(line, "shards"), "4/8");
    assert_eq!(field(line, "reporting"), "6");
    assert_eq!(field(line, "missing"), "3,8");
    assert_eq!(field(line, "minutes_planned"), "40");
    // 4 x 300s + 120s + 130s = 1450s = 24.2 min of 40 = 60%
    assert_eq!(field(line, "minutes_delivered"), "24.2");
    assert_eq!(field(line, "delivered_pct"), "60");
    assert_eq!(field(line, "budget"), "partial");
    assert_eq!(field(line, "findings"), "23");
    assert!(r.stderr.contains("::warning::2 of 8 fuzz shard(s) did not report"), "{}", r.stderr);

    // The workflow carries the qualification through $GITHUB_OUTPUT.
    let out = fs::read_to_string(&outputs).unwrap();
    for want in ["reporting=6", "missing=3,8", "shards_planned=8", "delivered_pct=60", "budget=partial"] {
        assert!(out.lines().any(|l| l == want), "missing `{want}` in GITHUB_OUTPUT:\n{out}");
    }
    // The per-shard table names each shard by number.
    assert!(r.stdout.contains("| 6 | 6787878 | truncated | 400 | 120s |"), "{}", r.stdout);
}

#[test]
fn a_full_night_says_so_and_a_shard_short_of_budget_still_counts_at_the_bar() {
    let dir = scratch("full");
    let shards = dir.join("shards");
    for s in 1..=8 {
        complete_shard(&shards, s, 1000, 300);
    }
    let r = bash(&["scripts/fuzz-night-verdict.sh", shards.to_str().unwrap(), "5", "8", "0"], &[]);
    assert_eq!(r.code, Some(0), "{}{}", r.stdout, r.stderr);
    assert!(r.stdout.starts_with("## Nightly fuzz verdict — findings=0 of 8/8 shards"), "{}", r.stdout);
    let line = r.stderr.lines().find(|l| l.starts_with("fuzz-night: ")).unwrap();
    assert_eq!(field(line, "missing"), "none");
    assert_eq!(field(line, "delivered_pct"), "100");
    assert_eq!(field(line, "budget"), "full");
    assert!(!r.stdout.contains("did not report"), "{}", r.stdout);

    // One shard of eight reclaimed = 87.5% delivered: a streak night under the ruling.
    let dir = scratch("seven");
    let shards = dir.join("shards");
    for s in 1..=7 {
        complete_shard(&shards, s, 1000, 300);
    }
    let r = bash(&["scripts/fuzz-night-verdict.sh", shards.to_str().unwrap(), "5", "8", "0"], &[]);
    let line = r.stderr.lines().find(|l| l.starts_with("fuzz-night: ")).unwrap();
    assert_eq!(field(line, "missing"), "8");
    assert_eq!(field(line, "delivered_pct"), "87");
    assert_eq!(field(line, "budget"), "full");
    assert!(r.stdout.contains("findings=0 of 7/8 shards"), "{}", r.stdout);
}

#[test]
fn no_shard_at_all_is_an_infra_failure_that_names_every_shard() {
    let dir = scratch("none");
    let shards = dir.join("shards");
    fs::create_dir_all(&shards).unwrap();
    let r = bash(&["scripts/fuzz-night-verdict.sh", shards.to_str().unwrap(), "5", "8", "0"], &[]);
    assert_eq!(r.code, Some(1));
    let line = r.stderr.lines().find(|l| l.starts_with("fuzz-night: ")).unwrap();
    assert_eq!(field(line, "missing"), "1,2,3,4,5,6,7,8");
    assert_eq!(field(line, "budget"), "partial");
    assert!(r.stdout.contains("findings=0 of 0/8 shards"), "{}", r.stdout);
}

#[test]
fn a_night_over_the_cap_says_it_is_showing_twenty_of_n() {
    let dir = scratch("cap");
    let findings = dir.join("night-findings");
    for i in 0..23 {
        finding(&findings, &format!("OutputDivergence__case{i:02}"), "OutputDivergence", i);
    }
    finding(&findings, "Slow__wasm_leg", "Slow", 99);

    let r = bash(
        &[
            "scripts/fuzz-night-issue-body.sh",
            findings.to_str().unwrap(),
            "correctness",
            "https://example.invalid/run/1",
            "6",
            "8",
            "3,8",
        ],
        &[],
    );
    assert_eq!(r.code, Some(0), "{}{}", r.stdout, r.stderr);
    let body = &r.stdout;
    assert!(body.contains("recorded **23** unique finding(s)"), "{body}");
    assert!(body.contains("collected from **6 of 8** shard(s)"), "{body}");
    assert!(body.contains("**2 shard(s) (shards 3, 8) did not report**"), "{body}");
    assert!(body.contains("showing 20 of 23"), "{body}");
    assert!(body.contains("... 3 more finding(s) not shown (showing 20 of 23)"), "{body}");
    assert_eq!(body.matches("reproduce   = ").count(), 20, "{body}");
    // The perf-class finding is not in the correctness listing.
    assert!(!body.contains("Slow__wasm_leg"), "{body}");

    // The perf path: same renderer, its own class, under the cap so no cap text.
    let r = bash(
        &[
            "scripts/fuzz-night-issue-body.sh",
            findings.to_str().unwrap(),
            "slow",
            "https://example.invalid/run/1",
            "8",
            "8",
            "none",
        ],
        &[],
    );
    assert_eq!(r.code, Some(0), "{}{}", r.stdout, r.stderr);
    assert!(r.stdout.contains("recorded **1** perf-class Slow finding(s)"), "{}", r.stdout);
    assert!(r.stdout.contains("Finding summaries (1)"), "{}", r.stdout);
    assert!(!r.stdout.contains("showing"), "{}", r.stdout);
    assert!(!r.stdout.contains("did not report"), "{}", r.stdout);
    assert_eq!(r.stdout.matches("reproduce   = ").count(), 1);
}

#[test]
fn a_body_with_no_shard_count_says_unknown_rather_than_the_plan() {
    let dir = scratch("unknown");
    let findings = dir.join("night-findings");
    finding(&findings, "OutputDivergence__one", "OutputDivergence", 1);
    let r = bash(
        &[
            "scripts/fuzz-night-issue-body.sh",
            findings.to_str().unwrap(),
            "correctness",
            "https://example.invalid/run/1",
            "?",
            "8",
            "unknown",
        ],
        &[],
    );
    assert_eq!(r.code, Some(0), "{}{}", r.stdout, r.stderr);
    assert!(r.stdout.contains("an **unknown** number of the 8 planned shard(s)"), "{}", r.stdout);
    assert!(!r.stdout.contains("**8 of 8**"), "{}", r.stdout);
}

/// The reader the streak scripts share, on forged verdict-job logs.
fn score(vjob: &str, log: &str) -> (String, String, String, String) {
    let dir = scratch(&format!("score-{}", log.len()));
    let logfile = dir.join("job.log");
    fs::write(&logfile, log).unwrap();
    let r = bash(
        &[
            "-c",
            &format!(
                ". scripts/lib/fuzz-night-line.sh; line=$(fuzz_night_line '{}'); fuzz_night_score '{vjob}' \"$line\"",
                logfile.display()
            ),
        ],
        &[],
    );
    assert_eq!(r.code, Some(0), "{}{}", r.stdout, r.stderr);
    let mut it = r.stdout.trim_end_matches('\n').splitn(4, '\t').map(str::to_owned);
    (it.next().unwrap(), it.next().unwrap(), it.next().unwrap(), it.next().unwrap())
}

#[test]
fn the_streak_readers_apply_the_75_percent_line_and_treat_no_record_as_unknown() {
    // A pre-#2390 line, as run 33848741367's verdict job logged it (with the
    // Actions timestamp prefix): 30.7 of 40 minutes = 76%, a streak night, but
    // the shards that did not report are unknown to it.
    let (full, green, cov, text) = score(
        "failure",
        "2026-09-04T02:11:00.0000000Z fuzz-night: shards=6/8 reporting=6 minutes_planned=40 minutes_delivered=30.7 generated=9980 throughput=324.9prog/min findings=23 correctness=23 slow=0\n",
    );
    assert_eq!((full.as_str(), green.as_str(), cov.as_str()), ("1", "0", "6/8 76%"));
    assert!(text.starts_with("FINDINGS"), "{text}");

    // The release-gate run 35524384103: 119.9 of 480 minutes = 24%. Green verdict,
    // but a quarter-executed night is not a streak night.
    let (full, green, cov, text) = score(
        "success",
        "fuzz-night: shards=1/8 reporting=2 minutes_planned=480 minutes_delivered=119.9 generated=20143 throughput=167.9prog/min findings=1 correctness=0 slow=1\n",
    );
    assert_eq!((full.as_str(), green.as_str(), cov.as_str()), ("0", "0", "2/8 24%"));
    assert!(text.starts_with("PARTIAL 24%"), "{text}");

    // The new shape at the bar, one shard reclaimed: counts, and names the shard.
    let (full, green, cov, text) = score(
        "success",
        "junk line\nfuzz-night: shards=7/8 reporting=7 missing=8 minutes_planned=40 minutes_delivered=35.0 delivered_pct=87 budget=full generated=9000 throughput=257.1prog/min findings=0 correctness=0 slow=0\r\n",
    );
    assert_eq!((full.as_str(), green.as_str(), cov.as_str()), ("1", "1", "7/8 87%"));
    assert!(text.contains("shard(s) 8 did not report: findings unknown, not zero"), "{text}");

    // No record line at all: unknown, not full and not clean.
    let (full, green, cov, text) = score("success", "the job ran but wrote nothing recognisable\n");
    assert_eq!((full.as_str(), green.as_str(), cov.as_str()), ("0", "0", "?"));
    assert!(text.starts_with("NO RECORD"), "{text}");

    // A verdict job that never concluded scores nothing either way.
    let (full, green, _, text) = score("cancelled", "fuzz-night: shards=8/8 reporting=8 minutes_planned=40 minutes_delivered=40.0 findings=0\n");
    assert_eq!((full.as_str(), green.as_str()), ("0", "0"));
    assert!(text.starts_with("NO VERDICT"), "{text}");
}

/// The workflow calls the scripts by these paths; a rename would leave the
/// verdict job posting the old inline body again.
#[test]
fn the_workflow_calls_the_renderers_by_their_committed_paths() {
    let wf = fs::read_to_string(repo_root().join(".github/workflows/fuzz-nightly.yml")).unwrap();
    assert!(wf.contains("bash scripts/fuzz-night-verdict.sh shards"), "verdict call");
    assert_eq!(wf.matches("bash scripts/fuzz-night-issue-body.sh night-findings").count(), 2, "one call per class");
    assert!(!wf.contains("head -60"), "the silent 20-finding cap is back");
    assert!(!wf.contains("across ${{ env.FUZZ_SHARDS }} shard(s)"), "the unconditional denominator is back");
    for p in ["scripts/fuzz-night-verdict.sh", "scripts/fuzz-night-issue-body.sh", "scripts/lib/fuzz-night-line.sh"] {
        assert!(repo_root().join(p).is_file(), "{p} missing");
    }
}
