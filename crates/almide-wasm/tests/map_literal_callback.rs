//! #1423 stage 5 (the 2026-09-08 campaign's `bind:unmapped` walls): a
//! callback lambda inside a MAP LITERAL's key or value — `["k": (if
//! list.all(xs, (x) => …) then … else …)]` — is inlined by the HOF arm
//! like any other, so its params must be collected as locals. The
//! collection walk had no `MapLiteral` arm; every such program walled the
//! structural leg and then hit the incumbent's unlinked typed twins.

mod harness;
use harness::run_wasm;

const SRC: &str = r#"fn main() -> Unit = {
  let xs: List[Bool] = [true, false]
  let m: Map[String, Int] = ["k0": (if list.all(xs, (x) => x) then 10 else 1), "k1": list.count([1, 2, 3], (n) => n > 1)]
  let keyed: Map[String, Int] = [(if list.any(xs, (x) => x) then "yes" else "no"): 7]
  println("${map.get_or(m, "k0", -1)} ${map.get_or(m, "k1", -1)} ${map.get_or(keyed, "yes", -1)}")
}
"#;

#[test]
fn a_callback_inside_a_map_literal_lowers_on_the_structural_leg() {
    let ir = almide_spine::s5::lower_to_ir("map_literal_callback.almd", SRC).expect("front");
    let bytes = almide_wasm::emit_program(&ir).expect("the structural leg lowers a map literal's callbacks");
    let out = run_wasm(&bytes).expect("run");
    assert_eq!(out.exit, 0, "{}", out.stderr);
    assert_eq!(out.stdout.trim(), "1 2 7");
}
