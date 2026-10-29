//! #3642 — `value.merge(a, b)` on the wasm leg costs O(|A|·|B|), as native's
//! `pa.iter().position(..)` per pair of B does, not O(|A|²). `$value_merge`
//! (value_helpers.rs) asked, for EVERY pair of A, whether it is the first
//! pair of its key in A (a scan of A's prefix) before asking whether B has
//! the key at all; a 2-key merge into an 80k-key object took 8.8 s on wasm
//! against 0.35 s native.
//!
//! The gate is an exact COUNT, no clock: wasmtime fuel (one unit per
//! operator) of the merge alone — the probe built with the merge minus the
//! probe built without it, both deterministic — at |A| = n and 4n with |B|
//! fixed at 2. Linear in |A| predicts a ratio of ~4, the quadratic prefix
//! scan ~16; the line is 6. The probe's answers pin the fold's observables
//! (B's value wins at A's position, B's new key appends once) on both sizes.

fn probe(n: u32, merge: bool) -> String {
    let r = if merge { "value.merge(a, b)" } else { "a" };
    format!(
        r#"fn probe() -> Int = {{
  let a = value.object(list.range(0, {n}) |> list.map((i) => ("k${{i}}", value.int(i))))
  let b = value.object([("k1", value.int(-1)), ("new", value.int(7))])
  let r = {r}
  let ks = list.map(value.as_object(r) ?? [], (kv) => kv.0)
  let k1 = value.as_int(value.get(r, "k1") ?? value.null()) ?? 99
  list.len(ks) * 1000 + k1
}}
"#
    )
}

/// (fuel consumed, result) of `probe()` in the library build of `src`.
fn fuel_of(src: &str) -> (u64, i64) {
    let ir = almide_spine::s5::lower_to_ir("value_merge_cost.almd", src).expect("front");
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

/// The fuel of the merge alone at |A| = n, and the merged probe's answer.
fn merge_fuel(n: u32) -> (u64, i64) {
    let (with, out) = fuel_of(&probe(n, true));
    let (without, base) = fuel_of(&probe(n, false));
    assert_eq!(base, i64::from(n) * 1000 + 1, "the probe without the merge answered wrong");
    (with - without, out)
}

#[test]
fn value_merge_is_linear_in_the_left_object() {
    const N: u32 = 2_000;
    let (small, out_s) = merge_fuel(N);
    let (large, out_l) = merge_fuel(4 * N);
    // B's "k1" overrides A's in place (-1), "new" appends: |A| + 1 keys.
    assert_eq!(out_s, i64::from(N + 1) * 1000 - 1, "merge into {N} keys answered wrong");
    assert_eq!(out_l, i64::from(4 * N + 1) * 1000 - 1, "merge into {} keys answered wrong", 4 * N);
    // Anti-vacuous: the merge must be measurable at the small size.
    assert!(small > 10 * u64::from(N), "the merge at n={N} cost {small} fuel — too little to gate on");
    let ratio = large as f64 / small as f64;
    println!("RATCHET value.merge fuel n={N}: {small}, n={}: {large}, ratio {ratio:.2}", 4 * N);
    assert!(
        ratio <= 6.0,
        "value.merge fuel grew {ratio:.2}x for 4x the left object ({small} -> {large}): \
         linear predicts ~4, the per-pair prefix scan of #3642 ~16"
    );
}
