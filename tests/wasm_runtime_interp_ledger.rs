//! The interp abstain-ledger GATE (CG-1 gap audit): the set of
//! `spec/wasm_cross/*.almd` fixtures the reference interpreter cannot evaluate
//! must equal the committed `crates/almide-interp/interp-abstain-ledger.txt`,
//! in both directions — a new abstain, stale entry, or changed reason fails.
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
///   - a recorded reason differing from the observed reason FAILS.
///
/// The ledger never decides WHAT is skipped (skips stay interp-self-reported);
/// it audits the set and its reasons. Regenerate after a deliberate change with
/// `ALMIDE_UPDATE_INTERP_LEDGER=1` and review the diff.
///
/// ONE sweep feeds BOTH ledgers (#2381). The abstain audit and the
/// bridge-fallback audit below read the same per-fixture interp outcome
/// (`run_interp_capture_with_fallbacks` returns the leg AND the fallbacks), so
/// the corpus is evaluated once — on a scoped thread pool, rows in corpus
/// order (interp_leg.rs::interp_sweep_parallel) — and each audit reads its
/// half. Before, the two gates were two `#[test]`s that each swept the corpus
/// serially: 1666 s + 1586 s on CI for what is the same computation twice.
/// The test's name contains both former names, so the regeneration commands
/// the ledger headers print (`… interp_abstain_ledger` /
/// `… interp_bridge_fallback_ledger`) still select it; under
/// `ALMIDE_UPDATE_INTERP_LEDGER` both ledgers are rewritten. Both audits
/// always run; their failure texts are joined so neither hides the other.
///
/// `ALMIDE_CORPUS_SHARD` (#2381) slices the ONE sweep, and each audit judges
/// what is sound on its half of a slice — the abstain ledger is keyed by
/// fixture both ways, the bridge ledger by NAME — see `Sweep` and the two
/// audits; `merge/N` walks nothing and judges the bridge names over every
/// shard's partial.
#[test]
fn interp_abstain_ledger_and_interp_bridge_fallback_ledger() {
    let Some(sweep) = sweep_corpus() else {
        return;
    };
    let mut failures = audit_abstain_ledger(&sweep);
    failures.push_str(&audit_bridge_fallback_ledger(&sweep));
    if !failures.is_empty() {
        panic!("{failures}");
    }
}

/// One fixture's row of the sweep: its stem, the interp leg and the bridge
/// fallbacks it reached, in sorted corpus order.
type SweepRow = (String, InterpLeg, Vec<(String, String)>);

/// The gate name the shard partials are filed under: the CI solo leg
/// (`interp_ledger` in scripts/ci-test-shard.sh), one walked-fixture list and
/// one bridge partial per shard for the one sweep both audits read.
const SHARD_GATE: &str = "interp_ledger";

/// The one sweep both audits read, plus what a corpus slice needs to judge
/// itself: the sorted stems of the WHOLE corpus (the merge's coverage
/// expectation) and the shard the rows were taken under.
struct Sweep {
    /// The rows actually evaluated — the whole corpus, one slice of it under
    /// `k/N`, or nothing under `merge/N`.
    rows: Vec<SweepRow>,
    /// Every fixture stem of spec/wasm_cross, sorted, before any slice.
    all_stems: Vec<String>,
    shard: Option<almide_corpus::CorpusShard>,
}

impl Sweep {
    /// The stems this sweep walked, for the ledger ∩ shard judgement.
    fn walked(&self) -> std::collections::BTreeSet<String> {
        self.rows.iter().map(|(stem, _, _)| stem.clone()).collect()
    }

    fn slice(&self) -> Option<almide_corpus::CorpusShard> {
        match self.shard {
            Some(s @ almide_corpus::CorpusShard::Slice { .. }) => Some(s),
            _ => None,
        }
    }

    fn merge_n(&self) -> Option<usize> {
        match self.shard {
            Some(almide_corpus::CorpusShard::Merge { n }) => Some(n),
            _ => None,
        }
    }

    /// Regenerating either ledger needs ONE sweep of the WHOLE corpus.
    fn refuse_update_under_shard(&self) {
        assert!(
            self.shard.is_none(),
            "ALMIDE_UPDATE_INTERP_LEDGER over a corpus slice or merge would not be one sweep of the whole corpus — unset ALMIDE_CORPUS_SHARD to regenerate"
        );
    }
}

