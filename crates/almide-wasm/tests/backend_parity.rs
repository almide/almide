//! Unit 6 burn-up gate: sweep the full run-parity corpus (the 590-fixture
//! manifest the interpreter is judged by), attempt emission on every
//! fixture, and hold two lines at once:
//!   - every fixture the backend CLAIMS to support must execute
//!     byte-identically to the oracle manifest (divergence is failure,
//!     never a skip);
//!   - everything else lands in a REASON histogram, and the supported count
//!     is a grow-only floor — the same burn-up mechanic that took the
//!     interpreter from 138 skips to 121.

mod harness;
use harness::run_wasm;
use sha2::{Digest, Sha256};
use std::collections::BTreeMap;
use std::path::PathBuf;
use std::sync::mpsc;
use std::time::{Duration, Instant};

/// Grow-only floor: raise as slices land, never lower.
const SUPPORTED_FLOOR: usize = 599;

fn workspace_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..").canonicalize().expect("test harness invariant")
}

/// The manifest's normalization, on BYTES (C-399: stdout is compared exactly,
/// invalid UTF-8 included): drop NUL bytes and trailing newlines the way
/// gen-run-manifest.sh's `$(...)` capture does, then end in one `\n` unless empty.
fn normalized_hash(stdout: &[u8]) -> String {
    let no_nul: Vec<u8> = stdout.iter().copied().filter(|b| *b != 0).collect();
    let end = no_nul.iter().rposition(|b| *b != b'\n').map_or(0, |i| i + 1);
    let mut text = no_nul[..end].to_vec();
    if !text.is_empty() {
        text.push(b'\n');
    }
    Sha256::digest(&text).iter().map(|b| format!("{b:02x}")).collect::<String>()
}

/// What one corpus program came to, owned so it can cross the worker thread.
enum Outcome {
    FrontSkipped,
    Ran { hash: String, exit: i32 },
    RunError(String),
    Unsupported(String),
    OwnershipLowering(String),
}

/// The most wall time one program may take through the front end, the
/// emitter and the run (#4210). The run itself is bounded by the harness's
/// 30 s epoch watchdog, but the front end, the emitter and a host call
/// blocked outside guest code are not: one of them ran a CI shard past its
/// 70-minute timeout with no name attached. Every program takes well under
/// a second; 180 s leaves room for a contended runner.
const PROGRAM_LIMIT: Duration = Duration::from_secs(180);

/// Run one program on a worker thread and wait at most `limit`: a program
/// that does not finish fails the burn-up BY NAME instead of hanging it.
/// The worker is left behind; the panic ends the test.
fn within<T: Send + 'static>(limit: Duration, rel: &str, work: impl FnOnce() -> T + Send + 'static) -> T {
    let (tx, rx) = mpsc::channel();
    std::thread::Builder::new()
        .name(format!("burn-up {rel}"))
        .spawn(move || {
            let _ = tx.send(work());
        })
        .expect("spawn the burn-up worker");
    match rx.recv_timeout(limit) {
        Ok(outcome) => outcome,
        Err(mpsc::RecvTimeoutError::Timeout) => {
            panic!("{rel} did not finish within {} s (front end + emit + run): a hang, or a drastic slowdown (#4210)", limit.as_secs_f64())
        }
        // The worker panicked; its message is already on stderr.
        Err(mpsc::RecvTimeoutError::Disconnected) => panic!("{rel}: the burn-up worker panicked"),
    }
}

/// The limit itself: a program that outlives it fails by name, promptly.
#[test]
#[should_panic(expected = "spec/slow_test.almd did not finish within 0.2 s")]
fn a_program_past_the_limit_fails_the_burn_up_by_name() {
    within(Duration::from_millis(200), "spec/slow_test.almd", || std::thread::sleep(Duration::from_secs(30)));
}

