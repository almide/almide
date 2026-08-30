//! Structural-witness floor (#1696 step 4 phase A): sweep the run-manifest
//! corpus with the witness sink armed, and hold three facts:
//!
//!   1. NO certificate is the `!poison` sentinel — the straightline gate
//!      and the recorder hooks agree on every admitted body (a poison
//!      means an RC event fired that the hooks could not attribute:
//!      a gate bug, loudly);
//!   2. every collected certificate BALANCES under the mirror of the
//!      proven rule (per object: no release at rc 0, no leak);
//!   3. the count of witnessed functions never shrinks —
//!      golden/witness-floor.txt, grow-only, ratified with
//!      ALMIDE_UPDATE_WITNESS_FLOOR=1 (phases B/C admit more shapes and
//!      raise it; a refactor that silently drops recording fails here).
//!
//! Phase A2 wires these certificates into proofs/gate.sh so the EXTRACTED
//! kernel-proven checker re-verifies them — this test is the Rust-side
//! mirror that keeps the pipeline honest per PR without the opam
//! toolchain.

use std::path::PathBuf;

fn workspace_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..").canonicalize().expect("test harness invariant")
}

fn floor_path() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/golden/witness-floor.txt")
}

#[cfg_attr(debug_assertions, ignore = "corpus sweep is release-only (CI: release-shape job)")]
#[test]
fn structural_witnesses_balance_and_hold_the_floor() {
    let root = workspace_root();
    let manifest = std::fs::read_to_string(
        root.join("crates/almide-spine/tests/golden/spec-run-manifest.txt"),
    )
    .expect("run manifest");

    let mut certs: std::collections::BTreeMap<String, String> = std::collections::BTreeMap::new();
    let mut poisoned: Vec<String> = Vec::new();
    let mut unbalanced: Vec<(String, String)> = Vec::new();
    let mut nondet: Vec<String> = Vec::new();
    for line in manifest.lines() {
        let rel = line.splitn(3, '\t').nth(2).expect("manifest row");
        let text = std::fs::read_to_string(almide_corpus::resolve(&root, rel)).expect("fixture readable");
        let Ok(ir) = almide_spine::s5::lower_to_ir(rel, &text) else { continue };
        almide_wasm::witness::start_collecting();
        let _ = almide_wasm::emit_program(&ir);
        for (name, cert) in almide_wasm::witness::take() {
            let key = format!("{rel} :: {name}");
            if cert.starts_with('!') {
                poisoned.push(key);
            } else if !almide_wasm::witness::balanced(&cert) {
                unbalanced.push((key, cert));
            } else {
                if std::env::var("ALMIDE_WITNESS_DUMP").is_ok() {
                    eprintln!("[witness] {key}\n{cert}");
                }
                // emit_program lowers every fn once per emission pass
                // (the reachability two-pass) — the passes must agree.
                if let Some(prev) = certs.insert(key.clone(), cert.clone()) {
                    if prev != cert {
                        nondet.push(key);
                    }
                }
            }
        }
    }
    let witnessed = certs.len();
    assert!(
        nondet.is_empty(),
        "the two emission passes disagree on {} witness(es): {nondet:?}",
        nondet.len()
    );

    assert!(
        poisoned.is_empty(),
        "gate/hook disagreement — {} function(s) poisoned their witness:\n{}",
        poisoned.len(),
        poisoned.join("\n")
    );
    assert!(
        unbalanced.is_empty(),
        "{} certificate(s) fail the balance mirror:\n{:?}",
        unbalanced.len(),
        unbalanced
    );

    let fp = floor_path();
    if std::env::var("ALMIDE_UPDATE_WITNESS_FLOOR").is_ok() {
        std::fs::write(&fp, format!("{witnessed}\n")).expect("write floor");
        return;
    }
    let floor: usize = std::fs::read_to_string(&fp)
        .expect("golden/witness-floor.txt — generate with ALMIDE_UPDATE_WITNESS_FLOOR=1")
        .trim()
        .parse()
        .expect("floor number");
    assert!(
        witnessed >= floor,
        "witnessed {witnessed} < floor {floor} — recording coverage shrank; \
         restore it or ratify the shrink deliberately"
    );
}