/// The interp sweep over spec/wasm_cross — sorted by path, evaluated once on
/// the pool. `None` when the corpus directory is missing or empty (skip).
///
/// `ALMIDE_CORPUS_SHARD=k/N` (#2381) is applied AFTER the sort and nowhere
/// else; the slice's walked list goes to `ALMIDE_CORPUS_SHARD_DIR` for the
/// coverage step (∪ shards == ls spec/wasm_cross). `merge/N` evaluates
/// nothing: the audits read the shards' partials instead.
fn sweep_corpus() -> Option<Sweep> {
    let dir = spec_dir();
    if !dir.exists() {
        eprintln!("interp ledgers: {} missing — skipping", dir.display());
        return None;
    }
    let mut entries: Vec<_> = std::fs::read_dir(&dir)
        .unwrap()
        .filter_map(|e| e.ok())
        .filter(|e| e.path().extension().map(|x| x == "almd").unwrap_or(false))
        .collect();
    entries.sort_by_key(|e| e.path());
    if entries.is_empty() {
        eprintln!("interp ledgers: corpus empty — skipping");
        return None;
    }
    let stem_of = |e: &std::fs::DirEntry| e.path().file_stem().unwrap().to_str().unwrap().to_string();
    let all_stems: Vec<String> = entries.iter().map(stem_of).collect();
    let shard = almide_corpus::corpus_shard();
    if let Some(s) = shard {
        entries = s.apply(entries, SHARD_GATE, stem_of);
        if let almide_corpus::CorpusShard::Slice { .. } = s {
            let walked: Vec<String> = entries.iter().map(stem_of).collect();
            almide_corpus::write_partial(s, SHARD_GATE, "fixtures", &walked);
        }
    }
    let stems: Vec<String> = entries.iter().map(stem_of).collect();
    let sources: Vec<String> = entries
        .iter()
        .map(|e| std::fs::read_to_string(e.path()).unwrap())
        .collect();
    let rows = interp_sweep_parallel(&sources, &stems);
    Some(Sweep {
        rows: stems
            .into_iter()
            .zip(rows)
            .map(|(stem, (leg, fallbacks))| (stem, leg, fallbacks))
            .collect(),
        all_stems,
        shard,
    })
}

