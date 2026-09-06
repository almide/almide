> Last updated: 2026-08-13

# Almide Language Specification

File extension: `.almd`

---

## 1. File Structure

An Almide source file consists of three sections in order:

1. **Module declaration** (optional) -- `module <path>`
2. **Imports** -- zero or more `import` declarations
3. **Declarations** -- functions, types, top-level lets, protocols, impls, tests

```almide project
// file: mylib/src/core.almd
type Parser = { pos: Int }
type Lexer = { src: String }
fn new_parser() -> Parser = Parser { pos: 0 }
// file: main.almd
module myapp.utils

import fs
import json
import mylib.core.{ Parser, new_parser }

type Config = { port: Int, host: String }

fn default_config() -> Config = Config { port: 8080, host: "localhost" }
fn fresh() -> Parser = new_parser()

test "default config" {
  let c = default_config()
  assert_eq(c.port, 8080)
  assert_eq(fresh().pos, 0)
}
```

### Imports

```almide project
// file: mylib/src/core.almd
type Lexer = { src: String }
fn lex(s: String) -> Lexer = Lexer { src: s }
fn version() -> String = "1"
// file: main.almd
import fs                           // simple module import
import mylib.core                   // dotted path → core.lex(...)
import mylib.core.{ lex, Lexer }    // selective import → lex(...)
import mylib.core as mc             // aliased import → mc.version()

test "every import form resolves" {
  assert_eq(core.lex("a").src, "a")
  let l: Lexer = lex("b")
  assert_eq(l.src, "b")
  assert_eq(mc.version(), "1")
  assert_eq(fs.exists("/nonexistent/almide-doctest"), false)
}
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
| `T?` | Shorthand for `Option[T]`, valid in every type position (ADR-0010). `?` binds to the preceding type atom, never across `->`: `(A) -> B?` returns `Option[B]`; an optional fn value spells `((A) -> B)?`; nested Option spells `(T?)?`. Canonical — `almide fmt` normalizes `Option[T]` to `T?` |

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

```almide
let x: Int = 42
let name: String = "alice"
```

### 3.2 Generic Types

Almide uses `[]` for generics, never `<>`.

```almide
let xs: List[Int] = [1, 2, 3]
let m: Map[String, Int] = ["a": 1]
let r: Result[Int, String] = ok(42)
```

### 3.3 Function Types

```almide
type Handler = (String) -> String
type Predicate = (Int) -> Bool
type Reducer = (Int, Int) -> Int
type Thunk = () -> Int
```

### 3.4 Tuple Types

```almide
type Pair = (Int, String)
type Triple = (Int, Int, Int)
```

### 3.5 Record Types (anonymous)

```almide
type User = { name: String, age: Int }
```

Record fields support default values and serialization aliases:

```almide
type Config = {
  host: String = "localhost",
  port: Int = 8080,
  name as "display_name": String,
}
```

### 3.6 Open Record Types (structural)

```almide
fn get_name[T: { name: String, .. }](obj: T) -> String = obj.name
```

The `..` indicates the record may have additional fields beyond those listed.

テスト: `spec/lang/type_annotation_test.almd`, `spec/lang/open_record_test.almd`, `spec/lang/generics_test.almd`

---

## 4. Declarations

### 4.1 Functions (`fn`)

```almide
fn add(x: Int, y: Int) -> Int = x + y
```

The body follows `=` and is a single expression. Multi-statement bodies use a block:

```almide
fn greet(name: String) -> String = {
  let upper = string.to_upper(name)
  "Hello, ${upper}!"
}
```

Braceless blocks are also supported -- when the body starts with `let`, `var`, or `guard`, the parser collects statements until the next top-level declaration:

```almide
fn process(x: Int) -> Int =
  let doubled = x * 2
  let capped = int.min(doubled, 100)
  capped
```

#### Generic Functions

```almide
protocol Show {
  fn show(a: Self) -> String
}
fn identity[T](x: T) -> T = x
fn first[A, B](pair: (A, B)) -> A = pair.0
fn apply[T: Show](x: T) -> String = x.show()
```

Generic parameters use `[]`. Bounds are specified with `:` and combined with `+`:

```almide
protocol Show {
  fn show(a: Self) -> String
}
fn same_or_both[T: Show + Eq](x: T, y: T) -> String =
  if x == y then x.show() else x.show() + "/" + y.show()
