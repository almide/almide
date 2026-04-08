<!-- description: Fix almide test to compile tests that transitively import native-deps modules -->
<!-- done: 2026-04-08 -->

# Test Build with Native Deps

## Problem

`almide test` fails to compile test files that transitively depend on modules using `@extern(rs, ...)` and `[native-deps]`.

Example: `dispatch_test.almd` imports `self.dispatch`, which imports `self.wasm_rt`, which has `@extern(rs, "wasmtime_bridge", ...)`. The test build fails with 29 Rust compilation errors because the test build doesn't include the native-deps or native/ Rust sources.

`almide build` succeeds because it includes native-deps. `almide test` does not.

## Reproduction

```
# porta project with native-deps in almide.toml
almide build    # → success
almide test     # → 3/6 test files fail (those that transitively import wasm_rt)
```

## Expected

`almide test` should include the same native-deps and native/ sources as `almide build` when the test file belongs to a project with `almide.toml`.

## Impact

porta's dispatch, mod, and wasm_rt tests are skipped in CI. 36 tests not running.
