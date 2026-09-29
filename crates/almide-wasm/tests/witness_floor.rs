//! Structural-witness floor (#1696 step 4 phase A): sweep the run-manifest
//! corpus with the witness sink armed, and hold three facts:
//!
//!   1. NO certificate is the `!poison` sentinel — the straightline gate
//!      and the recorder hooks agree on every admitted body (a poison
//!      means an RC event fired that the hooks could not attribute:
//!      a gate bug, loudly);
//!   2. every collected certificate BALANCES under the mirror of the
//!      proven rule (per object: no release at rc 0, no leak);
//!   3. the count of witnessed functions WITH RC EVENTS never shrinks —
//!      golden/witness-floor.txt, grow-only, ratified with
//!      ALMIDE_UPDATE_WITNESS_FLOOR=1 (phases B/C admit more shapes and
//!      raise it; a refactor that silently drops recording fails here).
//!      A scalar-only body yields an EMPTY certificate (true, and vacuous:
//!      2,767 of them after B1 admitted scalar tail calls) — counted as
//!      admitted but not as the floor, so the floor measures RC coverage.
//!   4. (step 4) a DECLINED frame (`!decline:<reason>`, the gate's or an
//!      emission-time withdrawal) is neither certificate nor poison: it is
//!      counted, and under ALMIDE_WITNESS_DUMP each is printed as
//!      `!decline:<reason> <fixture> :: <fn>` — the histogram
//!      (`grep -o '^!decline:[^ ]*' | sort | uniq -c | sort -rn`) names
//!      the next shape to admit.
//!
//!   5. (#2754) the PER-FIXTURE metric: a `spec/wasm_cross` fixture is
//!      CERTIFIED when its program emits on the structural leg and EVERY
//!      frame of the pass that SHIPS — user fns, reachable linked stdlib
//!      bodies, lifted lambdas,
//!      display / equality / scan helpers — carries a certificate the
//!      portable checker (`almide-verify`, held to the extracted
//!      kernel-proven checker's verdicts in proofs/gate.sh) ACCEPTS: no
//!      decline, no poison. golden/witness-fixtures.txt is that set,
//!      GROW-ONLY — the starting line of the step-4 issues (#2755–#2760);
//!      golden/witness-declines.txt is the decline histogram over the
//!      shipped pass (reason → declined frames, fixture × fn), SHRINK-ONLY
//!      per reason. Both are
//!      pinned exactly and re-anchored with ALMIDE_UPDATE_WITNESS_FLOOR=1,
//!      which refuses to drop a certified fixture still in the manifest.
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

fn fixtures_path() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/golden/witness-fixtures.txt")
}

fn declines_path() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/golden/witness-declines.txt")
}

/// The fixtures the per-fixture metric covers.
fn in_scope(rel: &str) -> bool {
    rel.starts_with("spec/wasm_cross/")
}

/// What one fixture's emission produced, for the per-fixture verdict.
#[derive(Default)]
struct FixtureTally {
    emitted: bool,
    frames: usize,
    uncertified: usize,
}

type Set = std::collections::BTreeSet<String>;
type Map<V> = std::collections::BTreeMap<String, V>;

/// The per-fixture measurement (#2754): what each in-scope fixture's
/// SHIPPED pass emitted, and the declined frames of that pass.
#[derive(Default)]
struct PerFixture {
    manifest: Set,
    tallies: Map<FixtureTally>,
    shipped_declined: Map<String>,
}

/// A frame is certified only if the PORTABLE checker accepts its
/// certificate — not the balance mirror; a `!` line never is.
fn frame_certified(cert: &str) -> bool {
    !cert.starts_with('!') && almide_verify::check(almide_verify::Property::Ownership, cert.as_bytes())
}

impl PerFixture {
    /// The per-fixture verdict reads the SHIPPED pass only: a dead linked
    /// body the first pass emitted is not in the artifact.
    /// Every in-scope manifest row counts, whether or not it lowers — a
    /// fixture the front stops lowering is LOST, not gone.
    fn note(&mut self, rel: &str) {
        if in_scope(rel) {
            self.manifest.insert(rel.to_string());
        }
    }

    fn record(&mut self, rel: &str, emitted: bool, shipped: &[(String, String)]) {
        if !in_scope(rel) {
            return;
        }
        let tally = self.tallies.entry(rel.to_string()).or_default();
        tally.emitted = emitted;
        tally.frames += shipped.len();
        tally.uncertified += shipped.iter().filter(|(_, c)| !frame_certified(c)).count();
        for (name, cert) in shipped {
            if let Some(reason) = cert.strip_prefix(almide_wasm::witness::DECLINE_PREFIX) {
                self.shipped_declined.insert(format!("{rel} :: {name}"), reason.trim().to_string());
            }
        }
    }

