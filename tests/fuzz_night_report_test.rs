//! #2390: the nightly fuzz report says what its numbers are conditional on.
//!
//! Two silent under-counts lived in the report. A shard the runner reclaimed
//! uploads nothing, so its findings left the night's total and the total was
//! published unqualified — `findings=23` for a night that found 24, with the
//! qualifier (`shards=6/8`) stopping in the step log. And the issue body listed
//! at most 20 findings (`grep | head -60`, three lines each) without saying it
//! was a cap: 23 in the headline, 20 in the list, nothing to explain the gap.
//!
//! #2513 is the third: "a reclaimed runner uploads nothing" was read as "its
//! run was not recorded". The job LOG is not an upload — it outlives the kill
//! and carries the campaign's last progress line — so four shards that fuzzed
//! 100s/180s/275s/295s on 2026-09-22 were scored as zero minutes and the night
//! read 50% instead of the 85% it delivered.
//!
//! The workflow cannot run here, so these forge what it would hand the scripts:
//! a `download-artifact` shard layout with two shards absent, the job logs of
//! the shards that uploaded nothing (`$FUZZ_SHARD_LOG_DIR`, the seam that
//! stands in for `gh api .../jobs/<id>/logs`), a findings dir with more entries
//! than the cap, and `fuzz-night:` lines as the verdict job logs carry them
//! (including the pre-#2390 shape, so history stays scoreable). A gate that
//! cannot be exercised off-network is the reason the forging is worth it: every
//! test here runs with no `gh`, no token and no run.

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

/// A shard the runner reclaimed BEFORE its upload step ran: no artifact at all,
/// and the only surviving record is its job log. Forged in the shape
/// `gh api repos/<repo>/actions/jobs/<id>/logs` returns — an Actions timestamp
/// in front of every line, the fuzzer's progress lines every few seconds, and
/// the shutdown message the runner appends when it takes the job. Two progress
/// lines, so "the LAST one" is a choice the reader has to make rather than the
/// only line present.
fn killed_shard_log(dir: &Path, shard: u32, secs: u32, generated: u32, findings: u32) {
    fs::create_dir_all(dir).unwrap();
    fs::write(
        dir.join(format!("shard-{shard}.log")),
        format!(
            "2026-09-22T08:09:59.0000000Z   seed     = {seed}\n\
             2026-09-22T08:12:00.0000000Z   [ {half:>4}s] generated={halfgen} clean={halfgen} rejects=2 findings=0 walls=0 skipped=0 | 210.0 prog/min\n\
             2026-09-22T08:15:48.0222083Z   [ {secs:>4}s] generated={generated} clean={generated} rejects=20 findings={findings} walls=1 skipped=1 | 225.3 prog/min\n\
             2026-09-22T08:15:49.0000000Z ##[error]The runner has received a shutdown signal\n",
            seed = 6_787_872 + shard,
            half = secs / 2,
            halfgen = generated / 2,
        ),
    )
    .unwrap();
}

