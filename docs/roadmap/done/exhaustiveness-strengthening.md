<!-- description: Nested pattern exhaustiveness via Maranget's algorithm -->
<!-- done: 2026-03-31 -->
# Exhaustiveness Strengthening — Nested Patterns

## Current State

Phase 1 (flat patterns) is complete and works well:

| Pattern type | Exhaustiveness check | Status |
|---|---|---|
| Variant (flat) | All constructors required | ✅ Working |
| Option | `some` + `none` | ✅ Working |
| Result | `ok` + `err` | ✅ Working |
| Bool | `true` + `false` | ✅ Working |
| Wildcard `_` / Ident | Exhaustive | ✅ Working |
| Guard arms | Correctly skipped | ✅ Working |

Implementation: `crates/almide-frontend/src/check/mod.rs:327-358`

## The Gap

`collect_covered()` only checks **one level deep**. It records which top-level constructor was matched, but ignores the structure of sub-patterns. This creates false negatives:

### 1. Nested constructor patterns

```almide
type Expr = | Lit(Int) | Add(Expr, Expr) | Neg(Expr)

match e {
  Lit(n) => n
  Add(Lit(a), Lit(b)) => a + b
  Neg(Lit(n)) => 0 - n
}
// No error today. Missing: Add(Add(..), ..), Add(.., Neg(..)), Neg(Add(..)), etc.
```

Compiler sees "Lit, Add, Neg are all covered" and passes. But `Add(Neg(x), y)` would panic.

### 2. Tuple patterns

```almide
fn describe(p: (Bool, Bool)) -> String =
  match p {
    (true, true) => "both"
    (false, false) => "neither"
  }
// No error today. collect_covered() ignores Pattern::Tuple entirely.
// Missing: (true, false), (false, true)
```

### 3. Record field patterns

```almide
type Shape = | Circle { radius: Float } | Rect { w: Float, h: Float }

match s {
  Circle { .. } => "circle"
  Rect { w, .. } => "rect"
}
// This is fine (flat). But:

match s {
  Circle { radius } if radius > 0.0 => "valid circle"
}
// Guard is skipped, so only Circle is considered. Missing: Rect
// This part works correctly. But nested record + variant would not.
```

### 4. Int/String/Float literals without `_`

```almide
match x {
  0 => "zero"
  1 => "one"
}
// No error today. Infinite domain requires `_` but currently no check.
```

## Impact on Mission

**Directly improves modification survival rate.** When an LLM adds a new variant case, all match sites with nested patterns must be updated. Without deep exhaustiveness checking, the compiler silently accepts incomplete patterns that panic at runtime. This is the #1 scenario where LLM-generated code breaks silently.

## Design

### Algorithm: Maranget's Usefulness Check

Use the algorithm from Luc Maranget's "Warnings for pattern matching" (Journal of Functional Programming, 2007). Same algorithm used by Rust, OCaml, Haskell, and Swift.

Core idea: a pattern matrix where each row is a match arm and each column is a position in the pattern. A match is exhaustive iff the pattern `(_, _, ..., _)` is NOT useful (i.e., every possible value is already covered by some row).

The algorithm recursively decomposes constructors:
1. For each constructor `C` of the matched type, filter rows that match `C`
2. Specialize the matrix by expanding `C`'s arguments as new columns
3. Recurse until base case (empty matrix or single column)

### What changes

| Component | Change |
|---|---|
| `check/mod.rs` | Replace `check_match_exhaustiveness` + `collect_covered` with matrix-based algorithm |
| `check/exhaustiveness.rs` | New file: pattern matrix, usefulness check, witness generation |
| Error message | List up to 3 missing pattern examples; if more, show "and N more" |

### Error output

```
error[E010]: non-exhaustive match
  --> app.almd:4:3
   |
 4 |   match e {
   |   ^^^^^ patterns not covered
   |
   = missing: Add(Neg(_), _), Neg(Add(_, _))
   = hint: Add arms for the missing patterns, or use '_ => ...' as a catch-all
```

## Implementation Plan

### Phase 1: Matrix algorithm for flat + nested constructors

Replace the current set-based check with Maranget's algorithm.

- [ ] Create `crates/almide-frontend/src/check/exhaustiveness.rs`
- [ ] Implement `PatternMatrix` and `specialize` / `is_useful` functions
- [ ] Handle: Variant constructors, Option, Result, Bool
- [ ] Handle nested patterns: `some(ok(x))`, `Add(Lit(_), Neg(_))`
- [ ] Witness generation: compute and display example missing patterns
- [ ] Replace `check_match_exhaustiveness` call in `infer.rs:227`
- [ ] Tests: nested variant patterns, nested Option/Result

### Phase 2: Tuple exhaustiveness

- [ ] Add tuple type decomposition to the pattern matrix
- [ ] `(Bool, Bool)` → 4 cells, `(Bool, Option[T])` → 4 cells, etc.
- [ ] Tests: incomplete tuple coverage

### Phase 3: Infinite domain enforcement

- [ ] Match on Int, Float, String without `_` or variable binding → error
- [ ] Error message: "match on Int requires a catch-all `_` pattern"
- [ ] Tests: Int/String/Float match without wildcard

### Phase 4: Record field depth (future)

- [ ] Decompose record field patterns within constructor arms
- [ ] Low priority: LLMs rarely write deep record destructuring in match

## Files to Modify

- `crates/almide-frontend/src/check/exhaustiveness.rs` — NEW: pattern matrix algorithm
- `crates/almide-frontend/src/check/mod.rs` — remove `check_match_exhaustiveness`, `collect_covered`
- `crates/almide-frontend/src/check/infer.rs` — update call site (L227)
- `tests/checker_test.rs` — add nested exhaustiveness tests
- `spec/lang/match_exhaustive_test.almd` — E2E tests

## References

- Maranget, L. "Warnings for pattern matching" (2007) — the canonical algorithm
- [Rust compiler `rustc_pattern_analysis`](https://github.com/rust-lang/rust/tree/master/compiler/rustc_pattern_analysis) — production implementation
- [Swift exhaustiveness checker](https://github.com/apple/swift/blob/main/lib/Sema/TypeCheckSwitchStmt.cpp)
