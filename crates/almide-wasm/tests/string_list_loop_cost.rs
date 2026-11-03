//! #4430 — the string/list loop of the issue costs no more on the wasm leg
//! than it did at 0.67.1. The self-hosted substring search became
//! Knuth–Morris–Pratt in 0.69.0-rc2 (#4206, so a search is linear in the
//! haystack and needle where the position-by-position compare was
//! O(slen·nlen)). Every `string.contains` / `index_of` / `split_once` then
//! allocated and freed a failure table, even for a one-byte needle that never
//! reads it, and paid one `__sk_step` call per haystack byte while nothing
//! had matched. The loop below ran 1.17x the fuel and 1.34x the time of
//! 0.67.1. The search stays linear; what changed is what each call pays
//! around it: no table for a needle of one or two bytes, a tight walk to the
//! next byte that can start a match, and one outright compare at the first
//! such byte before a longer needle builds its table.
//!
//! The gate is an exact COUNT, no clock: wasmtime fuel (one unit per
//! operator) of K iterations minus the same probe with none, divided by K.
//! The loop line is 0.67.1's cost, the regression's own definition (its
//! `almide build` of the issue program at n = 200,000: 2,667,023,888 fuel,
//! 13,335 an iteration; develop before the fix 14,457, now 11,205). The
//! `contains` cells pin the per-call cost of the three needle shapes the fast
//! paths cover, each against 0.67.1's cost of the same call (its `almide
//! build` of the same probe, 100,000 calls minus none; the library build
//! here measures the same counts to the unit on the fixed compiler). A
//! failure prints the measured value.

const LOOP: &str = r#"
fn work(n: Int) -> Int = {
  var total = 0
  var i = 0
  while i < n {
    let line = "const url" + int.to_string(i) + " = fetch(\"https://api.example.com/v1/items\") // note"
    let parts = line.split(" ")
    let kept = list.filter(parts, (p) => p.contains("a") or p.len() > 3)
    let lens = list.map(kept, (p) => p.len())
    total = total + list.fold(lens, 0, (a, x) => a + x)
    if line.contains("fetch") then { total = total + 1 } else ()
    i = i + 1
  }
  total
}
"#;

fn loop_probe(k: u32) -> String {
    format!("{LOOP}\nfn probe() -> Int = work({k})\n")
}

fn contains_probe(needle: &str, k: u32) -> String {
    format!(
        r#"fn go(s: String, k: Int, acc: Int) -> Int = if k == 0 then acc
else go(s, k - 1, acc + (if string.contains(s, "{needle}") then 1 else 0))

fn probe() -> Int = go("const url7 = fetch(\"https://api.example.com/v1/items\") // note", {k}, 0)
"#
    )
}

/// (fuel consumed, result) of `probe()` in the library build of `src`.
fn fuel_of(src: &str) -> (u64, i64) {
    let ir = almide_spine::s5::lower_to_ir("string_list_loop_cost.almd", src).expect("front");
    let (bytes, _) = almide_wasm::emit_library_with_ops(&ir).expect("emit");
    let mut config = wasmtime::Config::new();
    config.consume_fuel(true);
    let engine = wasmtime::Engine::new(&config).expect("engine");
    let module = wasmtime::Module::new(&engine, &bytes).expect("module");
    let mut linker = wasmtime::Linker::new(&engine);
    linker.define_unknown_imports_as_traps(&module).expect("imports");
    let mut store = wasmtime::Store::new(&engine, ());
    const BUDGET: u64 = 1 << 40;
    store.set_fuel(BUDGET).expect("fuel");
    let inst = linker.instantiate(&mut store, &module).expect("instantiate");
    let f = inst.get_typed_func::<(), i64>(&mut store, "probe").expect("probe export");
    let before = store.get_fuel().expect("fuel");
    let out = f.call(&mut store, ()).expect("probe runs");
    (before - store.get_fuel().expect("fuel"), out)
}

const K: u32 = 64;

/// The fuel of one unit (an iteration or a call) averaged over K, and the
/// K-unit answer.
fn per_unit(probe: impl Fn(u32) -> String) -> (f64, i64) {
    let (with, out) = fuel_of(&probe(K));
    let (without, base) = fuel_of(&probe(0));
    assert_eq!(base, 0, "the probe with no units answered wrong");
    (with.saturating_sub(without) as f64 / f64::from(K), out)
}

#[test]
fn the_issue_loop_costs_no_more_an_iteration_than_at_0_67_1() {
    let (fuel, total) = per_unit(loop_probe);
    println!("RATCHET #4430 loop fuel an iteration: {fuel:.0} (answer {total})");
    // Lines "const url0 …" to "const url63 …": the kept pieces' lengths and
    // one per `fetch`, as the native leg answers.
    assert_eq!(total, 3_574, "the loop answered wrong");
    assert!(fuel > 1_000.0, "the loop cost {fuel:.0} fuel an iteration — too little to gate on");
    assert!(
        fuel <= 13_335.0,
        "the #4430 loop cost {fuel:.0} fuel an iteration: 0.67.1 paid 13,335, \
         develop before the fix 14,457 (a failure table per contains, a call per byte)"
    );
}

/// One `contains` cell: the per-call fuel against its line.
fn contains_cell(needle: &str, hit: bool, line: f64, why: &str) {
    let (fuel, hits) = per_unit(|k| contains_probe(needle, k));
    println!("RATCHET #4430 contains({needle:?}) fuel a call: {fuel:.0}");
    assert_eq!(hits, if hit { i64::from(K) } else { 0 }, "contains({needle:?}) answered wrong");
    assert!(fuel > 20.0, "contains({needle:?}) cost {fuel:.0} fuel — too little to gate on");
    assert!(fuel <= line, "contains({needle:?}) cost {fuel:.0} fuel a call, line {line}: {why}");
}

#[test]
fn a_one_byte_needle_searches_without_a_table() {
    // 0.67.1 1,491; before the fix 2,115; now 790.
    contains_cell("a", true, 1_491.0, "a one-byte needle never reads the failure table");
}

#[test]
fn a_longer_needle_hit_at_its_first_candidate_builds_no_table() {
    // 0.67.1 895; before the fix 1,699; now 567.
    contains_cell("fetch", true, 895.0, "the outright compare at the first `f` answers it");
}

#[test]
fn a_needle_whose_first_byte_is_absent_walks_once() {
    // 0.67.1 2,981; before the fix 4,254; now 1,451.
    contains_cell("zzz", false, 2_981.0, "no byte can start a match, so no table and no step calls");
}
