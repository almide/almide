//! v1-MIR decision-gate spike — shape `boxed_pattern_610`.
//!
//! Replicates research/spike/v1-mir/src/main.rs for shape #3 (the #610 class):
//!
//!   type Tree = Leaf(Int) or Node(Tree, Tree)
//!   fn sum(t) = match t {
//!     Node(Leaf(a), Leaf(b)) => a + b,   // nested ctor at a BOXED field
//!     Node(l, r)             => sum(l) + sum(r),
//!     Leaf(n)                => n,
//!   }
//!
//! THE single ownership/layout decision for this shape:
//!
//!   Node's two fields are BOXED (recursive type ⇒ the layout registry boxes
//!   them). The nested pattern `Node(Leaf(a), Leaf(b))` does NOT consume the
//!   children: it READS THROUGH each box (a Borrow / load-through-ptr) to test
//!   the child's tag, and when the child is a Leaf it binds the Int payload,
//!   which is SCALAR (Copy — no RC, no clone). So:
//!
//!     - the box pointer is BORROWED (RC: load through ptr, count unchanged;
//!       Rust: &-deref / nested match on &child), NOT consumed, NOT dup'd;
//!     - the bound `a`,`b` are SCALAR copies of the leaf payloads — zero RC.
//!
//! That ONE decision (BoxBorrow + ScalarLoad, no Dup, no Drop of the children
//! until the owning node is dropped) is decided once in the MIR. Both renderers
//! only translate it.
//!
//! Rust has no box-pattern (`box Leaf(a)` is unstable), so the renderer lowers a
//! nested ctor at a boxed field to a TAG-GUARD + DEREF (match on `&*child`).
//! This is exactly the divergence #610 named: native-only-invalid Rust if you
//! naively try `Node(Leaf(a), Leaf(b))` as if box-patterns existed — so the
//! faithful renderer must emit the deref form. The gate asks: does the ONE
//! boxed-field + nested-bind decision render to BOTH targets with the same
//! layout/ownership choice, no divergence?
//!
//! Run: `cargo run` (standalone; needs rustc on PATH).

#![allow(dead_code, unused_variables)]

use std::fmt::Write as _;
use std::process::Command;

// ───────────────────────── Minimal MIR ─────────────────────────

/// Value representation — the LAYOUT half of the decision.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Repr {
    Scalar, // Copy (i64) — no RC, no clone. The Leaf payload.
    Boxed,  // recursive child behind a pointer — RC: a cell; Rust: Box.
}

/// How a use site treats a value — the OWNERSHIP half of the decision.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Use {
    Consume, // last use: RC = move the ref; Rust = move
    Borrow,  // read without consuming: RC = load through ptr (no inc/dec);
             //   Rust = & / deref. THIS is how a nested ctor reads a boxed field.
}

/// A single pattern arm of `sum`, expressed as MIR ownership/layout ops.
///
/// We model the body of `sum` as the recursive walk it really is, but the part
/// the gate cares about is the NESTED arm `Node(Leaf(a), Leaf(b))`: it must read
/// the two boxed children THROUGH the box (Borrow) and bind two SCALAR payloads,
/// with NO dup and NO drop of the children (the node still owns them).
#[derive(Clone, Debug)]
enum Arm {
    /// `Node(Leaf(a), Leaf(b)) => a + b` — the boxed-field nested bind.
    /// `left`/`right` name the two boxed child slots; each is read THROUGH the
    /// box (Borrow) and, if it is a Leaf, its Scalar payload is bound.
    NestedLeafPair {
        left: String,
        right: String,
        access: Use, // MUST be Borrow — the decision under test
        payload: Repr, // MUST be Scalar — the leaf int
    },
    /// `Node(l, r) => sum(l) + sum(r)` — general recursion: read each boxed
    /// child through the box (Borrow) and recurse.
    NodeRecurse { left: String, right: String, access: Use },
    /// `Leaf(n) => n` — base case: bind the Scalar payload.
    LeafBase { name: String, payload: Repr },
}

/// A tiny MIR "function": the `sum` matcher over a Tree literal.
struct MirProgram {
    /// the arms of `sum`, ownership/layout explicit
    arms: Vec<Arm>,
    /// faithful = honor the MIR (Borrow through box, no extra inc/dec).
    /// !faithful = the #610-class renderer re-decision: treat the boxed-field
    /// nested read as a CONSUME of the child (an extra dec) — i.e. re-decide
    /// ownership at the renderer and drop a child the node still owns.
    faithful: bool,
}

