//! ADR-0033 phase 0: the JSON ratchet. jsonround (records built as one
//! `Value`, stringified, parsed back, summed) split into its four phases, with
//! two DETERMINISTIC counts per phase pinned in golden/json-ratchet.txt:
//!
//! - `fuel`: the wasmtime fuel (one unit per operator) the phase consumes in
//!   the library build of the probe (the phase's probe minus the probe of the
//!   phases before it);
//! - `allocs`: the `$alloc` calls the phase makes in the armed program build
//!   (`alloc_count`), and `frees` beside it.
//!
//! Both are exact for a given compiler: the module bytes fix them. The rows
//! only move DOWN. A change that lowers a row re-records it in its own commit
//! (`ALMIDE_UPDATE_JSON_RATCHET=1 cargo test --release -p almide-wasm --test
//! json_ratchet`); the update REFUSES to write a row higher than its pin — a
//! rise is an owner ruling, written by hand with the ruling cited, never by a
//! tool. The world reference (run 42587113062) is printed beside the rows:
//! yyjson allocates 29 / 1 / 2 times for the build / stringify / parse of all
//! 50,000 records.
//!
//! The records are the json-world bench's (research/benchmark/perf/json-world),
//! at N = 2,000 so the interpreter-free fuel run stays fast; every count is
//! linear in N, and the ratchet prints the per-record figures.

mod harness;
use harness::run_wasm;

const N: u32 = 2_000;

fn record_src() -> String {
    format!(
        r#"import json

fn record(i: Int) -> Value = value.object([
  ("id", value.int(i)),
  ("name", value.string("user_${{i}}")),
  ("tags", value.array([value.string("t${{i % 7}}"), value.string("t${{i % 11}}")])),
  ("meta", value.object([
    ("score", value.int((i * 37) % 1000)),
    ("active", value.bool(i % 2 == 0)),
  ])),
])

fn score(r: Value) -> Int =
  value.get_int(value.get(r, "meta") ?? value.null(), "score") ?? 0

fn build() -> Value = value.array(list.range(0, {N}) |> list.map(record))
"#
    )
}

/// The four cumulative stages: each one's answer, as a library probe body.
const STAGES: [(&str, &str); 4] = [
    ("build", "list.len(value.as_array(build()) ?? [])"),
    ("stringify", "string.len(json.stringify(build()))"),
    ("parse", "list.len(value.as_array(json.parse(json.stringify(build())) ?? value.null()) ?? [])"),
    ("sum", "(value.as_array(json.parse(json.stringify(build())) ?? value.null()) ?? []) |> list.map(score) |> list.sum"),
];

