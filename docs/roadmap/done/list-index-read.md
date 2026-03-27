<!-- description: Add index-based read syntax for lists (xs[i]) -->
# List Index Read (`xs[i]`)

## The Problem

Almide supports index-based **write** (`xs[i] = value`) but not index-based **read** (`xs[i]`). Reading a list element requires a function call:

```almide
var data: List[Float] = [1.0, 2.0, 3.0]

// Write — works
data[0] = 99.0

// Read — must use function call
let v = list.get(data, 0)           // Option[Float] — needs unwrap
let v = list.get_or(data, 0, 0.0)   // Float — needs dummy default
```

This asymmetry is the single biggest friction point in numerical/benchmark code. The FFT benchmark has 10 `list.get_or(data, i, 0.0)` calls in its inner loop — every one of them should just be `data[i]`.

### Why this matters

1. **Write vs read asymmetry** — `data[i] = v` works but `let v = data[i]` doesn't. Every language with index-write also has index-read. This asymmetry surprises both humans and LLMs.
2. **`list.get` returns Option** — safe but extremely verbose for inner loops where the index is known valid. `match list.get(data, i) { some(v) => v, none => ... }` for every read is untenable.
3. **`list.get_or` requires a dummy default** — `list.get_or(data, i, 0.0)` hides the intent. The `0.0` is meaningless — the index is always valid. It's noise masquerading as safety.
4. **Performance code becomes unreadable** — FFT butterfly loop with `list.get_or` everywhere vs `data[ui]`:

```almide
// Current — 4 reads = 4 function calls with dummy defaults
let u_re = list.get_or(data, ui, 0.0)
let u_im = list.get_or(data, ui + 1, 0.0)
let vr = list.get_or(data, vi, 0.0)
let vim = list.get_or(data, vi + 1, 0.0)

// Proposed — direct and obvious
let u_re = data[ui]
let u_im = data[ui + 1]
let vr = data[vi]
let vim = data[vi + 1]
```

### Impact on LLM accuracy

LLMs trained on any mainstream language will write `data[i]` for list reads. Every single one. Forcing `list.get_or(data, i, 0.0)` means:
- LLMs must learn a non-universal pattern
- Every generated read is 5x more tokens than necessary
- The dummy default is a new error source (wrong type, wrong value)

## Design

```almide
let xs = [10, 20, 30]
let v = xs[1]           // 20
let first = xs[0]       // 10
```

### Semantics

- `xs[i]` in expression context evaluates to the element at index `i`
- **Runtime panic if out of bounds** — same as Rust's `vec[i]`, Python's `list[i]`, and JavaScript's array behavior
- This is consistent with `xs[i] = v` which already panics on out-of-bounds

### Why panic, not Option

`list.get` returning `Option` remains available for when you **want** to handle missing indices gracefully. `xs[i]` is for when you **know** the index is valid — the common case in loops, algorithms, and computed indices.

This mirrors the standard split in every mainstream language:
| Language | Direct access (panics/throws) | Safe access (returns Option/null) |
|----------|-------------------------------|-----------------------------------|
| Rust | `vec[i]` | `vec.get(i)` |
| Python | `xs[i]` (IndexError) | `xs[i] if i < len(xs)` |
| Go | `xs[i]` (panic) | bounds check yourself |
| Swift | `xs[i]` (crash) | `xs[safe: i]` / optional |

Almide currently only has the safe path. Adding the direct path fills the gap.

### What about safety?

Almide's philosophy is **LLM accuracy, not LLM hand-holding**. A bounds error in `xs[i]` produces a clear runtime panic with the index and length — far more debuggable than silently returning a wrong default from `list.get_or`.

The safety hierarchy:
1. `xs[i]` — direct, panics on OOB (for when index is known valid)
2. `list.get(xs, i)` — returns `Option[T]` (for when index might be invalid)
3. `list.get_or(xs, i, default)` — returns `T` with fallback (for when you have a sensible default)

All three have their place. Only #1 is missing.

## Codegen

### Rust
```rust
// xs[i]  →
xs[i as usize]
```
Same as what `IndexAssign` already generates for the write side. Rust's own Vec indexing panics on OOB — exact same semantics.

### TypeScript
```typescript
// xs[i]  →
xs[i]
```
JavaScript arrays return `undefined` on OOB, not panic. Two options:
- **(a)** Just use `xs[i]` — matches JS semantics, `undefined` propagates and fails visibly
- **(b)** Use a runtime helper that throws on OOB — matches Rust semantics exactly

Option (a) is simpler and sufficient for most cases. Option (b) is more correct. Decision: start with (a), add (b) later if needed.

## Implementation

The write side (`xs[i] = v`) is already fully implemented. The read side follows the same path with minor additions:

### AST

Add to `Expr` enum in `ast.rs`:
```rust
IndexAccess { object: Box<Expr>, index: Box<Expr>, span: Option<Span> }
```

### Parser

In `parse_postfix()` (expressions.rs), after parsing a primary expression, check for `[`:
```
primary [ expr ]  →  IndexAccess { object: primary, index: expr }
```

This naturally composes: `data[2 * i + 1]`, `matrix[row][col]`, etc.

### Checker

- Validate `object` type is `List[T]`
- Validate `index` type is `Int`
- Result type is `T`
- In the future: extend to `Map[K, V]` where `map[key]` returns `V` (panic on missing)

### Emit Rust

```rust
Expr::IndexAccess { object, index, .. } => {
    format!("{}[{} as usize]", self.gen_expr(object), self.gen_expr(index))
}
```

### Emit TS

```rust
Expr::IndexAccess { object, index, .. } => {
    format!("{}[{}]", self.gen_expr(object), self.gen_expr(index))
}
```

### Formatter

```rust
Expr::IndexAccess { object, index, .. } => {
    format!("{}[{}]", self.fmt_expr(object), self.fmt_expr(index))
}
```

## Interaction with existing features

- **`list.get` / `list.get_or`** — unchanged, still available for safe access
- **`xs[i] = v` (IndexAssign)** — unchanged, already works
- **String indexing** — not included in this proposal (strings are UTF-8, indexing is semantically different)
- **Map indexing** — future extension: `map[key]` → panic on missing key (like `dict[key]` in Python)
- **Tuple indexing** — already exists via `t.0`, `t.1` (different syntax, no change needed)

## Tasks

- [ ] AST: add `Expr::IndexAccess { object, index, span }`
- [ ] Parser: parse `expr[expr]` in postfix position
- [ ] Checker: validate List[T] + Int → T, report type errors
- [ ] Emit Rust: `obj[idx as usize]`
- [ ] Emit TS: `obj[idx]`
- [ ] Formatter: preserve `xs[i]` syntax
- [ ] Tests: basic indexing, nested indexing, type errors, OOB runtime panic
- [ ] Update FFT benchmark to use `data[i]` instead of `list.get_or`
