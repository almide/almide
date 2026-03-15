# Clone Reduction Phase 4 [ACTIVE]

Phases 0-3 (done in [codegen-optimization](../done/codegen-optimization.md)) established single-use move analysis, concat optimization, and Lobster-style borrow inference. Phase 4 targets the remaining unnecessary `.clone()` calls that survived those passes.

## Current State (v0.5.13)

Borrow analysis works at the **variable level** but not at the **field level**. The emitter still unconditionally clones in several common patterns:

| Pattern | Location | Problem |
|---------|----------|---------|
| `for x in iter.clone()` | ir_expressions.rs | Every for-in clones iterable, even ranges and single-use lists |
| `vec![a.clone(), b.clone()]` | ir_expressions.rs | All variables in list literals cloned, ignoring use-count |
| `x.name.clone()` | ir_expressions.rs | All non-Copy member accesses cloned, even if used once |
| `{ let mut __spread = x.clone(); ... }` | ir_expressions.rs | Record spread always clones base, even for single-use |
| `match x.clone() { ... }` | ir_blocks.rs | Match subject cloned even when never used after match |
| `args.clone()` in extern calls | program.rs | All extern call args cloned, even Copy types |

## Phase 4a: For-in Clone Conditional

**Current:** `for {binding} in ({iter_str}).clone() { ... }`

**Fix:** Check if iterable is:
- **Range type** (`i..j`) → no clone needed (Copy)
- **Single-use variable** (use_count == 1) → move, no clone
- **Literal expression** (`[1, 2, 3]`) → no clone needed (freshly constructed)
- **Otherwise** → clone (current behavior)

**Implementation:** In `gen_ir_expr` for `ForIn`, check `iterable.ty` and `single_use_vars` before emitting `.clone()`.

## Phase 4b: List/Tuple Element Clone

**Current:** All `Var` elements in list/tuple literals are cloned.

**Fix:** Consult `single_use_vars` — if a variable appears only once in the function body and this list literal is that use, move it.

## Phase 4c: Member Access Clone

**Current:** `x.name.clone()` for all non-Copy field accesses.

**Fix:** Two-level approach:
1. If the member access result is single-use in the function → consider context
2. If the object is single-use → the field can be moved out (destructure)

**Note:** This is harder than 4a/4b because member access on a borrowed reference can't move. Need to check whether the object is owned or borrowed.

## Phase 4d: Match Subject Clone

**Current:** Match subjects that are variables get cloned if non-Copy.

**Fix:** If the match subject variable is never used after the match (use-count analysis at the match point), skip clone and match by value.

## Phase 4e: Record Spread Clone

**Current:** `{ let mut __spread = x.clone(); __spread.field = v; __spread }`

**Fix:** If `x` is single-use, emit `{ let mut __spread = x; ... }` (move, no clone).

## Phase 4f: Extern Call Clone

**Current:** All extern call arguments unconditionally cloned.

**Fix:** Skip clone for Copy types (Int, Float, Bool). Use borrow analysis for heap types.

## Phase 4g: Field-Level Borrow Analysis

The biggest remaining gap. Current borrow analysis operates on whole variables only. If a `String` field of a record is passed to a function, the whole record is marked `Owned`.

**Goal:** Track escape at field granularity:
- `fn f(r: { name: String, .. })` where `r.name` is only used in `println` → `r.name` can be borrowed
- This enables `&r.name` instead of `r.name.clone()` at call sites

**Difficulty:** High. Requires extending `BorrowInfo` to track per-field ownership.

## Priority

| Item | Impact | Difficulty | Priority |
|------|--------|------------|----------|
| 4a. For-in | High (every loop) | Low | P0 |
| 4b. List element | High (common) | Low | P0 |
| 4d. Match subject | Medium | Medium | P1 |
| 4e. Record spread | Medium | Low | P1 |
| 4f. Extern clone | Low-Medium | Low | P1 |
| 4c. Member access | Medium | Medium | P2 |
| 4g. Field-level borrow | High | High | P3 |

## Affected Files

| File | Change |
|------|--------|
| `src/emit_rust/ir_expressions.rs` | 4a, 4b, 4c, 4e |
| `src/emit_rust/ir_blocks.rs` | 4d |
| `src/emit_rust/program.rs` | 4f |
| `src/emit_rust/borrow.rs` | 4g |
| `src/emit_rust/mod.rs` | Extended BorrowInfo for field-level |

## Continues

[Codegen Optimization Phases 0-3](../done/codegen-optimization.md) (done)
