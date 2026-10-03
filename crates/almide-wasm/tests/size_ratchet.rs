//! Module-size RATCHET (#1585) — the corpus-wide size story (2026-08-25
//! A/B: smaller on 450/599, median ratio 0.675, aggregate 4.11 MB) was
//! a one-shot snapshot; this gate makes it non-regressable. Every
//! manifest fixture's emitted byte size is pinned EXACTLY in
//! golden/size-baseline.txt, like the allocation ledger pins its
//! watermarks: any move, up or down, is red until the change that makes
//! it CLAIMS it by regenerating:
//!
//!   ALMIDE_UPDATE_SIZES=1 cargo test --release -p almide-wasm --test size_ratchet
//!
//! The regenerated diff makes a change's size impact visible in review.
//! Growth past the per-fixture allowance is named a REGRESSION (fix it
//! first). The pin is exact because a tolerance let rows go stale (#2309):
//! 26 shipped rows had drifted inside their caps, merged unclaimed, and
//! the next change that regenerated inherited them as its own diff.
//! Two roc-style broken-measurement guards keep the gate honest: a
//! module under 100 bytes, or a total collapsing under half the
//! baseline, reads as INSTRUMENTATION FAILURE, never as a win.
//!
//! Two ledgers, same rows, same caps (#1859): `size-baseline.txt` pins
//! `emit_program`'s bytes — the embedded-host module — and
//! `size-baseline-wasi.txt` pins the SHIPPED form, the same module after
//! `to_wasi` (the stock-runtime p1 command `almide build --target wasm`
//! writes). The transform adds the WASI imports and shims, and it is the
//! layer #1841 regressed by 1,033 B on every env-free module while the
//! first ledger did not move by a byte: only the shipped bytes see a
//! transform-level regression, and only a corpus-wide ledger sees it on
//! every program rather than on Hello, world alone.
//!
//! ISOLATION (#2309): the corpus pass compiles every fixture in ONE
//! process, and `almide build` compiles one program per process. A
//! process-global cache in the front or the emitter keyed too loosely
//! (a linked helper, a self-host body, an interner-ordered table) would
//! make a row depend on the fixtures compiled before it, so the ledger
//! would pin a module nobody ships. The ratchet therefore also measures
//! every fixture ALONE — this binary re-run once per fixture, in a fresh
//! process, through the `one_fixture_measured_alone` child entry — and
//! refuses any fixture whose two forms are not byte-identical both ways.
//!
//! DETERMINISM (#3143): the emitter once kept per-node facts keyed by a
//! node's memory ADDRESS, and a dropped temporary's address handed its
//! fact to the next node allocated there — so whether a build was right
//! depended on what the allocator happened to reuse (7 builds in 41 of one
//! program freed a list a binding still held, #3139). One fresh process
//! per fixture sees ONE heap history, which is why this check had passed
//! over that bug. Each fixture is therefore built alone [`PRESSURES`]
//! times, each child under a different allocation pressure installed by
//! this binary's own `#[global_allocator]` (almide_base::alloc_pressure —
//! nothing in the compiler installs it): `plain` (the system allocator as
//! is), `quarantine` (every free held behind a ring of 256 before it is
//! really freed, so an address is not handed straight back — the node
//! rebuilt right after its dropped twin lands elsewhere) and `ballast` (a
//! pseudo-random-size block held on every fifth allocation, so size
//! classes fill in another order). Every child's emitted AND shipped
//! digests must equal the
//! corpus pass's; a second hash fails the gate naming the fixture and the
//! pressure. COVERAGE, stated honestly: every run-manifest fixture, four
//! builds each (the corpus pass plus three children), on the one host and
//! allocator CI runs — a hazard whose firing needs an address coincidence
//! none of the four heap histories produces still passes. The structural
//! fix (`node_marks.rs`: no node the emitter marks can be freed while the
//! marks live) is what closes the class; this gate is the backstop that
//! shows it stayed closed.

use sha2::{Digest, Sha256};
use std::path::{Path, PathBuf};
use std::process::Command;

use almide_base::alloc_pressure::{Pressure, set_pressure};

/// Per-fixture regression allowance: growth past this factor plus slack
/// is named a regression rather than a move to claim (helper dedupe
/// shifts are real; silent 2x growth is not).
const PER_FIXTURE_FACTOR: f64 = 1.25;
const PER_FIXTURE_SLACK: u64 = 512;

