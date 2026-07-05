//! Phase 0 spike, shape #4 (closure_capture) for
//! docs/roadmap/active/v1-mir-architecture.md section 8.
//!
//! Replicates research/spike/v1-mir/src/main.rs's pattern for THIS shape:
//!
//!   Almide source:
//!     fn main() {
//!       let x = make_heap()          // fresh owned heap value
//!       let f = || use(x)            // closure captures x; reads it
//!       call f()                     // first call
//!       call f()                     // second call  → must be Fn, not FnOnce
//!     }
//!
//! THE single ownership decision (Perceus, decided once in MIR):
//!   The closure's CAPTURE of x is a `Dup x` into the closure environment
//!   (the env acquires its own owned ref). Each `use(x)` inside the body is a
//!   BORROW (read, no count change) — that is what makes the closure callable
//!   twice. When the closure value itself drops, its environment runs
//!   `Drop x` (one dec), releasing the captured ref.
//!
//!   capture(x)  ==  Dup x into env   +   Drop env-x at closure-drop
//!   use(x)      ==  Borrow x         (no count change)
//!
//! GATE (the real test for this shape): does that ONE decision render to BOTH
//!   (A) an idiomatic Rust closure (a `move ||` capturing an owned String,
//!       called twice, body borrows it), AND
//!   (B) the manual-RC env (inc at capture, borrow per call, dec at drop),
//! agreeing on output, WITHOUT Rust needing `Rc` + `RefCell` to express the
//! shared capture? If Rust needs `Rc` for the *shared read-only* capture, that
//! is an escape hatch and the verdict drops to conditional/fail.
//!
//! Contrast: a BUGGY renderer that re-decides — it treats the capture as a
//! move/consume of x (FnOnce semantics) yet the program calls f twice. In the
//! RC rendering that manifests as the env dec'ing x on the FIRST call (as if
//! the capture were consumed there), so the SECOND call reads a freed cell and
//! the final teardown dec's it again → DOUBLE FREE. (A pure Rust `FnOnce`
//! version would simply fail to COMPILE on the second call; we model the
//! ownership drift in RC where it becomes a memory bug, mirroring #643's class:
//! the renderer re-deciding ownership, not the MIR.)
//!
//! Run: `rustc --edition 2021 -O main.rs -o spike && ./spike`

#![allow(dead_code, unused_variables)]

use std::fmt::Write as _;
use std::process::Command;

// ───────────────────────── Minimal MIR ─────────────────────────
//
// Just enough for the closure-capture shape. The ownership decision is
// EXPLICIT and decided ONCE here; renderers only translate, never re-decide.

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Repr {
    Scalar, // Copy — no RC
    Heap,   // String — RC: a cell; Rust: owned String moved into the closure
}

/// How a use site treats a value — the ownership decision at that site.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Use {
    Consume, // last use: transfer ownership (RC = pass ref; Rust = move)
    Borrow,  // read without consuming (RC = no count change; Rust = &)
}

#[derive(Clone, Debug)]
enum Rhs {
    /// fresh heap value produced by a runtime call (owned, rc=1, no alias).
    FreshHeap { call: String },
}

#[derive(Clone, Debug)]
enum Op {
    /// `let v: repr = rhs`
    Let { v: String, repr: Repr, rhs: Rhs },
    /// Perceus dup — `v` acquires one extra owned reference.
    Dup { v: String },
    /// Perceus drop — release one reference held by `v`.
    Drop { v: String },
    /// Build a closure value named `name` that CAPTURES `captures` and whose
    /// body BORROW-reads each captured var (appending to output). The capture
    /// list records, per var, the ownership decision made for the capture.
    MakeClosure { name: String, captures: Vec<Capture> },
    /// Call the closure `name` (reads its captures, no consumption of the
    /// closure — it stays callable).
    CallClosure { name: String },
    /// The closure value itself drops at scope end: its environment releases
    /// every captured ref (one Drop per capture). Single decision, honored.
    DropClosure { name: String },
}

/// One captured variable + the ownership decision for the capture.
#[derive(Clone, Debug)]
struct Capture {
    var: String,
    /// THE decision: a faithful capture of a still-live value is a Dup-into-env
    /// (`Use::Borrow` here means "the body only reads it" → env keeps an owned
    /// ref via Dup, callable repeatedly). A buggy renderer would mis-model the
    /// capture as `Use::Consume` (move x into the env AND dec it on first use),
    /// which is FnOnce and breaks the second call.
    how: Use,
}

struct MirProgram {
    body: Vec<Op>,
    /// faithful = honor the MIR (Dup at capture, Drop-env at closure-drop,
    /// each call borrows). false = the buggy re-deciding renderer (treats the
    /// capture as consumed on first call → second call hits a freed cell).
    faithful: bool,
}

