//! Stdlib-LINKING size ladder (#2141) — the shipped bytes of programs that
//! pull the stdlib in by degrees, pinned per rung in golden/size-ladder.txt.
//!
//! `size_ratchet` pins every manifest fixture, so it sees a byte move on
//! 700 programs at once — but nearly all of them link almost nothing, and
//! the five programs the README's size story was drawn from (hello /
//! fizzbuzz / fibonacci / closure / variant) are the most favourable points
//! of the set. The neighbour-arena measurement (arena-breakthroughs §2.4,
//! 2026-09-13) showed the size story is decided by programs that LINK the
//! stdlib — Map / String / JSON — where the bump lane lost every cell to a
//! peer while hello won. Nothing ratcheted those. This gate does.
//!
//! The corpus is a ladder: one or more programs per rung, each rung linking
//! one more stdlib regime than the last (T0 nothing → T7 regex), every one
//! an existing program of the tree — the perf corpus's own programs where
//! it has one, a `spec/wasm_cross` fixture where it does not. Two rungs
//! carry both the imperative and the documented-idiom spelling of the same
//! program (#2098), so a size tax on the documented style cannot return
//! unnoticed either.
//!
//! What is pinned is the SHIPPED form — the module after `to_wasi`, the
//! bytes `almide build --target wasm` writes — because that is the number a
//! peer comparison reads. The row is a shrink-only ceiling pinned EXACTLY:
//! a measurement above its row is a REGRESSION and a measurement below it
//! is a stale row (a tolerance let rows drift unclaimed, #2309); either way
//! the change that moves a byte claims it by regenerating:
//!
//!   ALMIDE_UPDATE_SIZE_LADDER=1 cargo test --release -p almide-wasm --test size_ladder
//!
//! Raising a row is a deliberate commit with the reason in its message
//! (`scripts/check-ratchet-separation.sh` keeps that commit apart from the
//! implementation it judges). The ledger also carries the ladder's total,
//! so a spread of small growths that no single row names still reads red.
//! The table below is printed on every run — pass or fail — so a reader of
//! the gate output sees which rung grew, by how much, without a diff.
//! Two broken-measurement guards (as in `size_ratchet`): a module under
//! 100 bytes, or a total collapsing under half the ledger, is an
//! INSTRUMENTATION FAILURE, never a win.

use std::path::{Path, PathBuf};

/// The ladder: (rung, corpus-relative path). Order is the table's order.
/// A rung's name says what the program links beyond the rung before it.
const LADDER: &[(&str, &str)] = &[
    ("T0 nothing", "research/benchmark/perf/wasm-size/hello.almd"),
    ("T1 Int print", "research/benchmark/perf/wasm-size/fibonacci.almd"),
    ("T1 Int print", "research/benchmark/perf/binarytrees/binarytrees.almd"),
    ("T2 Float print", "research/benchmark/perf/spectralnorm/spectralnorm.almd"),
    ("T3 String", "research/benchmark/perf/strchurn/strchurn.almd"),
    ("T3 String", "research/benchmark/perf/strbuild/strbuild_append.almd"),
    ("T4 List", "research/benchmark/perf/listbuild/listbuild_append.almd"),
    ("T4 List", "research/benchmark/perf/listbuild/listbuild_combinator.almd"),
    ("T5 Map", "research/benchmark/perf/mapbuild/mapbuild.almd"),
    ("T5 Map", "research/benchmark/perf/wordfreq/wordfreq.almd"),
    ("T5 Map", "research/benchmark/perf/wordfreq/wordfreq_group.almd"),
    ("T6 JSON", "research/benchmark/perf/decode/decode.almd"),
    ("T6 JSON", "spec/wasm_cross/json_gltf_walk.almd"),
    ("T7 regex", "spec/wasm_cross/regex_engine.almd"),
];

/// The ledger row that carries the ladder's sum.
const TOTAL: &str = "total";
const LEDGER: &str = "size-ladder.txt";
const UPDATE: &str = "ALMIDE_UPDATE_SIZE_LADDER";

