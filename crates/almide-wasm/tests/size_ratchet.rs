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
//!
//! ISOLATION (#2309): the corpus pass compiles every fixture in ONE
//! process, and `almide build` compiles one program per process. A
//! process-global cache in the front or the emitter keyed too loosely
//! (a linked helper, a self-host body, an interner-ordered table) would
//! make a row depend on the fixtures compiled before it, so the ledger
//! would pin a module nobody ships. The ratchet therefore also measures
//! every fixture ALONE — this binary re-run once per fixture, in a fresh
//! process, through the `one_fixture_measured_alone` child entry — and
//! refuses any fixture whose two forms are not byte-identical both ways.

use sha2::{Digest, Sha256};
use std::path::{Path, PathBuf};
use std::process::Command;

/// Per-fixture regression allowance: a fixture may grow this factor
/// plus slack before the gate trips (helper dedupe shifts are real;
/// silent 2x growth is not).
const PER_FIXTURE_FACTOR: f64 = 1.25;
const PER_FIXTURE_SLACK: u64 = 512;
/// Aggregate regression allowance.
const TOTAL_FACTOR: f64 = 1.05;

/// The switch that turns this binary into the isolation check's child: the
/// ONE fixture a fresh process measures. Set by the ratchet itself.
const ALONE: &str = "ALMIDE_SIZE_ALONE";
/// The child entry the ratchet spawns this binary with.
const CHILD: &str = "one_fixture_measured_alone";
/// The tag in front of the child's one result, so libtest's own output is
/// never mistaken for it.
const CHILD_TAG: &str = "size-alone\t";

fn workspace_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..").canonicalize().expect("test harness invariant")
}

fn baseline_path(name: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/golden").join(name)
}

/// One fixture measured: both forms' sizes (`None` = the structural leg
/// refuses it, a `!` row) and the fingerprint the isolation check compares
/// — each form's size AND digest, so a same-size different module counts.
struct Fixture {
    rel: String,
    sizes: Option<(u64, u64)>,
    print: String,
}

/// Measure one fixture: the emitted module and its `to_wasi` twin. A
/// refusal is a `!` row in both ledgers; a module that emits but fails the
/// transform is an Almide bug, not a row.
fn measure_one(root: &Path, rel: &str) -> Fixture {
    let text = std::fs::read_to_string(almide_corpus::resolve(root, rel)).expect("fixture readable");
    let ir = almide_spine::s5::lower_to_ir(rel, &text).expect("front (manifest fixtures all lower)");
    // `!` row: the structural leg REFUSES this fixture (CLI reroutes
    // to the incumbent — #1688's unfoldable shapes). No size to pin;
    // the alloc ledger asserts the refusal stays a refusal.
    let Ok((bytes, host_ops)) = almide_wasm::emit_program_with_ops(&ir) else {
        return Fixture { rel: rel.to_string(), sizes: None, print: "!".to_string() };
    };
    let n = bytes.len() as u64;
    assert!(n >= 100, "{rel}: {n} bytes — too small to be a real module, measurement broken");
    let host_ops: Vec<i32> = host_ops.into_iter().collect();
    let wasi = almide_wasm_run::wasi::to_wasi(&bytes, &host_ops)
        .unwrap_or_else(|e| panic!("{rel}: to_wasi failed on an emitted module — an Almide bug: {e}"));
    let w = wasi.len() as u64;
    assert!(w >= n, "{rel}: shipped {w} B < emitted {n} B — the transform only ADDS sections, measurement broken");
    let digest = |b: &[u8]| Sha256::digest(b).iter().map(|x| format!("{x:02x}")).collect::<String>();
    let print = format!("emitted {n} B {} / shipped {w} B {}", digest(&bytes), digest(&wasi));
    Fixture { rel: rel.to_string(), sizes: Some((n, w)), print }
}

fn corpus_rows(root: &Path) -> Vec<String> {
    let manifest = std::fs::read_to_string(root.join("crates/almide-spine/tests/golden/spec-run-manifest.txt"))
        .expect("run manifest");
    almide_corpus::manifest_rows(&manifest)
        .map(|line| line.splitn(3, '\t').nth(2).expect("manifest row").to_string())
        .collect()
}

/// The child entry (#2309): with [`ALONE`] set, measure that one fixture in
/// this fresh process and print its fingerprint. Without it, nothing — so a
/// plain `-- --ignored` run passes it by.
#[test]
#[ignore = "the isolation check's child entry: corpus_sizes_hold_the_baseline spawns it once per fixture"]
fn one_fixture_measured_alone() {
    let Ok(rel) = std::env::var(ALONE) else { return };
    println!("{CHILD_TAG}{}", measure_one(&workspace_root(), &rel).print);
}

