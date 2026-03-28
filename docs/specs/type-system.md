> Last updated: 2026-03-28

# Type System Specification

Almide uses a constraint-based type system with bidirectional inference, structural typing for records, and a protocol system for ad-hoc polymorphism. All types are resolved at compile time; there are no runtime type checks.

---

## 1. Primitive Types

Seven built-in primitive types. Each has kind `*` (concrete, zero type parameters).

| Type     | Description                | Eq  | Hash | Ord |
|----------|----------------------------|-----|------|-----|
| `Int`    | 64-bit signed integer      | yes | yes  | yes |
| `Float`  | 64-bit IEEE 754            | yes | no   | yes |
| `String` | UTF-8 string               | yes | yes  | yes |
| `Bool`   | `true` / `false`           | yes | yes  | yes |
| `Unit`   | Zero-value type `()`       | yes | yes  | yes |
| `Bytes`  | Byte buffer                | yes | yes  | no  |
| `Matrix` | Numeric matrix             | yes | no   | no  |

`Float` is not hashable (cannot be a `Map` key or `Set` element). Function types (`Fn`) are neither Eq nor hashable.

```
let x = 42          // Int
let y = 3.14        // Float
let s = "hello"     // String
let b = true        // Bool
let u = ()          // Unit
```

Tests: `spec/lang/expr_test.almd`, `spec/lang/bytes_test.almd`, `spec/lang/matrix_test.almd`

---

## 2. Collection Types

### List[T]

Homogeneous ordered sequence. Kind: `* -> *`.

```
let xs = [1, 2, 3]              // List[Int]
let ys: List[String] = []       // empty list requires annotation
let zs = xs + [4, 5]            // + concatenates lists
let first = xs[0]               // Int (index access)
```

### Map[K, V]

Key-value dictionary. Kind: `* -> * -> *`. Keys must be hashable (no `Float`, `Fn`, or `Map` keys).

```
let m = ["a": 1, "b": 2]       // Map[String, Int]
let empty: Map[String, Int] = [:]
let v = m["a"]                  // Option[Int]
```

### Set[T]

Unique element collection. Kind: `* -> *`. Elements must be hashable.

```
let s = set.from_list([1, 2, 3])  // Set[Int]
```

### Tuple

Fixed-length heterogeneous product type. Variable arity.

```
let t = (1, "hello", true)     // (Int, String, Bool)
let x = t.0                    // Int — positional access
let (a, b) = (1, 2)            // destructuring
```

Tests: `spec/lang/data_types_test.almd`, `spec/lang/tuple_test.almd`, `spec/lang/map_literal_test.almd`

---

## 3. Option[T] and Result[T, E]

Built-in parameterized types for nullable values and error handling. Both are `Applied` types with dedicated constructors.

### Option[T]

Kind: `* -> *`. Constructors: `some(v)`, `none`.

```
let x: Option[Int] = some(42)
let y: Option[Int] = none
match x {
  some(v) => v,
  none => 0,
}
```

### Result[T, E]

Kind: `* -> * -> *`. Constructors: `ok(v)`, `err(e)`.

```
let x: Result[Int, String] = ok(42)
let y: Result[Int, String] = err("fail")
```

In `effect fn` bodies, `Result` is auto-unwrapped with `!`:

```
effect fn read(path: String) -> Result[String, String] = {
  let content = fs.read_text(path)!   // propagates err
  ok(content)
}
```

Tests: `spec/lang/data_types_test.almd`, `spec/lang/error_test.almd`, `spec/lang/unwrap_operators_test.almd`

---

## 4. Record Types

### Named Records

Declared with `type`. Fields are accessed by name.

```
type Point = { x: Float, y: Float }

let p: Point = { x: 1.0, y: 2.0 }
let px = p.x                          // Float
let p2 = { ...p, y: 5.0 }            // spread update
```

### Anonymous Records

Record literals without a type name are structurally typed.

```
let user = { name: "alice", age: 30 }   // { name: String, age: Int }
let n = user.name                        // String
```

### Open Records (Row Polymorphism)

A parameter typed `{ field: Type, .. }` accepts any record that has at least the required fields. Extra fields are allowed and preserved.

```
fn greet(who: { name: String, .. }) -> String = "Hello, ${who.name}!"

type Dog = { name: String, breed: String }
type Person = { name: String, age: Int, email: String }

greet(Dog { name: "Rex", breed: "Lab" })       // ok
greet(Person { name: "Alice", age: 30, email: "a@b" })  // ok
greet({ name: "Bob" })                          // ok — exact match
```

Open records can be used as type aliases (shape aliases):

```
type Named = { name: String, .. }
fn greet_named(who: Named) -> String = "Hi, ${who.name}!"
```

Nested open records are supported:

