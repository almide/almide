//! #2758 (#1696 step 4) — the `m[k] = v` statement (`IrStmtKind::MapInsert`)
//! in the structural witness. Its in-place window (map_inplace.rs) is the
//! one `map.insert(m, k, v)` takes: the key and the value go through
//! `lower_arg` under `Retain` (a borrowed handle shares, an owned one moves
//! into the map), and the var's block is rebound (`witness_mut_rebind`: the
//! old credit released, the block now in the slot held). The functional
//! fallback, which writes the var back without that rebind, declines at
//! emission (`map-insert:functional`). No new event letter.

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

/// The frames that declined `stmt:MapInsert` before the statement was
/// admitted. One `main` now stops at a later bucket (`forin-map`).
const FRAMES: [(&str, &str); 10] = [
    ("declaration_literal_domain", "main"),
    ("map_entries_reclaim", "counter"),
    ("map_index_order", "build"),
    ("map_index_order", "main"),
    ("map_set_in_place", "main"),
    ("map_set_index_threshold", "main"),
    ("nested_place_assign", "main"),
    ("place_assign_ascription", "main"),
    ("place_mutation", "main"),
    ("stdlib_call_temporaries_release", "main"),
];

#[test]
fn a_map_index_write_witnesses_through_the_in_place_window() {
    // ONE test: the witness sink is process-global.
    let mut by_fixture: BTreeMap<&str, BTreeMap<String, String>> = BTreeMap::new();
    for (f, _) in FRAMES {
        by_fixture.entry(f).or_insert_with(|| witnesses(f));
    }
    for (f, name) in FRAMES {
        let cert = &by_fixture[f][name];
        assert!(!cert.contains("MapInsert"), "{f}/{name} no longer declines stmt:MapInsert: {cert:?}");
        if cert.starts_with('!') {
            assert_eq!(cert, "!decline:forin-map\n", "{f}/{name}: only the later bucket");
        } else {
            assert!(accepted(cert), "{f}/{name}: the portable checker must accept {cert:?}");
        }
    }
    // `var m = map.new(); for i in 0..<n { m[i * stride] = i }; m`: the map
    // is loop-carried; each iteration receives the block, reads it (`b`),
    // and the window's rebind releases it (`ibd`) and holds the block now
    // in the slot, which the iteration hands on.
    let build = &by_fixture["map_index_order"]["build"];
    assert_eq!(build, "im\n\nim\n\nibd\nibamd\n");
    let counter = &by_fixture["map_entries_reclaim"]["counter"];
    assert!(counter.starts_with("ibd\nibd\nim\nibabdbd\n"), "{counter:?}");
    // A rebind that forgets the old block's release, or releases it twice,
    // is refused.
    for (bad, what) in [("im\n\nim\n\nib\nibamd\n", "old block kept"), ("im\n\nim\n\nibdd\nibamd\n", "old block released twice")] {
        assert!(!accepted(bad), "{what}: the checker must refuse {bad:?}");
    }
}