/// The switch that turns this binary into the isolation check's child: the
/// ONE fixture a fresh process measures. Set by the ratchet itself.
const ALONE: &str = "ALMIDE_SIZE_ALONE";
/// The child entry the ratchet spawns this binary with.
const CHILD: &str = "one_fixture_measured_alone";
/// The tag in front of the child's one result, so libtest's own output is
/// never mistaken for it.
const CHILD_TAG: &str = "size-alone\t";
/// The switch naming the allocation pressure a child builds under (#3143).
const PRESSURE: &str = "ALMIDE_SIZE_PRESSURE";
/// The pressures every fixture is built alone under — see the header.
const PRESSURES: [Pressure; 3] = Pressure::ALL;

#[global_allocator]
static ALLOC: almide_base::alloc_pressure::PressureAlloc = almide_base::alloc_pressure::PressureAlloc;

fn workspace_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..").canonicalize().expect("test harness invariant")
}

fn baseline_path(name: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/golden").join(name)
}

/// One fixture measured: both forms' sizes (`None` = the structural leg
/// refuses it, a `!` row) and the fingerprint the isolation check compares
/// — each form's size AND digest, so a same-size different module counts.
struct Fixture {
    rel: String,
    sizes: Option<(u64, u64)>,
    print: String,
}

/// Measure one fixture: the emitted module and its `to_wasi` twin. A
/// refusal is a `!` row in both ledgers; a module that emits but fails the
/// transform is an Almide bug, not a row.
fn measure_one(root: &Path, rel: &str) -> Fixture {
    let text = std::fs::read_to_string(almide_corpus::resolve(root, rel)).expect("fixture readable");
    let ir = almide_spine::s5::lower_to_ir(rel, &text).expect("front (manifest fixtures all lower)");
    // `!` row: the structural leg REFUSES this fixture (CLI reroutes
    // to the incumbent — #1688's unfoldable shapes). No size to pin;
    // the alloc ledger asserts the refusal stays a refusal.
    let Ok((bytes, host_ops)) = almide_wasm::emit_program_with_ops(&ir) else {
        return Fixture { rel: rel.to_string(), sizes: None, print: "!".to_string() };
    };
    let n = bytes.len() as u64;
    assert!(n >= 100, "{rel}: {n} bytes — too small to be a real module, measurement broken");
    let host_ops: Vec<i32> = host_ops.into_iter().collect();
    let wasi = almide_wasm_run::wasi::to_wasi(&bytes, &host_ops)
        .unwrap_or_else(|e| panic!("{rel}: to_wasi failed on an emitted module — an Almide bug: {e}"));
    let w = wasi.len() as u64;
    // Not `w >= n` any more: the shipped form drops the emitted module's
    // unreached fixed-slot helper stubs (#3136), so it can be the smaller.
    assert!(w >= 100, "{rel}: shipped {w} B — too small to be a real module, measurement broken");
    let mut dead = [dead_weight(&bytes, false), dead_weight(&wasi, true)].concat();
    dead.extend(component_dead_weight(&bytes, &host_ops));
    assert!(dead.is_empty(), "{rel}: ships bytes it can prove dead (#3114):\n  {}", dead.join("\n  "));
    let digest = |b: &[u8]| Sha256::digest(b).iter().map(|x| format!("{x:02x}")).collect::<String>();
    let print = format!("emitted {n} B {} / shipped {w} B {}", digest(&bytes), digest(&wasi));
    Fixture { rel: rel.to_string(), sizes: Some((n, w)), print }
}

