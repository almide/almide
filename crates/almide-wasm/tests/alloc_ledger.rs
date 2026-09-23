//! Allocation ledger (#1586, the roc lesson made corpus-wide): stdout
//! equality cannot see an allocation regression — the bump heap makes
//! the TOTAL observable for free (the `__heap` watermark is monotonic),
//! so every corpus fixture's row is pinned EXACTLY in
//! golden/alloc-baseline.txt. A route that starts double-allocating
//! drifts its row; ratify deliberately:
//!
//!   ALMIDE_UPDATE_ALLOC=1 cargo test --release -p almide-wasm --test alloc_ledger
//!
//! WHAT THE NUMBER IS (#2344). The row is the FINAL `__heap` offset, not
//! the bytes a run allocated. The static string pool occupies the bottom
//! of linear memory and the bump heap starts above it, so the row is
//! `pool size + allocation` and moves with EITHER. A change that touches
//! the pool moves every row while allocating nothing differently — #2344's
//! dedupe moved 467 of them, all down, by the pool's own shrinkage. Read a
//! drifted row as "the end of memory moved", then ask which half moved:
//! the emitted data segment says the pool, the difference says allocation.
//!
//! THE SECOND LEDGER (#2407). A watermark is a PEAK: the size-class free
//! lists let a run reach the same `__heap` whether it allocated a thousand
//! blocks once or reused ten blocks a hundred times, so churn is invisible
//! in it. golden/alloc-count-baseline.txt pins, per fixture, what the
//! allocator DID — `allocs reused bytes frees` (every `$alloc` call, the
//! calls a free-list pop served, the payload bytes requested, every `$free`
//! call) — measured on a second emission with the counter switch armed
//! (`almide_wasm::alloc_count`). Both ledgers regenerate from the ONE
//! update run above, so they cannot drift apart. The armed emission is
//! also held to the unarmed one: same stdout, same watermark — the
//! instrument must not perturb what it measures.
//!
//! Fixtures whose watermark is run-dependent (entropy-fed string widths
//! and kin) are SELF-CALIBRATED out at generation time — the update run
//! executes everything twice and pins `~` (excluded) where the two
//! watermarks differ; excluded rows stay listed, never silently absent.
//! Both ledgers are machine-independent for every other fixture: #2568
//! emitted map_insertion_order on macOS arm64 and ubuntu x86_64 and got
//! byte-identical modules (sha256 9b609c96… unarmed, 92a160a7… armed) and
//! the same count on both. The row that looked host-dependent was a pin
//! taken from a build predating the indexed group_by (60b184548), so a
//! count row that disagrees with CI is a STALE PIN until an emit on both
//! hosts says otherwise — regenerate from the tree under test.

mod harness;
use harness::run_wasm;
use std::path::PathBuf;

fn workspace_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..").canonicalize().expect("test harness invariant")
}

fn baseline_path() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/golden/alloc-baseline.txt")
}

fn count_baseline_path() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/golden/alloc-count-baseline.txt")
}

fn watermark(bytes: &[u8]) -> u64 {
    let r = run_wasm(bytes).expect("engine runs the module");
    r.heap_end.expect("__heap export present")
}

/// The armed run's observables: the count row (`allocs reused bytes
/// frees`, tab-separated), its watermark and its stdout.
fn counted(bytes: &[u8]) -> (String, u64, String) {
    let r = run_wasm(bytes).expect("engine runs the armed module");
    let c = r.alloc_count.expect("the armed module exports the four counters");
    (
        format!("{}\t{}\t{}\t{}", c.allocs, c.reused, c.bytes, c.frees),
        r.heap_end.expect("__heap export present"),
        r.stdout,
    )
}

/// The pinned side of a ledger: `rel -> Some(value)` for a pinned row
/// (the watermark, or the tab-joined count quadruple), `rel -> None` for a
/// calibrated-out one, plus the set of `!` rows the structural leg is
/// expected to keep refusing.
type Ledger = (std::collections::BTreeMap<String, Option<String>>, std::collections::BTreeSet<String>);

