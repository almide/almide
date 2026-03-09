# Tuple & Record Improvements

## Named Record Construction ✅ Implemented

```almide
type Point = {x: Int, y: Int}

let p = Point {x: 1, y: 2}   // named construction
let q = {x: 3, y: 4}         // anonymous (still works)
```

- [x] Parser: `TypeName {field: value, ...}` → `Expr::Record { name: Some("TypeName"), ... }`
- [x] AST: `Expr::Record` has `name: Option<String>`
- [x] Rust emitter: `Point { x: 1i64, y: 2i64 }`
- [x] TS emitter: name ignored (plain JS object)
- [x] Formatter: preserves name in output

## Tuple Index Access ✅ Implemented

```almide
let t = (1, "hello")
let x = t.0     // → 1
let s = t.1     // → "hello"
```

- [x] Parser: integer literal after `.` → `Expr::TupleIndex`
- [x] AST: `Expr::TupleIndex { object, index }`
- [x] Checker: validate index within tuple bounds, return element type
- [x] Rust emitter: `(expr).0`
- [x] TS emitter: `(expr)[0]`
- [x] Formatter: preserves `t.0` syntax

---

*Content from existing type-system.md:*

## Tuple Types

Records require names, which can be verbose.

```almide
// proposed
let pair: (Int, String) = (42, "hello")
let (a, b) = pair
```

## Structured Error Types

Currently Result[T, String] uses a fixed String error type, making it hard to distinguish error kinds.

```almide
// proposed
type AppError = NotFound(String) | Unauthorized | Internal(String)
type AppResult[T] = Result[T, AppError]
```

Enables branching by error type in match arms.

## Type Aliases

```almide
type UserId = Int
type Config = Map[String, String]
```

Newtype exists currently but is limited in scope.
