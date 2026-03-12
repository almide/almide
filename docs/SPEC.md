# Almide Language Specification v0.6

**"Not a language for writing freely, but a language for converging correctly."**

---

## 0. Design Philosophy

### Core Thesis

The essence of language design for LLMs is not maximizing expressiveness, but **minimizing the set of valid candidates at each generation step**.

### Four Pillars

| Principle | Definition |
|---|---|
| **Predictable** | When generating the continuation of code, the "next valid syntax, API, and semantics" can be narrowed down tightly |
| **Local** | The information needed to understand or modify a given location is as close as possible |
| **Repairable** | When errors occur, the compiler, runtime, and type system can return near-unique fix candidates in few steps |
| **Compact** | High semantic density with low syntactic noise. Strict yet concise |

### 7 Design Principles

1. **Canonicity** -- There should be, in principle, only one primary way to express the same meaning
2. **Surface Semantics** -- Side effects, fallibility, optionality, and mutability must appear in the syntax or type
3. **Local Reasoning** -- The meaning of a function or expression should be largely understandable from nearby syntax alone
4. **Incremental Completion** -- Incomplete code is legal; one can make progress by filling typed holes
5. **Repair-First** -- The compiler should be a repair tool, not a rejection tool; diagnostics are structured
6. **Vocabulary Economy** -- The standard library is small and has only a consistent vocabulary
7. **No Magic** -- Mechanisms that change meaning at runtime, context-dependent DSLs, and implicit type conversions are prohibited in principle

### Trade-offs (Intentionally Sacrificed)

- Writing freedom for experts
- Cultural "language feel"
- Ergonomic DSLs
- Explosive metaprogramming power
- Extreme type expressiveness
- Abbreviation aesthetics for brevity

**Goal: High conciseness + Low freedom**

---

## 1. Lexical Specification

### 1.1 Identifiers

```
Identifier ::= [a-z_][a-zA-Z0-9_]*
```

A single trailing `?` is allowed:

```
Name ::= Identifier | Identifier "?"
```

Semantic rules (enforced by static rule):
- `name?` -- **Bool predicate only** (return type must be Bool)

### 1.2 Type Names

```
TypeName ::= [A-Z][a-zA-Z0-9]*
TypeConstructor ::= TypeName
```

### 1.3 Literals

```
IntLiteral       ::= [0-9]+
FloatLiteral     ::= [0-9]+ "." [0-9]+
StringLiteral    ::= '"' ... '"'
InterpolatedStr  ::= '"' ( char | "${" Expr "}" )* '"'
BoolLiteral      ::= "true" | "false"
```

There is **no** null literal for missing values. Absence is represented by `none` (a constructor of `Option[T]`).

### 1.4 Reserved Words

```
import type trait impl for fn let var while
if then else match
ok err some none
try do
todo unsafe effect deriving test
async await guard newtype
```

Reserved for future use: `strict`, `where`

---

## 2. Statement Separators

**Newlines separate statements.** Semicolons are used only to place multiple statements on a single line.

```
let x = 1
let y = 2
let z = x + y    // newline as separator

let a = 1; let b = 2   // semicolon for multiple statements on one line
```

### 2.1 Line Continuation Rules

In the following cases, a newline is ignored and the statement continues on the next line:

**When the line ends with one of the following tokens:**
- Binary operators: `+`, `-`, `*`, `/`, `%`, `++`, `==`, `!=`, `<=`, `>=`, `<`, `>`, `and`, `or`, `|>`
- Delimiters: `,`, `.`, `:`
- Opening brackets: `(`, `{`, `[`
- Arrows: `->`, `=>`
- Assignment: `=`
- Keywords: `if`, `then`, `else`, `match`, `try`, `do`, `not`, `|`

**When the next line starts with one of the following tokens:**
- `.` (method chaining)
- `|>` (pipe)

```
let result = items
  .filter(fn(x) => x > 0)
  .map(fn(x) => x * 2)
  .fold(0, fn(acc, x) => acc + x)

text
  |> string.trim
  |> string.split(",")
```

---

## 3. Syntactic Categories

```
Program   ::= ImportDecl* TopDecl*

TopDecl   ::= TypeDecl | TraitDecl | ImplDecl | FnDecl | TestDecl | TopLetDecl

TopLetDecl ::= "let" Name [":" Type] "=" Expr    (* module-scope constant *)

Stmt      ::= LetStmt | VarStmt | AssignStmt | Expr

Expr      ::= Literal
            | Name
            | InterpolatedStr
            | RecordExpr
            | SpreadExpr
            | ListExpr
            | CallExpr
            | MemberExpr
            | IndexExpr
            | PipeExpr
            | IfExpr
            | MatchExpr
            | ForInExpr
            | WhileExpr
            | BlockExpr
            | DoExpr
            | LambdaExpr
            | HoleExpr
            | TodoExpr
            | TryExpr
            | BinaryExpr
            | UnaryExpr
            | "(" Expr ")"
```

---

## 4. Modules and Imports

> Full specification: **[specs/module-system.md](specs/module-system.md)**
> (package structure, sub-namespaces, aliases, visibility, diamond dependency, resolution pipeline)

Package identity is declared in `almide.toml` — no `module` declaration in source files.

### 4.1 Import Declaration

```
ImportDecl ::= "import" ImportPath
             | "import" ImportPath "as" Ident
             | "import" ImportPath "." "{" NameList "}"

NameList ::= Name ( "," Name )*
```

