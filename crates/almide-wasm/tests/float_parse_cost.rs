//! ADR-0033: the cost of `float.parse` on the wasm leg for the coordinate-shaped
//! literals of a float-heavy JSON document (17–18 significant digits, so every
//! one misses the <= 15-digit fast path and takes the high-precision decimal).
//! The self-hosted slow path shifted its 800-digit decimal ONE bit per pass —
//! ~60 full-array passes per literal, 195,871 fuel — and a 2.25 MB coordinate
//! document took 1.9 s to parse on wasm against 29 ms native. It now shifts up
//! to 59 bits right / 26 bits left per pass.
//!
//! The gate is an exact COUNT, no clock: wasmtime fuel (one unit per operator)
//! of the parses alone — the probe minus the same probe without the parse —
//! divided by the number of literals. The probe's answer pins every parsed
//! value's bits against the host's correctly-rounded parse.

const TOKENS: [&str; 16] = [
    "-110.855051165015382",
    "46.033966956980080",
    "-81.415897426413153",
    "42.897451466701710",
    "-91.770619612397979",
    "54.627556676503417",
    "-134.780096770276373",
    "60.297429327576808",
    "-136.625390740221349",
    "57.345827346495433",
    "-133.713011878284306",
    "43.628520533754603",
    "-101.793272977173743",
    "73.074084986881530",
    "-128.857823496531893",
    "48.929558584280585",
];

const ROUNDS: u32 = 8;

fn probe(parse: bool) -> String {
    let list = TOKENS.iter().map(|t| format!("\"{t}\"")).collect::<Vec<_>>().join(", ");
    let v = if parse { "float.parse(t) ?? 0.0" } else { "0.5" };
    format!(
        r#"fn toks() -> List[String] = [{list}]

fn probe() -> Int = list.range(0, {ROUNDS})
  |> list.map((_) => toks() |> list.map((t) => float.to_bits({v}) % 1000) |> list.sum)
  |> list.sum
"#
    )
}

/// (fuel consumed, result) of `probe()` in the library build of `src`.
fn fuel_of(src: &str) -> (u64, i64) {
    let ir = almide_spine::s5::lower_to_ir("float_parse_cost.almd", src).expect("front");
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

/// The fuel per 17–18-digit literal float.parse may spend. The one-bit
/// shifter spent 195,871; the multi-bit shifter spends 15,244 (probe run
/// 42607999728); the exact division path, which these literals now take,
/// spends 1,839 (probe run 42608518430). Only moves DOWN.
const CEILING_PER_FLOAT: u64 = 2_000;

#[test]
fn float_parse_long_literals_cost_a_few_passes() {
    let (with, out) = fuel_of(&probe(true));
    let (without, base) = fuel_of(&probe(false));
    let half = (0.5f64.to_bits() as i64) % 1000;
    assert_eq!(base, half * TOKENS.len() as i64 * i64::from(ROUNDS), "the probe without the parse answered wrong");
    let want: i64 = TOKENS
        .iter()
        .map(|t| (t.parse::<f64>().unwrap().to_bits() as i64) % 1000)
        .sum::<i64>()
        * i64::from(ROUNDS);
    assert_eq!(out, want, "a parsed literal's bits differ from the correctly-rounded value");
    let n = TOKENS.len() as u64 * u64::from(ROUNDS);
    let per = (with - without) / n;
    println!("RATCHET float.parse fuel per 17-18 digit literal: {per} (n={n}, ceiling {CEILING_PER_FLOAT})");
    assert!(
        per <= CEILING_PER_FLOAT,
        "float.parse spent {per} fuel per long literal (ceiling {CEILING_PER_FLOAT}): \
         the high-precision decimal is back to a few bits per pass"
    );
}
