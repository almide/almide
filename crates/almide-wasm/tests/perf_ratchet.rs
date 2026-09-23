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
//!   R5 asymptotic class of an element store (#2150): t(4n, r/4) / t(n, r)
//!      at a fixed number of stores. O(1) per store predicts ~1; the #1729
//!      copy-per-write shape (fft's ~3,500x at 2^18) predicts ~4. Line 2.5.
//!   R6 store/read parity (#2150): t(store loop) / t(read loop) at the same
//!      n. An element-only loop judges copy-on-write once per loop entry
//!      (cow_hoist.rs); judging per store read 2.92 against 1.13. Line 2.0.
//!   R7 the timing runner's harness tax (#2150): t(run_wasm_unbounded) /
//!      t(run_wasm) on mandelbrot's escape-time loop. The 30 s epoch
//!      watchdog of the test runner checks at every loop header (1.9x on
//!      mandelbrot); `almide bench --target wasm` must not pay it. Line 0.85.
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

/// `rounds` passes over a preallocated `List[Float]` of `n` elements, each
/// pass either storing into every element (`xs[i] = xs[i] + 1.0`) or only
/// reading it (`acc = acc + xs[i]`) — the fft butterfly's access, isolated.
fn index_kernel(n: usize, rounds: usize, store: bool) -> String {
    let (decl, step, out) = if store {
        ("", "xs[i] = xs[i] + 1.0", "xs[0]")
    } else {
        ("  var acc = 0.0\n", "acc = acc + xs[i]", "acc")
    };
    format!(
        r#"fn main() -> Unit = {{
  var xs: List[Float] = list.repeat(0.5, {n})
{decl}  var r = 0
  while r < {rounds} {{
    for i in 0..<{n} {{
      {step}
    }}
    r = r + 1
  }}
  println(float.to_string({out}))
}}
"#
    )
}

/// R5 + R6 (#2150): the two ways an element store has gone wrong on this leg.
#[cfg_attr(debug_assertions, ignore = "timing gate is release-only (CI: release-shape job)")]
#[test]
fn index_store_class_and_store_read_parity() {
    const N: usize = 50_000;
    const ROUNDS: usize = 600;
    // R5 asymptotic class at a fixed total of stores: t(4n, r/4) / t(n, r).
    // A store is O(1) and predicts ~1; the #1729 shape (a fresh block copy
    // per store) is O(n) per store and predicts ~4 — the fft row's ~3,500x
    // at 2^18. The gate line is 2.5.
    let ts = measure(&index_kernel(N, ROUNDS, true));
    let tb = measure(&index_kernel(4 * N, ROUNDS / 4, true));
    assert!(ts >= 5.0, "index store: measured {ts:.1}ms — too fast to gate on, re-size the kernel");
    let class = tb / ts;
    println!("RATCHET index store t(n={N})={ts:.1}ms t(4n, r/4)={tb:.1}ms ratio={class:.2}");
    assert!(
        class <= 2.5,
        "index store: t(4n, r/4)/t(n, r) = {class:.2} > 2.5 — a store costs O(len) again (the #1729 \
         copy-per-write class, fft's ~3,500x cliff)"
    );
    // R6 store/read parity at the same n: a store in a loop that reaches the
    // list only element-wise judges copy-on-write once per loop entry
    // (cow_hoist.rs), so it costs a read plus a store. Judging per store (a
    // call, a global compare and a reference-count load each) was the bulk of
    // fft's remaining wasm/native gap.
    let tr = measure(&index_kernel(N, ROUNDS, false));
    let parity = ts / tr;
    println!("RATCHET index store/read t(store)={ts:.1}ms t(read)={tr:.1}ms ratio={parity:.2}");
    assert!(
        parity <= STORE_READ_CEILING,
        "index store/read = {parity:.2} > {STORE_READ_CEILING} — the element-only loop is judging \
         copy-on-write per store again (cow_hoist.rs, #2150)"
    );
}

/// A/B on an M4 Pro, 2026-09-24, same kernel: 1.13 with the judge hoisted
/// per loop entry, 2.92 with it run per store. The line sits between.
const STORE_READ_CEILING: f64 = 2.0;

/// mandelbrot's escape-time inner loop over an `n`×`n` grid: a tight float
/// loop with a tiny body, where a per-back-edge check is a large fraction.
fn escape_time_kernel(n: usize) -> String {
    format!(
        r#"fn main() -> Unit = {{
  var inside = 0
  var y = 0
  while y < {n} {{
    let ci = 2.0 * float.from_int(y) / {n}.0 - 1.0
    var x = 0
    while x < {n} {{
      let cr = 2.0 * float.from_int(x) / {n}.0 - 1.5
      var zr = 0.0
      var zi = 0.0
      var it = 0
      while it < 50 and zr * zr + zi * zi <= 4.0 {{
        let t = zr * zr - zi * zi + cr
        zi = 2.0 * zr * zi + ci
        zr = t
        it = it + 1
      }}
      if it == 50 then inside = inside + 1 else ()
      x = x + 1
    }}
    y = y + 1
  }}
  println(int.to_string(inside))
}}
"#
    )
}

/// R7 (#2150): `almide bench --target wasm` times the program, not the
/// harness's watchdog. The in-process test runner (`run_wasm`) arms a 30 s
/// epoch deadline, and epoch interruption makes wasmtime check the epoch at
/// every loop header — 1.9x on mandelbrot at 4000, the whole of its embedded
/// wasm/native gap. The timing runner (`run_wasm_unbounded`) must not carry
/// it: t(unbounded) / t(watchdog) on the escape-time loop reads ~0.5 when it
/// does not; the gate line is 0.85 (a re-armed watchdog reads ~1.0).
#[cfg_attr(debug_assertions, ignore = "timing gate is release-only (CI: release-shape job)")]
#[test]
fn timing_runner_carries_no_watchdog_tax() {
    let src = escape_time_kernel(1000);
    let ir = almide_spine::s5::lower_to_ir("perf_ratchet.almd", &src).expect("front");
    let bytes = almide_wasm::emit_program(&ir).expect("emit");
    let best = |run: &dyn Fn(&[u8]) -> anyhow::Result<almide_wasm_run::RunResult>| {
        (0..3)
            .map(|_| {
                let t = Instant::now();
                let r = run(&bytes).expect("run");
                assert_eq!(r.exit, 0, "kernel exited nonzero: {}", r.stderr);
                t.elapsed().as_secs_f64() * 1000.0
            })
            .fold(f64::INFINITY, f64::min)
    };
    let watched = best(&almide_wasm_run::run_wasm);
    let timed = best(&almide_wasm_run::run_wasm_unbounded);
    assert!(watched >= 5.0, "escape-time: measured {watched:.1}ms — too fast to gate on, re-size the kernel");
    let ratio = timed / watched;
    println!("RATCHET timing runner/watchdog runner t={timed:.1}ms / {watched:.1}ms ratio={ratio:.2}");
    assert!(
        ratio <= 0.85,
        "timing runner / watchdog runner = {ratio:.2} > 0.85 — `almide bench --target wasm` pays the \
         epoch watchdog's per-loop-header check again (run_wasm_unbounded, #2150)"
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
