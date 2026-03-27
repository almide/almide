<!-- description: Fix ok(value) being lost in guard expressions within effect do-blocks -->
<!-- done: 2026-03-16 -->
# Guard `ok(value)` Value Loss in Effect Do-Block

**Test:** `spec/lang/error_test.almd`
**Status:** 1 rustc error, 0 checker errors

## Current State

```almide
effect fn hamming_distance(a: String, b: String) -> Result[Int, String] = {
  // ...
  var count = 0
  do {
    guard i < len else ok(count)   // count is an Int variable
    // ...
  }
}
```

Generates:

```rust
if !(j < almide_rt_list_len(&xs)) { return Ok(()) };
//                                            ^^ should be Ok(count)
```

## What's Broken

The guard's `else ok(count)` produces `return Ok(())` instead of `return Ok(count)`. The `count` variable reference is lost during lowering.

## Why It Happens

Suspected: the guard's else expression `ok(count)` goes through auto-try processing which strips the `Ok` wrapper, then the guard codegen re-wraps it in `Ok(...)` but with the wrong inner value. Or the IR lower pass for the guard's else expression loses the variable reference when `ok()` is involved.

## Investigation Needed

1. Check what IR the guard `else ok(count)` produces — is it `ResultOk { expr: Var(count) }` or `ResultOk { expr: Unit }`?
2. Trace through `lower_stmt` → `Guard` handler → `self.lower_expr(else_)` to see where `count` gets replaced with `()`
3. Check if the `list.len` template's `&` on `xs` causes a clone/move issue that shadows the count variable

## Expected Result

```rust
if !(i < len) { return Ok(count) };
```

## Proposed Fix

Debug the IR lower pass (`src/lower/statements.rs` guard handling) for `guard else ok(expr)` in effect functions. The issue is likely in how the guard's else expression is lowered when it contains `ok()` — the auto-try or Ok-wrap logic may interfere. Alternatively, check if the Almide→IR lowering in `src/lower/` loses the `count` reference.

**Effort:** ~10 lines once root cause identified (investigation is the main cost)