Examples:
```
import fs                       -- stdlib module
import mylib                    -- user package (loads all sub-namespaces)
import mylib.parser             -- direct sub-module import
import mylib as m               -- alias: m.hello(), m.parser.parse()
import mylib.formatter as fmt   -- sub-module alias: fmt.format_upper()
```

**Prohibited: wildcard imports.** `import fs.*` is a compile error.

### 4.2 Visibility Modifiers

```
fn pub_fn() -> String = ...       -- public (default)
mod fn internal() -> String = ... -- same project only
local fn helper() -> String = ... -- same file only
```

### 4.3 Minimal Prelude

Only truly fundamental types are implicitly imported:
- `Int`, `Float`, `Bool`, `String`, `Unit`
- `Option`, `Result`, `List`
- `some`, `none`, `ok`, `err`
- `true`, `false`

Functions like `map` and `filter` are provided only as methods on collection types and do not float as global functions.

---

## 5. Type Declarations

### 5.1 Generics -- `[]` Notation

**Type arguments use `[]`.** `<>` is reserved exclusively for comparison operators.

```
GenericParams ::= "[" TypeParam ( "," TypeParam )* "]"
TypeParam     ::= TypeName ( ":" TraitBound )?
TraitBound    ::= TypeName ( "+" TypeName )*
```

Rationale: `<>` syntactically conflicts with comparison operators, requiring the parser to perform context-dependent ambiguity resolution. `[]` always means generics, with zero ambiguity. The `>>` splitting problem also does not arise.

```
// No ambiguity
Result[List[Map[String, Int]], Error]
fn map[U](self, f: fn(T) -> U) -> List[U]
```

### 5.2 Record Types

```
TypeDecl   ::= "type" TypeName GenericParams? "=" TypeExpr DerivingClause?

RecordType ::= "{" FieldTypeList? "}"
FieldTypeList ::= FieldType ( "," FieldType )*
FieldType  ::= Identifier ":" TypeExpr
```

Examples:
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

```
VariantType  ::= VariantCase ( "|" VariantCase )*
VariantCase  ::= TypeConstructor
               | TypeConstructor "(" TypeExprList ")"
               | TypeConstructor "{" FieldTypeList "}"

TypeExprList ::= TypeExpr ( "," TypeExpr )*
```

Examples:
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

Variants allow **three forms: zero-argument, tuple-style (positional arguments), and record-style (named fields)**.

### 5.4 deriving

```
DerivingClause ::= "deriving" TypeName ( "," TypeName )*
```

Automatically derives `From` trait implementations for variant types. Mechanically generates `From[Type]` from cases of the form `Name(Type)`.

```
type ConfigError =
  | Io(IoError)
  | Parse(ParseError)
  | Decode(DecodeError)
  deriving From

// The above is equivalent to:
// impl From[IoError] for ConfigError { fn from(e: IoError) -> ConfigError = Io(e) }
// impl From[ParseError] for ConfigError { fn from(e: ParseError) -> ConfigError = Parse(e) }
// impl From[DecodeError] for ConfigError { fn from(e: DecodeError) -> ConfigError = Decode(e) }
```

Rationale: Handwriting `impl From` is a breeding ground for copy-paste errors. Having an LLM accurately generate three subtly different blocks is needless risk.

### 5.5 newtype

```
TypeExpr ::= ... | "newtype" TypeExpr
```

Creates a new type that has the same structure but is distinct at the type level:

```
type UserId = newtype Int
type Email = newtype String
```

- `UserId` and `Int` are not implicitly convertible
- Wrap: `UserId(42)` / Unwrap: `id.value`
- Zero runtime cost (distinction exists only at compile time)
- Prevents mix-ups of IDs and units at the type level

### 5.6 Type Application

```
SimpleType ::= TypeName
             | TypeName "[" TypeExprList "]"
```

Examples:
```
List[String]
Result[User, ParseError]
Map[String, List[Int]]
```

---

## 6. Traits (Minimal Abstraction Mechanism)

```
TraitDecl ::= "trait" TypeName GenericParams? "{" TraitMethodList "}"
TraitMethodList ::= ( TraitMethod )*
TraitMethod ::= "effect"? "fn" Name GenericParams? "(" ParamList ")" "->" TypeExpr
```

Examples:
```
trait Iterable[T] {
  fn map[U](self, f: fn(T) -> U) -> Self[U]
  fn filter(self, f: fn(T) -> Bool) -> Self[T]
  fn fold[U](self, init: U, f: fn(U, T) -> U) -> U
  fn any(self, f: fn(T) -> Bool) -> Bool
  fn all(self, f: fn(T) -> Bool) -> Bool
  fn len(self) -> Int
}

trait Storage[T] {
  effect fn save(self, item: T) -> Result[Unit, IoError]
  effect fn load(self, id: String) -> Result[T, IoError]
}
```

### impl

```
ImplDecl ::= "impl" TypeName GenericParams? "for" TypeName "{" FnDecl* "}"
```

Examples:
```
impl Iterable[T] for List[T] {
  fn map[U](self, f: fn(T) -> U) -> List[U] = _  // builtin
  fn filter(self, f: fn(T) -> Bool) -> List[T] = _
}
```

### Constraints

- Traits contain only method signatures (no default implementations in v0.1)
- No trait inheritance (in v0.1)
- Orphan rule: `impl` can only be written within your own crate

---

## 7. Basic Type Environment

### Primitives

```
Int, Float, Bool, String, Bytes, Path, Unit
```

### Collections

```
List[T], Map[K, V], Set[T]
```

### Effect Representation

```
Option[T], Result[T, E]
```

### Boundary Types

```
Json, Value
```

