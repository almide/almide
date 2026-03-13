# Built-in Protocols

## Design Decision

**No user-defined traits.** Almide provides built-in protocols that the compiler understands. User-defined `trait` / `impl` syntax is intentionally excluded.

**Eq is automatic.** All value types support `==` by default. Only function types cannot be compared. No `deriving Eq` needed — the compiler determines comparability from the type structure. This follows Almide's principle: "write it the obvious way and it works."

**Why no user traits:** User-defined traits increase abstraction depth, break locality (LLMs need to understand all impls to modify one callsite), and create expression branching (function vs method). This hurts modification survival rate.

## Built-in Protocols

| Protocol | Behavior | Status |
|----------|----------|--------|
| `Eq` | All value types are comparable. `Fn` types are not. Automatic — no annotation needed | **Done** |
| `Show` | Universal `show(x)` → String for displayable types | Planned |
| `Hash` | Enables use as Map key. Requires `deriving Hash` (opt-in) | Planned |
| `From` | Error type conversions via `deriving From` | **Done** |

## Eq Protocol (Done)

```almide
type Color = | Red | Green | Blue

// Just works — no deriving needed
fn same_color?(a: Color, b: Color) -> Bool = a == b

// Function types are rejected at compile time:
// let f = fn(x: Int) => x + 1
// f == f  // error: function types are not comparable
```

What is Eq:
- Primitives: Int, Float, String, Bool, Unit
- Containers: List[T], Option[T], Result[T, E], Map[K, V], Tuple — if contents are Eq
- Records: if all fields are Eq
- Variants: if all payloads are Eq
- Recursive types: handled (e.g. `Tree[T]` with `Node(Tree[T], Tree[T])`)

What is NOT Eq:
- `Fn` types (function values cannot be compared)

## Planned: Show

Universal `show()` function for all displayable types. Eliminates the need for per-module `int.to_string()`, `float.to_string()`, etc.

## Planned: Hash (opt-in via `deriving Hash`)

Unlike Eq, Hash requires explicit opt-in because:
- Float cannot be hashed (NaN)
- Map key constraint: `Map[K, V]` will require K to be Hash
- Checked at Almide level, not deferred to Rust compiler

## What NOT To Do

- No user-defined traits
- No `impl` blocks (all behavior is standalone functions or `deriving`)
- No associated types, default methods, orphan rules, trait objects
- No `deriving Eq` — Eq is automatic based on type structure
