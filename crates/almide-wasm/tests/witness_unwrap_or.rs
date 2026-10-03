//! #2755 (#1696 step 4) — `r ?? fallback` and the meter prims in the
//! structural witness. `??` is a two-arm branch site (data.rs): an OWNED join
//! takes one credit from each arm — the fresh fallback moves in on the
//! none / err arm, the payload view takes its share and moves on the other —
//! and the join is an owned value its consumer records. A BORROWED join (a
//! var fallback) is a view of a bound block, shared like any extraction. The
//! deterministic-meter prims (`fan.bounded`'s enter / exit) are scalar,
//! global-only runtime calls with no RC site: a region body certifies. The
//! meter's CUT runs the frame's exit plan before it returns (#3072), so a
//! metered frame that owns a block at a charge point certifies too.

const PROGRAM: &str = r#"fn get(i: Int) -> List[Int]? = if i > 0 then some([i]) else none

fn fresh(i: Int) -> Int = list.len(get(i) ?? [0])

fn borrowed(i: Int, d: List[Int]) -> Int = {
  let xs = get(i) ?? d
  list.len(xs)
}

fn heavy(n: Int) -> Int = {
  var acc = 0
  for i in 0..<n {
    acc = acc + i
  }
  acc
}

fn wordy(n: Int) -> Int = {
  let s = int.to_string(n) + "!"
  var acc = 0
  for i in 0..<n {
    acc = acc + string.len(s)
  }
  acc
}

effect fn main() -> Unit = {
  let a = fan.bounded(compute.ms(100)) { heavy(10) } ?? -1
  let b = fan.bounded(compute.ms(100)) { wordy(10) } ?? -1
  println("${fresh(1)} ${fresh(0)} ${borrowed(1, [4])} ${a} ${b}")
}
"#;

fn witnesses() -> std::collections::BTreeMap<String, String> {
    let ir = almide_spine::s5::lower_to_ir("unwrap_or.almd", PROGRAM).expect("front");
    almide_wasm::witness::start_collecting();
    let _ = almide_wasm::emit_program(&ir).expect("the structural leg lowers the probe");
    almide_wasm::witness::take().into_iter().collect()
}

fn accepted(cert: &str) -> bool {
    almide_verify::check(almide_verify::Property::Ownership, cert.as_bytes())
}

#[test]
fn unwrap_or_joins_and_meter_prims_witness_exactly() {
    // ONE test: the witness sink is process-global.
    let w = witnesses();
    let get = |n: &str| w.get(n).map(String::as_str).unwrap_or("<none>").to_string();
    // The carrier is born and released (`id`); the fallback moves into the
    // join on one arm, the payload view shares and moves on the other; the
    // owned join is lent to `list.len` and released (`id`).
    assert_eq!(get("fresh"), "ibd\n{|im}\n{|am}\nid\n");
    assert!(accepted(&get("fresh")));
    // A var fallback leaves the join a view of a bound carrier's payload or
    // of the fallback (#2755): the bind's share lands on that view and the
    // epilogue releases it (`ad`); the named carrier is born and released.
    assert_eq!(get("borrowed"), "\nibd\nad\n");
    assert!(accepted(&get("borrowed")));
    // The meter's cut runs the frame's exit plan (#3072, C-320): a metered
    // frame that owns a block at a charge point certifies.
    assert!(!get("wordy").starts_with('!') && accepted(&get("wordy")), "{:?}", get("wordy"));
    // The region bodies and main certify through the meter prims.
    for name in ["__almd_bounded_1", "__almd_bounded_2", "main"] {
        let c = get(name);
        assert!(!c.starts_with('!') && accepted(&c), "{name}: {c:?}");
    }
}
