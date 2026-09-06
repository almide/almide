//! The `consume(produce(scalars))` region window on the structural leg
//! (#1961): the producer's blocks are reclaimed wholesale when the
//! consumer returns, so a loop of `check(make(d))` runs in bounded heap
//! instead of a monotonic bump — and prints exactly what native prints.
//! Nullary variant cases are pool statics (a `Leaf` costs no
//! allocation at all).

mod harness;
use harness::run_wasm;

const TREES: &str = r#"type Tree = Leaf | Node(Tree, Tree)

fn make(depth: Int) -> Tree =
  if depth == 0 then Leaf
  else Node(make(depth - 1), make(depth - 1))

fn check(tree: Tree) -> Int = match tree {
  Leaf => 1,
  Node(left, right) => check(left) + check(right) + 1,
}

fn check_trees(iterations: Int, depth: Int) -> Int = {
  var total = 0
  for _ in 0..<iterations {
    total = total + check(make(depth))
  }
  total
}

effect fn main() -> Unit = {
  let keep = make(6)
  println("sum=${check_trees(64, 8)} keep=${check(keep)} one=${check(make(4))}")
}
"#;

/// A window must NOT open when the consumer's result is a heap value or
/// the producer reads a global — the same program shape, both refused
/// paths exercised for output equality only.
const NO_WINDOW: &str = r#"type Tree = Leaf | Node(Tree, Tree)

let base = 2

fn make(depth: Int) -> Tree =
  if depth == 0 then Leaf
  else Node(make(depth - 1), make(depth - 1))

fn make_g(depth: Int) -> Tree = make(depth + base)

fn check(tree: Tree) -> Int = match tree {
  Leaf => 1,
  Node(left, right) => check(left) + check(right) + 1,
}

fn left_of(tree: Tree) -> Tree = match tree {
  Leaf => Leaf,
  Node(left, _) => left,
}

effect fn main() -> Unit =
  println("g=${check(make_g(3))} l=${check(left_of(make(5)))}")
"#;

fn run(name: &str, src: &str) -> (String, u64) {
    let ir = almide_spine::s5::lower_to_ir(name, src).expect("front end");
    let bytes = almide_wasm::emit_program(&ir)
        .unwrap_or_else(|e| panic!("the structural leg must lower {name}: {e:?}"));
    let out = run_wasm(&bytes).expect("wasmtime run");
    assert_eq!(out.exit, 0, "{name} stderr: {}", out.stderr);
    (out.stdout, out.heap_end.expect("__heap export present"))
}

#[test]
fn a_window_reclaims_the_producer_wholesale() {
    let (out, heap) = run("trees.almd", TREES);
    // 64 trees of depth 8 (511 nodes each), the kept depth-6 tree, one
    // depth-4 tree.
    assert_eq!(out, "sum=32704 keep=127 one=31\n");
    // Without the window the 64 iterations bump 64 × 255 Node blocks
    // (measured: a 590 KB high-water, `ALMIDE_REGION_OFF=1`); with it
    // the peak is one tree's worth plus the kept tree above the fixed
    // heap floor (measured: 76 KB, most of it the line buffer's room).
    // The harness reports max(`__heap`, `__heap_high`).
    assert!(heap < 128 * 1024, "heap high-water {heap} — the window did not reclaim");
}

#[test]
fn a_refused_window_keeps_output_equality() {
    let (out, _) = run("no_window.almd", NO_WINDOW);
    assert_eq!(out, "g=63 l=31\n");
}
