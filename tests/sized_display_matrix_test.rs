//! #3187 matrix gate: every sized width prints the same text through every
//! display path, on every leg. The evidence is one fixture,
//! `spec/wasm_cross/sized_display_matrix.almd`, with one fn per cell named
//! `<path>_<width>`; the cross-target corpus gates run it on native, wasm and
//! the interpreter, and each integer row asserts its `${x}` against the
//! width's own `to_string`. This gate keeps the matrix whole:
//!
//! - the WIDTH axis is derived from the resolver's builtin type table
//!   (`BUILTIN_TYPE_HEADS`), so a new sized width without its cells fails here.
//!   `Int64` / `Float64` are excluded on purpose: they are the default `Int` /
//!   `Float`, whose display every other fixture already exercises.
//! - the PATH axis is the list below; a path added to it without its cells
//!   fails, and so does a cell deleted from the fixture or never run.
//!
//! What is NOT a column, and why: `println(x)` of a sized value is rejected by
//! the checker (E001: `println` takes a `String`), so it has no display to
//! diverge. A failing `assert_eq` renders its operands through the same
//! interpolation as `${x}` (the frontend's `desugar_assert_abort`); it aborts
//! at the first failure, so it is pinned once, for UInt64, by
//! `spec/wasm_cross/uint64_assert_display.almd` (checked below).

use almide::canonicalize::resolve::{BuiltinArity, BuiltinTypeHead, BUILTIN_TYPE_HEADS};
use almide::types::Ty;

/// Every display path a sized value reaches, as its cell fn-name prefix.
const PATHS: &[&str] = &[
    "interp",            // `"${x}"`, the sole part of a value-position build
    "interp_mid",        // `"<${x}|${x}>"`, parts between literals
    "concat",            // `"s=" + <width>.to_string(x)`
    "list",              // `${[x, x]}`
    "record",            // a declared record's field
    "anon_record",       // an anonymous record's field
    "generic_record",    // a generic record instance's field (`Box[T]`)
    "tuple",             // a tuple element
    "option",            // a `some(..)` payload
    "result_ok",         // an `ok(..)` payload
    "result_err",        // an `err(..)` payload
    "map_key",           // a map key
    "map_value",         // a map value
    "set",               // a set element
    "variant_tuple",     // a tuple-shaped variant case's payload
    "variant_record",    // a record-shaped variant case's field
    "generic_variant",   // a generic variant instance's payload (`Tag[T]`)
    "recursive_generic", // a RECURSIVE generic instance (`Tree[T]`, the helper path)
    "nested",            // `Option[List[(T, Rec)]]`, several levels down
    "err_propagate",     // the repr `!` carries into an effect fn's String channel
];

const FIXTURE: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/spec/wasm_cross/sized_display_matrix.almd");

/// The bare builtin heads (an applied head's builder needs its arguments).
fn bare_heads() -> impl Iterator<Item = &'static BuiltinTypeHead> {
    BUILTIN_TYPE_HEADS.iter().filter(|h| matches!(h.arity, BuiltinArity::Bare))
}

/// The sized widths: builtin numeric heads other than the default widths.
fn sized_widths() -> Vec<&'static str> {
    bare_heads()
        .filter(|h| {
            matches!(
                (h.build)(&[]),
                Ty::Int8 | Ty::Int16 | Ty::Int32 | Ty::UInt8 | Ty::UInt16 | Ty::UInt32 | Ty::UInt64 | Ty::Float32
            )
        })
        .map(|h| h.name)
        .collect()
}