    fn certified(&self) -> Set {
        self.tallies
            .iter()
            .filter(|(_, t)| t.emitted && t.frames > 0 && t.uncertified == 0)
            .map(|(rel, _)| rel.clone())
            .collect()
    }

    fn histogram(&self) -> Map<usize> {
        let mut h = Map::new();
        for reason in self.shipped_declined.values() {
            *h.entry(reason.clone()).or_default() += 1;
        }
        h
    }
}

fn golden_rows(text: &str) -> impl Iterator<Item = &str> {
    text.lines().map(str::trim).filter(|l| !l.is_empty() && !l.starts_with('#'))
}

fn bullets<'a>(mark: &str, items: impl IntoIterator<Item = &'a String>) -> String {
    items.into_iter().map(|f| format!("  {mark} {f}")).collect::<Vec<_>>().join("\n")
}

fn write_goldens(pf: &PerFixture, certified: &Set, histogram: &Map<usize>) {
    let mut out = String::from(
        "# spec/wasm_cross fixtures whose EVERY frame in the shipped emission pass carries a\n\
         # certificate the portable checker (almide-verify) accepts (#2754). GROW-ONLY; re-anchor with\n\
         # ALMIDE_UPDATE_WITNESS_FLOOR=1 cargo test --release -p almide-wasm --test witness_floor\n",
    );
    out.push_str(&format!("# certified: {} of {}\n", certified.len(), pf.manifest.len()));
    certified.iter().for_each(|f| out.push_str(&format!("{f}\n")));
    std::fs::write(fixtures_path(), out).expect("write fixtures");
    let mut out = String::from(
        "# Structural witness decline histogram (#2754): reason<TAB>declined frames of the\n\
         # SHIPPED emission pass (fixture x fn, over the spec/wasm_cross rows of the run manifest). SHRINK-ONLY per reason; re-anchor with\n\
         # ALMIDE_UPDATE_WITNESS_FLOOR=1 cargo test --release -p almide-wasm --test witness_floor\n",
    );
    histogram.iter().for_each(|(reason, n)| out.push_str(&format!("{reason}\t{n}\n")));
    std::fs::write(declines_path(), out).expect("write declines");
}

/// Per-reason drift of the histogram against the pinned one:
/// `(grew, shrank)`, each as `reason: was -> now` lines.
fn histogram_drift(now: &Map<usize>, pinned: &Map<usize>) -> (Vec<String>, Vec<String>) {
    let reasons: std::collections::BTreeSet<&String> = now.keys().chain(pinned.keys()).collect();
    let row = |r: &String| (pinned.get(r).copied().unwrap_or(0), now.get(r).copied().unwrap_or(0));
    let line = |r: &String| format!("  {r}: {} -> {}", row(r).0, row(r).1);
    let grew = reasons.iter().filter(|r| row(r).1 > row(r).0).map(|r| line(r)).collect();
    let shrank = reasons.iter().filter(|r| row(r).1 < row(r).0).map(|r| line(r)).collect();
    (grew, shrank)
}

