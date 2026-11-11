//! `native_slot_order`: which declarations move, and which stay.

use super::*;

fn func(origins: &[(u32, u32)]) -> MirFunction {
    MirFunction { slot_origins: origins.iter().map(|&(n, o)| (ValueId(n), ValueId(o))).collect(), ..Default::default() }
}

#[test]
fn a_reassigned_local_is_declared_at_its_vars_place() {
    let mut out = "    let mut v2: i64 = 2i64;\n    let mut v3: f64 = 1.5;\n    let mut v8: i64 = v2.wrapping_add(v7);\n    for v10 in 0i64..v9 {\n        v8 = v16;\n    }\n".to_string();
    declare_in_source_order(&func(&[(8, 2)]), &mut out);
    assert_eq!(out, "    let mut v2: i64 = 2i64;\n    let mut v8: i64;\n    let mut v3: f64 = 1.5;\n    v8 = v2.wrapping_add(v7);\n    for v10 in 0i64..v9 {\n        v8 = v16;\n    }\n");
}

#[test]
fn two_reassigned_locals_keep_their_vars_order() {
    // `a` (declared before v1) and `c` (declared as v2), reassigned as v6 and v8.
    let mut out = "    let mut v1: f64 = 1.5;\n    let mut v2: i64 = 2i64;\n    let mut v3: f64 = 3.0;\n    let mut v6: f64 = v3 - v1;\n    let mut v8: i64 = v2.wrapping_add(v2);\n".to_string();
    declare_in_source_order(&func(&[(6, 0), (8, 2)]), &mut out);
    assert_eq!(out, "    let mut v6: f64;\n    let mut v1: f64 = 1.5;\n    let mut v2: i64 = 2i64;\n    let mut v8: i64;\n    let mut v3: f64 = 3.0;\n    v6 = v3 - v1;\n    v8 = v2.wrapping_add(v2);\n");
}

#[test]
fn a_local_outside_the_body_level_or_declared_twice_stays() {
    // Declared inside a nested block: its assignment could not reach a
    // declaration hoisted out of that block's own level.
    let nested = "    let mut v3: f64 = 1.5;\n    if c {\n        let mut v8: i64 = 1i64;\n    }\n";
    let mut out = nested.to_string();
    declare_in_source_order(&func(&[(8, 2)]), &mut out);
    assert_eq!(out, nested);
    // Declared twice: which one the record names is not known.
    let twice = "    let mut v3: f64 = 1.5;\n    let mut v8: i64 = 1i64;\n    {\n        let mut v8: i64 = 2i64;\n    }\n";
    let mut out = twice.to_string();
    declare_in_source_order(&func(&[(8, 2)]), &mut out);
    assert_eq!(out, twice);
    // Not a scalar, or not one line: left alone.
    let heap = "    let mut v3: f64 = 1.5;\n    let mut v8: AlmideString = f();\n";
    let mut out = heap.to_string();
    declare_in_source_order(&func(&[(8, 2)]), &mut out);
    assert_eq!(out, heap);
    // Nothing declared between the `var` and the reassignment: already in order.
    let adjacent = "    let mut v2: i64 = 2i64;\n    let mut v8: i64 = 1i64;\n";
    let mut out = adjacent.to_string();
    declare_in_source_order(&func(&[(8, 9)]), &mut out);
    assert_eq!(out, adjacent);
}
