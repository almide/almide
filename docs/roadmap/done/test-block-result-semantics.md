<!-- description: Remove auto-unwrap of effect fn results in test blocks -->
<!-- done: 2026-04-03 -->
# Test Block Result Semantics

## Problem

Test blocks auto-unwrap effect fn results. This is invisible behavior that:

1. Prevents testing error paths of `effect fn foo() -> T`
2. Makes test blocks behave differently from effect fn bodies
3. Has no precedent in other languages

```almide
effect fn validate(n: Int) -> Int = {
  guard n > 0 else err("bad")!
  n
}

test "ok works" { assert_eq(validate(5), 5) }     // auto-unwrapped
test "err path" { ??? }                            // no way to test
```

Current workaround: `result_of(validate(-1))` — ad-hoc builtin just for this.

## Language Comparison

| Language | Test behavior | Error path testing |
|---|---|---|
| **Rust** | No auto-unwrap. Test fns can return `Result`. | `assert!(result.is_err())`, `?` propagation |
| **Go** | No auto-unwrap. `(value, err)` always visible. | `if err != nil { t.Fatal(err) }` |
| **Gleam** | No auto-unwrap. Result is explicit. | `should.equal(result, Error("bad"))` |
| **Zig** | No auto-unwrap. Error unions explicit. | `try` / `catch` / `expectError` |
| **Swift** | `throws` requires `try` in tests. | `XCTAssertThrowsError { try foo() }` |
| **Kotlin** | No auto-unwrap. Result explicit. | `assertThrows<E> { foo() }` |
| **Haskell** | IO/ExceptT explicit. | `shouldThrow`, `shouldReturn` |
| **Roc** | Task explicit. | `expect` on Task result |

**No mainstream language auto-unwraps in tests.** Every language requires the programmer to explicitly choose whether to handle or propagate errors in test contexts.

Almide's auto-unwrap is a codegen convenience that creates an asymmetry: the same `validate(5)` expression means `Int` in a test block but `Result[Int, String]` everywhere else.

## Proposal

Remove auto-unwrap in test blocks. Effect fn calls return `Result[T, String]` in test blocks, same as in effect fn bodies.

### Before (current)

```almide
effect fn validate(n: Int) -> Int = {
  guard n > 0 else err("bad")!
  n
}

test "ok" { assert_eq(validate(5), 5) }         // magic auto-unwrap
// test "err" { ??? }                            // impossible
```

### After (proposed)

```almide
test "ok value" { assert_eq(validate(5)!, 5) }          // explicit unwrap
test "ok result" { assert_eq(validate(5), ok(5)) }      // Result-aware
test "err" { assert_eq(validate(-1), err("bad")) }      // natural
```

## Impact Analysis

### Actual scope of auto-unwrap

Effect fns in the test suite fall into two categories:

1. **`effect fn foo() -> Result[T, E]`** — already explicit. Auto-unwrap does not apply. **This is the majority.**
2. **`effect fn foo() -> T`** — auto-unwrapped in test blocks. ~15 such functions in spec/, called from ~20 test sites.

Most test authors already write `-> Result[T, String]` explicitly, suggesting the auto-unwrap isn't the preferred pattern.

### Migration

~20 call sites need `!` added. Examples:

```diff
- assert_eq(returns_int(), 42)
+ assert_eq(returns_int()!, 42)

- assert_eq(guard_effect(5), 10)
+ assert_eq(guard_effect(5)!, 10)
```

Small, mechanical change. `almide fmt` could potentially auto-fix this.

## Implementation

### Checker change

In test blocks, effect fn calls to non-Result-returning effect fns should produce `Result[T, String]`:

```rust
// check_named_call_with_type_args, after computing ret type:
if sig.is_effect && self.env.in_test_block && !ret.is_result() {
    return Ty::result(ret, Ty::String);
}
```

Add `in_test_block: bool` flag to TypeEnv. Set it during `check_decl(Decl::Test { .. })`.

**Why this won't cascade** (unlike the v0.11.2 attempt): the change is strictly per-call-site. Only the return type of the specific call expression changes. Argument type inference and constraint propagation for the call's parameters are unaffected.

### Codegen change

In `ResultPropagationPass`, for test blocks, stop inserting `Unwrap` around calls to lifted effect fns:

```rust
// Remove:
if func.is_test {
    func.body = insert_try_for_lifted(std::mem::take(&mut func.body), &lifted_fns);
}
```

The call already returns `Result[T, String]` after lifting. No unwrap needed.

### Cleanup

- Remove `result_of` builtin (no longer needed)
- Remove `result_of` handling from ResultPropagationPass, BuiltinLoweringPass, WASM calls
- Update `result_of_test.almd` → test without `result_of`
- Update CHEATSHEET.md

## What We Keep

- `!` for explicit unwrap (propagates error in effect fn, panics in test)
- `?` for Result → Option conversion
- `??` for unwrap with fallback
- `match` on ok/err patterns (already works without unwrap when arms use Ok/Err)

## What We Remove

- Auto-unwrap of effect fn calls in test blocks
- `result_of` builtin

## Risk

- **v0.11.2 regression**: The previous attempt changed checker inference globally. This approach is scoped to `in_test_block` flag, affecting only return types of effect fn calls in test blocks.
- **Breaking change**: ~20 call sites in spec/. No user code affected (language is pre-1.0).
- **Semantic shift**: Tests become more explicit. This is the direction every other language has taken.

## Files to Modify

- `crates/almide-frontend/src/check/mod.rs` — add `in_test_block` flag, set during test block inference
- `crates/almide-frontend/src/check/calls.rs` — wrap return type in Result for effect fn calls in test blocks
- `crates/almide-codegen/src/pass_result_propagation.rs` — remove `insert_try_for_lifted` for test blocks
- `crates/almide-frontend/src/check/builtin_calls.rs` — remove `result_of`
- `crates/almide-codegen/src/pass_builtin_lowering.rs` — remove `result_of` fallback
- `crates/almide-codegen/src/emit_wasm/calls.rs` — remove `result_of` handler
- `spec/lang/result_of_test.almd` — rewrite without `result_of`
- `spec/lang/result_option_matrix_test.almd` — add `!` to ~8 call sites
- `docs/CHEATSHEET.md` — update test block section
