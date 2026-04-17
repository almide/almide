> Last updated: 2026-04-17

# Almide Language Specification

File extension: `.almd`

---

## 1. File Structure

An Almide source file consists of three sections in order:

1. **Module declaration** (optional) -- `module <path>`
2. **Imports** -- zero or more `import` declarations
3. **Declarations** -- functions, types, top-level lets, protocols, impls, tests

```
module myapp.utils

import fs
import json
import mylib.core.{ Parser, Lexer }

type Config = { port: Int, host: String }

fn default_config() -> Config = Config { port: 8080, host: "localhost" }

test "default config" {
  let c = default_config()
  assert_eq(c.port, 8080)
}
```

### Imports

```
import fs                           // simple module import
import mylib.core                   // dotted path
import mylib.core.{ Parser, Lexer } // selective import
import mylib.core as mc             // aliased import
```

Selective imports use the syntax `import path.{ Name1, Name2 }`. Aliases use `as`.

テスト: `spec/lang/import_test.almd`

---

## 2. Types

### 2.1 Primitive Types

| Type | Description |
|------|-------------|
| `Int` | 64-bit signed integer |
| `Float` | 64-bit floating point |
| `String` | UTF-8 string |
| `Bool` | `true` or `false` |
| `Unit` | Zero-value type, written `()` |
| `Path` | File path |
| `Bytes` | Byte sequence |

### 2.2 Collection Types

| Type | Description |
|------|-------------|
| `List[T]` | Ordered collection |
| `Map[K, V]` | Key-value mapping |
| `Set[T]` | Unique value collection |

### 2.3 Error Handling Types

| Type | Description |
|------|-------------|
| `Result[T, E]` | Success `ok(v)` or failure `err(e)` |
| `Option[T]` | Present `some(v)` or absent `none` |

### 2.4 Composite Types

| Type | Description |
|------|-------------|
| `(A, B, ...)` | Tuple |
| `{ field: Type, ... }` | Record (anonymous) |
| `Fn(A, B) -> C` | Function type |

テスト: `spec/lang/data_types_test.almd`, `spec/lang/tuple_test.almd`

---

## 3. Type Annotations

### 3.1 Simple Types

```
let x: Int = 42
let name: String = "alice"
```

### 3.2 Generic Types

Almide uses `[]` for generics, never `<>`.

```
let xs: List[Int] = [1, 2, 3]
let m: Map[String, Int] = ["a": 1]
let r: Result[Int, String] = ok(42)
```

### 3.3 Function Types

```
type Handler = (String) -> String
type Predicate = (Int) -> Bool
type Reducer = (Int, Int) -> Int
type Thunk = () -> Int
```

### 3.4 Tuple Types

```
type Pair = (Int, String)
type Triple = (Int, Int, Int)
```

### 3.5 Record Types (anonymous)

```
type User = { name: String, age: Int }
```

Record fields support default values and serialization aliases:

```
type Config = {
  host: String = "localhost",
  port: Int = 8080,
  name as "display_name": String,
}
```

### 3.6 Open Record Types (structural)

```
fn get_name[T: { name: String, .. }](obj: T) -> String = obj.name
```

The `..` indicates the record may have additional fields beyond those listed.

テスト: `spec/lang/type_annotation_test.almd`, `spec/lang/open_record_test.almd`, `spec/lang/generics_test.almd`

---

## 4. Declarations

### 4.1 Functions (`fn`)

```
fn add(x: Int, y: Int) -> Int = x + y
```

The body follows `=` and is a single expression. Multi-statement bodies use a block:

```
fn greet(name: String) -> String = {
  let upper = string.to_upper(name)
  "Hello, ${upper}!"
}
```

Braceless blocks are also supported -- when the body starts with `let`, `var`, or `guard`, the parser collects statements until the next top-level declaration:

```
fn process(x: Int) -> Int =
  let doubled = x * 2
  let capped = int.min(doubled, 100)
  capped
```

#### Generic Functions

```
fn identity[T](x: T) -> T = x
fn first[A, B](pair: (A, B)) -> A = pair.0
fn apply[T: Repr](x: T) -> String = repr(x)
```

Generic parameters use `[]`. Bounds are specified with `:` and combined with `+`:

```
fn show[T: Repr + Eq](x: T) -> String = repr(x)
```

Structural bounds use an open record type:

```
fn name_of[T: { name: String, .. }](x: T) -> String = x.name
```

