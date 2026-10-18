//! A string concat whose operand no call-argument form lowers (a `??` over a
//! `fan.bounded` inside a call) lowers on v1 by binding its operands first, as
//! the same body written with a `let` already did.
//! `spec/wasm_cross/fuel_interp_uncharged.almd :: main` walled on this shape.

fn lowered(src: &str) -> Vec<almide_mir::MirFunction> {
    almide_mir::pipeline::lowered_functions(src).expect("reaches the MIR lowering")
}

const WORK: &str = "fn w(n: Int) -> Int = {\n  var i = 0\n  while i < n {\n    i = i + 1\n  }\n  i\n}\n";

#[test]
fn a_concat_over_a_bounded_fan_fallback_lowers_in_argument_position() {
    let src = format!(
        "{WORK}effect fn main() -> Unit = {{\n  println(\"cut \" + int.to_string(fan.bounded(compute.ns(30)) {{ w(100) }} ?? -1))\n}}\n"
    );
    let funcs = lowered(&src);
    let f = funcs.iter().find(|f| f.name.as_str() == "main").unwrap_or_else(|| panic!("`main` walled on v1"));
    assert!(almide_mir::verify_ownership(f).is_ok(), "`main` fails ownership verification");
    let ops: Vec<String> = f.ops.iter().map(|op| format!("{op:?}")).collect();
    assert!(ops.iter().any(|o| o.contains("__str_concat")), "no concat call:\n{ops:#?}");
}
