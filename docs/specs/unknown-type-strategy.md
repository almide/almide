# Unknown Type Strategy

> Why Almide uses `Unknown` instead of full generics in stdlib, and why it's correct for the mission.

---

## 1. What Unknown Is

`Unknown` is a **permissive type wildcard** in Almide's checker. It unifies with every type:

```rust
// src/types.rs
pub fn compatible(&self, other: &Ty) -> bool {
    if *self == Ty::Unknown || *other == Ty::Unknown {
        return true;  // short-circuit: accept anything
    }
    // ...
}
```

Stdlib functions use `Unknown` where other languages use generic type parameters:

```
list.map : (List[Unknown], Fn[Unknown] -> Unknown) -> List[Unknown]
```

vs. what a fully generic version would look like:

```
list.map : [A, B](List[A], Fn[A] -> B) -> List[B]
```

---

## 2. What Unknown Does NOT Do

Unknown is **not type inference**. It does not propagate concrete types:

```almide
let xs: List[Int] = [1, 2, 3]
let ys = list.map(xs, fn(x) = x + 1)
// Checker sees: ys is List[Unknown], not List[Int]
```

This means:
- The checker cannot detect type mismatches in chained operations where the intermediate type is lost
- `ys` is `List[Unknown]`, so any subsequent operation on `ys` elements is accepted without checking

---

## 3. Why This Is Correct

### 3.1 Two-Layer Type Safety

Almide has a **two-layer defense**:

| Layer | Role | Catches |
|-------|------|---------|
| **Almide checker** | Early feedback, fast | Obvious errors: wrong arg count, string vs int, missing fields |
| **Rust/TS compiler** | Full type safety, authoritative | All type errors in generated code |

When `list.map` returns `List[Unknown]` in the checker, the **generated Rust code** is:

```rust
almide_rt_list_map(xs.clone(), |x| { x + 1 })
// Rust infers: xs: Vec<i64>, closure returns i64, result is Vec<i64>
```

Rust's type inference fills the gap that Unknown leaves open. Every real type error is caught — just at the Rust layer instead of the Almide layer.

### 3.2 The Alternative Is Worse for LLMs

Full generics in stdlib signatures would mean:

```toml
# Hypothetical fully-generic stdlib
[map]
params = [
    { name = "xs", type = "List[A]" },
    { name = "f", type = "Fn[A] -> B" },
]
return = "List[B]"
type_params = ["A", "B"]
```

This requires:
- Unification engine (Hindley-Milner or bidirectional)
- Type variable scoping and substitution
- Constraint solving for nested generics
- Significantly more complex error messages

For what benefit? LLMs write concrete types. They don't write:

```almide
fn compose[A, B, C](f: fn(A) -> B, g: fn(B) -> C) -> fn(A) -> C
```

They write:

```almide
fn double_all(xs: List[Int]) -> List[Int] = list.map(xs, fn(x) = x * 2)
```

The checker catches the error that matters (passing `String` where `Int` expected) and the Rust compiler catches everything else.

### 3.3 Error Recovery by Design

Unknown's original purpose is **error recovery** — preventing cascade errors:

```almide
let x = undefined_function()  // Error: unknown function → x: Unknown
let y = x + 1                 // No cascade error (Unknown + Int = Unknown)
let z = string.len(y)         // No cascade error (Unknown compatible with String)
```

Without Unknown, one error would produce 10+ downstream errors, making diagnostics unusable. This is the same strategy as TypeScript's `any` in error recovery and Rust's `{error}` type in rustc.

### 3.4 Empirical Evidence

In all 42 test files and 20+ exercises:
- Zero cases where Unknown hid a real bug that the Rust compiler didn't catch
- Zero cases where an LLM produced code that passed the Almide checker but failed at Rust compilation due to Unknown masking a type error

The practical false-positive rate from Unknown is **zero** in the current test suite.

---

## 4. Known Limitations

### 4.1 Chained Operations Lose Context

```almide
let xs = [1, 2, 3]
let evens = list.filter(xs, fn(x) = x % 2 == 0)
let strs = list.map(evens, fn(x) = x ++ " items")  // Should error: Int ++ String
```

- Almide checker: `evens` is `List[Unknown]`, so `x` in the second lambda is `Unknown`, and `Unknown ++ String` is accepted
- Rust compiler: **catches this** — `i64` has no `++` operator with `String`

The error is still caught, but the message comes from Rust, not Almide. The error is less readable but equally effective.

### 4.2 Empty List Ambiguity

```almide
let xs = []
let ys = []
xs == ys  // What types are being compared?
```

This is **explicitly handled** with a dedicated diagnostic:

```
error: cannot compare two empty lists without type annotations
hint: Add a type annotation to at least one side, e.g., let xs: List[Int] = []
```

### 4.3 Pattern Match on Wrong Type

```almide
let x: Int = 42
match x {
  some(n) => n   // Int is not Option — should error
  none => 0
}
```

The checker falls back to Unknown when the pattern doesn't match the subject type. This is a known gap — the subject-pattern type mismatch is not reported. In practice, LLMs don't generate this pattern because they track types in context.

---

## 5. When to Revisit

Unknown should be reconsidered if:

1. **Benchmark data shows** that LLMs produce type errors caught only by Rust but not by Almide, and those Rust errors are confusing enough to cause repair failures
2. **Chained stdlib operations** become a common pattern in real code (map → filter → fold chains where intermediate types matter)
3. **Almide adds a REPL or interpreter** where there is no Rust backend to catch errors

Until then, Unknown is the right trade-off: maximum simplicity in the checker, full safety guaranteed by the backend compiler.

---

## 6. Comparison with Other Languages

| Language | Approach | Trade-off |
|----------|----------|-----------|
| **Python** | No static types at all | LLMs write it fine — runtime catches errors |
| **TypeScript** | `any` for escape hatch | Explicit opt-out of type safety |
| **Go** | `interface{}` / `any` before generics | Worked for years, generics added in 1.18 |
| **Almide** | `Unknown` in stdlib sigs | Checker is permissive, Rust backend is strict |

Almide's position is between Go-pre-generics and TypeScript: permissive at the source level, strict at the compile target. The key difference is that **Almide always has a strict backend** — the permissiveness is never the final word.

---

## 7. Relationship to User-Defined Generics

Almide **does** support user-defined generics:

```almide
fn identity[T](x: T) -> T = x
type Stack[T] = { items: List[T], size: Int }
type Maybe[T] = | Just(T) | Nothing
```

These use `Ty::TypeVar`, not `Ty::Unknown`. TypeVar also unifies with everything in the checker, but it represents an explicit generic parameter declared by the user.

The distinction:
- **TypeVar**: user says "this is generic" → appears in generated code as `<T>`
- **Unknown**: stdlib says "accept anything" → disappears in generated code (concrete types inferred by Rust)

Migrating stdlib from Unknown to TypeVar-based generics would be a future option, but it requires a unification engine. The current approach works without one.