fn read_ledger(path: &std::path::Path) -> Ledger {
    let mut pinned = std::collections::BTreeMap::new();
    let mut refused = std::collections::BTreeSet::new();
    let text = std::fs::read_to_string(path).unwrap_or_else(|_| {
        panic!("{} — generate with ALMIDE_UPDATE_ALLOC=1", path.display())
    });
    for l in text.lines() {
        // The fixture path is the LAST column and carries no tab; the value
        // is everything before it (one column for the watermark ledger,
        // four for the count ledger).
        let (v, rel) = l.rsplit_once('\t').expect("baseline row");
        if v == "!" {
            refused.insert(rel.to_string());
        } else {
            pinned.insert(rel.to_string(), if v == "~" { None } else { Some(v.to_string()) });
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

/// One fixture's verdict against a ledger, as the offence it raises (if any).
/// `want` is the row the ledger holds for it — `None` means the row is missing.
fn verdict(what: &str, rel: &str, want: Option<Option<String>>, got: Option<&str>) -> Option<String> {
    match (want, got) {
        (Some(Some(want)), Some(w)) if want == w => None,
        (Some(Some(want)), Some(w)) => Some(format!("{rel}: {what} {w} != pinned {want}").replace('\t', " ")),
        // A calibrated-out row accepts any value, and a host-variant
        // fixture (got = None) accepts only a calibrated-out row.
        (Some(None), _) => None,
        (Some(Some(_)), None) => {
            Some(format!("{rel}: host-variant fixture carries a pinned {what} — regenerate"))
        }
        (None, _) => Some(format!("{rel}: not in the {what} ledger — regenerate to ratify")),
    }
}

/// The generation side: the rows this fixture's measurement writes to the
/// two ledgers. A second measurement separates deterministic totals from
/// entropy-fed ones, per ledger.
fn generated_rows(rel: &str, bytes: Option<&[u8]>, armed: Option<&[u8]>) -> (String, String) {
    match (bytes, armed) {
        (Some(bytes), Some(armed)) => {
            let (w, w2) = (watermark(bytes), watermark(bytes));
            let ((c, _, _), (c2, _, _)) = (counted(armed), counted(armed));
            (
                if w == w2 { format!("{w}\t{rel}\n") } else { format!("~\t{rel}\n") },
                if c == c2 { format!("{c}\t{rel}\n") } else { format!("~\t{rel}\n") },
            )
        }
        _ => (format!("~\t{rel}\n"), format!("~\t{rel}\n")),
    }
}

/// The same fixture emitted with the counter switch armed (a thread-local
/// guard — the process environment is never touched).
fn emit_armed(ir: &almide_ir::IrProgram) -> Vec<u8> {
    let _guard = almide_wasm::alloc_count::CountGuard::set();
    almide_wasm::emit_program(ir).expect("the armed emission of a fixture the unarmed one accepted")
}

/// A structural refusal must be a ledgered `!` row in `what`.
fn note_refusal(rel: &str, what: &str, refused: &mut std::collections::BTreeSet<String>, offences: &mut Vec<String>) {
    if !refused.remove(rel) {
        offences.push(format!("{rel}: structural leg refuses it but the {what} has no `!` row — regenerate"));
    }
}

/// The check side for one fixture: both ledgers' verdicts, then the
/// instrument's own obligation — the armed module prints the same stdout
/// and ends at the same watermark as the unarmed one. (The watermark is
/// compared only where the unarmed one is pinned — an entropy-fed
/// fixture's two runs differ on their own.)
fn check_fixture(
    rel: &str,
    bytes: Option<&[u8]>,
    armed: Option<&[u8]>,
    pinned: Option<Option<String>>,
    count_pinned: Option<Option<String>>,
    offences: &mut Vec<String>,
) {
    let got = bytes.map(|b| run_wasm(b).expect("engine runs the module"));
    let got_s = got.as_ref().map(|r| r.heap_end.expect("__heap export present").to_string());
    let deterministic = matches!(pinned, Some(Some(_)));
    offences.extend(verdict("watermark", rel, pinned, got_s.as_deref()));
    let armed_got = armed.map(counted);
    offences.extend(verdict("count", rel, count_pinned, armed_got.as_ref().map(|(c, _, _)| c.as_str())));
    let (Some(r), Some((_, aw, astdout))) = (&got, &armed_got) else { return };
    if deterministic && Some(*aw) != r.heap_end {
        offences.push(format!(
            "{rel}: the armed module's watermark {aw} != the unarmed {:?} — the counter perturbed the heap",
            r.heap_end
        ));
    }
    if *astdout != r.stdout {
        offences.push(format!("{rel}: the armed module's stdout differs from the unarmed one"));
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
    let (bp, cp) = (baseline_path(), count_baseline_path());
    let ((mut baseline, mut refused), (mut counts, mut count_refused)) =
        if update { Default::default() } else { (read_ledger(&bp), read_ledger(&cp)) };

    let mut rows = String::new();
    let mut count_rows = String::new();
    let mut offences = Vec::new();
    for line in almide_corpus::manifest_rows(&manifest) {
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
                Ok(b) => Some((b, emit_armed(&ir))),
                Err(_) => {
                    if update {
                        rows.push_str(&format!("!\t{rel}\n"));
                        count_rows.push_str(&format!("!\t{rel}\n"));
                    } else {
                        note_refusal(rel, "ledger", &mut refused, &mut offences);
                        note_refusal(rel, "count ledger", &mut count_refused, &mut offences);
                    }
                    continue;
                }
            }
        };
        let (bytes, armed) = match &emitted {
            Some((b, a)) => (Some(b.as_slice()), Some(a.as_slice())),
            None => (None, None),
        };
        if update {
            let (r, c) = generated_rows(rel, bytes, armed);
            rows.push_str(&r);
            count_rows.push_str(&c);
            continue;
        }
        check_fixture(rel, bytes, armed, baseline.remove(rel), counts.remove(rel), &mut offences);
    }
    if update {
        std::fs::write(&bp, &rows).expect("write baseline");
        std::fs::write(&cp, &count_rows).expect("write count baseline");
        return;
    }
    for (rel, _) in baseline {
        offences.push(format!("{rel}: in the ledger but not in the corpus — regenerate"));
    }
    for (rel, _) in counts {
        offences.push(format!("{rel}: in the count ledger but not in the corpus — regenerate"));
    }
    for rel in refused.iter().chain(&count_refused) {
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
