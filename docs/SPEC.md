# Almide Language Specification v0.8

**"Not a language for writing freely, but a language for converging correctly."**

---

## 0. Design Philosophy

### Core Thesis

The essence of language design for LLMs is not maximizing expressiveness, but **minimizing the set of valid candidates at each generation step**.

### Four Pillars

| Principle | Definition |
|---|---|
| **Predictable** | The "next valid syntax, API, and semantics" can be narrowed down tightly at each generation step |
| **Local** | The information needed to understand or modify a given location is as close as possible |
| **Repairable** | When errors occur, the compiler returns near-unique fix candidates in few steps |
| **Compact** | High semantic density with low syntactic noise. Strict yet concise |

### 7 Design Principles

1. **Canonicity** -- There should be, in principle, only one primary way to express the same meaning
2. **Surface Semantics** -- Side effects, fallibility, optionality, and mutability must appear in the syntax or type
3. **Local Reasoning** -- The meaning of a function or expression should be largely understandable from nearby syntax alone
4. **Incremental Completion** -- Incomplete code is legal; one can make progress by filling typed holes
5. **Repair-First** -- The compiler should be a repair tool, not a rejection tool; diagnostics are structured
6. **Vocabulary Economy** -- The standard library has a consistent vocabulary with no synonyms
7. **No Magic** -- Mechanisms that change meaning at runtime, context-dependent DSLs, and implicit type conversions are prohibited

### Trade-offs (Intentionally Sacrificed)

- Writing freedom for experts
- Cultural "language feel" and ergonomic DSLs
- Metaprogramming power (no macros, no reflection)
- Operator overloading and implicit conversions
- Multiple return styles and lambda notations
- Ad-hoc polymorphism via implicit instance resolution

**Goal: High conciseness + Low freedom**

---

## 1. Lexical Specification

### 1.1 Identifiers

```
Identifier ::= [a-z_][a-zA-Z0-9_]*
```

A single trailing `?` is allowed for predicates:

```
Name ::= Identifier | Identifier "?"
```

- `name?` -- **Bool predicate only** (return type must be `Bool`; compiler enforced)

### 1.2 Type Names

```
TypeName ::= [A-Z][a-zA-Z0-9]*
```

### 1.3 Literals

```
IntLiteral       ::= [0-9]+ | "0x" [0-9a-fA-F]+     // decimal or hex
FloatLiteral     ::= [0-9]+ "." [0-9]+ ([eE] [+-]? [0-9]+)?
StringLiteral    ::= '"' (char | "\n" | "\t" | "\r" | "\\" | "\"" | "\$")* '"'
InterpolatedStr  ::= '"' (char | "${" Expr "}")* '"'
SingleQuoteStr   ::= "'" (char | "\'" | "\\")* "'"   // escapes, interpolation via ${expr}
HeredocStr       ::= '"""' ... '"""'                   // multiline, indent-stripped
RawHeredocStr    ::= 'r"""' ... '"""'                  // no escapes, no interpolation
RawStr           ::= 'r"' ... '"'                      // no escapes, no interpolation
BoolLiteral      ::= "true" | "false"
```

There is **no** null literal. Absence is represented by `none` (a constructor of `Option[T]`).

Double-quote strings support interpolation via `${expr}` and backslash escapes. Single-quote strings also support interpolation and the escape sequences `\'`, `\\`, `\n`, `\t`, `\r`. Raw strings and raw heredocs support neither interpolation nor escapes.

Heredoc strings strip leading whitespace based on minimum indent of non-empty lines. The first line (if blank after `"""`) and the last line (if whitespace-only before `"""`) are dropped.

Numeric literals support `_` as a visual separator (e.g., `1_000_000`).

### 1.4 Comments

```
// line comment (to end of line)
/* block comment (nestable) */
```

Block comments nest: `/* outer /* inner */ still outer */` is valid.

### 1.5 Keywords (42)

```
module  import  type    trait   impl    for     in      fn
let     var     if      then    else    match   ok      err
some    none    try     do      todo    unsafe  true    false
not     and     or      strict  pub     effect  deriving test
async   await   guard   break   continue while  local   mod
newtype fan
```

### 1.6 Operators and Delimiters

```
Operators:   +  -  *  **  /  %  ^  ==  !=  <  <=  >  >=  |>  ..  ..=
Unary:       -  not
Logical:     and  or
Assignment:  =
Arrows:      ->  =>
Delimiters:  (  )  {  }  [  ]  ,  .  :  ;  |  _  @  ...
```

- `^` is XOR (integer)
- `+` is overloaded: addition for numbers, concatenation for strings and lists
- `**` is exponentiation (right-associative)
- `..` is exclusive range, `..=` is inclusive range
- `...` is spread (in records)
- `_` is wildcard (in match patterns) or placeholder (in pipe arguments)
- `@` is used for extern annotations

---

## 2. Statement Separators

**Newlines separate statements.** Semicolons are used only to place multiple statements on a single line.

```
let x = 1
let y = 2
let a = 1; let b = 2
```

### 2.1 Line Continuation Rules

A newline is ignored and the statement continues when:

**The line ends with:**
- Binary operators: `+`, `-`, `*`, `/`, `%`, `==`, `!=`, `<=`, `>=`, `<`, `>`, `and`, `or`, `|>`
- Delimiters: `,`, `.`, `:`
- Opening brackets: `(`, `{`, `[`
- Arrows: `->`, `=>`
- Assignment: `=`
- Keywords: `if`, `then`, `else`, `match`, `try`, `do`, `not`, `|`