/// Every numeric builtin head is classified: sized (in the matrix) or a
/// default-width alias. A new numeric `Ty` has to pick a side here.
#[test]
fn every_numeric_builtin_is_in_the_display_matrix_or_a_default_width() {
    for h in bare_heads() {
        let ty = (h.build)(&[]);
        let default_width = matches!(ty, Ty::Int | Ty::Float | Ty::Int64 | Ty::Float64);
        let numeric_name = h.name.starts_with("Int") || h.name.starts_with("UInt") || h.name.starts_with("Float");
        if numeric_name && !default_width {
            assert!(
                sized_widths().contains(&h.name),
                "builtin numeric type `{}` is neither a default width nor in the display matrix",
                h.name
            );
        }
    }
    assert_eq!(sized_widths().len(), 8, "sized widths: {:?}", sized_widths());
}

#[test]
fn every_width_has_every_display_path_cell() {
    let src = std::fs::read_to_string(FIXTURE).expect("read the display matrix fixture");
    let mut missing = Vec::new();
    for w in sized_widths() {
        let lw = w.to_lowercase();
        for p in PATHS {
            // The cell takes a value of THIS width.
            let cell = format!("fn {p}_{lw}(x: {w})");
            if !src.contains(&cell) {
                missing.push(cell);
            }
            // The cell is only evidence when its row runs it.
            if !src.contains(&format!("{p}_{lw}(x)")) {
                missing.push(format!("{p}_{lw} is never called"));
            }
        }
        // The outermost build in `main` (the room-free appends).
        if !src.contains(&format!("\"main_line_{lw}: ${{hi_{lw}}}\"")) {
            missing.push(format!("main_line_{lw}"));
        }
        if !src.contains(&format!("row_{lw}(")) {
            missing.push(format!("main does not run row_{lw}"));
        }
    }
    assert!(missing.is_empty(), "sized display matrix has holes:\n  {}", missing.join("\n  "));
}

/// The fixture defines no cell this gate does not know: an unlisted path
/// would be evidence nobody keeps whole.
#[test]
fn every_fixture_cell_is_a_listed_path() {
    let src = std::fs::read_to_string(FIXTURE).expect("read the display matrix fixture");
    let widths: Vec<String> = sized_widths().iter().map(|w| w.to_lowercase()).collect();
    let helpers = ["row", "fails", "lift"];
    let mut unlisted = Vec::new();
    for line in src.lines() {
        let Some(rest) = line.strip_prefix("fn ").or_else(|| line.strip_prefix("effect fn ")) else { continue };
        let Some(name) = rest.split('(').next() else { continue };
        let Some(w) = widths.iter().find(|w| name.ends_with(&format!("_{w}"))) else { continue };
        let path = &name[..name.len() - w.len() - 1];
        if !PATHS.contains(&path) && !helpers.contains(&path) {
            unlisted.push(name.to_string());
        }
    }
    assert!(unlisted.is_empty(), "cells outside PATHS: {unlisted:?}");
}

/// Each integer row checks its top-level leaf against the width's own
/// printer, so a digit string every leg gets wrong the same way still fails.
#[test]
fn every_integer_row_checks_its_leaf_against_to_string() {
    let src = std::fs::read_to_string(FIXTURE).expect("read the display matrix fixture");
    let mut missing = Vec::new();
    for w in sized_widths().iter().filter(|w| **w != "Float32") {
        let lw = w.to_lowercase();
        let check = format!("assert_eq(interp_{lw}(x), {lw}.to_string(x))");
        if !src.contains(&check) {
            missing.push(check);
        }
    }
    assert!(missing.is_empty(), "rows without an absolute check:\n  {}", missing.join("\n  "));
}

/// The assert-message column, pinned once for the width whose slot reading
/// differs from the carrier's.
#[test]
fn the_assert_message_fixture_names_the_upper_half() {
    let path = concat!(env!("CARGO_MANIFEST_DIR"), "/spec/wasm_cross/uint64_assert_display.almd");
    let src = std::fs::read_to_string(path).expect("read the assert display fixture");
    assert!(src.starts_with("// @contract: C-179"), "the fixture must declare C-179");
    assert!(src.contains("18446744073709551615") && src.contains("assert_eq("), "the fixture must fail an assert_eq on u64::MAX");
}
