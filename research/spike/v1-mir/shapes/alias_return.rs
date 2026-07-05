//! Phase 0 spike — shape #1 (alias_return) for
//! docs/roadmap/active/v1-mir-architecture.md §8.
//!
//! SHAPE:  fn first(o: Option[String]) -> String =
//!             match o { some(s) => s, none => "" }
//!
//! The returned value ALIASES the Option payload. The single Perceus ownership
//! decision for this shape (§2.2 "最後の consume"): `o` is consumed by the
//! match (its last use), so the `some(s) => s` arm MOVES the payload out of the
//! box and frees ONLY the box shell — the payload's single owned ref is
//! transferred to the caller. No dup of the payload, no free of the payload.
//!
//!   MIR (Perceus)          | Rust renderer        | RC (wasm-sem) renderer
//!   -----------------------+----------------------+------------------------------
//!   match consumes `o`     | match by value       | (we own the Option cell)
//!   some(s) => Consume s   | move `s` out (return)| transfer payload ptr (NO inc)
//!   free box shell         | (Rust drops the      | dec the Option *wrapper* cell
//!     (payload survives)   |  emptied box)        |  WITHOUT recursing into payload
//!   none => fresh ""       | "".to_string()       | h.alloc("")
//!
//! THE one decision: "the payload's ref is TRANSFERRED, the shell is freed."
//! A renderer that re-decides — e.g. frees the payload too (recursive box
//! free) OR dups the payload while also freeing the shell's child — diverges:
//! double-free or leak. The buggy variant reproduces the double-free (the
//! shell free recurses into the still-returned payload), which is exactly the
//! aliasing hazard #643's class warns about.
//!
//! Scope note (same as the #643 spike): we model the wasm RC *semantics* in
//! Rust with a manual refcount Heap (NOT std Rc); cells carry an explicit
//! count, inc/dec are manual, a count reaching 0 frees, a dec below 0 is a
//! DOUBLE FREE sentinel, and a live cell at teardown is a LEAK. Both renderings
//! compile with one compiler (rustc) and must AGREE on observable output.
//!
//! Run: `rustc --edition 2021 -O main.rs -o main && ./main`

#![allow(dead_code, unused_variables)]

use std::fmt::Write as _;
use std::process::Command;

// ───────────────────────── Minimal MIR ─────────────────────────
//
// Just enough to express `first`. The ownership decision (how the some-arm
// treats the payload, and what happens to the box shell) is EXPLICIT and
// decided ONCE here. Renderers translate; they never re-decide.

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Repr {
    Scalar, // Copy
    Heap,   // String — owned ref (RC: a cell; Rust: owned String)
    Boxed,  // Option[Heap] — a wrapper cell that OWNS a payload cell
}

/// How the some-arm treats the payload it binds.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum PayloadDecision {
    /// `o` is consumed (last use): MOVE the payload out, transfer its ref,
    /// free ONLY the shell. (The canonical Perceus "last consume".)
    ConsumeMoveOut,
    /// `o` is borrowed: DUP the payload (own ref), leave the box intact.
    BorrowDup,
}

/// What the renderer does to the Option shell after extracting the payload.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum ShellFate {
    /// free the shell WITHOUT touching the moved-out payload (faithful for
    /// ConsumeMoveOut: payload already transferred).
    FreeShellOnly,
    /// (BUGGY re-decision) free the shell AND recurse into the payload —
    /// double-frees the very ref we returned. The aliasing hazard.
    FreeShellRecursive,
    /// leave the shell alive (faithful for BorrowDup: the box still owns it).
    KeepShell,
}

/// The single MIR fact for the some-arm of `first`.
#[derive(Clone, Copy, Debug)]
struct AliasReturnDecision {
    payload: PayloadDecision,
    shell: ShellFate,
}

// The canonical (faithful) decision: consume the Option, move the payload out,
// free only the shell. This is the ONE decision both renderers honor.
fn faithful_consume() -> AliasReturnDecision {
    AliasReturnDecision { payload: PayloadDecision::ConsumeMoveOut, shell: ShellFate::FreeShellOnly }
}

// The BUGGY re-decision: same move-out, but the shell free recurses into the
// payload (a renderer "helpfully" deep-freeing the box) → double-free of the
// returned ref. This is the renderer RE-DECIDING ownership = #643's class.
fn buggy_recursive_free() -> AliasReturnDecision {
    AliasReturnDecision { payload: PayloadDecision::ConsumeMoveOut, shell: ShellFate::FreeShellRecursive }
}

// ───────────────────────── Renderer A: idiomatic Rust ─────────────────────
//
// Heap = owned `String`; Option[Heap] = `Option<String>`. ConsumeMoveOut →
// match by value and return the bound `s` (a move). FreeShellOnly → Rust drops
// the emptied Option automatically (no-op to write). `none => ""`.

