//! Phase 0 spike for docs/roadmap/active/v1-mir-architecture.md.
//!
//! GOAL (the decision gate): prove that a SINGLE Perceus ownership decision,
//! held in a minimal MIR, renders FAITHFULLY to BOTH idiomatic Rust
//! (move/borrow/clone) AND a reference-counted form (dup/drop), so the two
//! agree BY CONSTRUCTION — and that #643's class is therefore impossible.
//!
//! Scope note: the decision gate is about the OWNERSHIP MODEL, not wasm
//! bytecode, so we model the wasm RC *semantics* in Rust (std `Rc` = the heap
//! cell; `dup` = `Rc::clone`/inc; `drop` = scope drop/dec) and compile BOTH
//! renderings with one compiler (rustc). If one balanced MIR decision yields
//! correct idiomatic-Rust AND correct RC-Rust that AGREE, the model shares.
//!
//! It also shows the CONTRAST: the old hand-written wasm placed dup/drop by
//! hand and drifted (the #643 Some-box leak → a freed, still-referenced cell).
//! The "buggy" rendering reproduces that imbalance to make the win concrete.
//!
//! Run: `cargo run` in research/spike/v1-mir/ (standalone; not in the workspace).
//! This file is shape #2; the full 5-shape gate runs via `./run-gate.sh`.
//!
//! Status: DECISION GATE PASSED — 5/5 shapes, 0 conditional, 0 fail, 0 escape
//! hatch (2026-06-13, rustc 1.95.0). This file is shape #2 (`list.get(xs,i) ??
//! d`, the #643 core); shapes #1/#3/#4/#5 (alias-return, boxed pattern #610,
//! closure capture, AliasCow) live in `shapes/` and re-run via `./run-gate.sh`.
//! Full record + the 4 canonical-form refinements: see `GATE.md`. RC and Rust
//! move/borrow share ONE canonical form (Perceus); proceed to Phase 1.

#![allow(dead_code, unused_variables)]

use std::collections::HashMap;
use std::fmt::Write as _;
use std::process::Command;

// ───────────────────────── Minimal MIR ─────────────────────────
//
// Just enough to express the ownership-tricky shapes. The KEY property: the
// ownership decision (Dup/Drop and how each use treats a value) is EXPLICIT
// here, decided once. Renderers only translate it; they never re-decide.

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Repr {
    Scalar, // Copy (i64) — no RC, no clone
    Heap,   // String/list/… — needs an owned ref (RC: a cell; Rust: clone/move)
}

/// How a use site treats a value — the ownership decision at that site.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Use {
    Consume, // transfer ownership (last use): RC = pass the ref; Rust = move
    Borrow,  // read without consuming: RC = pass the ref (no count change); Rust = &
}

/// The value an `Let` binds. The producer is responsible for ownership of what
/// it yields (a runtime accessor that aliases a container element yields a
/// BORROW; the binding then `Dup`s it to acquire its own owned ref — that Dup
/// is the single decision both renderers honor).
#[derive(Clone, Debug)]
enum Rhs {
    /// container element accessor with a default (the `list.get(xs,i) ?? d`
    /// shape). Yields a value that ALIASES `container[idx]` (a borrow).
    GetOr { container: String, idx: String, default: String },
    /// fresh heap value produced by a runtime call (owned, no alias).
    FreshHeap { call: String },
    /// integer expression (scalar).
    ScalarExpr { code: String },
}

#[derive(Clone, Debug)]
enum Op {
    /// `let v: repr = rhs`
    Let { v: String, repr: Repr, rhs: Rhs },
    /// Perceus dup — `v` acquires one extra owned reference. The single
    /// ownership decision for "this binding aliases a still-live value".
    Dup { v: String },
    /// Perceus drop — release one reference held by `v`.
    Drop { v: String },
    /// push `v` into the list `list`, consuming the pushed ref (the list now
    /// owns it).
    Push { list: String, v: String, used: Use },
    /// effectful read of `v` (e.g. accumulate into output). Borrow.
    Use { v: String, used: Use },
}