/// (fuel consumed, answer) of `probe()` in the library build.
fn fuel_of(stage: &str) -> (u64, i64) {
    let src = format!("{}\nfn probe() -> Int = {stage}\n", record_src());
    let ir = almide_spine::s5::lower_to_ir("json_ratchet.almd", &src).expect("front");
    let (bytes, _) = almide_wasm::emit_library_with_ops(&ir).expect("emit");
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

/// (allocs, frees, stdout) of the armed PROGRAM build that prints the stage's answer.
fn allocs_of(stage: &str) -> (u64, u64, String) {
    let src = format!("{}\neffect fn main() -> Unit = {{\n  let answer = {stage}\n  println(\"${{answer}}\")\n}}\n", record_src());
    let ir = almide_spine::s5::lower_to_ir("json_ratchet.almd", &src).expect("front");
    let _guard = almide_wasm::alloc_count::CountGuard::set();
    let bytes = almide_wasm::emit_program(&ir).expect("emit");
    let r = run_wasm(&bytes).expect("run");
    assert_eq!(r.exit, 0, "the stage program exits 0: {}", r.stdout);
    let c = r.alloc_count.expect("the armed module exports the counters");
    (c.allocs, c.frees, r.stdout.trim().to_string())
}

fn golden() -> std::path::PathBuf {
    std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/golden/json-ratchet.txt")
}

/// `row -> value` from the golden file (`<value>\t<row>` lines, `#` comments).
fn read_pins() -> std::collections::BTreeMap<String, u64> {
    let text = std::fs::read_to_string(golden()).unwrap_or_default();
    text.lines()
        .filter(|l| !l.trim().is_empty() && !l.starts_with('#'))
        .map(|l| {
            let (v, k) = l.split_once('\t').expect("`<value>\\t<row>`");
            (k.to_string(), v.parse().expect("a count"))
        })
        .collect()
}

#[cfg_attr(debug_assertions, ignore = "the JSON ratchet is release-only (CI: release-shape job)")]
#[test]
fn json_phases_hold_their_counts() {
    // The answers each stage must give, so a count can never be pinned on a
    // wrong program: N records; the sum of (i * 37) % 1000 over 0..N.
    let sum: i64 = (0..i64::from(N)).map(|i| (i * 37) % 1000).sum();
    let mut got: Vec<(String, u64)> = Vec::new();
    let (mut prev_fuel, mut prev_allocs, mut prev_frees) = (0u64, 0u64, 0u64);
    for (name, stage) in STAGES {
        let (fuel, answer) = fuel_of(stage);
        let (allocs, frees, stdout) = allocs_of(stage);
        assert_eq!(stdout, answer.to_string(), "{name}: the program and the library probe disagree");
        match name {
            "build" | "parse" => assert_eq!(answer, i64::from(N), "{name}: {N} records"),
            "sum" => assert_eq!(answer, sum, "sum: the scores of {N} records"),
            _ => assert!(answer > i64::from(N) * 40, "stringify: a text of {answer} bytes is too short"),
        }
        let phase = (fuel.saturating_sub(prev_fuel), allocs.saturating_sub(prev_allocs), frees.saturating_sub(prev_frees));
        println!(
            "RATCHET json {name:<9} fuel {:>10} ({:>7.1}/record)  allocs {:>7} ({:>5.2}/record)  frees {:>7}",
            phase.0,
            phase.0 as f64 / f64::from(N),
            phase.1,
            phase.1 as f64 / f64::from(N),
            phase.2
        );
        got.push((format!("{name} fuel"), phase.0));
        got.push((format!("{name} allocs"), phase.1));
        got.push((format!("{name} frees"), phase.2));
        (prev_fuel, prev_allocs, prev_frees) = (fuel, allocs, frees);
    }
    println!("RATCHET json world reference (run 42587113062, 50,000 records): yyjson allocs build 29, stringify 1, parse 2");
    let pins = read_pins();
    if std::env::var("ALMIDE_UPDATE_JSON_RATCHET").is_ok() {
        let raised: Vec<String> = got
            .iter()
            .filter_map(|(k, v)| pins.get(k).filter(|p| v > *p).map(|p| format!("{k}: {p} -> {v}")))
            .collect();
        assert!(
            raised.is_empty(),
            "the update refuses to RAISE a pinned row (an owner ruling, written by hand): {raised:?}"
        );
        let mut out = String::from(
            "# ADR-0033 JSON ratchet: jsonround's phases at N = 2000 records (json_ratchet.rs).\n\
             # <value>\\t<row>. Rows only move down; a rise is written by hand with the owner ruling cited.\n",
        );
        for (k, v) in &got {
            out.push_str(&format!("{v}\t{k}\n"));
        }
        std::fs::write(golden(), out).expect("write the ratchet");
        return;
    }
    let mut offences = Vec::new();
    for (k, v) in &got {
        match pins.get(k) {
            None => offences.push(format!("{k}: {v} has no pin — record it (ALMIDE_UPDATE_JSON_RATCHET=1)")),
            Some(p) if v > p => offences.push(format!("{k}: {v} > the pinned {p} — the JSON ratchet only moves down")),
            Some(p) if v < p => offences.push(format!(
                "{k}: {v} < the pinned {p} — the gain is real; re-record the lower row in its own commit"
            )),
            _ => {}
        }
    }
    assert!(offences.is_empty(), "JSON ratchet:\n  {}", offences.join("\n  "));
}

/// The retention probe (ADR-0033 D9, falsifier 3): M documents of about S
/// bytes are parsed one after another, and one short string of each is kept.
/// Under the sub-value-keeps-its-document rule each kept string pins its whole
/// document, so the retained heap grows by about M × S; with per-node blocks it
/// grows by M × the key. The probe reports the retained bytes per kept byte —
/// the adversarial program of D9, recorded every run, not pinned (families
/// RAISE it by design; falsifier 3 judges real programs, not this one).
fn retention_src(keep_string: bool) -> String {
    let keep = if keep_string { "value.as_string(value.get(d, \"k\") ?? value.null()) ?? \"\"" } else { "\"\"" };
    format!(
        r#"import json

fn doc(i: Int) -> String = {{
  let pad = list.range(0, 64) |> list.map((j) => "\"f${{j}}\":\"${{i}}-${{j}}-xxxxxxxxxxxxxxxx\"") |> list.join(",")
  "{{\"k\":\"key${{i}}\",${{pad}}}}"
}}

effect fn main() -> Unit = {{
  let kept = list.range(0, 200) |> list.map((i) => {{
    let d = json.parse(doc(i)) ?? value.null()
    {keep}
  }})
  println("${{list.len(kept)}} ${{kept |> list.map((s) => string.len(s)) |> list.sum}}")
}}
"#
    )
}

#[cfg_attr(debug_assertions, ignore = "the JSON ratchet is release-only (CI: release-shape job)")]
#[test]
fn json_retention_probe_reports() {
    let run = |keep: bool| {
        let ir = almide_spine::s5::lower_to_ir("json_retention.almd", &retention_src(keep)).expect("front");
        let bytes = almide_wasm::emit_program(&ir).expect("emit");
        let r = run_wasm(&bytes).expect("run");
        assert_eq!(r.exit, 0, "the retention probe exits 0: {}", r.stdout);
        (r.heap_end.expect("__heap export present"), r.stdout.trim().to_string())
    };
    let (kept_heap, kept_out) = run(true);
    let (none_heap, none_out) = run(false);
    let kept_bytes: u64 = kept_out.split_whitespace().nth(1).and_then(|s| s.parse().ok()).expect("kept byte count");
    assert_eq!(none_out, "200 0", "the control keeps nothing");
    assert!(kept_bytes > 0, "the probe keeps its keys: {kept_out}");
    let retained = kept_heap.saturating_sub(none_heap);
    println!(
        "RETENTION json: heap {kept_heap} keeping 200 keys ({kept_bytes} bytes) vs {none_heap} keeping none: \
         {retained} bytes retained, {:.1} per kept byte",
        retained as f64 / kept_bytes as f64
    );
}
