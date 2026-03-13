# Bidirectional Type Inference for Lambda Parameters [ACTIVE]

## Problem

Lambda parameters without type annotations receive `Ty::Unknown`. This breaks ambiguous UFCS inside lambdas:

```almide
let nums = [1, 2, 3]
nums.map(fn(x) => x.to_string())   // ✅ to_string is unambiguous (Int only)
nums.map(fn(x) => x.len())         // ❌ len is ambiguous (string/list/map)

// Workaround: qualified call
nums.map(fn(x) => string.len(x))   // ✅ works but ugly
// Or: explicit annotation
nums.map(fn(x: Int) => x.len())    // ✅ works but redundant
```

The type checker **knows** `x` should be `Int` (from `List[Int]` + `map` signature), but this information is never propagated to lambda parameter types.

## Root Cause Analysis

### Current Flow

```
[1, 2, 3].map(fn(x) => x.len())

1. Checker: check_call(map, [[1,2,3], fn(x) => x.len()])
2. Checker: check_expr(fn(x) => x.len())
   → x has no annotation → Ty::Unknown
   → x.len() checked with x: Unknown
   → Lambda type: Fn[Unknown] -> Unknown
3. Checker: check_module_call("list.map")
   → unify(List[A], List[Int]) → A = Int
   → unify(Fn[A]->B, Fn[Unknown]->Unknown) → binds but too late
   → Lambda params already stored as Unknown in expr_types
4. Lowerer: lower lambda body
   → expr_ty(x) → Ty::Unknown (from var_table)
   → UFCS: resolve_ufcs_by_type("len", Unknown) → None
   → resolve_ufcs_candidates("len") → ["string", "list", "map"] (3)
   → Ambiguous → falls through to unresolved method
   → Generates: x.len() as method call → Rust compile error
```

### The Gap

Arguments are checked **before** being matched against the function signature. The checker calls `check_expr(arg)` on each argument independently, then validates types against the signature via `unify()`. By the time unification binds `A = Int`, the lambda body has already been checked with `x: Unknown`.

### Key Code Locations

| File | Lines | What |
|------|-------|------|
| `src/check/expressions.rs` | 432-451 | Lambda checking — params default to `Ty::Unknown` |
| `src/check/calls.rs` | 33-109 | `check_call` — args checked before signature matching |
| `src/check/calls.rs` | 293-312 | `check_module_call` — unification with TypeVars |
| `src/types.rs` | 199-242 | `unify()` — TypeVar binding infrastructure |
| `src/types.rs` | 244-270 | `substitute()` — type substitution |
| `src/lower.rs` | 466-478 | Lambda lowering — params from checker or `Ty::Unknown` |
| `src/lower.rs` | 711-750 | UFCS resolution — needs concrete receiver type |
| `src/stdlib.rs` | 52-136 | `resolve_ufcs_candidates` / `resolve_ufcs_by_type` |

## Fix: Bidirectional Inference in 3 Phases

### Phase 1: Pre-resolve expected types for lambda arguments

**Where**: `check_call` / `check_module_call` in `src/check/calls.rs`

Before checking each argument, look up the callee's signature. If a parameter expects `Fn[T] -> U` and the corresponding argument is a lambda, compute the expected parameter types from already-checked arguments.

```
check_call(callee, args):
  sig = lookup_signature(callee)
  bindings = {}

  // First pass: check non-lambda args, collect TypeVar bindings
  for (i, arg) in args:
    if arg is NOT Lambda:
      arg_ty = check_expr(arg)
      unify(sig.params[i], arg_ty, &bindings)

  // Second pass: check lambda args WITH expected types
  for (i, arg) in args:
    if arg IS Lambda:
      expected = substitute(sig.params[i], &bindings)  // e.g., Fn[Int] -> B
      arg_ty = check_expr_with(arg, Some(&expected))
      unify(sig.params[i], arg_ty, &bindings)
```

**Why two passes**: The receiver (e.g., `List[Int]`) must be checked first to bind `A = Int`. Then the lambda can receive `Fn[Int] -> B` as its expected type.

**Existing infrastructure**:
- `unify()` already handles TypeVar binding
- `substitute()` already replaces bound TypeVars
- Module call checker already does this pattern (lines 293-312)

### Phase 2: Thread expected type into lambda checking

**Where**: `check_expr_with` Lambda case in `src/check/expressions.rs`

Currently (lines 432-451):
```rust
ast::Expr::Lambda { params, body, .. } => {
    let pts: Vec<Ty> = params.iter().map(|p| {
        let ty = p.ty.as_ref()
            .map(|te| self.resolve_type_expr(te))
            .unwrap_or(Ty::Unknown);        // ← always Unknown
        self.env.define_var(&p.name, ty.clone());
        ty
    }).collect();
    let ret = self.check_expr(body);
    // ...
}
```