/// The dead weight #3114 and #3136 removed, held out (the cost is fixed, so
/// it would come back on every module at once): an active data segment that
/// begins or ends with a zero byte (linear memory is already zero), and — in
/// a SHIPPED form, the one a transform finishes — a defined function no
/// export, element or start reaches, or a function import or a global that
/// nothing reached names. The emitted form keeps its five `almide.*` imports,
/// its fixed helper slots and its fixed globals at constant indices on
/// purpose: the transforms and the embedded host address them by position.
fn dead_weight(wasm: &[u8], shipped: bool) -> Vec<String> {
    let mut refs = Refs::default();
    for payload in wasmparser::Parser::new(0).parse_all(wasm) {
        refs.payload(payload.expect("valid module"));
    }
    let mut out = std::mem::take(&mut refs.zero_ended);
    if shipped {
        let (live, gets) = refs.live();
        let n = refs.imports + refs.bodies.len() as u32;
        out.extend((0..refs.imports).filter(|f| !live.contains(f)).map(|f| format!("function import {f} is never called")));
        out.extend((refs.imports..n).filter(|f| !live.contains(f)).map(|f| format!("function {f} is never reached")));
        out.extend((0..refs.globals).filter(|g| !gets.contains(g)).map(|g| format!("global {g} is never read, written or exported")));
    }
    out
}

/// The same check on the p2 and p3 components' core module (#3136), for every
/// fixture whose op set the direct component shims serve.
fn component_dead_weight(bytes: &[u8], host_ops: &[i32]) -> Vec<String> {
    let mut out = Vec::new();
    for p3 in [false, true] {
        if almide_wasm_run::component_availability::check(host_ops, p3).is_err() {
            continue;
        }
        let form = if p3 { "p3" } else { "p2" };
        let component = if p3 { almide_wasm_run::wasi_p3::to_p3(bytes, host_ops) } else { almide_wasm_run::wasi_p2::to_p2(bytes) }
            .unwrap_or_else(|e| panic!("{form} transform failed on a served module — an Almide bug: {e}"));
        out.extend(dead_weight(core_module(&component), true).into_iter().map(|d| format!("{form}: {d}")));
    }
    out
}

/// The component's main core module: the largest one it embeds (the others
/// are wit-component's small indirect-lowering shims).
fn core_module(component: &[u8]) -> &[u8] {
    wasmparser::Parser::new(0)
        .parse_all(component)
        .filter_map(|p| match p.expect("valid component") {
            wasmparser::Payload::ModuleSection { unchecked_range, .. } => {
                Some(unchecked_range.start as usize..unchecked_range.end as usize)
            }
            _ => None,
        })
        .max_by_key(|r| r.len())
        .map(|r| &component[r])
        .expect("a component embeds its core module")
}

/// What [`dead_weight`] reads off a module: the function-import and global
/// counts, what each body and what the rest of the module names, and the
/// zero-ended active data segments.
#[derive(Default)]
struct Refs {
    imports: u32,
    globals: u32,
    /// The defined function whose body is being read.
    cur: Option<usize>,
    /// Functions and globals named outside every body.
    funcs: std::collections::BTreeSet<u32>,
    gets: std::collections::BTreeSet<u32>,
    /// Each body's named functions and globals.
    bodies: Vec<(std::collections::BTreeSet<u32>, std::collections::BTreeSet<u32>)>,
    zero_ended: Vec<String>,
}

impl Refs {
    fn op(&mut self, op: wasmparser::Operator<'_>) {
        use wasmparser::Operator as O;
        let (funcs, gets) = match self.cur {
            Some(i) => {
                let (f, g) = &mut self.bodies[i];
                (f, g)
            }
            None => (&mut self.funcs, &mut self.gets),
        };
        match op {
            O::Call { function_index } | O::ReturnCall { function_index } | O::RefFunc { function_index } => {
                funcs.insert(function_index);
            }
            O::GlobalGet { global_index } | O::GlobalSet { global_index } => {
                gets.insert(global_index);
            }
            _ => {}
        }
    }

    /// The functions the roots reach through the bodies, and the globals
    /// the roots and the reached bodies name.
    fn live(&self) -> (std::collections::BTreeSet<u32>, std::collections::BTreeSet<u32>) {
        let mut live = std::collections::BTreeSet::new();
        let mut gets = self.gets.clone();
        let mut work: Vec<u32> = self.funcs.iter().copied().collect();
        while let Some(f) = work.pop() {
            if !live.insert(f) {
                continue;
            }
            if let Some((fs, gs)) = f.checked_sub(self.imports).and_then(|d| self.bodies.get(d as usize)) {
                work.extend(fs.iter().copied());
                gets.extend(gs.iter().copied());
            }
        }
        (live, gets)
    }

    fn expr(&mut self, e: wasmparser::ConstExpr<'_>) {
        for op in e.get_operators_reader() {
            self.op(op.expect("const expr"));
        }
    }

