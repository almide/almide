//! Module-size RATCHET (#1585) — the corpus-wide size story (2026-08-25
//! A/B: smaller on 450/599, median ratio 0.675, aggregate 4.11 MB) was
//! a one-shot snapshot; this gate makes it non-regressable. Every
//! manifest fixture's emitted byte size is pinned EXACTLY in
//! golden/size-baseline.txt, like the allocation ledger pins its
//! watermarks: any move, up or down, is red until the change that makes
//! it CLAIMS it by regenerating:
//!
//!   ALMIDE_UPDATE_SIZES=1 cargo test --release -p almide-wasm --test size_ratchet
//!
//! The regenerated diff makes a change's size impact visible in review.
//! Growth past the per-fixture allowance is named a REGRESSION (fix it
//! first). The pin is exact because a tolerance let rows go stale (#2309):
//! 26 shipped rows had drifted inside their caps, merged unclaimed, and
//! the next change that regenerated inherited them as its own diff.
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

/// Per-fixture regression allowance: growth past this factor plus slack
/// is named a regression rather than a move to claim (helper dedupe
/// shifts are real; silent 2x growth is not).
const PER_FIXTURE_FACTOR: f64 = 1.25;
const PER_FIXTURE_SLACK: u64 = 512;

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
            let lines: Vec<&str> = stderr.lines().collect();
            let tail = lines[lines.len().saturating_sub(6)..].join(" | ");
            Err(format!("child exited {} without a result: {tail}", out.status))
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

    let emitted: Vec<(&str, Option<u64>)> = corpus.iter().map(|f| (f.rel.as_str(), f.sizes.map(|(n, _)| n))).collect();
    let shipped: Vec<(&str, Option<u64>)> = corpus.iter().map(|f| (f.rel.as_str(), f.sizes.map(|(_, w)| w))).collect();
    // Both ledgers are judged before either fails, so one run names every moved row.
    let verdicts: Vec<String> = [
        hold_the_baseline("size-baseline.txt", "emitted", &emitted),
        hold_the_baseline("size-baseline-wasi.txt", "shipped (to_wasi)", &shipped),
    ]
    .into_iter()
    .flatten()
    .collect();
    assert!(verdicts.is_empty(), "{}", verdicts.join("\n\n"));
}

/// A ledger's rows: `rel -> Some(size)`, or `None` for a `!` row (the
/// structural leg refuses the fixture; the CLI reroutes it).
fn parse_ledger(text: &str) -> std::collections::BTreeMap<&str, Option<u64>> {
    text.lines()
        .map(|l| {
            let (n, rel) = l.split_once('\t').expect("baseline row");
            (rel, if n == "!" { None } else { Some(n.parse().expect("baseline size")) })
        })
        .collect()
}

fn render_ledger(rows: &[(&str, Option<u64>)]) -> String {
    rows.iter()
        .map(|(rel, n)| match n {
            Some(n) => format!("{n}\t{rel}\n"),
            None => format!("!\t{rel}\n"),
        })
        .collect()
}

/// One fixture's measurement against its pinned row, as the offence it
/// raises (if any). `pinned` is `None` when the ledger has no row for it.
fn row_offence(rel: &str, pinned: Option<Option<u64>>, got: Option<u64>) -> Option<String> {
    match (pinned, got) {
        (Some(None), None) => None,
        (Some(Some(b)), Some(n)) if b == n => None,
        (None, Some(n)) => Some(format!("{rel}: NEW fixture ({n} B) not in the ledger")),
        (None, None) => Some(format!("{rel}: NEW fixture (structural-refused) not in the ledger")),
        (Some(None), Some(n)) => Some(format!("{rel}: pinned `!` (structural-refused) but now emits {n} B")),
        (Some(Some(b)), None) => Some(format!("{rel}: pinned {b} B but the structural leg now refuses it")),
        (Some(Some(b)), Some(n)) => {
            let cap = (b as f64 * PER_FIXTURE_FACTOR) as u64 + PER_FIXTURE_SLACK;
            let delta = n as i64 - b as i64;
            Some(if n > cap {
                format!("{rel}: {n} B > cap {cap} B (pinned {b} B, {delta:+} B) — a REGRESSION: fix it, do not claim it")
            } else {
                format!("{rel}: {n} B, pinned {b} B ({delta:+} B)")
            })
        }
    }
}

/// One ledger's verdict: `None` when every row is as pinned (or the ledger
/// was just regenerated), else the failure to report.
fn hold_the_baseline(name: &str, form: &str, rows: &[(&str, Option<u64>)]) -> Option<String> {
    let bp = baseline_path(name);
    let total: u64 = rows.iter().filter_map(|(_, n)| *n).sum();
    if std::env::var("ALMIDE_UPDATE_SIZES").is_ok() {
        std::fs::write(&bp, render_ledger(rows)).expect("write baseline");
        println!("RATCHET sizes [{form}]: {} rows, {total} B — ledger regenerated", rows.len());
        return None;
    }
    let text = std::fs::read_to_string(&bp)
        .unwrap_or_else(|_| panic!("golden/{name} — generate with ALMIDE_UPDATE_SIZES=1"));
    let mut pinned = parse_ledger(&text);
    let pinned_total: u64 = pinned.values().flatten().sum();
    if total * 2 < pinned_total {
        return Some(format!(
            "aggregate [{form}] {total} B is under HALF the ledger's {pinned_total} B — a collapse this size is a \
             broken measurement (stub emission?), not a win; re-ratify deliberately if it is real"
        ));
    }
    let mut offences: Vec<String> =
        rows.iter().filter_map(|(rel, n)| row_offence(rel, pinned.remove(rel), *n)).collect();
    offences.extend(pinned.keys().map(|rel| format!("{rel}: in the ledger but not in the corpus")));
    if offences.is_empty() {
        println!("RATCHET sizes [{form}]: {} rows, {total} B, every row as pinned", rows.len());
        return None;
    }
    Some(format!(
        "size ratchet [{form}] ({} row(s) moved, total {total} B against {pinned_total} B pinned) — every row \
         is pinned exactly: claim a move in the change that makes it with ALMIDE_UPDATE_SIZES=1, and fix a \
         regression first:\n{}",
        offences.len(),
        offences.join("\n")
    ))
}