fn workspace_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..").canonicalize().expect("test harness invariant")
}

fn ledger_path() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/golden").join(LEDGER)
}

/// One rung measured: emitted bytes and shipped (`to_wasi`) bytes. A program
/// the structural leg refuses is a hole in the ladder, not a row: the CLI
/// would reroute it to the incumbent and the ledger would pin a module that
/// never ships — so it fails here, loudly, until it emits or is swapped for
/// a program of the same rung that does.
fn measure(root: &Path, rung: &str, rel: &str) -> (u64, u64) {
    let text = std::fs::read_to_string(almide_corpus::resolve(root, rel)).expect("ladder program readable");
    let ir = almide_spine::s5::lower_to_ir(rel, &text)
        .unwrap_or_else(|e| panic!("{rel} ({rung}): the front refuses a ladder program: {e:?}"));
    let (bytes, host_ops) = almide_wasm::emit_program_with_ops(&ir).unwrap_or_else(|e| {
        panic!("{rel} ({rung}): the structural leg refuses a ladder program ({e:?}) — the ladder pins only what ships")
    });
    let n = bytes.len() as u64;
    assert!(n >= 100, "{rel}: {n} bytes — too small to be a real module, measurement broken");
    let host_ops: Vec<i32> = host_ops.into_iter().collect();
    let wasi = almide_wasm_run::wasi::to_wasi(&bytes, &host_ops)
        .unwrap_or_else(|e| panic!("{rel}: to_wasi failed on an emitted module — an Almide bug: {e}"));
    let w = wasi.len() as u64;
    assert!(w >= n, "{rel}: shipped {w} B < emitted {n} B — the transform only ADDS sections, measurement broken");
    (n, w)
}

/// The ledger's rows: `shipped<TAB>rung<TAB>rel`, the total last.
fn parse_ledger(text: &str) -> Vec<(String, String, u64)> {
    almide_corpus::manifest_rows(text)
        .map(|l| {
            let mut cols = l.splitn(3, '\t');
            let n: u64 = cols.next().expect("row").parse().expect("ledger size");
            let rung = cols.next().expect("ledger rung").to_string();
            let rel = cols.next().expect("ledger path").to_string();
            (rel, rung, n)
        })
        .collect()
}

fn render_ledger(rows: &[(&str, &str, u64)], total: u64) -> String {
    let mut out = String::from(
        "# Stdlib-linking size ladder (#2141): shipped (to_wasi) bytes per rung, pinned exactly.\n\
         # Regenerate (claims every move): ALMIDE_UPDATE_SIZE_LADDER=1 cargo test --release -p almide-wasm --test size_ladder\n\
         # shipped<TAB>rung<TAB>program; the last row is the ladder's total.\n",
    );
    for (rel, rung, n) in rows {
        out.push_str(&format!("{n}\t{rung}\t{rel}\n"));
    }
    out.push_str(&format!("{total}\tΣ\t{TOTAL}\n"));
    out
}

/// The table every run prints: rung, program, emitted, shipped, pinned, delta, verdict.
fn render_table(rows: &[(&str, &str, u64, u64)], pinned: &[(String, String, u64)], total: u64) -> String {
    let pin = |rel: &str| pinned.iter().find(|(r, _, _)| r == rel).map(|(_, _, n)| *n);
    let verdict = |got: u64, pinned: Option<u64>| match pinned {
        None => "NEW (not in the ledger)".to_string(),
        Some(p) if p == got => "as pinned".to_string(),
        Some(p) if got > p => format!("{:+} B  GREW", got as i64 - p as i64),
        Some(p) => format!("{:+} B  shrank (lower the row)", got as i64 - p as i64),
    };
    let mut out = format!(
        "{:<15} {:<62} {:>9} {:>9} {:>9}  {}\n",
        "rung", "program", "emitted", "shipped", "pinned", "verdict"
    );
    for (rung, rel, n, w) in rows {
        let p = pin(rel);
        out.push_str(&format!(
            "{rung:<15} {rel:<62} {n:>9} {w:>9} {:>9}  {}\n",
            p.map_or("-".to_string(), |p| p.to_string()),
            verdict(*w, p)
        ));
    }
    let p = pin(TOTAL);
    out.push_str(&format!(
        "{:<15} {:<62} {:>9} {:>9} {:>9}  {}\n",
        "Σ",
        TOTAL,
        "",
        total,
        p.map_or("-".to_string(), |p| p.to_string()),
        verdict(total, p)
    ));
    out
}