**The next line starts with:**
- `.` (method chaining)
- `|>` (pipe)

```
let result = items
  .filter((x) => x > 0)
  .map((x) => x * 2)

text
  |> string.trim
  |> string.split(",")
```

---

## 3. Syntactic Categories

```
Program   ::= ImportDecl* TopDecl*

TopDecl   ::= TypeDecl | TraitDecl | ImplDecl | FnDecl
            | TopLetDecl | StrictDecl | TestDecl

Stmt      ::= LetStmt | VarStmt | AssignStmt | GuardStmt | Expr

Expr      ::= Literal | Name | InterpolatedStr
            | RecordExpr | SpreadExpr | ListExpr | MapExpr
            | CallExpr | MemberExpr | IndexExpr
            | PipeExpr | BinaryExpr | UnaryExpr
            | IfExpr | MatchExpr
            | ForInExpr | WhileExpr | DoExpr | FanExpr
            | BlockExpr | LambdaExpr
            | HoleExpr | TodoExpr | TryExpr | AwaitExpr
            | RangeExpr | TupleExpr | UnsafeExpr
            | "(" Expr ")"
```

---

## 4. Modules and Imports

Package identity is declared in `almide.toml`. No `module` declaration in source files.

### 4.1 Import Declaration

```
ImportDecl ::= "import" ModulePath
             | "import" ModulePath "as" Ident
             | "import" ModulePath "." "{" NameList "}"

ModulePath ::= Ident ( "." Ident )*
```

Examples:
```
import fs                       // stdlib module
import json                     // stdlib module (not auto-imported)
import mylib                    // user package
import mylib.parser             // sub-module import
import mylib as m               // alias: m.hello()
import self as app              // self-alias for the current package
import mylib.{Parser, Lexer}    // selective import
```

**Prohibited: wildcard imports.** `import fs.*` is a compile error.

### 4.2 Auto-Imported Modules (Prelude)

The following modules are available without `import`:

```
string  list  int  float  math  map  result  option  value  set
```

All other stdlib modules require explicit `import`:

```
fs  env  io  process  json  random  regex  datetime  http
log  testing  error
```

Bundled stdlib packages (pure Almide): `args`, `path`, `time`, `encoding`, `hash`, `url`, `csv`.

### 4.3 Prelude Types

Implicitly imported types and constructors:
- Primitives: `Int`, `Float`, `Bool`, `String`, `Unit`, `Path`
- Collections: `List[T]`, `Map[K, V]`, `Set[T]`
- Error handling: `Option[T]`, `Result[T, E]`
- Constructors: `some(x)`, `none`, `ok(x)`, `err(x)`
- Booleans: `true`, `false`
- Boundary: `Json`, `Value`

### 4.4 Visibility Modifiers

```
fn pub_fn() -> String = ...       // public (default)
mod fn internal() -> String = ... // same project only (pub(crate) in Rust)
local fn helper() -> String = ... // same file only (private)
```

Visibility applies to `fn`, `type`, and `let` declarations:
```
local type Internal = { data: String }
mod let THRESHOLD = 100
```

---

## 5. Type Declarations

### 5.1 Generics -- `[]` Notation

**Type arguments use `[]`.** `<>` is reserved exclusively for comparison operators.

```
GenericParams ::= "[" TypeParam ( "," TypeParam )* "]"
TypeParam     ::= TypeName
```

```
Result[List[Map[String, Int]], Error]
fn map[U](xs: List[T], f: Fn(T) -> U) -> List[U]
```

### 5.2 Record Types

```
type User = {
  id: Int,
  name: String,
}

type Pair[A, B] = {
  first: A,
  second: B,
}
```

### 5.3 Variant Types

Three variant case forms: unit, tuple-style, and record-style.

```
type Token =
  | Word(String)
  | Number(Int)
  | Eof

type Shape =
  | Circle(Float)
  | Rect{ width: Float, height: Float }
  | Point
```

Inline variant (without leading `|`):

```
type Direction = North | South | East | West
```

### 5.4 deriving

```
DerivingClause ::= "deriving" TypeName
```

Automatically derives `From` trait implementations for variant types:

```
type ConfigError =
  | Io(IoError)
  | Parse(ParseError)
  | Decode(DecodeError)
  deriving From

// Equivalent to:
// impl From[IoError] for ConfigError { fn from(e: IoError) -> ConfigError = Io(e) }
// impl From[ParseError] for ConfigError { fn from(e: ParseError) -> ConfigError = Parse(e) }
// impl From[DecodeError] for ConfigError { fn from(e: DecodeError) -> ConfigError = Decode(e) }
```

### 5.5 Protocols

```
ProtocolDecl ::= "protocol" TypeName "{" ProtocolMethod* "}"
ProtocolMethod ::= ["effect"] "fn" Ident "(" Params ")" "->" TypeExpr
```

Protocols define sets of required convention methods. Types declare satisfaction with `: ProtocolName`.

```
protocol Serializable {
  fn serialize(a: Self) -> String
  fn deserialize(raw: String) -> Result[Self, String]
}

type Config: Serializable = { key: String, value: String }
fn Config.serialize(c: Config) -> String = c.key + "=" + c.value
fn Config.deserialize(raw: String) -> Result[Config, String] = ...
```

`Self` is a placeholder for the implementing type. Built-in conventions (Eq, Repr, Ord, Hash, Codec) are protocols.

Protocols can be used as generic bounds:

```
fn show[T: Serializable](item: T) -> String = item.serialize()
```

No dynamic dispatch — all protocol-bounded generics are monomorphized at compile time.

### 5.6 newtype

