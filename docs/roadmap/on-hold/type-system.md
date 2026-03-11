# Type System Extensions

## Stdlib Generics Migration (Unknown → TypeVar)

**Status:** DONE (v0.4.10)

Stdlib signatures now use named type variables instead of `Unknown` for generic positions:

```
# Before (v0.4.9 and earlier)
list.map : (List[Unknown], Fn[Unknown] -> Unknown) -> List[Unknown]

# After (v0.4.10)
list.map : [A, B](List[A], Fn[A] -> B) -> List[B]
```

### Implementation

- **TOML `type_params` field**: Each function declares its type variables (e.g., `type_params = ["A", "B"]`)
- **build.rs**: `parse_type()` checks against `type_params` and generates `Ty::TypeVar` instead of `Ty::Unknown`
- **Unification engine**: `unify()` in `types.rs` — binds TypeVars from arguments (e.g., `A = Int` from `List[Int]`)
- **Substitution**: `substitute()` replaces TypeVars in return type with bound concrete types
- **Effect auto-unwrap**: `unify()` handles closures returning `Result[X, _]` when `X` is expected (effect context)

### What this enables
- Checker propagates concrete types through stdlib calls (`List[Int]` in → `List[Int]` out)
- **Actionable error messages**: "expects Int but got String" instead of "expects Unknown but got String"
- Chained operations (`map → filter → fold`) maintain type context throughout

### Type variable conventions
- **list.toml**: `A` = element type, `B` = transformed type (38 functions)
- **map.toml**: `K` = key type, `V` = value type, `B` = transformed value (16 functions)
- Concrete-only functions (`sum`, `join`) have no type params

## Tuple Type Propagation

**Status:** DONE (v0.4.10)

Tuple types inside containers are now fully typed:

```
list.enumerate : [A](List[A]) -> List[(Int, A)]
list.zip : [A, B](List[A], List[B]) -> List[(A, B)]
list.partition : [A](List[A], Fn[A] -> Bool) -> (List[A], List[A])
map.entries : [K, V](Map[K, V]) -> List[(K, V)]
```

### Remaining Unknown (2 positions, requires new language features)

| Function | Current | What's needed |
|----------|---------|---------------|
| `map.new()` → `Map[Unknown, Unknown]` | 引数なし、型情報ゼロ | **型注釈構文** (`let m: Map[String, Int] = map.new()`) でコンテキストから推論 |
| `list.flatten()` input `List[Unknown]` | ネストされたリストの内側の型が不明 | **ネスト型制約** (`List[List[A]] → List[A]`) で入力型を制約 |

These are not solvable by the current TypeVar/unification approach — each requires a separate language feature. They are low priority because the Rust backend catches all type errors in these positions.

---

## Trait Bounds on Generics [PLANNED]

```almide
// Future syntax
fn sort[T: Ord](xs: List[T]) -> List[T] = ...
```

Depends on trait system maturation. Currently all type variables accept anything — auto-derived Rust bounds (`Clone + Debug + PartialEq + PartialOrd`) handle the backend.

## Full Trait Implementation [PLANNED]

Keywords exist in lexer/parser, but type checking and code generation are incomplete.

```almide
trait Show {
  fn show(self) -> String
}

impl Show for Point {
  fn show(self) -> String = "${self.x}, ${self.y}"
}
```

## Structured Error Types [PLANNED]

Currently `Result[T, String]` uses a fixed String error type.

```almide
type AppError = NotFound(String) | Unauthorized | Internal(String)
type AppResult[T] = Result[T, AppError]
```

Enables branching by error type in match arms.

## Type Annotation on Empty Containers [PLANNED]

`map.new()` and `[]` return `Unknown` types because there's no argument to infer from. A type annotation syntax would allow the checker to assign concrete types:

```almide
let m: Map[String, Int] = map.new()
let xs: List[Int] = []
```

This requires bidirectional type checking (expected type flows into the expression).

## Priority

Structured error types > type annotation on empty containers > trait bounds > full trait implementation