#[cfg_attr(debug_assertions, ignore = "size gate is release-only (bytes are profile-independent; time is not)")]
#[test]
fn stdlib_linking_ladder_holds_the_ceiling() {
    let root = workspace_root();
    let rows: Vec<(&str, &str, u64, u64)> =
        LADDER.iter().map(|(rung, rel)| { let (n, w) = measure(&root, rung, rel); (*rung, *rel, n, w) }).collect();
    let total: u64 = rows.iter().map(|(_, _, _, w)| *w).sum();
    let ledger_rows: Vec<(&str, &str, u64)> = rows.iter().map(|(rung, rel, _, w)| (*rel, *rung, *w)).collect();

    if std::env::var(UPDATE).is_ok() {
        std::fs::write(ledger_path(), render_ledger(&ledger_rows, total)).expect("write ledger");
        println!("{}", render_table(&rows, &[], total));
        println!("SIZE LADDER: {} rungs, {total} B shipped — ledger regenerated", rows.len());
        return;
    }
    let text = std::fs::read_to_string(ledger_path())
        .unwrap_or_else(|_| panic!("golden/{LEDGER} — generate with {UPDATE}=1"));
    let pinned = parse_ledger(&text);
    let table = render_table(&rows, &pinned, total);
    println!("{table}");

    let pinned_total = pinned.iter().find(|(r, _, _)| r == TOTAL).map(|(_, _, n)| *n).expect("ledger total row");
    assert!(
        total * 2 >= pinned_total,
        "ladder total {total} B is under HALF the ledger's {pinned_total} B — a collapse this size is a broken \
         measurement (stub emission?), not a win; re-ratify deliberately if it is real\n{table}"
    );

    let mut offences: Vec<String> = Vec::new();
    for (rung, rel, _, w) in &rows {
        match pinned.iter().find(|(r, _, _)| r == rel) {
            None => offences.push(format!("{rel} ({rung}): NEW rung ({w} B) not in the ledger")),
            Some((_, _, p)) if p == w => {}
            Some((_, _, p)) if w > p => offences.push(format!(
                "{rel} ({rung}): {w} B > ceiling {p} B ({:+} B) — a REGRESSION: fix it, or raise the row in its own \
                 commit whose message says why",
                *w as i64 - *p as i64
            )),
            Some((_, _, p)) => offences.push(format!(
                "{rel} ({rung}): {w} B under its {p} B row ({:+} B) — claim the shrink by lowering the row",
                *w as i64 - *p as i64
            )),
        }
    }
    for (rel, rung, _) in &pinned {
        if rel != TOTAL && !LADDER.iter().any(|(_, r)| r == rel) {
            offences.push(format!("{rel} ({rung}): in the ledger but not on the ladder"));
        }
    }
    if total != pinned_total {
        offences.push(format!(
            "total: {total} B against {pinned_total} B pinned ({:+} B)",
            total as i64 - pinned_total as i64
        ));
    }
    assert!(
        offences.is_empty(),
        "size ladder ({} row(s) moved) — every rung is a shrink-only ceiling pinned exactly: claim a move in the \
         change that makes it with {UPDATE}=1, and fix a regression first:\n{}",
        offences.len(),
        offences.join("\n")
    );
    println!("SIZE LADDER: {} rungs, {total} B shipped, every rung as pinned", rows.len());
}
