# Almide Quick Reference (for AI code generation)

File extension: `.almd`

## File structure
```
import <module>
// declarations...
```

## Types
```
type Name = { field: Type, ... }                     // record
type Name = | Case1(Type) | Case2 | Case3{f: Type}  // variant (leading |)
type Name[A, B] = { first: A, second: B }            // generic (use [] not <>)
type Name = Type                                     // type alias (transparent)
type Name = Case1(Type) | Case2(Type)                // inline variant (no leading |)
type Handler = (String) -> String                    // function type alias
```

### Conventions
```
type Color: Eq, Repr = Red | Green | Blue   // convention after type name with :
```

### Built-in types
- Primitives: `Int`, `Float`, `String`, `Bool`, `Unit`, `Path`
- Collections: `List[T]`, `Map[K, V]`, `Set[T]`
- Error: `Result[T, E]` (`ok(v)` / `err(e)`), `Option[T]` (`some(v)` / `none`)

## Functions
```
fn name(x: Type, y: Type) -> RetType = expr
effect fn name(x: Type) -> Result[T, E] = expr       // has side effects
```

### Visibility (optional prefix before fn/type)
- `fn f()` — public (default)
- `mod fn f()` — same project only (`pub(crate)` in Rust)
- `local fn f()` — this file only (private)

### Modifiers (order matters): `[local|mod]? effect? fn`

### Predicate: `fn empty(xs: List[T]) -> Bool` (Bool return only)

### Hole / Todo
```
fn parse(text: String) -> Ast = _                     // hole (type-checked stub)
fn optimize(ast: Ast) -> Ast = todo("implement later") // todo with message
```

## Built-in Protocols
Eq and Hash are automatic (compiler-derived from type structure). No annotation needed.
```
// Eq: all value types support == (except Fn)
let same = color_a == color_b  // just works
```
### Protocols (user-defined conventions)
```
// Define a protocol
protocol Action {
  fn name(a: Self) -> String
  fn execute(a: Self, ctx: Context) -> Result[String, String]
}

// Satisfy via convention methods
type GreetAction: Action = { greeting: String }
fn GreetAction.name(a: GreetAction) -> String = "greet"
fn GreetAction.execute(a: GreetAction, ctx: Context) -> Result[String, String] =
  ok(a.greeting)

// Use as generic bound
fn run_action[T: Action](action: T, ctx: Context) -> Result[String, String] =
  action.execute(ctx)
```
Built-in conventions (Eq, Repr, Ord, Hash, Codec) are protocols too.

## Expressions

### If (MUST have else — no standalone `if`)
```
if cond then expr else expr
if a then x else if b then y else z
```
**`if` without `else` returns Unit.** Use `guard` for early return instead.

### Match (exhaustive, supports guards)
```
match subject {
  Pattern => expr,
  Pattern if guard_cond => expr,
  _ => expr,
}
```

### Patterns
```
_                          // wildcard (match only — NOT a valid variable name)
name                       // bind
ok(inner) / err(inner)     // Result
some(inner) / none         // Option
TypeName(args...)          // constructor
TypeName{ field1, field2 } // record pattern
literal                    // int, float, string, bool
```
**`_` can appear in match patterns, `let _ = x` (discard), `for _ in xs`, and lambda params `(_ ) => expr`.**

### Lambda
```
(x) => expr
(x, y) => expr
items.map((x) => x + 1)

// multi-line: use a block as the body
let f = (x) => {
  let y = x * 2
  y + 1
}
```

### Block (last expression is the value)
```
{
  let x = 1
  let y = 2
  x + y
}
```

### For...in loop
```
for x in xs {
  println(x)
}

for (k, v) in config {
  println(k + " = " + v)
}

for key in m {
  println(key)           // iterates keys only
}
```
### While loop
```
var i = 0
while i < 10 {
  println(int.to_string(i))
  i = i + 1
}
```

### Range
```
0..5            // [0, 1, 2, 3, 4]  (exclusive end)
1..=5           // [1, 2, 3, 4, 5]  (inclusive end)
for i in 0..n { ... }    // optimized: no list allocation
let xs = list.map(0..10, (i) => i * i)   // range as List[Int]
```