```
fn get_port(app: { config: { port: Int, .. }, .. }) -> Int = app.config.port
```

Closed records (`{ name: String }` without `..`) require exact field match.

Tests: `spec/lang/open_record_test.almd`, `spec/lang/record_spread_test.almd`

---

## 5. Variant Types

Algebraic data types (tagged unions). Declared with `|`-separated cases.

### Unit Payload (Enum-like)

```
type Direction = | North | South | East | West

fn to_str(d: Direction) -> String = match d {
  North => "N",
  South => "S",
  East => "E",
  West => "W",
}
```

### Tuple Payload

```
type Shape = | Circle(Float) | Rect(Float, Float)

fn area(s: Shape) -> Float = match s {
  Circle(r) => 3.14 * r * r,
  Rect(w, h) => w * h,
}
```

### Record Payload

Variant cases can carry named fields:

```
type Pat =
  | Match { scope: String, regex: String }
  | BeginEnd { scope: String, begin: String, end_pat: String, patterns: List[Pat] }
  | Include(String)
  | Empty
```

Record variant construction and pattern matching:

```
let p = Match { scope: "keyword", regex: "\\bfn\\b" }
match p {
  Match { scope, regex } => scope + " " + regex,
  BeginEnd { scope, .. } => scope,
  _ => "other",
}
```

### Recursive Variants

Variant types can reference themselves. The type checker uses cycle detection to prevent infinite loops in Eq/Hash checks.

```
type Tree[T] = | Leaf(T) | Node(T, List[T])
```

### Inline Variants (No Leading Pipe)

When omitting the leading `|`, the first case starts immediately:

```
type AppError = NotFound(String) | Io(String)
```

Tests: `spec/lang/data_types_test.almd`, `spec/lang/type_system_test.almd`, `spec/lang/variant_record_test.almd`

---

## 6. Function Types

Functions are first-class values. The type syntax uses `fn(Params) -> Ret`.

```
fn apply(f: fn(Int) -> Int, x: Int) -> Int = f(x)

fn make_adder(n: Int) -> fn(Int) -> Int = (x) => x + n

let add5 = make_adder(5)
apply(add5, 10)              // 15
```

Internally represented as `Ty::Fn { params: Vec<Ty>, ret: Box<Ty> }`.

Function types are never Eq and never hashable.

### Effect Functions

`effect fn` marks functions that perform side effects. The type checker enforces that pure functions cannot call effect functions (error E006). The `is_effect` flag on `FnSig` tracks this.

```
effect fn read_file(path: String) -> Result[String, String] = fs.read_text(path)
```

### Type Aliases for Function Types

```
type Handler = (String) -> String
```

Tests: `spec/lang/type_system_test.almd`, `spec/lang/function_test.almd`, `spec/lang/lambda_test.almd`, `spec/lang/effect_fn_test.almd`

---

## 7. User-Defined Generics

Generic type parameters use `[]` syntax (not `<>`).

### Generic Functions

```
fn id[T](x: T) -> T = x
fn pair[A, B](a: A, b: B) -> (A, B) = (a, b)
```

Type arguments can be inferred or explicit:

```
id(42)           // T inferred as Int
id[String]("hi") // T explicitly String
```

### Generic Record Types

```
type Box[T] = { value: T, label: String }

fn unbox[T](b: Box[T]) -> T = b.value
```

### Generic Variant Types

```
type Either[A, B] = | Left(A) | Right(B)

fn map_right[A, B, C](e: Either[A, B], f: fn(B) -> C) -> Either[A, C] = match e {
  Left(a) => Left(a),
  Right(b) => Right(f(b)),
}
```

### Structural Bounds on Generics

Generic parameters can require specific record fields:

```
fn describe[T: { name: String, .. }](x: T) -> String = "name: ${x.name}"

fn set_name[T: { name: String, .. }](x: T, n: String) -> T = { ...x, name: n }
```

The bound `T: { name: String, .. }` is an `OpenRecord` constraint. The checker stores it in `FnSig.structural_bounds` and validates it at each call site.

### Protocol Bounds on Generics

```
fn display[T: Showable](item: T) -> String = item.show()
fn show_named[T: Showable + Nameable](item: T) -> String = item.get_name() + ": " + item.show()
```

Multiple bounds are joined with `+`. The checker validates that the concrete type at each call site has declared conformance to all required protocols.

Tests: `spec/lang/generics_test.almd`, `spec/lang/type_system_test.almd`, `spec/lang/open_record_test.almd`, `spec/lang/protocol_generics_test.almd`

---

## 8. Type Inference

Almide uses constraint-based type inference with a Union-Find data structure.

### Three-Pass Architecture

