//! Structural-witness floor (#1696 step 4 phase A): sweep the run-manifest
//! corpus with the witness sink armed, and hold three facts:
//!
//!   1. NO certificate is the `!poison` sentinel — the straightline gate
//!      and the recorder hooks agree on every admitted body (a poison
//!      means an RC event fired that the hooks could not attribute:
//!      a gate bug, loudly);
//!   2. every collected certificate is ACCEPTED by the portable checker
//!      (almide-verify, held to the extracted kernel-proven checker in
//!      proofs/gate.sh): per object and per path, no release at rc 0, no
//!      leak (#2756 — branch frames made the flat mirror too narrow);
//!   3. the witnessed COVERAGE never shrinks (re-keyed for #4328). Coverage
//!      is read from the WHOLE-MODULE leg: the same rows swept again in a
//!      child process with `ALMIDE_KEEP_BUNDLED_FNS=1`, so every bundled
//!      module a program loads is compiled whole, as before #4328's reach
//!      pruning — dead-code trimming in what ships cannot erase proof
//!      coverage of the stdlib itself. Its frames are judged like the shipped
//!      compile's (poison, checker, pass agreement, call modes). A frame WITH
//!      RC events counts once per IDENTITY: a program's own fn as
//!      `fixture :: fn`, a linked stdlib body by its name alone (`list.join`;
//!      the per-program `__selfhost_N` / `__almd_monoN_` counters dropped),
//!      since it is the same code whichever program links it.
//!      golden/witness-floor.txt is the count of distinct identities,
//!      grow-only; golden/witness-stdlib.txt lists the stdlib identities and
//!      every row must stay witnessed — a stdlib fn no program witnesses any
//!      more fails here by name. (The count used to be fixture x fn: one
//!      stdlib body linked into 40 fixtures counted 40 times, so #4328's
//!      pruning read as a 51k-frame loss that was duplicates and dead code.)
//!      Both re-anchor with ALMIDE_UPDATE_WITNESS_FLOOR=1. A scalar-only body
//!      yields an EMPTY certificate (true, and vacuous) — admitted but not
//!      coverage, so the floor measures RC coverage.
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

fn stdlib_path() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/golden/witness-stdlib.txt")
}

/// The switch that keeps every fn of a loaded bundled module (#4328).
const KEEP_BUNDLED: &str = "ALMIDE_KEEP_BUNDLED_FNS";

/// The corpus rows this run sweeps (the CI slice applies; the child process
/// inherits its environment and so selects the same rows).
fn selected_rows(root: &std::path::Path) -> Vec<String> {
    let manifest = std::fs::read_to_string(root.join("crates/almide-spine/tests/golden/spec-run-manifest.txt"))
        .expect("run manifest");
    let rows: Vec<String> = almide_corpus::manifest_rows(&manifest)
        .map(|line| line.splitn(3, '\t').nth(2).expect("manifest row").to_string())
        .collect();
    almide_corpus::select(rows, "witness_floor", |rel| rel.clone())
}

/// The whole-module leg's tag in front of each of its lines, so libtest's
/// own output is never mistaken for one.
const LEG_TAG: &str = "witness-whole\t";

fn escape(s: &str) -> String {
    s.replace('\\', "\\\\").replace('\n', "\\n").replace('\t', "\\t")
}

fn unescape(s: &str) -> String {
    let mut out = String::new();
    let mut chars = s.chars();
    while let Some(c) = chars.next() {
        if c != '\\' {
            out.push(c);
            continue;
        }
        match chars.next() {
            Some('n') => out.push('\n'),
            Some('t') => out.push('\t'),
            Some(o) => out.push(o),
            None => {}
        }
    }
    out
}