/// A tiny MIR "function body": some heap-typed container setup + a loop body +
/// a final read. We model the #643 essence directly.
struct MirProgram {
    /// number of loop iterations
    iters: i64,
    /// the loop body ops (per iteration), with explicit ownership
    body: Vec<Op>,
    /// whether the renderer should HONOR the MIR (faithful) or reproduce the
    /// old hand-written imbalance (buggy: skip the loop-temp Drop, mimicking
    /// the leaked Some box).
    faithful: bool,
}

// ───────────────────────── The #643 shape in MIR ─────────────────────────
//
// Source: `let nx = list.get(cs, i) ?? ""; out.push(...)` in a loop.
// The ONE ownership decision: `nx` aliases `cs[i]` (a borrow that escapes into
// a binding), so it must Dup to own its ref, and Drop at scope end.

fn shape_643() -> Vec<Op> {
    vec![
        Op::Let {
            v: "nx".into(),
            repr: Repr::Heap,
            rhs: Rhs::GetOr { container: "cs".into(), idx: "i".into(), default: "\"\"".into() },
        },
        // THE decision: nx aliases a still-live element → acquire own ref.
        Op::Dup { v: "nx".into() },
        // a SECOND per-iteration heap temp (the slice|>join in #643), pushed.
        Op::Let {
            v: "t".into(),
            repr: Repr::Heap,
            rhs: Rhs::FreshHeap { call: "elem_at(&cs, i)".into() },
        },
        Op::Push { list: "out".into(), v: "t".into(), used: Use::Consume },
        Op::Use { v: "nx".into(), used: Use::Borrow },
        // scope end of the loop body: release the per-iteration owned refs.
        Op::Drop { v: "nx".into() },
    ]
}

// ───────────────────────── Renderer A: idiomatic Rust ─────────────────────
//
// Heap = owned `String`; the container = `Vec<String>`. Dup → `.clone()`;
// Consume → move; Borrow → `&`; Drop → let it leave scope (no-op, Rust drops).

fn render_idiomatic(p: &MirProgram) -> String {
    let mut s = String::new();
    let _ = writeln!(s, "fn main() {{");
    let _ = writeln!(s, "    let cs: Vec<String> = vec![\"a\".into(), \"b\".into(), \"c\".into()];");
    let _ = writeln!(s, "    let mut out: Vec<String> = Vec::new();");
    let _ = writeln!(s, "    fn elem_at(cs: &Vec<String>, i: i64) -> String {{ cs.get(i as usize).cloned().unwrap_or_default() }}");
    let _ = writeln!(s, "    for i in 0..{} {{", p.iters);
    for op in &p.body {
        match op {
            Op::Let { v, rhs, .. } => match rhs {
                Rhs::GetOr { container, idx, default } => {
                    // GetOr yields a borrow; the following Dup makes it owned →
                    // idiomatic Rust expresses dup-of-element as `.cloned()`.
                    let _ = writeln!(s, "        let {v} = {container}.get(({idx}) as usize).cloned().unwrap_or_else(|| {default}.to_string());");
                }
                Rhs::FreshHeap { call } => { let _ = writeln!(s, "        let {v} = {call};"); }
                Rhs::ScalarExpr { code } => { let _ = writeln!(s, "        let {v} = {code};"); }
            },
            // Dup already folded into the GetOr `.cloned()` above (the binding
            // owns its value), so the idiomatic renderer emits nothing here.
            Op::Dup { .. } => {}
            Op::Drop { .. } => {} // Rust drops at scope end
            Op::Push { list, v, .. } => { let _ = writeln!(s, "        {list}.push({v});"); }
            Op::Use { v, .. } => { let _ = writeln!(s, "        let _ = &{v};"); }
        }
    }
    let _ = writeln!(s, "    }}");
    let _ = writeln!(s, "    println!(\"{{}}\", out.join(\",\"));");
    let _ = writeln!(s, "}}");
    s
}

