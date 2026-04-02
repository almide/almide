<!-- description: Unify inference variables with named TypeVars in generic function bodies -->
# Type Inference Unification for Generic Functions

## Problem

When a generic function calls stdlib functions, the checker creates fresh inference variables (`?n`) for the called function's type parameters. These `?n` are not unified with the caller's named TypeVars (`A`, `B`), causing monomorphization to fail on WASM.

```almide
fn zip_map[A, B, C](xs: List[A], ys: List[B], f: (A, B) -> C) -> List[C] =
  list.zip(xs, ys) |> list.map((p) => { let (a, b) = p; f(a, b) })
```

- Rust target: works (inference vars become Unknown, Rust's own inference fills gaps)
- WASM target: fails (`type mismatch: expected i64, found i32` — Unknown types produce wrong WASM types)

## Root Cause

The checker uses two kinds of type variables:
- `TypeVar(A)` — named, from generic declarations
- `TypeVar(?n)` — inference vars, from constraint solving

When `list.zip` is called inside `zip_map`:
1. `list.zip`'s generics get fresh `?3`, `?4`
2. Result type: `List[(?3, ?4)]` instead of `List[(TypeVar(A), TypeVar(B))]`
3. Constraint solving should unify `?3 ← TypeVar(A)` but doesn't (UnionFind shows `?3 → None`)
4. Lowering converts unresolved `?n → Unknown`
5. Mono's `substitute({A: Int})` can't touch `Unknown`

## Solution

### Approach: Propagate named TypeVars through generic calls

In `check_named_call_with_type_args` (calls.rs:252-254), when inside a generic function body:
- The `unify_call_arg` bindings already contain `A → TypeVar(A)` (from the caller's params)
- These should flow into `final_bindings` and be used to substitute the return type
- The return type `List[(A, B)]` → `substitute({A: TypeVar(A), B: TypeVar(B)})` → `List[(TypeVar(A), TypeVar(B))]`

This avoids creating fresh `?n` entirely for generic-to-generic calls.

### Additionally: BindDestructure constraints

`let (a, b) = p` where `p: (TypeVar(A), TypeVar(B))` should constrain:
- `a: TypeVar(A)`, `b: TypeVar(B)`
- Currently the pattern bindings get `?n` types that are never unified with the tuple element types

### Files to Modify

- `crates/almide-frontend/src/check/calls.rs:282-287` — Don't create fresh vars when bindings already have named TypeVars
- `crates/almide-frontend/src/check/infer.rs` — Add constraints for BindDestructure pattern types
- `crates/almide-frontend/src/lower/mod.rs:377` — `resolve_inference_typevars` could use UF to restore `?n → TypeVar(A)` as fallback

### Impact

- Fixes WASM codegen for generic functions that call stdlib with tuple returns
- No impact on Rust target (already works via Rust's type inference)
- Affects: `list.zip`, `list.enumerate`, any stdlib returning tuples in generic context

### Test

```almide
fn zip_with[A, B, C](xs: List[A], ys: List[B], f: (A, B) -> C) -> List[C] =
  list.zip(xs, ys) |> list.map((p) => { let (a, b) = p; f(a, b) })

test "zip_with" {
  assert_eq(zip_with([1,2], ["a","b"], (n, s) => int.to_string(n) + s), ["1a", "2b"])
}
```

Must pass on both `--target rust` and `--target wasm`.

## Reference Architecture Insights

Surveyed Gleam, Roc, Elm, Nickel, MoonBit compilers for relevant patterns:

| Problem | Pattern | Source |
|---|---|---|
| Inference var unification | **Rigid vs. unbound distinction**: declared generics are rigid (never instantiated), inferred types are unbound (get fresh instances). Prevents mixing the two. | Gleam |
| Monomorphization | **Variable → MonoTypeId cache** with deduplication. Walk inference solutions via `lower_var(subs, var)` to create concrete mono types. | Roc |
| WASM type resolution | **Defunctionalization**: closures → tagged unions → switch dispatch. Eliminates indirect calls and function pointer types. | Roc |
| Union-Find efficiency | **Rank-based pooling** (8 pools by variable rank) + **CPS unification** with lazy propagation. O(α(n)) amortized. | Elm |
| Sound generalization | **Level-based polymorphism** (VarLevel u16): prevents unsound generalization when levels mismatch. | Nickel |

### Recommended approach for Almide

Adopt Gleam's **rigid/unbound** pattern:
1. In `enter_generics`: mark TypeVars as **rigid** (new flag on TypeVar, or separate enum variant)
2. In `check_named_call_with_type_args`: when building bindings for a called function's generics, if the binding resolves to a rigid TypeVar from the outer scope, use it directly instead of creating a fresh `?n`
3. `resolve_inference_typevars`: rigid TypeVars pass through to IR untouched; only unbound `?n` become Unknown
4. Mono: `substitute({A: Int})` works on rigid TypeVars as before

This eliminates the `?n → Unknown → can't substitute` problem at its source.
