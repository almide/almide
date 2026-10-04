//! #2758 (#1696 step 4) — `!` over an `err(..)` CONSTRUCTOR in the
//! structural witness: the explicit raise of an effect body. The carrier is
//! certainly fresh, so the `!` site takes its owned-carrier route
//! (witness_unwrap.rs): the block is born at the site (`i`), leaves the frame
//! on the propagating arm (`m`), and is released on the ok path (`d`). The
//! constructor's payload store is recorded like any store into a holder. No
//! new event letter.

const PROGRAM: &str = r#"effect fn fail(msg: String) -> Int = err(msg)!

effect fn boom(x: Int) -> Int = err("kaboom")!

effect fn chk(s: String) -> Int = if s == "x" then err("bad " + s)!
else int.parse(s)!

effect fn visit(x: Int) -> Unit = if x < 0 then err("negative")!
else ()


effect fn main() -> Unit = {
  println("${chk("4")!}")
  visit(1)!
  println("${fail("f") ?? 0} ${boom(0) ?? 0}")
}
"#;

fn witnesses() -> std::collections::BTreeMap<String, String> {
    let ir = almide_spine::s5::lower_to_ir("unwrap_ctor.almd", PROGRAM).expect("front");
    almide_wasm::witness::start_collecting();
    let _ = almide_wasm::emit_program(&ir).expect("the structural leg lowers the probe");
    almide_wasm::witness::take().into_iter().collect()
}

fn accepted(cert: &str) -> bool {
    almide_verify::check(almide_verify::Property::Ownership, cert.as_bytes())
}

#[test]
fn a_constructor_carrier_under_bang_witnesses_as_an_owned_temporary() {
    // ONE test: the witness sink is process-global.
    let w = witnesses();
    for name in ["fail", "boom", "chk", "visit"] {
        let cert = w.get(name).unwrap_or_else(|| panic!("{name} is witnessed: {w:?}"));
        assert!(!cert.starts_with('!'), "{name} no longer declines tail:Unwrap: {cert:?}");
        assert!(accepted(cert), "{name}: the portable checker must accept {cert:?}");
    }
    // The literal payload moves into the err block (`im`); the carrier is
    // released on the ok path and leaves on the raising one (`{id|im}`); the
    // frame's own result wrap is the last line.
    assert_eq!(w["boom"], "im\n{id|im}\n{|im}\n");
    // A borrowed String payload is shared into the block (`am`).
    assert_eq!(w["fail"], "am\n{id|im}\n{|im}\n");
    // In an arm: the raise ends that path (`x`), the other arm carries none.
    assert_eq!(w["visit"], "{|im}\n{imx|}{|id}\n{|im}\n");
    assert_eq!(w["chk"], "{ibbamd|ibbd}\n{|im}\n{imx|}{|id}\n{imx|}{|id}\n{|im}\n");

    // The carrier's line is checked like any block's: a raise whose ok path
    // never releases the carrier, or releases it twice, is refused.
    for (bad, what) in [("im\n{i|im}\n{|im}\n", "leaked carrier"), ("im\n{idd|im}\n{|im}\n", "double-released carrier")] {
        assert!(!accepted(bad), "{what}: the checker must refuse {bad:?}");
    }
}