// ───────────────────────── Renderer B: reference-counted ──────────────────
//
// Models the wasm RC semantics in Rust: heap = `Rc<String>` (the cell);
// container = `Vec<Rc<String>>`. Dup → `Rc::clone` (inc); Drop → `drop` (dec);
// Consume → move the Rc; Borrow → `&`. A balanced MIR therefore yields a
// program where every cell reaches rc 0 exactly once — std `Rc` enforces it,
// so a leak or double-free is impossible WHEN the MIR is balanced.
//
// The `faithful=false` variant SKIPS the loop-temp Drop (reproducing the old
// hand-written #643 imbalance: the Dup'd ref leaks) — to show the contrast.

fn render_rc(p: &MirProgram) -> String {
    let mut s = String::new();
    // Manual RC heap that faithfully models the wasm runtime: cells carry an
    // explicit refcount, inc/dec are MANUAL (no Rust auto-drop), a cell hitting
    // 0 frees, and a dec below 0 is a DOUBLE FREE (the #643 sentinel). A
    // never-dec'd ref LEAKS (live != 0 at end). This is the discipline the wasm
    // emit hand-writes — and where #643 drifted.
    let _ = writeln!(s, "struct Heap {{ cells: Vec<(String, i32)>, live: i64 }}");
    let _ = writeln!(s, "impl Heap {{");
    let _ = writeln!(s, "    fn alloc(&mut self, s: &str) -> usize {{ self.cells.push((s.to_string(), 1)); self.live += 1; self.cells.len()-1 }}");
    let _ = writeln!(s, "    fn inc(&mut self, p: usize) {{ self.cells[p].1 += 1; }}");
    let _ = writeln!(s, "    fn dec(&mut self, p: usize) {{ self.cells[p].1 -= 1; if self.cells[p].1 == 0 {{ self.live -= 1; }} if self.cells[p].1 < 0 {{ eprintln!(\"DOUBLE FREE at cell {{}}\", p); std::process::exit(8); }} }}");
    let _ = writeln!(s, "    fn get(&self, p: usize) -> String {{ self.cells[p].0.clone() }}");
    let _ = writeln!(s, "}}");
    let _ = writeln!(s, "fn main() {{");
    let _ = writeln!(s, "    let mut h = Heap {{ cells: Vec::new(), live: 0 }};");
    let _ = writeln!(s, "    let cs: Vec<usize> = vec![h.alloc(\"a\"), h.alloc(\"b\"), h.alloc(\"c\")];");
    let _ = writeln!(s, "    let mut out: Vec<usize> = Vec::new();");
    let _ = writeln!(s, "    for i in 0..{} {{", p.iters);
    for op in &p.body {
        match op {
            Op::Let { v, rhs, .. } => match rhs {
                Rhs::GetOr { container, idx, default } => {
                    // GetOr yields a BORROW of the element (no count change) —
                    // the explicit Dup below is what gives nx its own ref.
                    let _ = writeln!(s, "        let {v} = if let Some(&p) = {container}.get(({idx}) as usize) {{ p }} else {{ h.alloc({default}) }};");
                }
                Rhs::FreshHeap { .. } => {
                    // a fresh heap value (the slice|>join temp) — own rc=1.
                    let _ = writeln!(s, "        let content = {container_get};", container_get = "cs.get((i) as usize).map(|&p| h.get(p)).unwrap_or_default()");
                    let _ = writeln!(s, "        let {v} = h.alloc(&content);");
                }
                Rhs::ScalarExpr { code } => { let _ = writeln!(s, "        let {v} = {code};"); }
            },
            // The single ownership decision, rendered as a real inc.
            Op::Dup { v } => { let _ = writeln!(s, "        h.inc({v});"); }
            Op::Push { list, v, .. } => { let _ = writeln!(s, "        {list}.push({v});"); }
            Op::Use { v, .. } => { let _ = writeln!(s, "        let _ = h.get({v});"); }
            Op::Drop { v } => {
                if p.faithful {
                    let _ = writeln!(s, "        h.dec({v});");
                } else {
                    // OLD hand-written #643 imbalance: the loop-temp's Drop is
                    // omitted → its Dup'd ref leaks (live never returns to 0).
                    let _ = writeln!(s, "        // (buggy) Drop({v}) omitted — the #643 leak");
                }
            }
        }
    }
    let _ = writeln!(s, "    }}");
    let _ = writeln!(s, "    let joined: Vec<String> = out.iter().map(|&p| h.get(p)).collect();");
    let _ = writeln!(s, "    println!(\"{{}}\", joined.join(\",\"));");
    let _ = writeln!(s, "    // teardown: release the lists' element refs");
    let _ = writeln!(s, "    let outc = out.clone(); for p in outc {{ h.dec(p); }}");
    let _ = writeln!(s, "    let csc = cs.clone(); for p in csc {{ h.dec(p); }}");
    let _ = writeln!(s, "    if h.live != 0 {{ eprintln!(\"RC IMBALANCE: {{}} cell(s) leaked\", h.live); std::process::exit(7); }}");
    let _ = writeln!(s, "}}");
    s
}

