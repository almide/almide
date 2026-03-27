# Unwrap operators: `!` `??` `?`

Three postfix operators that unify Result and Option handling. Replaces auto-`?` insertion, `From` convention, and most explicit match/guard patterns for error handling.

## Motivation

### Problems with current design

1. **auto-`?` is invisible** — `effect fn` silently inserts `?` on Result-returning calls. Violates Surface Semantics: the user can't see where errors propagate.

2. **`From` convention is opaque** — `type AppError: From = ...` generates implicit type conversions. Users (and LLMs) can't predict when conversion happens.

3. **Result/Option nesting hell** — `Option[Result[T, E]]` or `Result[Option[T], E]` requires verbose match/guard patterns. No composable unwrapping.

4. **Option has no propagation** — `?` only works for Result. Option requires manual `guard` + `match`.

### Design principle

All three problems share a root cause: **implicit unwrapping**. The fix is **explicit unwrapping with clear operators**.

## Specification

### `expr!` — must succeed (error propagation)

Unwraps the value. On failure, returns `err(...)` from the enclosing `effect fn`.

```
// On Result[T, E]: ok(v) → v, err(e) → return err(e)
// On Option[T]:    some(v) → v, none → return err("none")
```

```
effect fn load(path: String) -> Result[Config, String] = {
  let text = fs.read_text(path)!      // err → propagates
  let config = json.parse(text)!      // err → propagates
  ok(config)
}

effect fn find_user(id: Int) -> Result[User, String] = {
  let user = map.get(users, id)!      // none → err("none")
  ok(user)
}
```

Only valid inside `effect fn`. Compile error elsewhere.

### `expr ?? fallback` — or else (default value)

Unwraps the value. On failure, evaluates to the fallback expression.

```
// On Result[T, E]: ok(v) → v, err(_) → fallback
// On Option[T]:    some(v) → v, none → fallback
```

```
let port = int.parse(input) ?? 8080
let name = map.get(config, "name") ?? "anonymous"
let text = fs.read_text(path) ?? ""
```

Valid anywhere. Pure expression — no effect fn requirement.

### `expr?` — try (Option downgrade)

Unwraps the value. On failure, evaluates to `none`. Converts Result to Option.

```
// On Result[T, E]: ok(v) → some(v), err(_) → none
// On Option[T]:    some(v) → some(v), none → none (identity)
```

```
fn try_parse(s: String) -> Option[Int] =
  int.parse(s)?           // err → none

fn lookup(config: Map[String, String], key: String) -> Option[Int] =
  int.parse(map.get(config, key)?)?
  // map.get: Option → ? unwraps or returns none
  // int.parse: Result → ? converts err to none
```

Valid anywhere. Returns Option.

## Summary table

| Operator | On success | On failure | Context | Returns |
|----------|-----------|------------|---------|---------|
| `expr!` | unwrap | propagate err | `effect fn` only | `T` |
| `expr ?? val` | unwrap | use fallback | anywhere | `T` |
| `expr?` | unwrap → some | `none` | anywhere | `Option[T]` |

Strength: `!` (strict) > `??` (flexible) > `?` (lenient)

## What this replaces

| Current | New |
|---------|-----|
| auto-`?` insertion in `effect fn` | Explicit `!` operator |
| `From` convention on error types | Explicit `result.map_err(...)!` or `??` |
| `guard opt != none else err(...)` | `opt!` |
| `result.unwrap_or(expr, default)` | `expr ?? default` |
| `option.unwrap_or(expr, default)` | `expr ?? default` |
| `option.to_result(expr, "msg")` | `expr!` |
| `match x { some(v) => v, none => ... }` | `x ?? ...` or `x!` |

## Nesting solved

```
// Before: nested hell
let port: Option[Result[Int, String]] =
  option.map(map.get(config, "port"), (s) => int.parse(s))
let value = match port {
  some(ok(n)) => n,
  some(err(_)) => 8080,
  none => 8080,
}

// After: composable operators
let value = int.parse(map.get(config, "port") ?? "8080") ?? 8080
```

## Breaking changes

1. **`From` convention removed** — `type Foo: From = ...` no longer valid. Replace `: From` with explicit `map_err` + `!`.

2. **auto-`?` removed** — `effect fn` no longer silently inserts `?`. All Result-returning calls must use `!`, `??`, or `?` explicitly.

3. **`!` as prefix** — Currently `!expr` gives error "use `not`". Postfix `expr!` is a new token; no conflict.

## Migration

```
// Before (auto-?)
effect fn process() -> Result[String, String] = {
  let text = fs.read_text("data.txt")
  ok(text)
}

// After (explicit !)
effect fn process() -> Result[String, String] = {
  let text = fs.read_text("data.txt")!
  ok(text)
}
```

Automated migration: insert `!` after every Result-returning call inside `effect fn` that was previously auto-wrapped.

## Implementation plan

1. Add `!`, `??`, `?` as postfix operators to lexer + parser
2. Lower to existing IR nodes (Try, UnwrapOr, ToOption)
3. Remove `pass_result_propagation.rs` auto-insertion
4. Remove `From` convention from `lower/derive.rs`
5. Add migration lint: warn on bare Result calls in `effect fn` without operator
6. Update all spec/test files
7. Update docs, CHEATSHEET, SPEC

## Parser changes

```
// In parse_postfix(), after call/index/member:
if self.check(TokenType::Bang) && !self.newline_before_current() {
    // expr!
}
if self.check(TokenType::Question) && !self.newline_before_current() {
    // expr?  (distinct from ident? which is IdentQ token)
}
if self.check(TokenType::QuestionQuestion) {
    // expr ?? fallback
}
```

`?` on expressions vs `?` on identifiers: the lexer already distinguishes `IdentQ` (identifier with trailing `?`) from a standalone `?` token. Postfix `expr?` appears after a `)`, `]`, identifier, or literal — never after whitespace before an identifier.