- May be used as receivers for external input
- Requires `decode[T]` before bringing into domain logic
- Using `Json` as a core domain type triggers a linter warning

### Constructors

```
some(x)  : Option[T]
none     : Option[T]    // type inferred from context
ok(x)    : Result[T, E]
err(x)   : Result[T, E]
```

---

## 8. Function Declarations

```
FnDecl ::= "pub"? "async"? "effect"? "fn" Name GenericParams? "(" ParamList? ")" "->" TypeExpr "=" Expr

ParamList ::= Param ( "," Param )*
Param     ::= Identifier ":" TypeExpr
```

Modifier order: `pub? async? effect? fn`

Principles:
- **Argument types are required**
- **Return type is required**
- The body is an expression
- **Functions with side effects are declared with `effect fn`**
- **Async functions are declared with `async fn` (implicitly includes `effect`)**

### 8.1 `effect fn` -- Explicit Side Effects

When the `effect` keyword precedes a function declaration, it indicates that the function has side effects.

```
fn tracked?(index: Index, path: Path) -> Bool =
  index.entries.any(fn(entry) => entry.path == path)

effect fn add(index: Index, file: Path) -> Result[Index, IoError] =
  if tracked?(index, file) then ok(index)
  else do {
    let bytes = try read(file)
    let id = hash(bytes)
    ok(index.insert(file, id))
  }
```

Rationale: The `!` suffix from v0.2 (`read_text!`) embedded meta-information in the function name, requiring management on both the declaration and call sides. By using `effect fn`:
- Function names become pure identifiers (simplifying the lexer)
- Side effects are expressed via a keyword in the declaration (close to type information)
- Call sites simply call the function normally
- The compiler only needs to detect calls from non-`effect fn` to `effect fn`

### 8.2 Top-Level let -- Module-Scope Constants

```
TopLetDecl ::= "let" Name (":" TypeExpr)? "=" Expr
```

```
let PI = 3.14159265358979323846
let MAX_RETRIES = 3
let GREETING = "Hello, world"
```

Top-level `let` declares module-scope constants. They are evaluated at compile time when possible:
- Numeric literals and simple expressions → Rust `const`
- String and complex expressions → Rust `static LazyLock<T>`

This replaces the previous pattern of using functions as constants (`fn PI() -> Float = 3.14`).

---

## 9. Statements

### 9.1 let / var

```
LetStmt ::= "let" Identifier TypeAnnotation? "=" Expr
           | "let" "{" Identifier ("," Identifier)* "}" "=" Expr
VarStmt ::= "var" Identifier TypeAnnotation? "=" Expr
TypeAnnotation ::= ":" TypeExpr
```

- `let` is **immutable**
- `var` is **mutable**
- Local variables may omit type annotations. Public APIs, module boundaries, and fields require explicit types.

#### Destructuring Bindings

Extract fields from a record:

```
let { name, age } = user
```

Equivalent code:
```
let name = user.name
let age = user.age
```

- No `var` version is provided (immutable bindings only)
- Nested destructuring is not allowed (one level only)
- Renaming is not allowed (field names become variable names directly)

### 9.2 guard

```
GuardStmt ::= "guard" Expr "else" Expr
```

Precondition checking with early exit. When the condition is false, the expression in the else clause is returned.

```
fn f(x: Int) -> Result[Int, Error] = {
  guard x > 0 else err("must be positive")
  ok(x * 2)
}
```

- Can only be used within a block
- The else clause typically returns early with `err(...)`
- Flattens if-else nesting, allowing preconditions to be written first

### 9.3 Reassignment

```
AssignStmt ::= Identifier "=" Expr
```

Only allowed for identifiers bound with `var` (static rule).

### 9.4 Blocks

```
BlockExpr ::= "{" StmtList "}"
StmtList  ::= ( Stmt NEWLINE )* Stmt?
```

If the last statement is an expression, its value becomes the block value.

```
{
  let x = 1
  let y = 2
  x + y
}
```

---

## 10. Expressions

### 10.1 if Expression

```
IfExpr ::= "if" Expr "then" Expr "else" Expr
```

- **The condition must be `Bool`. Truthiness is prohibited.**

```
let msg = if x > 0 then "positive" else "non-positive"

// Compile error:
if x then ...         // Int is not Bool
if list then ...      // List is not Bool
```

### 10.2 match Expression

```
MatchExpr    ::= "match" Expr "{" MatchArmList "}"
MatchArmList ::= MatchArm ( "," MatchArm )*
MatchArm     ::= Pattern Guard? "=>" Expr
Guard        ::= "if" Expr
```