/// The abstain audit over a finished sweep: returns the failure text ("" when
/// the observed abstain set and reasons equal the ledger), or regenerates the
/// ledger under `ALMIDE_UPDATE_INTERP_LEDGER` and returns "".
///
/// Under a corpus slice this ledger is judged whole in the shard — it is
/// keyed by FIXTURE in both directions — once `stale` is taken against the
/// ledger ∩ this shard's walked fixtures: a ledgered fixture another shard
/// walks is not "no longer abstaining". It has no merge form (the coverage
/// step unions the walked lists), so `merge/N` judges nothing here.
fn audit_abstain_ledger(sweep: &Sweep) -> String {
    if let Some(n) = sweep.merge_n() {
        eprintln!("interp_abstain_ledger: merge/{n} — fixture-keyed, judged whole in each shard; nothing to merge");
        return String::new();
    }
    let total = sweep.rows.len();
    // fixture stem → first-line reason, in corpus order
    let observed: Vec<(String, String)> = sweep
        .rows
        .iter()
        .filter_map(|(name, leg, _)| match leg {
            InterpLeg::Skip(reason) => Some((name.clone(), reason.replace('\n', " "))),
            InterpLeg::Ran(..) => None,
        })
        .collect();

    if std::env::var("ALMIDE_UPDATE_INTERP_LEDGER").is_ok() {
        sweep.refuse_update_under_shard();
        let mut out = String::from(
            "# interp-abstain-ledger — fixtures of spec/wasm_cross/ the reference\n\
             # interpreter cannot evaluate (its self-reported coverage gaps), i.e. the\n\
             # current boundary of the executable spec. CG-1 gap audit; shrink to zero.\n\
             #\n\
             # Format: <fixture-stem>  <reason>  (two spaces separate them)\n\
             # Gate:   wasm_runtime_interp_ledger.rs::interp_abstain_ledger — fails on a\n\
             #         new abstain missing here, on a stale entry that now evaluates, AND\n\
             #         on a recorded reason the interpreter no longer reports (#2333: the\n\
             #         reason is what check-abstain-classes.sh classifies, so it is held\n\
             #         equal, not merely carried).\n\
             # Regenerate (then review the diff!):\n\
             #   ALMIDE_UPDATE_INTERP_LEDGER=1 cargo test --test wasm_runtime_interp_ledger interp_abstain_ledger\n\
             # Preferred alternative to adding an entry: widen the interp glue\n\
             # (bridge.rs / hofs.rs / dispatch.rs — see crates/almide-interp/CLAUDE.md).\n\
             #\n\
             # READING A ROW THAT NAMES A DECLARED TYPE (#2359). When a reason says the\n\
             # impl the interp RESOLVED declares some element type, that type is the\n\
             # parameter of the body the interp picked — NOT the program's type, and not a\n\
             # sign the fixture is miscompiled. The interp resolves a public container fn\n\
             # to its SCALAR core by name (`set.len` -> `set_len(s: Set[Int])` in\n\
             # stdlib/set_core.almd), because the pass that picks the _str/_skv/_hval\n\
             # variant is a MIR lowering it never runs. So a Set[String] fixture can\n\
             # legitimately meet a Set[Int] declaration here. Materializing anyway is what\n\
             # the guard refuses: a body comparing slots as raw i64s would run over heap\n\
             # values and same-content strings in different blocks would miss — a WRONG\n\
             # VOTE, which is worse than an abstain. Two readers took the older wording\n\
             # (\"under the declared element type Int\") as a claim about the program and\n\
             # went looking for a miscompile; the rows now say whose declaration it is.\n\n",
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
        return String::new();
    }

    let ledger_text = std::fs::read_to_string(ledger_path()).unwrap_or_else(|_| {
        panic!(
            "interp-abstain-ledger.txt missing at {} — seed it with \
             ALMIDE_UPDATE_INTERP_LEDGER=1 cargo test --test wasm_runtime_interp_ledger interp_abstain_ledger",
            ledger_path().display()
        )
    });
    // stem → recorded reason. The reason is held EQUAL to the observed one, not
    // merely carried: a ledger that records "the reason as last observed" and is
    // never checked against observation drifts silently, and then the very
    // regeneration the failure message prescribes rewrites rows nobody touched —
    // unclassing them in check-abstain-classes.sh, which reads the reason text
    // (#2333).
    let mut recorded = parse_reason_ledger(&ledger_text, false);
    if sweep.slice().is_some() {
        // The ledger ∩ this shard: `stale` below then asks only about the
        // fixtures this shard actually walked.
        let walked = sweep.walked();
        recorded.retain(|n, _| walked.contains(n));
    }
    let ledger: std::collections::BTreeSet<String> = recorded.keys().cloned().collect();
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
        "\ninterp_abstain_ledger (executable-spec coverage): {} fixtures | {} evaluated | {} abstained (ledgered){}",
        total,
        total - observed.len(),
        observed.len(),
        match sweep.slice() {
            Some(almide_corpus::CorpusShard::Slice { k, n }) =>
                format!(" — shard {k}/{n}: stale judged against the ledger ∩ this shard"),
            _ => String::new(),
        }
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
    let drifted: Vec<(&String, &String, &String)> = observed
        .iter()
        .filter_map(|(n, r)| {
            recorded
                .get(n)
                .filter(|rec| *rec != r)
                .map(|rec| (n, rec, r))
        })
        .collect();
    if !drifted.is_empty() {
        failures.push_str(&format!(
            "\nDRIFTED REASON(S) — {} ledgered fixture(s) still abstain, but for a \
             reason the ledger does not record:\n",
            drifted.len()
        ));
        for (n, rec, obs) in &drifted {
            failures.push_str(&format!(
                "    - {n}\n        recorded: {rec}\n        observed: {obs}\n"
            ));
        }
        failures.push_str(
            "  The reason is the auditable half of this ledger — check-abstain-classes.sh \
             classifies each abstain by its TEXT, so a reason that moves without the ledger \
             moving unclasses the row the next time anyone regenerates. Re-record in this \
             same PR (ALMIDE_UPDATE_INTERP_LEDGER=1) and keep the class patterns matching.\n",
        );
    }
    failures
}

// ── The bridge-fallback ledger gate (#2185) ──
//
// The lowered self-hosted body is the third vote at the pool boundary; the
// hand-mirrored bridge answers there only when that body ABSTAINS (a
// heap/effect prim outside the scalar floor, the eager `mut`-param gate, an
// arity it cannot take) or has no body at all, and INSIDE the pool tier it is
// the floor a body consumes (tagged `floor:` in the ledger). This gate holds
// the set of `module.func` names the bridge answers for, over the
// cross-target corpus, equal to the committed
// `crates/almide-interp/interp-bridge-fallback-ledger.txt` in both directions:
// a name outside the ledger is a NEW dependence on a Rust copy (widen the
// floor so the body evaluates, or record it in the same PR); a ledgered name
// the corpus no longer reaches through the bridge is a shadowed arm — delete
// it from bridge.rs and the entry, the ledger only shrinks. Backend-free like
// the abstain gate above, and fed by the SAME sweep (#2381): the audit below
// reads the fallbacks half of each row the abstain audit read the leg of.

/// The committed inventory of bridge arms the corpus still reaches as fallbacks.
fn fallback_ledger_path() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("crates/almide-interp/interp-bridge-fallback-ledger.txt")
}

