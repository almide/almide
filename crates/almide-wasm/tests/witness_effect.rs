//! #2758 (#1696 step 4) — EFFECT frames in the structural witness. An
//! effect fn's body lowers at its raw ok type and func.rs wraps the value in
//! the ok carrier: the raw value is recorded exactly as a pure tail is (an
//! owned value's credit moves into the carrier's slot, `im`; a bound Var is
//! shared and moves, `am`), and the carrier itself is born and moves out of
//! the frame (`im`). A Unit body answers ok(()) with a carrier and no
//! payload. The exits that are not hooks yet — the early `!` and a raw
//! position `ok` / `err` — decline with a counted reason.

const PROGRAM: &str = r#"effect fn echo(s: String) -> String = s

effect fn fresh(n: Int) -> List[Int] = [n, n]

effect fn noop(n: Int) -> Unit = {
  let xs = [n]
  let _ = list.len(xs)
}

effect fn relay(s: String) -> String = echo(s)!

effect fn checked(n: Int) -> Int = if n < 0 then err("neg") else ok(n)

effect fn main() -> Unit = {
  let a = echo("a")!
  let b = fresh(2)!
  noop(1)!
  let c = relay("c")!
  let d = checked(3)!
  println(a)
  println(c)
}
"#;

fn witnesses() -> std::collections::BTreeMap<String, String> {
    let ir = almide_spine::s5::lower_to_ir("effect.almd", PROGRAM).expect("front");
    almide_wasm::witness::start_collecting();
    let _ = almide_wasm::emit_program(&ir).expect("the structural leg lowers the probe");
    almide_wasm::witness::take().into_iter().collect()
}

#[test]
fn effect_frames_witness_the_carrier_and_unhooked_exits_decline() {
    // ONE test: the witness sink is process-global.
    let w = witnesses();
    let expect = [
        // The borrowed param is shared into the slot; the carrier moves out.
        ("echo", "am\nim\n"),
        // The fresh spine's credit moves into the slot; the carrier moves out.
        ("fresh", "im\nim\n"),
        // A Unit body: the local is born and released, the carrier moves out.
        ("noop", "id\nim\n"),
    ];
    for (name, cert) in expect {
        let got = w.get(name).unwrap_or_else(|| panic!("{name} must be witnessed; got {:?}", w.get(name)));
        assert_eq!(got, cert, "{name}");
        assert!(
            almide_verify::check(almide_verify::Property::Ownership, got.as_bytes()),
            "{name}: the portable checker must accept {got:?}"
        );
    }
    assert_eq!(w.get("relay").map(String::as_str), Some("!decline:tail:Unwrap\n"));
    assert_eq!(w.get("checked").map(String::as_str), Some("!decline:effect:carrier\n"));
}
