<!-- description: test where syntax — unified value/type/mock/table-driven test context -->
<!-- done: 2026-05-23 -->
# Test Where Syntax

> **Target: v0.23.0**
> **Status: Done**

## Problem

`effect fn` (network, IO, DB) cannot be unit tested. All mc-bot, mc-auth functions are untestable because they require live connections. Other languages solve this with mock/stub/DI — Almide should solve it as a language feature, not a library pattern.

## Solution

`test where` — unified context binding for tests. No `mock`/`stub`/`fake` keywords. The `where` keyword means "context that makes the body work."

### Syntax Elements

```
where   — test context binding
=       — value/type/impl/reference binding
=>      — call pattern response definition
```

### 1. Value Binding

```almide
test "parse age"
  where input = "20"
  where want = ok(20)
{
  assert_eq(parse_age(input), want)
}
```

### 2. Reference Override

```almide
test "checkout succeeds"
  where clock.now = fixed_now
  where payment.charge = charge_ok
{
  assert(checkout(cart) |> result.is_ok)
}
```

### 3. Call Pattern Response (`=>`)

```almide
test "load user"
  where http.get(path) => match path {
    "/users/abc" => ok("{\"name\":\"Steve\"}")
    _ => err("not found")
  }
{
  let user = load_user("abc")!
  assert_eq(user.name, "Steve")
}
```

### 4. Table-Driven Tests

```almide
test "parse age"
  where "valid" { input = "20"; want = ok(20) }
  where "empty" { input = ""; want = err("empty") }
  where "bad"   { input = "abc"; want = err("invalid") }
{
  assert_eq(parse_age(input), want)
}
```

### 5. Per-Case Call Responses

```almide
test "load user"
  where "found" {
    id = "abc"
    http.get(path) => match path {
      "/users/abc" => ok(steve_json)
      _ => err("not found")
    }
    want = ok(User { name: "Steve", id: "abc" })
  }
  where "missing" {
    id = "zzz"
    http.get(_) => err("not found")
    want = err("not found")
  }
{
  assert_eq(load_user(id), want)
}
```

### 6. File/Module Scope

```almide
// File-scoped: applies to all tests in this file
local test where {
  clock.now = fixed_now
  logger.emit = silent_logger
}

// Module-scoped: applies to all tests in this module
mod test where {
  db.connect = fake_db_connect
}
```

**Prohibited:**
- `test where { ... }` (bare top-level) — ambiguous scope
- `pub test where { ... }` — test context must not be exported

### 7. Type/Generic Binding

```almide
test "identity"
  where "int"    { T = Int; value = 1 }
  where "string" { T = String; value = "hello" }
{
  assert_eq(identity[T](value), value)
}
```

### 8. Scope Priority

```
case where > test inline where > local test where > mod test where > runner config > real impl
```

## `=` vs `=>` Rules

- `=` — binds a value, type, implementation, or reference
- `=>` — defines a response for a call pattern
- Same target cannot use both `=` and `=>` in same scope
- One `=>` per call target (use `match` for multiple patterns)

## Errors

```almide
// NG: bare top-level
test where { clock.now = fixed_now }

// NG: pub
pub test where { clock.now = fixed_now }

// NG: mixed = and => for same target
test "bad"
  where http.get = fake_get
  where http.get(path) => ok(...)

// NG: multiple => for same target (use match instead)
test "bad"
  where http.get("/a") => ok(a)
  where http.get("/b") => ok(b)
```

## Design Philosophy

```
where = context that makes the body work
```

This is the same `where` concept as type constraints (`where T: Ord`) — both mean "preconditions for the body to be valid." Test `where` binds values and effects; type `where` binds type constraints.

## Implementation Approach

`where` creates an environment layer stack:

```
real implementation
  + runner/root test config
  + mod test where
  + local test where
  + test inline where
  + case where
```

Name resolution searches top-down, nearest binding wins.

`where target(args) => expr` compiles to a generated replacement function that pattern-matches args and evaluates expr.

## Related

- Type definition `where` (`where T: Hash + Eq`) — separate roadmap item
- Effect system integration — `where` overrides are effect-level interception