/// The whole-module leg's child entry: with `KEEP_BUNDLED` set, sweep the
/// same rows with every bundled module a program loads compiled WHOLE (the
/// compile before #4328's reach pruning), and print every frame and call-mode
/// witness. Without the switch, nothing — so a plain `-- --ignored` run
/// passes it by.
#[test]
#[ignore = "the whole-module leg's child entry: structural_witnesses_balance_and_hold_the_floor spawns it"]
fn whole_module_leg_child() {
    if !almide_base::env::flag(KEEP_BUNDLED) {
        return;
    }
    let root = workspace_root();
    for rel in selected_rows(&root) {
        let text = std::fs::read_to_string(almide_corpus::resolve(&root, &rel)).expect("fixture readable");
        let Ok(ir) = almide_spine::s5::lower_to_ir(&rel, &text) else {
            println!("{LEG_TAG}unlowered\t{rel}");
            continue;
        };
        almide_wasm::witness::start_collecting();
        let _ = almide_wasm::emit_program(&ir);
        let (frames, _) = almide_wasm::witness::take_by_pass_with_context();
        println!("{LEG_TAG}program\t{rel}");
        for (pass, name, cert, ctx) in frames {
            println!("{LEG_TAG}frame\t{pass}\t{}\t{}\t{}", escape(&name), escape(&cert), escape(&ctx));
        }
        for (pass, w) in almide_wasm::witness::take_modes() {
            println!("{LEG_TAG}mode\t{pass}\t{}", escape(&w));
        }
    }
}

/// One program of the whole-module leg: its row, frames and call-mode witnesses.
type WholeProgram = (String, Vec<almide_wasm::witness::PassFrame>, Vec<(usize, String)>);

/// Start the whole-module leg: a fresh process of this binary with the
/// switch set (the resolver reads it from the environment), running while
/// this process sweeps the shipped compile.
fn spawn_whole_module_leg() -> std::process::Child {
    std::process::Command::new(std::env::current_exe().expect("the test binary's own path"))
        .args(["whole_module_leg_child", "--exact", "--ignored", "--nocapture", "--test-threads=1"])
        .env(KEEP_BUNDLED, "1")
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .spawn()
        .expect("spawn the whole-module leg")
}

/// The whole-module leg's programs, and the rows it could not lower.
fn read_whole_module_leg(child: std::process::Child) -> (Vec<WholeProgram>, Vec<String>) {
    let out = child.wait_with_output().expect("the whole-module leg");
    assert!(
        out.status.success(),
        "the whole-module leg's child exited {}:\n{}",
        out.status,
        String::from_utf8_lossy(&out.stderr)
    );
    let stdout = String::from_utf8_lossy(&out.stdout);
    let mut programs: Vec<WholeProgram> = Vec::new();
    let mut unlowered = Vec::new();
    for line in stdout.lines().filter_map(|l| l.split_once(LEG_TAG).map(|(_, rest)| rest)) {
        let f: Vec<&str> = line.split('\t').collect();
        let last = |p: &mut Vec<WholeProgram>| p.len().checked_sub(1).expect("a program line first");
        match f.as_slice() {
            ["unlowered", rel] => unlowered.push(rel.to_string()),
            ["program", rel] => programs.push((rel.to_string(), Vec::new(), Vec::new())),
            ["frame", pass, name, cert, ctx] => {
                let i = last(&mut programs);
                programs[i].1.push((pass.parse().expect("pass"), unescape(name), unescape(cert), unescape(ctx)));
            }
            ["mode", pass, w] => {
                let i = last(&mut programs);
                programs[i].2.push((pass.parse().expect("pass"), unescape(w)));
            }
            other => panic!("unreadable whole-module leg line: {other:?}"),
        }
    }
    (programs, unlowered)
}

/// The key prefix of a whole-module leg frame: its certificates are judged
/// like every other, but never compared with the shipped compile's (the two
/// compiles hold different fn sets, so different calling conventions).
const WHOLE: &str = "[whole] ";

/// Drop the decimal counter that follows each `marker` in `s`.
fn drop_counters(s: &str, marker: &str) -> String {
    let mut out = String::new();
    let mut rest = s;
    while let Some(i) = rest.find(marker) {
        out.push_str(&rest[..i + marker.len()]);
        rest = rest[i + marker.len()..].trim_start_matches(|c: char| c.is_ascii_digit());
    }
    out.push_str(rest);
    out
}

