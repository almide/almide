# Map Literal Syntax

## Goal

Add Map literal syntax to the language, enabling bidirectional type inference for empty Maps.

## Syntax

```almide
let m: Map[String, Int] = [:]                    // empty Map (type from annotation)
let m = ["name": "Alice", "age": "30"]           // non-empty Map
let m = [
  "host": "localhost",
  "port": "8080",
]                                                 // multi-line
```

## Design Decisions

- **Swift `[:]` style** — `[]` is already List, `[k: v]` is a natural extension
- No conflict with records (`{ field = value }` uses `=`, not `:`)
- Parser disambiguation: `[` then `]` → empty List, `[` then `:` then `]` → empty Map, expr then `:` → Map, expr then `,` or `]` → List

## Implementation Steps

### 1. Lexer/Parser
- Parse `[:]` as `Expr::EmptyMap`
- Parse `[expr: expr, ...]` as `Expr::MapLiteral { entries: Vec<(Expr, Expr)> }`
- Disambiguation logic after `[`

### 2. Type Checker
- `EmptyMap` with `expected: Some(Ty::Map(k, v))` → `Ty::Map(k, v)`
- `EmptyMap` without expected → error: "cannot infer Map type, add annotation"
- `MapLiteral` → infer K/V from first entry, unify all entries

### 3. IR Lowering
- `IrExprKind::EmptyMap`
- `IrExprKind::MapLiteral { entries }`

### 4. Codegen
- **Rust**: `EmptyMap` → `almide_rt_map_new()`, `MapLiteral` → chain of `almide_rt_map_set`
- **TS**: `EmptyMap` → `__almd_map.new()`, `MapLiteral` → `new Map([...entries])`

### 5. Tests
- Empty Map with type annotation
- Non-empty Map literal
- Nested Map
- Map literal in function arguments
- Bidirectional type flow through if/match/block