**match must be exhaustive** (exhaustiveness checking is the typechecker's responsibility).

Example with guards:
```
match value {
  ok(n) if n > 100 => "big",
  ok(n) => "small",
  err(e) => e,
}
```

Guards take a Bool expression after `if`. When the guard condition is false, the next arm is tried. This flattens nested if/match and keeps the structure of LLM-generated code simple.

### 10.3 Patterns

```
Pattern ::= "_"
          | Identifier
          | Literal
          | "some" "(" Pattern ")"
          | "none"
          | "ok" "(" Pattern ")"
          | "err" "(" Pattern ")"
          | TypeConstructor
          | TypeConstructor "(" PatternList ")"
          | TypeConstructor "{" FieldPatternList "}"

PatternList      ::= Pattern ( "," Pattern )*
FieldPatternList ::= FieldPattern ( "," FieldPattern )*
FieldPattern     ::= Identifier ":" Pattern | Identifier
```

Examples:
```
match shape {
  Circle(r) => 3.14 * r * r,
  Rect{ width, height } => width * height,
  Point => 0.0,
}

match result {
  ok(value) => value,
  err(e) => handle(e),
}
```

### 10.4 Lambdas

```
LambdaExpr ::= "fn" "(" LambdaParamList? ")" "=>" Expr
```

**Only one form. Shorthand notations are prohibited.**

```
fn(x) => x + 1
fn(x: Int, y: Int) => x + y
items.map(fn(x) => x * 2)
```

### 10.5 Named Arguments

```
CallExpr ::= Expr "(" CallArgList ")"
CallArg  ::= Expr | Identifier ":" Expr
```

Arguments can be named at the call site. No changes to the declaration side are needed.

```
// Positional arguments (as before)
create_user("alice", 30, true)

// Named arguments (order-independent, self-documenting)
create_user(name: "alice", age: 30, active: true)

// Mixed OK (named arguments after positional ones)
create_user("alice", age: 30, active: true)
```

Rationale: What LLMs get wrong most often is "when there are three or more arguments of the same type." A call like `f(true, false, true)` is meaningless without names. With names, even if the position is wrong, the names ensure correct correspondence.

### 10.6 String Interpolation

```
let name = "world"
let msg = "hello ${name}, 1+1=${1 + 1}"
```

Message construction is the most frequent thing LLMs write. Providing one canonical form eliminates drift between `+` concatenation and `format` functions.

### 10.7 Record Expressions and Spread

```
RecordExpr ::= "{" FieldInitList "}"
             | "{" "..." Expr "," FieldInitList "}"

FieldInit ::= Identifier ":" Expr
            | Identifier                 // shorthand: { name } is equivalent to { name: name }
```

Examples:
```
let alice = { name: "alice", age: 30 }
let bob = { ...alice, name: "bob" }      // age is inherited from alice
```

### 10.8 List Expressions

```
ListExpr    ::= "[" ExprList? "]"
IndexExpr   ::= Expr "[" Expr "]"
```

Index read: `xs[i]` returns `Option[T]` — `some(value)` if in bounds, `none` otherwise.
Index write: `xs[i] = v` (var only) modifies the list in place.

### 10.9 for...in Expression

```
ForInExpr ::= "for" Pattern "in" Expr "{" Stmt* "}"
```

```
for item in items {
  println(item)
}

for (i, item) in list.enumerate(items) {
  println(int.to_string(i) ++ ": " ++ item)
}
```

Iterates over a list. The loop body is `Unit`-typed. Use `for...in` for simple list iteration.

### 10.10 while Expression

```
WhileExpr ::= "while" Expr "{" Stmt* "}"
```

```
var i = 0
while i < 10 {
  println(int.to_string(i))
  i = i + 1
}
```

Loops while the condition is true. The loop body is `Unit`-typed. Use `while` for condition-based loops, `do { guard ... }` when you need to return a value from the loop.

### 10.11 Pipe

```
PipeExpr ::= Expr "|>" Expr
```

Examples:
```
text
  |> string.trim
  |> string.split(",")
  |> list.map(fn(s) => string.trim(s))
```

Canonicalizes the problem of mixing method chaining and function calls using pipes. `x |> f` is equivalent to `f(x)`.

#### Placeholder `_`

When using multi-argument functions on the right side of a pipe, `_` specifies where the left-hand value is inserted:

```
text |> split(_, ",")           // split(text, ",")
xs |> filter(_, fn(x) => x > 0)  // filter(xs, fn(x) => x > 0)
```

- `_` functions as a placeholder only within call arguments
- Multiple `_` in a single call is not allowed (compile error)
- When there is no `_`, the conventional `x |> f` -> `f(x)` applies

### 10.10 UFCS (Uniform Function Call Syntax)

**`f(x, y)` and `x.f(y)` are equivalent.** The compiler resolves this automatically.

```
// The following are all the same
string.trim(text)
text.trim()

// These are also the same
string.split(text, ",")
text.split(",")
```

#### Resolution Rules

When `x.f(args...)` is called:

1. If the type of `x` has a method `f`, call it
2. Otherwise, look for a function `f(x, args...)` in scope
3. If neither is found, it is a compile error

#### Rationale

One of the most frequent points of confusion for LLMs is "is this a method call or a function call?" With UFCS:
- Both `string.trim(text)` and `text.trim()` are correct
- The pipe `text |> string.trim` remains valid as well
- It may appear that canonical form options increase, but since **all forms mean the same thing**, mistakes cease to exist
- The boundary between trait methods and free functions disappears, so you can write without worrying about "where is this function defined?"

### 10.11 do Block (Automatic try Propagation for Result/Option)

```
DoExpr ::= "do" BlockExpr
```

Inside a `do` block, `try` is automatically applied to expressions that return `Result[T, E]` or `Option[T]`.

```
effect fn load(path: Path) -> Result[Config, ConfigError] =
  do {
    let text = fs.read_text(path)        // auto try: Result[String, IoError]
    let raw = json.parse(text)           // auto try: Result[Json, ParseError]
    decode[Config](raw)                  // auto try: Result[Config, DecodeError]
  }
```

Type inference rules for `do`:
- When the block's return type is `Result[T, E]`, if an expression in the block is `Result[U, E]`, it is automatically unwrapped and `U` is bound
- If the error types differ, conversion via the `From` trait is attempted; if conversion is not possible, it is a compile error

This is **the solution to the Result verbosity problem**. It offers the choice of handwriting `try` or automating with `do`, yielding two canonical forms with clearly separated semantics:
- `try`: unwrap just one expression
- `do`: write an entire block in a Result context

### 10.12 hole / todo / try

```
HoleExpr ::= "_"
TodoExpr ::= "todo" "(" StringLiteral ")"
TryExpr  ::= "try" Expr
```

**These three are at the core of this language.**

---

## 11. Operators

```
UnaryOp  ::= "-" | "not"
BinaryOp ::= "+" | "-" | "*" | "/" | "%" | "++"
            | "==" | "!=" | "<" | "<=" | ">" | ">="
            | "and" | "or"
            | "|>"
```

`++` is **exclusively for list/string concatenation**. String concatenation via `+` overloading confuses LLMs, so it is separated.

### Precedence

1. unary (`-`, `not`)
2. `*` `/` `%`
3. `+` `-` `++`
4. comparison (`==`, `!=`, `<`, `<=`, `>`, `>=`)
5. `and`
6. `or`
7. `|>`

- Assignment is a statement, not an operator
- Operator overloading is prohibited in principle (built-in types only)
- When in doubt, use parentheses

---

## 12. Error Model

### 12.1 Three-Layer Error Strategy

| Layer | Mechanism | Use Case |
|---|---|---|
| **Normal failure** | `Result[T, E]` | parse, validate, I/O, lookup |
| **Programmer error** | `panic` | unreachable, invariant violations |
| **Testing** | `expect` | simple unwrap within tests |

Exceptions **do not exist**. There is no `throw` / `catch`.

### 12.2 Typing Rule for try

#### try on Result

```
Γ ⊢ e : Result[T, E]
current_return_type = Result[R, E]
-----------------------------------
Γ ⊢ try e : T
```

#### try on Option

```
Γ ⊢ e : Option[T]
current_return_type = Option[R]
-----------------------------------
Γ ⊢ try e : T
```

#### No Mixing

There is no automatic conversion for using `try` on an `Option` inside a function that returns `Result`. Write an explicit conversion:

```
let value = try opt.ok_or(MyError("missing"))
```

### 12.3 Error Conversion

Error type conversion is done explicitly. However, it is made lightweight with `do` blocks + `From` trait + `deriving`:

```
trait From[T] {
  fn from(value: T) -> Self
}

type AppError =
  | Io(IoError)
  | Parse(ParseError)
  deriving From

// Inside a do block, when error types differ, automatic conversion via From if implemented
effect fn load(path: Path) -> Result[Config, AppError] =
  do {
    let text = fs.read_text(path)    // IoError -> AppError via From
    let raw = json.parse(text)       // ParseError -> AppError via From
    decode[Config](raw)
  }
```

---

## 13. Holes and Incomplete Code

**This is a core feature of the language.**

### 13.1 Hole

```
fn parse(text: String) -> Ast = _
```

### 13.2 todo

```
fn optimize(ast: Ast) -> Ast = todo("implement constant folding")
```

### 13.3 Typing Rule

```
expected_type = T
-------------------
Γ ⊢ _ : T          // hole: passes type checking but errors in final artifacts

expected_type = T
-------------------
Γ ⊢ todo(msg) : T  // todo: same as above, but retains a message
```

### 13.4 Compiler Obligation

When a hole is found, the compiler must return in a structured format:
- Expected type T
- Available variables in scope and their types
- Function candidates that can return the expected type
- Template candidate expressions

```json
{
  "error": "hole",
  "location": { "file": "main.lang", "line": 12, "col": 5 },
  "expected_type": "Result[Commit, ParseError]",
  "available_names": [
    { "name": "text", "type": "String" },
    { "name": "parse_header", "type": "(String) -> Result[Header, ParseError]" }
  ],
  "suggestions": [
    "parse_header(text)",
    "todo(\"return commit\")"
  ]
}
```

---

## 14. Effect Design

### 14.1 `effect fn` -- Enforced via Compile Error

Calling an `effect fn` from a non-`effect` function results in **an error, not a warning**.

```
fn pure_fn(x: Int) -> Int =
  read(some_path)    // Compile error: cannot call effect fn from non-effect fn
```

Warnings get ignored, causing the effect boundary to become meaningless. By making it an error, the side-effect boundary is guaranteed at the language level.

### 14.2 unsafe Block

When you truly need to bypass the effect boundary, do so explicitly:

```
fn technically_pure(x: Int) -> Int =
  unsafe { read(cache_path) }    // explicitly breaking safety
```

The presence of `unsafe` surfaces "here be danger."

### 14.3 Standard Library Conventions

```
effect fn now() -> Timestamp
effect fn getenv(key: String) -> Option[String]
effect fn read_text(path: Path) -> Result[String, IoError]
effect fn write(path: Path, data: String) -> Result[Unit, IoError]
effect fn random_int(min: Int, max: Int) -> Int
```

I/O, clock, env, net, and randomness are all `effect fn`.

---

## 15. Async/Await

### 15.1 async fn

`async fn` declares an asynchronous function. **`async` implicitly includes `effect`** (since all async operations involve I/O).

```
async fn fetch(url: String) -> Result[String, HttpError] = _
async fn fetch_json[T](url: String) -> Result[T, HttpError] = _
```

The return type of `async fn` is written as the inner type. The actual runtime return value is `Async[Result[String, HttpError]]`, but the type annotation is written as `Result[String, HttpError]`.

### 15.2 await

```
AwaitExpr ::= "await" Expr
```

`await` unwraps `Async[T]` to extract `T`. It is a prefix operator similar to `try`, usable only within `async fn`.

```
async fn load(url: String) -> Result[Config, AppError] = {
  let text = await fetch(url)      // fetch: Async[Result[String, HttpError]]
                                    // await: Result[String, HttpError]
  let config = try parse(text)     // try: Config
  ok(config)
}
```

### 15.3 Combining with do Blocks

Combine `await` and implicit `try` inside a `do` block:

```
async fn load(url: String) -> Result[Config, AppError] =
  do {
    let text = await fetch(url)     // await unwraps Async, do auto-tries Result
    let config = parse(text)        // do auto-tries Result
    config
  }
```

**`await` is explicit, `try` is made implicit by `do`.** This separation is important:
- Which lines are async is visible via `await` (local reasoning)
- Error handling is handled in bulk by `do` (noise reduction)

### 15.4 Structured Concurrency

Unstructured `spawn` / `join` are **prohibited**. Concurrent execution uses only built-in combinators:

```
// Execute all tasks in parallel and await all results
async fn parallel[T](tasks: List[Async[T]]) -> List[T]

// Return the result of the first task to complete
async fn race[T](tasks: List[Async[T]]) -> T

// Execute with a timeout
async fn timeout[T](ms: Int, task: Async[T]) -> Result[T, TimeoutError]

// Sleep
async fn sleep(ms: Int) -> Unit
```

Examples:
```
async fn load_all(urls: List[String]) -> Result[List[String], HttpError] =
  do {
    await parallel(urls.map(fn(url) => fetch(url)))
  }

async fn fetch_fastest(urls: List[String]) -> Result[String, HttpError] =
  do {
    await race(urls.map(fn(url) => fetch(url)))
  }

async fn fetch_with_timeout(url: String) -> Result[String, AppError] =
  do {
    await timeout(5000, fetch(url))
  }
```

### 15.5 Typing Rules

```
Γ ⊢ e : Async[T]
current_fn is async
----------------------------
Γ ⊢ await e : T
```

Using `await` inside a non-`async fn` is a compile error. Calling an `async fn` from a non-`async fn` or non-`effect fn` is a compile error.

### 15.6 Rationale

- Since `async` includes `effect`, there are at most two modifier types (`async fn` or `effect fn`). The confusion of "should I write `async effect fn`?" is resolved by `async` including `effect`
- Structured concurrency eliminates risks of resource leaks and deadlocks at the language level
- The combination of `do` + `await` makes async error-handling code nearly identical in shape to synchronous code

---

## 16. Testing

### 16.1 test Declaration

```
TestDecl ::= "test" StringLiteral BlockExpr
```

Tests are written as **top-level declarations** in the same file as functions. There is no need to separate them into test-only files.

```
fn add(x: Int, y: Int) -> Int = x + y

test "addition" {
  assert_eq(add(1, 2), 3)
  assert_eq(add(0, 0), 0)
}

test "negative addition" {
  assert_eq(add(-1, 1), 0)
}
```

### 16.2 Assertion Functions

Built-in functions available within test blocks:

```
assert(cond: Bool)                    // fails if cond is false
assert_eq(actual: T, expected: T)     // fails if actual != expected
assert_ne(actual: T, expected: T)     // fails if actual == expected
```

### 16.3 Rationale

- Test code is the most frequently generated output from LLMs. Having a single canonical way to write tests makes the generation distribution converge
- Having tests next to functions makes it easier for LLMs to understand the function's intent (local reasoning)
- `test "name" { ... }` has a simple structure that LLMs can easily learn as a template

---

## 17. Naming Conventions

### ? is for Bool Predicates Only

```
fn empty?(xs: List[Int]) -> Bool = xs.len == 0
fn tracked?(index: Index, path: Path) -> Bool = ...
fn exists?(path: Path) -> Bool = ...
```

A function with `?` whose return type is not `Bool` is a compile error.

### Destructive Updates

| Non-destructive (returns a new value) | Destructive (in-place, `effect fn`) |
|---|---|
| `fn push(list, item) -> List[T]` | `effect fn push(list, item) -> Unit` |
| `fn sort(list) -> List[T]` | `effect fn sort(list) -> Unit` |

---

## 18. Standard Library

### 18.1 Collection API (Trait-Based, Fixed Naming, No Aliases)

Unified across all collection types:

| Operation | Signature | Description |
|---|---|---|
| `map` | `fn[U](self, fn(T) -> U) -> Self[U]` | Transform |
| `filter` | `fn(self, fn(T) -> Bool) -> Self[T]` | Filter |
| `fold` | `fn[U](self, U, fn(U, T) -> U) -> U` | Accumulate |
| `any` | `fn(self, fn(T) -> Bool) -> Bool` | Any element satisfies condition |
| `all` | `fn(self, fn(T) -> Bool) -> Bool` | All elements satisfy condition |
| `len` | `fn(self) -> Int` | Length |
| `contains` | `fn(self, T) -> Bool` | Existence check |
| `find` | `fn(self, fn(T) -> Bool) -> Option[T]` | Search |
| `get` | `fn(self, key) -> Option[T]` | Key lookup |
| `first` | `fn(self) -> Option[T]` | First element |
| `last` | `fn(self) -> Option[T]` | Last element |

**`collect`, `select`, `inject`, `pluck`, etc. do not exist.**

### 18.2 Result / Option Methods

```
// Result[T, E]
fn map[U](self, fn(T) -> U) -> Result[U, E]
fn map_err[F](self, fn(E) -> F) -> Result[T, F]
fn and_then[U](self, fn(T) -> Result[U, E]) -> Result[U, E]
fn unwrap_or(self, default: T) -> T
fn is_ok?(self) -> Bool
fn is_err?(self) -> Bool

// Option[T]
fn map[U](self, fn(T) -> U) -> Option[U]
fn and_then[U](self, fn(T) -> Option[U]) -> Option[U]
fn unwrap_or(self, default: T) -> T
fn ok_or[E](self, err: E) -> Result[T, E]
fn is_some?(self) -> Bool
fn is_none?(self) -> Bool
```

### 18.3 String Operations

```
// string module
fn trim(s: String) -> String
fn split(s: String, sep: String) -> List[String]
fn join(parts: List[String], sep: String) -> String
fn starts_with?(s: String, prefix: String) -> Bool
fn ends_with?(s: String, suffix: String) -> Bool
fn contains?(s: String, sub: String) -> Bool
fn replace(s: String, from: String, to: String) -> String
fn len(s: String) -> Int
fn to_int(s: String) -> Option[Int]
fn to_float(s: String) -> Option[Float]
```

### 18.4 Core Modules

| Module | Purpose |
|---|---|
| `string` | String operations |
| `path` | Path operations |
| `fs` | File I/O (all `effect fn`) |
| `json` | JSON parsing and generation |
| `http` | HTTP communication (all `effect fn`) |
| `time` | Time (`now`, etc. are `effect fn`) |
| `env` | Environment variables (all `effect fn`) |

---

## 19. Prohibitions

| # | Prohibited | Reason |
|---|---|---|
| 1 | Implicit type conversion | LLMs mix up types |
| 2 | Truthiness | Conditions must be Bool only |
| 3 | Monkey patching / open classes | Runtime semantic changes are invisible to LLMs |
| 4 | Operator overloading | Operator meaning changing by type is unreadable |
| 5 | Exceptions (throw/catch) | Control flow is invisible |
| 6 | Multiple lambda notations | Generation distribution disperses |
| 7 | Internal DSLs | Strong context dependence |
| 8 | Wildcard imports | Source of names is unknown |
| 9 | null | Unified under `Option[T]` |
| 10 | API aliases | More vocabulary = more hallucinations |
| 11 | `<>` generics | Ambiguous with comparison operators. Use `[]` |

---

## 20. Compiler Responsibilities

### 20.1 One Error, One Root Cause

Suppress cascading derived errors. Present a single root cause.

### 20.2 Structured Error Output

```json
{
  "kind": "type_mismatch",
  "location": { "file": "main.lang", "line": 12, "col": 5 },
  "expected": "Result[Config, IoError]",
  "actual": "String",
  "suggestions": ["ok(text)", "try parse_config(text)"]
}
```

### 20.3 Auto-Fix Candidates

- Suggest missing imports
- Type conversion candidates
- Auto-generate missing match arms
- Point out missing `effect`
- Suggest `do` blocks (when 3 or more `try` expressions appear consecutively)

### 20.4 Official Formatter (Built into the Language)

- One AST yields exactly one formatted output
- Import order is fixed to alphabetical
- Trailing commas are included
- Line-breaking rules for long call chains / pipe chains are fixed

This directly impacts LLM diff stability.

---

## 21. Linter

Not a style police, but a **generation stabilization device**.

| Rule | Description |
|---|---|
| effect-leak | Calling `effect fn` from non-`effect fn` (error) |
| unused-result | Ignoring a `Result` |
| unsafe-unwrap | Unsafely collapsing an `Option` |
| long-chain | Chain exceeds 5 levels |
| ambiguous-name | Single-character variables (except lambda arguments) |
| missing-annotation | Type omission in public functions |
| json-in-core | Using `Json` type in core domain |

---

## 22. Gradual Strictness

```
strict types      // make all type annotations required
strict effects    // fully check effect propagation
```

In project configuration:
```
[strictness]
core = "all"      // fully strict
app = "medium"    // types only strict
script = "light"  // no strictness
```

---

## 23. Escape Hatch

Dangerous features are isolated in `unsafe` blocks:

```
unsafe {
  // effect rules can be ignored here
  // some type checks can be skipped here
}
```

The presence of `unsafe` surfaces "normal rules are being broken here."

---

## 24. Typing Rules

### Variables

```
Γ(x) = T
-----------
Γ ⊢ x : T
```

### let

```
Γ ⊢ e : T
--------------------
Γ, x:T ⊢ let x = e
```

### if

```
Γ ⊢ c : Bool    Γ ⊢ t : T    Γ ⊢ e : T
-----------------------------------------
Γ ⊢ if c then t else e : T
```

### Functions

```
Γ, x1:T1, ..., xn:Tn ⊢ body : R
-----------------------------------------
Γ ⊢ fn f(x1:T1,...,xn:Tn) -> R = body
```

### Option / Result Constructors

```
Γ ⊢ e : T                         Γ ⊢ e : T
-------------------                -------------------
Γ ⊢ some(e) : Option[T]           Γ ⊢ ok(e) : Result[T, E]

Γ ⊢ e : E
-------------------
Γ ⊢ err(e) : Result[T, E]
```

### match

```
Γ ⊢ e : T
Γ,p1 ⊢ e1 : R  ...  Γ,pn ⊢ en : R
exhaustive(p1...pn, T)
--------------------------------------
Γ ⊢ match e { p1=>e1, ..., pn=>en } : R
```

### try (Result)

```
Γ ⊢ e : Result[T, E]
return_type = Result[R, E]
----------------------------
Γ ⊢ try e : T
```

### try (Option)

```
Γ ⊢ e : Option[T]
return_type = Option[R]
----------------------------
Γ ⊢ try e : T
```

### do Block

```
return_type = Result[R, E]
Γ ⊢ block : R   (with implicit try on Result[_, E] expressions)
----------------------------
Γ ⊢ do { block } : Result[R, E]
```

Inside a `do` block, expressions of type `Result[T, E]` are implicitly unwrapped and bound as `T`. If the error type `E` differs, conversion via the `From` trait is attempted; if conversion is not possible, it is a compile error.

### hole / todo

```
expected_type = T               expected_type = T
-------------------             -------------------
Γ ⊢ _ : T                      Γ ⊢ todo(msg) : T
```

### pipe

```
Γ ⊢ x : A    Γ ⊢ f : A -> B
------------------------------
Γ ⊢ x |> f : B
```

### spread

```
Γ ⊢ base : { f1:T1, ..., fn:Tn }
Γ ⊢ ei : Ti  (for overridden fields)
--------------------------------------
Γ ⊢ { ...base, fi: ei, ... } : { f1:T1, ..., fn:Tn }
```

### await

```
Γ ⊢ e : Async[T]    enclosing function is async
-------------------------------------------------
Γ ⊢ await e : T
```

### destructure

```
Γ ⊢ e : { f1:T1, ..., fn:Tn }
--------------------------------------
Γ, f1:T1, ..., fn:Tn ⊢ let { f1, ..., fn } = e
```

### guard

```
Γ ⊢ cond : Bool    Γ ⊢ else_ : R
return_type = R
--------------------------------------
Γ ⊢ guard cond else else_
```

---

## 25. Complete Example

```
import fs
import json

type Config = {
  root: Path,
  bare: Bool,
  description: String,
}

type ConfigError =
  | Io(IoError)
  | Parse(ParseError)
  | Decode(DecodeError)
  deriving From

fn exists?(path: Path) -> Bool =
  fs.exists?(path)

effect fn load(path: Path) -> Result[Config, ConfigError] =
  do {
    let text = fs.read_text(path)
    let raw = json.parse(text)
    decode[Config](raw)
  }

fn with_description(config: Config, desc: String) -> Config =
  { ...config, description: desc }

fn default_config(root: Path) -> Config =
  { root: root, bare: false, description: "" }

fn summary(config: Config) -> String =
  "root=${config.root}, bare=${config.bare}"

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

Properties exhibited here:
- Failures use `Result`, no exceptions
- Side effects are visible via `effect fn`, enforced by the compiler
- `do` blocks minimize Result noise
- `deriving From` achieves type-safe error conversion without boilerplate
- `...` spread for immutable record updates
- String interpolation for canonical message construction
- `?` is for Bool predicates only, meaning is unambiguous
- `[]` generics with zero syntactic ambiguity
- Newline-separated for a natural appearance
- All type boundaries are visible
- Tests are right next to functions (local reasoning)
- Match guards allow flat pattern matching
- UFCS eliminates the method/function distinction

---

## 26. Changelog

### v0.5 -> v0.6

| Change | Reason |
|---|---|
| Destructuring bindings (`let { name, age } = user`) | Concise extraction of record fields. Eliminates repetitive `user.name` |
| `newtype` (`type UserId = newtype Int`) | Same structure but type-level distinction. Prevents mix-ups of IDs and units |
| Pipe placeholder (`x \|> f(_, y)`) | Enables multi-argument functions in pipes. No lambda needed |
| `guard` statement (`guard cond else expr`) | Flattens precondition checks. Reduces if-else nesting |

### v0.4 -> v0.5

| Change | Reason |
|---|---|
| Named arguments (`f(name: "alice")`) | Eliminates positional argument mix-ups. Self-documenting |
| `async fn` / `await` | Introduces async processing in a form consistent with `effect fn` |
| Structured concurrency (`parallel`, `race`, `timeout`) | Prohibits `spawn`/`join`, providing only safe concurrency patterns |

### v0.3 -> v0.4

| Change | Reason |
|---|---|
| Match guard (`pattern if cond => expr`) | Flattens nested if/match. Increases pattern matching expressiveness |
| UFCS (`f(x, y)` = `x.f(y)`) | Eliminates the method vs. function decision. Both are correct |
| `test "name" { ... }` syntax | Unifies test writing. Locality of writing tests next to functions |

### v0.2 -> v0.3

| Change | Reason |
|---|---|
| `<>` -> `[]` generics | Eliminates ambiguity with comparison operators. Eliminates the `>>` splitting problem |
| `fn name!()` -> `effect fn name()` | Separates meta-information from function names. Simplifies lexer/parser |
| Added `deriving` | Eliminates `impl From` boilerplate. Prevents copy-paste errors |
| Codified line continuation rules | Formalized the implicit rules for lines starting with `.` `\|>` |

---

## 27. Items Under Consideration for v0.7

- Variance rules for generics
- Default implementations for traits
- Basic stream type
- Variance rules for generics (already have `pub fn`, `mod fn`, `local fn`)

---

## 28. Evaluation Metrics

| Metric | Definition |
|---|---|
| **Pass@1** | Rate of passing compilation + tests on the first generation |
| **Repair Turns** | Number of fix iterations from first failure to final success |
| **Token Cost** | Total input/output tokens until success |
| **API Hallucination Rate** | Rate of nonexistent APIs or incorrect signatures appearing |
| **Edit Breakage Rate** | Rate of breaking unrelated behavior when modifying existing code |
| **Diagnostic Utilization Gain** | Performance difference in repair with/without structured diagnostics |

Comparison targets:
- Python / Ruby / TypeScript / Go baseline
- Python strict profile / Ruby canonical profile / TypeScript reduced profile
- This language
