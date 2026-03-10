# Tuple & Record Improvements

### Named Record Construction ✅ Implemented

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

### Tuple Index Access ✅ Implemented

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