```
type UserId = newtype Int
type Email = newtype String
```

- `UserId` and `Int` are not implicitly convertible
- Wrap: `UserId(42)` / Unwrap: `id.value`
- Zero runtime cost

### 5.7 Type Application

```
List[String]
Result[User, ParseError]
Map[String, List[Int]]
```

### 5.8 Tuple Types

```
(Int, String)              // tuple type
(Int, String, Bool)        // 3-tuple
```

Access via `.0`, `.1`, etc.

### 5.9 Function Types

```
Fn(Int) -> String
Fn(Int, Int) -> Bool
```

---

## 6. Traits and Implementations

### 6.1 trait

```
TraitDecl ::= "trait" TypeName GenericParams? "{" TraitMethod* "}"
TraitMethod ::= "async"? "effect"? "fn" Name "(" ParamList ")" "->" TypeExpr
```

```
trait Iterable[T] {
  fn map[U](self, f: Fn(T) -> U) -> Self[U]
  fn filter(self, f: Fn(T) -> Bool) -> Self[T]
  fn fold[U](self, init: U, f: Fn(U, T) -> U) -> U
}

trait Storage[T] {
  effect fn save(self, item: T) -> Result[Unit, IoError]
  effect fn load(self, id: String) -> Result[T, IoError]
}
```

### 6.2 impl

```
ImplDecl ::= "impl" TypeName GenericParams? "for" TypeName "{" FnDecl* "}"
```

```
impl Iterable[T] for List[T] {
  fn map[U](self, f: Fn(T) -> U) -> List[U] = _
  fn filter(self, f: Fn(T) -> Bool) -> List[T] = _
}
```

### 6.3 Built-in Protocols

- **Eq** and **Hash** are compiler-derived automatically from type structure. No `deriving` needed.
- `deriving From` is the only explicit deriving directive (for error type conversions).

---

## 7. Basic Type Environment

### Primitives

```
Int  Float  Bool  String  Path  Unit
```

`Bytes` is represented as `List[Int]`.

### Collections

```
List[T]  Map[K, V]  Set[T]
```

### Error Handling

```
Option[T]   -- some(x) | none
Result[T, E] -- ok(x) | err(e)
```

### Boundary Types

```
Json  Value
```

Used as receivers for external input. Requires explicit conversion before use in domain logic.

---

## 8. Function Declarations

```
FnDecl ::= Visibility? "async"? "effect"? "fn" Name GenericParams?
            "(" ParamList? ")" "->" TypeExpr "=" Expr

Visibility ::= "local" | "mod"        // default is public
ParamList  ::= Param ( "," Param )*
Param      ::= Identifier ":" TypeExpr ( "=" Expr )?   // default value optional
```

Modifier order: `[local|mod]? async? effect? fn`

### 8.1 Principles

- Argument types are required
- Return type is required
- The body is a single expression (after `=`)
- `effect fn` marks functions with side effects
- `fan { }` for concurrent execution (only inside `effect fn`)

```
fn add(x: Int, y: Int) -> Int = x + y

effect fn greet(name: String) -> Result[Unit, String] = {
  println("Hello, ${name}!")
  ok(())
}
```

### 8.2 Default Arguments

Parameters may have default values. All parameters after the first default must also have defaults.

```
fn connect(host: String, port: Int = 8080, secure: Bool = false) -> Connection = _
```

### 8.3 Named Arguments

Arguments can be named at the call site:

```
connect("localhost")
connect("localhost", port: 443, secure: true)
connect(host: "localhost", secure: true)
```

Named arguments after positional ones are allowed. Positional arguments after named ones are not.

### 8.4 Predicate Functions

Functions ending in `?` must return `Bool`:

```
fn empty?(xs: List[Int]) -> Bool = list.len(xs) == 0
fn tracked?(index: Index, path: Path) -> Bool = _
```

### 8.5 `effect fn` -- Explicit Side Effects

Calling an `effect fn` from a non-`effect` function is a **compile error**, not a warning.

```
fn pure_fn() -> String =
  fs.read_text("file.txt")    // Compile error: cannot call effect fn from non-effect fn
```

The `effect` system is a **search space reducer for code generation**: a pure function can only call other pure functions, shrinking the set of valid completions.

### 8.6 Top-Level let -- Module-Scope Constants

```
let PI = 3.14159265358979323846
let MAX_RETRIES = 3
let GREETING = "Hello, world"
```

Evaluated at compile time when possible. Numeric and simple expressions become `const`; String and complex expressions use `LazyLock<T>` in Rust codegen.

### 8.7 Extern Annotations

For FFI bindings to target-specific functions:

```
@extern(rust, "std::fs", "read_to_string")
@extern(ts, "fs", "readFileSync")
effect fn read_text(path: String) -> Result[String, String] = _
```

---

## 9. Statements

### 9.1 let / var

```
let x = 1                   // immutable
let x: Int = 1              // with type annotation
var y = 2                   // mutable
y = y + 1                   // reassign (var only)
```

### 9.2 Destructuring

```
let { name, age } = user    // record destructure (one level only)
```

- Immutable bindings only (no `var` destructure)
- Nested destructuring is not allowed
- Renaming is not allowed

### 9.3 guard

```
guard cond else expr
```

Precondition checking with early exit. When the condition is false, the else expression is returned.

```
effect fn validate(x: Int) -> Result[Int, String] = {
  guard x > 0 else err("must be positive")
  guard x < 1000 else err("too large")
  ok(x * 2)
}
```

In `do` loops, `guard cond else ok(())` acts as a break condition:

```
do {
  guard current != "NONE" else ok(())
  let data = fs.read_text(current)
  current = next
}
```

### 9.4 Reassignment

Only allowed for identifiers bound with `var`:

```
var count = 0
count = count + 1
xs[0] = "updated"           // index write (var only)
m["key"] = value             // map write (var only)
```

---

## 10. Expressions

### 10.1 if Expression

```
if cond then expr else expr
```

- `else` is **mandatory**. `if` without `else` is a syntax error.
- The condition must be `Bool`. No truthiness.
- Chaining: `if a then x else if b then y else z`

### 10.2 match Expression

```
match subject {
  Pattern => expr,
  Pattern if guard_cond => expr,
  _ => expr,
}
```

**match must be exhaustive** (enforced by the type checker).

Guards take a `Bool` expression after `if`. When the guard is false, the next arm is tried.

### 10.3 Patterns

```
Pattern ::= "_"                                      // wildcard
           | Identifier                               // bind
           | Literal                                  // int, float, string, bool
           | "some" "(" Pattern ")"                   // Option
           | "none"
           | "ok" "(" Pattern ")"                     // Result
           | "err" "(" Pattern ")"
           | TypeName                                 // unit constructor
           | TypeName "(" Pattern ("," Pattern)* ")"  // tuple constructor
           | TypeName "{" FieldPattern* ".."? "}"     // record constructor
           | "(" Pattern "," Pattern+ ")"             // tuple
```

`FieldPattern ::= Ident (":" Pattern)?`

Record patterns support `..` for partial matching:

```
match shape {
  Circle(r) => 3.14 * r * r,
  Rect{ width, height } => width * height,
  Rect{ width, .. } => width,
  Point => 0.0,
}
```

### 10.4 Lambdas

```
(x) => expr
(x, y) => expr
(x: Int) => x + 1
items.map((x) => x * 2)
```

One form only. Lambda parameters may optionally include type annotations.

### 10.5 Block Expression

The last expression in a block is its value:

```
{
  let x = 1
  let y = 2
  x + y
}
```

### 10.6 for...in

```
for item in items {
  println(item)
}

for (k, v) in map.entries(config) {
  println(k + " = " + v)
}

for i in 0..10 {
  println(int.to_string(i))
}
```

Iterates over a list. The loop body is `Unit`-typed. Use `for...in` for collection iteration.

### 10.7 while

```
var i = 0
while i < 10 {
  println(int.to_string(i))
  i = i + 1
}
```

Loops while the condition is true. The loop body is `Unit`-typed.

### 10.8 do Block

Two roles: error propagation block and loop with structured break.

**Error propagation:**

```
effect fn load(path: String) -> Result[Config, AppError] =
  do {
    let text = fs.read_text(path)     // auto try: Result unwrapped
    let raw = json.parse(text)        // auto try: Result unwrapped
    decode[Config](raw)
  }
```

Inside a `do` block, expressions returning `Result[T, E]` are automatically unwrapped. If error types differ, conversion via `From` is attempted.

**Loop with guard:**

```
do {
  guard current != "NONE" else ok(())   // break condition
  let data = fs.read_text(path)
  current = next
}
```

When a `do` block contains `guard`, it becomes a loop. `guard cond else expr` is the only way to exit.

### 10.9 fan (Structured Concurrency)

```
fan { expr1; expr2; expr3 }
```

Runs expressions concurrently. Returns results as a tuple. Only valid inside `effect fn`.

Rules:
- If any expression returns `Err`, the entire `fan` fails and siblings are cancelled
- No `let`/`var`/`for`/`while` inside `fan` blocks -- only expressions
- No `var` capture from outer scope (prevents data races)

Library forms:
- `fan.map(xs, f)` -- parallel map over a collection
- `fan.race(thunks)` -- first to complete wins, rest cancelled

### 10.10 Pipe

```
text |> string.trim |> string.split(",")
```

`x |> f` is equivalent to `f(x)`.

**Placeholder `_`** for multi-argument functions:

```
xs |> list.filter(_, (x) => x > 0)
text |> string.split(_, ",")
```

`_` specifies where the piped value is inserted. Multiple `_` in a single call is a compile error.

**Pipe into match:**

```
value |> match {
  some(x) => x,
  none => "default",
}
```

### 10.11 UFCS (Uniform Function Call Syntax)

`f(x, y)` and `x.f(y)` are equivalent. The compiler resolves automatically.

```
string.trim(text)       // canonical form
text.trim()             // UFCS form -- equivalent

string.split(text, ",")
text.split(",")         // equivalent
```

Resolution: when `x.f(args...)` is called, the compiler looks for `f(x, args...)` in scope.

### 10.12 Range

```
0..5            // [0, 1, 2, 3, 4]    exclusive end
1..=5           // [1, 2, 3, 4, 5]    inclusive end
for i in 0..n { ... }   // no list allocation (optimized)
```

### 10.13 Record and Spread

```
let alice = { name: "alice", age: 30 }
let bob = { ...alice, name: "bob" }       // age inherited from alice
{ name }                                   // shorthand: { name: name }
```

### 10.14 List

```
[1, 2, 3]
[]                        // empty list (type inferred from context)
xs[0]                     // index access
xs[i] = value             // index write (var only)
```

### 10.15 Map

```
["a": 1, "b": 2]         // map literal
[:]                       // empty map (requires type annotation)
let m: Map[String, Int] = [:]
m["key"]                  // index access (returns Option[V])
m["key"] = value          // index write (var only)
```

### 10.16 Tuple

