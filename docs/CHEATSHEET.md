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

### Predicate: `fn empty?(xs: List[T]) -> Bool` (? suffix = Bool return only)

### Hole / Todo
```
fn parse(text: String) -> Ast = _                     // hole (type-checked stub)
fn optimize(ast: Ast) -> Ast = todo("implement later") // todo with message
```

## Built-in Protocols
Eq and Hash are automatic (compiler-derived from type structure). No annotation needed.
`From` is the only explicit convention (for error type conversions), specified with `:` after the type name.
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
`. ()` > `not -` > `^` (power) > `* / %` > `+ -` > `== != < > <= >=` (non-assoc) > `and` > `or` > `|>` `>>`

`^` is exponentiation (right-associative, `**` also accepted). `+` is concatenation for strings and lists (overloaded with addition). XOR is available as `int.bxor(a, b)`.

## UFCS
`f(x, y)` ≡ `x.f(y)` — compiler resolves automatically.

## Standard library modules

### string (auto-imported)
`string.trim(s)`, `string.trim_start(s)`, `string.trim_end(s)`, `string.split(s, sep)`, `string.join(list, sep)`, `string.len(s)`, `string.lines(s)`, `string.pad_left(s, n, ch)`, `string.pad_right(s, n, ch)`, `string.starts_with?(s, prefix)`, `string.ends_with?(s, suffix)`, `string.slice(s, start)`, `string.slice(s, start, end)`, `string.to_bytes(s)`, `string.from_bytes(bytes)`, `string.contains(s, sub)`, `string.to_upper(s)`, `string.to_lower(s)`, `string.to_int(s)` → `Result[Int, String]`, `string.replace(s, from, to)`, `string.char_at(s, i)` → `Option[String]`, `string.chars(s)` → `List[String]`, `string.index_of(s, needle)` → `Option[Int]`, `string.repeat(s, n)`, `string.count(s, sub)` → `Int`, `string.reverse(s)`, `string.is_empty?(s)` → `Bool`, `string.is_digit?(s)`, `string.is_alpha?(s)`, `string.is_alphanumeric?(s)`, `string.is_whitespace?(s)`, `string.strip_prefix(s, prefix)` → `Option[String]`, `string.strip_suffix(s, suffix)` → `Option[String]`

### list (auto-imported)
`list.len(xs)`, `list.get(xs, i)` → `Option[T]`, `list.get_or(xs, i, default)` → `T`, `list.first(xs)` → `Option[T]`, `list.last(xs)` → `Option[T]`, `list.sort(xs)`, `list.sort_by(xs, (x) => key)`, `list.reverse(xs)`, `list.contains(xs, x)`, `list.index_of(xs, x)` → `Option[Int]`, `list.any(xs, (x) => bool)`, `list.all(xs, (x) => bool)`, `list.each(xs, f)`, `list.map(xs, f)`, `list.flat_map(xs, f)`, `list.filter(xs, f)`, `list.filter_map(xs, (x) => Option[B])` → `List[B]`, `list.find(xs, f)`, `list.fold(xs, init, f)`, `list.enumerate(xs)` → `List[(Int, T)]`, `list.zip(a, b)` → `List[(T, U)]`, `list.flatten(xss)`, `list.take(xs, n)`, `list.drop(xs, n)`, `list.chunk(xs, n)` → `List[List[T]]`, `list.unique(xs)`, `list.repeat(val, n)`, `list.join(xs, sep)` → `String`, `list.sum(xs)` → `Int`, `list.product(xs)` → `Int`, `list.min(xs)` → `Option[T]`, `list.max(xs)` → `Option[T]`, `list.is_empty?(xs)` → `Bool`, `list.push(xs, x)` (var), `list.pop(xs)` → `Option[T]` (var), `list.clear(xs)` (var)

### map (auto-imported)
`map.new()` → empty `Map[K, V]`, `map.get(m, key)` → `Option[V]`, `map.get_or(m, key, default)` → `V`, `map.set(m, key, value)` → `Map[K, V]`, `map.contains(m, key)` → `Bool`, `map.remove(m, key)` → `Map[K, V]`, `map.merge(a, b)` → `Map[K, V]`, `map.keys(m)` → `List[K]` (sorted), `map.values(m)` → `List[V]`, `map.len(m)` → `Int`, `map.entries(m)` → `List[(K, V)]`, `map.from_list(xs, (x) => (k, v))` → `Map[K, V]`, `map.is_empty?(m)` → `Bool`, `map.insert(m, key, value)` (var), `map.delete(m, key)` (var), `map.clear(m)` (var)

