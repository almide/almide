//! #3060 matrix gate: a literal in a tail-like position of a fn with a SIZED
//! return takes the declared width, on every leg. The evidence is one fixture,
//! `spec/wasm_cross/sized_tail_literal_matrix.almd`, with one fn per cell named
//! `<position>_<width>`; the cross-target corpus gates run it on native, wasm
//! and the interpreter. This gate keeps the matrix whole:
//!
//! - the WIDTH axis is derived from the resolver's builtin type table
//!   (`BUILTIN_TYPE_HEADS`), so a new sized width without its cells fails here.
//!   `Int64` / `Float64` are excluded on purpose: they are the default `Int` /
//!   `Float` width, so a literal needs no narrowing to fit them.
//! - the POSITION axis is the list below; a position added to it without its
//!   cells fails, and so does a cell deleted from the fixture.

use almide::canonicalize::resolve::{BuiltinArity, BuiltinTypeHead, BUILTIN_TYPE_HEADS};
use almide::types::Ty;

/// Every tail-like position a declared return reaches, as its fn-name prefix.
const POSITIONS: &[&str] = &[
    "whole",        // the whole body is the literal
    "if_arm",       // `if` arms in tail position
    "match_arm",    // `match` arms in tail position
    "block_tail",   // a block's tail expression
    "let_annot",    // a `let` with a sized annotation
    "some_payload", // `some(..)` into `-> Option[T]` (and `none` beside it)
    "ok_payload",   // `ok(..)` into `-> Result[T, E]`
    "err_payload",  // `err(..)` into `-> Result[E, T]`
    "effect_tail",  // an effect fn's raw tail
    "effect_ok",    // an effect fn's lifted `ok(..)` tail
    "lambda_let",   // a lambda body under a sized fn-type annotation
    "lambda_arg",   // a lambda body passed to a sized fn-type parameter
];

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
fn every_numeric_builtin_is_classified() {
    for h in bare_heads() {
        let ty = (h.build)(&[]);
        let default_width = matches!(ty, Ty::Int | Ty::Float | Ty::Int64 | Ty::Float64);
        let numeric_name = h.name.starts_with("Int") || h.name.starts_with("UInt") || h.name.starts_with("Float");
        if numeric_name && !default_width {
            assert!(
                sized_widths().contains(&h.name),
                "builtin numeric type `{}` is neither a default width nor in the sized matrix",
                h.name
            );
        }
    }
    assert_eq!(sized_widths().len(), 8, "sized widths: {:?}", sized_widths());
}

#[test]
fn every_width_has_every_tail_position_cell() {
    let path = concat!(env!("CARGO_MANIFEST_DIR"), "/spec/wasm_cross/sized_tail_literal_matrix.almd");
    let src = std::fs::read_to_string(path).expect("read the matrix fixture");
    let mut missing = Vec::new();
    for w in sized_widths() {
        let w = w.to_lowercase();
        for p in POSITIONS {
            let cell = format!("fn {p}_{w}(");
            if !src.contains(&cell) {
                missing.push(cell);
            }
            // The cell is only evidence when main prints it.
            let used = format!("{p}_{w}(n)");
            let used_whole = format!("{p}_{w}()");
            if !src.contains(&used) && !src.contains(&used_whole) {
                missing.push(format!("{p}_{w} is never called"));
            }
        }
        let shown = format!("show_{w}(1)!");
        if !src.contains(&shown) {
            missing.push(format!("main does not run show_{w}"));
        }
    }
    assert!(missing.is_empty(), "sized tail-literal matrix has holes:\n  {}", missing.join("\n  "));
}

/// #3161: every place a declaration gives a literal its width, as the cell
/// prefix in `spec/wasm_cross/sized_field_default_matrix.almd`.
const DECL_POSITIONS: &[&str] = &[
    "rec", // a record field default, filled when a literal omits the field
    "low", // the same at the low end (negative, or zero for an unsigned width)
    "var", // a record-variant payload field default
    "top", // a top-level `let` constant
];

#[test]
fn every_width_has_every_declared_default_cell() {
    let path = concat!(env!("CARGO_MANIFEST_DIR"), "/spec/wasm_cross/sized_field_default_matrix.almd");
    let src = std::fs::read_to_string(path).expect("read the field-default matrix fixture");
    let mut missing = Vec::new();
    for w in sized_widths() {
        for p in DECL_POSITIONS {
            let cell = match *p {
                "top" => format!("{}_{}", p.to_uppercase(), w.to_uppercase()),
                _ => format!("{p}_{}", w.to_lowercase()),
            };
            if !src.contains(&format!("{cell}: {w} = ")) {
                missing.push(format!("{cell}: {w} = .."));
            }
            // The cell is only evidence when main prints it.
            if !src.contains(&format!("{cell}}}")) {
                missing.push(format!("{cell} is never printed"));
            }
        }
    }
    assert!(missing.is_empty(), "sized declared-default matrix has holes:\n  {}", missing.join("\n  "));
}