```

Structural bounds use an open record type:

```almide
fn name_of[T: { name: String, .. }](x: T) -> String = x.name
```

#### Default Parameters

```almide
fn connect(host: String, port: Int = 8080) -> String =
  "${host}:${int.to_string(port)}"
```

All parameters after the first default must also have defaults.

#### Named Arguments

```almide
type User = { name: String, age: Int, active: Bool }

fn create(name: String, age: Int, active: Bool = true) -> User =
  User { name: name, age: age, active: active }

let u = create("alice", 30, active: false)

test "named argument" {
  assert_eq(u.active, false)
}
```

#### Self Parameter

Functions can take `self` as the first parameter for method-like dispatch:

```almide
type User = { name: String }

fn User.greet(self) -> String = "Hi, ${self.name}"

test "self parameter dispatches via UFCS" {
  assert_eq(User { name: "alice" }.greet(), "Hi, alice")
}
```

#### Hole and Todo

```almide
fn not_yet(x: Int) -> String = _                     // hole: type-checked stub
fn later(x: Int) -> String = todo("implement later")  // todo with message
```

#### Attributes

Function declarations can be prefixed with one or more `@name` or
`@name(args)` attributes. The parser accepts a generic shape:

```almide
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

Attribute names with semantic meaning today:

- `@extern(target, "module", "function")` — FFI binding for the target
  runtime. See [§11 of module-system.md](./module-system.md#11-extern).
- `@export(c, "symbol")` — export with C ABI. Paired with
  `--repr-c` output (see module-system §10).
- `@inline_rust("template")` — **bundled stdlib only**. Routes the
  Rust target's codegen for the annotated fn to an inline template,
  overriding the TOML-backed `arg_transforms` dispatch. `{param_name}`
  placeholders are replaced with the rendered Rust expression for the
  matching call argument. The fn's body is not emitted as a Rust
  function; the template is inlined at every call site. Used by
  `stdlib/<module>.almd` files during the Stdlib Declarative
  Unification arc. テスト: `spec/stdlib/int_bundled_inline_rust_test.almd`.

Other attribute names (`@pure`, `@schedule`, `@rewrite`,
`@wasm_intrinsic`) parse without error and are preserved in the AST,
but carry no semantic behavior yet. They are reserved for later
sub-phases of the Stdlib Declarative Unification and MLIR Backend
arcs (see `docs/roadmap/done/stdlib-declarative-unification.md` and
`docs/roadmap/on-hold/mlir-backend-adoption.md`). Writing them in user
code today is legal syntax but the compiler ignores them.

テスト: `crates/almide-syntax/src/parser/test_attributes.rs` (13
parse tests), `crates/almide-tools/src/fmt.rs::attr_tests` (6 format
round-trip tests).

テスト: `spec/lang/function_test.almd`, `spec/lang/default_args_test.almd`, `spec/lang/named_args_test.almd`, `spec/lang/generics_test.almd`

### 4.2 Effect Functions (`effect fn`)

Functions with side effects (IO, randomness, etc.) use the `effect fn` modifier. They return `Result[T, E]` and support automatic `?`-propagation via `!`.

```almide
import fs

effect fn read_config(path: String) -> Result[String, String] = {
  let content = fs.read_text(path)!
  ok(content)
}
```

When an `effect fn` returns `Result[T, E]` and the body is a block that ends without an explicit `ok(...)`, the compiler automatically wraps the trailing expression in `ok(())`:

```almide
effect fn log(msg: String) -> Result[Unit, String] = {
  println(msg)
  // ok(()) is inserted automatically
}
```

テスト: `spec/lang/effect_fn_test.almd`

### 4.3 Type Declarations (`type`)

#### Record Types

```almide
type User = { name: String, age: Int }
```

#### Variant Types (leading `|`)

```almide
type Color =
  | Red
  | Green
  | Blue
```

Variant cases can carry data:

```almide
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

```almide
type Outcome[T, E] = Good(T) | Bad(E)
```

When all cases are bare uppercase names with no payload, the parser treats `A | B | C` as a union/enum:

```almide
type Direction = North | South | East | West
```

#### Type Alias

```almide
type Name = String
type Handler = (String) -> String
```

#### Generic Types

```almide
type Pair[A, B] = { first: A, second: B }
type Tree[T] = | Leaf(T) | Node(Tree[T], Tree[T])
```

#### Conventions (Deriving)

```almide
type Color: Eq, Repr = Red | Green | Blue
type Point: Codec = { x: Float, y: Float }
```

Built-in conventions: `Eq`, `Repr`, `Ord`, `Hash`, `Codec`.

テスト: `spec/lang/data_types_test.almd`, `spec/lang/type_alias_test.almd`, `spec/lang/variant_record_test.almd`, `spec/lang/derive_conventions_test.almd`

### 4.4 Protocol Declarations

```almide
protocol Action {
  fn name(a: Self) -> String
  fn execute(a: Self, ctx: Context) -> Result[String, String]
  effect fn load(a: Self) -> Result[Unit, String]
}
```

Protocol methods can be `effect fn`.

テスト: `spec/lang/protocol_test.almd`, `spec/lang/protocol_advanced_test.almd`

### 4.5 Convention Methods

`impl` ブロックは存在しない（削除済み）。プロトコルの充足も型へのメソッド追加も、
convention method（`fn Type.method`）で行う:

```almide
type Context = { user: String }
type GreetAction = { greeting: String }

fn GreetAction.name(a: GreetAction) -> String = "greet"
fn GreetAction.execute(a: GreetAction, ctx: Context) -> Result[String, String] =
  ok("${a.greeting}, ${ctx.user}")

test "convention methods: qualified and UFCS" {
  let a = GreetAction { greeting: "hi" }
  assert_eq(GreetAction.name(a), "greet")
  assert_eq(a.name(), "greet")
  assert_eq(a.execute(Context { user: "bob" }), ok("hi, bob"))
}
```

呼び出しは `GreetAction.name(a)` でも UFCS の `a.name()` でも可。

テスト: `spec/lang/trait_impl_test.almd`, `spec/lang/protocol_test.almd`

### 4.6 Top-level `let`

Module-scope constants:

```almide
let PI = 3.14159265358979323846
let MAX_RETRIES = 3
let GREETING = "Hello"
```

Evaluated at compile time (const) or via `LazyLock` for non-const expressions.

テスト: `spec/lang/top_let_test.almd`

### 4.7 Test Declarations

```almide
test "addition works" {
  assert_eq(1 + 2, 3)
  assert(3 > 0)
  assert_ne(1, 2)
}
```

The body is a brace-delimited block expression.

テスト: All `spec/lang/*_test.almd` files contain `test` blocks.

---

## 5. Expressions

### 5.1 Literals

```almide
let a = 42                     // Int
let b = 0xFF                   // Int (hex)
let c = 1_000_000              // Int (underscores for readability)
let d = 3.14                   // Float
let e = "hello"                // String
let f = true                   // Bool
let g = false                  // Bool
let h = ()                     // Unit

test "literal values" {
  assert_eq(b, 255)
  assert_eq(c, 1000000)
}
```

テスト: `spec/lang/expr_test.almd`

### 5.2 String Interpolation

```almide
test "interpolation" {
  let name = "alice"
  assert_eq("hello ${name}", "hello alice")
  assert_eq("result = ${1 + 2}", "result = 3")
  assert_eq("nested ${string.len(name)}", "nested 5")
}
```

Expressions inside `${}` are parsed as full expressions.

テスト: `spec/lang/string_interp_test.almd`, `spec/lang/interpolation_edge_test.almd`

### 5.3 Heredoc (Multi-line Strings)

```almide
let id = 7
let sql = """
  SELECT *
  FROM users
  WHERE id = ${id}
"""

test "heredoc strips the common indent" {
  assert_eq(sql, "SELECT *\nFROM users\nWHERE id = 7")
}
```

Leading whitespace is stripped based on minimum indent. Interpolation `${expr}` works inside heredocs.

Raw heredoc (no escape processing): `r"""..."""`

テスト: `spec/lang/heredoc_test.almd`

### 5.4 List Literals

```almide
let empty: List[Int] = []      // empty list (needs the annotation)
let ints = [1, 2, 3]           // List[Int]
let strs = ["a", "b", "c"]     // List[String]
let trailing = [1, 2, 3,]      // trailing comma allowed

test "list literals" {
  assert_eq(list.len(empty), 0)
  assert_eq(ints, trailing)
}
```

テスト: `spec/lang/expr_test.almd`

### 5.5 Map Literals

```almide
let m: Map[String, Int] = [:]       // empty map (requires type annotation)
let ab = ["a": 1, "b": 2]           // Map[String, Int]

test "map literals" {
  assert_eq(map.len(m), 0)
  assert_eq(ab["b"], some(2))
}
```

Maps use `[key: value]` syntax -- braces `{}` are for records/blocks, brackets `[]` for lists and maps.

テスト: `spec/lang/map_literal_test.almd`, `spec/lang/map_edge_test.almd`

### 5.6 Record Literals

Anonymous records:

```almide
let alice = { name: "alice", age: 30 }
```

Named records (typed construction):

```almide
type User = { name: String, age: Int }

let alice = User { name: "alice", age: 30 }
```

Every field is written `name: value` — there is no field shorthand
(`{ name, age }` is a syntax error):

```almide check-fail=syntax
let name = "alice"
let age = 30
let r = { name, age }
```

#### Spread Records

```almide
type User = { name: String, age: Int }

let base = { name: "alice", age: 30 }
let bob = { ...base, name: "bob" }
let typed = User { ...base, name: "bob" }

test "spread copies the unnamed fields" {
  assert_eq(bob.age, 30)
  assert_eq(typed.age, 30)
}
```

テスト: `spec/lang/record_spread_test.almd`, `spec/lang/data_types_test.almd`

### 5.7 Tuple Expressions

```almide
let pair = (1, "hello")        // (Int, String)
let triple = (1, 2, 3)         // (Int, Int, Int)
```

Access tuple elements by index:

```almide
test "tuple index access" {
  let pair = (1, "hello")
  assert_eq(pair.0, 1)
  assert_eq(pair.1, "hello")
}
```

テスト: `spec/lang/tuple_test.almd`

### 5.8 If-Then-Else

`if` is an expression and requires `then`. `else` is optional -- without `else`, the result is `Unit`.

```almide
fn sign(x: Int) -> String = if x > 0 then "positive" else "non-positive"
fn pick(a: Bool, b: Bool, x: Int, y: Int, z: Int) -> Int =
  if a then x else if b then y else z

test "if is an expression" {
  assert_eq(sign(-1), "non-positive")
  assert_eq(pick(false, true, 1, 2, 3), 2)
}
```

```almide
let count = 1
let label = if count == 1 then "item" else "items"
```

テスト: `spec/lang/control_flow_test.almd`, `spec/lang/expr_test.almd`

### 5.9 Match

Exhaustive pattern matching:

```almide
type Shape =
  | Circle(Float)
  | Rect(Float, Float)
  | Named{ name: String, sides: Int }

fn area(shape: Shape) -> Float = match shape {
  Circle(r) => 3.14 * r * r,
  Rect(w, h) => w * h,
  Named{ name, sides } => float.from_int(sides),
}

test "every case is handled" {
  assert_eq(area(Rect(2.0, 3.0)), 6.0)
  assert_eq(area(Named{ name: "tri", sides: 3 }), 3.0)
}
```

Match arms support guards:

```almide
fn classify(n: Int) -> String = match n {
  x if x > 0 => "positive",
  0 => "zero",
  _ => "negative",
}

test "guards are tried in order" {
  assert_eq(classify(5), "positive")
  assert_eq(classify(0), "zero")
  assert_eq(classify(-2), "negative")
}
```

Pipe-match syntax:

```almide
fn or_zero(value: Option[Int]) -> Int = value |> match {
  some(x) => x,
  none => 0,
}

test "pipe-match" {
  assert_eq(or_zero(some(4)), 4)
  assert_eq(or_zero(none), 0)
}
```

テスト: `spec/lang/pattern_test.almd`, `spec/lang/match_edge_test.almd`

### 5.10 Lambda

```almide
test "lambda forms" {
  let inc = (x) => x + 1
  let add = (x, y) => x + y
  let dbl = (x: Int) => x * 2                    // with type annotation
  let k = (_) => 42                              // wildcard parameter
  let sum = ((a, b)) => a + b                    // tuple destructuring in parameter
  assert_eq(inc(1), 2)
  assert_eq(add(1, 2), 3)
  assert_eq(dbl(2), 4)
  assert_eq(k("anything"), 42)
  assert_eq(sum((1, 2)), 3)
}
```

Multi-line lambda body uses a block:

```almide
test "block-bodied lambda" {
  let f = (x: Int) => {
    let y = x * 2
    y + 1
  }
  assert_eq(f(3), 7)
}
```

テスト: `spec/lang/lambda_test.almd`

### 5.11 Block Expressions

The last expression in a block is the block's value:

```almide
let result = {
  let x = 1
  let y = 2
  x + y
}
// result = 3
```

テスト: `spec/lang/expr_test.almd`, `spec/lang/scope_test.almd`

### 5.12 For-In Loop

```almide
fn print_all(xs: List[Int]) -> Unit = {
  for x in xs {
    println(int.to_string(x))
  }
}
```

Tuple destructuring in for:

```almide
fn print_entries(m: Map[String, Int]) -> Unit = {
  for (k, v) in map.entries(m) {
    println("${k} = ${v}")
  }
}
```

Underscore for ignored variable:

```almide
fn tick_five() -> Unit = {
  for _ in 0..<5 {
    println("tick")
  }
}
```

テスト: `spec/lang/for_test.almd`, `spec/lang/for_tuple_test.almd`

### 5.13 While Loop

```almide
fn count_to_ten() -> Unit = {
  var i = 0
  while i < 10 {
    println(int.to_string(i))
    i = i + 1
  }
}
```

`break` and `continue` are supported inside loops:

```almide
fn odd_numbers() -> Unit = {
  var i = 0
  while true {
    if i >= 10 then break
    i = i + 1
    if i % 2 == 0 then continue
    println(int.to_string(i))
  }
}
```

テスト: `spec/lang/while_test.almd`, `spec/lang/while_loop_test.almd`

### 5.14 Range

```almide
test "range forms" {
  assert_eq(0..<5, [0, 1, 2, 3, 4])     // exclusive end
  assert_eq(1...5, [1, 2, 3, 4, 5])     // inclusive end
}
```

Ranges can be used in for loops (optimized, no list allocation):

```almide
fn count_to(n: Int) -> Unit = {
  for i in 0..<n {
    println(int.to_string(i))
  }
}
```

テスト: `spec/lang/range_test.almd`

### 5.15 Pipe Operator

```almide
test "pipe passes the left side as the FIRST argument" {
  let text = " a,b "
  assert_eq(text |> string.trim |> string.split(","), ["a", "b"])
  let xs = [1, -2, 3]
  assert_eq(xs |> list.filter((x) => x > 0), [1, 3])
}
```

The pipe operator `|>` passes the left side as the first argument of the
right side. There is no placeholder: `_` in a call argument is a hole with no
value and is rejected (E046).

```almide check-fail=E046
fn positives(xs: List[Int]) -> List[Int] = xs |> list.filter(_, (x) => x > 0)
```

テスト: `spec/lang/pipe_test.almd`

### 5.16 Compose Operator

```almide
test "compose" {
  let transform = string.trim >> string.to_upper
  assert_eq(transform("  hello  "), "HELLO")
}
```

The `>>` operator composes two functions left-to-right.

テスト: `spec/lang/compose_test.almd`

### 5.17 Fan Blocks (Concurrent Execution)

```almide
effect fn fetch_users() -> Result[Int, String] = ok(3)
effect fn fetch_orders() -> Result[Int, String] = ok(7)
effect fn fetch_config() -> Result[String, String] = ok("prod")

test "fan returns a tuple of the unwrapped results" {
  let (a, b, c) = fan {
    fetch_users()
    fetch_orders()
    fetch_config()
  }
  assert_eq(a + b, 10)
  assert_eq(c, "prod")
}
```

Fan blocks execute expressions concurrently. Each expression in the block runs in parallel; the block returns a tuple of results.

Fan blocks only allow expressions -- no `let`, `var`, `for`, or `while` statements.

The `fan.*` block heads share the surface:

```almide
effect fn a() -> Result[Int, String] = err("a failed")
effect fn b() -> Result[Int, String] = ok(2)
fn work(x: Int) -> Int = x * 2

test "fan.* heads" {
  let first = fan.any {                 // first Ok in source order
    a()
    b()
  }
  let (ra, rb) = fan.settle {           // a TUPLE of per-arm Results
    a()
    b()
  }
  let r = fan.bounded(compute.ms(100)) { work(21) } ?? -1  // deterministic budget
  assert_eq(first, ok(2))
  assert_eq(ra, err("a failed"))
  assert_eq(rb, ok(2))
  assert_eq(r, 42)
}
```

Arms are separated by newlines (or `,`), never `;`. `fan.race` was removed in
0.42.0 — under the deterministic model it was `thunks[0]()` by another name —
and a reference is a tombstone error (E027).

Budgets are built with the `compute.*` time constructors (closed unit set
`ns / us / ms / s / min / h`); a bare `Int` or a wall-clock `duration.*` value
is a type error. Full semantics: [docs/SPEC.md §13](../SPEC.md), normative
time rules: [docs/adr/0001-deterministic-time-units.md](../adr/0001-deterministic-time-units.md).

テスト: `spec/lang/fan_test.almd`, `spec/lang/fan_map_test.almd`, `spec/lang/fan_ext_test.almd`, `spec/wasm_cross/fan_*.almd`, `research/spike/charge-probe/fixtures/`

### 5.18 Option and Result Constructors

```almide
let present: Option[Int] = some(42)            // Option[Int] = some
let absent: Option[Int] = none                 // Option[T] = none

let success: Result[Int, String] = ok(42)      // Result[Int, E] = success
let failure: Result[Int, String] = err("failed")  // Result[T, String] = failure
```

テスト: `spec/lang/error_test.almd`, `spec/lang/unwrap_operators_test.almd`

### 5.19 Function Calls

```almide
fn add(x: Int, y: Int) -> Int = x + y
fn f[T](x: T) -> T = x

test "call forms" {
  let text = "a,b"
  let xs = [1, 2]
  assert_eq(add(1, 2), 3)                           // positional args
  assert_eq(string.split(text, ","), ["a", "b"])    // module function call
  assert_eq(list.map(xs, (x) => x + 1), [2, 3])     // higher-order
  assert_eq(f[Int](42), 42)                         // explicit type arguments
}
```

Named arguments:

```almide
fn connect(host: String, port: Int = 80, secure: Bool = false) -> String =
  "${if secure then "https" else "http"}://${host}:${int.to_string(port)}"

test "named arguments" {
  assert_eq(connect("localhost", port: 3000, secure: true), "https://localhost:3000")
}
```

テスト: `spec/lang/function_test.almd`, `spec/lang/named_args_test.almd`

### 5.20 Member Access and Index Access

```almide
test "member and index access" {
  let user = { name: "alice" }
  let pair = (1, "x")
  let xs = [10, 20]
  let m = ["key": 5]
  assert_eq(user.name, "alice")      // field access
  assert_eq(pair.0, 1)               // tuple index access
  assert_eq(xs[0], 10)               // list index
  assert_eq(m["key"], some(5))       // map index (returns Option[V])
}
```

テスト: `spec/lang/expr_test.almd`, `spec/lang/tuple_test.almd`

### 5.21 UFCS (Uniform Function Call Syntax)

`f(x, y)` is equivalent to `x.f(y)`. The compiler resolves automatically.

```almide
test "UFCS" {
  let text = " x "
  let trimmed = string.trim(text)
  let same = text.trim()           // equivalent via UFCS
  assert_eq(trimmed, same)
}
```

---

## 6. Statements

Statements appear inside blocks and function bodies. Newlines separate statements (no semicolons needed).

### 6.1 Let Binding (immutable)

```almide
fn some_fn() -> Result[Int, String] = ok(1)

test "let forms" {
  let x = 1
  let y: Int = 1                   // with type annotation
  let _ = some_fn()                // discard result
  assert_eq(x, y)
}
```

### 6.2 Var Binding (mutable)

```almide
test "var" {
  var count = 0
  count = count + 1                // reassignment (var only)
  assert_eq(count, 1)
}
```

### 6.3 Destructuring

Record destructuring:

```almide
test "record destructuring" {
  let user = { name: "alice", age: 30 }
  let { name, age } = user
  assert_eq(name, "alice")
  assert_eq(age, 30)
}
```

Tuple destructuring:

```almide
test "tuple destructuring" {
  let point = (1, 2)
  let triple = (1, 2, 3)
  let (x, y) = point
  let (first, _, third) = triple   // wildcard for unused
  assert_eq(x + y, 3)
  assert_eq(first + third, 4)
}
```

テスト: `spec/lang/variable_test.almd`, `spec/lang/data_types_test.almd`

### 6.4 Assignment

Simple reassignment (var only):

```almide
test "reassignment" {
  var x = 1
  x = x + 1
  assert_eq(x, 2)
}
```

Index assignment (var only):

```almide
test "index assignment" {
  var xs = [1, 2]
  var m: Map[String, Int] = [:]
  let value = 5
  xs[0] = 99
  m["key"] = value
  assert_eq(xs, [99, 2])
  assert_eq(m["key"], some(5))
}
```

Field assignment (var only):

```almide
test "field assignment" {
  var user = { name: "alice" }
  user.name = "bob"
  assert_eq(user.name, "bob")
}
```

### 6.5 Guard (early return)

```almide
import fs

type AppError = NotFound(String) | Invalid(String)

fn positive(x: Int) -> Result[Int, String] = {
  guard x > 0 else err("must be positive")
  ok(x)
}

effect fn require(path: String) -> Result[String, AppError] = {
  guard fs.exists(path) else err(NotFound(path))
  ok(path)
}

test "guard returns the else value early" {
  assert_eq(positive(-1), err("must be positive"))
  assert_eq(require("/nonexistent/almide-doctest"), err(NotFound("/nonexistent/almide-doctest")))
}
```

With block body:

```almide
import fs

effect fn create_once(path: String) -> Result[Unit, String] = {
  guard not fs.exists(path) else {
    println("already exists")
    ok(())
  }
  fs.write(path, "")!
  ok(())
}
```

`guard` checks a condition; if false, executes the `else` branch (which must diverge or return a Result).

テスト: `spec/lang/guard_test.almd`

### 6.6 Expression Statements

Any expression can appear as a statement. The last expression in a block is the block's value.

```almide
fn forty_three() -> Int = {
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

```almide
type Shape =
  | Circle(Float)
  | Rect(Float, Float)
  | Named{ name: String, sides: Int }

fn describe(value: Int) -> String = match value {
  0 => "zero",
  n if n > 0 => "positive: ${int.to_string(n)}",
  _ => "negative",
}

fn report(result: Result[String, String]) -> Unit = match result {
  ok(value) => println(value),
  err(msg) => eprintln(msg),
}

fn doubled(option: Option[Int]) -> Int = match option {
  some(x) => x * 2,
  none => 0,
}

fn area(shape: Shape) -> Float = match shape {
  Circle(r) => 3.14 * r * r,
  Rect(w, h) => w * h,
  Named{ name, .. } => {
    println(name)
    0.0
  },
}

test "pattern forms" {
  assert_eq(describe(3), "positive: 3")
  assert_eq(doubled(some(2)), 4)
  assert_eq(area(Circle(1.0)), 3.14)
}
```

### 7.3 Nested Patterns

Patterns compose:

```almide
fn add_both(pair: (Result[Int, String], Result[Int, String])) -> Result[Int, String] =
  match pair {
    (ok(x), ok(y)) => ok(x + y),
    (err(e), _) => err(e),
    (_, err(e)) => err(e),
  }

test "nested patterns" {
  assert_eq(add_both((ok(1), ok(2))), ok(3))
  assert_eq(add_both((ok(1), err("b"))), err("b"))
}
```

テスト: `spec/lang/pattern_test.almd`, `spec/lang/match_edge_test.almd`

---

## 8. Operators

### 8.1 Precedence (highest to lowest)

| Precedence | Operators | Associativity |
|------------|-----------|---------------|
| 1 | `.` `()` `[]` `!` `??` `?.` `?` | 後置族。`??` のチェーンは右入れ子(`a ?? b ?? c` = `a ?? (b ?? c)`、意味は「最初の成功が勝つ」) |
| 2 | `not` `-` (unary) | Right (prefix) |
| 3 | `^` | Right |
| 4 | `*` `/` `%` | Left |
| 5 | `+` `-` | Left |
| 6 | `..<` `...` | Non-associative |
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
| `!` | `expr!` | Unwrap Result/Option; propagate the failure (effect fn / test / a pure fn whose return resolves to Result, Option, or `T!` — C-211, ADR-0002 Phase 1) |
| `??` | `expr ?? fallback` | Unwrap or use fallback value. fallback は**遅延評価**(none/err のときだけ)。**最高位に結合**: `a ?? 1 + 2` = `(a ?? 1) + 2` — C 系 `?:`(ほぼ最下位)と逆なので算術と混ぜるときは括弧推奨。fallback は同じ行に置く(行またぎは E038、複数行は括弧 + `??` 後置) |
| `?` | `expr?` | Convert Result to Option (err becomes none) |
| `?.` | `expr?.field` | Optional chaining (Option[Record] to Option[FieldType]) — **Option 専用**。Result には専用診断で拒否(`(r?)?.x` と合成する) |

```almide
import fs

type User = { name: String }
fn risky_fn() -> Result[Int, String] = ok(1)

effect fn demo(m: Map[String, String], path: String, user: Option[User]) -> Result[String, String] = {
  let value = map.get(m, "key") ?? "default"
  let content = fs.read_text(path)!
  let name = user?.name
  let opt = risky_fn()?
  ok("${value} ${content} ${name ?? "?"} ${opt ?? 0}")
}
```

テスト: `spec/lang/unwrap_operators_test.almd`, `spec/lang/operator_test.almd`

### 8.6 Bitwise Operations

Bitwise operations are functions, not operators:

```almide
test "bitwise functions" {
  assert_eq(int.band(6, 3), 2)      // AND
  assert_eq(int.bor(6, 3), 7)       // OR
  assert_eq(int.bxor(6, 3), 5)      // XOR
  assert_eq(int.bnot(0), -1)        // NOT
  assert_eq(int.bshl(1, 3), 8)      // shift left
  assert_eq(int.bshr(8, 3), 1)      // shift right
}
```

### 8.7 String Concatenation

```almide
test "+ concatenates" {
  assert_eq("hello" + " " + "world", "hello world")   // string concatenation with +
  assert_eq([1, 2] + [3, 4], [1, 2, 3, 4])            // list concatenation with +
}
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

```almide
fn public_fn() -> Int = 42              // public (default)
mod fn internal_fn() -> Int = 42        // project-internal
local fn private_fn() -> Int = 42       // file-private
```

Modifier order: `[local|mod]? effect? fn`

```almide
local effect fn helper() -> Result[Unit, String] = ok(())
mod fn utility(x: Int) -> Int = x * 2
```

Visibility also applies to types and top-level lets:

```almide
local type InternalState = { count: Int }
mod let CACHE_SIZE = 256
```

テスト: `spec/lang/visibility_test.almd`

---

## 10. Comments

### 10.1 Line Comments

```almide
// This is a line comment
let x = 42  // inline comment
```

### 10.2 Block Comments

Block comments are nestable:

```almide
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

```almide
test "built-ins" {
  let s = "hi"
  println(s)              // print line to stdout
  eprintln(s)             // print line to stderr (debug only)
  assert_eq(1, 1)         // assert equal (test blocks)
  assert_ne(1, 2)         // assert not equal (test blocks)
  assert(true)            // assert true (test blocks)
}
```

String representation is not a built-in function: a type declares the `Repr`
convention and its value renders through interpolation (`"${x}"`, ALS-R2), or
it defines `fn T.repr(x: T) -> String` and calls `x.repr()`.

There is no `print` function -- use `println` for all output.

テスト: `spec/lang/prelude_test.almd`

---

## 12. Entry Point

```almide
import process

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
| `trait_impl_test.almd` | Convention methods + protocols |
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
