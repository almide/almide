//! #4399 — a regex operation on the wasm leg costs no more than it did before
//! the C-395 acceptance check and the walk's step budget came in. 0.69.0-rc1
//! re-ran the whole check on every call (a parse that allocated a block and a
//! String) and counted every step against a budget it reloaded each time, and
//! both legs of the #4392 program ran about 5% slower than 0.68.0-rc2; a short
//! subject, where the per-call check is most of the work, ran twice as slow.
//! The check and the budget stay exactly as they are; what changed is what each
//! call pays around them.
//!
//! The gate is an exact COUNT, no clock: wasmtime fuel (one unit per operator)
//! of K calls minus the same probe with none, divided by K. Two shapes, both
//! the #4392 rule `k0-[A-Za-z0-9_-]{20,}`:
//! - `find_all` over a 64-byte line it never matches (the #4399 program): one
//!   attempt per start position, so the walk's per-attempt work shows;
//! - `is_match` on a 2-byte subject: one or two attempts, so the per-call work
//!   (the check, the machine block) shows.
//!
//! Measured (fuel a call): find_all 92,000 at 0.69.0-rc2, 5,077 now, against
//! about 84,000 for 0.68.0-rc2's engine (no check, no budget; from the #4399
//! program's total); is_match 8,024 at 0.69.0-rc2, 4,153 now, against about
//! 4,600. The is_match line is 0.68.0-rc2's cost, the regression's own
//! definition. The find_all line keeps the per-call search work found once (the
//! first top-level alternative's end, the byte every match starts with) from
//! sliding back to a rescan per start position. A failure prints the measured
//! value.

fn probe(call: &str, subject: &str, k: u32) -> String {
    format!(
        r#"import regex

fn go(r: regex.Regex, s: String, k: Int, acc: Int) -> Int = if k == 0 then acc
else go(r, s, k - 1, acc + {call})

fn probe() -> Int = {{
  let r = regex.new("k0-[A-Za-z0-9_-]{{20,}}")
  let line = "const x = process.env.FOO + 'hello world this is a line of code'"
  let short = "k3"
  go(r, {subject}, {k}, 0) + string.len(short)
}}
"#
    )
}

/// (fuel consumed, result) of `probe()` in the library build of `src`.
fn fuel_of(src: &str) -> (u64, i64) {
    let ir = almide_spine::s5::lower_to_ir("regex_call_cost.almd", src).expect("front");
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

/// The fuel of one `call` on `subject` (`line` or `short`), averaged over K
/// calls, and the K-call answer.
fn per_call(call: &str, subject: &str) -> (f64, i64) {
    const K: u32 = 64;
    let (with, out) = fuel_of(&probe(call, subject, K));
    let (without, base) = fuel_of(&probe(call, subject, 0));
    assert_eq!(base, 2, "the probe without calls answered wrong");
    (with.saturating_sub(without) as f64 / f64::from(K), out - base)
}

#[test]
fn regex_find_all_costs_no_more_per_attempt_than_before_the_check() {
    let (fuel, hits) = per_call("list.len(regex.find_all(r, s))", "line");
    assert_eq!(hits, 0, "find_all found a match the line does not have");
    assert!(fuel > 500.0, "find_all cost {fuel:.0} fuel a call — too little to gate on");
    assert!(
        fuel <= 8_000.0,
        "regex.find_all cost {fuel:.0} fuel a call over a 64-byte line: it was 5,077 \
         with the search work found once per call; 0.68.0-rc2's engine cost about \
         84,000, 0.69.0-rc2's 92,000 (#4399)"
    );
}

#[test]
fn regex_is_match_on_a_short_subject_costs_no_more_than_before_the_check() {
    let (fuel, hits) = per_call("if regex.is_match(r, s) then 1 else 0", "short");
    assert_eq!(hits, 0, "is_match found a match \"k3\" does not have");
    assert!(fuel > 100.0, "is_match cost {fuel:.0} fuel a call — too little to gate on");
    assert!(
        fuel <= 4_600.0,
        "regex.is_match cost {fuel:.0} fuel a call on a 2-byte subject: \
         0.68.0-rc2's engine cost about 4,600, 0.69.0-rc2's 8,024 (#4399)"
    );
}
