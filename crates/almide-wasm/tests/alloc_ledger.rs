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

/// The pinned side of the ledger: `rel -> Some(watermark)` for a pinned row,
/// `rel -> None` for a calibrated-out one, plus the set of `!` rows the
/// structural leg is expected to keep refusing.
type Ledger = (std::collections::BTreeMap<String, Option<u64>>, std::collections::BTreeSet<String>);

fn read_ledger(path: &std::path::Path) -> Ledger {
    let mut pinned = std::collections::BTreeMap::new();
    let mut refused = std::collections::BTreeSet::new();
    let text = std::fs::read_to_string(path)
        .expect("golden/alloc-baseline.txt — generate with ALMIDE_UPDATE_ALLOC=1");
    for l in text.lines() {
        let (v, rel) = l.split_once('\t').expect("baseline row");
        if v == "!" {
            refused.insert(rel.to_string());
        } else {
            pinned.insert(rel.to_string(), if v == "~" { None } else { Some(v.parse().expect("watermark")) });
        }
    }
    (pinned, refused)
}

/// Host-boundary fixtures allocate HOST-SHAPED strings (cwd and temp-dir
/// lengths, directory listings) — their watermarks vary per machine, which
/// same-machine double-run calibration cannot see (11 fs_/env_ rows drifted on
/// the ubuntu runner). Excluded by principle, not by list.
fn is_host_variant(text: &str) -> bool {
    text.lines().any(|l| matches!(l.trim(), "import fs" | "import env" | "import process"))
}

/// One fixture's verdict against the ledger, as the offence it raises (if any).
/// `want` is the row the ledger holds for it — `None` means the row is missing.
fn verdict(rel: &str, want: Option<Option<u64>>, got: Option<u64>) -> Option<String> {
    match (want, got) {
        (Some(Some(want)), Some(w)) if want == w => None,
        (Some(Some(want)), Some(w)) => Some(format!("{rel}: watermark {w} != pinned {want}")),
        // A calibrated-out row accepts any watermark, and a host-variant
        // fixture (got = None) accepts only a calibrated-out row.
        (Some(None), _) => None,
        (Some(Some(_)), None) => {
            Some(format!("{rel}: host-variant fixture carries a pinned watermark — regenerate"))
        }
        (None, _) => Some(format!("{rel}: not in the ledger — regenerate to ratify")),
    }
}

/// The generation side: the row this fixture's measurement writes. A second
/// watermark separates deterministic totals from entropy-fed ones.
fn generated_row(rel: &str, bytes: Option<&[u8]>) -> String {
    match bytes {
        None => format!("~\t{rel}\n"),
        Some(bytes) => {
            let (w, w2) = (watermark(bytes), watermark(bytes));
            if w == w2 { format!("{w}\t{rel}\n") } else { format!("~\t{rel}\n") }
        }
    }
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
    let (mut baseline, mut refused) =
        if update { Default::default() } else { read_ledger(&bp) };

    let mut rows = String::new();
    let mut offences = Vec::new();
    for line in manifest.lines() {
        let rel = line.splitn(3, '\t').nth(2).expect("manifest row");
        let text = std::fs::read_to_string(almide_corpus::resolve(&root, rel)).expect("fixture readable");
        // A `!` row: the structural leg REFUSES this fixture (the CLI
        // reroutes it to the incumbent — sql_highlight_tokens' unfoldable
        // mut write-back under a loop branch, #1688). The ledger asserts
        // the refusal STAYS a refusal: this shape silently emitting again
        // is exactly the regression the wall exists to prevent.
        let emitted = if is_host_variant(&text) {
            None
        } else {
            let ir = almide_spine::s5::lower_to_ir(rel, &text).expect("front");
            match almide_wasm::emit_program(&ir) {
                Ok(b) => Some(b),
                Err(_) => {
                    if update {
                        rows.push_str(&format!("!\t{rel}\n"));
                    } else if !refused.remove(rel) {
                        offences.push(format!(
                            "{rel}: structural leg refuses it but the ledger has no `!` row — regenerate"
                        ));
                    }
                    continue;
                }
            }
        };
        if update {
            rows.push_str(&generated_row(rel, emitted.as_deref()));
            continue;
        }
        let got = emitted.as_deref().map(watermark);
        offences.extend(verdict(rel, baseline.remove(rel), got));
    }
    if update {
        std::fs::write(&bp, &rows).expect("write baseline");
        return;
    }
    for (rel, _) in baseline {
        offences.push(format!("{rel}: in the ledger but not in the corpus — regenerate"));
    }
    for rel in refused {
        offences.push(format!(
            "{rel}: ledgered `!` (structural-refused) but now lowers — regenerate to ratify the emit"
        ));
    }
    assert!(
        offences.is_empty(),
        "allocation ledger ({} drift(s)) — an allocation change is ratified by \
         ALMIDE_UPDATE_ALLOC=1 regeneration, never silently:\n{}",
        offences.len(),
        offences.join("\n")
    );
}