/// A witnessed frame's coverage identity (`rel :: name` key): a linked
/// stdlib body is the same code whichever program links it, so it is named
/// alone, without the per-program counters; anything else stays per fixture.
/// `None` for a program's own fn, `Some(name)` for a stdlib body.
fn stdlib_identity(name: &str) -> Option<String> {
    let module = name.split_once('.')?.0;
    let selfhost = module.strip_prefix("__selfhost_").is_some_and(|n| !n.is_empty() && n.bytes().all(|b| b.is_ascii_digit()));
    if !selfhost && !almide_types::stdlib_info::BUNDLED_MODULES.contains(&module) {
        return None;
    }
    Some(drop_counters(&drop_counters(name, "__selfhost_"), "__almd_mono"))
}

/// A whole-module leg key's coverage identity; `None` for a shipped-compile key.
fn coverage_identity(key: &str) -> Option<String> {
    let key = key.strip_prefix(WHOLE)?;
    let name = key.split_once(" :: ").map_or(key, |(_, n)| n);
    Some(stdlib_identity(name).unwrap_or_else(|| key.to_string()))
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
    // Under the CI slice the manifest seen is 1 in k rows: "left the
    // manifest" and the decline histogram are whole-corpus facts, judged on
    // the full run; a newly certified or a lost fixture is judged per fixture.
    let full = almide_corpus::full_run();
    let gone: Vec<&String> = if full { listed.iter().filter(|f| !pf.manifest.contains(*f)).collect() } else { Vec::new() };
    assert!(
        gained.is_empty() && gone.is_empty(),
        "golden/witness-fixtures.txt is behind: {} newly certified fixture(s), {} listed fixture(s) left the manifest \
         — re-anchor with ALMIDE_UPDATE_WITNESS_FLOOR=1:\n{}\n{}",
        gained.len(),
        gone.len(),
        bullets("+", gained.iter().copied()),
        bullets("-", gone.iter().copied())
    );
    if !full {
        eprintln!("[witness-floor] CI slice: the decline histogram and the floor are judged on the full run");
        return;
    }
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

/// #2758: judge every emission pass's call-mode witness of the program just
/// emitted; returns how many were judged, pushing each rejection.
fn judge_call_modes(rel: &str, passes: Vec<(usize, String)>, rejected: &mut Vec<(String, String)>) -> usize {
    let n = passes.len();
    for (pass, w) in passes {
        if !almide_verify::check(almide_verify::Property::CallModes, w.as_bytes()) {
            rejected.push((format!("{rel} [pass {pass}]"), w));
        }
    }
    n
}

/// A frame's sweep key. The checked pass (no bounded-line rewrites) emits
/// different code by design: it agrees with itself, not with passes 1-2.
fn frame_key(rel: &str, pass: usize, name: &str) -> String {
    if pass == almide_wasm::witness::CHECKED_PASS {
        format!("{rel} :: {name} [checked]")
    } else {
        format!("{rel} :: {name}")
    }
}

/// Was this frame compiled under the same calling conventions as the first
/// pass that emitted it? Two emission passes compile a fn under different
/// conventions when their fn sets differ (param_borrow.rs's fixpoint over the
/// whole linked graph vs the reachable set) and then legitimately emit
/// different code: such a certificate is compared with nothing (the shipped
/// pass's is the one a fixture reads). It is still judged on its own.
fn same_convention(contexts: &mut Map<String>, key: &str, ctx: String) -> bool {
    match contexts.get(key) {
        Some(prev) => *prev == ctx,
        None => {
            contexts.insert(key.to_string(), ctx);
            true
        }
    }
}

#[cfg_attr(debug_assertions, ignore = "corpus sweep is release-only (CI: release-shape job)")]
#[test]
fn structural_witnesses_balance_and_hold_the_floor() {
    let root = workspace_root();
    let whole_leg = spawn_whole_module_leg();

    let mut certs: std::collections::BTreeMap<String, String> = std::collections::BTreeMap::new();
    // Each frame's calling-convention context (`same_convention`).
    let mut contexts: Map<String> = Map::new();
    // The step-4 measurement channel: every frame the gate turned away,
    // with its reason — dumped as `!decline:<reason> <key>` lines under
    // ALMIDE_WITNESS_DUMP so `grep -o '^!decline:[^ ]*' | sort | uniq -c`
    // is the histogram that picks the next shape to admit.
    let mut declined: std::collections::BTreeMap<String, String> = std::collections::BTreeMap::new();
    let mut poisoned: Vec<String> = Vec::new();
    let mut unbalanced: Vec<(String, String)> = Vec::new();
    let mut nondet: Vec<String> = Vec::new();
    // #2758: the program-level call-mode witness of every pass.
    let mut modes_rejected: Vec<(String, String)> = Vec::new();
    let mut modes_checked = 0usize;
    let dump = std::env::var("ALMIDE_WITNESS_DUMP").is_ok();
    let mut per_fixture = PerFixture::default();
    let rows = selected_rows(&root);
    let mut lowered = Set::new();
    // Every emitted frame and call-mode witness of one program, judged.
    let mut absorb = |rel: &str, frames: Vec<almide_wasm::witness::PassFrame>, modes: Vec<(usize, String)>| {
        modes_checked += judge_call_modes(rel, modes, &mut modes_rejected);
        for (pass, name, cert, ctx) in frames {
            let key = frame_key(rel, pass, &name);
            let same_code = same_convention(&mut contexts, &key, ctx);
            if let Some(reason) = cert.strip_prefix(almide_wasm::witness::DECLINE_PREFIX) {
                if same_code {
                    declined.insert(key, reason.trim().to_string());
                }
            } else if cert.starts_with('!') {
                poisoned.push(key);
            // #2756: the certificate carries branch frames (`{…|…}`, the
            // v5 exit `x`) — judged by the portable checker itself, not a
            // flat-alphabet mirror that would reject every branch.
            } else if !almide_verify::check(almide_verify::Property::Ownership, cert.as_bytes()) {
                unbalanced.push((key, cert));
            } else {
                if dump {
                    eprintln!("[witness] {key}\n{cert}");
                }
                // emit_program lowers every fn once per emission pass
                // (the reachability two-pass) — the passes of one code
                // configuration must agree.
                if same_code
                    && let Some(prev) = certs.insert(key.clone(), cert.clone())
                    && prev != cert
                {
                    nondet.push(key);
                }
            }
        }
    };
    for rel in &rows {
        let rel = rel.as_str();
        per_fixture.note(rel);
        let text = std::fs::read_to_string(almide_corpus::resolve(&root, rel)).expect("fixture readable");
        let Ok(ir) = almide_spine::s5::lower_to_ir(rel, &text) else { continue };
        lowered.insert(rel.to_string());
        almide_wasm::witness::start_collecting();
        let emitted = almide_wasm::emit_program(&ir).is_ok();
        let (frames, shipped) = almide_wasm::witness::take_by_pass_with_context();
        per_fixture.record(rel, emitted, &shipped);
        absorb(rel, frames, almide_wasm::witness::take_modes());
    }
    // The whole-module leg: the same rows, every loaded bundled module whole.
    let (whole_programs, whole_unlowered) = read_whole_module_leg(whole_leg);
    let whole_rows: Set = whole_programs.iter().map(|(rel, _, _)| rel.clone()).collect();
    for (rel, frames, modes) in whole_programs {
        absorb(&format!("{WHOLE}{rel}"), frames, modes);
    }
    // A row the shipped compile lowers but the whole-module compile does not
    // would take its coverage out of the count unseen.
    let whole_missing: Vec<&String> = lowered.iter().filter(|r| !whole_rows.contains(*r)).collect();
    assert!(
        whole_missing.is_empty(),
        "the whole-module leg did not lower {} row(s) the shipped compile lowers ({} unlowered there): {whole_missing:?}",
        whole_missing.len(),
        whole_unlowered.len()
    );
    // The counts read the bounded configuration only (a checked-pass frame
    // is the same fn re-emitted, not more coverage); `admitted` and the
    // decline count read the shipped compile, the coverage the whole one.
    let bounded = |k: &String| !k.ends_with(" [checked]");
    let shipped_compile = |k: &String| bounded(k) && !k.starts_with(WHOLE);
    let admitted = certs.keys().filter(|k| shipped_compile(k)).count();
    let coverage: Set = certs
        .iter()
        .filter(|(k, c)| bounded(k) && !c.trim().is_empty())
        .filter_map(|(k, _)| coverage_identity(k))
        .collect();
    let stdlib_witnessed: Set = coverage.iter().filter(|i| !i.contains(" :: ")).cloned().collect();
    let witnessed = coverage.len();
    // A frame both passes admit AND decline is a pass disagreement too.
    for key in declined.keys().filter(|k| certs.contains_key(*k)) {
        nondet.push(key.clone());
    }
    if dump {
        coverage.iter().for_each(|id| eprintln!("[coverage] {id}"));
        for (key, reason) in &declined {
            eprintln!("{}{reason} {key}", almide_wasm::witness::DECLINE_PREFIX);
        }
    }
    eprintln!(
        "[witness-floor] admitted {admitted} function(s), {witnessed} distinct with RC events ({} stdlib), {} declined; \
         {} of {} spec/wasm_cross fixture(s) fully certified; {modes_checked} call-mode witness(es), {} rejected",
        stdlib_witnessed.len(),
        declined.keys().filter(|k| shipped_compile(k)).count(),
        per_fixture.certified().len(),
        per_fixture.manifest.len(),
        modes_rejected.len()
    );
    assert!(
        modes_rejected.is_empty(),
        "{} call-mode witness(es) rejected — a site hands an argument over unlike its callee's frame assumes:\n{}",
        modes_rejected.len(),
        modes_rejected.iter().map(|(k, w)| format!("{k}: {w}")).collect::<Vec<_>>().join("\n")
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
        "{} certificate(s) rejected by the portable checker:\n{:?}",
        unbalanced.len(),
        unbalanced
    );

    // #2754: the per-fixture set and the decline histogram.
    let update = std::env::var("ALMIDE_UPDATE_WITNESS_FLOOR").is_ok();
    if update {
        almide_corpus::refuse_regen("witness_floor");
    }
    per_fixture_ratchets(&per_fixture, update);
    if !almide_corpus::full_run() {
        return;
    }
    let fp = floor_path();
    let sp = stdlib_path();
    if update {
        std::fs::write(&fp, format!("{witnessed}\n")).expect("write floor");
        let mut out = String::from(
            "# Stdlib fns whose body carries a certificate with RC events when the corpus is compiled with every\n\
             # loaded bundled module whole (ALMIDE_KEEP_BUNDLED_FNS=1). Every row must stay witnessed; re-anchor with\n\
             # ALMIDE_UPDATE_WITNESS_FLOOR=1 cargo test --release -p almide-wasm --test witness_floor\n",
        );
        stdlib_witnessed.iter().for_each(|f| out.push_str(&format!("{f}\n")));
        std::fs::write(&sp, out).expect("write stdlib witnesses");
        return;
    }
    let listed_text = std::fs::read_to_string(&sp).expect("golden/witness-stdlib.txt — generate with ALMIDE_UPDATE_WITNESS_FLOOR=1");
    let lost: Vec<&str> = golden_rows(&listed_text).filter(|f| !stdlib_witnessed.contains(*f)).collect();
    assert!(
        lost.is_empty(),
        "{} stdlib fn(s) are witnessed NOWHERE any more — no corpus program, compiled with its bundled modules \
         whole, emits a certificate with RC events for them; restore the recording, or ratify a deleted fn \
         deliberately with ALMIDE_UPDATE_WITNESS_FLOOR=1:\n{}",
        lost.len(),
        lost.iter().map(|f| format!("  - {f}")).collect::<Vec<_>>().join("\n")
    );
    let floor: usize = std::fs::read_to_string(&fp)
        .expect("golden/witness-floor.txt — generate with ALMIDE_UPDATE_WITNESS_FLOOR=1")
        .trim()
        .parse()
        .expect("floor number");
    assert!(
        witnessed >= floor,
        "witnessed {witnessed} distinct function(s) < floor {floor} — recording coverage shrank; \
         restore it or ratify the shrink deliberately"
    );
}
