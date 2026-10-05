//! #2758 (#1696 step 4) — a NULLARY variant case stored into a block, when
//! the ownership predicate does not class it as owned (a generic instance's
//! case, resolved only by the slot's type). The case is a pool static
//! (`lower_variant_ctor`, #1961); the store's share guard takes a real
//! `rc_inc_top` on it, a no-op below the heap floor, and the container's
//! credit on it is never released for real. It is recorded as a view's
//! share and move (`am`). No new event letter.
//!
//! Since #3394 a bare unit case of a generic variant carries its instance's
//! type arguments, so `Leaf` in `sized_display_matrix` is typed `Tree[Int8]`
//! and the predicate classes it as owned: it is witnessed as an owned
//! introduction moved into the node (`im`), still accepted by the portable
//! checker. The `am` shape stays pinned below as an accepted line.

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
fn a_stored_nullary_case_of_a_generic_instance_witnesses_as_a_view_share() {
    // ONE test: the witness sink is process-global.
    let w = witnesses("sized_display_matrix");
    // `Node(Node(Leaf, x), x)`: the inner `Leaf` is stored into the inner
    // node (`im`, owned since #3394), the inner node moves into the outer one
    // (`im`), the outer node is displayed and released (`ibd`), the text
    // moves out (`im`).
    let pinned = [
        ("recursive_generic_int8", "im\nim\nibd\nim\n"),
        ("recursive_generic_uint64", "im\nim\nibd\nim\n"),
        // A Float32 payload adds the printed float's own temporary (`id`).
        ("recursive_generic_float32", "im\nim\nibd\nid\nim\n"),
    ];
    for (name, want) in pinned {
        let cert = w.get(name).unwrap_or_else(|| panic!("{name} is witnessed: {w:?}"));
        assert!(!cert.starts_with('!'), "{name} no longer declines store:borrowed-temp: {cert:?}");
        assert!(accepted(cert), "{name}: the portable checker must accept {cert:?}");
        assert_eq!(cert, want, "{name}");
    }
    for name in ["int16", "int32", "uint8", "uint16", "uint32"] {
        let name = format!("recursive_generic_{name}");
        assert_eq!(w[&name], "im\nim\nibd\nim\n", "{name}");
    }
    // The view-share form is still a balanced line; a share that never moves
    // into the container, or one taken twice, is refused like any unbalanced line.
    assert!(accepted("am\nim\nibd\nim\n"), "the view-share line stays accepted");
    for (bad, what) in [("a\nim\nibd\nim\n", "share left over"), ("aam\nim\nibd\nim\n", "double share")] {
        assert!(!accepted(bad), "{what}: the checker must refuse {bad:?}");
    }
}
