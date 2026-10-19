//! The canonical-allocator switch: the inspection meter's fixed allocator.
//!
//! The complexity criterion's bytes-moved series must be a function of n.
//! Under the shipped allocator it is also a function of the heap's state:
//! - a class free-list pop or a large-list take can hand a growing buffer a
//!   block with spare capacity;
//! - the large list keeps a remainder under 64 KiB on the block it hands out.
//! Either way a growth copy is skipped, and the series bends. Measured on
//! option.collect, set.map, set.from_list and list.unique.
//!
//! Under the switch, `$alloc` never reuses: every request bumps, so a
//! program's copies depend only on its requests. Reuse can only remove
//! copies, so the canonical series is an upper bound of the shipped one.
//! The approach follows the cohort's test-scoped allocators (Rust's
//! `Allocator` parameter, Roc's TestEnv); none of them measures copies under
//! a reusing allocator.
//!
//! - OFF: the module's bytes are those of a build that never heard of the
//!   switch.
//! - ON: the same stdout, and no allocation is served by reuse, whether the
//!   small-block or the large-block (> 64 KiB) free list is involved.

mod harness;
use harness::run_wasm;

fn emit(src: &str, canonical: bool) -> Vec<u8> {
    let ir = almide_spine::s5::lower_to_ir("probe.almd", src).expect("front");
    let _count = almide_wasm::alloc_count::CountGuard::set();
    let _canonical = canonical.then(almide_wasm::canonical_alloc::CanonicalGuard::set);
    almide_wasm::emit_program(&ir).expect("emit")
}

fn emit_plain(src: &str, canonical: bool) -> Vec<u8> {
    let ir = almide_spine::s5::lower_to_ir("probe.almd", src).expect("front");
    let _canonical = canonical.then(almide_wasm::canonical_alloc::CanonicalGuard::set);
    almide_wasm::emit_program(&ir).expect("emit")
}

/// Small blocks: a fresh string per iteration, released at its end.
const SMALL_CHURN: &str = r#"fn main() -> Unit = {
  var acc = 0
  for i in 0..<1000 {
    let s = "row " + int.to_string(i)
    acc = acc + string.len(s)
  }
  println("${acc}")
}
"#;

/// Large blocks: a 20,000-element list (160 KB, above the 64 KiB class
/// ceiling) built and released per iteration.
const LARGE_CHURN: &str = r#"fn main() -> Unit = {
  var acc = 0
  for i in 0..<8 {
    let xs = list.map(list.range(0, 20000), (x) => x + i)
    acc = acc + list.len(xs)
  }
  println("${acc}")
}
"#;

/// `iters` rounds of a 100 KB buffer (above the class ceiling) released at
/// the end of its round, with a small string allocated after it so two
/// rounds' buffers are never adjacent and cannot coalesce on release — the
/// shape of a read call's `Bytes` followed by its 16-byte `Result` box.
fn large_rounds(iters: u32) -> String {
    format!(
        r#"fn probe() -> Int = {{
  var acc = 0
  for i in 0..<{iters} {{
    let b = bytes.new(100000 + i % 2)
    let s = "r" + int.to_string(i)
    acc = acc + bytes.len(b) + string.len(s)
  }}
  acc
}}
"#
    )
}

/// (fuel consumed, result) of the canonical library build's `probe()`:
/// wasmtime fuel, one unit per operator, so the count is exact.
fn canonical_fuel(src: &str) -> (u64, i64) {
    let ir = almide_spine::s5::lower_to_ir("canonical_fuel.almd", src).expect("front");
    let bytes = {
        let _canonical = almide_wasm::canonical_alloc::CanonicalGuard::set();
        almide_wasm::emit_library_with_ops(&ir).expect("emit").0
    };
    let mut config = wasmtime::Config::new();
    config.consume_fuel(true);
    let engine = wasmtime::Engine::new(&config).expect("engine");
    let module = wasmtime::Module::new(&engine, &bytes).expect("module");
    let mut linker = wasmtime::Linker::new(&engine);
    linker.define_unknown_imports_as_traps(&module).expect("imports");
    let mut store = wasmtime::Store::new(&engine, ());
    store.set_fuel(1 << 40).expect("fuel");
    let inst = linker.instantiate(&mut store, &module).expect("instantiate");
    let f = inst.get_typed_func::<(), i64>(&mut store, "probe").expect("probe export");
    let before = store.get_fuel().expect("fuel");
    let out = f.call(&mut store, ()).expect("probe runs");
    (before - store.get_fuel().expect("fuel"), out)
}

