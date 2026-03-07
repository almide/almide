# Selmite Quick Reference (for AI code generation)

File extension: `.selm`

## File structure
```
module <path.separated.by.dots>
import <module>
import <module>.{Name1, Name2}
// declarations...
```

## Types
```
type Name = { field: Type, ... }                     // record
type Name = | Case1(Type) | Case2 | Case3{f: Type}  // variant (leading |)
type Name[A, B] = { first: A, second: B }            // generic (use [] not <>)
type Name = newtype Type                              // newtype (zero-cost wrapper)
type Name = Case1(Type) | Case2(Type)                // inline variant (no leading |)
```

### deriving
```
type ConfigError =
  | Io(IoError)
  | Parse(ParseError)
  deriving From            // auto-generates impl From for each case
```

### Built-in types
- Primitives: `Int`, `Float`, `String`, `Bool`, `Unit`, `Path`
- Collections: `List[T]`, `Map[K, V]`, `Set[T]`
- Error: `Result[T, E]` (`ok(v)` / `err(e)`), `Option[T]` (`some(v)` / `none`)

## Functions
```
fn name(x: Type, y: Type) -> RetType = expr
effect fn name(x: Type) -> Result[T, E] = expr       // has side effects
async fn name(x: Type) -> Result[T, E] = expr        // async (implies effect)
async effect fn name(x: Type) -> Result[T, E] = expr // explicit async+effect
```

### Modifiers (order matters): `pub? async? effect? fn`

### Predicate: `fn empty?(xs: List[T]) -> Bool` (? suffix = Bool return only)

### Hole / Todo
```
fn parse(text: String) -> Ast = _                     // hole (type-checked stub)
fn optimize(ast: Ast) -> Ast = todo("implement later") // todo with message
```

## Trait & Impl
```
trait Iterable[T] {
  fn map[U](self, f: fn(T) -> U) -> List[U]
  fn filter(self, f: fn(T) -> Bool) -> List[T]
}

impl From[IoError] for ConfigError {
  fn from(e: IoError) -> ConfigError = Io(e)
}
```

## Expressions

### If (MUST have else — no standalone `if`)
```
if cond then expr else expr
if a then x else if b then y else z
```
**`if` without `else` is a syntax error.** Use `guard` for early return instead.

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
**`_` can ONLY appear in match patterns.** `let _ = x` is a syntax error.

### Lambda
```
fn(x) => expr
fn(x, y) => expr
items.map(fn(x) => x + 1)
```

### Block (last expression is the value)
```
{
  let x = 1
  let y = 2
  x + y
}
```

### Do block (loop + auto-propagation)
```
// As loop: use guard to break
do {
  guard current != "NONE" else ok(())   // break condition
  let data = fs.read_text(path)
  current = next
}

// As error propagation block:
do {
  let text = fs.read_text(path)    // auto try
  let raw = json.parse(text)       // auto try
  decode(raw)                       // last expr is the result
}
```
**There is no `loop` or `while` keyword.** Use `do { ... }` with `guard ... else` for loops.

### Pipe
```
text |> string.trim |> string.split(",")
xs |> filter(_, fn(x) => x > 0)      // _ = placeholder for piped value
```

### Named arguments
```
create_user(name: "alice", age: 30)
create_user("alice", age: 30)          // mixed positional + named
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
```

### String interpolation
```
"hello ${name}, result=${1 + 1}"
```

## Statements

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

### Guard (early return / loop break)
```
guard x > 0 else err("must be positive")
guard fs.exists?(path) else err(NotFound(path))

// with block body:
guard not fs.exists?(path) else {
  println("already exists")
  ok(())
}
```
In `do { }` loops, `guard cond else ok(())` acts as a break condition.

### Try / Await
```
let text = try fs.read_text(path)   // unwrap Result, propagate error
let data = await fetch(url)          // unwrap async, must be in async fn
```