1. **Infer** (`infer.rs`): Walk the AST, assign fresh type variables (`?0`, `?1`, ...) to unknown types, collect equality constraints between types.
2. **Solve** (`solving.rs`): Process constraints via unification. The Union-Find merges equivalent type variables and binds them to concrete types.
3. **Substitute** (`mod.rs`): Replace all inference variables in `expr_types` with their resolved concrete types.

### Inference Variables

Fresh type variables are named `?N` (e.g., `?0`, `?1`). They are distinct from user-declared `TypeVar`s (`T`, `U`). The `UnionFind` structure manages equivalence classes with union-by-rank and path compression.

### Bidirectional Inference

Types flow both forward (from arguments to return) and backward (from expected type to expression):

```
let xs: List[Int] = []         // [] gets type List[Int] from annotation
let f = (x) => x + 1          // x inferred as Int from + operator
```

### Let-Polymorphism

Generic functions are instantiated with fresh inference variables at each call site:

```
fn id[T](x: T) -> T = x
id(42)          // T = Int at this call
id("hello")     // T = String at this call
```

### Lambda Parameter Inference

Lambda parameters are inferred from how they are used:

```
list.map([1, 2, 3], (x) => x * 2)   // x: Int inferred from List[Int]
```

### Constraint Solving

Constraints are `(expected, actual, context)` triples. The solver unifies each pair:

- **Inference var + concrete**: Bind the var to the concrete type.
- **Inference var + inference var**: Union the two vars.
- **Concrete + concrete**: Structurally recurse (e.g., `List[?0]` vs `List[Int]` unifies `?0 = Int`).
- **Unknown**: Unifies with everything (error recovery sentinel).

An **occurs check** prevents infinite types (e.g., `T = List[T]`).

Tests: `spec/lang/bidirectional_type_test.almd`, `spec/lang/type_annotation_test.almd`, `spec/lang/lambda_test.almd`

---

## 9. Structural Typing and Open Records

Almide supports structural subtyping for records via open record types.

### Compatibility Rules

| Parameter type   | Argument type    | Result  |
|------------------|------------------|---------|
| `{ a: Int }`     | `{ a: Int }`     | ok      |
| `{ a: Int }`     | `{ a: Int, b: String }` | error (closed, extra field) |
| `{ a: Int, .. }` | `{ a: Int, b: String }` | ok (open, extra allowed)    |
| `{ a: Int, .. }` | `Named` with field `a: Int` | ok (named types resolved) |
| `{ a: Int, .. }` | `{ a: Int }`     | ok (exact match allowed)    |

### Unification with Open Records

Open records use order-independent field matching. For `{ a: Int, .. }` vs `{ a: Int, b: String }`, unification succeeds if every required field has a matching field (by name) in the actual type.

### Chain Calling

Open record parameters compose: a function accepting `{ name: String, breed: String, .. }` can pass its argument to a function accepting `{ name: String, .. }`.

```
fn chain_b(x: { name: String, .. }) -> String = x.name
fn chain_a(x: { name: String, breed: String, .. }) -> String = chain_b(x)
```

Tests: `spec/lang/open_record_test.almd`

---

## 10. Protocol System

Protocols define a set of methods that conforming types must implement. They serve the same role as traits (Rust) or typeclasses (Haskell).

### Defining a Protocol

```
protocol Showable {
  fn show(a: Self) -> String
}
```

`Self` in method signatures is a `TypeVar("Self")` that gets substituted with the concrete type at each conformance site.

### Declaring Conformance

Two ways to declare that a type implements a protocol:

**Convention methods** (inline):

```
type Dog: Showable = { name: String }
fn Dog.show(d: Dog) -> String = "Dog: " + d.name
```

**Impl blocks**:

```
type Cat = { name: String }

impl Showable for Cat {
  fn show(c: Cat) -> String = "Cat: " + c.name
}
```

Both register methods as `Type.method` in the function environment.

### Built-in Protocols

| Protocol | Methods | Notes |
|----------|---------|-------|
| `Eq`     | `fn eq(a: Self, b: Self) -> Bool` | All value types are Eq except `Fn`. Auto-derived. |
| `Repr`   | `fn repr(v: Self) -> String` | String representation. |
| `Ord`    | `fn compare(a: Self, b: Self) -> Int` | Ordering (-1, 0, 1). |
| `Hash`   | `fn hash(v: Self) -> Int` | Hash code. No `Float`, `Fn`, or `Map`. |
| `Codec`  | `fn encode(v: Self) -> Value`, `fn decode(v: Value) -> Result[Self, String]` | Serialization. |

### Protocol Validation

After all declarations are registered, the checker validates:

1. All required methods are defined (convention or impl block).
2. Method signatures match the protocol definition (parameter types, return type, arity).
3. `Self` is correctly substituted with the concrete type.

### Using Protocols as Generic Bounds

```
fn display[T: Showable](item: T) -> String = item.show()
```