fn render_idiomatic(d: AliasReturnDecision) -> String {
    let mut s = String::new();
    let _ = writeln!(s, "fn first(o: Option<String>) -> String {{");
    let _ = writeln!(s, "    match o {{");
    match d.payload {
        // `o` consumed by value → the bound `s` is a move out of the box;
        // returning it transfers ownership. The emptied Option drops (shell
        // free) on its own. No clone, fully idiomatic.
        PayloadDecision::ConsumeMoveOut => {
            let _ = writeln!(s, "        Some(s) => s,");
        }
        // borrowed form would clone; not the canonical path here.
        PayloadDecision::BorrowDup => {
            let _ = writeln!(s, "        Some(ref s) => s.clone(),");
        }
    }
    let _ = writeln!(s, "        None => \"\".to_string(),");
    let _ = writeln!(s, "    }}");
    let _ = writeln!(s, "}}");
    let _ = writeln!(s, "fn main() {{");
    // exercise both arms: a Some-with-payload and a None.
    let _ = writeln!(s, "    let a = first(Some(\"hi\".to_string()));");
    let _ = writeln!(s, "    let b = first(None);");
    let _ = writeln!(s, "    println!(\"{{}}|{{}}|len={{}}\", a, if b.is_empty() {{ \"<empty>\" }} else {{ &b }}, a.len());");
    let _ = writeln!(s, "}}");
    s
}

// ───────────────────────── Renderer B: reference-counted ──────────────────
//
// Models the wasm RC semantics. Heap String = a refcounted cell. Option[Heap]
// = a "shell" cell that, when alive, OWNS exactly one payload cell (it holds
// the payload's ptr and contributes its one ref). Freeing a shell normally
// must decide: does the payload travel out (ConsumeMoveOut: shell free does NOT
// dec payload, because the ref left with the return value) or stay (the shell
// owns it; freeing the shell decs the payload)?
//
// THE decision lives in `payload`+`shell`. The faithful path = move out + free
// shell only. The buggy path = free shell recursively → double-frees payload.

fn render_rc(d: AliasReturnDecision) -> String {
    let mut s = String::new();
    // Manual RC heap faithfully modelling the wasm runtime: explicit counts,
    // manual inc/dec, 0 frees, <0 is DOUBLE FREE, live!=0 at end is a LEAK.
    // A cell carries (string-or-"", refcount). The Option box is ALSO a heap
    // cell here (a real shell with its own refcount, holding the payload ptr),
    // so "free the shell" is a real `dec` that must NOT cascade into the
    // payload — that distinction is precisely the ownership decision under test.
    let _ = writeln!(s, "struct Heap {{ cells: Vec<(String, i32)>, live: i64 }}");
    let _ = writeln!(s, "impl Heap {{");
    let _ = writeln!(s, "    fn alloc(&mut self, s: &str) -> usize {{ self.cells.push((s.to_string(), 1)); self.live += 1; self.cells.len()-1 }}");
    let _ = writeln!(s, "    fn inc(&mut self, p: usize) {{ self.cells[p].1 += 1; }}");
    let _ = writeln!(s, "    fn dec(&mut self, p: usize) {{ self.cells[p].1 -= 1; if self.cells[p].1 == 0 {{ self.live -= 1; }} if self.cells[p].1 < 0 {{ eprintln!(\"DOUBLE FREE at cell {{}}\", p); std::process::exit(8); }} }}");
    let _ = writeln!(s, "    fn get(&self, p: usize) -> String {{ self.cells[p].0.clone() }}");
    let _ = writeln!(s, "}}");
    // The Option shell is a HEAP cell carrying the payload ptr (or a None
    // sentinel). `shell_payload` reads the ptr it owns; `free_shell` decs the
    // shell cell, and — per the decision — either leaves the payload alone
    // (faithful) or recurses into it (buggy deep-free).
    let _ = writeln!(s, "const NONE_TAG: usize = usize::MAX; // shell holds NONE_TAG when empty");
    let _ = writeln!(s, "");
    // `first` takes the shell ptr (owned: its last use) and returns a payload
    // ptr — an owned ref handed to the caller.
    let _ = writeln!(s, "fn first(h: &mut Heap, shell: usize) -> usize {{");
    // Read the payload ptr the shell owns. The shell stores the ptr as text so
    // the manual heap stays uniform; NONE shells store the NONE_TAG sentinel.
    let _ = writeln!(s, "    let pay: usize = h.get(shell).parse().unwrap();");
    let _ = writeln!(s, "    if pay == NONE_TAG {{");
    // none arm: free the (empty) shell, return a fresh "" cell.
    let _ = writeln!(s, "        h.dec(shell); // shell consumed (last use)");
    let _ = writeln!(s, "        return h.alloc(\"\");");
    let _ = writeln!(s, "    }}");
    let _ = writeln!(s, "    // some arm — THE decision:");
    match d.payload {
        PayloadDecision::ConsumeMoveOut => {
            // Move the payload ptr out — its ref TRANSFERS to the return value
            // (no inc). The shell is consumed; how we free it is `shell`.
            match d.shell {
                ShellFate::FreeShellOnly => {
                    // Faithful: dec ONLY the shell cell. The payload's one ref
                    // left with the return value, untouched.
                    let _ = writeln!(s, "    h.dec(shell); // free shell ONLY; payload ref transferred out");
                    let _ = writeln!(s, "    pay");
                }
                ShellFate::FreeShellRecursive => {
                    // BUGGY re-decision: deep-free the box — dec the shell AND
                    // the payload it just handed out → double free when the
                    // caller later drops its (now-stale) ref.
                    let _ = writeln!(s, "    h.dec(pay);   // (buggy) recursive free decs the returned payload");
                    let _ = writeln!(s, "    h.dec(shell);");
                    let _ = writeln!(s, "    pay");
                }
                ShellFate::KeepShell => {
                    let _ = writeln!(s, "    pay");
                }
            }
        }
        PayloadDecision::BorrowDup => {
            let _ = writeln!(s, "    h.inc(pay); h.dec(shell); pay");
        }
    }
    let _ = writeln!(s, "}}");
    let _ = writeln!(s, "");
    let _ = writeln!(s, "fn main() {{");
    let _ = writeln!(s, "    let mut h = Heap {{ cells: Vec::new(), live: 0 }};");
    // Build the Some("hi") payload cell, then a shell cell owning its ptr.
    let _ = writeln!(s, "    let pay = h.alloc(\"hi\");");
    let _ = writeln!(s, "    let some_shell = h.alloc(&pay.to_string());      // Some(pay)");
    let _ = writeln!(s, "    let none_shell = h.alloc(&NONE_TAG.to_string()); // None");
    // Call first on Some → get an owned payload ptr. Then on None.
    let _ = writeln!(s, "    let a = first(&mut h, some_shell);");
    let _ = writeln!(s, "    let b = first(&mut h, none_shell);");
    let _ = writeln!(s, "    let a_s = h.get(a);");
    let _ = writeln!(s, "    let b_s = h.get(b);");
    let _ = writeln!(s, "    println!(\"{{}}|{{}}|len={{}}\", a_s, if b_s.is_empty() {{ \"<empty>\".to_string() }} else {{ b_s.clone() }}, a_s.len());");
    let _ = writeln!(s, "    // caller drops its owned refs (a transferred from the box, b fresh)");
    let _ = writeln!(s, "    h.dec(a);");
    let _ = writeln!(s, "    h.dec(b);");
    let _ = writeln!(s, "    if h.live != 0 {{ eprintln!(\"RC IMBALANCE: {{}} cell(s) leaked\", h.live); std::process::exit(7); }}");
    let _ = writeln!(s, "}}");
    s
}