/// The bridge-fallback audit over a finished sweep: the failure text ("" on
/// equality), or a regeneration under `ALMIDE_UPDATE_INTERP_LEDGER` and "".
///
/// This ledger is keyed by NAME, not fixture (#2381): a name reached only by
/// fixtures outside this shard would read as stale, and a shard cannot tell.
/// So a slice (`k/N`) judges only what IS sound on a subset — a name the
/// ledger lacks — and leaves its observed names as a partial; `merge/N`
/// unions the N partials (first fixture in corpus order wins, as in one
/// sweep) and runs the whole judge below unchanged.
fn audit_bridge_fallback_ledger(sweep: &Sweep) -> String {
    let slice = sweep.slice();
    // name → (first fixture that reached it, the body's reason), corpus order
    let mut observed: std::collections::BTreeMap<String, (String, String)> =
        std::collections::BTreeMap::new();
    let mut calls = 0usize;
    let walked_n;
    if let Some(n) = sweep.merge_n() {
        almide_corpus::assert_partials_cover(n, SHARD_GATE, &sweep.all_stems);
        for lines in almide_corpus::read_partials(n, SHARD_GATE, "bridge") {
            for l in lines {
                let mut it = l.splitn(3, '\t');
                let name = it.next().expect("bridge partial row: name").to_string();
                let stem = it.next().expect("bridge partial row: fixture").to_string();
                let why = it.next().expect("bridge partial row: reason").to_string();
                // Stems sort as the corpus does (one directory, one
                // extension), so the smallest stem is the first fixture of
                // the unsharded sweep.
                let e = observed.entry(name).or_insert((stem.clone(), why.clone()));
                if stem < e.0 {
                    *e = (stem, why);
                }
            }
        }
        walked_n = sweep.all_stems.len();
        eprintln!("interp_bridge_fallback_ledger: merged {n} shard partial(s) — {} name(s)", observed.len());
    } else {
        for (stem, _, fallbacks) in &sweep.rows {
            calls += fallbacks.len();
            for (name, why) in fallbacks {
                observed
                    .entry(name.clone())
                    .or_insert((stem.clone(), why.replace('\n', " ")));
            }
        }
        walked_n = sweep.rows.len();
        if let Some(s) = slice {
            let rows: Vec<String> = observed
                .iter()
                .map(|(n, (stem, why))| format!("{n}\t{stem}\t{why}"))
                .collect();
            almide_corpus::write_partial(s, SHARD_GATE, "bridge", &rows);
        }
    }

    if std::env::var("ALMIDE_UPDATE_INTERP_LEDGER").is_ok() {
        sweep.refuse_update_under_shard();
        let mut out = String::from(
            "# interp-bridge-fallback-ledger — the `module.func` names the hand-mirrored\n\
             # bridge (crates/almide-interp/src/bridge.rs) answers over the spec/wasm_cross\n\
             # corpus (#2185): `floor:` rows are consumed INSIDE a self-hosted body (the\n\
             # bridge is that tier's floor, like prim.*); the others are boundary\n\
             # fallbacks where the body abstained or has no body. The body is the third\n\
             # vote; this is the bridge's measured residue. An arm absent here is dead.\n\
             #\n\
             # Format: <module.func>  <first fixture>  <floor: … | why the body abstained>\n\
             # Gate:   wasm_runtime_interp_ledger.rs::interp_bridge_fallback_ledger —\n\
             #         fails on a name missing here AND on a ledgered name the corpus no\n\
             #         longer reaches through the bridge (a shadowed arm: delete it), AND\n\
             #         on a recorded reason that differs from the observed reason.\n\
             # Regenerate (then review the diff!):\n\
             #   ALMIDE_UPDATE_INTERP_LEDGER=1 cargo test --test wasm_runtime_interp_ledger interp_bridge_fallback_ledger\n\n",
        );
        for (name, (stem, why)) in &observed {
            out.push_str(&format!("{name}  {stem}  {why}\n"));
        }
        std::fs::write(fallback_ledger_path(), out).unwrap();
        eprintln!(
            "interp_bridge_fallback_ledger: regenerated with {} name(s) ({} calls) — review the diff",
            observed.len(),
            calls
        );
        return String::new();
    }

    let ledger_text = std::fs::read_to_string(fallback_ledger_path()).unwrap_or_else(|_| {
        panic!(
            "interp-bridge-fallback-ledger.txt missing at {} — seed it with \
             ALMIDE_UPDATE_INTERP_LEDGER=1 cargo test --test wasm_runtime_interp_ledger interp_bridge_fallback_ledger",
            fallback_ledger_path().display()
        )
    });
    // name → recorded reason, held equal to the observed one for the same
    // reason the abstain ledger above holds its own (#2333).
    let recorded = parse_reason_ledger(&ledger_text, true);
    let ledger: std::collections::BTreeSet<String> = recorded.keys().cloned().collect();

    let unledgered: Vec<(&String, &(String, String))> = observed
        .iter()
        .filter(|(n, _)| !ledger.contains(*n))
        .collect();
    // A slice cannot judge `stale` or a reason drift: the name's first
    // fixture may be in another shard. Both are judged by `merge/N`.
    let stale: Vec<&String> = if slice.is_some() {
        Vec::new()
    } else {
        ledger.iter().filter(|n| !observed.contains_key(*n)).collect()
    };

    eprintln!(
        "\ninterp_bridge_fallback_ledger: {} bridge name(s) still answer as the body's fallback ({} calls over {} fixtures){}",
        observed.len(),
        calls,
        walked_n,
        match slice {
            Some(almide_corpus::CorpusShard::Slice { k, n }) => format!(" — shard {k}/{n}: new names judged here, stale/drift in the merge"),
            _ => String::new(),
        }
    );

    let mut failures = String::new();
    if !unledgered.is_empty() {
        failures.push_str(&format!(
            "\nUNLEDGERED BRIDGE FALLBACK(S) — {} name(s) the bridge answered for that \
             interp-bridge-fallback-ledger.txt does not record:\n",
            unledgered.len()
        ));
        for (n, (stem, why)) in &unledgered {
            failures.push_str(&format!("    - {n} (first in {stem}): {why}\n"));
        }
        failures.push_str(
            "  Preferred fix: widen the prim floor so the self-hosted body evaluates \
             (dispatch_module.rs / heap.rs — see crates/almide-interp/CLAUDE.md).\n  \
             Otherwise: record the fallback IN THIS SAME PR (ALMIDE_UPDATE_INTERP_LEDGER=1 \
             regenerates) — a third vote taken from a Rust copy is a reviewed decision.\n",
        );
    }
    if !stale.is_empty() {
        failures.push_str(&format!(
            "\nSTALE LEDGER ENTRY(IES) — {} ledgered name(s) the corpus no longer reaches \
             through the bridge (the body evaluates now, or the fixture is gone):\n",
            stale.len()
        ));
        for n in &stale {
            failures.push_str(&format!("    - {n}\n"));
        }
        failures.push_str(
            "  Delete the shadowed arm from bridge*.rs and the entry \
             (ALMIDE_UPDATE_INTERP_LEDGER=1 regenerates) — the ledger may only shrink.\n",
        );
    }
    let drifted: Vec<(&String, &String, &String)> = observed
        .iter()
        .filter(|_| slice.is_none())
        .filter_map(|(n, (_, why))| {
            recorded
                .get(n)
                .filter(|rec| *rec != why)
                .map(|rec| (n, rec, why))
        })
        .collect();
    if !drifted.is_empty() {
        failures.push_str(&format!(
            "\nDRIFTED REASON(S) — {} ledgered name(s) the bridge still answers for, but \
             for a reason the ledger does not record:\n",
            drifted.len()
        ));
        for (n, rec, obs) in &drifted {
            failures.push_str(&format!(
                "    - {n}\n        recorded: {rec}\n        observed: {obs}\n"
            ));
        }
        failures.push_str(
            "  The reason says WHY the body did not vote — the whole point of the row. \
             Re-record it in this same PR (ALMIDE_UPDATE_INTERP_LEDGER=1).\n",
        );
    }
    failures
}