// ───────────────────────── The #610 shape in MIR ─────────────────────────
//
// One ownership/layout decision per concept, decided ONCE:
//   - Node fields: Boxed.
//   - Nested ctor at a boxed field: read THROUGH the box = Borrow (no RC change).
//   - Leaf payload: Scalar (Copy).

fn shape_610() -> Vec<Arm> {
    vec![
        // Node(Leaf(a), Leaf(b)) => a + b
        Arm::NestedLeafPair {
            left: "l".into(),
            right: "r".into(),
            access: Use::Borrow,  // ← THE decision: read through the box
            payload: Repr::Scalar, // ← leaf int is scalar
        },
        // Node(l, r) => sum(l) + sum(r)
        Arm::NodeRecurse { left: "l".into(), right: "r".into(), access: Use::Borrow },
        // Leaf(n) => n
        Arm::LeafBase { name: "n".into(), payload: Repr::Scalar },
    ]
}

// ───────────────────────── Renderer A: idiomatic Rust ─────────────────────
//
// Boxed = `Box<Tree>`; Scalar = `i64`. The recursive enum is the layout.
// Borrow-through-box = match on `&**child` (deref the Box, take a ref) — there
// is no box-pattern, so the faithful renderer emits the deref/nested-match form.
// No clone, no Rc: the Borrow decision means we never take ownership of a child
// in the nested arm, we only read its tag + copy the scalar payload.

fn render_idiomatic(p: &MirProgram) -> String {
    let mut s = String::new();
    let _ = writeln!(s, "#[derive(Clone)]");
    let _ = writeln!(s, "enum Tree {{ Leaf(i64), Node(Box<Tree>, Box<Tree>) }}");
    let _ = writeln!(s, "use Tree::*;");
    let _ = writeln!(s);
    // `sum` borrows its argument (&Tree) — the whole walk is read-only.
    //
    // The two `Node(..)` MIR arms (NestedLeafPair refines NodeRecurse) share the
    // SAME outer constructor, so a faithful Rust translation merges them into one
    // `Node` arm and discriminates the refinement inside via the box-deref guard.
    // Emitting them as two separate top-level arms would produce an
    // `unreachable_pattern` (dead code) — a renderer artifact, not an ownership
    // fact — so we collapse them. The ownership decision (Borrow through the box,
    // Scalar payload) is preserved verbatim.
    let _ = writeln!(s, "fn sum(t: &Tree) -> i64 {{");
    let _ = writeln!(s, "    match t {{");
    let nested = p.arms.iter().find_map(|a| match a {
        Arm::NestedLeafPair { left, right, access, payload } => {
            assert!(matches!(access, Use::Borrow), "MIR says Borrow; renderer must not re-decide");
            assert!(matches!(payload, Repr::Scalar), "leaf payload is Scalar");
            Some((left.clone(), right.clone()))
        }
        _ => None,
    });
    let recurse = p.arms.iter().find_map(|a| match a {
        Arm::NodeRecurse { left, right, access } => {
            assert!(matches!(access, Use::Borrow));
            Some((left.clone(), right.clone()))
        }
        _ => None,
    });
    if let (Some((nl, nr)), Some((_rl, _rr))) = (nested, recurse) {
        // One merged Node arm: box-deref tag-guard for the leaf pair, then fall
        // through to the general recursion. No box-pattern; pure & + Copy.
        let _ = writeln!(s, "        Node({nl}, {nr}) => match (&**{nl}, &**{nr}) {{");
        let _ = writeln!(s, "            (Leaf(a), Leaf(b)) => a + b,");
        let _ = writeln!(s, "            (lc, rc) => sum(lc) + sum(rc),");
        let _ = writeln!(s, "        }},");
    }
    for arm in &p.arms {
        if let Arm::LeafBase { name, payload } = arm {
            assert!(matches!(payload, Repr::Scalar));
            let _ = writeln!(s, "        Leaf({name}) => *{name},");
        }
    }
    let _ = writeln!(s, "    }}");
    let _ = writeln!(s, "}}");
    let _ = writeln!(s);
    let _ = writeln!(s, "fn main() {{");
    // A tree exercising BOTH the nested-leaf-pair arm and deeper recursion:
    //   Node( Node(Leaf 1, Leaf 2),  Node(Leaf 3, Leaf 4) )  = 1+2+3+4 = 10
    let _ = writeln!(s, "    let t = Node(");
    let _ = writeln!(s, "        Box::new(Node(Box::new(Leaf(1)), Box::new(Leaf(2)))),");
    let _ = writeln!(s, "        Box::new(Node(Box::new(Leaf(3)), Box::new(Leaf(4)))),");
    let _ = writeln!(s, "    );");
    let _ = writeln!(s, "    println!(\"{{}}\", sum(&t));");
    let _ = writeln!(s, "}}");
    s
}

