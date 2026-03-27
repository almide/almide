<!-- description: Promote pattern match exhaustiveness from warning to hard error -->
# Exhaustiveness Check — Hard Error

## Current State

Pattern matching exhaustiveness is checked but only emits **warnings**, not errors. Non-exhaustive `match` expressions silently fall through with a `_` arm that panics at runtime.

```almide
type Color = | Red | Green | Blue

fn name(c: Color) -> String =
  match c {
    Red => "red"
    Green => "green"
  }
  // Warning: non-exhaustive (Blue not covered)
  // Runtime: panic if Blue is passed
```

## Goal

Make non-exhaustive `match` a **compile error** with an actionable hint listing the missing patterns.

```
error: non-exhaustive match — missing pattern: Blue
  --> app.almd:4:3
   |
 4 |   match c {
   |   ^^^^^ missing: Blue
   |
   = hint: add `Blue => ...` or use `_ => ...` as a catch-all
```

## Design

### What counts as exhaustive

| Pattern set | Exhaustive? |
|-------------|-------------|
| All variant cases covered | Yes |
| `_` wildcard present | Yes |
| Variable binding (`x`) as last arm | Yes |
| All literal values covered (Bool only) | Yes (`true` + `false`) |
| Int/String/Float literals without `_` | Error — infinite domain |

### Nested patterns

Exhaustiveness must check nested destructuring:

```almide
type Expr = | Lit(Int) | Add(Expr, Expr)

match e {
  Lit(n) => n
  Add(Lit(a), Lit(b)) => a + b
  // Missing: Add(Add(..), ..), Add(.., Add(..))
}
```

### Implementation

The checker already has pattern analysis infrastructure. Changes needed:

1. **`src/check/statements.rs`**: Change exhaustiveness diagnostic from `warn` to `error`
2. **`src/check/mod.rs`**: Add `missing_patterns()` function that computes uncovered cases
3. **Pattern matrix algorithm**: Implement Maranget's algorithm (used by Rust, OCaml) for nested pattern usefulness checking
4. **Hint generation**: List up to 3 missing patterns in the error message; if more, show "and N more"

### Exceptions

- `match` on `Int`, `Float`, `String` always requires `_` (infinite domain). Error if missing.
- `match` on `Bool` is exhaustive with `true` + `false`.
- `match` on `Option[T]` requires `some(_)` + `none`.
- `match` on `Result[T, E]` requires `ok(_)` + `err(_)`.

### Phase 1: Flat patterns

- [ ] Variant exhaustiveness (all constructors covered)
- [ ] Option/Result exhaustiveness
- [ ] Bool exhaustiveness
- [ ] Emit error with missing case names

### Phase 2: Nested patterns

- [ ] Maranget's usefulness algorithm
- [ ] Nested constructor coverage
- [ ] Guard-aware exhaustiveness (arms with guards don't guarantee coverage)

## Risk

- **Breaking change**: Existing programs with non-exhaustive matches will fail to compile. This is intentional — runtime panics are worse than compile errors.
- **Mitigation**: Users can add `_ => todo("handle this")` to acknowledge incomplete handling.
