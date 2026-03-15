# Built-in Protocols [ON HOLD]

> **Note**: Eq, Hash は実装済み。Repr は残件。型システム全体の設計は [Type System Extensions](../active/type-system.md) に移行しており、container protocols (Mappable, Chainable 等) や `deriving` による conformance はそちらを参照のこと。本文書は Eq/Hash/Repr の built-in protocol に限定した初期設計を記録したものである。

## Design Principle

**All protocols are automatic.** The compiler determines protocol support from the type structure. No `deriving` annotations needed (except `From` for error conversions). This follows Almide's principle: "write it the obvious way and it works." Rust's trait system must never leak into user-facing semantics.

**No user-defined traits.** User-defined traits increase abstraction depth, break locality, and create expression branching. This hurts modification survival rate.

## Built-in Protocols

| Protocol | Behavior | Status |
|----------|----------|--------|
| `Eq` | `==` / `!=` on all value types. `Fn` rejected | **Done** |
| `Repr` | `show(x)` → String for all value types. `Fn` rejected | Planned |
| `Hash` | Map key constraint. `Fn` and `Float` rejected | **Done** |
| `From` | Error type conversions via `deriving From` | **Done** |

## Eq Protocol (Done)

Automatic. All value types support `==`. Only `Fn` types are rejected.

```almide
type Color = | Red | Green | Blue
fn same_color?(a: Color, b: Color) -> Bool = a == b  // just works
```

- Primitives: Int, Float, String, Bool, Unit — always Eq
- Containers: List[T], Option[T], Result[T, E], Map[K, V], Tuple — Eq if contents are Eq
- Records, Variants — Eq if all fields/payloads are Eq
- Recursive types — handled (cycle detection)
- `Fn` types — compile error

## Repr Protocol (Planned)

**Automatic.** All value types can be converted to String. Replaces per-module `int.to_string()`, `float.to_string()`, etc.

```almide
show(42)              // "42"
show(3.14)            // "3.14"
show(true)            // "true"
show(Red)             // "Red"
show([1, 2, 3])       // "[1, 2, 3]"
show(some(42))        // "some(42)"
show({ x: 1, y: 2 }) // "{ x: 1, y: 2 }"
```

What is Repr:
- Primitives: Int → digits, Float → digits, String → itself, Bool → "true"/"false", Unit → "()"
- Containers: List → "[elem, ...]", Option → "some(x)" / "none", Result → "ok(x)" / "err(e)"
- Records: "{ field: value, ... }"
- Variants: "Name" / "Name(payload)" / "Name { field: value }"
- Tuple: "(a, b, c)"
- `Fn` types — compile error (or return something like `"<function>"`)

### Implementation

- Add built-in `show` function: `fn show[T: Repr](x: T) -> String`
- Checker: all types except `Fn` are Repr (same pattern as `is_eq`)
- Rust codegen: generate `Display` impl for user-defined types, use `format!("{:?}", x)` or custom formatting
- TS codegen: generate `toString()` or use JSON.stringify-based approach
- Existing `int.to_string()` etc. remain for backward compatibility but `show()` becomes the recommended way

### Open Questions

- String interpolation: should `"value is ${x}"` auto-call `show(x)` for non-String types? Currently requires explicit conversion. If yes, this massively improves ergonomics
- Debug vs display: should `show(Red)` produce `"Red"` (debug-style) or allow user customization? Almide answer: always debug-style, no customization. One way to do things

## Hash Protocol (Done)

**Automatic.** The compiler determines hashability from the type structure. No `deriving Hash` needed. Implemented via `is_hash()` in `src/types.rs` with Float rejection and cycle detection.

```almide
type Color = | Red | Green | Blue

let m: Map[Color, String] = [:]  // OK: Color is hashable
let n: Map[Float, String] = [:]  // error: Float cannot be used as Map key
```

What is Hash:
- Int, String, Bool, Unit — hashable
- **Float — NOT hashable** (NaN != NaN makes hashing unsound)
- List[T], Option[T], Tuple — hashable if contents are Hash
- Records, Variants — hashable if all fields are Hash (and no Float fields)
- `Fn` types — not hashable

### Implementation

- Add `is_hash()` to TypeEnv (same pattern as `is_eq()` with cycle detection)
- Checker: `Map[K, V]` requires K to be Hash at type-check time
- Error message: "Map key type Float is not hashable — use String or Int as keys"
- Rust codegen: emit `#[derive(Hash)]` for types that are Hash
- No user-facing syntax change — just better compile errors

## What NOT To Do

- No user-defined traits
- No `impl` blocks
- No associated types, default methods, orphan rules, trait objects
- No `deriving Eq` / `deriving Repr` / `deriving Hash` — all automatic from type structure
- Only exception: `deriving From` (genuinely opt-in for error type conversions)