// ───────────────────────── Renderer B: manual-RC (wasm semantics) ──────────
//
// Models the wasm runtime FAITHFULLY: a Heap of cells, each cell a Tree node
// with an explicit refcount; inc/dec are MANUAL; a cell hitting 0 frees; a dec
// below 0 is a DOUBLE FREE; a live!=0 at teardown is a LEAK. Box<Tree> becomes
// a cell index (the recursive pointer). Scalar payloads live inline (no RC).
//
// The Borrow decision means: when `sum` walks into a child to read its tag, it
// LOADS THROUGH the pointer — it does NOT inc the child and does NOT dec it.
// The node owns its children; they are freed exactly when the node is freed.
//
// faithful=false reproduces the #610-class renderer re-decision: the boxed-field
// nested read is treated as if it CONSUMED the child (an extra dec) — a renderer
// that re-decides ownership at a box-pattern. That double-frees the children.

fn render_rc(p: &MirProgram) -> String {
    let mut s = String::new();
    // The faithful manual-RC heap (mirrors the template's Heap, extended to a
    // recursive tagged node so a boxed child is a real cell pointer).
    let _ = writeln!(s, "#[derive(Clone)]");
    let _ = writeln!(s, "enum Node {{ Leaf(i64), Branch(usize, usize) }}"); // Branch holds child cell ptrs
    let _ = writeln!(s, "struct Heap {{ cells: Vec<(Node, i32)>, live: i64 }}");
    let _ = writeln!(s, "impl Heap {{");
    let _ = writeln!(s, "    fn alloc(&mut self, n: Node) -> usize {{ self.cells.push((n, 1)); self.live += 1; self.cells.len()-1 }}");
    let _ = writeln!(s, "    fn inc(&mut self, p: usize) {{ self.cells[p].1 += 1; }}");
    // dec: at 0, free AND recursively dec children (the node owned them).
    let _ = writeln!(s, "    fn dec(&mut self, p: usize) {{");
    let _ = writeln!(s, "        self.cells[p].1 -= 1;");
    let _ = writeln!(s, "        if self.cells[p].1 < 0 {{ eprintln!(\"DOUBLE FREE at cell {{}}\", p); std::process::exit(8); }}");
    let _ = writeln!(s, "        if self.cells[p].1 == 0 {{");
    let _ = writeln!(s, "            self.live -= 1;");
    let _ = writeln!(s, "            if let Node::Branch(l, r) = self.cells[p].0.clone() {{ self.dec(l); self.dec(r); }}");
    let _ = writeln!(s, "        }}");
    let _ = writeln!(s, "    }}");
    let _ = writeln!(s, "    fn tag_is_leaf(&self, p: usize) -> Option<i64> {{ if let Node::Leaf(v) = &self.cells[p].0 {{ Some(*v) }} else {{ None }} }}");
    let _ = writeln!(s, "    fn children(&self, p: usize) -> Option<(usize, usize)> {{ if let Node::Branch(l, r) = &self.cells[p].0 {{ Some((*l, *r)) }} else {{ None }} }}");
    let _ = writeln!(s, "}}");
    let _ = writeln!(s);

    // `sum` over cell pointers. `consume_children` flips Borrow→Consume to model
    // the renderer re-decision in the buggy variant.
    let consume_children = !p.faithful;
    let _ = writeln!(s, "fn sum(h: &mut Heap, p: usize) -> i64 {{");
    // The nested-leaf-pair arm: if p is a Branch and BOTH children are Leaves,
    // read THROUGH the child pointers (Borrow) and add their scalar payloads.
    let _ = writeln!(s, "    if let Some((l, r)) = h.children(p) {{");
    let _ = writeln!(s, "        if let (Some(a), Some(b)) = (h.tag_is_leaf(l), h.tag_is_leaf(r)) {{");
    if consume_children {
        // #610-class RENDERER RE-DECISION: treat the boxed-field nested read as a
        // consume of the children (extra dec) — drops nodes the branch still owns.
        let _ = writeln!(s, "            // (buggy) renderer re-decides: box-pattern read treated as CONSUME");
        let _ = writeln!(s, "            h.dec(l); h.dec(r);");
    } else {
        let _ = writeln!(s, "            // faithful: Borrow through the box — no inc/dec on the children");
    }
    let _ = writeln!(s, "            return a + b;");
    let _ = writeln!(s, "        }}");
    // general recursion (still a pure Borrow walk — no inc/dec)
    let _ = writeln!(s, "        return sum(h, l) + sum(h, r);");
    let _ = writeln!(s, "    }}");
    // Leaf base case
    let _ = writeln!(s, "    h.tag_is_leaf(p).unwrap()");
    let _ = writeln!(s, "}}");
    let _ = writeln!(s);

    let _ = writeln!(s, "fn main() {{");
    let _ = writeln!(s, "    let mut h = Heap {{ cells: Vec::new(), live: 0 }};");
    // Build Node( Node(Leaf 1, Leaf 2), Node(Leaf 3, Leaf 4) ) on the heap.
    let _ = writeln!(s, "    let l1 = h.alloc(Node::Leaf(1));");
    let _ = writeln!(s, "    let l2 = h.alloc(Node::Leaf(2));");
    let _ = writeln!(s, "    let l3 = h.alloc(Node::Leaf(3));");
    let _ = writeln!(s, "    let l4 = h.alloc(Node::Leaf(4));");
    let _ = writeln!(s, "    let nl = h.alloc(Node::Branch(l1, l2));");
    let _ = writeln!(s, "    let nr = h.alloc(Node::Branch(l3, l4));");
    let _ = writeln!(s, "    let root = h.alloc(Node::Branch(nl, nr));");
    let _ = writeln!(s, "    let result = sum(&mut h, root);");
    let _ = writeln!(s, "    println!(\"{{}}\", result);");
    // teardown: drop the one owned ref we hold (the root). A balanced Borrow
    // walk leaves all rc==1, so dec(root) cascades to free the whole tree.
    let _ = writeln!(s, "    h.dec(root);");
    let _ = writeln!(s, "    if h.live != 0 {{ eprintln!(\"RC IMBALANCE: {{}} cell(s) leaked\", h.live); std::process::exit(7); }}");
    let _ = writeln!(s, "}}");
    s
}

