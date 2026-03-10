# Type System Specification

> Verified by `exercises/generics-test/` (3 test files, 21 tests) + all exercises (zero regressions).

---

## 1. Primitive Types

| Almide | Rust | TypeScript |
|--------|------|------------|
| `Int` | `i64` | `number` |
| `Float` | `f64` | `number` |
| `Bool` | `bool` | `boolean` |
| `String` | `String` | `string` |
| `Unit` | `()` | `void` |
| `Path` | `String` | `string` |
| `Bytes` | `Vec<u8>` | `Uint8Array` |

---

## 2. Collection Types

| Almide | Rust | TypeScript |
|--------|------|------------|
| `List[T]` | `Vec<T>` | `T[]` |
| `Map[K, V]` | `HashMap<K, V>` | `Map<K, V>` |
| `Option[T]` | `Option<T>` | `T \| null` |
| `Result[T, E]` | `Result<T, E>` | `T` (error = throw) |

---

## 3. User-Defined Types

### 3.1 Record Types

```almide
type User = { id: Int, name: String }
type Pair[A, B] = { fst: A, snd: B }
```

- Compiled to `struct` (Rust) / `interface` (TypeScript)
- Anonymous record literals auto-resolve to named struct types when field names match
- Generic record types supported with type parameters

### 3.2 Variant Types

```almide
type Token =
  | Word(String)
  | Number(Int)
  | Eof

type Shape =
  | Circle(Float)
  | Rect{ width: Float, height: Float }
  | Point
```

Three forms: unit (no payload), tuple-style (positional), record-style (named fields).

- Compiled to `enum` (Rust) / discriminated union (TypeScript)
- Non-generic variants use `use Enum::*` for unqualified constructor access
- `deriving From` generates `impl From<InnerType>` for single-field tuple cases

### 3.3 Newtype

```almide
type UserId = newtype Int
type Email = newtype String
```

- Wrap: `UserId(42)` / Unwrap: `id.value`
- Zero runtime cost
- Prevents mix-ups at the type level

### 3.4 Type Alias

```almide
type Name = String
type UserList = List[User]
```

---

## 4. Generics

### 4.1 Syntax

Type parameters use `[]` notation. `<>` is reserved for comparison operators.

```
GenericParams ::= "[" TypeParam ( "," TypeParam )* "]"
TypeParam     ::= TypeName
```

No trait bounds yet — `T` accepts anything (see §4.7).

### 4.2 Generic Functions

```almide
fn identity[T](x: T) -> T = x
fn pair[A, B](a: A, b: B) -> (A, B) = (a, b)
fn wrap_list[T](x: T) -> List[T] = [x]
```

- Rust: `fn identity<T: Clone + Debug + PartialEq + PartialOrd>(x: T) -> T`
- TypeScript: `function identity<T>(x: T): T` (erased in JS mode)
- Type variables are registered per declaration and cleaned up after

### 4.3 Generic Record Types

```almide
type Stack[T] = { items: List[T], size: Int }
type Pair[A, B] = { fst: A, snd: B }

fn stack_push[T](s: Stack[T], item: T) -> Stack[T] = {
  { items: s.items ++ [item], size: s.size + 1 }
}
```

- Anonymous record literals `{ items: ..., size: ... }` auto-resolve to `Stack<T>` when field names match
- Rust: generic bounds `<T: Clone + Debug + PartialEq + PartialOrd>` on struct and impl blocks
- TypeScript: `<T>` on interfaces

### 4.4 Generic Variant Types

```almide
type Maybe[T] =
  | Just(T)
  | Nothing

fn from_option[T](opt: Option[T]) -> Maybe[T] =
  match opt {
    some(v) => Just(v)
    none => Nothing
  }
```

