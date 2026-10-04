//! #2758 (#1696 step 4) — an EXIT from inside a loop body (`return`, a
//! guard's early return, a `!` propagation, a fuel cut) in the structural
//! witness. The exit ends the activation with no hand-on, and its releases
//! are accounted per object by the holder that has it (witness_paths.rs
//! `loop_exits`): an object born in the iteration on its iteration line, an
//! object the loop was entered with as one more path of the frame line (a
//! `{<e>x|}` item when exits fold). No new event letter.

use std::collections::BTreeMap;

const FIXTURES: [&str; 7] = [
    "guard_value_exit_in_loop",
    "effect_unwrap_in_loop",
    "fuel_cut_releases_frame",
    "fuel_dyn_charge",
    "guard_jump_in_value_fn",
    "effect_unit_yields_result",
    "nested_unwrap_propagation",
];

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

/// The 14 frames that declined `loop-exit` before #2758 admitted the shape,
/// with their exact certificates.
const PINNED: [(&str, &str, &str); 14] = [
    ("guard_value_exit_in_loop", "effect_int_for", "\n\n{|im}\nim\n"),
    ("guard_value_exit_in_loop", "effect_int_nested", "\n\n{|im}\nim\n"),
    ("guard_value_exit_in_loop", "effect_int_while", "\n{|im}\nim\n"),
    ("guard_value_exit_in_loop", "effect_str_for", "\n{ibamd|ibd}\n\n\n{|im}\n\n{|im}\nim\n"),
    ("guard_value_exit_in_loop", "effect_unit_for", "\n\n{|im}\nim\n"),
    ("guard_value_exit_in_loop", "effect_unit_while", "\n{|im}\nim\n"),
    ("guard_value_exit_in_loop", "pure_unit_for", "\n"),
    ("effect_unwrap_in_loop", "sum_parse", "\n\nam\n\n{ibadm|ibd}\nim\n"),
    ("effect_unwrap_in_loop", "sum_positive", "\n\n{|am}\n\nibd\n\n{ibadmx|}{|ibd}\nim\n"),
    ("fuel_cut_releases_frame", "build", "i{dx|}{dx|}m{x|}\n{x|}\n\n{|im}\n\n{ibd|im}\n{|ibd}\n"),
    ("fuel_dyn_charge", "build", "i{dx|}{dx|}m{x|}\n{x|}\n\n{|im}\n\n{ibd|im}\n{|ibd}\n"),
    ("guard_jump_in_value_fn", "effect_int_for", "\n\n{|im}\n\nib{admx|}d\nim\n"),
    ("effect_unit_yields_result", "walk", "\n\n{ibadm|ibd}\nim\n"),
    ("nested_unwrap_propagation", "sum_all", "\n\nam\n\n{ibadm|ibd}\nim\n"),
];

#[test]
fn an_exit_inside_a_loop_body_witnesses_on_the_line_of_each_holder() {
    // ONE test: the witness sink is process-global.
    let mut by_fixture: BTreeMap<&str, BTreeMap<String, String>> = BTreeMap::new();
    for f in FIXTURES {
        by_fixture.insert(f, witnesses(f));
    }
    for (fixture, name, want) in PINNED {
        let w = &by_fixture[fixture];
        let cert = w.get(name).unwrap_or_else(|| panic!("{fixture}/{name} is witnessed: {w:?}"));
        assert!(!cert.starts_with('!'), "{fixture}/{name} no longer declines loop-exit: {cert:?}");
        assert!(accepted(cert), "{fixture}/{name}: the portable checker must accept {cert:?}");
        assert_eq!(cert, want, "{fixture}/{name}: the pinned certificate");
    }

    // effect_str_for (guard_value_exit_in_loop): `acc`, bound before the
    // loop, is held by the FRAME. The guard's early return inside the body
    // reads it and releases it; that exiting iteration's account of the
    // block is one more frame path (`ibd`), next to the path that leaves the
    // loop and returns it (`ibamd`).
    //
    // effect_int_for (guard_jump_in_value_fn): the `int.parse` carrier is
    // born in the iteration and stays on the iteration line; the `!` exit
    // folds there as an item (`ib{admx|}d`): the err payload shared out, the
    // carrier released, the err moving out — with no hand-on.
    //
    // build (fuel_cut_releases_frame): `s` is loop-carried and the fuel cut
    // exits inside the body. The frame's block moved into the loop (`m`), so
    // the lifted exit holds nothing of it (`{x|}`); the block the iteration
    // received is released by the exit itself (`ibd`, against `im` on the
    // iteration that hands it on).
    //
    // Each is checked like any exit: a leak or a double release on the loop
    // exit path is refused, on the frame line and on the iteration line.
    for (bad, what) in [
        ("\n{ibamd|ib}\n\n\n{|im}\n\n{|im}\nim\n", "frame-held block leaked on the loop exit"),
        ("\n{ibamd|ibdd}\n\n\n{|im}\n\n{|im}\nim\n", "frame-held block released twice on the loop exit"),
        ("\n\n{|im}\n\nib{amx|}d\nim\n", "iteration-born block leaked on the loop exit"),
        ("\n\n{|im}\n\nib{addmx|}d\nim\n", "iteration-born block released twice on the loop exit"),
        ("i{dx|}{dx|}m{dx|}\n{x|}\n\n{|im}\n\n{ibd|im}\n{|ibd}\n", "moved-on block released by the loop exit"),
        ("i{dx|}{dx|}m{x|}\n{x|}\n\n{|im}\n\n{ib|im}\n{|ibd}\n", "received block leaked on the loop exit"),
        ("i{dx|}{dx|}m{x|}\n{x|}\n\n{|im}\n\n{ibdd|im}\n{|ibd}\n", "received block released twice on the loop exit"),
    ] {
        assert!(!accepted(bad), "{what}: the checker must refuse {bad:?}");
    }
}