// ───────────────────────── Harness ─────────────────────────

fn compile_and_run(label: &str, src: &str) -> (bool, String, String) {
    let dir = std::env::temp_dir().join(format!("v1gate610_{label}"));
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
    println!("== v1-MIR gate: boxed_pattern_610 — one MIR decision, two faithful renderings ==\n");

    let faithful = MirProgram { arms: shape_610(), faithful: true };
    let idiom = render_idiomatic(&faithful);
    let rc = render_rc(&faithful);

    let (i_ok, i_out, i_err) = compile_and_run("idiom", &idiom);
    let (r_ok, r_out, r_err) = compile_and_run("rc", &rc);

    println!("[idiomatic Rust] compile_ok={i_ok} out={i_out:?} {}", if i_err.is_empty() { String::new() } else { format!("err={i_err}") });
    println!("[RC (wasm sem.)] compile_ok={r_ok} out={r_out:?} {}", if r_err.is_empty() { String::new() } else { format!("err={r_err}") });

    let agree = i_ok && r_ok && i_out == r_out && i_out == "10";
    println!("\n  → render agreement: {} (expected 10)", if agree { "YES (by construction)" } else { "NO" });

    // Contrast: the #610-class renderer re-decision (box-pattern read = consume).
    let buggy = MirProgram { arms: shape_610(), faithful: false };
    let rc_buggy = render_rc(&buggy);
    let (b_ok, b_out, b_err) = compile_and_run("rc_buggy", &rc_buggy);
    let broke = b_err.contains("DOUBLE FREE") || b_err.contains("RC IMBALANCE") || !b_ok;
    println!("  → renderer re-decides ownership at box-pattern (consume children): {} (out={b_out:?} err={b_err:?})",
        if broke { "DOUBLE-FREE/DIVERGE as #610" } else { "unexpectedly ok" });

    println!("\n== DECISION GATE (boxed_pattern_610 shape) ==");
    let pass = agree && broke;
    println!("  one boxed-field + nested-bind MIR decision → idiomatic Rust AND RC agree: {agree}");
    println!("  the bug is the RENDERER re-deciding ownership at the box-pattern, not the MIR: {broke}");
    println!("  VERDICT: {}", if pass { "PASS — boxed-field Borrow + Scalar payload share one canonical form" } else { "FAIL — investigate" });
    std::process::exit(if pass { 0 } else { 1 });
}
