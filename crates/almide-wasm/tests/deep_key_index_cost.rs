//! #4887 — a Set element or Map key of a record, variant or tuple type
//! (a "deep key") took the linear `$scan_deep` walk on every insert and
//! lookup, so building a set of n distinct records was O(n²) on wasm
//! (40,000 BigInts: 4.21 s). Such a key now takes the hash index when its
//! type qualifies (map_index_deep.rs).
//!
//! The gate is an exact COUNT, no clock: wasmtime fuel (one unit per
//! operator) of the build-and-probe alone — the probe minus the same probe
//! with no set — at n and 4n. Linear predicts a ratio of ~4, the scan ~16;
//! the line is 6. The answers pin the set's observables on both sizes.
//!
//! The flood case builds a set of tuples crafted to share their first 14
//! hash bits, so every key starts its probe at the same slot: the block
//! must leave the hash index for the ordered one (#4104) ordered by the
//! tuple's own total order, and stay O(n log n). The hash is mirrored here,
//! and its constants are checked against map_index_deep.rs, so a changed
//! hash cannot quietly stop colliding.

const DEEP_MUL: u64 = 0x9E37_79B9_7F4A_7C15;
const DEEP_SHIFT: u32 = 29;
const DEEP_SEED: u64 = 0x243F_6A88_85A3_08D3;
const HASH_STR_MUL: u64 = 0x9E37_79B1;

/// The deep hash of the tuple `(a, b)` of two Ints, as the emitted
/// `$named_hashty` computes it.
fn tuple_hash(a: i64, b: i64) -> u32 {
    let mut acc = DEEP_SEED;
    for x in [a as u64, b as u64] {
        acc = (acc ^ x).wrapping_mul(DEEP_MUL);
        acc ^= acc >> DEEP_SHIFT;
    }
    let x = (acc >> 32) ^ acc;
    (x.wrapping_mul(HASH_STR_MUL) >> 32) as u32
}

#[test]
fn the_mirrored_hash_is_the_emitters() {
    let src = std::fs::read_to_string(concat!(env!("CARGO_MANIFEST_DIR"), "/src/map_index_deep.rs")).expect("read map_index_deep.rs");
    for c in ["DEEP_MUL: u64 = 0x9E37_79B9_7F4A_7C15", "DEEP_SHIFT: i64 = 29", "DEEP_SEED: u64 = 0x243F_6A88_85A3_08D3"] {
        assert!(src.contains(c), "map_index_deep.rs no longer has `{c}`: update this test's mirror");
    }
    let idx = std::fs::read_to_string(concat!(env!("CARGO_MANIFEST_DIR"), "/src/map_index.rs")).expect("read map_index.rs");
    assert!(idx.contains("HASH_STR_MUL: u64 = 0x9E37_79B1"), "the fold constant moved: update this test's mirror");
}

/// (fuel consumed, result) of `probe()` in the library build of `src`.
fn fuel_of(src: &str) -> (u64, i64) {
    let ir = almide_spine::s5::lower_to_ir("deep_key_index_cost.almd", src).expect("front");
    let (bytes, _) = almide_wasm::emit_library_with_ops(&ir).expect("emit");
    let mut config = wasmtime::Config::new();
    config.consume_fuel(true);
    let engine = wasmtime::Engine::new(&config).expect("engine");
    let module = wasmtime::Module::new(&engine, &bytes).expect("module");
    let mut linker = wasmtime::Linker::new(&engine);
    linker.define_unknown_imports_as_traps(&module).expect("imports");
    let mut store = wasmtime::Store::new(&engine, ());
    const BUDGET: u64 = 1 << 42;
    store.set_fuel(BUDGET).expect("fuel");
    let inst = linker.instantiate(&mut store, &module).expect("instantiate");
    let f = inst.get_typed_func::<(), i64>(&mut store, "probe").expect("probe export");
    let before = store.get_fuel().expect("fuel");
    let out = f.call(&mut store, ()).expect("probe runs");
    (before - store.get_fuel().expect("fuel"), out)
}

const RECORDS: &str = r#"type Point = { x: Int, name: String }
type Shape = | Dot | Box(Int, Int) | Named(String, List[Int])
"#;

