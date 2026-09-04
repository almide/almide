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
//!
//! Two ledgers, same rows, same caps (#1859): `size-baseline.txt` pins
//! `emit_program`'s bytes — the embedded-host module — and
//! `size-baseline-wasi.txt` pins the SHIPPED form, the same module after
//! `to_wasi` (the stock-runtime p1 command `almide build --target wasm`
//! writes). The transform adds the WASI imports and shims, and it is the
//! layer #1841 regressed by 1,033 B on every env-free module while the
//! first ledger did not move by a byte: only the shipped bytes see a
//! transform-level regression, and only a corpus-wide ledger sees it on
//! every program rather than on Hello, world alone.

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

fn baseline_path(name: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/golden").join(name)
}

/// One corpus pass measures both forms per fixture: the emitted module
/// and its `to_wasi` twin. Refusals are `!` rows in both ledgers; a
/// module that emits but fails the transform is an Almide bug, not a row.
struct Measured {
    emitted: Vec<(String, u64)>,
    shipped: Vec<(String, u64)>,
    emitted_rows: String,
    shipped_rows: String,
}

fn measure_corpus() -> Measured {
    let root = workspace_root();
    let manifest = std::fs::read_to_string(
        root.join("crates/almide-spine/tests/golden/spec-run-manifest.txt"),
    )
    .expect("run manifest");

    let mut m = Measured {
        emitted: Vec::new(),
        shipped: Vec::new(),
        emitted_rows: String::new(),
        shipped_rows: String::new(),
    };
    for line in manifest.lines() {
        let rel = line.splitn(3, '\t').nth(2).expect("manifest row");
        let text = std::fs::read_to_string(almide_corpus::resolve(&root, rel)).expect("fixture readable");
        let ir = almide_spine::s5::lower_to_ir(rel, &text).expect("front (manifest fixtures all lower)");
        // `!` row: the structural leg REFUSES this fixture (CLI reroutes
        // to the incumbent — #1688's unfoldable shapes). No size to pin;
        // the alloc ledger asserts the refusal stays a refusal.
        let Ok((bytes, host_ops)) = almide_wasm::emit_program_with_ops(&ir) else {
            m.emitted_rows.push_str(&format!("!\t{rel}\n"));
            m.shipped_rows.push_str(&format!("!\t{rel}\n"));
            continue;
        };
        let n = bytes.len() as u64;
        assert!(n >= 100, "{rel}: {n} bytes — too small to be a real module, measurement broken");
        m.emitted_rows.push_str(&format!("{n}\t{rel}\n"));
        m.emitted.push((rel.to_string(), n));

        let host_ops: Vec<i32> = host_ops.into_iter().collect();
        let wasi = almide_wasm_run::wasi::to_wasi(&bytes, &host_ops)
            .unwrap_or_else(|e| panic!("{rel}: to_wasi failed on an emitted module — an Almide bug: {e}"));
        let w = wasi.len() as u64;
        assert!(w >= n, "{rel}: shipped {w} B < emitted {n} B — the transform only ADDS sections, measurement broken");
        m.shipped_rows.push_str(&format!("{w}\t{rel}\n"));
        m.shipped.push((rel.to_string(), w));
    }
    m
}

#[cfg_attr(debug_assertions, ignore = "size gate is release-only (bytes are profile-independent; time is not)")]
#[test]
fn corpus_sizes_hold_the_baseline() {
    let m = measure_corpus();
    hold_the_baseline("size-baseline.txt", "emitted", &m.emitted_rows, &m.emitted);
    hold_the_baseline("size-baseline-wasi.txt", "shipped (to_wasi)", &m.shipped_rows, &m.shipped);
}

fn hold_the_baseline(name: &str, form: &str, rows: &str, sizes: &[(String, u64)]) {
    let bp = baseline_path(name);
    if std::env::var("ALMIDE_UPDATE_SIZES").is_ok() {
        std::fs::write(&bp, rows).expect("write baseline");
    }
    let baseline = std::fs::read_to_string(&bp)
        .unwrap_or_else(|_| panic!("golden/{name} — generate with ALMIDE_UPDATE_SIZES=1"));
    let mut base: std::collections::BTreeMap<&str, u64> = std::collections::BTreeMap::new();
    for l in baseline.lines() {
        let (n, rel) = l.split_once('\t').expect("baseline row");
        if n == "!" {
            continue; // structural-refused row — nothing to compare
        }
        base.insert(rel, n.parse().expect("baseline size"));
    }

    let mut offences = Vec::new();
    let mut total: u64 = 0;
    let mut base_total: u64 = 0;
    for (rel, n) in sizes {
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
        "size ratchet [{form}] ({} offence(s)) — a regression needs a fix or a deliberate \
         ALMIDE_UPDATE_SIZES=1 re-ratification:\n{}",
        offences.len(),
        offences.join("\n")
    );
    let total_cap = (base_total as f64 * TOTAL_FACTOR) as u64;
    assert!(
        total <= total_cap,
        "aggregate [{form}] {total} B > cap {total_cap} B (baseline {base_total} B) — corpus-wide size regression"
    );
    assert!(
        total * 2 >= base_total,
        "aggregate [{form}] {total} B is under HALF the baseline {base_total} B — a collapse this size is a \
         broken measurement (stub emission?), not a win; re-ratify deliberately if it is real"
    );
    println!("RATCHET sizes [{form}]: {} fixtures, {total} B (baseline {base_total} B)", sizes.len());
}
