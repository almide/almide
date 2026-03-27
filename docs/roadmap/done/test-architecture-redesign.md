<!-- description: Separate effect permission from Result auto-unwrap in test infra -->
# Test Architecture Redesign

> Status: On Hold (Compiler Internals)
> Phase: 2.x

## Problem

Test infrastructure is complex because `in_effect` conflates two orthogonal concerns:
1. **Permission to call effect fns** (I/O access)
2. **Result auto-unwrap in let bindings** (ergonomic sugar)

This forced 5 workarounds:
- `in_test` checker flag to suppress auto-unwrap
- `!func.is_test` in ResultPropagationPass to suppress auto-?
- `__test_almd_` prefix to avoid name collision with convention methods
- `force_test` mode for files with both main and test blocks
- `research/` exclusion from test discovery

## Root Cause

```
in_effect = true  →  can call effect fns  (permission)
                  →  auto-unwrap Result   (sugar)
```

Tests need the permission but not the sugar. Today this requires `in_test` to carve out an exception.

## Ideal Design

Split `in_effect` into two orthogonal flags:

```
can_call_effect: bool   — permission to call effect fns
auto_unwrap: bool       — Result auto-unwrap in let/var bindings
```

| Context | can_call_effect | auto_unwrap |
|---------|----------------|-------------|
| pure fn | false | false |
| effect fn | true | true |
| test block | true | false |

This eliminates `in_test` entirely. The checker just checks `can_call_effect` for effect isolation and `auto_unwrap` for let binding types.

## Additional Improvements

### Test namespacing
- Convention methods (`Type.method`) can collide with test names
- Current fix: `__test_almd_` prefix (brute force)
- Better: hash-based test names, or `mod __almide_tests` isolation

### Test compilation mode
- Detect test blocks at IR level, emit `#[cfg(test)]` in codegen
- Eliminate `force_test` flag — `--test` decision based on IR, not source scanning

### Test template
- Consider `fn test_name() -> Result<(), String>` with auto-? enabled
- Tests that want to inspect Results use `match` (no let binding = no unwrap)
- Would allow `let x = effect_fn()` to auto-unwrap in tests too (opt-in via let)

## Impact

- Remove `in_test` flag from TypeEnv
- Remove `!func.is_test` from ResultPropagationPass
- Remove `__test_almd_` prefix logic
- Remove `force_test` / `cmd_run_inner_test`
- Simplify `collect_test_files` (no exclusion list needed)
- ~50 lines of workaround code eliminated

## Non-blocking

Current implementation (v0.8.1) works: 110/110 tests pass. This is a cleanup for conceptual integrity, not a bug fix.
