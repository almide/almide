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

Most stdlib positions are now fully typed with TypeVars (v0.4.10). Tuple types in containers were also resolved — `enumerate`, `zip`, `partition`, `entries` now return proper `Tuple` types.

Only 2 positions remain `Unknown`, each requiring a separate language feature:

| Function | Why Unknown | What would fix it |
|----------|------------|-------------------|
| `map.new()` → `Map[Unknown, Unknown]` | Empty container — no argument to infer from | **Bidirectional type checking** with type annotations (`let m: Map[String, Int] = map.new()`) |
| `list.flatten()` input `List[Unknown]` | Needs `List[List[A]]` constraint on input | **Nested type constraints** — a higher-kinded input restriction |
| Error recovery | `undefined_function()` → `Unknown` | N/A (by design, prevents cascade errors) |

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

## 8. Future: Closing the Last 2 Unknown Gaps

Tuple types in containers were resolved in v0.4.10. The 2 remaining `Unknown` positions require distinct language features:

### `map.new()` → Bidirectional type checking
```almide
let m: Map[String, Int] = map.new()  // annotation flows expected type into expression
```
Requires the checker to propagate the expected type from `let` binding into the call, then bind K=String, V=Int on the empty map. Standard bidirectional approach.

### `list.flatten()` → Nested type constraints
```
list.flatten : [A](List[List[A]]) -> List[A]
```
Currently the input is `List[Unknown]` because `parse_type` can't express "the input must be a list of lists". Fixing this requires either a nested-list constraint in TOML or a special-case in the checker.

Both are low priority — the Rust backend catches all type errors in these positions. Tracked in [type-system.md](../roadmap/planned/type-system.md).
