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

Use [the generated stdlib reference](https://github.com/almide/almide/blob/develop/docs/stdlib/README.md) for current signatures
and [CHEATSHEET](https://github.com/almide/almide/blob/develop/docs/CHEATSHEET.md) for import and calling examples. Core modules
are auto-imported as module names, not as every unqualified function name.
Platform/opt-in modules require explicit imports. The former `error` module is retired. `io.read_all()` remains available and
returns `String`; see the IO reference for its effect-context calling syntax and target behavior.

This selected name index is checked against the stdlib sources by CI; the
generated reference remains authoritative for signatures and availability.

```
AUTO-IMPORTED (no import needed):
  string: len trim split lines contains starts_with ends_with replace
          index_of slice to_upper to_lower to_list pad_start pad_end
  list:   len get first last map flat_map filter find fold enumerate
          zip sort sort_by reverse any all take drop unique join sum
  map:    new get set contains remove keys values to_list from_list merge len
  set:    new insert contains remove union intersection len
  int:    to_string parse parse_radix abs min max clamp pow to_hex from from_checked from_saturating
  float:  to_string from abs min max pow round floor ceil
  math:   sqrt log sin cos pi e
  value:  get get_string get_int get_bool get_float get_array get_path as_object
  datetime, value, option, result, bytes — also auto-imported

IMPORT REQUIRED (effectful / platform / opt-in):
  import fs      — read_text write_text read_lines append_text exists mkdir_p remove list_dir stat temp_dir
  import env     — get set args cwd os sleep_ms
  import process — exec output run
  import io      — read_line print read_all
  import json    — parse stringify stringify_pretty
  import random  — int float shuffle
  import regex   — is_match find find_all replace split captures
  import path    — join dirname basename extension stem normalize is_absolute
  import args    — has_flag option positional
```
