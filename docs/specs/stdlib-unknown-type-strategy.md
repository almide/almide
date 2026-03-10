# Stdlib Type Strategy

> How Almide's stdlib type signatures evolved from `Unknown` wildcards to named type variables, and where `Unknown` still plays a role.

---

## 1. Current Architecture (v0.4.10+)

Stdlib functions use **named type variables** declared via `type_params` in TOML definitions:

```toml
# stdlib/defs/list.toml
[map]
type_params = ["A", "B"]
params = [
    { name = "xs", type = "List[A]" },
    { name = "f", type = "Fn[A] -> B" },
]
return = "List[B]"
```

The checker uses **unification** to bind type variables from arguments:

```almide
let xs = [1, 2, 3]
let ys = list.map(xs, fn(x) => int.to_string(x))
// Unification: A = Int (from List[Int]), B = String (from closure return)
// Substitution: List[B] → List[String]
// ys is List[String]
```

This enables **actionable error messages** at the Almide layer:

```
# v0.4.9: list.contains([1,2,3], "hello")
error: expects Unknown but got String  ← useless

# v0.4.10:
error: expects Int but got String      ← actionable
```

---

## 2. Unification Engine

Located in `src/types.rs`:

- **`unify(sig_ty, actual_ty, bindings)`** — Matches a signature type against a concrete type, collecting TypeVar bindings. Returns `true` if compatible.
- **`substitute(ty, bindings)`** — Replaces TypeVars in a type with their bound concrete types. Unbound TypeVars become `Unknown`.

Special handling:
- **Effect auto-unwrap**: When unifying Fn return types, if the actual closure returns `Result[X, _]` and the signature expects `X`, the Result wrapper is automatically stripped. This mirrors the effect function auto-unwrap behavior.
- **Unknown passthrough**: `Unknown` still unifies with everything (error recovery).
- **TypeVar passthrough**: Unbound TypeVars in actual types are accepted (polymorphic context).

---

## 3. Where Unknown Remains

`Unknown` is intentionally preserved in positions where type information is structurally unavailable:

| Function | Why Unknown | What would fix it |
|----------|------------|-------------------|
| `map.new()` return | Empty map — no type context | Type annotation syntax |
| `list.enumerate()` return | Returns `List[(Int, A)]` — tuples in containers untyped | Tuple type in containers |
| `list.zip()` return | Returns `List[(A, B)]` | Tuple type in containers |
| `list.partition()` return | Returns `(List[A], List[A])` | Tuple return types |
| `map.entries()` return | Returns `List[(K, V)]` | Tuple type in containers |
| `list.flatten()` input | Needs `List[List[A]]` constraint | Higher-kinded input constraint |
| `map.from_entries()` | Tuple list input, types not inferrable | Tuple type |
| Error recovery | `undefined_function()` → `Unknown` | N/A (by design) |

---

## 4. Two-Layer Type Safety

Almide retains a **two-layer defense** even with TypeVar migration:

| Layer | Role | Coverage |
|-------|------|----------|
| **Almide checker** | Early feedback, actionable errors | TypeVar-bound positions (most stdlib calls) |
| **Rust/TS compiler** | Full type safety, authoritative | Everything including Unknown positions |

When a position uses `Unknown` (e.g., `enumerate` return), the Almide checker is permissive but the generated Rust/TS code is fully typed. No real type error escapes.

---

## 5. Error Recovery

Unknown's original and continuing role is **error recovery** — preventing cascade errors:

```almide
let x = undefined_function()  // Error: unknown function → x: Unknown
let y = x + 1                 // No cascade error (Unknown + Int = Unknown)
let z = string.len(y)         // No cascade error (Unknown compatible with String)
```

Without Unknown, one error would produce 10+ downstream errors. This is the same strategy as TypeScript's `any` in error recovery and Rust's `{error}` type.

Unbound TypeVars also substitute to `Unknown` for this reason — if a type variable can't be resolved from arguments, it silently falls back to permissive mode rather than producing a confusing error.

---

## 6. Historical Context

### v0.1–v0.4.9: Pure Unknown Strategy

All stdlib signatures used `Ty::Unknown` for generic positions. The checker was maximally permissive — it caught argument count errors and obvious type mismatches, but all generic type checking was deferred to the Rust backend.

**Rationale at the time**: Simplicity. No unification engine needed. LLMs write concrete types, so the checker catches the errors that matter and Rust catches the rest.

**Problem that emerged**: Error messages like "expects Unknown" were confusing and unhelpful. The language philosophy — actionable diagnostics that guide toward a specific fix — required Almide-layer type errors to use Almide types, not Rust types like `Vec<i64>`.

### v0.4.10: TypeVar Migration

Added `type_params` to TOML definitions, `unify()` + `substitute()` to the checker. ~60 lines of unification code, ~20 lines of build.rs changes, plus TOML annotations on 54 functions across list.toml and map.toml.

The migration was smaller than initially estimated (~500 lines → ~80 lines) because the unification engine only needs to handle the stdlib call pattern, not full Hindley-Milner inference.

---

## 7. Comparison with Other Languages

| Language | Approach | Trade-off |
|----------|----------|-----------|
| **Python** | No static types at all | Runtime catches errors |
| **TypeScript** | Full generics + `any` escape | Complex inference engine |
| **Go** | `interface{}` → generics in 1.18 | Same migration path as Almide |
| **Almide** | TypeVar in stdlib + Unknown fallback | Simple unification, Rust backend as safety net |

Almide's approach is closest to Go's evolution: start permissive, add generics where they improve error messages, keep a strict backend as the ultimate safety net.

---

## 8. Future: Closing the Unknown Gaps

The remaining `Unknown` positions (Section 3) can be closed by adding **tuple types in containers**:

```
list.enumerate : [A](List[A]) -> List[Tuple[Int, A]]
list.zip : [A, B](List[A], List[B]) -> List[Tuple[A, B]]
map.entries : [K, V](Map[K, V]) -> List[Tuple[K, V]]
```

This is tracked in [type-system.md](../roadmap/planned/type-system.md) under "Tuple Type Propagation".
