<!-- description: Fix effect fn Rust codegen to wrap return type in Result -->
<!-- done: 2026-03-24 -->
# Effect fn Result Wrapping

**Priority:** 1.0 blocker
**Prerequisite:** ResultPropagationPass exists

## Problem

`effect fn` Rust codegen is broken. It inserts the `?` operator but does not convert the return type to `Result`.

```
effect fn fetch() -> String = do { http.get(url) }

Current:  fn fetch() -> String { (http_get(url))? }          ← rustc error
Expected: fn fetch() -> Result<String, String> { Ok((http_get(url))?) }  ← correct
```

CLAUDE.md design intent: `effect fn` → `Result<T, String>`, auto `?` propagation

## Affected patterns

| Pattern | Current | After fix |
|---|---|---|
| `effect fn foo() -> String` | `-> String` + `?` ❌ | `-> Result<String, String>` + `Ok()` |
| `effect fn foo() -> Result[T, E]` | `-> Result<T, E>` ✅ | No change |
| `effect fn main() -> Unit` | `-> ()` + `?` ❌ | `-> Result<(), String>` (Termination trait) |
| `test "..." { ... }` | ✅ | No change (separate is_test handling) |
| Pure fn | ✅ | No change |

## Design

**Change location:** Centralized in a single place in `src/codegen/pass_result_propagation.rs`

Condition: `is_effect && !is_test && !ret_ty.is_result()`

1. Convert `func.ret_ty` to `Result<ret_ty, String>`
2. Wrap body tail with `Ok(expr)`
3. Existing logic (`fn_returns_result=true`) works as-is

### TS target

TS handles this with `async/await` + `try/catch`. This transform is Rust/WASM only. Add target detection if the same pass runs for TS.

### main() handling

No special treatment needed. Rust's `main()` can return `Result<(), E: Debug>` (`Termination` trait).

### Caller-side effects

- **effect fn → effect fn:** Auto `?` makes the chain correct
- **test → effect fn:** test has separate handling with `is_test=true`, receives Result via match
- **pure fn → effect fn:** Checker rejects with E006

## Implementation progress

| Phase | Content | Status |
|---|---|---|
| Phase 1 | ret_ty conversion + Ok wrapping in ResultPropagationPass (Rust only) | ✅ Complete |
| Phase 2 | Verify main() works with Result<(), String> | ✅ Complete |
| Phase 3 | Checker auto-unwrap (effect fn body matches Result → T) | ✅ Complete |
| Phase 4 | LICM effect detection: replace with TypeEnv-derived effect_fn_names | ✅ Complete |
| Phase 5 | Lifted fn calls inside tests → .unwrap() | ✅ Complete |
| Phase 6 | All CI passing (Rust/TS/WASM) | ✅ Complete |