```
(1, "hello")              // tuple literal
let pair = (1, "hello")
pair.0                    // index access: 1
pair.1                    // "hello"
```

### 10.17 String Interpolation

```
let name = "world"
let msg = "hello ${name}, 1+1=${1 + 1}"
```

Works in double-quote strings, single-quote strings, and heredocs (but not raw strings).

### 10.18 Hole / Todo / Try / Await

```
fn parse(text: String) -> Ast = _                     // hole (type-checked stub)
fn optimize(ast: Ast) -> Ast = todo("implement later") // todo with message
let text = try fs.read_text(path)                      // unwrap Result, propagate error
let (a, b) = fan { task_a(); task_b() }               // concurrent execution
```

`_` (hole) and `todo(msg)` accept any expected type. The compiler reports the expected type, available variables, and suggestions.

### 10.19 unsafe Block

```
fn technically_pure(x: Int) -> Int =
  unsafe { fs.read_text(cache_path) }    // bypass effect boundary
```

`unsafe` surfaces "normal rules are being broken here."

---

## 11. Operators

### Precedence (lowest to highest)

| Level | Operators | Associativity |
|-------|-----------|---------------|
| 1 | `\|>` (pipe) | left |
| 2 | `or` | left |
| 3 | `and` | left |
| 4 | `==` `!=` `<` `<=` `>` `>=` | left |
| 5 | `..` `..=` (range) | none |
| 6 | `+` `-` (additive) | left |
| 7 | `*` `/` `%` `^` (multiplicative, XOR) | left |
| 8 | `**` (power) | right |
| 9 | `-` `not` (unary) | prefix |
| 10 | `.` `()` `[]` (postfix) | left |

- Assignment (`=`) is a statement, not an operator
- Operator overloading is prohibited -- built-in types only
- `&&` and `||` are rejected with hints to use `and`/`or`

### Operator Semantics

| Operator | Meaning |
|----------|---------|
| `+` | Addition (Int, Float) or concatenation (String, List) |
| `-` `*` `/` `%` | Arithmetic |
| `**` | Exponentiation |
| `^` | Bitwise XOR (Int) |
| `==` `!=` | Deep equality (all types except Fn) |
| `<` `<=` `>` `>=` | Comparison |
| `and` `or` `not` | Boolean logic |
| `\|>` | Pipe |
| `..` `..=` | Range (exclusive / inclusive) |

In Rust codegen, `==`/`!=` emit the `almide_eq!` macro for deep structural equality. In TS/JS codegen, they emit `__deep_eq()`.

---

## 12. Error Model

### 12.1 Three-Layer Strategy

| Layer | Mechanism | Use Case |
|---|---|---|
| **Normal failure** | `Result[T, E]` | parse, validate, I/O, lookup |
| **Programmer error** | `panic` | unreachable, invariant violations |
| **Testing** | `assert_eq`, `assert` | test assertions |

Exceptions **do not exist**. There is no `throw`/`catch`.

### 12.2 try

`try` unwraps a `Result[T, E]` or `Option[T]`, propagating the error to the enclosing function:

```
// In a function returning Result[R, E]:
let value = try some_result    // unwraps T, propagates E

// In a function returning Option[R]:
let value = try some_option    // unwraps T, propagates none
```

No automatic conversion between `Result` and `Option`. Use explicit conversion:

```
let value = try opt.ok_or("missing")
```

### 12.3 Error Conversion with deriving From

```
type AppError =
  | Io(IoError)
  | Parse(ParseError)
  deriving From

effect fn load(path: String) -> Result[Config, AppError] =
  do {
    let text = fs.read_text(path)    // IoError -> AppError via From
    let raw = json.parse(text)       // ParseError -> AppError via From
    decode[Config](raw)
  }
```

---

## 13. Holes and Incomplete Code

**Core feature of the language.** Allows incremental development.

```
fn parse(text: String) -> Ast = _
fn optimize(ast: Ast) -> Ast = todo("implement constant folding")
```

### Compiler Obligation

When a hole is found, the compiler returns:
- Expected type `T`
- Available variables in scope and their types
- Function candidates that can return the expected type
- Suggested expressions

---

## 14. Concurrency: `fan`

### 14.1 fan Block

`fan { }` runs expressions concurrently. Only valid inside `effect fn`.

```
effect fn main() -> Result[Unit, String] = {
  let (a, b) = fan {
    fetch_user(1)
    fetch_posts(1)
  }
  println("${a}, ${b}")
  ok(())
}
```

Results are returned as a tuple. If any expression returns `Err`, the entire `fan` fails and siblings are cancelled.

### 14.2 fan.map / fan.race

```
let results = fan.map(urls, (url) => fetch(url))   // parallel map
let first = fan.race([task_a, task_b])              // first to complete wins
```

### 14.3 Rules

- `fan { }` only inside `effect fn` — pure functions cannot fork
- No `var` capture — only `let` bindings from outer scope (prevents data races)
- No unstructured `spawn` — all concurrency is scoped
- Same fail-fast semantics as `do` — first error cancels all siblings

---

## 15. Testing

### 15.1 test Declaration

```
test "description" {
  assert_eq(add(1, 2), 3)
  assert(x > 0)
  assert_ne(a, b)
}
```

Tests are top-level declarations in the same file as functions. No separate test files required (convention: `*_test.almd` for dedicated test files).

### 15.2 Assertion Functions

```
assert(cond: Bool)                    // fails if false
assert_eq(actual: T, expected: T)     // fails if actual != expected
assert_ne(actual: T, expected: T)     // fails if actual == expected
```

### 15.3 Running Tests

