//! Phase 0 spike — shape #5 (AliasCow), the "real test" per
//! docs/roadmap/active/v1-mir-architecture.md §8.
//!
//! Almide source:
//!     var a = list[1, 2]
//!     var b = a            // b aliases a (mutable alias)
//!     list.push(b, 3)      // mutate THROUGH the possibly-shared ref
//!     println(a)           // value semantics: a must stay [1, 2]
//!
//! THE ONE OWNERSHIP DECISION (the AliasCow row of §2.2):
//!   "a mutation through a possibly-shared ref must MAKE-UNIQUE first."
//!   - idiomatic Rust : clone-on-alias  (`b = a.clone()`)  → b owns its buffer
//!   - manual RC      : `cow_check` at the mutation site    → if rc>1, clone the
//!                      cell (and dec the shared one) so the write hits a uniquely
//!                      owned cell. rc==1 ⇒ mutate in place.
//!
//! Both spellings are the SAME decision: "writer must hold a unique buffer." We
//! render that one MakeUnique node to both idioms and check they AGREE that
//! a == [1, 2], and that the manual-RC form is leak/double-free clean while a
//! buggy re-deciding renderer (skip cow_check → write the shared cell) both
//! corrupts `a` AND breaks RC discipline.
//!
//! Run: `rustc --edition 2021 -O main.rs -o spike && ./spike`

#![allow(dead_code, unused_variables)]

use std::fmt::Write as _;
use std::process::Command;

// ───────────────────────── Minimal MIR ─────────────────────────
//
// Just enough to express the AliasCow shape. The KEY property: the ownership
// decision (where to MakeUnique) is EXPLICIT here, decided once. Renderers only
// translate it; they never re-decide.

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Repr {
    Scalar, // Copy (i64)
    Heap,   // list / String / … — RC: a cell; Rust: an owned Vec
}

/// The single ownership-relevant fact at the alias point: is the binding a fresh
/// owner, or does it alias a still-live mutable value?
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Bind {
    /// `b = a` where `a` stays live and `b` may later be mutated. RC: share the
    /// cell (inc). Rust: this is the alias whose later write forces MakeUnique.
    AliasMut,
}

#[derive(Clone, Debug)]
enum Op {
    /// `var v = list[..]` — fresh heap value, sole owner (RC rc=1).
    NewList { v: String, elems: Vec<i64> },
    /// `var b = a` — alias a still-live mutable value.
    AliasBind { v: String, of: String, bind: Bind },
    /// THE decision: before mutating `v` (which may be shared), ensure `v` holds
    /// a UNIQUE buffer. RC: cow_check (rc>1 ⇒ clone+dec-shared). Rust: the alias
    /// that produced `v` was a clone, so `v` is already unique (no-op here).
    MakeUnique { v: String },
    /// mutate `v` in place (push). PRECONDITION: `v` is unique (MakeUnique ran).
    Push { v: String, x: i64 },
    /// observe `v` (println). Borrow.
    Println { v: String },
    /// Perceus drop — release one ref held by `v` at scope end.
    Drop { v: String },
}

/// The MIR body for the AliasCow shape, decided ONCE.
fn shape_alias_cow() -> Vec<Op> {
    vec![
        Op::NewList { v: "a".into(), elems: vec![1, 2] },
        Op::AliasBind { v: "b".into(), of: "a".into(), bind: Bind::AliasMut },
        // THE single ownership decision: the write below goes through a possibly
        // -shared ref, so make `b` unique first.
        Op::MakeUnique { v: "b".into() },
        Op::Push { v: "b".into(), x: 3 },
        Op::Println { v: "a".into() }, // value semantics: a is still [1, 2]
        Op::Drop { v: "b".into() },
        Op::Drop { v: "a".into() },
    ]
}

// ───────────────── Renderer A: idiomatic Rust ─────────────────
//
// Heap = owned `Vec<i64>`. AliasMut whose value is later mutated → clone-on-alias
// (`b = a.clone()`), so MakeUnique is already satisfied (no-op). Push → in-place.
// Drop → scope end (no-op). a is observed unchanged BY OWNERSHIP (b has its own
// buffer).

fn render_idiomatic(faithful: bool) -> String {
    let body = shape_alias_cow();
    let mut s = String::new();
    let _ = writeln!(s, "fn main() {{");
    for op in &body {
        match op {
            Op::NewList { v, elems } => {
                let lits = elems.iter().map(|e| e.to_string()).collect::<Vec<_>>().join(", ");
                let _ = writeln!(s, "    let mut {v}: Vec<i64> = vec![{lits}];");
            }
            Op::AliasBind { v, of, .. } => {
                if faithful {
                    // MakeUnique-by-clone folded into the alias: b owns its buffer.
                    let _ = writeln!(s, "    let mut {v}: Vec<i64> = {of}.clone();");
                } else {
                    // BUGGY re-decide: alias by &mut (no clone) → write hits a's
                    // buffer. The renderer re-decided ownership instead of honoring
                    // MakeUnique. (Mirrors emit_wasm skipping cow_check.)
                    let _ = writeln!(s, "    let {v}: &mut Vec<i64> = &mut {of};");
                }
            }
            // MakeUnique: in the faithful idiomatic form the alias already cloned,
            // so this is a no-op. (The decision is HONORED, just folded.)
            Op::MakeUnique { .. } => {}
            Op::Push { v, x } => { let _ = writeln!(s, "    {v}.push({x});"); }
            Op::Println { v } => { let _ = writeln!(s, "    println!(\"{{:?}}\", {v});"); }
            Op::Drop { .. } => {} // Rust drops at scope end
        }
    }
    let _ = writeln!(s, "}}");
    s
}

