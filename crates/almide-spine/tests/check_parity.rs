//! Diagnostics-parity gate (unit 4): the ported checker behind
//! `check_file_json` must reproduce, byte for byte, the stdout of the ORACLE
//! `almide check <file> --json` over the spec corpus. The oracle is the CLI
//! built from THIS tree: `scripts/check-parity-goldens.sh` (CI) regenerates
//! the manifest from the same commit's release binary and fails on any
//! diff, so a golden row can only change by the CLI's output changing.
//!
//! Coverage discipline (no silent gaps): every spec/**/*.almd is in exactly
//! one of the oracle manifest / oracle exclusions. Within the manifest, files
//! whose imports are not all stdlib modules fall outside `check_file_json`'s
//! purity contract; they are counted and printed, never silently dropped,
//! and everything else must hash-match.

use sha2::{Digest, Sha256};
use std::collections::BTreeMap;
use std::path::PathBuf;

fn workspace_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..").canonicalize().expect("test harness invariant")
}

fn sha(s: &str) -> String {
    Sha256::digest(s.as_bytes()).iter().map(|b| format!("{b:02x}")).collect::<String>()
}

/// The manifest rows this run judges (crates/almide-corpus/src/ci_slice.rs):
/// every row on a full run, the CI slice plus the change's own fixtures under
/// `ALMIDE_CI_SLICE`. Each row is a per-file hash comparison, sound on any
/// subset; the whole-corpus checks (stale rows, the `> 900` floors) are full-run
/// only.
fn judged(manifest: BTreeMap<String, String>, gate: &str) -> BTreeMap<String, String> {
    let keep: std::collections::BTreeSet<String> =
        almide_corpus::select(manifest.keys().cloned().collect(), gate, |k| k.clone()).into_iter().collect();
    manifest.into_iter().filter(|(k, _)| keep.contains(k)).collect()
}

/// The floor a full run holds the compared count to; a slice must still
/// compare something.
fn assert_compared(compared: usize, what: &str) {
    if almide_corpus::full_run() {
        assert!(compared > 900, "{what}: suspiciously few files compared ({compared}) — purity filter too broad?");
    } else {
        assert!(compared > 0, "{what}: the CI slice compared no file");
    }
}

type Check = for<'db> fn(&'db dyn salsa::Database, almide_spine::SourceFile) -> &'db almide_spine::s3::CheckOutput;

/// One file's check under the per-fixture limit (#3768), so a checker that
/// never returns fails naming the file. The db moves to the file's thread
/// and back, so the sweep keeps its one cache. The checker has no step meter
/// to poll the cut's stop with (#3809), so a cut check is reported as left
/// running; the interpreter sweeps, where the corpus's time goes, do stop.
fn check_limited(db: &mut Option<almide_spine::SpineDb>, rel: &str, text: String, check: Check) -> almide_spine::s3::CheckOutput {
    let held = db.take().expect("the db came back from the previous file");
    let path = rel.to_string();
    let (held, out) = almide_corpus::run_limited(rel, almide_corpus::FIXTURE_LIMIT, move |_stop| {
        let file = almide_spine::SourceFile::new(&held, path, text);
        let out = check(&held, file).clone();
        (held, out)
    });
    *db = Some(held);
    out
}

#[test]
fn spec_corpus_check_matches_oracle_hashes() {
    let root = workspace_root();
    let golden = root.join("crates/almide-spine/tests/golden");
    // manifest rows: sha256 \t exit \t path  → path -> sha256
    let mut manifest: BTreeMap<String, String> = BTreeMap::new();
    let text = std::fs::read_to_string(golden.join("spec-check-manifest.txt")).expect("run scripts/gen-check-manifest.sh");
    // #2405: the `# oracle:` header is read, not skipped — the rows must have
    // been recorded by the CLI built from this tree (version + `dev`).
    almide_corpus::verify_oracle_header(&root, &text)
        .unwrap_or_else(|e| panic!("spec-check-manifest.txt: {e}"));
    for l in almide_corpus::manifest_rows(&text) {
        let mut it = l.splitn(3, '\t');
        let h = it.next().expect("test harness invariant").to_string();
        let _rc = it.next().expect("test harness invariant");
        let p = it.next().expect("test harness invariant").to_string();
        assert!(manifest.insert(p, h).is_none(), "duplicate path in manifest");
    }
    let mut exclusions: BTreeMap<String, String> = BTreeMap::new();
    let excl = std::fs::read_to_string(golden.join("spec-check-exclusions.txt")).expect("test harness invariant");
    for l in almide_corpus::manifest_rows(&excl) {
        let (p, r) = l.split_once('\t').expect("test harness invariant");
        exclusions.insert(p.to_string(), r.to_string());
    }

    // Both roots of the corpus partition (see almide-corpus): one walker.
    // Under the CI slice the partition is asked of the sliced files only, and
    // staleness (a row with no file) waits for the full run.
    let files = almide_corpus::walk_spec(&root);
    let slice = almide_corpus::ci_slice();
    for (rel, _) in &files {
        if slice.is_some_and(|s| !s.keeps(rel)) {
            continue;
        }
        let in_m = manifest.contains_key(rel);
        let in_e = exclusions.contains_key(rel);
        assert!(in_m ^ in_e, "{rel}: must be in exactly one of manifest/exclusions");
    }
    if slice.is_none() {
        assert_eq!(files.len(), manifest.len() + exclusions.len(), "stale golden entries");
    }
    let manifest = judged(manifest, "check_parity");

    let mut db = Some(almide_spine::SpineDb::default());
    let mut purity_skipped = Vec::new();
    let mut mismatches = Vec::new();
    let mut compared = 0usize;
    for (rel, want) in &manifest {
        let text = std::fs::read_to_string(almide_corpus::resolve(&root, rel)).expect("test harness invariant");
        // Purity contract: resolve must not read the FS inside a query.
        let tokens = almide::lexer::Lexer::tokenize(&text);
        let mut parser = almide::parser::Parser::new(tokens).with_file(rel);
        match parser.parse() {
            Ok(prog) if !almide_spine::s3::stdlib_only(&prog) => {
                purity_skipped.push(rel.clone());
                continue;
            }
            _ => {}
        }
        let out = check_limited(&mut db, rel, text, almide_spine::s3::check_file_json);
        assert!(out.fatal.is_none(), "{rel}: greenfield hit a fatal path the oracle did not");
        assert!(out.module_diags.is_empty(), "{rel}: module diagnostics on a stdlib-only file");
        let stdout = if out.diags.is_empty() {
            String::new()
        } else {
            format!("{}\n", out.diags.join("\n"))
        };
        if sha(&stdout) != *want {
            mismatches.push(rel.clone());
        }
        compared += 1;
    }
    println!(
        "check parity: {compared} compared, {} purity-skipped (non-stdlib imports): {:?}…",
        purity_skipped.len(),
        &purity_skipped[..purity_skipped.len().min(3)]
    );
    assert!(
        mismatches.is_empty(),
        "{} of {compared} files diverge from oracle check output, first: {} (debug: ORACLE almide check {} --json)",
        mismatches.len(), mismatches[0], mismatches[0]
    );
    assert_compared(compared, "check parity");
}