```bash
almide test                      # all .almd with test blocks (recursive)
almide test spec/lang/           # directory
almide test spec/lang/expr_test.almd  # single file
almide test --run "pattern"      # filter by name
```

---

## 16. Strict Mode

```
strict types      // require all type annotations
strict effects    // fully check effect propagation
```

Declared at the top of a file. Enables stricter compiler checking.

---

## 17. Naming Conventions

### Predicates

`?` suffix on function names: `is_empty?`, `exists?`, `contains?`. Return type must be `Bool`.

The `is_` prefix convention is used for predicates in the stdlib: `string.is_empty?(s)`, `string.is_digit?(s)`.

### Stdlib Naming Rules

| Convention | Rule | Example |
|---|---|---|
| Module prefix | `module.function()` canonical form | `string.len(s)`, `list.get(xs, i)` |
| Predicate suffix | `?` for boolean-returning functions | `string.is_empty?(s)`, `fs.exists?(path)` |
| No synonyms | One name per operation | `len` not `length`/`size`/`count` |
| Symmetric pairs | Matching names for inverses | `split`/`join`, `to_string`/`to_int` |
| Return type consistency | Fallible lookups return `Option`, I/O returns `Result` | `list.get() -> Option[T]` |

---

## 18. Standard Library

381 native functions across 22 native modules, plus 10 bundled modules (pure Almide). Runtime implementation: 100%.

### 18.1 Auto-Imported Modules

**string** (41 functions):
`trim`, `trim_start`, `trim_end`, `split`, `join`, `len`, `lines`, `pad_left`, `pad_right`, `starts_with?`, `ends_with?`, `slice`, `to_bytes`, `from_bytes`, `contains`, `to_upper`, `to_lower`, `to_int`, `replace`, `char_at`, `chars`, `index_of`, `repeat`, `count`, `reverse`, `is_empty?`, `is_digit?`, `is_alpha?`, `is_alphanumeric?`, `is_whitespace?`, `strip_prefix`, `strip_suffix`, `capitalize`, `is_upper?`, `is_lower?`, `codepoint`, `from_codepoint`, `pad_end`, `replace_first`, `last_index_of`, `to_float`

**list** (54 functions):
`len`, `get`, `get_or`, `first`, `last`, `sort`, `sort_by`, `reverse`, `contains`, `index_of`, `any`, `all`, `each`, `map`, `flat_map`, `filter`, `find`, `fold`, `enumerate`, `zip`, `flatten`, `take`, `drop`, `chunk`, `unique`, `join`, `sum`, `product`, `min`, `max`, `is_empty?`, `push`, `pop`, `insert`, `remove`, `concat`, `slice`, `range`, `count`, `find_index`, `partition`, `scan`, `window`, `zip_with`, `unzip`, `group_by`, `frequencies`, `intersperse`, `reduce`, `take_while`, `drop_while`, `dedup`, `rotate`, `transpose`

**map** (16 functions):
`new`, `get`, `get_or`, `set`, `contains`, `remove`, `merge`, `keys`, `values`, `len`, `entries`, `from_list`, `is_empty?`, `map_values`, `filter`, `fold`

**int** (21 functions):
`to_string`, `to_hex`, `parse`, `parse_hex`, `abs`, `min`, `max`, `band`, `bor`, `bxor`, `bshl`, `bshr`, `bnot`, `wrap_add`, `wrap_mul`, `rotate_right`, `rotate_left`, `to_u32`, `to_u8`, `clamp`, `signum`

**float** (16 functions):
`to_string`, `to_int`, `from_int`, `round`, `floor`, `ceil`, `abs`, `sqrt`, `parse`, `min`, `max`, `clamp`, `is_nan?`, `is_infinite?`, `signum`, `truncate`

**math** (21 functions):
`min`, `max`, `abs`, `pow`, `pi`, `e`, `sin`, `cos`, `tan`, `log`, `exp`, `sqrt`, `asin`, `acos`, `atan`, `atan2`, `log2`, `log10`, `floor`, `ceil`, `round`

**result** (9 functions):
`map`, `map_err`, `and_then`, `unwrap_or`, `is_ok?`, `is_err?`, `ok`, `err`, `flatten`

**option** (9 functions):
`map`, `and_then`, `unwrap_or`, `ok_or`, `is_some?`, `is_none?`, `flatten`, `filter`, `zip`

**value** (19 functions):
Type-agnostic value operations for dynamic data handling.

**set** (auto-imported):
Set operations: `new`, `insert`, `contains`, `remove`, `union`, `intersection`, `difference`, `len`, `is_empty?`, `to_list`, `from_list`.

### 18.2 Import-Required Modules

**fs** (24 functions, all effect):
`read_text`, `read_bytes`, `read_lines`, `write`, `write_bytes`, `append`, `mkdir_p`, `exists?`, `is_dir?`, `is_file?`, `remove`, `list_dir`, `copy`, `rename`, `remove_dir`, `metadata`, `glob`, `walk_dir`, `read_dir`, `create_dir`, `symlink`, `read_link`, `canonical`, `temp_dir`

**env** (9 functions, effect):
`unix_timestamp`, `millis`, `args`, `get`, `set`, `cwd`, `sleep_ms`, `home_dir`, `hostname`

**io** (3 functions, effect):
`read_line`, `print`, `read_all`

**process** (6 functions, effect):
`exec`, `exec_status`, `exit`, `stdin_lines`, `spawn`, `pid`