At each call site, the checker verifies that the concrete type for `T` has declared conformance to `Showable`.

### Multiple Protocols

```
type Widget: Showable, Nameable = { id: Int, name: String }
fn show_named[T: Showable + Nameable](item: T) -> String = item.get_name() + ": " + item.show()
```

### Marker Protocols

Protocols with no methods serve as markers:

```
protocol Serializable {}
type Marker: Serializable = { tag: String }
```

Tests: `spec/lang/protocol_test.almd`, `spec/lang/impl_block_test.almd`, `spec/lang/protocol_generics_test.almd`, `spec/lang/protocol_advanced_test.almd`, `spec/lang/derive_conventions_test.almd`

---

## 11. Type Aliases

`type Name = ExistingType` creates a transparent alias. The alias is interchangeable with the underlying type.

```
type Score = Int
type Label = String

let s: Score = 100
let total = s + 50      // works: Score is Int
```

Type aliases are resolved by `TypeEnv.resolve_named`, which looks up the name in `env.types` and returns the underlying type definition.

Tests: `spec/lang/type_alias_test.almd`

---

## 12. Union Types

Inline union types represent a value that can be one of several types:

```
type StringOrInt = Int | String
```

Internally, `Ty::Union(Vec<Ty>)` stores members sorted and deduplicated. The `Ty::union()` constructor flattens nested unions and deduplicates:

- `Ty::union([Int, String, Int])` produces `Ty::Union([Int, String])`
- `Ty::union([Int])` produces `Ty::Int` (single member unwrapped)

Unification with unions tries each member with snapshotted bindings, committing the first success.

---

## 13. How Types Flow Through the Compiler

```
Source (.almd)
    │
    ▼
  Parse → AST (untyped)
    │
    ▼
  Check → expr_types: HashMap<ExprId, Ty>
    │        ├── registration.rs  — register FnSig, type decls, protocols
    │        ├── infer.rs         — walk AST, assign ?N vars, collect constraints
    │        ├── solving.rs       — Union-Find unification
    │        └── resolve          — substitute ?N → concrete Ty in expr_types
    │
    ▼
  Lower → Typed IR (IrProgram)
    │        ├── trusts expr_types — no type guessing
    │        ├── desugars pipe, UFCS, interpolation, operators
    │        └── assigns VarId to every variable reference
    │
    ▼
  Codegen → target source (Rust / WASM)
             ├── reads IR types for dispatch decisions
             └── emits target-specific type representations
```

### Key Data Structures

- **`Ty` enum** (`src/types/mod.rs`): Internal type representation. 17 variants covering all types.
- **`TypeEnv`** (`src/types/env.rs`): The type environment. Holds type declarations, function signatures, scopes, protocol definitions, and conformance tracking.
- **`FnSig`** (`src/types/mod.rs`): Function signature with params, return type, generics, structural bounds, and protocol bounds.
- **`TypeConstructorId`** (`src/types/constructor.rs`): Identifies type constructors (List, Option, Result, Map, Set, user-defined). Used by the `Applied` variant of `Ty`.
- **`Kind`** (`src/types/constructor.rs`): The "type of a type constructor" — `*`, `* -> *`, `* -> * -> *`.
- **`UnionFind`** (`src/check/types.rs`): Disjoint-set structure for inference variable equivalence. Union-by-rank with path compression.
- **`Checker`** (`src/check/mod.rs`): Orchestrates the three-pass type checking pipeline.

### Unification (`src/types/unify.rs`)

The `unify` function matches a signature type against a concrete type, collecting `TypeVar` bindings:

1. `TypeVar` on signature side: bind to actual type (with occurs check).
2. `TypeVar` on actual side: accept (polymorphic compatibility).
3. `Applied` types: match constructor ID, recursively unify args.
4. `Union` types: try each member with snapshotted bindings.
5. Everything else: delegate to `Ty::compatible`.

`substitute` replaces bound `TypeVar`s in a type with their bindings.

### Type Constructor Registry (`src/types/constructor.rs`)

Every type constructor is registered with its kind and algebraic laws. The registry enables uniform operations across all container types and supports stream fusion optimizations:

| Constructor | Kind           | Algebraic Laws                              |
|-------------|----------------|---------------------------------------------|
| `List`      | `* -> *`       | FunctorComposition, FunctorIdentity, FilterComposition, MapFoldFusion, MapFilterFusion |
| `Option`    | `* -> *`       | FunctorComposition, FunctorIdentity, MonadAssociativity |
| `Result`    | `* -> * -> *`  | FunctorComposition, FunctorIdentity         |
| `Set`       | `* -> *`       | (none)                                      |
| `Map`       | `* -> * -> *`  | (none)                                      |

User-defined types are registered via `register_user_type` with their arity.
