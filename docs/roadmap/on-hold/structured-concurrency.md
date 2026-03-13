# Structured Concurrency [ON HOLD]

## Overview

Layer 2 of Almide's async model. Provides structured task lifecycle management on top of existing `async fn` / `await` (Layer 1). All concurrent work has a clear scope — tasks cannot outlive their parent, eliminating task leaks.

## Design Principles

- **No fire-and-forget** — all tasks complete within their scope
- **Cancellation propagation** — parent cancelled → children stop too
- **No task leaks** — structurally impossible in AI-generated code
- **Composes with `do` blocks** — error propagation works inside concurrent scopes
- **Minimal syntax delta** — sequential → parallel is adding one word (`async` before `let`)

## Why This Matters

| Existing system | What it lacks |
|-----------------|---------------|
| Go goroutines | No structured scope — goroutines leak freely |
| Erlang processes | Linked but not scoped — lifecycle is explicit, not structural |
| JS Promise.all | No cancellation — rejected promises don't stop siblings |
| Rust tokio | `JoinSet` exists but isn't a language-level guarantee |

Almide can guarantee at the language level what others do at the library level.

## Syntax Design: `async let` (Swift-inspired)

### Core: `async let` for parallel execution

`async let` forks a task at the declaration site. `await` joins at the use site.

```almide
async fn load_dashboard(user_id: String) -> Dashboard =
  do {
    async let profile = fetch_profile(user_id)
    async let posts = fetch_posts(user_id)
    async let stats = fetch_stats(user_id)
    Dashboard { await profile, await posts, await stats }
  }
```

Sequential → parallel is a one-word change:

```almide
// Sequential
let a = fetch_a()
let b = fetch_b()

// Parallel
async let a = fetch_a()
async let b = fetch_b()
```

### Semantics

- `async let x = expr` — immediately starts evaluating `expr` as a concurrent task
- `await x` — suspends until the task completes and returns its value
- Scope exit with un-awaited bindings → automatic cancellation
- Inside `do` block: any task failure → cancel siblings → propagate error
- No new keywords — `async` and `let` are both existing

### Comparison with Swift

| | Swift | Almide |
|---|---|---|
| Fork | `async let a = fetchA()` | `async let a = fetch_a()` |
| Join | `await a` | `await a` |
| Error handling | `try await a` (explicit per use) | `do` block handles all errors automatically |
| Scope exit | Un-awaited tasks auto-cancelled | Same |
| race/timeout | `TaskGroup` (manual) | `race()` / `timeout()` stdlib functions |

Almide's advantage: `do` block absorbs error handling, so no `try` noise at every join point.

### Composition with `do` blocks

```almide
async fn checkout(cart: Cart) -> Result[Order, AppError] =
  do {
    async let stock = verify_stock(cart.items)
    async let payment = authorize_payment(cart.total)
    // both must succeed — if either fails, the other is cancelled, do propagates error
    await finalize_order(await stock, await payment)
  }
```

### race / timeout — stdlib functions, not syntax

No new syntax needed. These are async stdlib functions:

```almide
// Race — first to complete wins, rest cancelled
let fastest = await race(fetch_cache(key), fetch_db(key))

// Timeout — fail if not complete within duration
let data = await timeout(5s, fetch(url))

// Sleep
await sleep(100ms)
```

## Codegen Strategy

| Syntax | Rust output | TS output |
|--------|-------------|-----------|
| `async let x = expr` | `let x = tokio::spawn(async { expr })` | `const x = expr()` (Promise, no await) |
| `await x` | `x.await?` (join handle) | `await x` |
| `race(a, b)` | `tokio::select!` | `Promise.race([a, b])` |
| `timeout(d, f)` | `tokio::time::timeout(d, f)` | `Promise.race([f, sleep(d).then(throw)])` |

### Cancellation

- **Rust**: Drop the `JoinHandle` → future is cancelled. `select!` handles this natively.
- **TS**: `AbortController` + `AbortSignal` threaded through. Requires runtime cooperation.
- **WASM**: Single-threaded — `async let` degrades to eager evaluation. `race` picks first resolved microtask.

## Implementation Phases

### Phase 1: `async let` + `await` codegen

- [ ] Parse `async let` as a new binding form in declarations
- [ ] Type check: `async let x: T` produces a future/handle, `await x` yields `T`
- [ ] Rust codegen: `tokio::spawn` + `.await`
- [ ] TS codegen: unawaited Promise + `await`
- [ ] Replace `almide_block_on` busy-wait with proper tokio runtime
- [ ] Scope exit cancellation: drop handles for un-awaited bindings
- [ ] Tests in `spec/lang/async_test.almd`

### Phase 2: `race` / `timeout` / `sleep` stdlib

- [ ] Add `race`, `timeout`, `sleep` as stdlib async functions
- [ ] Rust codegen: `tokio::select!`, `tokio::time::timeout`, `tokio::time::sleep`
- [ ] TS codegen: `Promise.race`, `setTimeout` wrapper

### Phase 3: Async streams

- [ ] `async fn* generator() -> Stream[T]` or similar
- [ ] `for await item in stream { }` syntax
- [ ] Backpressure via bounded channels

## Dependencies

- Layer 1 (`async fn` / `await`) — DONE
- Rust codegen needs tokio (or async-std) runtime instead of `almide_block_on`

## Status

Not started. Layer 1 (async/await) is implemented.