- Rust: `use Enum::*` is skipped for generic enums (Rust doesn't allow it)
- Instead, constructor wrapper functions are generated:
  - `fn Just<T>(_0: T) -> Maybe<T> { Maybe::Just(_0) }`
  - `fn Nothing<T>() -> Maybe<T> { Maybe::Nothing }`
- Unit constructors (e.g. `Nothing`) auto-call as `Nothing()` in expression context
- Pattern matching uses qualified paths: `Maybe::Just(v)`, `Maybe::Nothing`

### 4.5 Recursive Generic Variants

```almide
type Tree[T] =
  | Leaf(T)
  | Node(Tree[T], Tree[T])

fn tree_sum(t: Tree[Int]) -> Int =
  match t {
    Leaf(v) => v
    Node(left, right) => tree_sum(left) + tree_sum(right)
  }

fn tree_map[A, B](t: Tree[A], f: fn(A) -> B) -> Tree[B] =
  match t {
    Leaf(v) => Leaf(f(v))
    Node(left, right) => Node(tree_map(left, f), tree_map(right, f))
  }
```

Self-referencing variant fields are auto-detected and handled transparently:

- **Rust enum definition**: recursive fields wrapped with `Box<>` to avoid infinite-size types
  - `Node(Box<Tree<T>>, Box<Tree<T>>)`
- **Constructor wrappers**: accept unboxed values, insert `Box::new()` internally
  - `fn Node<T>(_0: Tree<T>, _1: Tree<T>) -> Tree<T> { Tree::Node(Box::new(_0), Box::new(_1)) }`
- **Pattern matching**: auto-deref with temporary bindings
  - `Tree::Node(__boxed_left, __boxed_right) => { let left = *__boxed_left; let right = *__boxed_right; ... }`
- **TypeScript**: no special handling needed (reference types, no size issue)

The user never writes `Box`, `Box::new()`, or deref operators — the compiler handles it entirely.

### 4.6 Call-site Type Arguments

```almide
identity[Int](42)
stack_new[Int]()
pair[Int, String](1, "hello")
```

- Parser: `peek_type_args_call()` lookahead distinguishes `f[Type](args)` from list indexing `a[0]`
- Rust: turbofish syntax `f::<i64>(args)`
- TypeScript: type args erased (TS infers from usage)
- Formatter: roundtrips `f[Type](args)` correctly

### 4.7 Type Inference

Type variables (`T`, `A`, `B`) are compatible with everything during type checking. The checker does not attempt full Hindley-Milner inference — it relies on explicit type annotations and structural compatibility.

Design rationale: AI-friendliness over type safety rigor. LLM-generated code nearly always uses concrete types. Strict inference would add complexity without proportional benefit for the target use case.

### 4.8 Auto-derived Bounds (Rust Target)

All generic type parameters receive the following Rust bounds:

```
T: Clone + std::fmt::Debug + PartialEq + PartialOrd
```

| Bound | Reason |
|-------|--------|
| `Clone` | Almide values are always copyable (no ownership model exposed) |
| `Debug` | Required for `println!("{:?}", ...)` formatting |
| `PartialEq` | Required for `==` / `!=` operators |
| `PartialOrd` | Required for `>` / `<` / `>=` / `<=` operators |

---

## 5. Function Types

```almide
fn(Int) -> String           // function taking Int, returning String
fn(A, B) -> C               // generic function type
fn() -> Unit                // no-argument function
```

- Used in higher-order function parameters
- Rust: `fn(i64) -> String` or closure types
- TypeScript: `(a: number) => string`

---

## 6. Tuple Types

```almide
(Int, String)               // pair
(A, B, C)                   // triple
```

- Rust: `(i64, String)`
- TypeScript: `[number, string]`

---

## 7. Type Checker Behavior

### 7.1 Type Resolution

The checker resolves types through the following process:

1. Register all declarations (functions, types, traits, impls) in a single pass
2. For each function: register generic type variables as `Ty::TypeVar`
3. Check parameter types and body expression type
4. Verify return type compatibility (with `resolve_named` for structural comparison)
5. Clean up type variables after each declaration

### 7.2 Compatibility Rules

| Left | Right | Compatible? |
|------|-------|-------------|
| `TypeVar` | anything | Yes |
| anything | `TypeVar` | Yes |
| `Int` | `Int` | Yes |
| `Int` | `Float` | No |
| `List[T]` | `List[Int]` | Yes (via TypeVar) |
| `{ a: Int }` | `{ a: Int }` | Yes (structural) |
| Record literal | Named type | Yes (if fields match) |

### 7.3 Named Type Resolution

Anonymous record literals auto-resolve to declared struct types. The emitter maintains a `named_record_types` map (field names → struct name) that is consulted before generating anonymous `AlmdRec` structs.

```almide
type Point = { x: Int, y: Int }
let p = { x: 1, y: 2 }  // resolves to Point, not AlmdRec0
```

---

## 8. Implementation Details

### 8.1 Compiler Pipeline

| Layer | Type System Role |
|-------|-----------------|
| AST (`ast.rs`) | `TypeExpr` enum, `GenericParam`, `Expr::Call.type_args` |
| Parser | `try_parse_generic_params()`, `peek_type_args_call()`, `parse_type_expr()` |
| Type Checker (`check/`) | `Ty::TypeVar`, `FnSig.generics`, compatibility checking |
| Rust Emitter | Generic bounds, Box wrapping, turbofish, constructor wrappers |
| TS Emitter | `<T>` on declarations (erased in JS mode) |
| Formatter | Roundtrip `[T]` on declarations and call-site type args |

### 8.2 Internal Type Representation

```rust
enum Ty {
    Int, Float, String, Bool, Unit,
    List(Box<Ty>),
    Map(Box<Ty>, Box<Ty>),
    Option(Box<Ty>),
    Result(Box<Ty>, Box<Ty>),
    Fn(Vec<Ty>, Box<Ty>),
    Tuple(Vec<Ty>),
    Record(Vec<(String, Ty)>),
    Named(String),
    TypeVar(String),        // generic type variable
    Unknown,
}
```

### 8.3 Emitter State for Generics

| Field | Purpose |
|-------|---------|
| `named_record_types` | field names → struct name (for anonymous record resolution) |
| `generic_variant_constructors` | constructor name → enum name (for pattern qualification) |
| `generic_variant_unit_ctors` | unit constructor names (for auto-`()` in expression context) |
| `boxed_variant_args` | (constructor, arg_index) pairs (for recursive variant Box wrapping) |

---

## 9. Not Yet Implemented

### 9.1 Trait Bounds

```almide
// Future syntax:
fn sort[T: Ord](xs: List[T]) -> List[T] = ...
```

Depends on trait system maturation. Currently all type variables accept anything.

### 9.2 Higher-Kinded Types (HKT)

```almide
// Not planned:
type Functor[F[_]] = trait { ... }
```

Out of scope. Requires kind system, type inference engine rewrite, and Rust emitter overhaul. No production language with multi-target codegen has succeeded at this. The investment (2-4 months) does not align with the mission — LLMs rarely generate code that requires HKT.

### 9.3 Variance

Covariance/contravariance rules for generic type parameters are under consideration for v0.7. Currently not enforced.

---

## 10. Test Reference

| File | Tests | Covers |
|------|-------|--------|
| `exercises/generics-test/generics_test.almd` | 10 | Generic functions, generic records, call-site type args |
| `exercises/generics-test/generics_variant_test.almd` | 6 | Generic variants, PartialOrd, pattern matching, maybe_map |
| `exercises/generics-test/generics_recursive_test.almd` | 5 | Recursive variants, tree_sum, tree_size, tree_map |
