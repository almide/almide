<!-- description: Automatic deep equality for all value types without deriving -->
<!-- done: 2026-03-13 -->
# Eq Protocol

Automatic `==` / `!=` for all value types. No `deriving` needed.

```almide
type Color = | Red | Green | Blue
fn same_color?(a: Color, b: Color) -> Bool = a == b  // just works
```

## Supported Types

- Primitives: Int, Float, String, Bool, Unit — always Eq
- Containers: List[T], Option[T], Result[T, E], Map[K, V], Tuple — Eq if contents are Eq
- Records, Variants — Eq if all fields/payloads are Eq
- Recursive types — handled (cycle detection)
- `Fn` types — compile error

## Codegen

- **Rust**: `almide_eq!` macro for deep structural equality
- **TS**: `__deep_eq` runtime function

See [Built-in Protocols](../on-hold/trait-impl.md) for remaining protocols (Show, Hash).
