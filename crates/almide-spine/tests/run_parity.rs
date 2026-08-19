//! Run-parity gate (unit 3): the ported interpreter must reproduce the
//! ORACLE's execution of every wasm_cross / wasm_fail fixture — stdout
//! (sha256, bash-normalized: NUL bytes stripped, exactly one trailing
//! newline when nonempty) and exit code. The oracle leg is `almide run
//! --target wasm` (clean a877d2138 build), legitimate as reference because
//! wasm_cross fixtures are cross-target byte-identical by the incumbent's
//! own CI definition. This is the new engine joining the incumbent's
//! 3-way-oracle bench as a measured, not trusted, participant.

use sha2::{Digest, Sha256};
use std::collections::BTreeMap;
use std::path::PathBuf;

fn workspace_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..").canonicalize().unwrap()
}

/// Mirror the generator's bash normalization: drop NUL bytes (command
/// substitution ignores them), strip trailing newlines, re-add exactly one
/// when nonempty.
fn normalized_hash(stdout: &str) -> String {
    let no_nul: String = stdout.chars().filter(|c| *c != '\0').collect();
    let trimmed = no_nul.trim_end_matches('\n');
    let text = if trimmed.is_empty() { String::new() } else { format!("{trimmed}\n") };
    format!("{:x}", Sha256::digest(text.as_bytes()))
}

#[test]
fn wasm_cross_fixtures_run_identically_on_the_interpreter() {
    let root = workspace_root();
    let golden = root.join("crates/almide-spine/tests/golden");
    let mut manifest: BTreeMap<String, (String, i32)> = BTreeMap::new();
    for l in std::fs::read_to_string(golden.join("spec-run-manifest.txt"))
        .expect("run scripts/gen-run-manifest.sh")
        .lines()
    {
        let mut it = l.splitn(3, '\t');
        let h = it.next().unwrap().to_string();
        let rc: i32 = it.next().unwrap().parse().unwrap();
        let p = it.next().unwrap().to_string();
        assert!(manifest.insert(p, (h, rc)).is_none());
    }
    assert!(manifest.len() > 550, "suspiciously small manifest");

    // The interpreter is the incumbent's PRE-codegen oracle: a fixture using
    // an intrinsic outside its bridge coverage returns Unsupported (exit -2),
    // and the incumbent's own 3-way gate SKIPS those rather than voting.
    // Same doctrine here — skipped WITH the reason printed, and the count is
    // a shrink-only ceiling so coverage can only grow.
    const MAX_UNSUPPORTED: usize = 138;
    // FuelExhausted (-3) is the interpreter's second distinguished outcome
    // ("NOT a hang or panic"); the one huge-range fixture hits it. Ceiling 1.
    const MAX_FUEL: usize = 1;
    let mut mismatches = Vec::new();
    let mut front_end_failures = Vec::new();
    let mut unsupported: BTreeMap<String, usize> = BTreeMap::new();
    let mut n_unsupported = 0usize;
    let mut n_fuel = 0usize;
    let mut n_ok = 0usize;
    for (rel, (want_hash, want_exit)) in &manifest {
        let text = std::fs::read_to_string(root.join(rel)).unwrap();
        match almide_spine::s5::run_file(rel, &text) {
            Ok(out) if out.exit == -2 => {
                let reason = out.stderr.lines().next().unwrap_or("?").to_string();
                *unsupported.entry(reason).or_default() += 1;
                n_unsupported += 1;
            }
            Ok(out) if out.exit == -3 => {
                n_fuel += 1;
            }
            Ok(out) => {
                if normalized_hash(&out.stdout) != *want_hash || out.exit != *want_exit {
                    mismatches.push(format!("{rel} (exit {} vs {want_exit})", out.exit));
                } else {
                    n_ok += 1;
                }
            }
            Err(e) => front_end_failures.push(format!("{rel}: {e}")),
        }
    }
    println!("run parity: {n_ok} identical, {n_unsupported} unsupported-skipped, {n_fuel} fuel-exhausted, {} diverge", mismatches.len());
    for (reason, n) in unsupported.iter().take(10) {
        println!("  unsupported ×{n}: {reason}");
    }
    assert!(
        front_end_failures.is_empty(),
        "{} fixtures failed before execution, first: {}",
        front_end_failures.len(), front_end_failures[0]
    );
    assert!(
        n_unsupported <= MAX_UNSUPPORTED,
        "unsupported count {n_unsupported} exceeds the shrink-only ceiling {MAX_UNSUPPORTED}"
    );
    assert!(n_fuel <= MAX_FUEL, "fuel-exhausted count {n_fuel} exceeds ceiling {MAX_FUEL}");
    assert!(
        mismatches.is_empty(),
        "{} of {} fixtures diverge from the oracle run, first: {}",
        mismatches.len(), manifest.len(), mismatches[0]
    );
}
