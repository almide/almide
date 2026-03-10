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
- Genuinely untyped positions (`enumerate` return, `flatten` input) remain `Unknown`

### What remains Unknown (intentionally)
- `map.new()` — empty map, no type information available
- `list.enumerate()` return — tuples `(Int, A)` not expressible in current type system
- `list.zip()` return — mixed tuples `(A, B)` not expressible
- `list.partition()` return — tuple `(List[A], List[A])` not expressible
- `list.flatten()` input — would need `List[List[A]]` constraint
- `from_entries()` — tuple list input, types not inferrable

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

## Tuple Type Propagation [PLANNED]

Currently `enumerate`, `zip`, `partition`, `entries` return `Unknown` because tuples inside containers can't be expressed. A proper `Tuple[A, B]` type in container positions would close this gap.

```
list.enumerate : [A](List[A]) -> List[Tuple[Int, A]]
list.zip : [A, B](List[A], List[B]) -> List[Tuple[A, B]]
map.entries : [K, V](Map[K, V]) -> List[Tuple[K, V]]
```

## Priority

Structured error types > tuple type propagation > trait bounds > full trait implementation