fn one_program(rel: &str, text: &str) -> Outcome {
    let Ok(ir) = almide_spine::s5::lower_to_ir(rel, text) else { return Outcome::FrontSkipped };
    match almide_wasm::emit_program(&ir) {
        Ok(bytes) => match run_wasm(&bytes) {
            Ok(r) => Outcome::Ran { hash: normalized_hash(&r.stdout_bytes), exit: r.exit },
            Err(e) => Outcome::RunError(e.to_string()),
        },
        Err(almide_wasm::EmitError::Unsupported(reason)) => Outcome::Unsupported(reason),
        Err(almide_wasm::EmitError::OwnershipLowering(d)) => Outcome::OwnershipLowering(d.to_string()),
    }
}

#[test]
fn corpus_burn_up() {
    let root = workspace_root();
    let manifest = std::fs::read_to_string(
        root.join("crates/almide-spine/tests/golden/spec-run-manifest.txt"),
    )
    .expect("run manifest (scripts/gen-run-manifest.sh)");

    let mut supported = 0usize;
    let mut divergent: Vec<String> = Vec::new();
    let mut unsupported: BTreeMap<String, usize> = BTreeMap::new();
    let mut front_skipped = 0usize;
    let mut total = 0usize;

    let limit = PROGRAM_LIMIT;
    let mut timings: Vec<(f64, String)> = Vec::new();
    for line in almide_corpus::manifest_rows(&manifest) {
        let mut it = line.splitn(3, '\t');
        let want_hash = it.next().expect("test harness invariant");
        let want_exit: i32 = it.next().expect("test harness invariant").parse().expect("test harness invariant");
        let rel = it.next().expect("test harness invariant").to_string();
        total += 1;
        let text = std::fs::read_to_string(almide_corpus::resolve(&root, &rel)).expect("test harness invariant");
        let started = Instant::now();
        let name = rel.clone();
        let outcome = within(limit, &rel, move || one_program(&name, &text));
        timings.push((started.elapsed().as_secs_f64(), rel.clone()));
        match outcome {
            Outcome::FrontSkipped => front_skipped += 1,
            // Abort parity IS the claim (C-153 family): a nonzero-exit
            // oracle row is claimed only when the wasm leg reproduces both
            // the stdout-before-abort hash AND the exit code. The manifest
            // hashes stdout only, so stderr stays fuzz-verified for now.
            Outcome::Ran { hash, exit } => {
                if hash == want_hash && exit == want_exit {
                    supported += 1;
                } else {
                    divergent.push(format!("{rel} (exit {exit} want {want_exit})"));
                }
            }
            Outcome::RunError(e) => divergent.push(format!("{rel} (runtime: {e})")),
            Outcome::Unsupported(reason) => *unsupported.entry(reason).or_default() += 1,
            // E083 is a compiler defect, not a wall: the burn-up fails on it.
            Outcome::OwnershipLowering(d) => panic!("{d}"),
        }
    }

    // The slowest programs by name (#4210): a shard that drifts toward its
    // timeout shows here which program it is spending the time on.
    timings.sort_by(|a, b| b.0.total_cmp(&a.0));
    println!("unit6 burn-up: slowest programs (front end + emit + run):");
    for (secs, rel) in timings.iter().take(10) {
        println!("  {secs:7.2}s  {rel}");
    }
    println!("unit6 burn-up: {supported} supported / {total} fixtures; {front_skipped} front-skipped; top unsupported:");
    let mut hist: Vec<_> = unsupported.iter().collect();
    hist.sort_by(|a, b| b.1.cmp(a.1));
    for (reason, n) in hist.iter().take(15) {
        println!("  ×{n}: {reason}");
    }
    assert!(
        divergent.is_empty(),
        "{} fixtures the backend claims to support DIVERGE from the oracle: {:?}",
        divergent.len(),
        &divergent[..divergent.len().min(5)]
    );
    assert!(
        supported >= SUPPORTED_FLOOR,
        "supported count {supported} fell below the grow-only floor {SUPPORTED_FLOOR}"
    );
}