#### Default Parameters

```
fn connect(host: String, port: Int = 8080) -> String =
  "${host}:${int.to_string(port)}"
```

All parameters after the first default must also have defaults.

#### Named Arguments

```
fn create(name: String, age: Int, active: Bool = true) -> User =
  User { name, age, active }

let u = create("alice", 30, active: false)
```

#### Self Parameter

Functions can take `self` as the first parameter for method-like dispatch:

```
fn User.greet(self) -> String = "Hi, ${self.name}"
```

#### Hole and Todo

```
fn not_yet(x: Int) -> String = _                     // hole: type-checked stub
fn later(x: Int) -> String = todo("implement later")  // todo with message
```

#### Attributes

Function declarations can be prefixed with one or more `@name` or
`@name(args)` attributes. The parser accepts a generic shape:

```
@pure
@inline_rust("almide_rt_int_abs({n})")
@schedule(device=gpu, tile=32, unroll=true)
fn decorated(n: Int) -> Int = int.abs(n)
```

Grammar:

- `@name` — no args
- `@name(arg, ...)` — positional, named (`key=value`), or mixed
- Argument values: `"string"`, `42`, `0xff`, `-1`, `true`, `false`, or
  a bare identifier (treated as a symbolic tag, not a value reference)

Two attribute names have semantic meaning today:

