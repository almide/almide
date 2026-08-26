//! Allocation ledger (#1586, the roc lesson made corpus-wide): stdout
//! equality cannot see an allocation regression — the bump heap makes
//! the TOTAL observable for free (the `__heap` watermark is monotonic),
//! so every corpus fixture's allocation total is pinned EXACTLY in
//! golden/alloc-baseline.txt. A route that starts double-allocating
//! drifts its row; ratify deliberately:
//!
//!   ALMIDE_UPDATE_ALLOC=1 cargo test --release -p almide-wasm --test alloc_ledger
//!
//! Fixtures whose watermark is run-dependent (entropy-fed string widths
//! and kin) are SELF-CALIBRATED out at generation time — the update run
//! executes everything twice and pins `~` (excluded) where the two
//! watermarks differ; excluded rows stay listed, never silently absent.

mod harness;
use harness::run_wasm;
use std::path::PathBuf;

fn workspace_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..").canonicalize().expect("test harness invariant")
}

fn baseline_path() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/golden/alloc-baseline.txt")
}

fn watermark(bytes: &[u8]) -> u64 {
    let r = run_wasm(bytes).expect("engine runs the module");
    r.heap_end.expect("__heap export present")
}

#[cfg_attr(debug_assertions, ignore = "ledger sweep is release-only (CI: release-shape job)")]
#[test]
fn corpus_allocation_watermarks_hold() {
    let root = workspace_root();
    let manifest = std::fs::read_to_string(
        root.join("crates/almide-spine/tests/golden/spec-run-manifest.txt"),
    )
    .expect("run manifest");

    let update = std::env::var("ALMIDE_UPDATE_ALLOC").is_ok();
    let bp = baseline_path();
    let mut baseline: std::collections::BTreeMap<String, Option<u64>> = std::collections::BTreeMap::new();
    if !update {
        for l in std::fs::read_to_string(&bp)
            .expect("golden/alloc-baseline.txt — generate with ALMIDE_UPDATE_ALLOC=1")
            .lines()
        {
            let (v, rel) = l.split_once('\t').expect("baseline row");
            baseline.insert(rel.to_string(), if v == "~" { None } else { Some(v.parse().expect("watermark")) });
        }
    }

    let mut rows = String::new();
    let mut offences = Vec::new();
    for line in manifest.lines() {
        let rel = line.splitn(3, '\t').nth(2).expect("manifest row");
        let text = std::fs::read_to_string(almide_corpus::resolve(&root, rel)).expect("fixture readable");
        // Host-boundary fixtures allocate HOST-SHAPED strings (cwd and
        // temp-dir lengths, directory listings) — their watermarks vary
        // per machine, which same-machine double-run calibration cannot
        // see (11 fs_/env_ rows drifted on the ubuntu runner). Excluded
        // by principle, not by list.
        let host_variant = text
            .lines()
            .any(|l| matches!(l.trim(), "import fs" | "import env" | "import process"));
        if host_variant {
            if update {
                rows.push_str(&format!("~\t{rel}\n"));
            } else {
                match baseline.remove(rel) {
                    Some(None) => {}
                    Some(Some(_)) => offences.push(format!(
                        "{rel}: host-variant fixture carries a pinned watermark — regenerate"
                    )),
                    None => offences.push(format!("{rel}: not in the ledger — regenerate to ratify")),
                }
            }
            continue;
        }
        let ir = almide_spine::s5::lower_to_ir(rel, &text).expect("front");
        let bytes = almide_wasm::emit_program(&ir).expect("emit");
        let w = watermark(&bytes);
        if update {
            // Self-calibration: a second run separates deterministic
            // watermarks from entropy-fed ones.
            let w2 = watermark(&bytes);
            if w == w2 {
                rows.push_str(&format!("{w}\t{rel}\n"));
            } else {
                rows.push_str(&format!("~\t{rel}\n"));
            }
            continue;
        }
        match baseline.remove(rel) {
            Some(Some(want)) if want == w => {}
            Some(Some(want)) => offences.push(format!("{rel}: watermark {w} != pinned {want}")),
            Some(None) => {} // calibrated-out (nondeterministic) row
            None => offences.push(format!("{rel}: not in the ledger — regenerate to ratify")),
        }
    }
    if update {
        std::fs::write(&bp, &rows).expect("write baseline");
        return;
    }
    for (rel, _) in baseline {
        offences.push(format!("{rel}: in the ledger but not in the corpus — regenerate"));
    }
    assert!(
        offences.is_empty(),
        "allocation ledger ({} drift(s)) — an allocation change is ratified by \
         ALMIDE_UPDATE_ALLOC=1 regeneration, never silently:\n{}",
        offences.len(),
        offences.join("\n")
    );
}