// ───────────── The closure_capture shape in MIR ─────────────
//
// let x = make_heap()
// let f = || use(x)        // capture x by Dup-into-env; body borrows x
// call f()                 // borrow read #1
// call f()                 // borrow read #2  (needs Fn, not FnOnce)
// (scope end) drop f       // env releases its captured ref (Drop x)
// (scope end) drop x       // the original binding releases its ref

fn shape_closure_capture() -> Vec<Op> {
    vec![
        Op::Let {
            v: "x".into(),
            repr: Repr::Heap,
            rhs: Rhs::FreshHeap { call: "\"captured\"".into() },
        },
        // THE decision: the closure captures x. A faithful capture of a
        // still-live value is a Dup into the closure environment; the body
        // only reads x, so each call is a Borrow.
        Op::MakeClosure {
            name: "f".into(),
            captures: vec![Capture { var: "x".into(), how: Use::Borrow }],
        },
        Op::CallClosure { name: "f".into() }, // call #1
        Op::CallClosure { name: "f".into() }, // call #2
        // scope end: the closure value drops → env releases captured ref.
        Op::DropClosure { name: "f".into() },
        // the original `x` binding drops too.
        Op::Drop { v: "x".into() },
    ]
}

// ───────────────────────── Renderer A: idiomatic Rust ─────────────────────
//
// Heap = owned `String`. The closure is a `move ||` that captures x (the env
// owns its String — that IS the Dup-into-env, rendered idiomatically as a
// clone-in / move of an owned value). The body BORROWS x on each call (reads
// `&x` to push onto a shared output buffer), so the closure is `Fn` and is
// callable twice. Drop = scope end (Rust drops the closure env). NO Rc, NO
// RefCell: a read-only capture needs neither.

fn render_idiomatic(p: &MirProgram) -> String {
    let mut s = String::new();
    let _ = writeln!(s, "fn main() {{");
    let _ = writeln!(s, "    let mut out: Vec<String> = Vec::new();");
    for op in &p.body {
        match op {
            Op::Let { v, rhs: Rhs::FreshHeap { call }, .. } => {
                let _ = writeln!(s, "    let {v}: String = {call}.to_string();");
            }
            // Dup-into-env is expressed by the `move` closure capturing an
            // owned clone of x. We clone x INTO the closure so the original
            // binding `x` survives to its own Drop (mirrors the env's own ref
            // distinct from the original ref — two refs, two drops).
            Op::MakeClosure { name, captures } => {
                // Each captured var: clone into the env (Dup-into-env). The
                // `move ||` then owns these env copies. The original `x`
                // binding survives to its own Drop (two refs, two drops) —
                // exactly mirroring the RC env's own ref distinct from the
                // original. The closure RETURNS the read value (a Fn closure),
                // so the body only READS the capture — it does NOT capture the
                // output channel. This isolates the capture decision under test
                // from the body's side effect.
                for c in captures {
                    let _ = writeln!(s, "    let {var}_env = {var}.clone(); // Dup x into env", var = c.var);
                }
                let body_reads: Vec<String> = captures
                    .iter()
                    .map(|c| {
                        if p.faithful {
                            // Borrow: read &x_env each call → Fn, callable any
                            // number of times. Returns a clone of the read.
                            format!("(&{var}_env).clone()", var = c.var)
                        } else {
                            // BUGGY re-decide: consume the capture in the body
                            // (move x_env out by value) → this is FnOnce; rustc
                            // rejects the second call at COMPILE time.
                            format!("{var}_env", var = c.var)
                        }
                    })
                    .collect();
                let _ = writeln!(s, "    let {name} = move || {{ {} }};", body_reads.join(" "));
            }
            // Each call reads the capture; main collects the returned read.
            Op::CallClosure { name } => { let _ = writeln!(s, "    out.push({name}());"); }
            // The closure env drops at scope end (Rust); the per-var env clone
            // is released here implicitly. Nothing to emit.
            Op::DropClosure { .. } => {}
            // The original binding drops at scope end (Rust). Nothing to emit.
            Op::Drop { .. } => {}
            Op::Dup { .. } => {}
        }
    }
    let _ = writeln!(s, "    println!(\"{{}}\", out.join(\",\"));");
    let _ = writeln!(s, "}}");
    s
}

