<!-- description: Fixed conventions with colon syntax for polymorphism without traits -->
<!-- done: 2026-03-15 -->
# Derive Conventions

## Summary
Achieve polymorphism with fixed conventions + colon syntax without introducing traits/typeclasses.
A design decision that maximizes LLM generation accuracy.

## Design Rationale
- **LLMs write fixed patterns most accurately**: With 6 conventions, they can memorize them completely
- **Eliminate type error sources**: No combinatorial explosion of trait + impl + bound
- **Avoid "unfamiliar pattern" problem**: Same conventions across all projects
- **Human learning cost can be guided by error messages**: No first-encounter problem for LLMs

## Almide's polymorphism model

Only 2 patterns:
1. **Built-in conventions** — declared with colon, linked to operators and language features
2. **Structural bounds** — write methods and they work, constrain with bounds

```almide
// 1. Built-in convention
type Dog: Eq, Repr = { name: String, breed: String }

fn Dog.eq(a: Dog, b: Dog) -> Bool = d.name == other.name
fn Dog.repr(d: Dog) -> String = "${d.name} (${d.breed})"

// 2. structural bound (no convention definition needed, methods + bounds)
fn print_all[T: { display: () -> String }](items: List[T]) =
  for item in items { println(item.display()) }
```

## Syntax

```almide
type Dog: Eq, Repr = { name: String, breed: String }
type Color: Eq, Repr = Red | Green | Blue | Rgb(Int, Int, Int)
type UserId = Int  // alias — no convention
```

## Fixed Conventions (6 total, no more will be added)

| Convention | Required Function | Enables |
|---|---|---|
| `Eq` | `T.eq(self, other: T) -> Bool` | `==`, `!=` |
| `Repr` | `T.repr(self) -> String` | string interpolation, `println` |
| `Ord` | `T.ord(self, other: T) -> Int` | `sort()`, `<`, `>`, `<=`, `>=` |
| `Hash` | `T.hash(self) -> Int` | `Map` key, `Set` |
| `Encode` | `T.encode(self) -> String` | JSON/TOML serialize |
| `Decode` | `T.decode(s: String) -> Result[T, String]` | deserialize (static method) |

Name selection criteria: concept names that appear most frequently in LLM training data.
- `Eq` — common to Rust/Haskell/MoonBit
- `Repr` — Python `__repr__` (largest language in training data)
- `Ord` — common to Rust/Haskell
- `Hash` — common to all languages

Auto derive: compiler auto-generates when custom functions are not defined.

## Implementation Phases

### Phase 1: Parser + Checker + Codegen mapping ✅ DONE
- `type Dog: Eq, Repr = { ... }` colon syntax parsing
- `fn Dog.eq(self, ...)` method definition syntax parsing
- Checker validates convention names (fixed set of 6)
- Rust codegen maps to `#[derive(PartialEq, Eq, Ord, Hash)]`
- Formatter outputs `: Eq, Repr`

### Phase 2: Method Resolution
- Checker registers `fn Dog.repr(self, ...)` as an associated function of the type
- `dog.repr()` → resolved to `Dog.repr(dog)` via UFCS
- Lowerer converts `Dog.repr` to IR `CallTarget`

### Phase 3: Operator Dispatch
- `a == b` on Dog → dispatches to `Dog.eq(a, b)` (when `Eq` declared)
- `"${dog}"` → dispatches to `Dog.repr(dog)` (when `Repr` declared)
- `dogs.sort()` → uses `Dog.ord` (when `Ord` declared)

### Phase 4: Auto Derive
- When convention functions are undefined, auto-generate in IR
- `Eq`: compare with `==` on all fields
- `Repr`: `TypeName { field1: value1, ... }` format
- `Ord`: lexicographic comparison in field order
- `Hash`: combine hash of all fields

### Phase 5: Static Methods + Encode/Decode
- `Config.decode(json)` — static method call with type name as namespace
- `Encode`: JSON format output
- `Decode`: JSON parse

## Files
```
src/parser/mod.rs          — fn Dog.eq() parsing (expect_any_fn_name)
src/parser/declarations.rs — type Dog: Eq, Repr parsing
src/check/mod.rs           — convention name validation, associated function registration
src/lower.rs               — convention method resolution, auto derive generation
src/emit_rust/lower_rust.rs — Rust derive mapping
src/emit_ts/lower_ts.rs    — TS convention dispatch
src/fmt.rs                 — colon syntax output
```