/// Build a set of n distinct records and a map keyed by n distinct variant
/// values, then probe both. `built = false` runs the same list work with
/// no set or map, to subtract.
fn records_probe(n: u32, built: bool) -> String {
    let body = if built {
        r#"let s = set.from_list(pts)
  let m = map.from_list(shapes |> list.map((sh) => (sh, 1)))
  let hits = list.range(0, @N@) |> list.filter((i) => set.contains(s, { x: i, name: "p${i % 7}" })) |> list.len
  let miss = if set.contains(s, { x: 1, name: "nope" }) then 1 else 0
  set.len(s) * 1000000 + hits * 10 + miss + map.len(m) * 100000000000 + (map.get(m, Box(2, 1)) ?? 0)"#
    } else {
        r#"let hits = list.range(0, @N@) |> list.filter((i) => i % 7 == i % 7) |> list.len
  list.len(pts) * 1000000 + hits * 10 + list.len(shapes) * 100000000000 + 1"#
    };
    format!(
        "{RECORDS}fn probe() -> Int = {{
  let pts = list.range(0, @N@) |> list.map((i) => {{ x: i, name: \"p${{i % 7}}\" }})
  let shapes = list.range(0, @N@) |> list.map((i) => if i % 2 == 0 then Box(i, 1) else Named(\"n${{i}}\", [i]))
  {body}
}}
"
    )
    .replace("@N@", &n.to_string())
}

/// The fuel of the build and probes alone at n, and the probe's answer.
fn records_fuel(n: u32) -> (u64, i64) {
    let (with, out) = fuel_of(&records_probe(n, true));
    let (without, base) = fuel_of(&records_probe(n, false));
    let n64 = i64::from(n);
    assert_eq!(base, n64 * 1_000_000 + n64 * 10 + n64 * 100_000_000_000 + 1, "the probe without the set answered wrong");
    (with - without, out)
}

#[test]
fn a_set_of_records_and_a_map_of_variants_build_in_linear_fuel() {
    const N: u32 = 1_000;
    let (small, out_s) = records_fuel(N);
    let (large, out_l) = records_fuel(4 * N);
    for (n, out) in [(N, out_s), (4 * N, out_l)] {
        let n = i64::from(n);
        assert_eq!(out, n * 1_000_000 + n * 10 + n * 100_000_000_000 + 1, "the set and map at n={n} answered wrong");
    }
    assert!(small > 10 * u64::from(N), "the build at n={N} cost {small} fuel — too little to gate on");
    let ratio = large as f64 / small as f64;
    assert!(ratio <= 6.0, "deep-key set and map fuel grew {ratio:.2}x for 4x the keys ({small} -> {large}): linear predicts ~4, the scan ~16");
}

/// n tuples `(0, b)` whose hashes share their low 14 bits.
fn colliding_tuples(n: usize) -> Vec<i64> {
    (0i64..).filter(|b| tuple_hash(0, *b) & 0x3FFF == 0).take(n).collect()
}

fn flood_probe(bs: &[i64], built: bool) -> String {
    let list = bs.iter().map(|b| b.to_string()).collect::<Vec<_>>().join(", ");
    let body = if built {
        "let s = set.from_list(keys)
  let hits = bs |> list.filter((b) => set.contains(s, (0, b))) |> list.len
  let miss = if set.contains(s, (1, 0)) then 1 else 0
  set.len(s) * 1000000 + hits * 10 + miss"
    } else {
        "let hits = bs |> list.filter((b) => b == b) |> list.len
  list.len(keys) * 1000000 + hits * 10"
    };
    format!(
        "fn probe() -> Int = {{
  let bs = [{list}]
  let keys = bs |> list.map((b) => (0, b))
  {body}
}}
"
    )
}

fn flood_fuel(n: usize) -> (u64, i64) {
    let bs = colliding_tuples(n);
    let (with, out) = fuel_of(&flood_probe(&bs, true));
    let (without, base) = fuel_of(&flood_probe(&bs, false));
    let n64 = n as i64;
    assert_eq!(base, n64 * 1_000_000 + n64 * 10, "the flood probe without the set answered wrong");
    (with - without, out)
}

#[test]
fn colliding_tuple_keys_leave_the_hash_index_for_the_ordered_one() {
    const N: usize = 600;
    // The crafted keys do collide under the mirrored hash.
    assert!(colliding_tuples(N).iter().all(|b| tuple_hash(0, *b) & 0x3FFF == 0));
    let (small, out_s) = flood_fuel(N);
    let (large, out_l) = flood_fuel(4 * N);
    for (n, out) in [(N, out_s), (4 * N, out_l)] {
        let n = n as i64;
        assert_eq!(out, n * 1_000_000 + n * 10, "the flooded set at n={n} answered wrong");
    }
    let ratio = large as f64 / small as f64;
    assert!(ratio <= 6.0, "a flooded deep-key set grew {ratio:.2}x for 4x the keys ({small} -> {large}): the ordered fallback predicts ~4.5, a stuck probe chain ~16");
}