    fn element(&mut self, e: wasmparser::Element<'_>) {
        if let wasmparser::ElementKind::Active { offset_expr, .. } = e.kind {
            self.expr(offset_expr);
        }
        match e.items {
            wasmparser::ElementItems::Functions(fs) => self.funcs.extend(fs.into_iter().map(|f| f.expect("element fn"))),
            wasmparser::ElementItems::Expressions(_, es) => es.into_iter().for_each(|x| self.expr(x.expect("element expr"))),
        }
    }

    fn data(&mut self, i: usize, d: wasmparser::Data<'_>) {
        let zero_end = d.data.first() == Some(&0) || d.data.last() == Some(&0);
        if matches!(d.kind, wasmparser::DataKind::Active { .. }) && zero_end {
            self.zero_ended.push(format!("data segment {i} has a zero run at an end ({} B) — memory starts zeroed", d.data.len()));
        }
    }

    fn export(&mut self, e: wasmparser::Export<'_>) {
        match e.kind {
            wasmparser::ExternalKind::Func => self.funcs.insert(e.index),
            wasmparser::ExternalKind::Global => self.gets.insert(e.index),
            _ => false,
        };
    }

    fn payload(&mut self, payload: wasmparser::Payload<'_>) {
        use wasmparser::Payload as P;
        match payload {
            P::ImportSection(r) => {
                let funcs = r.into_imports().filter(|i| matches!(i.as_ref().expect("import").ty, wasmparser::TypeRef::Func(_)));
                self.imports += funcs.count() as u32;
            }
            P::GlobalSection(r) => r.into_iter().for_each(|g| {
                self.globals += 1;
                self.expr(g.expect("global").init_expr);
            }),
            P::ExportSection(r) => r.into_iter().for_each(|e| self.export(e.expect("export"))),
            P::StartSection { func, .. } => {
                self.funcs.insert(func);
            }
            P::ElementSection(r) => r.into_iter().for_each(|e| self.element(e.expect("element"))),
            P::CodeSectionEntry(b) => {
                self.cur = Some(self.bodies.len());
                self.bodies.push(Default::default());
                b.get_operators_reader().expect("body").into_iter().for_each(|op| self.op(op.expect("operator")));
                self.cur = None;
            }
            P::DataSection(r) => r.into_iter().enumerate().for_each(|(i, d)| self.data(i, d.expect("data segment"))),
            _ => {}
        }
    }
}

fn corpus_rows(root: &Path) -> Vec<String> {
    let manifest = std::fs::read_to_string(root.join("crates/almide-spine/tests/golden/spec-run-manifest.txt"))
        .expect("run manifest");
    almide_corpus::manifest_rows(&manifest)
        .map(|line| line.splitn(3, '\t').nth(2).expect("manifest row").to_string())
        .collect()
}

/// The child entry (#2309): with [`ALONE`] set, measure that one fixture in
/// this fresh process and print its fingerprint. Without it, nothing — so a
/// plain `-- --ignored` run passes it by.
#[test]
#[ignore = "the isolation check's child entry: corpus_sizes_hold_the_baseline spawns it once per fixture"]
fn one_fixture_measured_alone() {
    let Ok(rel) = std::env::var(ALONE) else { return };
    let root = workspace_root();
    let p = std::env::var(PRESSURE).map_or(Pressure::Plain, |p| Pressure::from_name(&p).expect("a pressure name"));
    set_pressure(p);
    println!("{CHILD_TAG}{}", measure_one(&root, &rel).print);
}

/// One fixture's fingerprint from a fresh process of this binary, built
/// under one allocation pressure.
fn measured_alone(exe: &Path, rel: &str, pressure: Pressure) -> Result<String, String> {
    let out = Command::new(exe)
        .args([CHILD, "--exact", "--ignored", "--nocapture", "--test-threads=1"])
        .env(ALONE, rel)
        .env(PRESSURE, pressure.name())
        .output()
        .map_err(|e| format!("spawn failed: {e}"))?;
    let stdout = String::from_utf8_lossy(&out.stdout);
    // libtest prints `test <name> ... ` on the same line before the result.
    match stdout.lines().find_map(|l| l.split_once(CHILD_TAG).map(|(_, print)| print)) {
        Some(print) if out.status.success() => Ok(print.to_string()),
        _ => {
            let stderr = String::from_utf8_lossy(&out.stderr);
            let lines: Vec<&str> = stderr.lines().collect();
            let tail = lines[lines.len().saturating_sub(6)..].join(" | ");
            Err(format!("child exited {} without a result: {tail}", out.status))
        }
    }
}