**json** (36 functions):
`parse`, `stringify`, `stringify_pretty`, `get`, `get_string`, `get_int`, `get_float`, `get_bool`, `get_array`, `keys`, `to_string`, `to_int`, `as_string`, `as_int`, `as_float`, `as_bool`, `as_array`, `object`, `s`, `i`, `f`, `b`, `null`, `array`, `from_string`, `from_int`, `from_float`, `from_bool`, `from_map`, `is_null?`, `is_string?`, `is_int?`, `is_array?`, `is_object?`, `len`, `merge`

**random** (4 functions, effect):
`int`, `float`, `choice`, `shuffle`

**datetime** (21 functions):
Date/time operations: `now`, `year`, `month`, `day`, `hour`, `minute`, `second`, `weekday`, `to_iso`, `from_parts`, etc.

**log** (8 functions, effect):
`debug`, `info`, `warn`, `error`, `trace`, `set_level`, `with_context`, `flush`

**testing** (7 functions):
`assert`, `assert_eq`, `assert_ne`, `assert_ok`, `assert_err`, `assert_some`, `assert_none`

**error** (3 functions):
`message`, `chain`, `context`

**regex** (8 functions):
`match?`, `full_match?`, `find`, `find_all`, `replace`, `replace_first`, `split`, `captures`

**http** (26 functions, effect):
HTTP client operations.

### 18.3 Bundled Modules (Pure Almide)

| Module | Functions |
|--------|-----------|
| args | 6 |
| path | 7 |
| time | 20 |
| encoding | 10 |
| hash | 3 |
| url | 21 |
| csv | 9 |

### 18.4 Built-in Functions

Available everywhere without import:

```
println(s: String)              // print line to stdout
eprintln(s: String)             // print line to stderr
assert_eq(a: T, b: T)          // assert equal
assert_ne(a: T, b: T)          // assert not equal
assert(cond: Bool)              // assert true
```

There is no `print` function (use `io.print` for no-newline output). `println` requires `String` -- no implicit conversion.

---

## 19. Entry Point

```
effect fn main(args: List[String]) -> Result[Unit, AppError] = {
  let cmd = list.get(args, 1)
  match cmd {
    some("run") => do_something(),
    some(other) => err(UnknownCommand(other)),
    none => err(NoCommand),
  }
}
```

`args[0]` is the program name, `args[1]` is the first argument. The runtime calls `main(args)`.

---

## 20. Codegen Architecture

### 20.1 Multi-Target

Almide compiles to multiple targets:

| Target | Status | Approach |
|--------|--------|----------|
| **Rust** | Production | Full ownership analysis, borrow/clone passes |
| **TypeScript** | Production | Result erasure (ok(x)->x, err(e)->throw) |
| **JavaScript** | Production | Same as TypeScript, different module system |
| **WASM** | Production | Via Rust target |
| **Go** | Planned | Stub pipeline |
| **Python** | Planned | Stub pipeline |

### 20.2 Codegen v3 Pipeline

Three-layer architecture: IR -> Nanopass -> Templates.

```
IrProgram (typed IR)
    |
Layer 1: Core IR normalization (target-agnostic)
    |
Layer 2: Semantic Rewrite (target-specific Nanopass pipeline)
    |
Layer 3: Template Renderer (TOML-driven syntax output)
    |
Target source code
```

**Rust pipeline passes:**
1. TypeConcretization -- resolve generic types
2. BorrowInsertion -- insert `&` references
3. CloneInsertion -- insert `.clone()` calls
4. StdlibLowering -- module calls to named calls with arg decoration
5. ResultPropagation -- insert `?` for effect fn calls
6. BuiltinLowering -- assert_eq, println, etc. to Rust macros
7. FanLowering -- fan blocks to tokio::join!/spawn

**TypeScript/JavaScript pipeline passes:**
1. MatchLowering -- match expressions to if/else chains
2. ResultErasure -- ok(x)->x, err(e)->throw, some(x)->x, none->null
3. ShadowResolve -- re-declarations to reassignment
4. FanLowering -- fan blocks to Promise.all

Templates are defined in TOML files (`codegen/templates/*.toml`), separating syntax from semantics. Adding a new target requires implementing passes and templates, not modifying the core emitter.

### 20.3 Cross-Target Semantics

| Feature | Rust | TypeScript/JavaScript |
|---------|------|---------------------|
| `Result[T, E]` | `Result<T, String>` | erased: ok(x)->x, err(e)->throw |
| `Option[T]` | `Option<T>` | erased: some(x)->x, none->null |
| `effect fn` | returns `Result<T, String>`, auto `?` | normal function |
| `==` / `!=` | `almide_eq!` macro (deep) | `__deep_eq()` (deep) |
| `+` on String | `format!` / owned concat | `+` operator |
| `+` on List | `[...a, ...b]` | `[...a, ...b]` |
| `fan { }` | `tokio::join!` | `Promise.all` |

---

## 21. Prohibitions

| # | Prohibited | Reason |
|---|---|---|
| 1 | Implicit type conversion | LLMs mix up types |
| 2 | Truthiness | Conditions must be `Bool` only |
| 3 | Operator overloading | Operator meaning must not change by type |
| 4 | Exceptions (`throw`/`catch`) | Control flow invisible |
| 5 | Multiple lambda notations | Generation distribution disperses |
| 6 | Internal DSLs | Strong context dependence |
| 7 | Wildcard imports | Source of names unknown |
| 8 | `null` | Unified under `Option[T]` |
| 9 | API aliases | More vocabulary = more hallucinations |
| 10 | `<>` generics | Ambiguous with comparison operators |
| 11 | Monkey patching / open classes | Runtime semantic changes invisible |
| 12 | Macros and reflection | Language surface must be fixed |
| 13 | `return` keyword | Last expression is always the value |
| 14 | `class` keyword | Use `type` for records and variants |

