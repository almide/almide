//! #3348 — blocks above the 64 KiB class ceiling are REUSED: an
//! address-ordered, exact-size free list with split and coalesce
//! (runtime_large.rs; proofs/LargeList.v + LargeTree.v + StructuralRun.v).
//!
//! Before it, a freed block above the class ceiling was abandoned, so a loop
//! that rebuilt a large list grew linear memory by one list per iteration —
//! the peak tracked the CALL COUNT, not the live data. Each shape below
//! builds a new list while the previous one is still live and then drops the
//! old one (no in-place path applies: `list.map` / `list.repeat` always
//! allocate), and pins the frontier's growth over an empty program to at
//! most 1.5× the peak LIVE payload.
//!
//! `cargo test -p almide-wasm --test large_block_reuse -- --ignored
//! --nocapture` prints the full-size table (8 MB lists, 200 rounds).

mod harness;
use harness::run_wasm;

fn heap_end(src: &str) -> (u64, String) {
    let ir = almide_spine::s5::lower_to_ir("large_block_reuse.almd", src).expect("front");
    let bytes = almide_wasm::emit_program(&ir).expect("the structural leg lowers the probe");
    let out = run_wasm(&bytes).expect("run");
    assert_eq!(out.exit, 0, "{}", out.stderr);
    (out.heap_end.expect("the module exports __heap"), out.stdout)
}

fn main_of(body: &str) -> String {
    format!("effect fn main() -> Unit = {{\n{body}\n}}\n")
}

/// Repeatedly rebuild an `n`-Float list: two lists live at the peak.
fn copy_loop(n: u64, rounds: u32) -> (String, u64) {
    let body = format!(
        "  var a = list.repeat(0.0, {n})\n  for k in 0..<{rounds} {{\n    a = list.map(a, (x) => x + 1.0)\n  }}\n  println(\"${{a[0]}}\")"
    );
    (main_of(&body), 2 * n * 8)
}

/// Two lists of awkward sizes rebuilt alternately.
fn mixed(n1: u64, n2: u64, rounds: u32) -> (String, u64) {
    let body = format!(
        "  var a = list.repeat(0.0, {n1})\n  var b = list.repeat(0.0, {n2})\n  for k in 0..<{rounds} {{\n    a = list.map(a, (x) => x + 1.0)\n    b = list.map(b, (x) => x + 1.0)\n  }}\n  println(\"${{a[0]}} ${{b[0]}}\")"
    );
    (main_of(&body), (2 * n1 + n2) * 8)
}

/// A growing working set: each round's list is one step bigger than the
/// last, which dies after the new one exists.
fn ramp(step: u64, steps: u64) -> (String, u64) {
    let body = format!(
        "  var c = list.repeat(0.0, {step})\n  for k in 2..<{} {{\n    c = list.repeat(1.0, k * {step})\n  }}\n  println(\"${{c[0]}}\")",
        steps + 1
    );
    (main_of(&body), (2 * steps - 1) * step * 8)
}

/// Peak frontier growth over an empty program, as a multiple of live.
fn ratio((src, live): (String, u64)) -> (f64, u64, u64) {
    let (base, _) = heap_end(&main_of("  println(\"0\")"));
    let (end, _) = heap_end(&src);
    let grown = end - base;
    (grown as f64 / live as f64, grown, live)
}

fn assert_bounded(name: &str, shape: (String, u64)) {
    let (r, grown, live) = ratio(shape);
    assert!(r <= 1.5, "{name}: the heap grew {grown} bytes for {live} live — {r:.2}× (gate 1.5×)");
}

#[test]
fn rebuilding_a_list_above_the_ceiling_reuses_its_block() {
    // 70 000 Floats = 560 KB, above the class ceiling; 100 rounds would be
    // 56 MB if the dead copies were abandoned.
    assert_bounded("copy 560 KB x100", copy_loop(70_000, 100));
}

#[test]
fn two_sizes_rebuilt_alternately_share_the_list() {
    assert_bounded("mixed 1.2 MB + 600 KB x40", mixed(150_000, 75_000, 40));
}

#[test]
fn a_growing_working_set_coalesces_its_dead_blocks() {
    assert_bounded("ramp 100 KB..2 MB", ramp(12_500, 20));
}

/// The #3348 measurement table at full size (slow: run on demand).
#[test]
#[ignore]
fn print_the_peak_table() {
    let shapes = [
        ("#3337 copy 8 MB x200", copy_loop(1_000_000, 200)),
        ("#3343 copy 560 KB x200", copy_loop(70_000, 200)),
        ("mixed 4.2 MB + 600 KB x100", mixed(525_000, 75_000, 100)),
        ("ramp 100 KB..5 MB", ramp(12_500, 50)),
    ];
    for (name, shape) in shapes {
        let (r, grown, live) = ratio(shape);
        println!("{name:30} live {:8.1} MB  peak {:8.1} MB  {r:.2}x", live as f64 / 1e6, grown as f64 / 1e6);
    }
}
