<!-- description: Syntax sugar (ranges, exhaustiveness check, lambda shorthand) -->
<!-- done: 2026-03-15 -->
# Syntax Sugar

## Range Literals ✅

```almide
let xs = 0..10        // [0, 1, 2, ..., 9]
let ys = 0..=10       // [0, 1, 2, ..., 10]
let zs = 10..0..-1    // [10, 9, 8, ..., 1]
```

## Exhaustiveness Checking for Pattern Match ✅

Detects at compile time when a match on a variant type does not cover all cases. Implemented in `src/check/mod.rs`.

---

## Lambda Syntax: Deprecate `fn`, unify to parentheses style ✅

Deprecated `fn(params) => expr` and unified to `(params) => expr`.
- Removed `fn(` lambda path from parser
- Batch-replaced all `.almd` files (44 files)
- Updated Rust tests, documentation, and hint messages
- `fn` keyword is now exclusively for function declarations (`fn name(...) -> Type = ...`)

---

## Default Arguments ✅

Implemented via call-site expansion. Default values inserted into IR for missing arguments during lowering. No codegen changes needed.

```almide
fn greet(name: String, greeting: String = "Hello") -> String =
  "${greeting}, ${name}!"

greet("Alice")              // → greet("Alice", "Hello") at IR level
greet("Alice", "Hi")        // → greet("Alice", "Hi")
```

- Parser: store `name: Type = expr` in Param.default
- Checker: allow argument count via fn_min_params
- Lowerer: expand defaults at call-site (target-independent)

---

## List Comprehensions — Won't Do

Violates Canonicity. `xs |> list.filter(...) |> list.map(...)` achieves the same thing.

## Named Arguments

```almide
// Optional, only usable after positional args
create_user("Alice", admin: true)
http.response(status: 200, body: "OK")

// Positional args only is also OK
create_user("Alice", 30, false)
```

Inspired by Swift but without external/internal name separation. Almide prioritizes Vocabulary Economy.

## Raw String Literals ✅

```almide
let regex_pattern = r"^\d{3}-\d{4}$"
let path = r"C:\Users\test"
```

## Block Comments ✅

```almide
/* inline comment */
/* nestable /* nested */ comment */
```

---

## Priority

Range ✅ > Exhaustiveness ✅ > Lambda ✅ > Default args ✅ > Block comments ✅ > Raw strings ✅ > ~~List comprehensions~~ (Won't do) > **Named arguments**
