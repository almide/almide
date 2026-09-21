//! Run-parity gate (unit 3): the ported interpreter must reproduce the
//! ORACLE's execution of every wasm_cross / wasm_fail fixture — stdout
//! (sha256, bash-normalized: NUL bytes stripped, exactly one trailing
//! newline when nonempty) and exit code. The oracle is the CLI built from
//! THIS tree — `almide run --target wasm` at HEAD — legitimate as reference
//! because wasm_cross fixtures are cross-target byte-identical by the
//! incumbent's own CI definition. This is the new engine joining the
//! incumbent's 3-way-oracle bench as a measured, not trusted, participant.
//!
//! The goldens are not pinned to an old binary: `scripts/check-parity-goldens.sh`
//! (CI, almide-gates job) regenerates them from the release binary of the
//! same commit and fails on any diff, so a manifest row can only change by
//! the CLI's own output changing — never by hand, never from a stale build.

use sha2::{Digest, Sha256};
use std::collections::BTreeMap;
use std::path::PathBuf;

/// The measured, shrink-only ceilings this gate enforces (see the comment at
/// their use).
const BASELINE: &str = "proofs/run-parity-unsupported-baseline.txt";

fn workspace_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..").canonicalize().expect("test harness invariant")
}

/// Mirror the generator's bash normalization: drop NUL bytes (command
/// substitution ignores them), strip trailing newlines, re-add exactly one
/// when nonempty.
fn normalized_hash(stdout: &str) -> String {
    let no_nul: String = stdout.chars().filter(|c| *c != '\0').collect();
    let trimmed = no_nul.trim_end_matches('\n');
    let text = if trimmed.is_empty() { String::new() } else { format!("{trimmed}\n") };
    Sha256::digest(text.as_bytes()).iter().map(|b| format!("{b:02x}")).collect::<String>()
}

