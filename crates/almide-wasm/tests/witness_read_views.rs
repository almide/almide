//! #2758 (#1696 step 4) — values READ by an operator or an interpolation
//! (they spend no credit) beyond a Var, a literal and a slot read: `r ?? v`
//! / `r ?? "lit"` over a bound carrier, a join that is never owned (both
//! arms views, no RC site). No new event letter.

use std::collections::BTreeMap;

fn witnesses(fixture: &str) -> BTreeMap<String, String> {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let rel = format!("spec/wasm_cross/{fixture}.almd");
    let text = std::fs::read_to_string(root.join(&rel)).expect("fixture readable");
    let ir = almide_spine::s5::lower_to_ir(&rel, &text).expect("front");
    almide_wasm::witness::start_collecting();
    let _ = almide_wasm::emit_program(&ir).expect("the structural leg lowers the fixture");
    almide_wasm::witness::take().into_iter().collect()
}

fn accepted(cert: &str) -> bool {
    almide_verify::check(almide_verify::Property::Ownership, cert.as_bytes())
}

#[test]
fn a_never_owned_unwrap_or_join_is_read_as_a_view() {
    // ONE test: the witness sink is process-global.
    // `fn os(o: String?) = if option.is_some(o) then "S" + (o ?? "?") else
    // "N"`: the concat reads the join; only the owned param's own reads and
    // release appear (`{ibbd|ibd}`), each arm's String moves out.
    let w = witnesses("compound_eq");
    let cert = &w["os"];
    assert!(accepted(cert), "the portable checker must accept {cert:?}");
    assert_eq!(cert, "{ibbd|ibd}\n{|im}\n{|im}\nim\n");
    let w = witnesses("fs_read_directory_errno");
    for name in ["branch_lift_synth_3", "branch_lift_synth_7", "branch_lift_synth_9"] {
        let cert = w.get(name).unwrap_or_else(|| panic!("{name} is witnessed: {w:?}"));
        assert!(!cert.starts_with('!'), "{name} no longer declines: {cert:?}");
        assert!(accepted(cert), "{name}: the portable checker must accept {cert:?}");
    }
    // A read that released the param it reads, or never released it, is
    // refused.
    for (bad, what) in [("{ibbdd|ibd}\n{|im}\n{|im}\nim\n", "released twice"), ("{ibb|ibd}\n{|im}\n{|im}\nim\n", "never released")] {
        assert!(!accepted(bad), "{what}: the checker must refuse {bad:?}");
    }
}