### Pipe
```
text |> string.trim |> string.split(",")
xs |> filter(_, (x) => x > 0)      // _ = placeholder for piped value
```

### Record & Spread
```
{ name: "alice", age: 30 }
{ ...base, name: "bob" }
```

### List
```
[1, 2, 3]
[]                         // empty list (there is NO list.new())
xs[0]                      // index read
xs[i] = value              // index write (var only)
```

### Map
```
["a": 1, "b": 2]          // map literal
[:]                        // empty map (requires type annotation)
let m: Map[String, Int] = [:]
m["key"]                   // index read (returns Option[V])
m["key"] = value           // index write (var only)
```

### String interpolation
```
"hello ${name}, result=${1 + 1}"
```

### Heredoc (multi-line strings)
```
let sql = """
  SELECT *
  FROM users
"""
// Leading whitespace stripped based on minimum indent
// Interpolation ${expr} works the same
// Raw heredoc: r"""...""" (no escapes)
```

## Statements

### Top-level let (module-scope constant)
```
let PI = 3.14159265358979323846
let MAX_RETRIES = 3
let GREETING = "Hello"
```
Top-level `let` is evaluated at compile time (const) or via `LazyLock` (for non-const expressions like String).

### let / var
```
let x = 1                   // immutable
let x: Int = 1              // with type annotation
var y = 2                   // mutable
y = y + 1                   // reassign (var only)
```

### Destructuring
```
let { name, age } = user    // record destructure (1 level only)
```

### Unwrap operators
```
expr!              // unwrap Result/Option, propagate err (effect fn only)
expr ?? fallback   // unwrap or use fallback value
expr?              // Result → Option (err → none)
expr?.field        // optional chaining (Option[Record] → Option[FieldType])
```

### Guard (early return / loop break)
```
guard x > 0 else err("must be positive")
guard fs.exists(path) else err(NotFound(path))

// with block body:
guard not fs.exists(path) else {
  println("already exists")
  ok(())
}
```
## Test
```
test "description" {
  assert_eq(add(1, 2), 3)
  assert(x > 0)
  assert_ne(a, b)
}
```

## Built-in functions
```
println(s)                 // print line to stdout
eprintln(s)                // print line to stderr
assert_eq(a, b)            // assert equal
assert_ne(a, b)            // assert not equal
assert(cond)               // assert true
```
**There is no `print` function.** Use `println` for all output (including error messages to user).
`eprintln` is for debug/internal errors only — user-facing messages MUST use `println`.

## Entry point
```
effect fn main() -> Unit = {
  let args = process.args()              // command-line args (Go-style)
  let name = list.get(args, 1) ?? "world"
  let content = fs.read_text("config.txt")!   // propagate error with !
  println("Hello, ${name}: ${content}")
}
```
`effect fn main()` is auto-wrapped to return `Result<(), String>`. No need to write `ok(())` or `-> Result[...]`.
Command-line arguments are accessed via `process.args()` (not main parameters).

## Operators (precedence high→low)
`. () ! ?? ?` > `not -` > `^` (power) > `* / %` > `+ -` > `== != < > <= >=` (non-assoc) > `and` > `or` > `|>` `>>`

`^` is exponentiation (right-associative, `**` also accepted). `+` is concatenation for strings and lists (overloaded with addition). XOR is available as `int.bxor(a, b)`.

## UFCS
`f(x, y)` ≡ `x.f(y)` — compiler resolves automatically.

## Standard library modules

Full function signatures: [docs/stdlib/](stdlib/)

