<!-- description: User-defined error types in Result, and test block Result visibility -->
# Flexible Error Types

## Current State

Error type is hardcoded to `String`:

- `effect fn` always produces `Result[T, String]`
- `!` propagates `String` errors
- `err("message")` is the only way to create errors
- Test blocks auto-unwrap effect fn results, so `assert_eq(f(), ok(1))` fails — must write `-> Result[Int, String]` explicitly to test error cases

This is simple and LLM-friendly, but has two limitations:
1. Can't match on error kinds without string parsing
2. Can't test error paths of `effect fn foo() -> Int` without changing the signature

## Design Goal

Two orthogonal improvements:

### 1. User-defined error types (variant as E)

Allow `Result[T, E]` where `E` is a variant type:

```almide
type AppError =
  | NotFound(String)
  | PermissionDenied(String)
  | Timeout

effect fn fetch_user(id: String) -> Result[User, AppError] = {
  let resp = http.get("/users/${id}")
  if resp.status == 404 then err(NotFound(id))
  else if resp.status == 403 then err(PermissionDenied(id))
  else ok(json.parse(resp.body)!)
}

effect fn main() -> Unit = {
  match fetch_user("123") {
    ok(user) => println(user.name),
    err(NotFound(id)) => println("User ${id} not found"),
    err(PermissionDenied(_)) => println("Access denied"),
    err(Timeout) => println("Request timed out"),
  }
}
```

No `throws` keyword. Use `-> Result[T, E]` with explicit error type. Default stays `String`.

### 2. Test block Result visibility

In test blocks, `effect fn` calls should return `Result[T, String]` without auto-unwrap:

```almide
effect fn validate(n: Int) -> Int = {
  guard n > 0 else err("bad")!
  n
}

// Ideal: works without changing validate's signature
test "ok" { assert_eq(validate(1), ok(1)) }
test "err" { assert_eq(validate(-1), err("bad")) }
```

### Current workaround

```almide
// Must declare Result explicitly to test error cases
effect fn validate(n: Int) -> Result[Int, String] = {
  guard n > 0 else return err("bad")
  ok(n)
}
```

## Design Principles

### No `throws` keyword

Other languages add `throws E` to function signatures. Almide does not:
- `throws` is another keyword LLMs must learn
- `-> Result[T, E]` is explicit and compositional
- `-> Int` with `effect fn` remains the simple default

### No error conversion chains

Rust's `From<E1> for E2` + `?` creates implicit conversion chains. Almide requires explicit mapping:

```almide
effect fn load(path: String) -> Result[Data, AppError] = {
  let text = fs.read_text(path) |> result.map_err((e) => IoFailed(e))
  match text {
    ok(t) => parse(t),
    err(e) => err(e),
  }
}
```

### Variant types as errors

Error types must be variant types or `String`. This ensures errors are always matchable and exhaustiveness checking applies.

## Implementation Plan

### Phase 1: Test block Result visibility

Make `effect fn` calls return `Result[T, String]` in test blocks instead of auto-unwrapping.

**Challenge**: Attempted in v0.11.2 but reverted. Changing checker inference caused constraint solver side effects — `ok()` / `err()` constructors and `??` / `?` operators in unrelated `assert_eq` calls picked up wrong types.

**Root cause**: The checker's constraint-based unification propagates type changes across all expressions in a block. Changing one call's return type from `Int` to `Result[Int, String]` affects the `T` in `assert_eq(T, T)`, which cascades.

**Approach**: Instead of changing checker inference, introduce a scoped mechanism:
- [ ] Add `result_of` stdlib function: `result_of(f())` captures effect fn result as `Result[T, String]` without auto-unwrap
- [ ] Or: codegen-only approach — test codegen skips `Unwrap` insertion while keeping checker types unchanged, using explicit `Result` annotation in generated code
- [ ] Or: two-pass checking — first pass infers with auto-unwrap, second pass re-checks test blocks with Result types

### Phase 2: Variant error types in Result

Allow `Result[T, MyError]` where `MyError` is a variant type:

- [ ] `err(NotFound(id))` type-checked against `Result[T, AppError]` context
- [ ] `!` propagation: only when inner and outer error types match
- [ ] `??` and `?` work unchanged with any error type
- [ ] Exhaustiveness check on `match result { ok(_) => ..., err(NotFound(_)) => ..., ... }`
- [ ] `pass_result_propagation.rs`: use declared error type instead of hardcoded `Ty::String`

### Phase 3: Result combinators

- [ ] `result.map_err(f)` — transform error type
- [ ] Exhaustiveness on nested `err(Variant)` patterns in match

## What We Are NOT Adding

| Feature | Reason |
|---|---|
| `throws` keyword | `-> Result[T, E]` is explicit and sufficient |
| `From` trait / auto error conversion | LLMs get conversion chains wrong |
| Error trait / protocol | Variant types + `String` cover all cases |
| `try` / `catch` | Almide uses `match` on `Result` |

## Files to Modify

- `crates/almide-frontend/src/check/infer.rs` — test context Result inference
- `crates/almide-codegen/src/pass_result_propagation.rs` — test Unwrap control
- `crates/almide-frontend/src/check/calls.rs` — variant error type validation
- `crates/almide-ir/src/lib.rs` — error type in IrFunction (for Phase 2)
- `spec/lang/error_types_test.almd` — E2E tests
