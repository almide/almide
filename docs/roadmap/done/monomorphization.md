<!-- description: Function monomorphization for generic structural bounds in Rust codegen -->
<!-- done: 2026-03-15 -->
# Monomorphization

Function monomorphization infrastructure required for Rust codegen of generic structural bounds (`T: { name: String, .. }`).

## Why

The current Rust codegen maps 1 function = 1 Rust function. Open record Phase 1 worked around this with field projection (packing only the required fields into AlmdRec at the call site), but projection cannot handle the following:

| Feature | Problem |
|---------|---------|
| Generic structural bounds `T: { name, .. }` | The code for `.name` access differs per concrete type of `T`. When the return type is `T`, the concrete type must be preserved and returned |
| Container protocols `F: Mappable` | Different `.map()` calls must be generated for `List` and `Option` |

Common solution: **clone the function for each concrete type at the call site (monomorphize)**.

## Current status

### Done ✅

- [x] Generic structural bounds syntax: `fn set_name[T: { name: String, .. }](x: T) -> T`
- [x] Parser: `parse_generic_param` parses `{ field: Type, .. }` as a structural constraint
- [x] AST: `GenericParam.structural_bound: Option<TypeExpr>`
- [x] Checker: register structural constraints in `FnSig.structural_bounds` and `TypeEnv.structural_bounds`
- [x] Checker: unify TypeVar to concrete type at call-site (`T` -> `Dog`)
- [x] Checker: resolve fields from structural constraints in `check_member_access` (`x.name` on `T`)
- [x] Monomorphization pass (`src/mono.rs`): IR-to-IR transformation
  - [x] Instantiation discovery: traverse call graph to collect concrete types
  - [x] Function cloning: `set_name` -> `set_name__Dog`, `set_name__Person`
  - [x] Type substitution: replace `TypeVar`/`Named("T")` in function body with concrete type
  - [x] Call-site rewriting: `set_name(dog)` -> `set_name__Dog(dog)`
- [x] Rust codegen: skip functions with structural constraints (emit only monomorphized versions)
- [x] Formatter: output structural constraints (`[T: { name: String, .. }]`)
- [x] Tests: all 16 tests pass (3 structural constraints + 2 monomorphization + 11 existing)

### Remaining

- [x] Transitive monomorphization: fixed-point loop to resolve A -> B -> C chains
- [x] Multiple structural bounds per function: already working (bindings track all TypeVars)
- [ ] Container protocols integration (`F: Mappable`) — design below
- [ ] TS target: emit structural constraints as type annotations (no mono needed, structural typing)

## Codegen model

```almide
fn set_name[T: { name: String, .. }](x: T, n: String) -> T =
  { ...x, name: n }

// Dog で呼ばれた → Dog 版を生成:
// fn set_name__Dog(x: Dog, n: String) -> Dog { Dog { name: n, ..x } }

// Person で呼ばれた → Person 版を生成:
// fn set_name__Person(x: Person, n: String) -> Person { Person { name: n, ..x } }
```

- 1 function × N concrete types = N Rust functions
- The function body knows the concrete type, so field access, spread, and return are type-safe
- The original generic function is not emitted (only specialized versions)

## Name mangling

```
set_name[T=Dog]                 → set_name__Dog
set_name[T=Person]              → set_name__Person
set_name[T={name, age}]         → set_name__age_name
transform[T=List[Int]]          → set_name__List_Int
```

## Affected files

| File | Change |
|------|--------|
| `src/ast.rs` | Add `structural_bound` to `GenericParam` |
| `src/types.rs` | Add `FnSig.structural_bounds`, `TypeEnv.structural_bounds` |
| `src/parser/types.rs` | Parse `T: { .. }` |
| `src/check/mod.rs` | Register/unregister structural constraints |
| `src/check/calls.rs` | Call-site unification, extend `check_member_access` |
| `src/mono.rs` (new) | Monomorphization pass |
| `src/emit_rust/program.rs` | Skip functions with structural constraints |
| `src/main.rs` | Insert mono pass into pipeline |
| `src/fmt.rs` | Format structural constraints |
| `src/lib.rs` | `pub mod mono` |

## Risk

- **Code size explosion**: N types x M functions = NxM Rust functions. In practice, N is usually small (< 10), so this is acceptable
- **Compile time**: Instantiation discovery is O(calls x types). Performance should be monitored as programs grow

## Container Protocols Design

### Problem
`list.map`, `option.map`, `result.map` all mean "apply a function to the inner value" but are separate stdlib functions.
To write generically:

```almide
// これは書けない — list.map は List 専用
fn double_all[F: Mappable](container: F[Int]) -> F[Int] =
  container.map((x) => x * 2)
```

### Almide's Approach: No Traits, Convention-Based

Almide has no trait/typeclass. Instead, protocols are expressed via **fixed conventions**:

```almide
// Protocol = 構造的制約 + 固定メソッド名
// Mappable は「.map(f) を持つコンテナ」

fn transform[C: Mappable[Int, Int]](c: C, f: fn(Int) -> Int) -> C =
  c.map(f)
```

### Implementation Approach: Type-Parameterized Structural Bounds

```
Mappable[A, B] = { map: fn(fn(A) -> B) -> Self[B] }
```

This is close to HKT (Higher-Kinded Types), but Almide substitutes with the following:

1. **Protocol = fixed-name function set** — `Mappable` means "has a `map` method"
2. **Resolved in mono pass** — when `C = List[Int]`, `c.map(f)` is rewritten to `list.map(c, f)`
3. **Protocol-compatible types are fixed** — List, Option, Result only. No user-defined types (Canonicity)

### Codegen model

```almide
fn transform[C: Mappable](xs: C[Int], f: fn(Int) -> Int) -> C[Int] =
  xs.map(f)

// C = List[Int] の場合:
// fn transform__List_Int(xs: Vec<i64>, f: impl Fn(i64) -> i64) -> Vec<i64> {
//     list_map(xs, f)
// }

// C = Option[Int] の場合:
// fn transform__Option_Int(xs: Option<i64>, f: impl Fn(i64) -> i64) -> Option<i64> {
//     xs.map(f)
// }
```

### Priority

Container protocols offer high expressiveness, but:
- Low usage frequency (most code uses concrete types)
- Complexity close to HKT, which may impact LLM modification survival rate
- High implementation cost (type parameter kind resolution, protocol resolution, mono extension)

**Verdict: on-hold**. Structural bounds + derive conventions satisfy current needs.
Container protocols will be implemented when the need arises.