// ───────────────────────── Renderer B: manual reference-counted ──────────────
//
// Models the wasm RC semantics in Rust with a HAND-WRITTEN heap (NOT std Rc):
// cells carry an explicit refcount; inc/dec are manual; a cell hitting 0 frees
// (live--); a dec below 0 is a DOUBLE FREE (sentinel, exit 8); a never-dec'd
// ref LEAKS (live != 0 at end, exit 7). This is the discipline the wasm emit
// hand-writes.
//
// The closure ENV is modeled as a struct holding the captured cell pointers.
// capture(x) = inc(x) into the env. CallClosure = read each captured cell
// (Borrow, no count change). DropClosure = dec each captured cell (the env
// releases its refs). The original `x` Drop = its own dec.
//
// faithful=false (the buggy re-deciding renderer): the env dec's x on the
// FIRST call (mis-modeling the capture as consumed), so call #2 reads a cell
// already at rc 0 and the teardown dec's it again → DOUBLE FREE.

fn render_rc(p: &MirProgram) -> String {
    let mut s = String::new();
    let _ = writeln!(s, "struct Heap {{ cells: Vec<(String, i32, bool)>, live: i64 }}");
    let _ = writeln!(s, "impl Heap {{");
    let _ = writeln!(s, "    fn alloc(&mut self, s: &str) -> usize {{ self.cells.push((s.to_string(), 1, true)); self.live += 1; self.cells.len()-1 }}");
    let _ = writeln!(s, "    fn inc(&mut self, p: usize) {{");
    let _ = writeln!(s, "        if !self.cells[p].2 {{ eprintln!(\"USE AFTER FREE (inc) at cell {{}}\", p); std::process::exit(9); }}");
    let _ = writeln!(s, "        self.cells[p].1 += 1;");
    let _ = writeln!(s, "    }}");
    let _ = writeln!(s, "    fn dec(&mut self, p: usize) {{");
    let _ = writeln!(s, "        if !self.cells[p].2 {{ eprintln!(\"DOUBLE FREE at cell {{}}\", p); std::process::exit(8); }}");
    let _ = writeln!(s, "        self.cells[p].1 -= 1;");
    let _ = writeln!(s, "        if self.cells[p].1 == 0 {{ self.cells[p].2 = false; self.live -= 1; }}");
    let _ = writeln!(s, "        if self.cells[p].1 < 0 {{ eprintln!(\"REFCOUNT NEGATIVE at cell {{}}\", p); std::process::exit(8); }}");
    let _ = writeln!(s, "    }}");
    let _ = writeln!(s, "    fn read(&self, p: usize) -> String {{");
    let _ = writeln!(s, "        if !self.cells[p].2 {{ eprintln!(\"USE AFTER FREE (read) at cell {{}}\", p); std::process::exit(9); }}");
    let _ = writeln!(s, "        self.cells[p].0.clone()");
    let _ = writeln!(s, "    }}");
    let _ = writeln!(s, "}}");
    // The closure environment: holds captured cell pointers.
    let _ = writeln!(s, "struct Env {{ caps: Vec<usize> }}");
    let _ = writeln!(s, "fn main() {{");
    let _ = writeln!(s, "    let mut h = Heap {{ cells: Vec::new(), live: 0 }};");
    let _ = writeln!(s, "    let mut out: Vec<String> = Vec::new();");
    for op in &p.body {
        match op {
            Op::Let { v, rhs: Rhs::FreshHeap { call }, .. } => {
                let _ = writeln!(s, "    let {v} = h.alloc({call});");
            }
            Op::MakeClosure { name, captures } => {
                // capture(x) = inc(x) into the env (the env's own owned ref).
                let caps: Vec<String> = captures.iter().map(|c| c.var.clone()).collect();
                for c in captures {
                    let _ = writeln!(s, "    h.inc({var}); // Dup x into closure env", var = c.var);
                }
                let _ = writeln!(s, "    let {name}_env = Env {{ caps: vec![{}] }};", caps.join(", "));
                let _ = writeln!(s, "    let mut {name}_dropped = false;");
            }
            Op::CallClosure { name } => {
                // Each call BORROW-reads every captured cell (no count change).
                let _ = writeln!(s, "    for &cp in &{name}_env.caps {{ out.push(h.read(cp)); }} // borrow read");
                if !p.faithful {
                    // BUGGY re-decide: treat the capture as CONSUMED on use →
                    // dec the captured cell here. This is the renderer placing
                    // a drop the MIR did not (the #643 class: re-deciding
                    // ownership). The second call then hits a freed cell, and
                    // teardown dec's it again → double free.
                    let _ = writeln!(s, "    for &cp in &{name}_env.caps {{ h.dec(cp); }} // (buggy) capture consumed on call");
                }
            }
            Op::DropClosure { name } => {
                if p.faithful {
                    // The env releases each captured ref exactly once.
                    let _ = writeln!(s, "    for &cp in &{name}_env.caps {{ h.dec(cp); }} // Drop env: release captured refs");
                } else {
                    // The buggy renderer already (wrongly) dec'd on each call;
                    // it omits the env-drop dec (it thinks the capture is gone).
                    let _ = writeln!(s, "    // (buggy) DropClosure({name}) omitted — env thinks capture was consumed");
                }
            }
            Op::Drop { v } => {
                // The original binding releases its own ref.
                let _ = writeln!(s, "    h.dec({v}); // Drop original x");
            }
            Op::Dup { v } => { let _ = writeln!(s, "    h.inc({v});"); }
        }
    }
    let _ = writeln!(s, "    println!(\"{{}}\", out.join(\",\"));");
    let _ = writeln!(s, "    if h.live != 0 {{ eprintln!(\"RC IMBALANCE: {{}} cell(s) leaked\", h.live); std::process::exit(7); }}");
    let _ = writeln!(s, "}}");
    s
}