Modified:
```rust
ast::Expr::Lambda { params, body, .. } => {
    // Extract expected param types from expected Fn type
    let expected_params = match expected {
        Some(Ty::Fn { params: ep, .. }) => Some(ep),
        _ => None,
    };

    let pts: Vec<Ty> = params.iter().enumerate().map(|(i, p)| {
        let ty = if let Some(te) = &p.ty {
            self.resolve_type_expr(te)          // Explicit annotation wins
        } else if let Some(ep) = &expected_params {
            ep.get(i).cloned().unwrap_or(Ty::Unknown)  // ← NEW: from expected
        } else {
            Ty::Unknown
        };
        self.env.define_var(&p.name, ty.clone());
        ty
    }).collect();

    // Thread expected return type to body
    let expected_ret = match expected {
        Some(Ty::Fn { ret, .. }) => Some(ret.as_ref()),
        _ => None,
    };
    let ret = self.check_expr_with(body, expected_ret);
    // ...
}
```

**Result**: Lambda param `x` in `nums.map(fn(x) => ...)` gets `Ty::Int` instead of `Ty::Unknown`.

### Phase 3: Propagation to IR lowerer

**Where**: `src/lower.rs` — no changes needed.

The lowerer already reads from `expr_types` (populated by the checker). Once the checker stores `x: Int` instead of `x: Unknown`, the lowerer's `expr_ty()` returns `Ty::Int` → UFCS resolves correctly.

The lowerer's lambda handling (line 469) also checks `expr_types` via the span lookup, so if the checker stores the correct type for the lambda param span, it flows through automatically.

**Verification**: After Phase 2, confirm that:
1. `expr_types` contains the lambda param span → correct type
2. `infer_expr_ty(Ident("x"))` returns the correct type via `var_table`
3. UFCS resolves ambiguous methods inside lambdas

## Edge Cases

### Chained HOFs
```almide
[1, 2, 3].map(fn(x) => x * 2).filter(fn(y) => y > 3)
```
- `map` binds `A = Int`, lambda param `x: Int` ✅
- `map` return type `List[Int]` → `filter` binds `A = Int`, lambda param `y: Int` ✅
- Requires: return type substitution propagates through chained calls

### Nested Lambdas
```almide
[[1, 2], [3, 4]].map(fn(inner) => inner.map(fn(x) => x + 1))
```
- Outer: `List[List[Int]]` → `inner: List[Int]` ✅
- Inner: `List[Int]` → `x: Int` ✅
- Requires: recursive application of Phase 1-2

### Lambda with Partial Annotations
```almide
pairs.map(fn(k: String, v) => v.len())
```
- `k` explicitly `String`, `v` inferred from expected type
- Phase 2 handles this: explicit annotation takes precedence

### UFCS Calls as Callee
```almide
words.map(fn(w) => w.len())
```
- UFCS resolves `words.map` to `list.map` → signature available
- Phase 1 needs to handle both direct calls and UFCS-resolved calls

### Closures Passed to User Functions
```almide
fn apply(f: Fn[Int] -> String) -> String = f(42)
apply(fn(x) => x.to_string())
```
- User function signature has `Fn[Int] -> String` directly (no TypeVars)
- Phase 1: substitute gives `Fn[Int] -> String` immediately
- Phase 2: `x` inferred as `Int` ✅

## Test Plan

```almide
// Phase 1-2: Basic inference
test "map lambda param inferred" {
  let result = [1, 2, 3].map(fn(x) => x + 10)
  assert_eq(result, [11, 12, 13])
}

// Ambiguous UFCS in lambda
test "len in map lambda" {
  let lengths = ["hello", "hi"].map(fn(s) => s.len())
  assert_eq(lengths, [5, 2])
}

// Chained HOFs
test "map then filter" {
  let result = [1, 2, 3, 4].map(fn(x) => x * 2).filter(fn(y) => y > 4)
  assert_eq(result, [6, 8])
}

// Nested lambdas
test "nested map" {
  let result = [[1, 2], [3]].map(fn(inner) => inner.map(fn(x) => x + 1))
  assert_eq(result, [[2, 3], [4]])
}

// join inside lambda (the original almide-grammar blocker)
test "join in map lambda" {
  let groups = [["a", "b"], ["c"]]
  let result = groups.map(fn(g) => g.join(","))
  assert_eq(result, ["a,b", "c"])
}

// User function with Fn param
test "user fn lambda inference" {
  fn apply(f: Fn[Int] -> Int) -> Int = f(5)
  assert_eq(apply(fn(x) => x * 3), 15)
}
```

## Implementation Order

1. **Phase 1** — Two-pass argument checking in `check_call` / `check_module_call`
2. **Phase 2** — Thread expected type into lambda checking
3. **Test** — All test cases above + `almide test` full suite
4. **Phase 3** — Verify lowerer picks up correct types (likely zero changes)
5. **Cleanup** — Remove qualified-call workarounds in almide-grammar once inference works

## Relationship to UFCS Type Resolution Roadmap

This is a prerequisite for fully solving [UFCS Type Resolution](ufcs-type-resolution.md). The UFCS roadmap covers span-based lookup failures for member access / call chains. This roadmap covers the deeper issue: lambda parameters never receive inferred types, making all ambiguous UFCS inside lambdas fundamentally broken regardless of span lookup accuracy.

Once both are complete, all UFCS calls should work uniformly — inside or outside lambdas, with or without chaining.
