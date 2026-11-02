<!-- description: Standing ledger of compiler warnings CI tolerates, and the plan to drive each class to zero -->
# CI Warnings Cleanup

[![English](https://img.shields.io/badge/English-24292f?style=flat-square)](ci-warnings-cleanup.md) [![日本語](https://img.shields.io/badge/%E6%97%A5%E6%9C%AC%E8%AA%9E-d0d7de?style=flat-square)](ci-warnings-cleanup.ja.md) [![简体中文](https://img.shields.io/badge/%E7%AE%80%E4%BD%93%E4%B8%AD%E6%96%87-d0d7de?style=flat-square)](ci-warnings-cleanup.zh-CN.md)

> This is a non-normative reader copy of the [pinned original](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/active/ci-warnings-cleanup.md), not a new authority, generator output or automatically synchronized document. Original sources and machine consumers are unchanged. Historical claims and examples have not been independently revalidated here.


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