/// A log the API returned but that holds no progress line at all — the shard
/// was taken before the fuzzer printed one, or the fetch came back truncated.
fn unreadable_shard_log(dir: &Path, shard: u32) {
    fs::create_dir_all(dir).unwrap();
    fs::write(
        dir.join(format!("shard-{shard}.log")),
        "2026-09-22T08:09:00.0000000Z Current runner version: '2.331.0'\n\
         2026-09-22T08:09:01.0000000Z ##[error]The runner has received a shutdown signal\n",
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
    // An empty log directory: shards 3 and 8 yielded nothing either way, which
    // is also what keeps this test off the network wherever it runs.
    let logs = dir.join("logs");
    fs::create_dir_all(&logs).unwrap();

    let r = bash(
        &["scripts/fuzz-night-verdict.sh", shards.to_str().unwrap(), "5", "8", "23", "0"],
        &[
            ("GITHUB_OUTPUT", outputs.to_str().unwrap()),
            ("FUZZ_SHARD_LOG_DIR", logs.to_str().unwrap()),
        ],
    );
    assert_eq!(r.code, Some(0), "verdict must not fail on coverage loss\n{}{}", r.stdout, r.stderr);

    // The heading carries the denominator.
    assert!(
        r.stdout.lines().next().unwrap().contains("findings=23 of 6/8 shards"),
        "heading:\n{}",
        r.stdout
    );
    assert!(r.stdout.contains("(shards 3, 8) returned nothing AND"), "{}", r.stdout);
    assert!(r.stdout.contains("NOT in\nthis count"), "{}", r.stdout);
    assert!(r.stderr.contains("could not read shard 3's log"), "{}", r.stderr);
    assert!(r.stderr.contains("could not read shard 8's log"), "{}", r.stderr);

    // The record line names the missing shards and states delivered vs planned.
    let line = r.stderr.lines().find(|l| l.starts_with("fuzz-night: ")).expect("record line");
    assert_eq!(field(line, "shards"), "4/8");
    assert_eq!(field(line, "reporting"), "6");
    assert_eq!(field(line, "recovered"), "none");
    assert_eq!(field(line, "missing"), "3,8");
    assert_eq!(field(line, "minutes_planned"), "40");
    // 4 x 300s + 120s + 130s = 1450s = 24.2 min of 40 = 60%
    assert_eq!(field(line, "minutes_delivered"), "24.2");
    assert_eq!(field(line, "delivered_pct"), "60");
    assert_eq!(field(line, "budget"), "partial");
    assert_eq!(field(line, "findings"), "23");
    assert_eq!(field(line, "findings_recovered"), "0");
    assert!(r.stderr.contains("::warning::2 of 8 fuzz shard(s) yielded nothing at all"), "{}", r.stderr);

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
    let logs = dir.join("logs");
    fs::create_dir_all(&logs).unwrap();
    for s in 1..=8 {
        complete_shard(&shards, s, 1000, 300);
    }
    let env = [("FUZZ_SHARD_LOG_DIR", logs.to_str().unwrap())];
    let r = bash(&["scripts/fuzz-night-verdict.sh", shards.to_str().unwrap(), "5", "8", "0"], &env);
    assert_eq!(r.code, Some(0), "{}{}", r.stdout, r.stderr);
    assert!(r.stdout.starts_with("## Nightly fuzz verdict — findings=0 of 8/8 shards"), "{}", r.stdout);
    let line = r.stderr.lines().find(|l| l.starts_with("fuzz-night: ")).unwrap();
    assert_eq!(field(line, "missing"), "none");
    assert_eq!(field(line, "delivered_pct"), "100");
    assert_eq!(field(line, "budget"), "full");
    assert!(!r.stdout.contains("did not report"), "{}", r.stdout);
    // A full night has nothing to recover, and says so rather than staying silent.
    assert_eq!(field(line, "recovered"), "none");
    assert_eq!(field(line, "findings_recovered"), "0");
    assert!(!r.stdout.contains("recovered from job logs"), "{}", r.stdout);
    assert!(!r.stderr.contains("could not read"), "{}", r.stderr);

    // One shard of eight reclaimed = 87.5% delivered: a streak night under the ruling.
    let dir = scratch("seven");
    let shards = dir.join("shards");
    let logs = dir.join("logs");
    fs::create_dir_all(&logs).unwrap();
    for s in 1..=7 {
        complete_shard(&shards, s, 1000, 300);
    }
    let r = bash(
        &["scripts/fuzz-night-verdict.sh", shards.to_str().unwrap(), "5", "8", "0"],
        &[("FUZZ_SHARD_LOG_DIR", logs.to_str().unwrap())],
    );
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
    let logs = dir.join("logs");
    fs::create_dir_all(&shards).unwrap();
    // Even with every shard's log on hand, a night where NOTHING uploaded stays
    // an infra failure: the #976 vacuous-pass rule is a separate judgement the
    // workflow has already made by then, and recovery does not overturn it.
    for s in 1..=8 {
        killed_shard_log(&logs, s, 290, 1000, 0);
    }
    let r = bash(
        &["scripts/fuzz-night-verdict.sh", shards.to_str().unwrap(), "5", "8", "0"],
        &[("FUZZ_SHARD_LOG_DIR", logs.to_str().unwrap())],
    );
    assert_eq!(r.code, Some(1));
    let line = r.stderr.lines().find(|l| l.starts_with("fuzz-night: ")).unwrap();
    assert_eq!(field(line, "missing"), "1,2,3,4,5,6,7,8");
    assert_eq!(field(line, "recovered"), "none");
    assert_eq!(field(line, "budget"), "partial");
    assert!(r.stdout.contains("findings=0 of 0/8 shards"), "{}", r.stdout);
}

/// #2513: the four shards run 35702279584 lost, read back out of their logs.
#[test]
fn a_reclaimed_shard_is_recovered_from_its_job_log_and_lifts_the_night_over_the_line() {
    let dir = scratch("recovered");
    let shards = dir.join("shards");
    let logs = dir.join("logs");
    // The night of 2026-09-22: 3, 5, 7 and 8 finished; 1, 2, 4 and 6 were
    // reclaimed mid-budget and uploaded nothing. Their logs still hold
    // 295s/180s/275s/100s, which is 85% of the plan, not the 50% it was scored.
    for s in [3, 5, 7, 8] {
        complete_shard(&shards, s, 1000, 300);
    }
    killed_shard_log(&logs, 1, 295, 1108, 0);
    killed_shard_log(&logs, 2, 180, 565, 0);
    killed_shard_log(&logs, 4, 275, 1045, 0);
    killed_shard_log(&logs, 6, 100, 303, 0);
    let outputs = dir.join("outputs.txt");
    fs::write(&outputs, "").unwrap();

    let r = bash(
        &["scripts/fuzz-night-verdict.sh", shards.to_str().unwrap(), "5", "8", "0", "0"],
        &[
            ("GITHUB_OUTPUT", outputs.to_str().unwrap()),
            ("FUZZ_SHARD_LOG_DIR", logs.to_str().unwrap()),
        ],
    );
    assert_eq!(r.code, Some(0), "{}{}", r.stdout, r.stderr);

    let line = r.stderr.lines().find(|l| l.starts_with("fuzz-night: ")).expect("record line");
    assert_eq!(field(line, "shards"), "4/8", "the recovered shards did NOT finish their budget");
    assert_eq!(field(line, "reporting"), "4", "`reporting` still means `uploaded`");
    assert_eq!(field(line, "recovered"), "1@295s,2@180s,4@275s,6@100s");
    assert_eq!(field(line, "missing"), "none", "nothing was unreadable");
    // 4 x 300s + 295 + 180 + 275 + 100 = 2050s = 34.2 min of 40 = 85%.
    assert_eq!(field(line, "minutes_delivered"), "34.2");
    assert_eq!(field(line, "delivered_pct"), "85");
    assert_eq!(field(line, "budget"), "full", "the night #924 was told it missed");
    // 4 x 1000 uploaded + 1108 + 565 + 1045 + 303 recovered.
    assert_eq!(field(line, "generated"), "7021");
    assert_eq!(field(line, "findings"), "0");
    assert_eq!(field(line, "findings_recovered"), "0");

    assert!(r.stdout.contains("(+4 recovered from job logs)"), "heading:\n{}", r.stdout);
    assert!(r.stdout.contains("only up to the second it is tagged with**"), "{}", r.stdout);
    assert!(!r.stdout.contains("returned nothing AND"), "nothing is missing now:\n{}", r.stdout);
    // The per-shard table distinguishes a recovered row from a complete one.
    assert!(r.stdout.contains("| 1 | 6787873 | recovered | 1108 | 295s |"), "{}", r.stdout);
    assert!(r.stdout.contains("| 3 | 6787875 | complete | 1000 | 300.0s |"), "{}", r.stdout);

    let out = fs::read_to_string(&outputs).unwrap();
    for want in ["recovered=1@295s,2@180s,4@275s,6@100s", "findings_recovered=0", "missing=none"] {
        assert!(out.lines().any(|l| l == want), "missing `{want}` in GITHUB_OUTPUT:\n{out}");
    }
}

/// A log that cannot be read is UNKNOWN. Not zero minutes, not a clean shard,
/// and the reason is printed — an absent value read as a good value is the
/// defect one level up, and it is the one this whole file exists about.
#[test]
fn an_unreadable_log_stays_missing_a_zero_second_kill_is_still_a_reading() {
    let dir = scratch("unreadable");
    let shards = dir.join("shards");
    let logs = dir.join("logs");
    for s in [3, 5, 7, 8] {
        complete_shard(&shards, s, 1000, 300);
    }
    killed_shard_log(&logs, 1, 295, 1108, 2); // recovered, and it had found things
    unreadable_shard_log(&logs, 2); // a log with no progress line in it
    killed_shard_log(&logs, 4, 0, 0, 0); // taken before it fuzzed a second
    // shard 6: no log file at all — the fetch itself failed.

    let r = bash(
        &["scripts/fuzz-night-verdict.sh", shards.to_str().unwrap(), "5", "8", "1", "0"],
        &[("FUZZ_SHARD_LOG_DIR", logs.to_str().unwrap())],
    );
    assert_eq!(r.code, Some(0), "{}{}", r.stdout, r.stderr);
    let line = r.stderr.lines().find(|l| l.starts_with("fuzz-night: ")).expect("record line");

    // A 0s kill is a reading (that shard delivered nothing, and we know it);
    // a log we could not read is not (that shard's minutes are unknown).
    assert_eq!(field(line, "recovered"), "1@295s,4@0s");
    assert_eq!(field(line, "missing"), "2,6");
    assert!(r.stderr.contains("could not read shard 2's log"), "{}", r.stderr);
    assert!(r.stderr.contains("could not read shard 6's log"), "{}", r.stderr);
    assert!(!r.stderr.contains("could not read shard 1's log"), "{}", r.stderr);
    assert!(r.stderr.contains("UNKNOWN, not zero"), "{}", r.stderr);

    // 4 x 300 + 295 + 0 = 1495s = 24.9 min of 40 = 62%: the two unreadable
    // shards contribute NOTHING, and the percentage says so rather than
    // pretending they delivered.
    assert_eq!(field(line, "minutes_delivered"), "24.9");
    assert_eq!(field(line, "delivered_pct"), "62");
    assert_eq!(field(line, "budget"), "partial");

    // 1 uploaded finding + 2 read out of shard 1's log, and the recovered part
    // is named separately because it is only "2 up to second 295".
    assert_eq!(field(line, "findings"), "3");
    assert_eq!(field(line, "findings_recovered"), "2");
    assert_eq!(field(line, "correctness"), "1", "the class split covers the UPLOADED findings");
    assert!(r.stdout.contains("this night is NOT green"), "{}", r.stdout);
    assert!(r.stdout.contains("(shards 2, 6) returned nothing AND"), "{}", r.stdout);
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

/// #2513, on the reader side: a recovered night counts, and a count read off a
/// truncated log can never be mistaken for a complete one.
#[test]
fn a_recovered_night_counts_and_a_recovered_finding_is_never_a_green_night() {
    // 2026-09-22 (run 35702279584) as the recovery scores it: 4 uploaded, 4 read
    // back out of their job logs, 34.2 of 40 minutes — a streak night.
    let (full, green, cov, text) = score(
        "success",
        "fuzz-night: shards=4/8 reporting=4 recovered=1@295s,2@180s,4@275s,6@100s missing=none minutes_planned=40 minutes_delivered=34.2 delivered_pct=85 budget=full generated=7021 throughput=205.5prog/min findings=0 findings_recovered=0 correctness=0 slow=0\n",
    );
    assert_eq!((full.as_str(), green.as_str(), cov.as_str()), ("1", "1", "4/8 85%"));
    assert!(text.starts_with("GREEN (shard(s) 1@295s, 2@180s, 4@275s, 6@100s recovered"), "{text}");
    assert!(text.contains("counted only up to the second named"), "{text}");

    // The verdict job sees only the shards that uploaded, so it can conclude
    // success over a night whose reclaimed shard had already found something.
    // The word GREEN does not survive that, and the streak does not take it.
    let (full, green, _, text) = score(
        "success",
        "fuzz-night: shards=7/8 reporting=7 recovered=8@200s missing=none minutes_planned=40 minutes_delivered=38.3 delivered_pct=95 budget=full findings=2 findings_recovered=2\n",
    );
    assert_eq!((full.as_str(), green.as_str()), ("1", "0"), "a recovered finding is not clean");
    assert!(text.starts_with("NOT GREEN"), "{text}");
    assert!(text.contains("class unknown"), "{text}");

    // Recovered AND still-missing shards: the two are said apart, because one
    // set has numbers behind it and the other has none.
    let (full, green, cov, text) = score(
        "success",
        "fuzz-night: shards=4/8 reporting=4 recovered=1@295s,4@0s missing=2,6 minutes_planned=40 minutes_delivered=24.9 delivered_pct=62 budget=partial findings=0 findings_recovered=0\n",
    );
    assert_eq!((full.as_str(), green.as_str(), cov.as_str()), ("0", "0", "4/8 62%"));
    assert!(text.contains("shard(s) 2, 6 did not report: findings unknown, not zero"), "{text}");
    assert!(text.contains("shard(s) 1@295s, 4@0s recovered from their job logs"), "{text}");

    // A line from before the recovery existed scores exactly as it used to:
    // absent means no recovery was attempted, not zero findings recovered.
    let (full, green, _, text) = score(
        "success",
        "fuzz-night: shards=7/8 reporting=7 missing=8 minutes_planned=40 minutes_delivered=35.0 delivered_pct=87 budget=full findings=0 correctness=0 slow=0\n",
    );
    assert_eq!((full.as_str(), green.as_str()), ("1", "1"));
    assert_eq!(text, "GREEN (shard(s) 8 did not report: findings unknown, not zero)");
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

/// #2611: when exactly ONE shard uploads, `download-artifact` (pattern mode)
/// extracts that artifact FLAT into the target dir — `shards/fuzz-output.txt`,
/// no `fuzz-shard-<run>-<n>/` directory. That is the night recovery exists for
/// (7 of 8 reclaimed), and the verdict used to lose the shard number there,
/// print `missing=unknown`, and recover nothing: runs 35955335655, 35961934193
/// and 36051987508 all published 12% where their logs held 40-52%.
///
/// The number now also travels in the output's `fuzz-shard: N` header, and,
/// for outputs written before the header existed, in the derived seed
/// (`run_id * 16 + shard`).
#[test]
fn a_lone_shard_extracted_flat_still_names_itself_and_the_rest_are_recovered() {
    for (label, header) in [("header", true), ("seed", false)] {
        let dir = scratch(&format!("flat-{label}"));
        let shards = dir.join("shards");
        let logs = dir.join("logs");
        fs::create_dir_all(&shards).unwrap();
        let run_id: u64 = 424_242;
        let seed = run_id * 16 + 7;
        fs::write(
            shards.join("fuzz-output.txt"),
            format!(
                "{}  seed     = {seed}\n  [  120s] generated=300 clean=290 rejects=10 findings=0 walls=0 skipped=0 | 150.0 prog/min\n\n=== campaign summary ===\n  elapsed          = 300.0s\n  generated        = 1000\n",
                if header { "fuzz-shard: 7\n" } else { "" }
            ),
        )
        .unwrap();
        // Its findings dir is flat too: `shards/tools/xtarget-fuzz/findings/…`.
        finding(&shards.join("tools/xtarget-fuzz/findings"), "OutputDivergence__x", "OutputDivergence", 3);
        for s in [1, 2, 3, 4, 5, 6, 8] {
            killed_shard_log(&logs, s, 240, 800, 0);
        }
        let r = bash(
            &["scripts/fuzz-night-verdict.sh", shards.to_str().unwrap(), "5", "8", "0"],
            &[("FUZZ_SHARD_LOG_DIR", logs.to_str().unwrap()), ("GITHUB_RUN_ID", "424242")],
        );
        assert_eq!(r.code, Some(0), "{label}: {}{}", r.stdout, r.stderr);
        let line = r.stderr.lines().find(|l| l.starts_with("fuzz-night: ")).unwrap();
        assert_eq!(field(line, "missing"), "none", "{label}: {line}");
        assert_eq!(field(line, "recovered"), "1@240s,2@240s,3@240s,4@240s,5@240s,6@240s,8@240s", "{label}");
        // 300 s uploaded + 7 * 240 s recovered = 1980 s = 33 of 40 minutes.
        assert_eq!(field(line, "minutes_delivered"), "33.0", "{label}: {line}");
        assert_eq!(field(line, "budget"), "full", "{label}: {line}");
        assert!(r.stdout.contains("| 7 | "), "{label}: {}", r.stdout);
    }
}

/// #2611: the workflow's findings aggregate must find a finding at ANY depth,
/// because the lone-shard layout above puts it at `shards/tools/…` — the old
/// `shards/*/tools/…` glob missed it, and the v0.63.1-rc2 gate (36051987508)
/// concluded success with a correctness finding in its only artifact.
#[test]
fn the_findings_aggregate_does_not_assume_a_per_artifact_directory() {
    let wf = fs::read_to_string(repo_root().join(".github/workflows/fuzz-nightly.yml")).unwrap();
    assert!(!wf.contains("shards/*/tools/xtarget-fuzz/findings"), "the one-level glob is back");
    assert!(wf.contains("find shards -type d -path '*/tools/xtarget-fuzz/findings/*' -prune"), "aggregate");
    assert!(wf.contains("echo \"fuzz-shard: ${{ matrix.shard }}\" > fuzz-output.txt"), "shard header");
    assert!(wf.contains("| tee -a fuzz-output.txt"), "the fuzzer output must APPEND after the header");
}