| Module | Description | Import | # |
|---|---|---|---|
| [string](stdlib/string.md) | String manipulation | auto-imported | 44 |
| [list](stdlib/list.md) | List operations | auto-imported | 59 |
| [map](stdlib/map.md) | Map (dictionary) operations | auto-imported | 24 |
| [set](stdlib/set.md) | Set operations | auto-imported | 19 |
| [int](stdlib/int.md) | Integer arithmetic and bitwise | auto-imported | 21 |
| [float](stdlib/float.md) | Floating-point operations | auto-imported | 16 |
| [value](stdlib/value.md) | Dynamic value manipulation | auto-imported | 19 |
| [result](stdlib/result.md) | Result type operations | auto-imported | 12 |
| [option](stdlib/option.md) | Option type operations | auto-imported | 12 |
| [json](stdlib/json.md) | JSON parsing and querying | `import json` | 28 |
| [math](stdlib/math.md) | Mathematical functions | `import math` | 21 |
| [regex](stdlib/regex.md) | Regular expressions | `import regex` | 8 |
| [datetime](stdlib/datetime.md) | Date and time | `import datetime` | 21 |
| [bytes](stdlib/bytes.md) | Binary data | `import bytes` | 14 |
| [matrix](stdlib/matrix.md) | 2D matrix operations | `import matrix` | 13 |
| [testing](stdlib/testing.md) | Test assertions | `import testing` | 7 |
| [error](stdlib/error.md) | Error construction | `import error` | 3 |
| [path](stdlib/path.md) | File path manipulation | `import path` | 5 |
| [args](stdlib/args.md) | CLI argument parsing | `import args` | 4 |
| [fs](stdlib/fs.md) | File system | `import fs` | 24 |
| [env](stdlib/env.md) | Environment and system | `import env` | 9 |
| [process](stdlib/process.md) | Process execution | `import process` | 6 |
| [io](stdlib/io.md) | Standard I/O | `import io` | 5 |
| [http](stdlib/http.md) | HTTP client and server | `import http` | 20 |
| [random](stdlib/random.md) | Random number generation | `import random` | 4 |
## Key rules
- Newline = statement separator (no semicolons needed)
- `[]` for generics, NOT `<>`
- `<` `>` are always comparison operators
- `effect fn` for side effects, NOT `fn name!()`
- Predicate functions return `Bool` (no special suffix)
- No exceptions — use `Result[T, E]` everywhere
- No null — use `Option[T]`
- No inheritance — use composition
- No macros, no operator overloading, no implicit conversions
- Empty list = `[]`, empty map = `[:]` (with type annotation)
- `_` is ONLY for match wildcard patterns, never as a variable name
- The stdlib functions listed above are exhaustive — no other functions exist
- Use `for x in xs { ... }` for iteration

## Common mistakes (DO NOT)
- `list[1, 2, 3]` → **WRONG**. Write `[1, 2, 3]`. `list` is a module, not a type constructor
- `each(xs, f)` → **WRONG**. Write `list.each(xs, f)`. All stdlib functions need module prefix
- `map[K, V]` as a value → **WRONG**. Write `[:]` with type annotation to create an empty map
- `List.new()` → **WRONG**. Write `[]`. There is no `new()` for List
- `{"a": 1}` as a map → **WRONG**. Write `["a": 1]`. Braces `{}` are for records/blocks, brackets `[]` for lists and maps
- `string.length(s)` → **WRONG**. Write `string.len(s)`. No synonyms
- `println(x)` where x is Int → **WRONG**. Write `println(int.to_string(x))`. No implicit conversion
- `1 :: 2 :: []` → **WRONG**. Write `[1, 2]`. There is no cons operator `::`
- `fn foo<T>(x: T)` → **WRONG**. Write `fn foo[T](x: T)`. Use `[]` for generics, not `<>`

## Complete example
```
import fs

type AppError =
  | NotFound(String)
  | Io(String)

effect fn greet(name: String) -> Result[Unit, AppError] = {
  guard string.len(name) > 0 else err(NotFound("empty name"))
  println("Hello, ${name}!")
  ok(())
}

effect fn main() -> Result[Unit, AppError] = {
  let args = process.args()
  let cmd = list.get(args, 1) ?? "help"
  match cmd {
    "greet" => {
      let name = list.get(args, 2) ?? "world"
      greet(name)
    },
    "read" => {
      let path = list.get(args, 2) ?? "input.txt"
      let content = fs.read_text(path).map_err((e) => Io(e))!
      println(content)
      ok(())
    },
    other => {
      println("Usage: app <greet|read> [arg]")
      ok(())
    },
  }
}

test "greet succeeds" {
  assert_eq(string.len("hello"), 5)
}
```
