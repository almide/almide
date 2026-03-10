# LLM × Immutable Data Structures [ROADMAP]

LLMs trained on Python/JS/Go default to mutable algorithms. Almide's immutable lists cause systematic failures when LLMs port mutable patterns directly.

## The Problem

LLM-generated quicksort in Almide:
```
fn partition(arr: List[Int], low: Int, high: Int) -> Int = {
  let pivot = list.get(arr, high)
  var i = low - 1
  for j in low..high {
    let elem = list.get(arr, j)
    if elem < pivot then {
      i = i + 1
      arr = list.set(arr, i, elem)    // <-- parameter reassignment (now caught)
      arr = list.set(arr, j, tmp)
    }
  }
  i + 1  // modified arr is lost — only index returned
}
```

Failures:
1. **Parameter reassignment** — `arr` is immutable (now detected: v0.4.6)
2. **Lost mutations** — `partition` returns `Int`, modified list is discarded
3. **Return type mismatch** — should return `(List[Int], Int)` tuple

This isn't a quicksort problem — it affects any in-place algorithm: sorting, graph traversal, matrix operations, tree rotations.

## Current Mitigations

| Mitigation | Status | Effect |
|------------|--------|--------|
| `cannot reassign immutable binding` error | v0.4.6 | LLM gets immediate feedback on parameter reassignment |
| `list.set` returns new list | v0.4.5 | Semantics are correct, but LLMs miss the return value |
| Optional `else` / braceless let-chain | v0.4.5-6 | Reduces syntax friction for functional style |

## Roadmap

### Tier 1 — Error message quality (short-term)

#### 1.1 Suggest tuple return for lost mutations
When a function modifies a collection via `list.set` but doesn't return the modified collection:
```
warning: 'arr' is modified via list.set but not returned
  --> quicksort.almd:4
  hint: Return the modified list alongside the result: -> (List[Int], Int)
```

**Effort**: Medium. Requires tracking `list.set` targets and checking if they appear in the return expression.

#### 1.2 Suggest `var` with shadowing pattern
When parameter reassignment is detected, additionally suggest the idiomatic pattern:
```
error: cannot reassign immutable binding 'arr'
  hint: Use 'var' instead of 'let' to declare a mutable variable, or use a different name
  hint: Idiomatic pattern: var arr_ = arr, then use arr_ for mutations
```

**Effort**: Low. Just improve the error message.

#### 1.3 Rich source location in errors
Currently:
```
error: cannot reassign immutable binding 'arr'
  --> example.almd:12
```
Target:
```
error: cannot reassign immutable binding 'arr'
  --> example.almd:12:7
   |
12 |       arr = list.set(arr, i, elem)
   |       ^^^ parameter 'arr' is immutable
   |
  = hint: Declare as 'var arr_ = arr' at the start of the function
```

**Effort**: Medium-high. Needs source text retention for display. See [error-diagnostics.md](./error-diagnostics.md).

### Tier 2 — Stdlib patterns for immutable algorithms (medium-term)

#### 2.1 `list.swap` — immutable swap
```
fn swap(xs: List[T], i: Int, j: Int) -> List[T]
```
Most common missing primitive. Quicksort, selection sort, heap operations all need this.

**Effort**: Low. `list.set(list.set(xs, i, get(xs, j)), j, get(xs, i))`.

#### 2.2 `list.update` — functional update at index
```
fn update(xs: List[T], i: Int, f: fn(T) -> T) -> List[T]
```
Enables `list.update(xs, i, fn(x) => x + 1)` instead of `list.set(xs, i, list.get(xs, i) + 1)`.

#### 2.3 `list.slice` / `list.range` / `list.insert` / `list.remove_at`
See [list-stdlib-gaps.md](./list-stdlib-gaps.md) for full plan.

### Tier 3 — Language-level support (long-term, research)

#### 3.1 Mutable local collections (`var xs = [1,2,3]`)
Allow `var` lists to use `xs[i] = v` syntax that compiles to `xs = list.set(xs, i, v)` under the hood.
```
var arr = [3, 1, 2]
arr[0] = 99          // desugars to: arr = list.set(arr, 0, 99)
```
LLMs write this naturally. Semantics stay immutable (copy-on-write), syntax is familiar.

**Trade-off**: Looks mutable, is actually immutable. May confuse developers expecting O(1) mutation.
**Effort**: High. Needs parser + codegen changes for IndexAssign on var-bound lists.

#### 3.2 `with` expression for bulk updates
```
let arr2 = arr with {
  [0] = 99
  [2] = 42
}
```
Batches multiple updates into a single new list creation. More efficient than chained `list.set`.

**Effort**: High. New syntax + optimization pass.

#### 3.3 Auto-return-modified-collection analysis
Detect when a function takes a collection, calls `list.set`/`list.swap` on it, but only returns a non-collection value. Suggest returning a tuple.

**Effort**: High. Needs data-flow analysis.

## Comparison: How other languages handle this

| Language | Approach | LLM compatibility |
|----------|----------|-------------------|
| Python | Mutable by default | High (LLMs trained on it) |
| JavaScript | Mutable + immutable methods (`with`, `toSorted`) | Medium |
| Rust | Mutable by default (`&mut`), borrow checker prevents bugs | Medium (LLMs struggle with borrowing) |
| Go | Mutable slices | High |
| Haskell | Fully immutable, `ST` monad for local mutation | Low (LLMs can't write monadic code reliably) |
| Elixir | Fully immutable, pipe-heavy functional style | Low-medium |
| Kotlin | Mutable + immutable collections (separate types) | High |
| **Almide** | Immutable + `var` for rebinding + clear errors | **Medium → High (with Tier 1-2)** |

## Success metric

An LLM should be able to write a working quicksort in Almide within 2 attempts:
1. First attempt: mutable pattern → clear error with actionable fix
2. Second attempt: functional pattern using tuple return + `list.swap`

Currently: LLMs fail silently (mutations lost) or get cryptic Rust errors.
Target: Almide-level error on first attempt, correct code on second.

## Priority

**Tier 1.1 + 1.2** (error messages) → **Tier 2.1** (`list.swap`) → **Tier 1.3** (rich source) → **Tier 3.1** (var indexing sugar)
