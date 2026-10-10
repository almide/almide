//! #2758 (#1696 step 4) — `f(x)!` over a MOVE-MODE effect call (C-132): the
//! call is typed with its raw payload, but its wasm value is the effect
//! ABI's Result block, handed over at rc 1. The `!` site takes the
//! owned-carrier route (witness_unwrap.rs): the carrier is born at the site
//! (`i`), leaves on the propagating arm (`m`) and is released on the ok
//! path (`d`, `release_ok_carrier`). No new event letter.

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

/// The frames that declined `rhs:Unwrap` before the move-mode carrier was
/// admitted, with the bucket each now stops at (`None`: certified).
const FRAMES: [(&str, &str, Option<&str>); 10] = [
    ("destructure_owned_tuple_release", "main", None),
    ("effect_call_arg_credit", "main", None),
    ("effect_mut_generic_port", "main", None),
    ("effect_mut_generic_port", "Mem.put", None),
    ("mut_param_call_chain", "g", None),
    ("mut_param_call_chain", "main", None),
    ("mut_param_closure_capture_write", "main", None),
    ("mut_param_effect_never_err", "main", None),
    // #2755: its later bucket, a Var read as a statement, is admitted too.
    ("mut_param_effect_unpropagated", "main", None),
    ("mut_param_tail_recursion", "main", None),
];

#[test]
fn a_bang_over_a_move_mode_effect_call_witnesses_its_abi_carrier_as_owned() {
    // ONE test: the witness sink is process-global.
    let mut by_fixture: BTreeMap<&str, BTreeMap<String, String>> = BTreeMap::new();
    for (f, _, _) in FRAMES {
        by_fixture.entry(f).or_insert_with(|| witnesses(f));
    }
    for (f, name, later) in FRAMES {
        let w = &by_fixture[f];
        let cert = w.get(name).unwrap_or_else(|| panic!("{f}/{name} is witnessed: {w:?}"));
        match later {
            Some(r) => assert_eq!(cert, &format!("!decline:{r}\n"), "{f}/{name}: only the later bucket"),
            None => assert!(accepted(cert), "{f}/{name}: the portable checker must accept {cert:?}"),
        }
    }
    // `effect fn g(mut a: Int) = { h(a)! }`: h's carrier is born at the site,
    // released on the ok path and moving out on the propagating one
    // (`{id|im}`); g's own Unit result wrap is the last line.
    assert_eq!(by_fixture["mut_param_call_chain"]["g"], "{id|im}\n{|im}\n");
    // `Mem.put` (effect_mut_generic_port) and Tally's `main`
    // (mut_param_effect_never_err) carry the buffer the call hands back.
    // `self.ks = self.ks + [k]` is the field accumulator window (#3501): the
    // element shares straight into the pushed slot, so the one-element list
    // literal and the concat result (`ibd`, `im`) are never born; the old
    // record's line (`ibd`) is the unique judge's read and rebind, and the
    // rebound record's line gains one read (`b`): the append reads the place
    // through it.
    assert_eq!(by_fixture["effect_mut_generic_port"]["Mem.put"], "ibd\nam\nibbamd\nim\n");
    // Tally's `t` and `u` are rebound by the writeback before any read, so
    // the mut-param move-in (writeback_move.rs) hands their block to the
    // callee and stores NULL in the var: `i b m` — no share (`a`) at the
    // call, and the writeback's release of the old block is a release of
    // NULL, so no `d` on that line.
    assert_eq!(by_fixture["mut_param_effect_never_err"]["main"], "ibm\nid\n{|ibd}\n{|ad}\n{|ibm}\n{|ibd}\n{|ad}\n");
    // A carrier the ok path never releases, or releases twice, is refused.
    for (bad, what) in [("{i|im}\n{|im}\n", "leaked carrier"), ("{idd|im}\n{|im}\n", "double-released carrier")] {
        assert!(!accepted(bad), "{what}: the checker must refuse {bad:?}");
    }
}
