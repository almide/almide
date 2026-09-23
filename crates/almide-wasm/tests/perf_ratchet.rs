//! Perf RATCHET (#1585) — the probe's measure-only era ends here. The
//! gate holds machine-stable RELATIONS, never absolute milliseconds
//! (the koka/B2 doctrine: the same commit reads 1.58 on an M4 Pro and
//! 0.91 on a CI runner — only dimensionless quantities survive a
//! machine change):
//!
//!   R1 asymptotic class, measured directly: t(4n) / t(n) for the two
//!      sort surfaces. Merge sort predicts ~4.6; O(n^2) predicts ~16.
//!      The gate line is 8 — the survey's C2 defect (an O(n^2) sort
//!      hiding under stdout-equality for years) is unrepresentable
//!      while this holds.
//!   R2 lockstep overhead: t(sort_by) / t(sort) at the same n stays
//!      bounded (the keys+values merge moves twice the bytes, not an
//!      algorithm class more).
//!   R3 key-count independence of `list.group_by`: t(K=4000) / t(K=40)
//!      at the same n stays a small constant. An indexed lookup predicts
//!      ~1-2 (more distinct keys = more entries, once each); the linear
//!      scan #2156 shipped with predicts ~K2/K1 = 100 (every element
//!      walked every group key — 11 µs per element over 5,000 keys). The
//!      gate line is 8.
//!   R4 per-node cost of a region window's producer (#2318), a COUNT
//!      relation so it needs no clock: one allocation, no free and no
//!      free-list reuse per `Node`, allocated through the inlined bump.
//!      It lives beside the window's other gates in region_window.rs
//!      (`a_window_local_node_costs_one_inlined_allocation_and_no_free`).
//!
//! Anti-vacuous floor: the small-size measurement must be slow enough
//! to mean something — if an optimizer ever elides the loop, the gate
//! demands re-sizing instead of silently passing on noise.

mod harness;
use harness::run_wasm;
use std::time::Instant;

fn kernel(sort_call: &str, n: usize, rounds: usize) -> String {
    format!(
        r#"fn mk(n: Int) -> List[Int] = {{
  var out: List[Int] = []
  var i = 0
  while i < n {{
    list.push(out, (i * 7919) % 10007)
    i = i + 1
  }}
  out
}}

fn main() -> Unit = {{
  let xs = mk({n})
  var acc = 0
  var r = 0
  while r < {rounds} {{
    let s = {sort_call}
    acc = acc + s[0] + s[{last}]
    r = r + 1
  }}
  println(int.to_string(acc))
}}
"#,
        last = n - 1
    )
}

/// Best-of-3 wall milliseconds for one kernel source (emit once).
fn measure(src: &str) -> f64 {
    let ir = almide_spine::s5::lower_to_ir("perf_ratchet.almd", src).expect("front");
    let bytes = almide_wasm::emit_program(&ir).expect("emit");
    let mut best = f64::INFINITY;
    for _ in 0..3 {
        let t = Instant::now();
        let r = run_wasm(&bytes).expect("run");
        assert_eq!(r.exit, 0, "kernel exited nonzero: {}", r.stderr);
        best = best.min(t.elapsed().as_secs_f64() * 1000.0);
    }
    best
}

#[cfg_attr(debug_assertions, ignore = "timing gate is release-only (CI: release-shape job)")]
#[test]
fn sort_asymptotic_and_lockstep_relations() {
    const N: usize = 4_000;
    const ROUNDS: usize = 200;
    let sorts: &[(&str, String, String)] = &[
        ("list.sort", kernel("list.sort(xs)", N, ROUNDS), kernel("list.sort(xs)", 4 * N, ROUNDS)),
        (
            "list.sort_by",
            kernel("list.sort_by(xs, (x) => 0 - x)", N, ROUNDS),
            kernel("list.sort_by(xs, (x) => 0 - x)", 4 * N, ROUNDS),
        ),
    ];
    let mut small_ms = Vec::new();
    for (name, small, big) in sorts {
        let ts = measure(small);
        let tb = measure(big);
        // Anti-vacuous: a small-side measurement under 5ms means the
        // workload got elided or the sizes drifted — re-size, don't trust.
        assert!(ts >= 5.0, "{name}: small size measured {ts:.1}ms — too fast to gate on, re-size the kernel");
        let ratio = tb / ts;
        println!("RATCHET {name} t({N})={ts:.1}ms t({})={tb:.1}ms ratio={ratio:.2}", 4 * N);
        assert!(
            ratio <= 8.0,
            "{name}: t(4n)/t(n) = {ratio:.2} > 8 — the sort left the n log n class \
             (merge sort predicts ~4.6, O(n^2) predicts ~16; survey C2 is the precedent)"
        );
        small_ms.push(ts);
    }
    // R2: lockstep sort_by stays within a constant factor of sort.
    let (sort_t, sort_by_t) = (small_ms[0], small_ms[1]);
    let overhead = sort_by_t / sort_t;
    println!("RATCHET lockstep sort_by/sort = {overhead:.2}");
    assert!(
        overhead <= 4.0,
        "sort_by/sort = {overhead:.2} > 4 — the lockstep merge picked up more than a constant factor"
    );
}

/// One `list.group_by` over `n` String keys drawn from a `k`-word
/// vocabulary — the wordfreq shape (#2156) with the vocabulary size as
/// the free variable.
fn group_by_kernel(n: usize, k: usize) -> String {
    format!(
        r#"fn main() -> Unit = {{
  let xs = list.range(0, {n}) |> list.map((i) => int.to_string((i * 7919) % {k}))
  let g = list.group_by(xs, (w) => w)
  println(int.to_string(map.len(g)))
}}
"#
    )
}

#[cfg_attr(debug_assertions, ignore = "timing gate is release-only (CI: release-shape job)")]
#[test]
fn group_by_key_count_relation() {
    const N: usize = 200_000;
    const K_SMALL: usize = 40;
    const K_BIG: usize = 4_000;
    let ts = measure(&group_by_kernel(N, K_SMALL));
    let tb = measure(&group_by_kernel(N, K_BIG));
    // Anti-vacuous: the small-vocabulary side must still be a measurement.
    assert!(ts >= 2.0, "group_by: K={K_SMALL} measured {ts:.1}ms — too fast to gate on, re-size the kernel");
    let ratio = tb / ts;
    println!("RATCHET list.group_by t(K={K_SMALL})={ts:.1}ms t(K={K_BIG})={tb:.1}ms ratio={ratio:.2}");
    assert!(
        ratio <= 8.0,
        "list.group_by: t(K={K_BIG})/t(K={K_SMALL}) = {ratio:.2} > 8 — the lookup left the index lane \
         (the accumulator must grow in place so its address is stable; a per-key copy pins it to the \
         linear scan, #2156's 110x)"
    );
}