/// Malformed or duplicate rows must not disappear from the reason comparison.
fn parse_reason_ledger(
    text: &str,
    has_fixture: bool,
) -> std::collections::BTreeMap<String, String> {
    let mut recorded = std::collections::BTreeMap::new();
    for line in text
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty() && !line.starts_with('#'))
    {
        let (name, rest) = line
            .split_once("  ")
            .expect("ledger row needs a name and reason");
        let reason = if has_fixture {
            let (fixture, reason) = rest
                .trim()
                .split_once("  ")
                .expect("bridge ledger row needs a witness fixture and reason");
            assert!(
                !fixture.trim().is_empty(),
                "bridge witness must not be empty"
            );
            reason.trim()
        } else {
            rest.trim()
        };
        assert!(!reason.is_empty(), "ledger reason must not be empty");
        assert!(
            recorded
                .insert(name.to_string(), reason.to_string())
                .is_none(),
            "duplicate ledger name: {name}"
        );
    }
    recorded
}

#[test]
fn malformed_reason_records_fail_closed() {
    for (text, has_fixture) in [
        ("name", false),
        ("name  ", false),
        ("name  reason\nname  different reason", false),
        ("module.fn  witness", true),
        ("module.fn  witness  ", true),
        ("module.fn  witness  reason\nmodule.fn  other  reason", true),
    ] {
        assert!(
            std::panic::catch_unwind(|| parse_reason_ledger(text, has_fixture)).is_err(),
            "malformed row was accepted: {text:?}"
        );
    }
    for (text, has_fixture) in [
        ("# comment\nname  observed reason\n", false),
        ("name  witness  observed reason\n", true),
    ] {
        assert_eq!(
            parse_reason_ledger(text, has_fixture)["name"],
            "observed reason"
        );
    }
}