// ───────────────── Renderer B: manual reference-counted (wasm sem.) ─────────
//
// Models the wasm RC + copy-on-write runtime in Rust. Heap = a handle (cell
// index) into a Heap whose cells carry an EXPLICIT refcount and the list buffer.
//   AliasMut  → inc (share the cell): rc 1→2.
//   MakeUnique→ cow_check: if rc>1, clone the cell into a fresh rc=1 cell, dec the
//               shared one, and rebind the handle. rc==1 ⇒ no-op.
//   Push      → mutate the cell's buffer in place (sound because cow_check ran).
//   Drop      → dec; rc 0 ⇒ free.
// A balanced program reaches live==0 with no dec-below-zero. The buggy variant
// SKIPS cow_check (writes the shared cell) → corrupts `a` AND leaves an extra
// live ref / unbalanced dec (the AliasCow analogue of the #643 drift).

fn render_rc(faithful: bool) -> String {
    let body = shape_alias_cow();
    let mut s = String::new();
    // Manual RC+COW heap faithfully modelling the wasm runtime: cells carry an
    // explicit refcount + the list buffer; inc/dec are MANUAL; rc→0 frees; a dec
    // below 0 is a DOUBLE FREE; a live cell at end is a LEAK.
    let _ = writeln!(s, "struct Heap {{ cells: Vec<(Vec<i64>, i32, bool)>, live: i64 }}");
    let _ = writeln!(s, "impl Heap {{");
    let _ = writeln!(s, "    fn alloc(&mut self, v: Vec<i64>) -> usize {{ self.cells.push((v, 1, true)); self.live += 1; self.cells.len()-1 }}");
    let _ = writeln!(s, "    fn inc(&mut self, p: usize) {{ self.cells[p].1 += 1; }}");
    let _ = writeln!(s, "    fn dec(&mut self, p: usize) {{");
    let _ = writeln!(s, "        self.cells[p].1 -= 1;");
    let _ = writeln!(s, "        if self.cells[p].1 < 0 {{ eprintln!(\"DOUBLE FREE at cell {{}}\", p); std::process::exit(8); }}");
    let _ = writeln!(s, "        if self.cells[p].1 == 0 {{ if !self.cells[p].2 {{ eprintln!(\"DOUBLE FREE (resurrected) at cell {{}}\", p); std::process::exit(8); }} self.cells[p].2 = false; self.live -= 1; }}");
    let _ = writeln!(s, "    }}");
    // cow_check: the single ownership decision, rendered as a real RC primitive.
    let _ = writeln!(s, "    fn cow_check(&mut self, p: usize) -> usize {{");
    let _ = writeln!(s, "        if self.cells[p].1 > 1 {{ let copy = self.cells[p].0.clone(); let np = self.alloc(copy); self.dec(p); np }} else {{ p }}");
    let _ = writeln!(s, "    }}");
    let _ = writeln!(s, "    fn push(&mut self, p: usize, x: i64) {{ self.cells[p].0.push(x); }}");
    let _ = writeln!(s, "    fn get(&self, p: usize) -> Vec<i64> {{ self.cells[p].0.clone() }}");
    let _ = writeln!(s, "}}");
    let _ = writeln!(s, "fn main() {{");
    let _ = writeln!(s, "    let mut h = Heap {{ cells: Vec::new(), live: 0 }};");
    for op in &body {
        match op {
            Op::NewList { v, elems } => {
                let lits = elems.iter().map(|e| e.to_string()).collect::<Vec<_>>().join(", ");
                let _ = writeln!(s, "    let mut {v}: usize = h.alloc(vec![{lits}]);");
            }
            Op::AliasBind { v, of, .. } => {
                // AliasMut: share the cell (inc). Both faithful and buggy share —
                // the difference is whether the WRITE makes-unique.
                let _ = writeln!(s, "    let mut {v}: usize = {of}; h.inc({v});");
            }
            Op::MakeUnique { v } => {
                if faithful {
                    let _ = writeln!(s, "    {v} = h.cow_check({v});");
                } else {
                    // BUGGY re-decide: skip cow_check → the push below writes the
                    // SHARED cell. The renderer re-decided ownership (omitted the
                    // MakeUnique) — the AliasCow analogue of the #643 imbalance.
                    let _ = writeln!(s, "    // (buggy) cow_check({v}) omitted — write hits the shared cell");
                }
            }
            Op::Push { v, x } => { let _ = writeln!(s, "    h.push({v}, {x});"); }
            Op::Println { v } => { let _ = writeln!(s, "    println!(\"{{:?}}\", h.get({v}));"); }
            Op::Drop { v } => { let _ = writeln!(s, "    h.dec({v});"); }
        }
    }
    let _ = writeln!(s, "    if h.live != 0 {{ eprintln!(\"RC IMBALANCE: {{}} cell(s) leaked\", h.live); std::process::exit(7); }}");
    let _ = writeln!(s, "}}");
    s
}

