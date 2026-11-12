//! `ALMIDE_TIME_PHASES` partitions a build's whole timeline (#4683).
//!
//! Every lap reads one process-wide clock, so the laps of a build sum to its
//! cumulative time: a stretch of work that no lap names would show up as a
//! gap between the two. The build's later phases (the v1 render, rustc, the
//! digest, the install; the wasm leg's packaging and write) must each be
//! named. Before #4683, everything after the frontend's last lap was in no
//! lap, and that was about 128 ms of jsonround's 170 ms front end.

use std::path::{Path, PathBuf};
use std::process::Command;

fn almide() -> &'static str {
    env!("CARGO_BIN_EXE_almide")
}

fn scratch(name: &str) -> PathBuf {
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| d.as_nanos());
    let dir = std::env::temp_dir().join(format!("almide-phase-timer-{name}-{}-{nanos}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

/// (label, lap ms, cumulative ms) for every `[phase]` line.
fn laps(stderr: &str) -> Vec<(String, f64, f64)> {
    stderr
        .lines()
        .filter_map(|l| l.strip_prefix("[phase] "))
        .filter_map(|rest| {
            let (head, cum) = rest.split_once("(cumulative")?;
            let mut words: Vec<&str> = head.split_whitespace().collect();
            let lap = words.pop()?.trim_end_matches("ms").parse().ok()?;
            let cum = cum.trim().trim_end_matches(')').trim_end_matches("ms").trim().parse().ok()?;
            Some((words.join(" "), lap, cum))
        })
        .collect()
}

/// `None` when the laps add up to the last cumulative figure. Each printed
/// value is rounded to the millisecond, so the sum may drift by half a
/// millisecond per lap.
fn unaccounted(laps: &[(String, f64, f64)]) -> Option<String> {
    let (_, _, total) = laps.last()?;
    let sum: f64 = laps.iter().map(|(_, l, _)| l).sum();
    let slack = 0.5 * laps.len() as f64 + 1.0;
    ((sum - total).abs() > slack).then(|| format!("laps sum to {sum} ms, the timeline is {total} ms: {laps:?}"))
}

fn build(file: &Path, extra: &[&str], project_dir: &Path, out: &Path) -> String {
    let o = Command::new(almide())
        .arg("build")
        .arg(file)
        .args(extra)
        .arg("-o")
        .arg(out)
        .env("ALMIDE_RUN_PROJECT_DIR", project_dir)
        .env("ALMIDE_TIME_PHASES", "1")
        .output()
        .expect("spawn almide");
    let stderr = String::from_utf8_lossy(&o.stderr).into_owned();
    assert!(o.status.success(), "almide build failed:\n{stderr}");
    stderr
}

fn program(dir: &Path) -> PathBuf {
    // A constant no other test spells, so the IR-keyed cache cannot answer.
    let nanos = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map_or(0, |d| d.subsec_nanos());
    let file = dir.join("p.almd");
    std::fs::write(&file, format!("fn main() -> Unit = println(\"${{{nanos} + 1}}\")\n")).unwrap();
    file
}

fn assert_named(laps: &[(String, f64, f64)], want: &[&str]) {
    for w in want {
        assert!(laps.iter().any(|(l, ..)| l == w), "no `{w}` lap: {laps:?}");
    }
}

#[test]
fn a_native_build_names_every_phase_through_the_install() {
    let dir = scratch("native");
    let laps = laps(&build(&program(&dir), &[], &dir.join("proj"), &dir.join("out")));
    assert_named(&laps, &["cli", "parse+resolve", "check+lower", "optimize+link", "v1-render", "cache-key", "rustc", "digest", "install"]);
    assert_eq!(unaccounted(&laps), None);
}

#[test]
fn a_wasm_build_names_every_phase_through_the_write() {
    let dir = scratch("wasm");
    let laps = laps(&build(&program(&dir), &["--target", "wasm"], &dir.join("proj"), &dir.join("out.wasm")));
    assert_named(&laps, &["cli", "parse+resolve", "typecheck", "render", "package", "write"]);
    assert_eq!(unaccounted(&laps), None);
}

/// Negative control: a stretch no lap names is caught.
#[test]
fn an_unnamed_stretch_of_the_timeline_is_reported() {
    let named = "[phase] a  10ms   (cumulative 10ms)\n[phase] b  5ms   (cumulative 15ms)\n";
    assert_eq!(unaccounted(&laps(named)), None);
    let gap = "[phase] a  10ms   (cumulative 10ms)\n[phase] b  5ms   (cumulative 45ms)\n";
    assert!(unaccounted(&laps(gap)).is_some());
}