---

## 22. Compiler Diagnostics

### 22.1 One Error, One Root Cause

Cascading derived errors are suppressed. A single root cause is presented.

### 22.2 Structured Error Output

```json
{
  "kind": "type_mismatch",
  "location": { "file": "app.almd", "line": 12, "col": 5 },
  "expected": "Result[Config, IoError]",
  "actual": "String",
  "suggestions": ["ok(text)", "try parse_config(text)"]
}
```

### 22.3 Rejected Syntax Hints

The compiler recognizes common syntax from other languages and provides actionable hints:

```
'!' is not valid in Almide at line 5:12
  Hint: Use 'not x' for boolean negation, not '!x'.

'return' is not valid in Almide at line 12:5
  Hint: Use the last expression as the return value, or 'guard ... else' for early exit.

'||' is not valid in Almide at line 8:10
  Hint: Use 'or' for logical OR.

'&&' is not valid in Almide at line 8:10
  Hint: Use 'and' for logical AND.
```

### 22.4 Auto-Fix Candidates

- Missing imports
- Type conversion candidates
- Missing match arms
- Missing `effect` modifier
- Suggest `do` blocks when multiple `try` expressions appear

### 22.5 Official Formatter

```bash
almide fmt app.almd
```

- One AST yields exactly one formatted output
- Alphabetical import order
- Trailing commas included
- Fixed line-breaking rules

---

## 23. Typing Rules

### Variables

```
G(x) = T
-----------
G |- x : T
```

### let

```
G |- e : T
--------------------
G, x:T |- let x = e
```

### if

```
G |- c : Bool    G |- t : T    G |- e : T
-------------------------------------------
G |- if c then t else e : T
```

### Functions

```
G, x1:T1, ..., xn:Tn |- body : R
------------------------------------------
G |- fn f(x1:T1,...,xn:Tn) -> R = body
```

### Option / Result Constructors

```
G |- e : T                          G |- e : T
--------------------                --------------------
G |- some(e) : Option[T]           G |- ok(e) : Result[T, E]

G |- e : E
--------------------
G |- err(e) : Result[T, E]
```

### match

```
G |- e : T
G,p1 |- e1 : R  ...  G,pn |- en : R
exhaustive(p1...pn, T)
---------------------------------------
G |- match e { p1=>e1, ..., pn=>en } : R
```

### try

```
G |- e : Result[T, E]         G |- e : Option[T]
return_type = Result[R, E]    return_type = Option[R]
----------------------------   ----------------------------
G |- try e : T                 G |- try e : T
```

### do Block

```
return_type = Result[R, E]
G |- block : R   (with implicit try on Result[_, E] expressions)
----------------------------
G |- do { block } : Result[R, E]
```

### pipe

```
G |- x : A    G |- f : A -> B
-------------------------------
G |- x |> f : B
```

### spread

```
G |- base : { f1:T1, ..., fn:Tn }
G |- ei : Ti  (for overridden fields)
---------------------------------------
G |- { ...base, fi: ei, ... } : { f1:T1, ..., fn:Tn }
```

### fan

```
G |- e1 : Result[T1, E]    G |- e2 : Result[T2, E]    enclosing function is effect
------------------------------------------------------------------------------------
G |- fan { e1; e2 } : Result[(T1, T2), E]
```

### guard

```
G |- cond : Bool    G |- else_ : R
return_type = R
---------------------------------------
G |- guard cond else else_
```

### hole / todo

```
expected_type = T               expected_type = T
-------------------             -------------------
G |- _ : T                      G |- todo(msg) : T
```

---

## 24. Complete Example

```
import fs
import json

type Config = {
  root: String,
  bare: Bool,
  description: String,
}

type ConfigError =
  | Io(IoError)
  | Parse(ParseError)
  deriving From

fn default_config(root: String) -> Config =
  { root: root, bare: false, description: "" }

fn with_description(config: Config, desc: String) -> Config =
  { ...config, description: desc }

fn summary(config: Config) -> String =
  "root=${config.root}, bare=${config.bare}"

effect fn load(path: String) -> Result[Config, ConfigError] =
  do {
    let text = fs.read_text(path)
    let raw = json.parse(text)
    json.get_string(raw, "root")
      |> option.ok_or("missing root")
      |> result.map((r) => { root: r, bare: false, description: "" })
  }

effect fn main(args: List[String]) -> Result[Unit, ConfigError] = {
  let path = match list.get(args, 1) {
    some(p) => p,
    none => "config.json",
  }
  let config = load(path)
  println(summary(config))
  ok(())
}

test "default config" {
  let cfg = default_config("/repo")
  assert_eq(cfg.bare, false)
  assert_eq(cfg.description, "")
}

test "with_description updates correctly" {
  let cfg = default_config("/repo")
  let updated = with_description(cfg, "my repo")
  assert_eq(updated.description, "my repo")
  assert_eq(updated.root, cfg.root)
}
```

---

## 25. Evaluation Metrics

| Metric | Definition |
|---|---|
| **Pass@1** | Rate of passing compilation + tests on the first generation |
| **Repair Turns** | Number of fix iterations from first failure to final success |
| **Token Cost** | Total input/output tokens until success |
| **API Hallucination Rate** | Rate of nonexistent APIs or incorrect signatures |
| **Edit Breakage Rate** | Rate of breaking unrelated behavior when modifying code |
| **Diagnostic Utilization Gain** | Performance difference with/without structured diagnostics |