// ───────────────── Harness: compile + run ─────────────────

fn compile_and_run(label: &str, src: &str) -> (bool, String, String) {
    let dir = std::env::temp_dir().join("v1gate_alias_cow").join(label);
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
    println!("== Phase 0 spike: AliasCow shape — one MIR decision (MakeUnique), two faithful renderings ==\n");

    let idiom = render_idiomatic(true);
    let rc = render_rc(true);
    println!("--- idiomatic Rust source ---\n{idiom}");
    println!("--- manual-RC source ---\n{rc}");

    let (i_ok, i_out, i_err) = compile_and_run("idiom", &idiom);
    let (r_ok, r_out, r_err) = compile_and_run("rc", &rc);

    println!("[idiomatic Rust] compile_ok={i_ok} out={i_out:?} {}", if i_err.is_empty() { String::new() } else { format!("err={i_err}") });
    println!("[RC (wasm sem.)] compile_ok={r_ok} out={r_out:?} {}", if r_err.is_empty() { String::new() } else { format!("err={r_err}") });

    let want = "[1, 2]"; // value semantics: a unchanged by the push through b
    let agree = i_ok && r_ok && i_out == r_out;
    let correct = i_out == want && r_out == want;
    println!("\n  → render agreement: {} (both {:?})", if agree { "YES (by construction)" } else { "NO" }, i_out);
    println!("  → value semantics (a == [1, 2]): {}", if correct { "YES" } else { "NO" });

    // Contrast: a renderer that RE-DECIDES (skips MakeUnique / cow_check).
    let idiom_bug = render_idiomatic(false);
    let rc_bug = render_rc(false);
    let (ib_ok, ib_out, ib_err) = compile_and_run("idiom_bug", &idiom_bug);
    let (rb_ok, rb_out, rb_err) = compile_and_run("rc_bug", &rc_bug);
    println!("\n  → buggy idiomatic (alias by &mut, no clone): compile_ok={ib_ok} out={ib_out:?} {}",
        if ib_err.is_empty() { String::new() } else { format!("err={ib_err}") });
    println!("  → buggy RC (cow_check omitted): compile_ok={rb_ok} out={rb_out:?} {}",
        if rb_err.is_empty() { String::new() } else { format!("err={rb_err}") });

    // The omitted decision (skip MakeUnique / cow_check) must corrupt `a` on
    // BOTH idioms identically — that is what proves the decision is shared. Note
    // (honest): for AliasCow, skipping cow_check is a VALUE-semantics violation,
    // not an RC-count imbalance — both handles still point at one cell and each is
    // dec'd once, so the buggy RC stays count-balanced (live==0) while printing the
    // WRONG value. (Contrast with #643, where the renderer-re-decides bug WAS a
    // count leak. The class detected is the same — "renderer re-decided ownership"
    // — but its observable signature here is corrupted output, not a leak.)
    let bug_both_corrupt = ib_out == "[1, 2, 3]" && rb_out == "[1, 2, 3]"; // a mutated through b
    let bug_agree = ib_out == rb_out;                                       // same wrong answer
    let rc_bug_count_balanced = rb_ok && rb_err.is_empty();                 // not an RC leak (honest)
    println!("  → buggy idiom & RC both corrupt a to [1,2,3]: {}  (agree: {})", bug_both_corrupt, bug_agree);
    println!("  → buggy RC count-balanced (corruption is value-sem, not a leak): {}", rc_bug_count_balanced);

    println!("\n== DECISION GATE (AliasCow shape) ==");
    let faithful_rc_clean = r_ok && r_err.is_empty(); // no leak/double-free on the FAITHFUL path
    let gate = agree && correct && faithful_rc_clean && bug_both_corrupt && bug_agree;
    println!("  (1) idiomatic Rust AND RC agree on a == [1,2] from one MakeUnique: {}", agree && correct);
    println!("  (2) faithful RC is leak/double-free clean: {}", faithful_rc_clean);
    println!("  (3) omitting MakeUnique corrupts a IDENTICALLY on both idioms: {}", bug_both_corrupt && bug_agree);
    println!("  VERDICT: {}", if gate { "PASS — MakeUnique (clone | cow_check) is ONE decision, two faithful idioms" } else { "FAIL — investigate" });
    std::process::exit(if gate { 0 } else { 1 });
}
