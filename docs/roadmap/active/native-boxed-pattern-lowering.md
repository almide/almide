<!-- description: Fix native (Rust) codegen for nested ctor/literal patterns on a Box'd recursive-variant field (#610) -->
# native: nested ctor/literal pattern at a Box'd (recursive-variant) field

Status: active — analyzed, ready to implement (#610)
Owner: compiler / codegen
Scope: NATIVE (Rust) target only. wasm is already correct (control.rs #607
restructures nested boxed patterns into a tag-guard + nested match).

## Symptom

```almide
type Tree = | Leaf(Int) | Node(Tree, Tree)
fn sum(t: Tree) -> Int = match t {
  Node(Leaf(a), Leaf(b)) => a + b,   // nested ctor at a Box'd field
  Node(l, r)             => sum(l) + sum(r),
  Leaf(n)                => n,
}
```

- wasm: correct (`6`).
- native: emitted Rust does NOT compile — two coupled defects, both in the line
  `Tree::Node(Tree::Leaf(a), Tree::Leaf(b)) => ((*a) + (*b))`:

  - **(a) E0614 spurious deref** — `(*a)`/`(*b)` though `a:Int` is not boxed.
    `pass_box_deref.rs::collect_bind_vars` (≈ line 450) recurses into a nested
    `Constructor`'s args UNCONDITIONALLY and marks the inner binds (`a`,`b`) as
    deref vars, but only the DIRECTLY-Box'd field position should deref. Fix:
    `collect_bind_vars` must mark ONLY a direct `IrPattern::Bind` — drop the
    `Constructor { args }` recursion (a nested ctor at a boxed field is defect
    (b)'s job, and its inner binds are post-deref values, not the boxed pointer).
  - **(b) E0308 box-not-matched** — `Tree::Leaf(a)` is matched inline against
    `Box<Tree>` (Node's field). Stable Rust has no `box` patterns (the generated
    Rust is stable, no `#![feature(box_patterns)]`), so a nested ctor/literal at
    a boxed field cannot be matched inline.

(a) and (b) MUST land together: fixing (a) alone removes the masking deref and
leaves E0308; fixing (b) alone leaves the E0614.

## Fix for (b): mirror `pass_pattern_literal_guard.rs` for ctors

`pass_pattern_literal_guard.rs` already solves the analogous problem for nested
STRING literals (Rust-only pass): it replaces a payload-nested `Literal{LitStr}`
with a fresh `Bind __lit_N` and ANDs `__lit_N == "..."` onto the arm guard, so a
guard-false falls through to later arms exactly like the wasm tag-then-value
check. `IrMatchArm` has a `guard: Option<IrExpr>` field the walker renders as
`pattern if guard`.

For #610, extend (or add a sibling to) that pass: for an `IrPattern::Constructor`
(or a non-Int/Bool/Float `Literal`) sitting at a field whose type
`ty_contains_any_recursive` (the same predicate as the Box-render and defect (a)
sites — reuse `walker::ty_contains_any_recursive` + the cycle set), rewrite it:

1. Replace the nested pattern `Leaf(a)` with a fresh `Bind __box_N`.
2. AND a tag guard onto the arm: `match *__box_N { Leaf(_) => true, _ => false }`
   (an `IrExpr` Match, Bool-typed) — a guard-false falls through to later arms,
   preserving reachability byte-identically to wasm.
3. Re-bind the inner vars in the arm BODY before the original body runs, e.g.
   prepend `let Leaf(a) = *__box_N else { unreachable!() }` (the guard already
   proved the shape) — or an equivalent inner `match *__box_N { Leaf(a) => …}`.
   The inner binds are the DEREF'd values (so defect (a)'s no-recurse change is
   what keeps them non-deref'd at use sites).

Pin in `tests/crossmod_matrix_test.rs` (or a `spec/wasm_cross/` fixture) with the
`Tree`/`sum` shape above (native AND wasm must both print `6`); the gate already
cross-checks wasm byte-for-byte.

## Why deferred from the v0.27.6 batch

native-only (the cross-target byte gate is unaffected — wasm is correct), and
defect (b) is a new match-restructuring pass (tag-guard IrExpr generation +
inner-bind extraction) that warrants focused implementation + a dedicated test
run rather than bundling into a multi-fix release. The trigger is narrow (a
nested ctor/literal at a boxed field, a shape that currently fails to compile),
so regression risk on currently-passing programs is low.
