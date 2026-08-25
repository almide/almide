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

/// Grow-only floor: raise as slices land, never lower.
const SUPPORTED_FLOOR: usize = 535;

fn workspace_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..").canonicalize().expect("test harness invariant")
}

fn normalized_hash(stdout: &str) -> String {
    let no_nul: String = stdout.chars().filter(|c| *c != '\0').collect();
    let trimmed = no_nul.trim_end_matches('\n');
    let text = if trimmed.is_empty() { String::new() } else { format!("{trimmed}\n") };
    format!("{:x}", Sha256::digest(text.as_bytes()))
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

    for line in manifest.lines() {
        let mut it = line.splitn(3, '\t');
        let want_hash = it.next().expect("test harness invariant");
        let want_exit: i32 = it.next().expect("test harness invariant").parse().expect("test harness invariant");
        let rel = it.next().expect("test harness invariant");
        total += 1;
        let text = std::fs::read_to_string(almide_corpus::resolve(&root, rel)).expect("test harness invariant");
        let ir = match almide_spine::s5::lower_to_ir(rel, &text) {
            Ok(ir) => ir,
            Err(_) => {
                front_skipped += 1;
                continue;
            }
        };
        match almide_wasm::emit_program(&ir) {
            // Abort parity IS the claim (C-153 family): a nonzero-exit
            // oracle row is claimed only when the wasm leg reproduces both
            // the stdout-before-abort hash AND the exit code. The manifest
            // hashes stdout only, so stderr stays fuzz-verified for now.
            Ok(bytes) => match run_wasm(&bytes) {
                Ok(r) => {
                    if normalized_hash(&r.stdout) == want_hash && r.exit == want_exit {
                        supported += 1;
                    } else {
                        divergent.push(format!(
                            "{rel} (exit {} want {want_exit})",
                            r.exit
                        ));
                    }
                }
                Err(e) => divergent.push(format!("{rel} (runtime: {e})")),
            },
            Err(almide_wasm::EmitError::Unsupported(reason)) => {
                *unsupported.entry(reason).or_default() += 1;
            }
        }
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
