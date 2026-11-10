//! Fused sum consumes the chain as one activation; intermediate owned map
//! values are discharged even when a filter skips the element.
#[test]
fn numeric_and_owned_stage_sum_certificates_are_accepted() {
    let src = r#"fn total(n: Int) -> Int = list.range(0, n)
      |> list.map((x) => x * 3) |> list.filter((x) => x % 2 == 0) |> list.sum
    fn lengths(xs: List[String]) -> Int = xs
      |> list.map((s) => s + "!") |> list.filter((s) => s != "drop!")
      |> list.map((s) => string.len(s)) |> list.sum
    fn main() -> Unit = println("${total(10)} ${lengths(["a", "drop", "bb"])}")"#;
    let ir = almide_spine::s5::lower_to_ir("witness_sum_fusion.almd", src).expect("front");
    almide_wasm::witness::start_collecting();
    almide_wasm::emit_program(&ir).expect("emit");
    let witnesses: std::collections::BTreeMap<_, _> = almide_wasm::witness::take().into_iter().collect();
    for name in ["total", "lengths"] {
        let cert = witnesses.get(name).expect("witnessed function");
        assert!(!cert.starts_with('!'), "{name}: {cert}");
        assert!(almide_verify::check(almide_verify::Property::Ownership, cert.as_bytes()), "{name}: {cert}");
    }
}

/// A bare captured List has no producer chain to expose. Its ordinary
/// call must keep the existing closure's RC witness (the corpus ratchet
/// caught an over-broad forwarding draft dropping this identity).
#[test]
fn captured_list_sum_retains_its_closure_witness() {
    let src = include_str!("../../../spec/wasm_cross/fan_block_rc_closure_capture.almd");
    let ir = almide_spine::s5::lower_to_ir("fan_block_rc_closure_capture.almd", src).expect("front");
    almide_wasm::witness::start_collecting();
    almide_wasm::emit_program(&ir).expect("emit");
    let witnesses: std::collections::BTreeMap<_, _> = almide_wasm::witness::take().into_iter().collect();
    let cert = witnesses.get("<lambda#1>").expect("captured list closure must remain witnessed");
    assert!(!cert.trim().is_empty(), "the capture must retain its RC events");
    assert!(almide_verify::check(almide_verify::Property::Ownership, cert.as_bytes()), "{cert}");
}