#[test]
fn wasm_cross_fixtures_run_identically_on_the_interpreter() {
    let root = workspace_root();
    let golden = root.join("crates/almide-spine/tests/golden");
    let mut manifest: BTreeMap<String, (String, i32)> = BTreeMap::new();
    let text = std::fs::read_to_string(golden.join("spec-run-manifest.txt")).expect("run scripts/gen-run-manifest.sh");
    // #2405: the `# oracle:` header is read, not skipped — the rows must have
    // been recorded by the CLI built from this tree (version + `dev`).
    almide_corpus::verify_oracle_header(&root, &text)
        .unwrap_or_else(|e| panic!("spec-run-manifest.txt: {e}"));
    for l in almide_corpus::manifest_rows(&text) {
        let mut it = l.splitn(3, '\t');
        let h = it.next().expect("test harness invariant").to_string();
        let rc: i32 = it.next().expect("test harness invariant").parse().expect("test harness invariant");
        let p = it.next().expect("test harness invariant").to_string();
        assert!(manifest.insert(p, (h, rc)).is_none());
    }
    assert!(manifest.len() > 550, "suspiciously small manifest");

    // The interpreter is the incumbent's PRE-codegen oracle: a fixture using
    // an intrinsic outside its bridge coverage returns Unsupported (exit -2),
    // and the incumbent's own 3-way gate SKIPS those rather than voting.
    // Same doctrine here — skipped WITH the reason printed, and the count is
    // a shrink-only ceiling so coverage can only grow. The ceilings live in
    // proofs/run-parity-unsupported-baseline.txt (measured, lowered in their
    // own commit — a ratchet artifact, not a constant next to the code it
    // judges); a fixture that newly lands in the manifest and is unsupported
    // must be carried by a bridge extension, not by raising the number.
    let max_unsupported = almide_corpus::ratchet_ceiling(&root, BASELINE, "unsupported");
    // FuelExhausted (-3) is the interpreter's second distinguished outcome
    // ("NOT a hang or panic"): the one huge-range fixture hits it, and
    // effect_tco_err_rewrap pins a TCO the interpreter does not perform on
    // the err-rewrap path, so it spins to fuel exhaustion there.
    let max_fuel = almide_corpus::ratchet_ceiling(&root, BASELINE, "fuel_exhausted");

    // ALMIDE_CORPUS_SHARD (#2381). The per-fixture verdicts (a mismatch, a
    // front-end failure) are judged in each slice; the two ceilings are
    // counts over the WHOLE manifest, and a slice's count judged against the
    // whole ceiling would pass N times over N fractions — so a slice only
    // records its counts, and `merge/N` sums the N partials and judges the
    // sums against the ceilings here, in the same assertions.
    const GATE: &str = "run_parity";
    let shard = almide_corpus::corpus_shard();
    if let Some(almide_corpus::CorpusShard::Merge { n }) = shard {
        let expected: Vec<String> = manifest.keys().cloned().collect();
        almide_corpus::assert_partials_cover(n, GATE, &expected);
        let mut sums: BTreeMap<String, usize> = BTreeMap::new();
        for lines in almide_corpus::read_partials(n, GATE, "counts") {
            for l in lines {
                let (k, v) = l.split_once('\t').expect("counts partial row: name<TAB>value");
                *sums.entry(k.to_string()).or_default() += v.trim().parse::<usize>().expect("counts partial value");
            }
        }
        let count = |k: &str| *sums.get(k).unwrap_or_else(|| panic!("counts partial has no `{k}` row"));
        let (n_ok, n_unsupported, n_fuel, rows) =
            (count("identical"), count("unsupported"), count("fuel_exhausted"), count("rows"));
        assert_eq!(rows, manifest.len(), "the {n} shards walked {rows} rows, the manifest has {}", manifest.len());
        println!("run parity (merge of {n} shards): {n_ok} identical, {n_unsupported} unsupported-skipped, {n_fuel} fuel-exhausted over {rows} rows");
        assert!(
            n_unsupported <= max_unsupported,
            "unsupported count {n_unsupported} exceeds the shrink-only ceiling {max_unsupported} ({BASELINE})"
        );
        assert!(n_fuel <= max_fuel, "fuel-exhausted count {n_fuel} exceeds ceiling {max_fuel} ({BASELINE})");
        return;
    }
    let mut rows: Vec<(&String, &(String, i32))> = manifest.iter().collect();
    if let Some(s) = shard {
        rows = s.apply(rows);
        let walked: Vec<String> = rows.iter().map(|(rel, _)| (*rel).clone()).collect();
        almide_corpus::write_partial(s, GATE, "fixtures", &walked);
    }

    let mut mismatches = Vec::new();
    let mut front_end_failures = Vec::new();
    let mut unsupported: BTreeMap<String, usize> = BTreeMap::new();
    let mut n_unsupported = 0usize;
    let mut n_fuel = 0usize;
    let mut n_ok = 0usize;
    for (rel, (want_hash, want_exit)) in rows {
        let text = std::fs::read_to_string(almide_corpus::resolve(&root, rel)).expect("test harness invariant");
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
                if normalized_hash(&out.stdout) == *want_hash && out.exit == *want_exit {
                    n_ok += 1;
                } else {
                    mismatches.push(format!("{rel} (exit {} vs {want_exit})", out.exit));
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
    if let Some(s) = shard {
        // A slice: record the counts for the merge, judge the per-fixture
        // half, and leave the ceilings to `merge/N`.
        let counts = [
            format!("identical\t{n_ok}"),
            format!("unsupported\t{n_unsupported}"),
            format!("fuel_exhausted\t{n_fuel}"),
            format!("rows\t{}", n_ok + n_unsupported + n_fuel + mismatches.len()),
        ];
        almide_corpus::write_partial(s, GATE, "counts", &counts);
        println!("  ceilings ({BASELINE}) are judged by the merge over every shard, not here");
        assert!(
            mismatches.is_empty(),
            "{} fixtures of this shard diverge from the oracle run, first: {}",
            mismatches.len(), mismatches[0]
        );
        return;
    }
    assert!(
        n_unsupported <= max_unsupported,
        "unsupported count {n_unsupported} exceeds the shrink-only ceiling {max_unsupported} ({BASELINE})"
    );
    assert!(n_fuel <= max_fuel, "fuel-exhausted count {n_fuel} exceeds ceiling {max_fuel} ({BASELINE})");
    assert!(
        mismatches.is_empty(),
        "{} of {} fixtures diverge from the oracle run, first: {}",
        mismatches.len(), manifest.len(), mismatches[0]
    );
}