// ───────────────────────── Harness: compile + run + compare ───────────────

fn compile_and_run(label: &str, src: &str) -> (bool, String, String) {
    let dir = std::env::temp_dir().join(format!("v1gate_cc_{label}"));
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
    println!("== Phase 0 spike: closure_capture shape — one MIR decision, two faithful renderings ==\n");

    // Faithful MIR-driven program.
    let faithful = MirProgram { body: shape_closure_capture(), faithful: true };
    let idiom = render_idiomatic(&faithful);
    let rc = render_rc(&faithful);

    println!("---- rendered idiomatic Rust ----\n{idiom}");
    println!("---- rendered manual-RC Rust ----\n{rc}");

    let (i_ok, i_out, i_err) = compile_and_run("idiom", &idiom);
    let (r_ok, r_out, r_err) = compile_and_run("rc", &rc);

    println!("[idiomatic Rust] compile_ok={i_ok} out={i_out:?} {}", if i_err.is_empty() { String::new() } else { format!("err={i_err}") });
    println!("[RC (wasm sem.)] compile_ok={r_ok} out={r_out:?} {}", if r_err.is_empty() { String::new() } else { format!("err={r_err}") });

    let agree = i_ok && r_ok && i_out == r_out;
    println!("\n  → render agreement: {}", if agree { "YES (by construction)" } else { "NO" });

    // Contrast: the BUGGY re-deciding renderer (capture mis-modeled as consumed
    // on first call). Idiomatic side fails to COMPILE (FnOnce called twice);
    // RC side double-frees at runtime. Either way the bug is the RENDERER
    // re-deciding ownership — exactly #643's class — not the MIR.
    let buggy = MirProgram { body: shape_closure_capture(), faithful: false };
    let idiom_buggy = render_idiomatic(&buggy);
    let rc_buggy = render_rc(&buggy);
    let (bi_ok, bi_out, bi_err) = compile_and_run("idiom_buggy", &idiom_buggy);
    let (br_ok, br_out, br_err) = compile_and_run("rc_buggy", &rc_buggy);
    let idiom_buggy_caught = !bi_ok; // FnOnce-called-twice → compile error
    let rc_buggy_caught = br_err.contains("DOUBLE FREE")
        || br_err.contains("USE AFTER FREE")
        || br_err.contains("RC IMBALANCE")
        || !br_ok;
    println!("\n  → buggy idiomatic (capture consumed → FnOnce called twice): {} (compile_ok={bi_ok})",
        if idiom_buggy_caught { "REJECTED by rustc" } else { "unexpectedly ok" });
    println!("  → buggy RC (capture consumed on call → 2nd call/​teardown): {} (out={br_out:?} err={br_err:?})",
        if rc_buggy_caught { "DOUBLE FREE / use-after-free as #643 class" } else { "unexpectedly ok" });

    println!("\n== DECISION GATE (closure_capture shape) ==");
    let faithful_clean = r_ok && r_err.is_empty(); // RC: no leak, no double free
    let buggy_caught = idiom_buggy_caught && rc_buggy_caught;
    let no_escape_hatch = !idiom.contains("Rc<") && !idiom.contains("RefCell") && !idiom.contains("Rc::");
    let pass = agree && faithful_clean && buggy_caught && no_escape_hatch;
    println!("  one balanced MIR decision → idiomatic Rust AND RC agree: {}", agree);
    println!("  faithful RC is leak/double-free clean: {}", faithful_clean);
    println!("  buggy re-deciding renderer is caught (rustc reject / RC double-free): {}", buggy_caught);
    println!("  idiomatic Rust uses NO Rc/RefCell for the shared read-only capture: {}", no_escape_hatch);
    println!("  VERDICT: {}", if pass {
        "PASS — capture=Dup-into-env+Drop-env renders cleanly to a plain move-Fn closure AND the RC env, agreeing, no escape hatch"
    } else {
        "FAIL/CONDITIONAL — investigate"
    });
    std::process::exit(if pass { 0 } else { 1 });
}
