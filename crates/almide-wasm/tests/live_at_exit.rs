//! The live-heap instrument's own A/B (the live-at-exit ledger in
//! alloc_ledger.rs judges the corpus with it): `allocs − frees −
//! region_reclaimed` after `main` returns must read 0 for a program that
//! releases everything, must NOT count what is live by design (top-let
//! globals) or what a region window reclaims wholesale, and must go
//! non-zero when the emitter omits a release — the negative half, through
//! the `KeepTopLetsGuard` hook that drops the armed `main`'s top-let
//! releases.

mod harness;
use harness::run_wasm;

fn run_armed(src: &str, keep_top_lets: bool) -> almide_wasm_run::AllocCount {
    let ir = almide_spine::s5::lower_to_ir("probe.almd", src).expect("front");
    let _armed = almide_wasm::alloc_count::CountGuard::set();
    let _keep = keep_top_lets.then(almide_wasm::alloc_count::KeepTopLetsGuard::set);
    let bytes = almide_wasm::emit_program(&ir).expect("emit");
    let r = run_wasm(&bytes).expect("run");
    assert_eq!(r.exit, 0, "stderr: {}", r.stderr);
    r.alloc_count.expect("the armed module exports the counters")
}

const CLEAN: &str = r#"fn main() -> Unit = {
  var acc = 0
  for i in 0..<100 {
    let s = "row " + int.to_string(i)
    acc = acc + string.len(s)
  }
  let xs = [1, 2, 3] |> list.map((x) => x * 2)
  println("${acc} ${list.len(xs)}")
}
"#;

// A String top-let: a heap block live by design until exit. (A List /
// Map / Set / Bytes top-let is copied into its global and the fresh
// initializer block is not released — a real leak the corpus ledger
// records, not what this probe is about.)
const TOP_LET: &str = r#"let GREETING = "hello " + int.to_string(42)

fn main() -> Unit = {
  println("${string.len(GREETING)}")
}
"#;

const WINDOW: &str = r#"type Tree =
  | Leaf
  | Node(Tree, Tree)

fn make(d: Int) -> Tree =
  if d == 0 then Leaf else Node(make(d - 1), make(d - 1))

fn check(t: Tree) -> Int =
  match t {
    Leaf => 1,
    Node(l, r) => 1 + check(l) + check(r),
  }

fn main() -> Unit = {
  var total = 0
  for d in 1..<6 {
    total = total + check(make(d))
  }
  println("${total}")
}
"#;

#[test]
fn a_program_that_releases_everything_reads_zero() {
    let c = run_armed(CLEAN, false);
    assert!(c.allocs > 100, "the probe allocates: {c}");
    assert_eq!(c.live(), 0, "{c}");
}

#[test]
fn a_top_let_is_released_by_the_measuring_main_and_not_counted() {
    let c = run_armed(TOP_LET, false);
    assert!(c.allocs > 0, "{c}");
    assert_eq!(c.live(), 0, "a top-let is live by design until exit: {c}");
}

/// The negative half: the emitter omits one release (the armed `main`'s
/// top-let drop) and the gate sees exactly that block.
#[test]
fn an_omitted_release_is_seen_as_a_live_block() {
    let released = run_armed(TOP_LET, false);
    let kept = run_armed(TOP_LET, true);
    assert_eq!(kept.allocs, released.allocs, "the hook changes releases only: {kept} vs {released}");
    assert!(kept.live() > 0, "an omitted release must read as live at exit: {kept}");
    assert_eq!(kept.live() as u64, released.frees - kept.frees, "{kept} vs {released}");
}

/// A region window (#1961) reclaims its blocks without a `$free`; the
/// restore books them as `reclaimed`, so they are not a leak.
#[test]
fn a_region_window_reclaim_is_not_a_leak() {
    let c = run_armed(WINDOW, false);
    assert!(c.reclaimed > 0, "the probe opens region windows: {c}");
    assert_eq!(c.live(), 0, "{c}");
}