/// Every fixture measured alone under every pressure, one fresh process
/// each, spread over the available cores; the results come back in corpus
/// order, one per (fixture, pressure) — fixture-major.
fn measure_each_alone(rels: &[String]) -> Vec<Result<String, String>> {
    let exe = std::env::current_exe().expect("the test binary's own path");
    let jobs: Vec<(&str, Pressure)> =
        rels.iter().flat_map(|rel| PRESSURES.iter().map(move |&p| (rel.as_str(), p))).collect();
    let next = std::sync::atomic::AtomicUsize::new(0);
    let workers = std::thread::available_parallelism().map_or(4, |n| n.get());
    let mut results: Vec<(usize, Result<String, String>)> = std::thread::scope(|s| {
        let handles: Vec<_> = (0..workers)
            .map(|_| {
                s.spawn(|| {
                    let mut mine = Vec::new();
                    loop {
                        let i = next.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
                        let Some(&(rel, p)) = jobs.get(i) else { break mine };
                        mine.push((i, measured_alone(&exe, rel, p)));
                    }
                })
            })
            .collect();
        handles.into_iter().flat_map(|h| h.join().expect("isolation worker")).collect()
    });
    results.sort_by_key(|(i, _)| *i);
    results.into_iter().map(|(_, r)| r).collect()
}

/// Refuse any fixture whose module differs between the corpus pass and a
/// fresh process under any pressure: the ledger must pin what a one-program
/// build ships, and that build must not depend on the heap it ran in.
fn hold_isolation(corpus: &[Fixture], alone: &[Result<String, String>]) {
    assert_eq!(alone.len(), corpus.len() * PRESSURES.len(), "one child per (fixture, pressure)");
    let offences: Vec<String> = corpus
        .iter()
        .zip(alone.chunks(PRESSURES.len()))
        .flat_map(|(f, prints)| {
            PRESSURES.iter().map(|p| p.name()).zip(prints).filter_map(move |(p, a)| match a {
                Ok(print) if *print == f.print => None,
                Ok(print) => Some(format!(
                    "{}:\n    corpus order:      {}\n    alone ({p:<10}): {print}",
                    f.rel, f.print
                )),
                Err(e) => Some(format!("{} ({p}): {e}", f.rel)),
            })
        })
        .collect();
    assert!(
        offences.is_empty(),
        "size ratchet isolation/determinism ({} build(s)) — a fixture's module differs between the one-process \
         corpus pass and a fresh process. Under `plain` too: a process-global cache in the front or the emitter \
         carries one program's state into the next (#2309); scope that state per program. Only under \
         `quarantine` / `ballast`: the emitter's output depends on what the allocator reuses — a fact keyed by \
         an address that outlived its node (#3143):\n{}",
        offences.len(),
        offences.join("\n")
    );
}

#[cfg_attr(debug_assertions, ignore = "size gate is release-only (bytes are profile-independent; time is not)")]
#[test]
fn corpus_sizes_hold_the_baseline() {
    let root = workspace_root();
    let rels = corpus_rows(&root);
    // The children run while this thread measures the corpus in order.
    let (corpus, alone) = std::thread::scope(|s| {
        let alone = s.spawn(|| measure_each_alone(&rels));
        let corpus: Vec<Fixture> = rels.iter().map(|rel| measure_one(&root, rel)).collect();
        (corpus, alone.join().expect("isolation pass"))
    });
    hold_isolation(&corpus, &alone);

    let emitted: Vec<(&str, Option<u64>)> = corpus.iter().map(|f| (f.rel.as_str(), f.sizes.map(|(n, _)| n))).collect();
    let shipped: Vec<(&str, Option<u64>)> = corpus.iter().map(|f| (f.rel.as_str(), f.sizes.map(|(_, w)| w))).collect();
    // Both ledgers are judged before either fails, so one run names every moved row.
    let verdicts: Vec<String> = [
        hold_the_baseline("size-baseline.txt", "emitted", &emitted),
        hold_the_baseline("size-baseline-wasi.txt", "shipped (to_wasi)", &shipped),
    ]
    .into_iter()
    .flatten()
    .collect();
    assert!(verdicts.is_empty(), "{}", verdicts.join("\n\n"));
}