/// The two #2754 ratchets. Under `update` they are re-anchored — except
/// that a certified fixture still in the manifest may never be dropped.
fn per_fixture_ratchets(pf: &PerFixture, update: bool) {
    let certified = pf.certified();
    let histogram = pf.histogram();
    let listed_text = std::fs::read_to_string(fixtures_path()).unwrap_or_default();
    let listed: Set = golden_rows(&listed_text).map(str::to_string).collect();
    let lost: Vec<&String> = listed.iter().filter(|f| pf.manifest.contains(*f) && !certified.contains(*f)).collect();
    assert!(
        lost.is_empty(),
        "{} certified wasm_cross fixture(s) lost their certificate — golden/witness-fixtures.txt is grow-only \
         and the update switch cannot drop them:\n{}",
        lost.len(),
        bullets("-", lost.iter().copied())
    );
    if update {
        write_goldens(pf, &certified, &histogram);
        return;
    }
    let gained: Vec<&String> = certified.difference(&listed).collect();
    let gone: Vec<&String> = listed.iter().filter(|f| !pf.manifest.contains(*f)).collect();
    assert!(
        gained.is_empty() && gone.is_empty(),
        "golden/witness-fixtures.txt is behind: {} newly certified fixture(s), {} listed fixture(s) left the manifest \
         — re-anchor with ALMIDE_UPDATE_WITNESS_FLOOR=1:\n{}\n{}",
        gained.len(),
        gone.len(),
        bullets("+", gained.iter().copied()),
        bullets("-", gone.iter().copied())
    );
    let pinned_text = std::fs::read_to_string(declines_path())
        .expect("golden/witness-declines.txt — generate with ALMIDE_UPDATE_WITNESS_FLOOR=1");
    let pinned: Map<usize> = golden_rows(&pinned_text)
        .filter_map(|l| l.split_once('\t'))
        .map(|(r, n)| (r.to_string(), n.parse().expect("decline count")))
        .collect();
    let (grew, shrank) = histogram_drift(&histogram, &pinned);
    assert!(
        grew.is_empty(),
        "declined frames GREW for {} reason(s) (golden/witness-declines.txt is shrink-only per reason; a new \
         fixture's frames are ratified with ALMIDE_UPDATE_WITNESS_FLOOR=1):\n{}",
        grew.len(),
        grew.join("\n")
    );
    assert!(
        shrank.is_empty(),
        "declined frames shrank for {} reason(s) — ratchet golden/witness-declines.txt down with \
         ALMIDE_UPDATE_WITNESS_FLOOR=1:\n{}",
        shrank.len(),
        shrank.join("\n")
    );
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
    // The step-4 measurement channel: every frame the gate turned away,
    // with its reason — dumped as `!decline:<reason> <key>` lines under
    // ALMIDE_WITNESS_DUMP so `grep -o '^!decline:[^ ]*' | sort | uniq -c`
    // is the histogram that picks the next shape to admit.
    let mut declined: std::collections::BTreeMap<String, String> = std::collections::BTreeMap::new();
    let mut poisoned: Vec<String> = Vec::new();
    let mut unbalanced: Vec<(String, String)> = Vec::new();
    let mut nondet: Vec<String> = Vec::new();
    let dump = std::env::var("ALMIDE_WITNESS_DUMP").is_ok();
    let mut per_fixture = PerFixture::default();
    for line in almide_corpus::manifest_rows(&manifest) {
        let rel = line.splitn(3, '\t').nth(2).expect("manifest row");
        per_fixture.note(rel);
        let text = std::fs::read_to_string(almide_corpus::resolve(&root, rel)).expect("fixture readable");
        let Ok(ir) = almide_spine::s5::lower_to_ir(rel, &text) else { continue };
        almide_wasm::witness::start_collecting();
        let emitted = almide_wasm::emit_program(&ir).is_ok();
        let (frames, shipped) = almide_wasm::witness::take_by_pass();
        per_fixture.record(rel, emitted, &shipped);
        for (pass, name, cert) in frames {
            // The checked pass (no bounded-line rewrites) emits different
            // code by design: it agrees with itself, not with passes 1-2.
            let key = if pass == almide_wasm::witness::CHECKED_PASS {
                format!("{rel} :: {name} [checked]")
            } else {
                format!("{rel} :: {name}")
            };
            if let Some(reason) = cert.strip_prefix(almide_wasm::witness::DECLINE_PREFIX) {
                declined.insert(key, reason.trim().to_string());
            } else if cert.starts_with('!') {
                poisoned.push(key);
            } else if !almide_wasm::witness::balanced(&cert) {
                unbalanced.push((key, cert));
            } else {
                if dump {
                    eprintln!("[witness] {key}\n{cert}");
                }
                // emit_program lowers every fn once per emission pass
                // (the reachability two-pass) — the passes of one code
                // configuration must agree.
                if let Some(prev) = certs.insert(key.clone(), cert.clone())
                    && prev != cert
                {
                    nondet.push(key);
                }
            }
        }
    }
    // The counts read the bounded configuration only (a checked-pass frame
    // is the same fn re-emitted, not more coverage).
    let bounded = |k: &String| !k.ends_with(" [checked]");
    let admitted = certs.keys().filter(|k| bounded(k)).count();
    let witnessed = certs.iter().filter(|(k, c)| bounded(k) && !c.trim().is_empty()).count();
    // A frame both passes admit AND decline is a pass disagreement too.
    for key in declined.keys().filter(|k| certs.contains_key(*k)) {
        nondet.push(key.clone());
    }
    if dump {
        for (key, reason) in &declined {
            eprintln!("{}{reason} {key}", almide_wasm::witness::DECLINE_PREFIX);
        }
    }
    eprintln!(
        "[witness-floor] admitted {admitted} function(s), {witnessed} with RC events, {} declined; \
         {} of {} spec/wasm_cross fixture(s) fully certified",
        declined.keys().filter(|k| bounded(k)).count(),
        per_fixture.certified().len(),
        per_fixture.manifest.len()
    );
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

    // #2754: the per-fixture set and the decline histogram.
    let update = std::env::var("ALMIDE_UPDATE_WITNESS_FLOOR").is_ok();
    per_fixture_ratchets(&per_fixture, update);
    let fp = floor_path();
    if update {
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
