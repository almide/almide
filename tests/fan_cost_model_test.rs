//! #3341: the fan cost model is ONE formula on both legs. The native runtime
//! (`runtime/rs/src/list.rs` `almide_rt_fan_plan`) and the embedded wasm host
//! (`crates/almide-wasm-run/src/host_fan.rs` `plan`) each carry a copy — the
//! native one is emitted into every program, so it cannot import the host's.
//! This test pins both copies to the measured constants in
//! `docs/benchmarks/fan-cost-model.txt` and to each other's formula, so the
//! legs cannot drift apart and nobody changes a constant without the ledger.

fn read(rel: &str) -> String {
    std::fs::read_to_string(std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join(rel)).unwrap_or_else(|e| panic!("{rel}: {e}"))
}

/// `key = value` from the ledger (comments are `#` lines).
fn ledger(key: &str) -> u128 {
    read("docs/benchmarks/fan-cost-model.txt")
        .lines()
        .filter(|l| !l.trim_start().starts_with('#'))
        .find_map(|l| l.split_once('=').filter(|(k, _)| k.trim() == key).map(|(_, v)| v.trim().replace('_', "")))
        .unwrap_or_else(|| panic!("ledger has no `{key} =` line"))
        .parse()
        .expect("a number")
}

/// The value of `const NAME: u128 = N;` in `src`.
fn constant(src: &str, name: &str) -> u128 {
    let at = src.find(&format!("{name}: u128 = ")).unwrap_or_else(|| panic!("no `{name}`"));
    let rest = &src[at + name.len() + ": u128 = ".len()..];
    rest[..rest.find(';').expect(";")].replace('_', "").parse().expect("a number")
}

/// The two formula lines (`let saved` / `let cost`) of a plan function,
/// with the leg's constant names normalised.
fn formula(src: &str) -> Vec<String> {
    src.lines()
        .map(str::trim)
        .filter(|l| l.starts_with("let saved = ") || l.starts_with("let cost = ") || l.starts_with("if saved > cost"))
        .map(|l| l.replace("ALMIDE_FAN_", "FAN_").replace("almide_rt_list_par_workers", "worker_count"))
        .collect()
}

#[test]
fn both_legs_carry_the_ledgers_constants_and_the_same_formula() {
    let native = read("runtime/rs/src/list.rs");
    let host = read("crates/almide-wasm-run/src/host_fan.rs");
    for (key, name) in [("offer_ns", "FAN_OFFER_NS"), ("worker_ns", "FAN_WORKER_NS")] {
        let want = ledger(key);
        assert_eq!(constant(&native, &format!("ALMIDE_{name}")), want, "native {name} vs ledger {key}");
        assert_eq!(constant(&host, name), want, "wasm host {name} vs ledger {key}");
    }
    let (fn_native, fn_host) = (formula(&native), formula(&host));
    assert_eq!(fn_native.len(), 3, "native formula lines: {fn_native:?}");
    assert_eq!(fn_native, fn_host, "the two legs' cost formulas differ");
}
