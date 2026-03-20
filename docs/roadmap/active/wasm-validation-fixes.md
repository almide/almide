# WASM Validation Error Fixes [ACTIVE]

## Current Status

- **WASM**: 22/73 pass, 51 fail (scope_test recovered)
  - 15 compile failures (validation error) — **this roadmap's scope**
  - 36 runtime traps — separate issue (unimplemented features)
- **Rust**: 153/153 pass (must not regress)

## Completed

- **Scratch local undercount**: Fixed in commits b59aaeb..47d1602
  - `count_scratch_depth` for ForIn, Call, Lambda, Try
  - `collect_var_refs` Lambda recursion
  - 4 files unblocked (control_flow, for, scope, variable)

- **Closure env typed zero-init**: Fixed (this branch, uncommitted)
  - `emit_lambda_closure` (calls.rs:1286) used `i32_const(0)` for all capture types
  - Now emits `i64_const(0)` for Int, `f64_const(0.0)` for Float
  - scope_test validation pass recovered

## Root Cause 1: TypeVar Leak — solutions overwrite in unify_infer

### Confirmed Root Cause (fully traced)

`check/mod.rs` `unify_infer()` uses `HashMap::insert` which **overwrites** existing
solutions without propagation. When multiple constraints touch the same TypeVar,
a concrete solution gets destroyed.

**Concrete example**: `assert_eq(list.map(xs: List[Int], (x) => x * 2), [])`

```
Step 1: lambda param x → fresh ?1
Step 2: list.map constrains Fn(Int)->?B vs Fn(?1)->?1
        → constrain(Fn(Int)->?1, Fn(?1)->?1)
        → eager unify_infer: Int vs ?1 → solutions[?1] = Int  ✓
Step 3: list.map return type → List[?B] where B=?1 → List[?1]
Step 4: assert_eq constrains List[?1] vs List[?2]  (where ?2 = empty list [] element)
        → unify_infer(?1, ?2)
        → solutions[?1] = ?2  ← OVERWRITES Int! ✗
Step 5: resolve_vars: ?1 → ?2 → (no solution) → TypeVar("?2") LEAKED
```

### Fix (not yet landed — current attempt regresses grade-report)

When `solutions[?a]` already has a value and a new `unify_infer(?a, new_val)` is called:
- Must propagate existing solution to `new_val` (so chain is resolved)
- Must NOT prevent overwrite unconditionally (breaks cases where later
  constraints refine the type, e.g. `list.fold(students, ok([]), fn)`)

**Correct approach**: overwrite IS allowed, but also `unify_infer(old_solution, new_val)`
to propagate. The key: propagation must happen REGARDLESS of whether we overwrite.

```rust
if let Some(existing) = self.solutions.get(&id_a).cloned() {
    self.unify_infer(&existing, b);  // propagate: old ↔ new
}
self.solutions.insert(id_a, b.clone());  // overwrite as before
```

This ensures `?2 = Int` is created (from propagation) even after `?1` is overwritten
to `?2`. Both paths exist in solutions: `?1→?2` and `?2→Int`. `resolve_vars` follows
the chain correctly.

**Status**: Not yet implemented. Previous attempts either:
- Blocked overwrite → broke `list.fold` in grade-report (152/153 Rust)
- Need the "propagate + overwrite" approach above

### Affected Files (11)

auto_derive_test, codec_list_test, codec_p0_test, codec_weather_test,
default_fields_test, edge_cases_test, function_test, generics_test,
protocol_advanced_test, protocol_extreme_test, protocol_stress_test, type_system_test

### After Fix: Cleanup

1. Remove `resolve_lambda_param_ty` heuristic from `emit_wasm/mod.rs`
   (TypeVar→Int default becomes unnecessary when checker resolves all TypeVars)
2. Make `ty_to_valtype` panic on TypeVar instead of silent i32 fallback
3. Add IR validation assert: no `TypeVar("?N")` in any `IrExpr.ty`
4. Remove `default_unresolved_vars` from `check/types.rs` (dead code)

## Root Cause 2: Codec/Value Type Mismatch [4 files]

**Files**: codec_convenience_test, codec_nested_test, codec_test, value_utils_test

These have 0 TypeVar leaks. The issue is that Codec-generated IR constructs Value
variant types with raw Int/Float payloads. WASM codegen stores i64/f64 where an
i32 pointer is expected.

**Fix**: Either implement Value-aware WASM codegen for Codec, or skip.

## Fix Order

| Step | What | Files Fixed | Cumulative |
|------|------|-------------|------------|
| 1 | unify_infer propagation fix | +11 compile failures gone | ~32/73 |
| 2 | Cleanup: remove heuristics, add validation | 0 (correctness) | ~32/73 |
| 3 | Codec WASM support (or skip) | +4 | ~36/73 |
| — | Runtime traps (separate work) | remaining 36 | — |

## Runtime Traps (out of scope)

After validation errors are fixed, remaining failures are runtime traps:
- Map iteration, record destructure, deep equality
- float.to_string, protocol dispatch, string operations
- Fan/async (N/A for WASM), TCO