// ───────────────────────── Harness: compile + run + compare ───────────────

fn compile_and_run(label: &str, src: &str) -> (bool, String, String) {
    let dir = std::env::temp_dir().join(format!("v1gate_ar_{label}"));
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
    println!("== Phase 0 spike: alias_return shape — one MIR decision, two faithful renderings ==\n");

    let d = faithful_consume();
    let idiom = render_idiomatic(d);
    let rc = render_rc(d);

    let (i_ok, i_out, i_err) = compile_and_run("idiom", &idiom);
    let (r_ok, r_out, r_err) = compile_and_run("rc", &rc);

    println!("[idiomatic Rust] compile_ok={i_ok} out={i_out:?} {}", if i_err.is_empty() { String::new() } else { format!("err={i_err}") });
    println!("[RC (wasm sem.)] compile_ok={r_ok} out={r_out:?} {}", if r_err.is_empty() { String::new() } else { format!("err={r_err}") });

    let agree = i_ok && r_ok && i_out == r_out;
    println!("\n  → render agreement: {}", if agree { "YES (by construction)" } else { "NO" });

    // Contrast: the BUGGY re-decision (shell free recurses into the returned
    // payload) = an aliasing double-free, #643's class.
    let bd = buggy_recursive_free();
    let rc_buggy = render_rc(bd);
    let (b_ok, b_out, b_err) = compile_and_run("rc_buggy", &rc_buggy);
    let broke = b_err.contains("DOUBLE FREE") || b_err.contains("RC IMBALANCE") || !b_ok;
    println!("  → buggy re-decision (shell free recurses into returned payload): {} (out={b_out:?} err={b_err:?})",
        if broke { "DOUBLE-FREE/DIVERGE as #643's class" } else { "unexpectedly ok" });

    println!("\n== DECISION GATE (alias_return shape) ==");
    let pass = agree && broke;
    println!("  one balanced MIR decision → idiomatic Rust AND RC agree: {}", agree);
    println!("  the bug is in the RENDERER re-deciding (deep-freeing the box), not the MIR: {}", broke);
    println!("  VERDICT: {}", if pass { "PASS — RC and move/borrow share one canonical form for this shape" } else { "FAIL — investigate" });
    std::process::exit(if pass { 0 } else { 1 });
}
