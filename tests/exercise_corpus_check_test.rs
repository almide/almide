//! #1322 — the benchmark exercises are the `tools/v1_gap_measure.py` corpus;
//! rot (an exercise drifting out of current language rules) silently shrinks
//! the measured surface and only surfaced during unrelated tier runs. This
//! gate makes rot loud: every exercise must pass `almide check` on every
//! commit. (Runtime behavior is covered by the wasm tier; check-cleanliness
//! is the invariant this guard owns.)

use std::process::Command;

fn almide() -> &'static str {
    env!("CARGO_BIN_EXE_almide")
}

#[test]
fn every_benchmark_exercise_passes_check() {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("research/benchmark/exercises");
    let mut failures: Vec<String> = Vec::new();
    let mut seen = 0;
    for dir in std::fs::read_dir(&root).expect("exercises dir exists").flatten() {
        if !dir.path().is_dir() {
            continue;
        }
        for f in std::fs::read_dir(dir.path()).expect("exercise dir").flatten() {
            let path = f.path();
            if path.extension().is_none_or(|e| e != "almd") {
                continue;
            }
            seen += 1;
            let out = Command::new(almide())
                .arg("check")
                .arg(&path)
                .output()
                .expect("run almide check");
            if !out.status.success() {
                failures.push(format!(
                    "{}:\n{}",
                    path.display(),
                    String::from_utf8_lossy(&out.stdout)
                ));
            }
        }
    }
    assert!(seen >= 20, "corpus walk found only {seen} exercises — wrong root?");
    assert!(
        failures.is_empty(),
        "{} exercise(s) fail `almide check` (corpus rot — fix the exercise):\n{}",
        failures.len(),
        failures.join("\n")
    );
}
