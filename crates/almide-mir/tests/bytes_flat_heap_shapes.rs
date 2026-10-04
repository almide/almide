//! #2739: the MIR rung lowers the `Bytes` shapes it walled while the
//! structural leg ran them — a `Bytes` list element (literal and concat onto a
//! module `var`), a `Bytes` variant payload, a `(String, Bytes)` pair, a field
//! read off a record literal — and a map key write to a module `var`. Each
//! must lower, and the ownership verifier must accept it.

#[test]
fn bytes_flat_heap_shapes_lower_and_verify() {
    let src = include_str!("fixtures/bytes_flat_heap_shapes.almd");
    let fns = almide_mir::pipeline::lowered_functions(src).expect("the fixture must lower");
    for name in [
        "push_global",
        "bytes_list_literal",
        "bytes_payload",
        "bytes_pair_map",
        "record_literal_field",
        "global_map_key_write",
    ] {
        let f = fns.iter().find(|f| f.name == name).unwrap_or_else(|| panic!("`{name}` must lower"));
        assert_eq!(almide_mir::verify_ownership(f), Ok(()), "`{name}`");
    }
}