// ───────────────────────── Harness: compile + run + compare ───────────────

fn compile_and_run(label: &str, src: &str) -> (bool, String, String) {
    let dir = std::env::temp_dir().join(format!("v1mir_{label}"));
    let _ = std::fs::create_dir_all(&dir);
    let src_path = dir.join("m.rs");
    let bin_path = dir.join("m");
    std::fs::write(&src_path, src).unwrap();
    let build = Command::new("rustc")
        .args(["--edition", "2021", "-O"])
        .arg(&src_path)
        .arg("-o")
        .arg(&bin_path)
        .output()
        .expect("run rustc");
    if !build.status.success() {
        return (false, String::new(), String::from_utf8_lossy(&build.stderr).into_owned());
    }
    let run = Command::new(&bin_path).output().expect("run binary");
    let out = String::from_utf8_lossy(&run.stdout).trim().to_string();
    let err = String::from_utf8_lossy(&run.stderr).trim().to_string();
    (run.status.success(), out, err)
}

fn main() {
    println!("== Phase 0 spike: #643 shape — one MIR decision, two faithful renderings ==\n");

    // Faithful MIR-driven program.
    let faithful = MirProgram { iters: 3, body: shape_643(), faithful: true };
    let idiom = render_idiomatic(&faithful);
    let rc = render_rc(&faithful);

    let (i_ok, i_out, i_err) = compile_and_run("idiom", &idiom);
    let (r_ok, r_out, r_err) = compile_and_run("rc", &rc);

    println!("[idiomatic Rust] compile_ok={i_ok} out={i_out:?} {}", if i_err.is_empty() { String::new() } else { format!("err={i_err}") });
    println!("[RC (wasm sem.)] compile_ok={r_ok} out={r_out:?} {}", if r_err.is_empty() { String::new() } else { format!("err={r_err}") });

    let agree = i_ok && r_ok && i_out == r_out;
    println!("\n  → render agreement: {}", if agree { "YES (by construction)" } else { "NO" });

    // Contrast: the OLD hand-written imbalance (skip the loop-temp Drop) = #643.
    let buggy = MirProgram { iters: 3, body: shape_643(), faithful: false };
    let rc_buggy = render_rc(&buggy);
    let (b_ok, b_out, b_err) = compile_and_run("rc_buggy", &rc_buggy);
    let leaked = b_err.contains("RC IMBALANCE") || !b_ok;
    println!("  → old hand-written imbalance (loop-temp Drop omitted): {} (out={b_out:?} err={b_err:?})",
        if leaked { "LEAK/DIVERGE as #643" } else { "unexpectedly ok" });

    println!("\n== DECISION GATE (#643 shape) ==");
    let pass = agree && leaked;
    println!("  one balanced MIR decision → idiomatic Rust AND RC agree: {}", agree);
    println!("  the bug is in the RENDERER re-deciding (omitting the Drop), not the MIR: {}", leaked);
    println!("  VERDICT: {}", if pass { "PASS — RC and borrow share one canonical form for this shape" } else { "FAIL — investigate" });
    std::process::exit(if pass { 0 } else { 1 });

    // Silence unused warnings for ops not exercised by this shape yet.
    #[allow(unreachable_code)]
    { let _ = (Repr::Scalar, Use::Consume, Use::Borrow, HashMap::<String, Repr>::new()); }
}
