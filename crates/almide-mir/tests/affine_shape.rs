//! The affine-shape classification (#4236) agrees with the ownership the MIR
//! lowering decided: a body that never aliases a live heap value is
//! affine-shaped, and each `Dup` it does emit is attributed to its source —
//! a borrowed param or a value the body owns.

use almide_mir::affine_shape::AffineShape;

fn shape_of(src: &str, name: &str) -> AffineShape {
    let (shapes, walled) = almide_mir::pipeline::program_affine_shapes(src, &[]).expect("the program lowers");
    assert!(!walled.iter().any(|(n, _)| n == name), "`{name}` walled: {walled:?}");
    shapes
        .into_iter()
        .find(|(n, _)| n == name)
        .map(|(_, s)| s)
        .unwrap_or_else(|| panic!("`{name}` was not lowered"))
}

#[test]
fn a_scalar_body_is_affine_shaped() {
    let src = "fn add(a: Int, b: Int) -> Int = a + b\n";
    assert!(shape_of(src, "add").is_affine());
}

#[test]
fn a_fresh_heap_value_used_once_is_affine_shaped() {
    let src = "fn make() -> List[Int] = [1, 2, 3]\n";
    assert!(shape_of(src, "make").is_affine());
}

#[test]
fn returning_a_borrowed_param_is_a_param_dup() {
    let src = "fn same(xs: List[Int]) -> List[Int] = xs\n";
    let s = shape_of(src, "same");
    assert!(!s.is_affine());
    assert_eq!(s.param_dups.len(), 1, "{s:?}");
    assert!(s.local_dups.is_empty(), "{s:?}");
}

#[test]
fn a_local_heap_value_stored_twice_is_a_local_dup() {
    let src = "fn twice() -> List[String] = {\n  let a = \"x\" + int.to_string(1)\n  [a, a]\n}\n";
    let s = shape_of(src, "twice");
    assert!(!s.is_affine());
    assert!(!s.local_dups.is_empty(), "{s:?}");
    assert!(s.param_dups.is_empty(), "{s:?}");
}
