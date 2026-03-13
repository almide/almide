# Structured Concurrency [ON HOLD]

## Overview

Layer 2 of Almide's async model. Provides structured task lifecycle management on top of existing `async fn` / `await` (Layer 1). All concurrent work has a clear scope — tasks cannot outlive their parent, eliminating task leaks.

## Design Principles

- **No fire-and-forget** — all tasks complete within their scope
- **Cancellation propagation** — parent cancelled → children stop too
- **No task leaks** — structurally impossible in AI-generated code
- **Composes with `do` blocks** — error propagation works inside concurrent scopes

## Why This Matters

| Existing system | What it lacks |
|-----------------|---------------|
| Go goroutines | No structured scope — goroutines leak freely |
| Erlang processes | Linked but not scoped — lifecycle is explicit, not structural |
| JS Promise.all | No cancellation — rejected promises don't stop siblings |
| Rust tokio | `JoinSet` exists but isn't a language-level guarantee |

Almide can guarantee at the language level what others do at the library level.

## Syntax Design

### concurrent — parallel execution, wait for all

```almide
async fn load_dashboard(user_id: String) -> Dashboard =
  do {
    concurrent {
      let profile = await fetch_profile(user_id)
      let posts = await fetch_posts(user_id)
      let stats = await fetch_stats(user_id)
    }
    // all three complete before reaching here
    // any failure → cancel siblings → propagate error
    Dashboard { profile, posts, stats }
  }
```

### race — first to complete wins, cancel the rest

```almide
async fn fetch_fastest(url: String) -> Response =
  race {
    await fetch_via_cdn(url)
    await fetch_direct(url)
  }
```

### timeout — scoped deadline

```almide
async fn fetch_safe(url: String) -> Result[Response, TimeoutError] =
  timeout 5s {
    await http.get(url)
  }
```

### Composition — concurrent + do + await

```almide
async fn checkout(cart: Cart) -> Result[Order, AppError] =
  do {
    concurrent {
      let stock = await verify_stock(cart.items)
      let payment = await authorize_payment(cart.total)
    }
    // both must succeed — if either fails, the other is cancelled
    await finalize_order(stock, payment)
  }
```

## Alternative: Function-based API (simpler, less powerful)

The SPEC currently defines function-based variants. These are a valid MVP:

```almide
let results = await parallel([fetch(url1), fetch(url2)])
let fastest = await race([fetch_cache(key), fetch_db(key)])
let data = await timeout(5000, fetch(url))
```

**Trade-off:** Function-based requires `List[Async[T]]` (homogeneous types). Block-based `concurrent {}` supports heterogeneous bindings (profile + posts + stats with different types). Recommend implementing function-based first, block-based later.

## Codegen Strategy

| Syntax | Rust output | TS output |
|--------|-------------|-----------|
| `concurrent { }` | `tokio::join!()` or `futures::join!()` | `Promise.all([])` + destructure |
| `race { }` | `tokio::select!` | `Promise.race([])` |
| `timeout T { }` | `tokio::time::timeout(Duration, fut)` | `Promise.race([task, sleep(ms)])` |
| `parallel(list)` | `futures::future::join_all()` | `Promise.all(list)` |

### Cancellation

- **Rust**: Drop the `JoinHandle` → future is cancelled. `select!` handles this natively.
- **TS**: `AbortController` + `AbortSignal` threaded through. Requires runtime cooperation.
- **WASM**: Single-threaded — `concurrent` degrades to sequential execution. `race` picks first resolved microtask.

## Implementation Phases

### Phase 1: Function-based primitives (MVP)

- [ ] Add `parallel`, `race`, `timeout`, `sleep` as stdlib async functions
- [ ] Rust codegen: `join_all`, `select!`, `tokio::time::timeout`
- [ ] TS codegen: `Promise.all`, `Promise.race`, `setTimeout` wrapper
- [ ] Replace `almide_block_on` busy-wait with proper tokio runtime
- [ ] Tests in `spec/lang/async_test.almd`

### Phase 2: Block-based syntax

- [ ] Parse `concurrent { }`, `race { }`, `timeout T { }` as expressions
- [ ] Type check: concurrent bindings available after block, heterogeneous types
- [ ] Codegen: expand to join/select with destructuring
- [ ] Cancellation semantics: drop/abort on scope exit

### Phase 3: Async streams

- [ ] `async fn* generator() -> Stream[T]` or similar
- [ ] `for await item in stream { }` syntax
- [ ] Backpressure via bounded channels

## Dependencies

- Layer 1 (`async fn` / `await`) — DONE
- Rust codegen needs tokio (or async-std) runtime instead of `almide_block_on`

## Status

Not started. Layer 1 (async/await) is implemented. Function-based API is the recommended starting point.
