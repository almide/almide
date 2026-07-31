//! The one-driver gate (Unit 0.43, #925).
//!
//! B1's inventory found that the workspace did not merely have six hand-synced copies of
//! the post-typecheck stage order — it had TWO DIFFERENT ORDERS shipping at once:
//!
//!   * `src/cli/build.rs`, `src/cli/commands.rs`      → lower → **ir_link** → optimize → mono
//!   * `crates/almide-mir/src/pipeline.rs` (wasm),
//!     `crates/almide-interp` (the third oracle)      → lower → optimize → mono → **ir_link**
//!
//! Native shipped one, wasm shipped the other, and the independent judge sat on wasm's
//! side of the question by construction. `spec/wasm_cross` was green throughout, which is
//! the point: the equivalence claim was being carried by an untested assumption, not by a
//! shared driver.
//!
//! So the gate pins the ORDER, not the count of call sites. A gate that only forbade a
//! second *spelling* would have been satisfied by the pre-0.43 tree the moment someone
//! moved both spellings into one file, while the divergence survived.

use std::fs;
use std::path::{Path, PathBuf};

/// Files allowed to spell the stage order themselves.
///
/// `almide-driver` IS the driver. The two `almide-mir` examples are standalone
/// demonstrations of the cut point whose whole purpose is to show the sequence explicitly;
/// they are listed here rather than migrated so the exemption is visible and reviewable
/// instead of implicit.
const ALLOWED: &[&str] = &[
    "crates/almide-driver/src/lib.rs",
    "crates/almide-mir/examples/emit_cert_from_source.rs",
    "crates/almide-mir/examples/classify_corpus.rs",
];

/// The sites still spelling the order themselves, as of Unit 0.43's B2. This is a
/// RATCHET, not an exemption list: the gate asserts the set matches EXACTLY, so a new
/// hand-written driver fails immediately, and removing one requires editing this list —
/// which makes each migration visible in the diff instead of silently satisfying a
/// count. B3 emptied the two CLI driver rows (build.rs, commands.rs) after verifying the
/// order flip byte-identical on all 329 `spec/wasm_cross` fixtures; B4 empties the rest;
/// B5 deletes the constant.
///
/// Note the count: #925 said "≥6 independent driver sequences" and the mechanical sweep
/// found NINE. `classify_corpus_parts/classify_corpus_b.rs`,
/// `render_wasm/tests_part1.rs`, and `wasm_runtime_test_parts/p4_corpus.rs` were not in
/// the issue's inventory — which is itself the argument for a gate over a hand count.
const MIGRATION_BACKLOG: &[&str] = &[];

fn repo_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
}

fn rust_sources(dir: &Path, out: &mut Vec<PathBuf>) {
    let Ok(entries) = fs::read_dir(dir) else { return };
    for e in entries.flatten() {
        let p = e.path();
        let name = p.file_name().and_then(|s| s.to_str()).unwrap_or("");
        if p.is_dir() {
            if name == "target" || name == ".git" || name == "node_modules" {
                continue;
            }
            rust_sources(&p, out);
        } else if name.ends_with(".rs") {
            out.push(p);
        }
    }
}

/// A file "spells the order" when it calls `ir_link` and `monomorphize` CLOSE TOGETHER.
///
/// Proximity, not mere co-occurrence: a hand-written driver spells the stages adjacently,
/// while a file can legitimately mention both far apart in unrelated functions —
/// `src/cli/emit.rs` calls `ir_link` at line 108 inside `emit_codegen_output` and
/// `monomorphize` at 164 inside something else, and flagging it as a seventh driver was a
/// false positive that would have been "fixed" by an exemption entry hiding a real one
/// later. Ten lines is comfortably wider than any real driver (the widest was six) and far
/// narrower than the emit.rs gap.
const ADJACENCY_LINES: usize = 10;

fn spells_the_order(src: &str) -> bool {
    let mut link: Vec<usize> = Vec::new();
    let mut mono: Vec<usize> = Vec::new();
    for (i, l) in src.lines().enumerate() {
        let t = l.trim_start();
        if t.starts_with("//") || t.starts_with("///") || t.starts_with("//!") {
            continue;
        }
        if l.contains("ir_link(") {
            link.push(i);
        }
        if l.contains("monomorphize(") {
            mono.push(i);
        }
    }
    link.iter()
        .any(|a| mono.iter().any(|b| a.abs_diff(*b) <= ADJACENCY_LINES))
}

#[test]
fn only_the_driver_spells_the_stage_order() {
    let root = repo_root();
    let mut files = Vec::new();
    for sub in ["src", "crates", "tests"] {
        rust_sources(&root.join(sub), &mut files);
    }

    let mut offenders: Vec<String> = Vec::new();
    for f in &files {
        let rel = f.strip_prefix(&root).unwrap().to_string_lossy().replace('\\', "/");
        if ALLOWED.contains(&rel.as_str()) || rel == "tests/one_driver_test.rs" {
            continue;
        }
        let Ok(src) = fs::read_to_string(f) else { continue };
        if spells_the_order(&src) {
            offenders.push(rel);
        }
    }

    offenders.sort();
    let mut expected: Vec<String> = MIGRATION_BACKLOG.iter().map(|s| s.to_string()).collect();
    expected.sort();

    let unexpected: Vec<_> = offenders.iter().filter(|o| !expected.contains(o)).collect();
    assert!(
        unexpected.is_empty(),
        "a NEW file spells the post-typecheck stage order itself instead of calling \
         `almide_driver::link_ir`. That is how the tree ended up shipping `ir_link` FIRST on \
         native and LAST on wasm (#925 / #785):\n  {}",
        unexpected.iter().map(|s| s.as_str()).collect::<Vec<_>>().join("\n  ")
    );

    let migrated: Vec<_> = expected.iter().filter(|e| !offenders.contains(e)).collect();
    assert!(
        migrated.is_empty(),
        "these sites no longer spell the order — the ratchet only counts DOWN, so remove them \
         from MIGRATION_BACKLOG in the same commit that migrated them:\n  {}",
        migrated.iter().map(|s| s.as_str()).collect::<Vec<_>>().join("\n  ")
    );
}

/// The driver's own order must not be re-permuted. `ir_link` last is the shipped-wasm and
/// v1-trust-spine order, chosen so the linker sees the MONOMORPHIZED call graph rather than
/// resolving calls mono is about to specialize.
#[test]
fn the_driver_runs_ir_link_after_monomorphize() {
    let src = fs::read_to_string(repo_root().join("crates/almide-driver/src/lib.rs"))
        .expect("the driver crate is missing");
    let body_start = src.find("pub fn link_ir(").expect("link_ir is missing");
    let body = &src[body_start..];
    let opt = body.find("optimize_program(").expect("optimize is missing from the driver");
    let mono = body.find("monomorphize(").expect("monomorphize is missing from the driver");
    let link = body.find("ir_link(").expect("ir_link is missing from the driver");
    assert!(
        opt < mono && mono < link,
        "the driver must run optimize → monomorphize → ir_link, in that order \
         (found optimize@{opt}, monomorphize@{mono}, ir_link@{link})"
    );
}