/// A ledger's rows: `rel -> Some(size)`, or `None` for a `!` row (the
/// structural leg refuses the fixture; the CLI reroutes it).
fn parse_ledger(text: &str) -> std::collections::BTreeMap<&str, Option<u64>> {
    text.lines()
        .map(|l| {
            let (n, rel) = l.split_once('\t').expect("baseline row");
            (rel, if n == "!" { None } else { Some(n.parse().expect("baseline size")) })
        })
        .collect()
}

fn render_ledger(rows: &[(&str, Option<u64>)]) -> String {
    rows.iter()
        .map(|(rel, n)| match n {
            Some(n) => format!("{n}\t{rel}\n"),
            None => format!("!\t{rel}\n"),
        })
        .collect()
}

/// One fixture's measurement against its pinned row, as the offence it
/// raises (if any). `pinned` is `None` when the ledger has no row for it.
fn row_offence(rel: &str, pinned: Option<Option<u64>>, got: Option<u64>) -> Option<String> {
    match (pinned, got) {
        (Some(None), None) => None,
        (Some(Some(b)), Some(n)) if b == n => None,
        (None, Some(n)) => Some(format!("{rel}: NEW fixture ({n} B) not in the ledger")),
        (None, None) => Some(format!("{rel}: NEW fixture (structural-refused) not in the ledger")),
        (Some(None), Some(n)) => Some(format!("{rel}: pinned `!` (structural-refused) but now emits {n} B")),
        (Some(Some(b)), None) => Some(format!("{rel}: pinned {b} B but the structural leg now refuses it")),
        (Some(Some(b)), Some(n)) => {
            let cap = (b as f64 * PER_FIXTURE_FACTOR) as u64 + PER_FIXTURE_SLACK;
            let delta = n as i64 - b as i64;
            Some(if n > cap {
                format!("{rel}: {n} B > cap {cap} B (pinned {b} B, {delta:+} B) — a REGRESSION: fix it, do not claim it")
            } else {
                format!("{rel}: {n} B, pinned {b} B ({delta:+} B)")
            })
        }
    }
}

/// One ledger's verdict: `None` when every row is as pinned (or the ledger
/// was just regenerated), else the failure to report.
fn hold_the_baseline(name: &str, form: &str, rows: &[(&str, Option<u64>)]) -> Option<String> {
    let bp = baseline_path(name);
    let total: u64 = rows.iter().filter_map(|(_, n)| *n).sum();
    if std::env::var("ALMIDE_UPDATE_SIZES").is_ok() {
        std::fs::write(&bp, render_ledger(rows)).expect("write baseline");
        println!("RATCHET sizes [{form}]: {} rows, {total} B — ledger regenerated", rows.len());
        return None;
    }
    let text = std::fs::read_to_string(&bp)
        .unwrap_or_else(|_| panic!("golden/{name} — generate with ALMIDE_UPDATE_SIZES=1"));
    let mut pinned = parse_ledger(&text);
    let pinned_total: u64 = pinned.values().flatten().sum();
    if total * 2 < pinned_total {
        return Some(format!(
            "aggregate [{form}] {total} B is under HALF the ledger's {pinned_total} B — a collapse this size is a \
             broken measurement (stub emission?), not a win; re-ratify deliberately if it is real"
        ));
    }
    let mut offences: Vec<String> =
        rows.iter().filter_map(|(rel, n)| row_offence(rel, pinned.remove(rel), *n)).collect();
    offences.extend(pinned.keys().map(|rel| format!("{rel}: in the ledger but not in the corpus")));
    if offences.is_empty() {
        println!("RATCHET sizes [{form}]: {} rows, {total} B, every row as pinned", rows.len());
        return None;
    }
    Some(format!(
        "size ratchet [{form}] ({} row(s) moved, total {total} B against {pinned_total} B pinned) — every row \
         is pinned exactly: claim a move in the change that makes it with ALMIDE_UPDATE_SIZES=1, and fix a \
         regression first:\n{}",
        offences.len(),
        offences.join("\n")
    ))
}
