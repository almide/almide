# Almide (.almd)

## STOP — Read These Rules First

Almide is NOT Rust/TypeScript/Python. These are the most common mistakes:

```
WRONG                          RIGHT
─────────────────────────────  ──────────────────────────────
a && b                         a and b
a || b                         a or b
!x                             not x
if cond { ... }                if cond then expr else expr
|x| x + 1                     (x) => x + 1
let mut x = 0                  var x = 0
return expr                    (last expression IS the return)
fn foo() -> Int { ... }        fn foo() -> Int = { ... }
import { json }                import json
struct Foo { x: Int }          type Foo = { x: Int }
x.len()                        WORKS! (UFCS — auto-resolves by type)
try { } catch(e) { }          match result { Ok(v) => ..., Err(e) => ... }
while cond { ... }             while cond { ... }  (SAME — while IS supported)
```

## Quick Patterns

```almide
// Function
fn add(a: Int, b: Int) -> Int = a + b

// Effect function (does I/O — returns Result; add `import fs` to the file)
effect fn read(path: String) -> Result[String, String] = {
  let text = fs.read_text(path)!
  ok(text)
}

// Main entry point (args via env.args(), Result is auto-wrapped)
effect fn main() -> Unit = {
  println("hello")
}

// If-then-else (omitting else gives Unit)
let x = if n > 0 then "pos" else "neg"

// Match
let name = match list.get(args, 1) {
  some(v) => v,
  none => "default",
}

// Lambda — params in parens, NO `fn` keyword (single or multi-arg)
let doubled = list.map(xs, (x) => x * 2)
let total = list.fold(xs, 0, (acc, x) => acc + x)

// For loop
for item in items {
  println(item)
}
for (i, item) in list.enumerate(items) {
  println(int.to_string(i) + ": " + item)
}

// While loop
var i = 0
while i < 10 {
  println(int.to_string(i))
  i = i + 1
}

// Top-level constant
let PI = 3.14159265358979323846

// Mutable state
var count = 0
count = count + 1

// List index read
let xs = [10, 20, 30]
let second = xs[1]       // 20 (direct indexing returns T; use list.get for Option[T])

// Guard (early exit)
guard list.len(args) > 1 else err("need args")

// String interpolation
let msg = "Hello ${name}, you are ${int.to_string(age)} years old"

// Record type
type Point = { x: Int, y: Int }
let p = Point { x: 1, y: 2 }

// Variant type
type Shape =
  | Circle(Float)
  | Rect(Float, Float)

// List/String concat
let combined = [1, 2] + [3, 4]
let greeting = "hello" + " " + "world"

// UFCS — the checker resolves by receiver type and scope, not an unconditional rewrite
let n = "hello".len()          // same as string.len("hello")
let m = [1, 2, 3].len()       // same as list.len([1, 2, 3])
let r = "hello".split(" ").reverse()  // chaining works too
```

## Stdlib Quick Reference

Use [the generated stdlib reference](../stdlib/README.md) for current signatures
and [CHEATSHEET](../CHEATSHEET.md) for import and calling examples. Core modules
are auto-imported as module names, not as every unqualified function name.
Platform/opt-in modules require explicit imports. The former `error` module is retired. `io.read_all()` remains available and
returns `String`; see the IO reference for its effect-context calling syntax and target behavior.