### int / float (auto-imported)
`int.to_string(n)`, `int.to_hex(n)`, `int.parse(s)` → `Result[Int, String]`, `int.parse_hex(s)` → `Result[Int, String]`, `int.abs(n)`, `int.min(a, b)`, `int.max(a, b)`, `int.band(a, b)`, `int.bor(a, b)`, `int.bxor(a, b)`, `int.bshl(a, n)`, `int.bshr(a, n)`, `int.bnot(a)`, `int.wrap_add(a, b, bits)`, `int.wrap_mul(a, b, bits)`, `int.rotate_right(a, n, bits)`, `int.rotate_left(a, n, bits)`, `int.to_u32(a)`, `int.to_u8(a)`
`float.to_string(n)`, `float.to_int(n)`, `float.from_int(n)`, `float.round(n)`, `float.floor(n)`, `float.ceil(n)`, `float.abs(n)`, `float.sqrt(n)`, `float.parse(s)` → `Result[Float, String]`

### fs (requires `import fs`, effect fns)
`fs.read_text(path)`, `fs.read_bytes(path)`, `fs.read_lines(path)`, `fs.write(path, content)`, `fs.write_bytes(path, bytes)`, `fs.append(path, content)`, `fs.mkdir_p(path)`, `fs.exists?(path)` → `Bool`, `fs.is_dir?(path)` → `Bool`, `fs.is_file?(path)` → `Bool`, `fs.remove(path)`, `fs.list_dir(path)`, `fs.copy(src, dst)`, `fs.rename(src, dst)`

### path (requires `import path`)
`path.join(base, child)`, `path.dirname(p)`, `path.basename(p)`, `path.extension(p)` → `Option[String]`, `path.is_absolute?(p)` → `Bool`

### env (requires `import env`, effect fns)
`env.unix_timestamp()` → `Int`, `env.millis()` → `Int`, `env.args()` → `List[String]`, `env.get(name)` → `Option[String]`, `env.set(name, value)`, `env.cwd()` → `Result[String, String]`, `env.sleep_ms(ms)`

### process (requires `import process`, effect fns)
`process.exec(cmd, args)` → `Result[String, String]`, `process.exec_status(cmd, args)` → `Result[{code: Int, stdout: String, stderr: String}, String]`, `process.exit(code)`, `process.stdin_lines()` → `Result[List[String], String]`

### io (requires `import io`, effect fns)
`io.read_line()` → `String`, `io.print(s)` (no newline), `io.read_all()` → `String`

### json (requires `import json`)
`json.parse(text)` → `Result[Json, String]`, `json.stringify(j)`, `json.stringify_pretty(j)`, `json.get(j, key)` → `Option[Json]`, `json.get_string(j, key)` → `Option[String]`, `json.get_int(j, key)` → `Option[Int]`, `json.get_float(j, key)` → `Option[Float]`, `json.get_bool(j, key)` → `Option[Bool]`, `json.get_array(j, key)` → `Option[List[Json]]`, `json.keys(j)` → `List[String]`, `json.to_string(j)` → `Option[String]`, `json.to_int(j)` → `Option[Int]`, `json.as_string(j)` → `Option[String]`, `json.as_int(j)` → `Option[Int]`, `json.as_float(j)` → `Option[Float]`, `json.as_bool(j)` → `Option[Bool]`, `json.as_array(j)` → `Option[List[Json]]`, `json.object(entries)` → `Json`, `json.s(v)`, `json.i(v)`, `json.f(v)`, `json.b(v)`, `json.null()`, `json.array(items)`, `json.from_string(s)`, `json.from_int(n)`, `json.from_float(n)`, `json.from_bool(b)`, `json.from_map(m)`

### math (requires `import math`)
`math.min(a, b)`, `math.max(a, b)`, `math.abs(n)`, `math.pow(base, exp)`, `math.pi()`, `math.e()`, `math.sin(x)`, `math.cos(x)`, `math.tan(x)`, `math.log(x)`, `math.exp(x)`, `math.sqrt(x)`

### random (requires `import random`, effect fns)
`random.int(min, max)` (inclusive), `random.float()` (0.0..1.0), `random.choice(xs)` → `Option[T]`, `random.shuffle(xs)`

### regex (requires `import regex`)
`regex.match?(pat, s)`, `regex.full_match?(pat, s)`, `regex.find(pat, s)` → `Option[String]`, `regex.find_all(pat, s)`, `regex.replace(pat, s, rep)`, `regex.replace_first(pat, s, rep)`, `regex.split(pat, s)`, `regex.captures(pat, s)` → `Option[List[String]]`

### args (requires `import args`)
`args.flag?(name)` → `Bool`, `args.option(name)` → `Option[String]`, `args.option_or(name, fallback)` → `String`, `args.positional()` → `List[String]`

## Key rules
- Newline = statement separator (no semicolons needed)
- `[]` for generics, NOT `<>`
- `<` `>` are always comparison operators
- `effect fn` for side effects, NOT `fn name!()`
- `?` suffix is for Bool predicates only
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
import json

type AppError =
  | NotFound(String)
  | Io(String)

effect fn greet(name: String) -> Result[Unit, AppError] = {
  guard string.len(name) > 0 else err(NotFound("empty name"))
  println("Hello, ${name}!")
  ok(())
}

effect fn process_all(items: List[String]) -> Result[Unit, AppError] = {
  for item in items {
    println("Processing: ${item}")
  }
  ok(())
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
