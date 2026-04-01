<!-- description: Allow user-defined error types in Result and effect fn -->
# Flexible Error Types

## Current State

Error type is hardcoded to `String` everywhere:

```rust
// crates/almide-codegen/src/pass_result_propagation.rs:35
func.ret_ty = Ty::result(orig, Ty::String);
```

- `effect fn` always produces `Result[T, String]`
- `!` (unwrap) propagates `String` errors
- `?` converts to `String` via `.to_string()` or display
- `err("message")` is the only way to create errors

This is simple and LLM-friendly, but limiting for real applications:
- Can't match on error kinds without string parsing
- Can't distinguish between "not found" and "permission denied" programmatically
- Error context is lost across call boundaries

## Design Goal

Allow variant types as error types while keeping the simplicity that makes LLMs accurate.

```almide
type AppError =
  | NotFound(String)
  | PermissionDenied(String)
  | Timeout

effect fn fetch_user(id: String) -> User throws AppError = {
  let resp = http.get("/users/${id}")
  if resp.status == 404 then err(NotFound(id))
  else if resp.status == 403 then err(PermissionDenied(id))
  else json.parse(resp.body)
}

effect fn main() -> Unit = {
  match fetch_user("123") {
    ok(user) => print(user.name)
    err(NotFound(id)) => print("User ${id} not found")
    err(PermissionDenied(_)) => print("Access denied")
    err(Timeout) => print("Request timed out")
  }
}
```

## Design Principles

### 1. Default stays `String`

`effect fn` without `throws` still produces `Result[T, String]`. Zero migration cost. LLMs writing simple code don't need to know about custom errors.

```almide
// These are equivalent:
effect fn foo() -> Int = { ... }
effect fn foo() -> Int throws String = { ... }
```

### 2. No error conversion chains

Rust's `From<E1> for E2` + `?` creates implicit conversion chains that LLMs get wrong constantly. Almide does NOT add automatic error conversion.

If you call an `effect fn` that throws `IoError` from within a function that throws `AppError`, you must explicitly convert:

```almide
effect fn load(path: String) -> Data throws AppError = {
  let text = match fs.read_text(path) {
    ok(t) => t
    err(e) => err(IoFailed(e))
  }
  parse(text)
}
```

Or with a helper:

```almide
effect fn load(path: String) -> Data throws AppError = {
  let text = fs.read_text(path) |> result.map_err((e) => IoFailed(e))!
  parse(text)
}
```

### 3. Variant types as errors

Error types must be variant types (or `String`). No arbitrary types. This ensures errors are always matchable and exhaustiveness checking applies.

### 4. `err()` creates errors, `!` propagates them

```almide
err(NotFound(id))         // create error value
risky_call()!             // propagate error (same type only)
risky_call() ?? default   // fallback on error
```

## Implementation Plan

### Phase 1: `throws` clause parsing and type checking

- [ ] Add `throws: Option<TypeExpr>` to `Decl::Fn` in AST
- [ ] Parser: `effect fn foo() -> T throws E = ...`
- [ ] Type checker: verify `E` is a variant type or `String`
- [ ] `err(value)` type-checked against the function's error type
- [ ] `!` propagation: only allowed when inner and outer error types match
- [ ] Default: `throws String` when omitted

### Phase 2: Codegen

- [ ] `pass_result_propagation.rs`: use declared error type instead of `Ty::String`
- [ ] Rust codegen: `Result<T, E>` where E is the generated Rust enum
- [ ] WASM codegen: error variant stored in linear memory
- [ ] `err(Variant(...))` → constructor call in generated code

### Phase 3: Result combinators with typed errors

- [ ] `result.map_err(f)` — transform error type
- [ ] `result.unwrap_or(default)` — existing, works unchanged
- [ ] `??` operator — existing, works unchanged
- [ ] Exhaustiveness check on `match result { ok(_) => ..., err(E1) => ..., err(E2) => ... }`

### Phase 4: Error context (future)

- [ ] `err(NotFound(id)).context("loading user profile")` — attach context string
- [ ] Preserves the variant type while adding human-readable context
- [ ] Low priority: simple variant payloads cover most needs

## What We Are NOT Adding

| Feature | Reason |
|---|---|
| `From` trait / auto error conversion | LLMs get conversion chains wrong. Explicit mapping is more accurate |
| Error trait / protocol | Overkill. Variant types + `String` cover all cases |
| `try` / `catch` | Almide uses `match` on `Result`. No exceptions |
| Stack traces in errors | Runtime complexity. Variant payload + context string is sufficient |
| `throws` on pure fn | Pure functions don't fail. Only `effect fn` can throw |

## Files to Modify

- `crates/almide-syntax/src/ast.rs` — add `throws` to `Decl::Fn`
- `crates/almide-syntax/src/parser/declarations.rs` — parse `throws E`
- `crates/almide-frontend/src/check/infer.rs` — type check error types
- `crates/almide-codegen/src/pass_result_propagation.rs` — use declared error type
- `crates/almide-types/src/types/mod.rs` — `FnSig` gets error type field
- `crates/almide-ir/src/lib.rs` — `IrFunction` gets error type
- `stdlib/defs/*.toml` — stdlib effect fn signatures (all use default `String`)
- `spec/lang/error_types_test.almd` — E2E tests
