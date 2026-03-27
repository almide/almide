<!-- description: Fix Rust codegen emitting invalid type for anonymous records -->
<!-- done: 2026-03-18 -->
# Anonymous Record Codegen Fix

**Priority:** High — affects 30% of all tasks in Grammar Lab experiments
**Estimate:** 1-2 days
**Branch:** develop

## Problem

When an empty list literal `[]` gets a type annotation with an anonymous record type, Rust codegen emits the non-existent type name `AnonRecord`.

```almide
let ps: List[Product] = []
// Product = { name: String, price: Int, category: String }
```

Generated Rust:
```rust
let ps: Vec<Product> = Vec::<AnonRecord>::new();  // ← AnonRecord is undefined
```

Correct output:
```rust
let ps: Vec<Product> = Vec::<Product>::new();
// or
let ps: Vec<Product> = vec![];
```

## Impact

- In Grammar Lab `optional-handling` experiment, 3 out of 10 tasks (t07, t08, t10) fail due to this bug
- LLM output is correct but compilation fails = **artificially lowering LLM survival rate**
- Triggered every time `let xs: List[T] = []` is written in tests. Common pattern

## Reproduction

```almide
type Item = { name: String, value: Int }

test "empty list of records" {
  let items: List[Item] = []
  assert_eq(list.len(items), 0)
}
```

```
$ almide test repro.almd
error[E0412]: cannot find type `AnonRecord` in this scope
```

## Root Cause (suspected)

In `emit_rust/`'s empty list codegen, the anonymous record type mapping is missing when resolving type parameters. The `Ty::Record(fields)` → Rust struct name (`AlmdRec_*` or type alias) conversion is absent in the empty list context.

## Fix Strategy

1. Locate empty list `[]` codegen in `emit_rust/`
2. Check where element type is obtained from type annotations
3. Apply `Ty::Record` → concrete Rust type name conversion to empty lists as well
4. Test: verify `let xs: List[{a: Int}] = []` compiles and passes

## Verification Tasks

- [ ] Grep `AnonRecord` in `src/emit_rust/` → identify generation site
- [ ] Check if the same issue exists elsewhere (e.g., `none` type inference)
- [ ] After fix, verify Grammar Lab t07/t08/t10 change to PASS

## Related

- [design-debt.md #2](../on-hold/design-debt.md) — Fundamental anonymous record design
- Grammar Lab [optional-handling REPORT](../../../research/grammar-lab/experiments/optional-handling/REPORT.md)
