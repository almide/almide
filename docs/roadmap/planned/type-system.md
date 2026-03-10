# Type System Extensions [PLANNED]

## Stdlib Generics Migration (Unknown → TypeVar)

**Status:** Future option — not blocked, not urgent

Currently, stdlib signatures use `Unknown` as a permissive wildcard:

```
list.map : (List[Unknown], Fn[Unknown] -> Unknown) -> List[Unknown]
```

This works because Almide has **two-layer type safety** — the checker is permissive, the Rust/TS backend catches all real type errors. See [stdlib-unknown-type-strategy.md](../../specs/stdlib-unknown-type-strategy.md) for the full rationale.

A future migration would replace `Unknown` with proper type variables:

```
list.map : [A, B](List[A], Fn[A] -> B) -> List[B]
```

### What this enables
- Checker propagates concrete types through stdlib calls (`List[Int]` in → `List[Int]` out)
- Better error messages at the Almide layer instead of deferring to Rust
- Chained operations (`map → filter → fold`) maintain type context throughout

### What this requires
- **Unification engine**: Constraint-based type solver that binds `A = Int` when `List[A]` receives `List[Int]`, then substitutes `A → Int` in the return type
- **TOML extension**: Type parameters on function definitions (`type_params = ["A", "B"]`)
- **`build.rs` changes**: Generate `Ty::TypeVar` instead of `Ty::Unknown` for generic params, emit generic bounds in sigs
- **Checker changes**: Unification pass after argument type checking, substitution into return type

### Estimated complexity
- Unification engine: ~500 lines in `check/` (standard Algorithm W subset)
- TOML + build.rs: ~100 lines
- Testing: Verify all 42 test files + exercises still pass, no regressions

### When to do this
- When benchmark data shows LLMs are confused by Rust-layer type errors that Almide could have caught earlier
- When chained stdlib operations become common in real-world Almide code
- NOT before the current Unknown approach shows measurable problems

### Design sketch

```toml
# Future stdlib/defs/list.toml
[map]
type_params = ["A", "B"]
params = [
    { name = "xs", type = "List[A]" },
    { name = "f", type = "Fn[A] -> B" },
]
return = "List[B]"
```

The unification engine would:
1. Receive call `list.map([1, 2, 3], fn(x) = to_string(x))`
2. Bind `A = Int` from arg 0 (`List[Int]` unifies with `List[A]`)
3. Bind `B = String` from closure return type
4. Substitute into return type: `List[B]` → `List[String]`
5. Caller sees `List[String]` instead of `List[Unknown]`

---

## Trait Bounds on Generics

```almide
// Future syntax
fn sort[T: Ord](xs: List[T]) -> List[T] = ...
```

Depends on trait system maturation. Currently all type variables accept anything — auto-derived Rust bounds (`Clone + Debug + PartialEq + PartialOrd`) handle the backend.

## Full Trait Implementation

Keywords exist in lexer/parser, but type checking and code generation are incomplete.

```almide
trait Show {
  fn show(self) -> String
}

impl Show for Point {
  fn show(self) -> String = "${self.x}, ${self.y}"
}
```

## Structured Error Types

Currently `Result[T, String]` uses a fixed String error type.

```almide
type AppError = NotFound(String) | Unauthorized | Internal(String)
type AppResult[T] = Result[T, AppError]
```

Enables branching by error type in match arms.

## Priority

Structured error types > stdlib generics migration > trait bounds > full trait implementation