/// The inspection meter runs a call k and 2k times and takes the
/// difference, so a round's cost must not depend on how many rounds came
/// before it. `$free` filed every released large block into the
/// address-ordered large list, and a canonical bump puts each new block
/// above every filed one, so filing walked the whole list: round i cost
/// O(i), and the marginal of k rounds grew with k. The meter calibrates k
/// per size, so that growth read as a superlinear step in n
/// (net.tcp_read_exact's wasm series: nlogn, rms 4.0%, at k = 4096). Under
/// the switch nothing takes from the list, so `$free` does not file there.
#[test]
fn a_canonical_round_costs_the_same_however_many_rounds_came_before() {
    const K: u32 = 400;
    let fuel = |iters: u32| {
        let (f, out) = canonical_fuel(&large_rounds(iters));
        let want: i64 = (0..iters).map(|i| 100_000 + i64::from(i % 2) + 1 + i.to_string().len() as i64).sum();
        assert_eq!(out, want, "{iters} rounds answered wrong");
        f
    };
    let (f1, f2, f4) = (fuel(K), fuel(2 * K), fuel(4 * K));
    let (first, second) = (f2 - f1, f4 - f2);
    // Anti-vacuous: K rounds must be measurable.
    assert!(first > 10 * u64::from(K), "{K} rounds cost {first} fuel — too little to gate on");
    // Constant per round: 2K rounds past 2K cost twice K rounds past K. A
    // walk over every earlier round's block adds a term quadratic in the
    // rounds, which the excess over twice reads directly (each round's
    // buffer fill is linear and cancels). Measured before the fix: excess
    // 8.2M fuel on a first difference of 44.4M (18%); after: 46.8K (0.1%).
    let excess = second as f64 - 2.0 * first as f64;
    println!("RATCHET canonical large rounds fuel: {K} rounds past {K}: {first}, {} past {}: {second}, excess over 2x {excess}", 2 * K, 2 * K);
    assert!(
        excess <= first as f64 / 100.0,
        "a canonical round's cost grew with the rounds before it: {first} then {second} fuel, {excess} over twice the first"
    );
}

#[test]
fn an_unarmed_build_is_byte_identical_to_one_that_never_heard_of_the_switch() {
    let off = emit_plain(SMALL_CHURN, false);
    let _armed = emit_plain(SMALL_CHURN, true);
    assert_eq!(emit_plain(SMALL_CHURN, false), off, "the switch leaves no residue in the next build");
    assert_ne!(emit_plain(SMALL_CHURN, true), off, "the armed build differs: its $alloc skips the free lists");
}

#[test]
fn the_canonical_allocator_never_serves_a_request_by_reuse() {
    for (name, src, out) in [("small", SMALL_CHURN, "6890\n"), ("large", LARGE_CHURN, "160000\n")] {
        let shipped = run_wasm(&emit(src, false)).expect("run");
        let canonical = run_wasm(&emit(src, true)).expect("run");
        assert_eq!(shipped.stdout, out, "{name}");
        assert_eq!(canonical.stdout, out, "{name}: the canonical allocator changes no output");
        let s = shipped.alloc_count.expect("counters");
        let c = canonical.alloc_count.expect("counters");
        assert!(s.reused > 0, "{name}: the shipped allocator reuses here (else this program tests nothing): {s:?}");
        assert_eq!(c.reused, 0, "{name}: the canonical allocator bumps every request: {c:?}");
        assert_eq!(c.allocs, s.allocs, "{name}: the same requests, only served differently");
    }
    // The large program exercises the large-block list, not only the small
    // classes. Shipped, each 160 KB list reuses the one the previous
    // iteration released. Canonically, each bumps a fresh one.
    let shipped = run_wasm(&emit(LARGE_CHURN, false)).expect("run").heap_end.expect("watermark");
    let canonical = run_wasm(&emit(LARGE_CHURN, true)).expect("run").heap_end.expect("watermark");
    assert!(
        canonical >= shipped + 4 * 160_000,
        "the large lists are reused when shipped and bumped when canonical: shipped heap_end {shipped}, canonical {canonical}"
    );
}
