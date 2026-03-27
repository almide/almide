<!-- description: Dedicated while loop syntax replacing do-block guard pattern -->
# While Loop

## The Problem

Almide has no dedicated conditional loop syntax. The current workaround uses `do` blocks with guards:

```almide
var len = 2
do {
  guard len <= n else break
  // body
  len = len * 2
}
```

This is 3 lines of boilerplate for something every other language expresses in 1 line. In algorithms with nested while loops (FFT, binary search, bit manipulation), the noise compounds:

```almide
// FFT bit-reversal: 2 nested while loops = 6 lines of guard boilerplate
var bit = n / 2
do {
  guard bit > 0 and j >= bit else break
  j = j - bit
  bit = bit / 2
}
```

### Why this matters for LLMs

Every LLM writes `while condition { body }` on first attempt. It's universal across Python, Rust, Go, Swift, Kotlin, MoonBit, JavaScript — essentially every imperative and hybrid language.

Forcing LLMs to use `do { guard ... else break; ... }` instead means:
- 3x more tokens per loop (guard + else break + body vs condition + body)
- A non-universal pattern that requires Almide-specific training
- Higher error rate: LLMs forget `else break`, misplace the guard, or confuse `do { }` (auto-? block) with `do { guard ... }` (loop)

## Design

```almide
while condition {
  body
}
```

That's it. One new keyword, one construct, zero ambiguity.

### Examples

```almide
// Simple countdown
var n = 10
while n > 0 {
  println(int.to_string(n))
  n = n - 1
}

// Binary search
var lo = 0
var hi = list.len(xs) - 1
while lo <= hi {
  let mid = (lo + hi) / 2
  let v = xs[mid]
  if v == target then { return some(mid) }
  else if v < target then { lo = mid + 1 }
  else { hi = mid - 1 }
}

// FFT bit-reversal (nested while)
var bit = n / 2
while bit > 0 and j >= bit {
  j = j - bit
  bit = bit / 2
}
```

### Rules

- `while` is a new keyword
- Condition is any expression that evaluates to `Bool`
- Block uses `{ }` (consistent with `for`, `if`, `match`)
- `break` and `continue` work inside `while` (same as `for` and `do` guard loops)
- No parentheses around condition (same as `if condition then`)
- No `while let` — pattern matching loops are handled by `do { guard ... }` or `for ... in`

### Relationship to existing constructs

| Construct | Use case | Stays? |
|-----------|----------|--------|
| `for i in 0..n { }` | Bounded iteration over range/list | Yes — primary loop |
| `while condition { }` | Condition-based looping | **New** |
| `do { guard ... else break }` | Complex multi-guard loops, early exit patterns | Yes — niche |
| `do { ... }` (no guard) | Auto-? error propagation block | Yes — unrelated |
| `list.map/filter/fold` | Functional transforms | Yes — preferred when no mutation needed |

`while` does NOT replace `do { guard ... }`. The guard pattern is still useful when:
- Multiple guards with different exit behaviors (`break` vs `continue` vs `ok(())`)
- The loop condition is complex enough that expressing it as a single boolean is awkward

But for the 90% case — "loop while this is true" — `while` is the right tool.

## Semantics

- `while false { ... }` — body never executes (condition checked first)
- `while true { ... }` — infinite loop (equivalent to `do { ... }` with no guard that breaks)
- The condition is evaluated before each iteration
- No implicit return value — `while` is a statement, evaluates to `Unit`

## Codegen

### Rust

```rust
// while condition { body }  →
while condition {
    body
}
```

Direct mapping. Rust's `while` has identical semantics.

### TypeScript

```typescript
// while condition { body }  →
while (condition) {
    body
}
```

Direct mapping. Only difference is parentheses (TS requires them).

## Impact

### FFT benchmark (before → after)

```almide
// Before: 11 lines for 2 while loops
var bit = n / 2
do {
  guard bit > 0 and j >= bit else break
  j = j - bit
  bit = bit / 2
}

var len = 2
do {
  guard len <= n else break
  // ...
  var i = 0
  do {
    guard i < n else break
    // ...
    i = i + len
  }
  len = len * 2
}

// After: 8 lines, reads like any other language
var bit = n / 2
while bit > 0 and j >= bit {
  j = j - bit
  bit = bit / 2
}

var len = 2
while len <= n {
  // ...
  var i = 0
  while i < n {
    // ...
    i = i + len
  }
  len = len * 2
}
```

3 guard boilerplates eliminated. Each one saves 2 lines and ~10 tokens.

### LLM accuracy prediction

| Metric | `do { guard ... }` | `while` |
|--------|-------------------|---------|
| Tokens per loop header | ~8 | ~3 |
| LLM first-attempt success | Low (non-standard) | Near 100% (universal) |
| Nesting readability | Poor | Good |

## Tasks

- [x] Lexer: add `While` keyword token
- [x] AST: `Expr::While { cond, body, span }` (implemented as Expr, not Stmt)
- [x] Parser: parse `while expr { stmts }`
- [x] Checker: validate condition is `Bool`, check body
- [x] Emit Rust: `while condition { body }`
- [x] Emit TS: `while (condition) { body }`
- [x] Formatter: preserve `while` syntax
- [x] Tests: basic while, nested while, break/continue in while
- [x] FFT benchmark already uses `while`