- `@extern(target, "module", "function")` — FFI binding for the target
  runtime. See [§11 of module-system.md](./module-system.md#11-extern).
- `@export(c, "symbol")` — export with C ABI. Paired with
  `--repr-c` output (see module-system §10).

All other attribute names parse without error and are preserved in the
AST, but carry no semantic behavior yet. They are reserved for the
Stdlib Declarative Unification and MLIR Backend arcs (see
`docs/roadmap/active/stdlib-declarative-unification.md` and
`docs/roadmap/active/mlir-backend-adoption.md`). Writing
`@inline_rust(...)` or `@schedule(...)` in user code today is legal
syntax but the compiler ignores it.

テスト: `crates/almide-syntax/src/parser/test_attributes.rs` (13
parse tests), `crates/almide-tools/src/fmt.rs::attr_tests` (6 format
round-trip tests).

テスト: `spec/lang/function_test.almd`, `spec/lang/default_args_test.almd`, `spec/lang/named_args_test.almd`, `spec/lang/generics_test.almd`

### 4.2 Effect Functions (`effect fn`)

Functions with side effects (IO, randomness, etc.) use the `effect fn` modifier. They return `Result[T, E]` and support automatic `?`-propagation via `!`.

```
effect fn read_config(path: String) -> Result[String, String] = {
  let content = fs.read_text(path)!
  ok(content)
}
```

When an `effect fn` returns `Result[T, E]` and the body is a block that ends without an explicit `ok(...)`, the compiler automatically wraps the trailing expression in `ok(())`:

```
effect fn log(msg: String) -> Result[Unit, String] = {
  println(msg)
  // ok(()) is inserted automatically
}
```

テスト: `spec/lang/effect_fn_test.almd`

### 4.3 Type Declarations (`type`)

#### Record Types

```
type User = { name: String, age: Int }
```

#### Variant Types (leading `|`)

```
type Color =
  | Red
  | Green
  | Blue
```

Variant cases can carry data:

```
type Shape =
  | Circle(Float)
  | Rect(Float, Float)
  | Named{ name: String, sides: Int }
```

Three forms of variant cases:
- **Unit**: `| CaseName` -- no payload
- **Tuple**: `| CaseName(Type, ...)` -- positional fields
- **Record**: `| CaseName{ field: Type, ... }` -- named fields

#### Inline Variant (no leading `|`)

```
type Result[T, E] = Ok(T) | Err(E)
```

When all cases are bare uppercase names with no payload, the parser treats `A | B | C` as a union/enum:

```
type Direction = North | South | East | West
```

#### Type Alias

```
type Name = String
type Handler = (String) -> String
```

#### Generic Types

```
type Pair[A, B] = { first: A, second: B }
type Tree[T] = | Leaf(T) | Node(Tree[T], Tree[T])
```

#### Conventions (Deriving)

```
type Color: Eq, Repr = Red | Green | Blue
type Point: Codec = { x: Float, y: Float }
```

Built-in conventions: `Eq`, `Repr`, `Ord`, `Hash`, `Codec`.

テスト: `spec/lang/data_types_test.almd`, `spec/lang/type_alias_test.almd`, `spec/lang/variant_record_test.almd`, `spec/lang/derive_conventions_test.almd`

### 4.4 Protocol Declarations

```
protocol Action {
  fn name(a: Self) -> String
  fn execute(a: Self, ctx: Context) -> Result[String, String]
  effect fn load(a: Self) -> Result[Unit, String]
}
```

Protocol methods can be `effect fn`.

テスト: `spec/lang/protocol_test.almd`, `spec/lang/protocol_advanced_test.almd`

### 4.5 Impl Blocks

```
impl Action for GreetAction {
  fn name(a: GreetAction) -> String = "greet"
  fn execute(a: GreetAction, ctx: Context) -> Result[String, String] =
    ok(a.greeting)
}
```

テスト: `spec/lang/impl_block_test.almd`, `spec/lang/trait_impl_test.almd`

### 4.6 Top-level `let`

Module-scope constants:

```
let PI = 3.14159265358979323846
let MAX_RETRIES = 3
let GREETING = "Hello"
```

Evaluated at compile time (const) or via `LazyLock` for non-const expressions.

テスト: `spec/lang/top_let_test.almd`

### 4.7 Test Declarations

```
test "addition works" {
  assert_eq(1 + 2, 3)
  assert(3 > 0)
  assert_ne(1, 2)
}
```

The body is a brace-delimited block expression.

テスト: All `spec/lang/*_test.almd` files contain `test` blocks.

### 4.8 Strict Mode

```
strict warnings
```

Enables stricter compiler diagnostics.

---

## 5. Expressions

### 5.1 Literals

```
42                     // Int
0xFF                   // Int (hex)
1_000_000              // Int (underscores for readability)
3.14                   // Float
"hello"                // String
true                   // Bool
false                  // Bool
()                     // Unit
```

テスト: `spec/lang/expr_test.almd`

### 5.2 String Interpolation

```
"hello ${name}"
"result = ${1 + 2}"
"nested ${string.len(name)}"
```

Expressions inside `${}` are parsed as full expressions.

テスト: `spec/lang/string_interp_test.almd`, `spec/lang/interpolation_edge_test.almd`

### 5.3 Heredoc (Multi-line Strings)

```
let sql = """
  SELECT *
  FROM users
  WHERE id = ${id}
"""
```

Leading whitespace is stripped based on minimum indent. Interpolation `${expr}` works inside heredocs.

Raw heredoc (no escape processing): `r"""..."""`

テスト: `spec/lang/heredoc_test.almd`

### 5.4 List Literals

```
[]                     // empty list
[1, 2, 3]             // List[Int]
["a", "b", "c"]       // List[String]
[1, 2, 3,]            // trailing comma allowed
```

テスト: `spec/lang/expr_test.almd`

### 5.5 Map Literals

```
[:]                                  // empty map (requires type annotation)
["a": 1, "b": 2]                    // Map[String, Int]
let m: Map[String, Int] = [:]       // typed empty map
```

Maps use `[key: value]` syntax -- braces `{}` are for records/blocks, brackets `[]` for lists and maps.

テスト: `spec/lang/map_literal_test.almd`, `spec/lang/map_edge_test.almd`

### 5.6 Record Literals

Anonymous records:

```
{ name: "alice", age: 30 }
```

Named records (typed construction):

```
User { name: "alice", age: 30 }
```

Field shorthand -- when the value is a variable with the same name as the field:

```
let name = "alice"
let age = 30
{ name, age }           // equivalent to { name: name, age: age }
```

#### Spread Records

```
let base = { name: "alice", age: 30 }
{ ...base, name: "bob" }
User { ...base, name: "bob" }
```

テスト: `spec/lang/record_spread_test.almd`, `spec/lang/data_types_test.almd`

### 5.7 Tuple Expressions

```
(1, "hello")           // (Int, String)
(1, 2, 3)             // (Int, Int, Int)
```

Access tuple elements by index:

```
let pair = (1, "hello")
pair.0                 // 1
pair.1                 // "hello"
```

テスト: `spec/lang/tuple_test.almd`

### 5.8 If-Then-Else

`if` is an expression and requires `then`. `else` is optional -- without `else`, the result is `Unit`.

```
if x > 0 then "positive" else "non-positive"
if a then x else if b then y else z
```

```
let label = if count == 1 then "item" else "items"
```

テスト: `spec/lang/control_flow_test.almd`, `spec/lang/expr_test.almd`

### 5.9 Match

Exhaustive pattern matching:

```
match shape {
  Circle(r) => 3.14 * r * r,
  Rect(w, h) => w * h,
  Named{ name, sides } => float.from_int(sides),
}
```

Match arms support guards:

```
match n {
  x if x > 0 => "positive",
  0 => "zero",
  _ => "negative",
}
```

Pipe-match syntax:

```
value |> match {
  some(x) => x,
  none => 0,
}
```

テスト: `spec/lang/pattern_test.almd`, `spec/lang/match_edge_test.almd`

### 5.10 Lambda

```
(x) => x + 1
(x, y) => x + y
(x: Int) => x * 2                    // with type annotation
(_) => 42                             // wildcard parameter
((a, b)) => a + b                     // tuple destructuring in parameter
```

Multi-line lambda body uses a block:

```
let f = (x) => {
  let y = x * 2
  y + 1
}
```

テスト: `spec/lang/lambda_test.almd`

### 5.11 Block Expressions

The last expression in a block is the block's value:

```
let result = {
  let x = 1
  let y = 2
  x + y
}
// result = 3
```

テスト: `spec/lang/expr_test.almd`, `spec/lang/scope_test.almd`

### 5.12 For-In Loop

```
for x in xs {
  println(int.to_string(x))
}
```

Tuple destructuring in for:

```
for (k, v) in map.entries(m) {
  println("${k} = ${v}")
}
```

Underscore for ignored variable:

```
for _ in 0..5 {
  println("tick")
}
```

テスト: `spec/lang/for_test.almd`, `spec/lang/for_tuple_test.almd`

### 5.13 While Loop

```
var i = 0
while i < 10 {
  println(int.to_string(i))
  i = i + 1
}
```

`break` and `continue` are supported inside loops:

```
var i = 0
while true {
  if i >= 10 then break
  i = i + 1
  if i % 2 == 0 then continue
  println(int.to_string(i))
}
```

テスト: `spec/lang/while_test.almd`, `spec/lang/while_loop_test.almd`

### 5.14 Range

```
0..5              // [0, 1, 2, 3, 4]     (exclusive end)
1..=5             // [1, 2, 3, 4, 5]     (inclusive end)
```

Ranges can be used in for loops (optimized, no list allocation):

```
for i in 0..n {
  println(int.to_string(i))
}
```

テスト: `spec/lang/range_test.almd`

### 5.15 Pipe Operator

```
text |> string.trim |> string.split(",")
xs |> list.filter(_, (x) => x > 0)      // _ = placeholder for piped value
```

The pipe operator `|>` passes the left side as an argument to the right side. Use `_` as a placeholder for the piped value in function calls.

テスト: `spec/lang/pipe_test.almd`

### 5.16 Compose Operator

```
let transform = string.trim >> string.to_upper
transform("  hello  ")  // "HELLO"
```

The `>>` operator composes two functions left-to-right.

テスト: `spec/lang/compose_test.almd`

### 5.17 Fan Blocks (Concurrent Execution)

```
let (a, b, c) = fan {
  fetch_users()
  fetch_orders()
  fetch_config()
}
```

Fan blocks execute expressions concurrently. Each expression in the block runs in parallel; the block returns a tuple of results.

Fan blocks only allow expressions -- no `let`, `var`, `for`, or `while` statements.

テスト: `spec/lang/fan_test.almd`, `spec/lang/fan_map_test.almd`, `spec/lang/fan_race_test.almd`, `spec/lang/fan_ext_test.almd`

### 5.18 Option and Result Constructors

```
some(42)       // Option[Int] = some
none           // Option[T] = none

ok(42)         // Result[Int, E] = success
err("failed")  // Result[T, String] = failure
```

テスト: `spec/lang/error_test.almd`, `spec/lang/unwrap_operators_test.almd`

### 5.19 Function Calls

```
add(1, 2)                          // positional args
string.split(text, ",")           // module function call
list.map(xs, (x) => x + 1)       // higher-order
f[Int](42)                         // explicit type arguments
```

Named arguments:

```
connect("localhost", port: 3000, secure: true)
```

テスト: `spec/lang/function_test.almd`, `spec/lang/named_args_test.almd`

### 5.20 Member Access and Index Access

```
user.name                  // field access
pair.0                     // tuple index access
xs[0]                      // list index
m["key"]                   // map index (returns Option[V])
```

テスト: `spec/lang/expr_test.almd`, `spec/lang/tuple_test.almd`

### 5.21 UFCS (Uniform Function Call Syntax)

`f(x, y)` is equivalent to `x.f(y)`. The compiler resolves automatically.

```
let trimmed = string.trim(text)
let trimmed = text.trim()          // equivalent via UFCS
```

---

## 6. Statements

Statements appear inside blocks and function bodies. Newlines separate statements (no semicolons needed).

### 6.1 Let Binding (immutable)

```
let x = 1
let x: Int = 1                     // with type annotation
let _ = some_fn()                  // discard result
```

### 6.2 Var Binding (mutable)

```
var count = 0
count = count + 1                  // reassignment (var only)
```

### 6.3 Destructuring

Record destructuring:

```
let { name, age } = user
```

Tuple destructuring:

```
let (x, y) = point
let (first, _, third) = triple     // wildcard for unused
```

テスト: `spec/lang/variable_test.almd`, `spec/lang/data_types_test.almd`

### 6.4 Assignment

Simple reassignment (var only):

```
x = x + 1
```

Index assignment (var only):

```
xs[0] = 99
m["key"] = value
```

Field assignment (var only):

```
user.name = "bob"
```

### 6.5 Guard (early return)

```
guard x > 0 else err("must be positive")
guard fs.exists(path) else err(NotFound(path))
```

With block body:

```
guard not fs.exists(path) else {
  println("already exists")
  ok(())
}
```

`guard` checks a condition; if false, executes the `else` branch (which must diverge or return a Result).

テスト: `spec/lang/guard_test.almd`

### 6.6 Expression Statements

Any expression can appear as a statement. The last expression in a block is the block's value.

```
{
  println("side effect")           // expression statement (Unit)
  let x = 42
  x + 1                           // final expression = block value
}
```

---

## 7. Pattern Matching

Patterns appear in `match` arms, `let` destructuring, and `for` loop variables.

### 7.1 Pattern Forms

| Pattern | Syntax | Description |
|---------|--------|-------------|
| Wildcard | `_` | Matches anything, binds nothing |
| Identifier | `name` | Matches anything, binds to `name` |
| Literal | `42`, `3.14`, `"text"`, `true`, `false` | Matches exact value |
| Negative literal | `-1`, `-3.14` | Matches negative number |
| Constructor | `TypeName(p1, p2)` | Matches variant with tuple payload |
| Record | `TypeName{ field1, field2 }` | Matches variant with record payload |
| Record (nested) | `TypeName{ field: pattern }` | Matches with nested pattern on field |
| Record (rest) | `TypeName{ field, .. }` | Matches with additional fields ignored |
| Tuple | `(p1, p2, p3)` | Matches tuple |
| `some(p)` | `some(inner)` | Matches `Option` some case |
| `none` | `none` | Matches `Option` none case |
| `ok(p)` | `ok(inner)` | Matches `Result` ok case |
| `err(p)` | `err(inner)` | Matches `Result` err case |

### 7.2 Examples

```
match value {
  0 => "zero",
  n if n > 0 => "positive: ${int.to_string(n)}",
  _ => "negative",
}

match result {
  ok(value) => println(value),
  err(msg) => eprintln(msg),
}

match option {
  some(x) => x * 2,
  none => 0,
}

match shape {
  Circle(r) => 3.14 * r * r,
  Rect(w, h) => w * h,
  Named{ name, .. } => {
    println(name)
    0.0
  },
}
```

### 7.3 Nested Patterns

Patterns compose:

```
match pair {
  (ok(x), ok(y)) => ok(x + y),
  (err(e), _) => err(e),
  (_, err(e)) => err(e),
}
```

テスト: `spec/lang/pattern_test.almd`, `spec/lang/match_edge_test.almd`

---

## 8. Operators

### 8.1 Precedence (highest to lowest)

| Precedence | Operators | Associativity |
|------------|-----------|---------------|
| 1 | `.` `()` `[]` `!` `??` `?.` `?` | Left (postfix) |
| 2 | `not` `-` (unary) | Right (prefix) |
| 3 | `^` | Right |
| 4 | `*` `/` `%` | Left |
| 5 | `+` `-` | Left |
| 6 | `..` `..=` | Non-associative |
| 7 | `==` `!=` `<` `>` `<=` `>=` | Non-associative |
| 8 | `and` | Left |
| 9 | `or` | Left |
| 10 | `\|>` `>>` | Left |

### 8.2 Arithmetic Operators

| Operator | Description |
|----------|-------------|
| `+` | Addition (Int, Float); concatenation (String, List) |
| `-` | Subtraction |
| `*` | Multiplication |
| `/` | Division |
| `%` | Modulo |
| `^` | Exponentiation (right-associative) |
| `-` (unary) | Negation |

`+` is overloaded: addition for numbers, concatenation for strings and lists.

### 8.3 Comparison Operators

| Operator | Description |
|----------|-------------|
| `==` | Equal (deep equality) |
| `!=` | Not equal |
| `<` | Less than |
| `>` | Greater than |
| `<=` | Less than or equal |
| `>=` | Greater than or equal |

Comparisons are **non-associative**: chaining like `a < b < c` is a compile error. Use `a < b and b < c`.

### 8.4 Logical Operators

| Operator | Description |
|----------|-------------|
| `and` | Logical AND (short-circuit) |
| `or` | Logical OR (short-circuit) |
| `not` | Logical NOT (prefix) |

Almide uses words, not symbols: `&&` and `||` are rejected with hints.

### 8.5 Unwrap Operators

| Operator | Syntax | Description |
|----------|--------|-------------|
| `!` | `expr!` | Unwrap Result/Option; propagate error (effect fn only) |
| `??` | `expr ?? fallback` | Unwrap or use fallback value |
| `?` | `expr?` | Convert Result to Option (err becomes none) |
| `?.` | `expr?.field` | Optional chaining (Option[Record] to Option[FieldType]) |

```
let value = map.get(m, "key") ?? "default"
let content = fs.read_text(path)!
let name = user?.name
let opt = risky_fn()?
```

テスト: `spec/lang/unwrap_operators_test.almd`, `spec/lang/operator_test.almd`

### 8.6 Bitwise Operations

Bitwise operations are functions, not operators:

```
int.band(a, b)          // AND
int.bor(a, b)           // OR
int.bxor(a, b)          // XOR
int.bnot(a)             // NOT
int.bshl(a, n)          // shift left
int.bshr(a, n)          // shift right
```

### 8.7 String Concatenation

```
"hello" + " " + "world"        // string concatenation with +
[1, 2] + [3, 4]                // list concatenation with +
```

テスト: `spec/lang/string_test.almd`, `spec/lang/operator_test.almd`

---

## 9. Visibility

Visibility modifiers appear before `fn`, `type`, or `let` at the top level.

| Modifier | Scope | Rust equivalent |
|----------|-------|-----------------|
| (none) | Public -- anyone can access | `pub` |
| `mod` | Same project only | `pub(crate)` |
| `local` | This file only | (private) |

```
fn public_fn() -> Int = 42              // public (default)
mod fn internal_fn() -> Int = 42        // project-internal
local fn private_fn() -> Int = 42       // file-private
```

Modifier order: `[local|mod]? effect? fn`

```
local effect fn helper() -> Result[Unit, String] = ok(())
mod fn utility(x: Int) -> Int = x * 2
```

Visibility also applies to types and top-level lets:

```
local type InternalState = { count: Int }
mod let CACHE_SIZE = 256
```

テスト: `spec/lang/visibility_test.almd`

---

## 10. Comments

### 10.1 Line Comments

```
// This is a line comment
let x = 42  // inline comment
```

### 10.2 Block Comments

Block comments are nestable:

```
/* This is a block comment */

/*
  /* Nested block comments work */
  Still inside the outer comment
*/
```

Block comments are fully skipped by the lexer (not emitted as tokens).

テスト: `spec/lang/block_comment_raw_string_test.almd`

---

## 11. Built-in Functions

```
println(s)              // print line to stdout
eprintln(s)             // print line to stderr (debug only)
assert_eq(a, b)         // assert equal (test blocks)
assert_ne(a, b)         // assert not equal (test blocks)
assert(cond)            // assert true (test blocks)
repr(x)                 // string representation (types with Repr)
```

There is no `print` function -- use `println` for all output.

テスト: `spec/lang/prelude_test.almd`

---

## 12. Entry Point

```
effect fn main() -> Unit = {
  let args = process.args()
  let name = list.get(args, 1) ?? "world"
  println("Hello, ${name}!")
}
```

`main` takes no parameters. Command-line arguments are accessed via `process.args()`. `effect fn main()` is auto-wrapped to return `Result<(), String>` — no need to write `ok(())` or explicit `Result` type.

---

## 13. Key Design Rules

- Newline = statement separator (no semicolons needed)
- `[]` for generics, never `<>`
- `<` and `>` are always comparison operators
- `effect fn` for side effects
- No exceptions -- use `Result[T, E]`
- No null -- use `Option[T]`
- No inheritance -- use composition and protocols
- No macros, no operator overloading, no implicit conversions
- Empty list = `[]`, empty map = `[:]` (with type annotation)
- `_` is for match wildcard, let discard, for discard, and lambda wildcard params
- All stdlib functions require module prefix: `string.len(s)`, not `len(s)`
- `println(x)` where x is Int requires explicit conversion: `println(int.to_string(x))`

---

## Appendix: Test File Index

All test files are located under `spec/lang/`:

| File | Coverage |
|------|----------|
| `auto_derive_test.almd` | Automatic Eq/Hash derivation |
| `bidirectional_type_test.almd` | Type inference |
| `block_comment_raw_string_test.almd` | Block comments, raw strings |
| `bytes_test.almd` | Bytes type |
| `capture_clone_test.almd` | Closure capture semantics |
| `codec_*_test.almd` | Codec convention (serialization) |
| `compose_test.almd` | `>>` compose operator |
| `control_flow_test.almd` | If/else, match, loops |
| `data_types_test.almd` | Records, variants, tuples, collections |
| `default_args_test.almd` | Default parameter values |
| `default_fields_test.almd` | Default record field values |
| `derive_conventions_test.almd` | Convention deriving (Eq, Repr, etc.) |
| `edge_cases_test.almd` | Parser/compiler edge cases |
| `effect_fn_test.almd` | Effect functions |
| `eq_protocol_test.almd` | Eq protocol |
| `equality_test.almd` | Deep equality semantics |
| `error_test.almd` | Result/Option handling |
| `escape_analysis_test.almd` | Ownership analysis |
| `expr_test.almd` | Basic expressions |
| `fan_*_test.almd` | Fan blocks (concurrency) |
| `for_test.almd`, `for_tuple_test.almd` | For-in loops |
| `function_test.almd` | Function declarations |
| `generics_test.almd` | Generic types and functions |
| `guard_test.almd` | Guard statements |
| `hash_protocol_test.almd` | Hash protocol |
| `heredoc_test.almd` | Multi-line strings |
| `impl_block_test.almd`, `impl_error_test.almd` | Impl blocks |
| `import_test.almd` | Import declarations |
| `interpolation_edge_test.almd` | String interpolation edge cases |
| `lambda_test.almd` | Lambda expressions |
| `map_literal_test.almd`, `map_edge_test.almd` | Map literals |
| `match_edge_test.almd` | Match edge cases |
| `matrix_test.almd` | Matrix type |
| `named_args_test.almd` | Named arguments |
| `open_record_test.almd` | Open record types |
| `operator_test.almd`, `operator_protocol_test.almd` | Operators |
| `panic_test.almd` | Panic behavior |
| `pattern_test.almd` | Pattern matching |
| `pipe_test.almd` | Pipe operator |
| `prelude_test.almd` | Built-in functions |
| `protocol_*_test.almd` | Protocol system |
| `range_test.almd` | Range expressions |
| `record_spread_test.almd` | Record spread |
| `scope_test.almd` | Variable scoping |
| `string_interp_test.almd`, `string_test.almd` | Strings |
| `tco_test.almd` | Tail call optimization |
| `top_let_test.almd` | Top-level let bindings |
| `trait_impl_test.almd` | Trait implementation |
| `tuple_test.almd` | Tuples |
| `type_alias_test.almd` | Type aliases |
| `type_annotation_test.almd` | Type annotations |
| `type_system_test.almd` | Type system |
| `unwrap_operators_test.almd` | `!`, `??`, `?`, `?.` operators |
| `value_utils_test.almd` | Value utilities |
| `variable_test.almd` | Variable bindings |
| `variant_*_test.almd` | Variant types |
| `visibility_test.almd` | Visibility modifiers |
| `while_test.almd`, `while_loop_test.almd` | While loops |
