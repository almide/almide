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
// Sequential — await each result before starting the next
let a = await fetch_a()
let b = await fetch_b()

// Parallel — start all, then await results
async let a = fetch_a()
async let b = fetch_b()
use(await a, await b)
```

Note: `async fn` returns `Future[T]`. Calling it without `await` or `async let` creates an unevaluated future. `await` resolves it (sequential). `async let` starts it immediately and binds a handle (parallel).

### Semantics

- `async let x = expr` — immediately starts evaluating `expr` as a concurrent task. `x` is a `Future[T]` handle.
- `await x` — suspends until the task completes and returns its value. **Consumes** the handle.
- `await x` a second time is a **compile error** (handle is already consumed). To reuse the value, bind it: `let v = await x; use(v, v)`.
- Scope exit with un-awaited bindings → automatic cancellation.
- Inside `do` block: any task failure → **cancel all sibling tasks** → propagate error. Partial success is not observable.
- No new keywords — `async` and `let` are both existing.

### Failure and cancellation rules

```almide
do {
  async let a = fetch_a()   // starts
  async let b = fetch_b()   // starts
  async let c = fetch_c()   // starts
  use(await a, await b, await c)
}
// If a fails: b and c are cancelled. do propagates a's error.
// If b fails: a and c are cancelled. do propagates b's error.
// This matches do's existing behavior: first error exits the block.
```

**Rationale:**
- Consistent with `do` — `do` exits on the first `Result` error, so `async let` + `do` exits on the first failed task.
- AI doesn't need to write cleanup logic for partially-succeeded parallel operations.
- "All succeed or all fail" is the simplest mental model.

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

## `await` semantics

`await` is a single operation: **`Future[T]` → `T`**.

- `await fetch_user(id)` — `fetch_user` returns `Future[Result[User, E]]`, `await` unwraps the future
- `async let x = expr` — `x` is a `Future[T]` (task handle), `await x` joins it
- Inside `do` block: `await` unwraps the future, `do` propagates the `Result`

This unification means `await` always does one thing: resolve a future. Error handling is always `do`'s job.

## Codegen Strategy

**Important:** The codegen table describes semantic equivalents, not literal lowering. The actual runtime abstraction may differ from these specific APIs.

| Syntax | Rust | TS |
|--------|------|-----|
| `async let x = expr` | Scoped task handle (runtime-managed spawn within scope) | Runtime-managed task handle with `AbortController` |
| `await x` | Join task handle, propagate error | `await` task handle |
| `race(a, b)` | Select first completed, cancel rest | `Promise.race` + abort remaining |
| `timeout(d, f)` | Deadline-scoped execution | `Promise.race([f, deadline])` + abort on timeout |

### Cancellation

- **Rust**: `async let` lowers to a scoped task handle. Scope exit drops the handle → future is cancelled. Initial implementation may use a single-threaded executor; tokio integration is a later optimization. Avoids `tokio::spawn` directly to prevent `Send + 'static` constraints on simple `async let` usage.
- **TS**: `AbortController` + `AbortSignal` per task handle. Scope exit triggers `abort()`. Requires a thin Almide runtime layer over raw Promises to manage lifecycle.
- **WASM**: Single-threaded — `async let` degrades to eager evaluation. `race` picks first resolved microtask.

## Implementation Phases

### Phase 1: `async let` + `await` codegen

- [ ] Parse `async let` as a new binding form in declarations
- [ ] Type check: `async let x: T` produces a future/handle, `await x` yields `T`
- [ ] Rust codegen: scoped task handle + join (single-threaded executor initially, tokio later)
- [ ] TS codegen: runtime-managed task handle with AbortController
- [ ] Replace `almide_block_on` busy-wait with proper async executor
- [ ] Scope exit cancellation: drop handles for un-awaited bindings
- [ ] Tests in `spec/lang/async_test.almd`

### Phase 2: `race` / `timeout` / `sleep` stdlib

- [ ] Add `race`, `timeout`, `sleep` as stdlib async functions
- [ ] Rust codegen: `tokio::select!`, `tokio::time::timeout`, `tokio::time::sleep`
- [ ] TS codegen: `Promise.race`, `setTimeout` wrapper

### Phase 3: Async streams

- [ ] `Stream[T]` type for async iteration
- [ ] Consumption via stdlib: `stream.for_each(|item| ...)`, `stream.map(...)`, `stream.collect()`
- [ ] `loop { let item = await stream.next() }` for manual iteration
- [ ] Backpressure via bounded channels
- [ ] Note: `for await x in stream { }` syntax is NOT planned. Prefer stdlib functions over new syntax to keep the language small.

## Dependencies

- Layer 1 (`async fn` / `await`) — DONE
- Rust codegen needs tokio (or async-std) runtime instead of `almide_block_on`

## Status

Not started. Layer 1 (async/await) is implemented.
