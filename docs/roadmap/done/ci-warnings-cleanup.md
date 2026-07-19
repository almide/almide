<!-- description: Clean up CI warnings and non-fatal ICE messages on Windows/macOS CI -->
<!-- done: 2026-06-06 -->
# CI Warnings Cleanup

> **Status**: Active
> **Observed**: v0.23.10, both Windows and macOS CI
> **Impact**: Noise in CI logs, 4 ICE messages (non-fatal)

## Summary

CI output contains ~40 warnings and 4 ICE messages across spec tests. All tests pass — these are diagnostic noise, not failures. But ICE messages indicate real monomorphization gaps.

## Categories

### Critical: ICE — TypeVar remains after monomorphization (4 files)

```
[ICE] N TypeVar(s) remain after monomorphization. Generic params should be fully substituted.
```

| File | TypeVars remaining |
|------|--------------------|
| `spec/integration/codegen_patterns_test.almd` | 2 |
| `spec/lang/generics_test.almd` | 3 |
| `spec/lang/regression_v0_11_test.almd` | 6 |
| `spec/lang/type_system_test.almd` | 9 |

**Root cause**: User-defined generic functions (`first[T]`, `zip_with[A,B,C]`, `unwrap_or[T]`) are not fully monomorphized. The mono pass doesn't substitute all type parameters.

**Related**: `spec/lang/function_test.almd` also shows 1 remaining TypeVar (generic factorial?).

### Critical: POSTCONDITION VIOLATION — Lambda param unresolved (1 file)

```
[POSTCONDITION VIOLATION] Lambda param VarId(30) still unresolved: ir=Unknown vt=Unknown
[POSTCONDITION VIOLATION] [ConcretizeTypes] 3 expressions remain with unresolved types.
```

File: `spec/lang/single_quote_test.almd` or nearby (fn `__test_almd_override`)

**Root cause**: Lambda with param type Unknown survives ConcretizeTypes. The override/shadowing pattern may confuse type resolution.

### Noise: Unused variable warnings (~25 instances)

Spec test files use variables for side effects only (e.g., `for (k, v) in m { ... }` where only `v` is used).

**Fix**: Prefix with `_` in spec files, or suppress in test mode.

### Noise: Unused import warnings (4 instances)

- `value` import in block_comment_raw_string_test
- `fs`, `env`, `process` imports in stdlib tests

**Fix**: Remove unused imports or prefix with `_`.

### Noise: E015 — stdlib shadow warnings (5 instances)

```
warning[E015]: fn 'factorial' has the same signature as stdlib `math.factorial`
```

Files: `codegen_stress_test`, `function_test`, `generics_test`, `regression_v0_11_test`, `type_system_test`

**Not a bug**: These are intentional re-implementations in test files. Could suppress E015 in test blocks.

## Recommended Actions

1. **Fix ICE**: Investigate mono pass for user-defined generics — TypeVar substitution incomplete
2. **Fix POSTCONDITION**: Lambda param type resolution for override patterns
3. **Suppress test noise**: `_` prefix for unused vars, remove unused imports
4. **Consider**: `--quiet-warnings` flag for CI test steps, or suppress E015 in `test` blocks
