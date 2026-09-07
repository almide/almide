//! The interp abstain-ledger GATE (CG-1 gap audit): the set of
//! `spec/wasm_cross/*.almd` fixtures the reference interpreter cannot evaluate
//! must equal the committed `crates/almide-interp/interp-abstain-ledger.txt`,
//! in both directions — a new abstain and a stale entry both fail.
//!
//! Backend-free by design: only the interp leg runs, so this binary needs no
//! `almide` binary and no wasmtime, and it NEVER self-skips on CI. It includes
//! interp_leg.rs alone — not corpus.rs — so nothing here can trigger the
//! native+wasm corpus builds and destroy exactly that property.
//!
//! Split out of the former `wasm_runtime_test` binary with the other gates so
//! the CI shard packer (scripts/ci-test-shard.sh) can spread them.

// interp_leg.rs also serves the 3-way oracle; the ledger only reads `Skip`.
#![allow(dead_code)]

use std::path::{Path, PathBuf};

include!("wasm_runtime_test_parts/interp_leg.rs");

// ── The abstain ledger gate (CG-1 gap audit) ──
//
// Backend-free by design: it evaluates only the interp leg, so it NEVER
// self-skips on a missing almide binary or wasmtime. It deliberately does NOT
// read `corpus()` — touching that table would trigger 318 native+wasm builds
// and destroy exactly the property that makes this gate trustworthy.

fn spec_dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("spec/wasm_cross")
}

/// The committed inventory of fixtures the interpreter cannot evaluate.
fn ledger_path() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("crates/almide-interp/interp-abstain-ledger.txt")
}

/// Coverage audit for the executable spec: runs ONLY the interp leg over the
/// cross-target corpus (no almide binary, no wasmtime — this gate never
/// self-skips on CI) and holds the observed abstain set equal to the committed
/// ledger, in both directions:
///
///   - a fixture the interp cannot evaluate but absent from the ledger FAILS —
///     coverage shrinkage must be a reviewed ledger edit in the same PR, never
///     a silent drift (the documented weakness this gate exists to close);
///   - a ledger entry whose fixture now evaluates (or was renamed/removed)
///     FAILS — stale entries hide progress; the ledger may only shrink.
///
/// The ledger never decides WHAT is skipped (skips stay interp-self-reported);
/// it only audits the set. Regenerate after a deliberate change with
/// `ALMIDE_UPDATE_INTERP_LEDGER=1` and review the diff.
#[test]
fn interp_abstain_ledger() {
    let dir = spec_dir();
    if !dir.exists() {
        eprintln!(
            "interp_abstain_ledger: {} missing — skipping",
            dir.display()
        );
        return;
    }
    let mut entries: Vec<_> = std::fs::read_dir(&dir)
        .unwrap()
        .filter_map(|e| e.ok())
        .filter(|e| e.path().extension().map(|x| x == "almd").unwrap_or(false))
        .collect();
    entries.sort_by_key(|e| e.path());
    if entries.is_empty() {
        eprintln!("interp_abstain_ledger: corpus empty — skipping");
        return;
    }

    let total = entries.len();
    // fixture stem → first-line reason, in corpus order
    let mut observed: Vec<(String, String)> = Vec::new();
    for entry in &entries {
        let path = entry.path();
        let name = path.file_stem().unwrap().to_str().unwrap().to_string();
        let source = std::fs::read_to_string(&path).unwrap();
        if let InterpLeg::Skip(reason) = run_interp_capture(&source) {
            observed.push((name, reason.replace('\n', " ")));
        }
    }

    if std::env::var("ALMIDE_UPDATE_INTERP_LEDGER").is_ok() {
        let mut out = String::from(
            "# interp-abstain-ledger — fixtures of spec/wasm_cross/ the reference\n\
             # interpreter cannot evaluate (its self-reported coverage gaps), i.e. the\n\
             # current boundary of the executable spec. CG-1 gap audit; shrink to zero.\n\
             #\n\
             # Format: <fixture-stem>  <reason as last observed>  (first token is the key)\n\
             # Gate:   wasm_runtime_interp_ledger.rs::interp_abstain_ledger — fails on a\n\
             #         new abstain missing here AND on a stale entry that now evaluates.\n\
             # Regenerate (then review the diff!):\n\
             #   ALMIDE_UPDATE_INTERP_LEDGER=1 cargo test --test wasm_runtime_interp_ledger interp_abstain_ledger\n\
             # Preferred alternative to adding an entry: widen the interp glue\n\
             # (bridge.rs / hofs.rs / dispatch.rs — see crates/almide-interp/CLAUDE.md).\n\n",
        );
        for (n, r) in &observed {
            out.push_str(&format!("{n}  {r}\n"));
        }
        std::fs::write(ledger_path(), out).unwrap();
        eprintln!(
            "interp_abstain_ledger: regenerated with {} abstain(s) of {} fixtures — review the diff",
            observed.len(),
            total
        );
        return;
    }

    let ledger_text = std::fs::read_to_string(ledger_path()).unwrap_or_else(|_| {
        panic!(
            "interp-abstain-ledger.txt missing at {} — seed it with \
             ALMIDE_UPDATE_INTERP_LEDGER=1 cargo test --test wasm_runtime_interp_ledger interp_abstain_ledger",
            ledger_path().display()
        )
    });
    let ledger: std::collections::BTreeSet<String> = ledger_text
        .lines()
        .map(str::trim)
        .filter(|l| !l.is_empty() && !l.starts_with('#'))
        .filter_map(|l| l.split_whitespace().next().map(str::to_string))
        .collect();
    let observed_set: std::collections::BTreeSet<String> =
        observed.iter().map(|(n, _)| n.clone()).collect();

    let new_abstains: Vec<&(String, String)> = observed
        .iter()
        .filter(|(n, _)| !ledger.contains(n))
        .collect();
    let stale: Vec<&String> = ledger
        .iter()
        .filter(|n| !observed_set.contains(*n))
        .collect();

    eprintln!(
        "\ninterp_abstain_ledger (executable-spec coverage): {} fixtures | {} evaluated | {} abstained (ledgered)",
        total,
        total - observed.len(),
        observed.len()
    );

    let mut failures = String::new();
    if !new_abstains.is_empty() {
        failures.push_str(&format!(
            "\nUNLEDGERED ABSTAIN(S) — the interpreter cannot evaluate {} fixture(s) not \
             recorded in interp-abstain-ledger.txt:\n",
            new_abstains.len()
        ));
        for (n, r) in &new_abstains {
            failures.push_str(&format!("    - {n}: {r}\n"));
        }
        failures.push_str(
            "  Preferred fix: widen the interp glue so the fixture evaluates \
             (bridge.rs / hofs.rs / dispatch.rs — see crates/almide-interp/CLAUDE.md).\n  \
             Otherwise: record the abstention in the ledger IN THIS SAME PR \
             (ALMIDE_UPDATE_INTERP_LEDGER=1 regenerates) — shrinking the executable \
             spec's coverage is a reviewed decision, never a silent drift.\n",
        );
    }
    if !stale.is_empty() {
        failures.push_str(&format!(
            "\nSTALE LEDGER ENTRY(IES) — {} ledgered fixture(s) no longer abstain \
             (now evaluated, renamed, or removed):\n",
            stale.len()
        ));
        for n in &stale {
            failures.push_str(&format!("    - {n}\n"));
        }
        failures.push_str(
            "  Remove the entries (ALMIDE_UPDATE_INTERP_LEDGER=1 regenerates) — \
             the ledger may only shrink toward zero.\n",
        );
    }
    if !failures.is_empty() {
        panic!("{failures}");
    }
}
