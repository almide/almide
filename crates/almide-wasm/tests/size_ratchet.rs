//! Module-size RATCHET (#1585) — the corpus-wide size story (2026-08-25
//! A/B: smaller on 450/599, median ratio 0.675, aggregate 4.11 MB) was
//! a one-shot snapshot; this gate makes it non-regressable. Every
//! manifest fixture's emitted byte size is pinned in
//! golden/size-baseline.txt; a regression is red, an improvement passes
//! and is CLAIMED by regenerating:
//!
//!   ALMIDE_UPDATE_SIZES=1 cargo test --release -p almide-wasm --test size_ratchet
//!
//! The regenerated diff makes a change's size impact visible in review.
//! Two roc-style broken-measurement guards keep the gate honest: a
//! module under 100 bytes, or a total collapsing under half the
//! baseline, reads as INSTRUMENTATION FAILURE, never as a win.

use std::path::PathBuf;

/// Per-fixture regression allowance: a fixture may grow this factor
/// plus slack before the gate trips (helper dedupe shifts are real;
/// silent 2x growth is not).
const PER_FIXTURE_FACTOR: f64 = 1.25;
const PER_FIXTURE_SLACK: u64 = 512;
/// Aggregate regression allowance.
const TOTAL_FACTOR: f64 = 1.05;

fn workspace_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..").canonicalize().expect("test harness invariant")
}

fn baseline_path() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/golden/size-baseline.txt")
}

#[cfg_attr(debug_assertions, ignore = "size gate is release-only (bytes are profile-independent; time is not)")]
#[test]
fn corpus_sizes_hold_the_baseline() {
    let root = workspace_root();
    let manifest = std::fs::read_to_string(
        root.join("crates/almide-spine/tests/golden/spec-run-manifest.txt"),
    )
    .expect("run manifest");

    let mut rows = String::new();
    let mut sizes: Vec<(String, u64)> = Vec::new();
    for line in manifest.lines() {
        let rel = line.splitn(3, '\t').nth(2).expect("manifest row");
        let text = std::fs::read_to_string(almide_corpus::resolve(&root, rel)).expect("fixture readable");
        let ir = almide_spine::s5::lower_to_ir(rel, &text).expect("front (manifest fixtures all lower)");
        let bytes = almide_wasm::emit_program(&ir).expect("emit (manifest fixtures all emit)");
        let n = bytes.len() as u64;
        assert!(n >= 100, "{rel}: {n} bytes — too small to be a real module, measurement broken");
        rows.push_str(&format!("{n}\t{rel}\n"));
        sizes.push((rel.to_string(), n));
    }

    let bp = baseline_path();
    if std::env::var("ALMIDE_UPDATE_SIZES").is_ok() {
        std::fs::write(&bp, &rows).expect("write baseline");
    }
    let baseline = std::fs::read_to_string(&bp)
        .expect("golden/size-baseline.txt — generate with ALMIDE_UPDATE_SIZES=1");
    let mut base: std::collections::BTreeMap<&str, u64> = std::collections::BTreeMap::new();
    for l in baseline.lines() {
        let (n, rel) = l.split_once('\t').expect("baseline row");
        base.insert(rel, n.parse().expect("baseline size"));
    }

    let mut offences = Vec::new();
    let mut total: u64 = 0;
    let mut base_total: u64 = 0;
    for (rel, n) in &sizes {
        total += n;
        let Some(&b) = base.get(rel.as_str()) else {
            offences.push(format!("{rel}: NEW fixture ({n} B) not in the baseline — regenerate to ratify"));
            continue;
        };
        base_total += b;
        let cap = (b as f64 * PER_FIXTURE_FACTOR) as u64 + PER_FIXTURE_SLACK;
        if *n > cap {
            offences.push(format!("{rel}: {n} B > cap {cap} B (baseline {b} B)"));
        }
    }
    if base.len() != sizes.len() {
        offences.push(format!(
            "baseline has {} rows, corpus has {} — regenerate to ratify the partition",
            base.len(),
            sizes.len()
        ));
    }
    assert!(
        offences.is_empty(),
        "size ratchet ({} offence(s)) — a regression needs a fix or a deliberate \
         ALMIDE_UPDATE_SIZES=1 re-ratification:\n{}",
        offences.len(),
        offences.join("\n")
    );
    let total_cap = (base_total as f64 * TOTAL_FACTOR) as u64;
    assert!(
        total <= total_cap,
        "aggregate {total} B > cap {total_cap} B (baseline {base_total} B) — corpus-wide size regression"
    );
    assert!(
        total * 2 >= base_total,
        "aggregate {total} B is under HALF the baseline {base_total} B — a collapse this size is a \
         broken measurement (stub emission?), not a win; re-ratify deliberately if it is real"
    );
    println!("RATCHET sizes: {} fixtures, {total} B (baseline {base_total} B)", sizes.len());
}