/// One fixture's fingerprint from a fresh process of this binary.
fn measured_alone(exe: &Path, rel: &str) -> Result<String, String> {
    let out = Command::new(exe)
        .args([CHILD, "--exact", "--ignored", "--nocapture", "--test-threads=1"])
        .env(ALONE, rel)
        .output()
        .map_err(|e| format!("spawn failed: {e}"))?;
    let stdout = String::from_utf8_lossy(&out.stdout);
    // libtest prints `test <name> ... ` on the same line before the result.
    match stdout.lines().find_map(|l| l.split_once(CHILD_TAG).map(|(_, print)| print)) {
        Some(print) if out.status.success() => Ok(print.to_string()),
        _ => {
            let stderr = String::from_utf8_lossy(&out.stderr);
            let tail: Vec<&str> = stderr.lines().rev().take(6).collect();
            Err(format!("child exited {} without a result: {}", out.status, tail.into_iter().rev().collect::<Vec<_>>().join(" | ")))
        }
    }
}

/// Every fixture measured alone, one fresh process each, spread over the
/// available cores; the results come back in corpus order.
fn measure_each_alone(rels: &[String]) -> Vec<Result<String, String>> {
    let exe = std::env::current_exe().expect("the test binary's own path");
    let next = std::sync::atomic::AtomicUsize::new(0);
    let workers = std::thread::available_parallelism().map_or(4, |n| n.get());
    let mut results: Vec<(usize, Result<String, String>)> = std::thread::scope(|s| {
        let handles: Vec<_> = (0..workers)
            .map(|_| {
                s.spawn(|| {
                    let mut mine = Vec::new();
                    loop {
                        let i = next.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
                        let Some(rel) = rels.get(i) else { break mine };
                        mine.push((i, measured_alone(&exe, rel)));
                    }
                })
            })
            .collect();
        handles.into_iter().flat_map(|h| h.join().expect("isolation worker")).collect()
    });
    results.sort_by_key(|(i, _)| *i);
    results.into_iter().map(|(_, r)| r).collect()
}

/// Refuse any fixture whose module differs between the corpus pass and a
/// fresh process: the ledger must pin what a one-program build ships.
fn hold_isolation(corpus: &[Fixture], alone: &[Result<String, String>]) {
    let offences: Vec<String> = corpus
        .iter()
        .zip(alone)
        .filter_map(|(f, a)| match a {
            Ok(print) if *print == f.print => None,
            Ok(print) => Some(format!("{}:\n    corpus order: {}\n    alone:        {print}", f.rel, f.print)),
            Err(e) => Some(format!("{}: {e}", f.rel)),
        })
        .collect();
    assert!(
        offences.is_empty(),
        "size ratchet isolation ({} fixture(s)) — a fixture's module differs between the one-process corpus \
         pass and a fresh process, so a process-global cache in the front or the emitter carries one \
         program's state into the next (#2309); scope that state per program:\n{}",
        offences.len(),
        offences.join("\n")
    );
}

#[cfg_attr(debug_assertions, ignore = "size gate is release-only (bytes are profile-independent; time is not)")]
#[test]
fn corpus_sizes_hold_the_baseline() {
    let root = workspace_root();
    let rels = corpus_rows(&root);
    // The children run while this thread measures the corpus in order.
    let (corpus, alone) = std::thread::scope(|s| {
        let alone = s.spawn(|| measure_each_alone(&rels));
        let corpus: Vec<Fixture> = rels.iter().map(|rel| measure_one(&root, rel)).collect();
        (corpus, alone.join().expect("isolation pass"))
    });
    hold_isolation(&corpus, &alone);

    let mut emitted_rows = String::new();
    let mut shipped_rows = String::new();
    let (mut emitted, mut shipped) = (Vec::new(), Vec::new());
    for f in &corpus {
        match f.sizes {
            None => {
                emitted_rows.push_str(&format!("!\t{}\n", f.rel));
                shipped_rows.push_str(&format!("!\t{}\n", f.rel));
            }
            Some((n, w)) => {
                emitted_rows.push_str(&format!("{n}\t{}\n", f.rel));
                shipped_rows.push_str(&format!("{w}\t{}\n", f.rel));
                emitted.push((f.rel.clone(), n));
                shipped.push((f.rel.clone(), w));
            }
        }
    }
    hold_the_baseline("size-baseline.txt", "emitted", &emitted_rows, &emitted);
    hold_the_baseline("size-baseline-wasi.txt", "shipped (to_wasi)", &shipped_rows, &shipped);
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