/// #2359: a reason that names a declared type must name WHOSE declaration it
/// is. `Set[Int]` in one of these rows is the parameter of the impl the interp
/// resolved — a public container fn resolves to its scalar core by name — not
/// the program's type. The older wording said only "under the declared element
/// type Int", and two readers independently took it as a claim about the
/// fixture and went looking for a miscompile. A diagnostic that names a type
/// without naming what the type belongs to lets the reader supply the owner
/// from whatever they were already thinking about, so it does not merely fail
/// to help: it steers.
///
/// The ledger is an audited list whose purpose is that an honest limit can be
/// told apart from a hidden defect by reading it. This keeps that readable.
#[test]
fn a_reason_naming_a_declared_type_says_whose_declaration_it_is() {
    let text = std::fs::read_to_string(ledger_path()).expect("read abstain ledger");
    let mut offenders = Vec::new();
    for line in text.lines() {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        let Some((stem, reason)) = line.split_once("  ") else {
            continue;
        };
        if !reason.contains("declare") {
            continue;
        }
        // The owner must be named in the same row. "RESOLVED" is the marker the
        // reasons use; "no declared element type" is the un-hinted path, which
        // asserts the ABSENCE of a declaration and so has no owner to name.
        let names_owner = reason.contains("interp RESOLVED") || reason.contains("no declared");
        if !names_owner {
            offenders.push(format!("{stem}: {reason}"));
        }
    }
    assert!(
        offenders.is_empty(),
        "abstain reason(s) name a declared type without saying whose declaration it is:\n  {}\n\n\
         Say which body the declaration belongs to. A bare \"the declared element type Int\" on a \
         Set[String] fixture reads as a miscompiled program; it is the scalar core impl the interp \
         resolved by name, because the _str/_skv/_hval variant choice is a MIR pass it does not run.",
        offenders.join("\n  ")
    );
}
