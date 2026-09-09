# Native read-only ownership review

## String comparisons (#2066)

The old compiler emits `almide_eq!(t.kind.clone(), want)` for a borrowed
record and allocates a String for a literal comparison operand. The regression
test fails on that emission. CloneInsertion now wraps stable String operands
of equality and ordering comparisons in the existing string-borrow IR node.
The normal liveness traversal still counts both reads. The existing Borrow
renderer emits a literal as str and borrows fields, so no walker target branch
or public language change is needed.

Calls, indexing and blocks retain their value evaluation path. In particular,
`t.kind == change(t)` must compare the original field value when the RHS
mutates `t`; borrowing the field across that call would change its lifetime.
The regression exercises this case and reuse after comparison on native and
structural WASM, alongside field/parameter/literal equality and ordering.

## List fields in loops (#2068)

The existing loop proof elided a collection clone only for a bare Var. A
Member chain now identifies the same root binding. If the body does not write
that root, the loop borrows the list field. The existing borrowed-element proof
then avoids element copies when every use only reads the element. If the body
replaces the parent, writes its fields, or mutably borrows that place, the
collection snapshot remains. The tests exercise both branches and later use
of the parent on native and structural WASM. When a field's root is owned,
this is its final use outside an enclosing loop, and the body needs owned
elements, a consumed-loop annotation selects `into_iter()` instead. The
ownership decision remains in the pass, and the walker only reads that
annotation. Consuming elements from a live borrowed collection still requires
an owned copy. The regression calls a helper borrowing the parent before its
final consuming loop and verifies both native and structural outputs.

## Reference milestone

The reference compiler is cloned at `../almide-references/rust`, commit
`0d31508599a7814a7044e9a7a871e3dc5f037753`. In
`compiler/rustc_mir_build/src/builder/expr/as_rvalue.rs`, the closure-capture
lowering uses place construction instead of a temporary only for side-effect
free, disjoint places. That distinction informs this change: stable place
reads can borrow, while evaluation that can invalidate a place keeps its
snapshot. The loop retains a copy when its owner is mutated. This is a design
comparison, not a claim that Rust's implementation proves Almide's analysis.

Validation is recorded by the PR and executable regressions:
`tests/native_string_comparison_borrow_test.rs` and
`tests/for_in_clone_elision_test.rs`.

## Validation of the integrated revision

All 439 Almide test files pass on native and default (427 WASM, 12 native
fallbacks). The 43 nanopass tests, 13 snapshots, 11 loop ownership tests, and
one string-comparison matrix pass. The original comparison fixture and the
read-only field-loop fixture failed before the fixes. The borrowed-parent
negative case caught an unsafe provisional move rule before it was corrected.
Codopsy 2.2.0 reports 90/A for codegen overall with unchanged thresholds;
the extracted loop renderer and projection helpers keep files below 800 lines.
