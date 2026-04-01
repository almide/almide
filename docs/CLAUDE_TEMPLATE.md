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
|x| x + 1                     fn(x) => x + 1
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

// Effect function (does I/O — returns Result)
effect fn read(path: String) -> Result[String, String] = {
  let text = fs.read_text(path)
  ok(text)
}

// Main entry point (args via process.args(), Result is auto-wrapped)
effect fn main() -> Unit = {
  println("hello")
}

// If-then-else (else is MANDATORY)
let x = if n > 0 then "pos" else "neg"

// Match
let name = match list.get(args, 1) {
  some(v) => v,
  none => "default",
}

// Lambda
let doubled = list.map(xs, fn(x) => x * 2)

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
let second = xs[1]       // 20 (returns Option[T])

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

// UFCS — method syntax works on any type (auto-resolves to correct module)
let n = "hello".len()          // same as string.len("hello")
let m = [1, 2, 3].len()       // same as list.len([1, 2, 3])
let r = "hello".split(" ").reverse()  // chaining works too
```

## Stdlib Quick Reference

```
AUTO-IMPORTED (no import needed):
  string: len trim split join lines contains? starts_with? ends_with? replace
          index_of slice to_upper to_lower to_int chars pad_start pad_end
  list:   len get get_or first last map flat_map filter find fold enumerate
          zip sort sort_by reverse any? all? take drop unique join sum
  map:    new get set contains? remove keys values entries from_list merge len
  int:    to_string parse abs min max clamp to_hex parse_hex
  float:  to_string from_int abs min max round floor ceil
  fs:     read_text write read_lines append exists? mkdir_p remove list_dir
  path:   join dirname basename extension stem normalize is_absolute?
  env:    get get_or
  process: exec exec_status
  io:     read_line print read_all

IMPORT REQUIRED:
  import json    — parse stringify get get_string get_int get_array keys
  import math    — sqrt pow log sin cos pi e
  import random  — int float shuffle
  import regex   — match? find find_all replace split captures
  import args    — flag? option option_or positional positional_at
```