## Async
```
async fn fetch(url: String) -> Result[String, HttpError] = _
async fn load(url: String) -> Result[Config, AppError] =
  do {
    let text = await fetch(url)
    parse(text)
  }
```

### Structured concurrency
```
await parallel(tasks)      // all must succeed
await race(tasks)          // first to complete
await timeout(duration, task) // with timeout
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
effect fn main(args: List[String]) -> Result[Unit, AppError] = {
  // args[0] = program name, args[1] = first argument
  let cmd = list.get(args, 1)    // returns Option[String]
  match cmd {
    some("run") => do_something(),
    some(other) => err(UnknownCommand(other)),
    none => err(NoCommand),
  }
}
```
The runtime calls `main(args)` where `args` includes the program name at index 0.

## Operators (precedence high→low)
`. ()` > `not -` > `* / % ^` > `+ - ++` > `== != < > <= >=` > `and` > `or` > `|>`

`^` is XOR (integer), `++` is list concatenation.

## UFCS
`f(x, y)` ≡ `x.f(y)` — compiler resolves automatically.

## Standard library modules

### fs (filesystem)
`fs.read_text(path)`, `fs.read_bytes(path)`, `fs.write(path, content)`, `fs.write_bytes(path, bytes)`, `fs.append(path, content)`, `fs.mkdir_p(path)`, `fs.exists?(path)` → Bool (no `try` needed)

### string
`string.trim(s)`, `string.split(s, sep)`, `string.join(list, sep)`, `string.len(s)`, `string.pad_left(s, n, ch)`, `string.starts_with?(s, prefix)`, `string.slice(s, start)`, `string.slice(s, start, end)`, `string.to_bytes(s)`, `string.contains(s, sub)`

### list
`list.len(xs)`, `list.get(xs, i)` → `Option[T]`, `list.sort(xs)`, `list.contains(xs, x)`, `list.each(xs, f)`, `list.map(xs, f)`, `list.filter(xs, f)`, `list.find(xs, f)`, `list.fold(xs, init, f)`

### int
`int.to_hex(n)` — takes 64-bit Int, returns hex String

### env
`env.unix_timestamp()` → Int, `env.args()` → `List[String]`

## Key rules
- Newline = statement separator (no semicolons needed)
- `[]` for generics, NOT `<>`
- `<` `>` are always comparison operators
- `effect fn` for side effects, NOT `fn name!()`
- `?` suffix is for Bool predicates only
- No exceptions — use `Result[T, E]` everywhere
- No null — use `Option[T]`
- No inheritance — use trait + impl
- No macros, no operator overloading, no implicit conversions
- Empty list = `[]` (no `list.new()` or `list.empty()`)
- `_` is ONLY for match wildcard patterns, never as a variable name
- The stdlib functions listed above are exhaustive — no other functions exist

## Complete example
```
module app

import fs
import env
import string
import list

type AppError =
  | NotFound(String)
  | Io(IoError)
  deriving From

effect fn greet(name: String) -> Result[Unit, AppError] = {
  guard string.len(name) > 0 else err(NotFound("empty name"))
  println("Hello, ${name}!")
  ok(())
}

effect fn process_all(items: List[String]) -> Result[Unit, AppError] = {
  var remaining = list.len(items)
  do {
    guard remaining > 0 else ok(())
    let idx = list.len(items) - remaining
    let item = match list.get(items, idx) {
      some(v) => v,
      none => "",
    }
    println("Processing: ${item}")
    remaining = remaining - 1
  }
}

effect fn main(args: List[String]) -> Result[Unit, AppError] = {
  let cmd = list.get(args, 1)
  match cmd {
    some("greet") => {
      let name = match list.get(args, 2) {
        some(n) => n,
        none => "world",
      }
      greet(name)
    },
    some(other) => {
      println("Unknown: ${other}")
      ok(())
    },
    none => {
      println("Usage: app <command>")
      ok(())
    },
  }
}

test "greet succeeds" {
  assert_eq(string.len("hello"), 5)
}
```
