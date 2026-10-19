//! #3848 — `zlib.crc32` on the wasm leg costs a table lookup per byte, not eight
//! data-dependent branches per byte. The bit-at-a-time loop
//! (`if acc & 1 == 1 then 0xEDB88320 ^ (acc >> 1) else acc >> 1`, eight times a
//! byte) is a branch the predictor loses on half the bits; LLVM turns it into a
//! select on the native leg and Cranelift keeps the branch, so a 1.9 GB input
//! ran 41 s on wasm against 12.6 s native — the fuzzer's `Slow` finding.
//!
//! The gate is an exact COUNT, no clock: wasmtime fuel (one unit per operator)
//! of the checksum alone — the probe built with the crc32 minus the probe built
//! without it, both deterministic — divided by the input length. The bit loop
//! costs 212 operators a byte, the table loop 58; the line is 100. The
//! probe's answers pin the checksum on both the table path and the short path.

fn probe(n: u32, crc: bool) -> String {
    let r = if crc { "zlib.crc32(b)" } else { "bytes.len(b)" };
    format!(
        r#"import zlib
fn probe() -> Int = {{
  let b = bytes.pad_right(bytes.from_list([1, 7, 2, 2, 7]), {n}, 255)
  {r}
}}
"#
    )
}

/// (fuel consumed, result) of `probe()` in the library build of `src`.
fn fuel_of(src: &str) -> (u64, i64) {
    let ir = almide_spine::s5::lower_to_ir("zlib_crc32_cost.almd", src).expect("front");
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

/// The fuel of the checksum alone over n bytes, and the checksum.
fn crc_fuel(n: u32) -> (u64, i64) {
    let (with, out) = fuel_of(&probe(n, true));
    let (without, len) = fuel_of(&probe(n, false));
    assert_eq!(len, i64::from(n), "the probe without the checksum answered wrong");
    (with - without, out)
}

#[test]
fn zlib_crc32_costs_a_table_lookup_per_byte() {
    // Known answers from the reference CRC-32 (IEEE): both sides of the
    // short-input path, and the measured size.
    for (n, want) in [(255u32, 2380377564i64), (256, 1894285138), (257, 2048252071)] {
        assert_eq!(crc_fuel(n).1, want, "crc32 over {n} bytes answered wrong");
    }
    const N: u32 = 16_384;
    let (fuel, out) = crc_fuel(N);
    assert_eq!(out, 241196957, "crc32 over {N} bytes answered wrong");
    let per_byte = fuel as f64 / f64::from(N);
    println!("RATCHET zlib.crc32 fuel n={N}: {fuel}, {per_byte:.1} per byte");
    // Anti-vacuous: the checksum must be measurable.
    assert!(per_byte > 5.0, "crc32 cost {per_byte:.1} fuel a byte — too little to gate on");
    assert!(
        per_byte <= 100.0,
        "zlib.crc32 cost {per_byte:.1} fuel a byte: the table loop is 58, \
         the eight-branch bit loop 212"
    );
}