/// The stage-2 variant (no #862 stdlib loop) must ALSO match the oracle
/// byte-for-byte — this is the adjudication that the skipped loop has no
/// observable effect on stdlib-only entries.
#[test]
fn stage2_variant_matches_oracle_hashes() {
    let root = workspace_root();
    let golden = root.join("crates/almide-spine/tests/golden");
    let mut manifest: BTreeMap<String, String> = BTreeMap::new();
    let text = std::fs::read_to_string(golden.join("spec-check-manifest.txt")).expect("test harness invariant");
    for l in almide_corpus::manifest_rows(&text) {
        let mut it = l.splitn(3, '\t');
        let h = it.next().expect("test harness invariant").to_string();
        let _rc = it.next().expect("test harness invariant");
        manifest.insert(it.next().expect("test harness invariant").to_string(), h);
    }
    let manifest = judged(manifest, "check_parity");
    let mut db = Some(almide_spine::SpineDb::default());
    let mut compared = 0usize;
    let mut mismatches = Vec::new();
    for (rel, want) in &manifest {
        let text = std::fs::read_to_string(almide_corpus::resolve(&root, rel)).expect("test harness invariant");
        let tokens = almide::lexer::Lexer::tokenize(&text);
        let mut parser = almide::parser::Parser::new(tokens).with_file(rel);
        match parser.parse() {
            Ok(prog) if !almide_spine::s3::stdlib_only(&prog) => continue,
            _ => {}
        }
        let out = check_limited(&mut db, rel, text, almide_spine::s3::check_file_json_v2);
        if out.fatal.is_some() { continue; }
        let stdout = if out.diags.is_empty() { String::new() } else { format!("{}\n", out.diags.join("\n")) };
        if sha(&stdout) != *want { mismatches.push(rel.clone()); }
        compared += 1;
    }
    println!("stage2 parity: {compared} compared");
    assert!(mismatches.is_empty(), "{} of {compared} diverge without the #862 loop, first: {}", mismatches.len(), mismatches[0]);
    assert_compared(compared, "stage2 parity");
}

/// v3 (env template + split canonicalize) must also match byte-for-byte.
#[test]
fn stage2_v3_template_matches_oracle_hashes() {
    let root = workspace_root();
    let golden = root.join("crates/almide-spine/tests/golden");
    let mut manifest: BTreeMap<String, String> = BTreeMap::new();
    let text = std::fs::read_to_string(golden.join("spec-check-manifest.txt")).expect("test harness invariant");
    for l in almide_corpus::manifest_rows(&text) {
        let mut it = l.splitn(3, '\t');
        let h = it.next().expect("test harness invariant").to_string();
        let _rc = it.next().expect("test harness invariant");
        manifest.insert(it.next().expect("test harness invariant").to_string(), h);
    }
    let manifest = judged(manifest, "check_parity");
    let mut db = Some(almide_spine::SpineDb::default());
    let mut compared = 0usize;
    let mut mismatches = Vec::new();
    for (rel, want) in &manifest {
        let text = std::fs::read_to_string(almide_corpus::resolve(&root, rel)).expect("test harness invariant");
        let tokens = almide::lexer::Lexer::tokenize(&text);
        let mut parser = almide::parser::Parser::new(tokens).with_file(rel);
        match parser.parse() {
            Ok(prog) if !almide_spine::s3::stdlib_only(&prog) => continue,
            _ => {}
        }
        let out = check_limited(&mut db, rel, text, almide_spine::s3::check_file_json_v3);
        if out.fatal.is_some() { continue; }
        let stdout = if out.diags.is_empty() { String::new() } else { format!("{}\n", out.diags.join("\n")) };
        if sha(&stdout) != *want { mismatches.push(rel.clone()); }
        compared += 1;
    }
    println!("stage2 v3 parity: {compared} compared");
    assert!(mismatches.is_empty(), "{} of {compared} diverge with the env template, first: {}", mismatches.len(), mismatches[0]);
    assert_compared(compared, "stage2 parity");
}
