//! Diagnostics-parity gate (unit 4): the ported checker behind
//! `check_file_json` must reproduce, byte for byte, the stdout of the ORACLE
//! `almide check <file> --json` (clean a877d2138 build) over the spec corpus.
//!
//! Coverage discipline (no silent gaps): every spec/**/*.almd is in exactly
//! one of the oracle manifest / oracle exclusions. Within the manifest, files
//! whose imports are not all stdlib modules fall outside `check_file_json`'s
//! purity contract; they are counted and printed, never silently dropped,
//! and everything else must hash-match.

use sha2::{Digest, Sha256};
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

fn workspace_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..").canonicalize().unwrap()
}

fn walk_almd(dir: &Path, out: &mut Vec<PathBuf>) {
    for entry in std::fs::read_dir(dir).unwrap() {
        let p = entry.unwrap().path();
        if p.is_dir() {
            walk_almd(&p, out);
        } else if p.extension().is_some_and(|e| e == "almd") {
            out.push(p);
        }
    }
}

fn sha(s: &str) -> String {
    format!("{:x}", Sha256::digest(s.as_bytes()))
}

#[test]
fn spec_corpus_check_matches_oracle_hashes() {
    let root = workspace_root();
    let golden = root.join("crates/almide-spine/tests/golden");
    // manifest rows: sha256 \t exit \t path  → path -> sha256
    let mut manifest: BTreeMap<String, String> = BTreeMap::new();
    for l in std::fs::read_to_string(golden.join("spec-check-manifest.txt"))
        .expect("run scripts/gen-check-manifest.sh")
        .lines()
    {
        let mut it = l.splitn(3, '\t');
        let h = it.next().unwrap().to_string();
        let _rc = it.next().unwrap();
        let p = it.next().unwrap().to_string();
        assert!(manifest.insert(p, h).is_none(), "duplicate path in manifest");
    }
    let mut exclusions: BTreeMap<String, String> = BTreeMap::new();
    for l in std::fs::read_to_string(golden.join("spec-check-exclusions.txt")).unwrap().lines() {
        let (p, r) = l.split_once('\t').unwrap();
        exclusions.insert(p.to_string(), r.to_string());
    }

    let mut files = Vec::new();
    walk_almd(&root.join("spec"), &mut files);
    files.sort();
    for f in &files {
        let rel = f.strip_prefix(&root).unwrap().to_string_lossy().to_string();
        let in_m = manifest.contains_key(&rel);
        let in_e = exclusions.contains_key(&rel);
        assert!(in_m ^ in_e, "{rel}: must be in exactly one of manifest/exclusions");
    }
    assert_eq!(files.len(), manifest.len() + exclusions.len(), "stale golden entries");

    let db = almide_spine::SpineDb::default();
    let mut purity_skipped = Vec::new();
    let mut mismatches = Vec::new();
    let mut compared = 0usize;
    for (rel, want) in &manifest {
        let text = std::fs::read_to_string(root.join(rel)).unwrap();
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
        let file = almide_spine::SourceFile::new(&db, rel.clone(), text);
        let out = almide_spine::s3::check_file_json(&db, file);
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
    assert!(compared > 900, "suspiciously few files compared ({compared}) — purity filter too broad?");
}
