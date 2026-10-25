//! WASM dispatch coverage gate — runs the stdlib spec tests against the
//! WASM backend (`almide test --target wasm`) and fails when any file
//! panics with an `[ICE] emit_wasm:` during WASM codegen.
//!
//! Motivation: the WASM dispatcher is a per-module `emit_<m>_call` match
//! (plus the `dispatch_runtime_fallback` path). When mono rename, a new
//! stdlib fn, or a bundled-body landing under-specifies its WASM route,
//! the gap only surfaces as a runtime panic on the affected test file.
//! This gate makes the gap a CI failure rather than a silent skip.
//!
//! Mechanism: drive the existing `almide test --target wasm` runner over
//! `spec/stdlib/`, parse its `X passed, Y failed, Z skipped` summary,
//! and fail on any `failed`. `skipped` is allowed up to a known baseline
//! (the matrix `rms_norm_rows` / `attention_weights` ICE set that pre-
//! dates this test); dropping below the baseline is fine, rising above
//! it is not.
//!
//! Running: `cargo test --test wasm_dispatch_coverage_test`.

use std::path::PathBuf;
use std::process::Command;

fn almide() -> &'static str { env!("CARGO_BIN_EXE_almide") }

fn repo_root() -> PathBuf { PathBuf::from(env!("CARGO_MANIFEST_DIR")) }

/// Pure `// wasm:skip` directives — files that legitimately cannot run
/// on WASM (native-only `process` fns; `testing.assert_throws`, which
/// relied on panic catching, left at dialect epoch 14). The 3 matrix ops (`rms_norm_rows`,
/// `attention_weights`, `swiglu_gate`) are now implemented inline;
/// bumping this upward requires a roadmap entry.
const SKIP_BASELINE: usize = 35;

#[test]
fn stdlib_spec_compiles_and_runs_on_wasm() {
    if Command::new("wasmtime").arg("--version").output().map_or(true, |o| !o.status.success()) {
        eprintln!("skipping: wasmtime not on PATH");
        return;
    }
    let root = repo_root().join("spec/stdlib");
    // A full run tests the directory in one invocation, as before. Under the
    // CI slice (ALMIDE_CI_SLICE) each selected file runs alone and the
    // per-file summaries are summed; the SKIP_BASELINE ceiling counts the
    // whole directory, so it is judged on a full run only.
    let targets: Vec<PathBuf> = if almide_corpus::full_run() {
        vec![root.clone()]
    } else {
        let mut files: Vec<PathBuf> = std::fs::read_dir(&root)
            .expect("spec/stdlib")
            .flatten()
            .map(|e| e.path())
            .filter(|p| p.extension().is_some_and(|x| x == "almd"))
            .filter(|p| std::fs::read_to_string(p).is_ok_and(|t| t.contains("\ntest ") || t.starts_with("test ")))
            .collect();
        files.sort();
        almide_corpus::select(files, "wasm_dispatch_coverage", |p| {
            format!("spec/stdlib/{}", p.file_name().unwrap_or_default().to_string_lossy())
        })
    };
    let (mut stdout, mut stderr, mut summaries) = (String::new(), String::new(), Vec::new());
    for t in &targets {
        let out = Command::new(almide())
            .args(["test", t.to_str().unwrap(), "--target", "wasm"])
            .output()
            .expect("almide test --target wasm");
        let (o, e) = (String::from_utf8_lossy(&out.stdout).to_string(), String::from_utf8_lossy(&out.stderr).to_string());
        summaries.push(
            format!("{o}{e}").lines().find(|l| l.contains(" passed") && l.contains(" failed")).unwrap_or("<no summary>").to_string(),
        );
        stdout.push_str(&o);
        stderr.push_str(&e);
    }
    let combined = format!("{}{}", stdout, stderr);

    let ice_hits: Vec<&str> = combined.lines()
        .filter(|l| l.contains("[ICE] emit_wasm"))
        .collect();

    let summary = summaries.join(" | ");

    let parse_count = |needle: &str| -> usize {
        summaries.iter().map(|summary| summary.split_whitespace()
            .collect::<Vec<_>>()
            .windows(2)
            .find(|w| w[1].starts_with(needle))
            .and_then(|w| w[0].parse::<usize>().ok())
            .unwrap_or(0)).sum()
    };
    let passed = parse_count("passed");
    let failed = parse_count("failed");
    let skipped = parse_count("skipped");

    eprintln!("── WASM dispatch coverage ───────────────────────────");
    eprintln!("  Summary : {}", summary);
    eprintln!("  ICE hits : {}", ice_hits.len());
    for hit in &ice_hits { eprintln!("    {}", hit); }
    eprintln!("─────────────────────────────────────────────────────");

    assert_eq!(failed, 0,
        "stdlib spec has {} WASM failures:\n{}\n{}", failed, stdout, stderr);
    assert!(!almide_corpus::full_run() || skipped <= SKIP_BASELINE,
        "WASM skip count regressed: {} > {} (baseline). New dispatch gap?\nsummary: {}\nstderr:\n{}",
        skipped, SKIP_BASELINE, summary, stderr);
    assert!(passed > 0, "no WASM tests ran — orchestrator failed?\n{}\n{}", stdout, stderr);
}
