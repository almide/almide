<!-- description: Async model with async fn, await, and async let constructs -->
<!-- done: 2026-03-14 -->
# Structured Concurrency

## Philosophy

> **Almide keeps async boring on purpose: explicit fork, explicit join, automatic cancellation, and the same fail-fast semantics as `do`.**

Non-goals: novel concurrency syntax, implicit parallelism, actor primitives in the language. Almide's async is intentionally conservative — readable, hard to break, easy to implement.

## Overview

Layer 2 of Almide's async model. Three language constructs only:

| Construct | Purpose |
|-----------|---------|
| `async fn` | Declares an async function (implicitly `effect`) |
| `await expr` | Resolves `Future[T]` to `T` — one operation, always explicit |
| `async let x = expr` | Starts a concurrent task, binds a single-use handle |

Everything else (`race`, `timeout`, `sleep`) is a stdlib function, not syntax.

## Design Principles

- **No fire-and-forget** — all tasks complete within their scope
- **Cancellation propagation** — parent cancelled → children stop too
- **No task leaks** — structurally impossible in AI-generated code
- **Composes with `do` blocks** — error propagation works inside concurrent scopes
- **Minimal syntax delta** — sequential → parallel is adding one word (`async` before `let`)
- **Boring on purpose** — no novel concurrency constructs; consistency over cleverness

## Core Syntax

### `async let` for parallel execution

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

## Semantics

### `await`: one operation

`await` is a single operation: **`Future[T]` → `T`**.

- `await fetch_user(id)` — `fetch_user` returns `Future[Result[User, E]]`, `await` unwraps the future
- `async let x = expr` — `x` is a `Future[T]` (task handle), `await x` joins it
- Inside `do` block: `await` unwraps the future, `do` propagates the `Result`

This unification means `await` always does one thing: resolve a future. Error handling is always `do`'s job.

### `async let`: task lifecycle

- `async let x = expr` — immediately starts evaluating `expr` as a concurrent task. `x` is a `Future[T]` handle.
- `await x` — suspends until the task completes and returns its value. **Consumes** the handle.
- `await x` a second time is a **compile error** (handle is already consumed). To reuse the value, bind it: `let v = await x; use(v, v)`.
- Scope exit with un-awaited bindings → automatic cancellation.
- Inside `do` block: any task failure → **cancel all sibling tasks** → propagate error. Partial success is not observable.
- No new keywords — `async` and `let` are both existing.

### Failure and cancellation

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

**Rules:**

1. `do` exits on the first `Result` error → `async let` + `do` exits on the first failed task
2. All sibling tasks are cancelled before error propagation
3. Scope exit (normal or error) cancels all un-awaited handles
4. Partial success is never observable — all succeed or all fail

**Rationale:**
- Consistent with `do` — sequential and concurrent code follow the same fail-fast rule
- AI doesn't need to write cleanup logic for partially-succeeded parallel operations
- "All succeed or all fail" is the simplest mental model

### Comparison with Swift

| | Swift | Almide |
|---|---|---|
| Fork | `async let a = fetchA()` | `async let a = fetch_a()` |
| Join | `await a` | `await a` |
| Error handling | `try await a` (explicit per use) | `do` block handles all errors automatically |
| Scope exit | Un-awaited tasks auto-cancelled | Same |
| race/timeout | `TaskGroup` (manual) | `race()` / `timeout()` stdlib functions |

Almide's advantage: `do` block absorbs error handling, so no `try` noise at every join point.

## Composition with `do` blocks

```almide
async fn checkout(cart: Cart) -> Result[Order, AppError] =
  do {
    async let stock = verify_stock(cart.items)
    async let payment = authorize_payment(cart.total)
    // both must succeed — if either fails, the other is cancelled, do propagates error
    await finalize_order(await stock, await payment)
  }
```

## race / timeout — stdlib functions, not syntax

No new syntax needed. These are async stdlib functions:

```almide
// Race — first to complete wins, rest cancelled
let fastest = await race(fetch_cache(key), fetch_db(key))

// Timeout — fail if not complete within duration
let data = await timeout(5s, fetch(url))

// Sleep
await sleep(100ms)
```

---

## Multi-Target Async: Research Findings and Design Decisions

### WASM Async Support in Other Languages

| Language | WASM async | Approach | Constraints |
|------|-----------|-----------|------|
| **Rust** | wasm-bindgen-futures | Future-Promise bridge. Delegates to browser's microtask queue. No custom executor needed | WASM side is fully single-threaded |
| **SwiftWasm** | 2 executors | 1) cooperative executor (CLI/WASI, does not return control to host) 2) JS event loop executor (browser, explicit switch) | libdispatch not supported |
| **AssemblyScript** | **Not supported** | No event loop, so async/await itself does not exist. Waiting for Stack Switching proposal | Depends on WASM stack switching (Phase 3) |
| **Kotlin/Wasm** | Beta | GC proposal required. Coroutine WASM support not published | Browser version only, coroutine support unclear |
| **Gleam** | JS target only | "Erlang and JS have incompatible concurrency systems." Concurrency provided at the library layer | Actor model works only on Erlang |

### WASM Ecosystem Async-Related Specifications

| Spec | Phase | Content | Impact on Almide |
|------|---------|------|----------------|
| **JSPI** (JS Promise Integration) | **Phase 4 (standardized)**| Automatic WASM<->JS Promise bridge. Transparently call async JS APIs from synchronous WASM code. ~1us/call. Chrome 137+, Firefox 139+ | **Most important**. Base technology for Almide's WASM target |
| **Asyncify** (Binaryen) | Available | Compile-time transformation to save/restore WASM stack. Code size +50% | Fallback for environments where JSPI is unavailable |
| **Threads + SharedArrayBuffer** | Standardized | Shared memory between Workers. CORS constraints | Only when true parallelism is needed. Not needed in Phase 1 |
| **Stack Switching** | Phase 3 | WASM-level coroutines/fibers | Could become the foundation for cooperative executors in the future |

### Core Insight

**"Concurrency" in WASM environments is entirely cooperative multitasking on a single thread.** True parallel execution does not exist.

This works in Almide's favor:
- `async let` semantics are limited to "concurrency while waiting for I/O" (not CPU parallelism)
- Most LLM-written code follows the pattern of "fire multiple fetches simultaneously and wait for all"
- Complex thread safety issues do not arise

### Design Decisions

#### Decision 1: Include `Future[T]` in the type system?

**Decision: No. Handle implicitly.**

Reasons:
- Return type of `async fn foo() -> Int` is `Int` (not `Future[Int]`)
- `await` is a no-op at the type level (`T -> T`). Effect is codegen only
- In `async let x = foo()`, `x`'s type is `Int`. `await x` is also `Int`
- Same approach as Swift: `async let` bindings are "not yet available T", and `await` "makes them available"
- Exposing `Future[T]` would confuse LLMs with `Future[Future[T]]` and generics boundaries

Type checker implementation:
- `async let x = expr` -> `x`'s type is the return type `T` of `expr`. With `consumed: false` flag
- `await x` -> type is `T`. Changes to `consumed = true`
- Second `await x` -> compile error "handle already consumed"
- Bindings with `consumed = false` at scope exit -> warning (cancellation occurs)

#### Decision 2: Rust target executor

**Decision: Use tokio. But design to avoid the `Send + 'static` constraint.**

Reasons:
- `almide_block_on`'s busy-wait (dummy waker + `yield_now` loop) is not usable in production
- Custom executor has high maintenance cost and no ecosystem compatibility
- tokio is the de facto standard for Rust async

Workarounds:
- `async let` implemented with `tokio::task::JoinSet` + local references, not `tokio::spawn`
- Use `tokio::task::LocalSet` when `Send` constraint is problematic (single-thread executor)
- Replace `almide_block_on` with `tokio::runtime::Runtime::block_on`

```rust
// Current almide_block_on (busy-wait — to be deprecated)
fn almide_block_on<F: std::future::Future>(future: F) -> F::Output {
    let waker = unsafe { Waker::from_raw(dummy_raw_waker()) };
    let mut cx = Context::from_waker(&waker);
    let mut future = Box::pin(future);
    loop {
        match future.as_mut().poll(&mut cx) {
            Poll::Ready(val) => return val,
            Poll::Pending => std::thread::yield_now(),
        }
    }
}

// Replacement
fn almide_block_on<F: std::future::Future>(future: F) -> F::Output {
    tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap()
        .block_on(future)
}
```

Dependency impact:
- Add `tokio = { version = "1", features = ["rt", "time", "macros"] }` to `Cargo.toml`
- Generated binary size increase (~hundreds of KB)
- Do not use tokio for WASM target (separate path)

#### Decision 3: TS target

**Decision: Use native async/await as-is. `async let` converts to a variable binding that immediately starts a Promise.**

```typescript
// Almide: async let a = fetch_a()
// TS:     const __a_promise = fetch_a();  // starts immediately

// Almide: await a
// TS:     await __a_promise

// Cancellation implemented with AbortController
const __a_ctrl = new AbortController();
const __a_promise = fetch_a({ signal: __a_ctrl.signal });
// On scope exit: __a_ctrl.abort()
```

Challenges:
- AbortController requires all async functions to accept a `signal` parameter
- Whether to implicitly inject signal into Almide stdlib async functions, or make cancellation best-effort

**Decision: Cancellation is best-effort.** Aborting a Promise may not stop an in-flight fetch. This is a JS constraint, not a problem Almide should solve. Waiting with `Promise.allSettled` on scope exit is sufficient.

#### Decision 4: WASM target

**Decision: Implement based on JSPI. Phase 1 uses eager sequential fallback.**

With JSPI (Phase 4, Chrome 137+ / Firefox 139+), async JS APIs can be called from synchronous WASM code. For Almide's WASM target:

```
async let a = fetch_a()   ->  Sequential execution within WASM (JSPI handles suspend/resume)
async let b = fetch_b()   ->  b starts after a completes
await a                    ->  Already completed
await b                    ->  Already completed
```

In Phase 1, `async let` degrades to "sequential execution on a single thread." This is not correct but safe (no deadlocks, same results, just slower).

Future improvement paths:
1. **JSPI + Promise.all**: Bundle multiple `async let` tasks into `Promise.all` on the JS side, WASM waits for all to complete with a single suspend
2. **Stack Switching (waiting for Phase 3)**: Once cooperative scheduling is possible within WASM, true concurrency is achieved

#### Decision 5: `do` block requirement for `async let`

**Decision: `async let` can be used outside `do` blocks. However, cancellation propagation is automated inside `do`.**

Reasons:
- Requiring `do` would force async functions that do not return errors (`async fn foo() -> Int`) into `do`
- Swift also allows `async let` without `do`

```almide
// Without do — concurrent execution of error-free functions
async fn fast_compute() -> Int =
  async let a = compute_a()
  async let b = compute_b()
  await a + await b

// With do — concurrent execution with errors (with cancellation propagation)
async fn risky_compute() -> Result[Int, String] =
  do {
    async let a = try_compute_a()
    async let b = try_compute_b()
    await a + await b
  }
```

---

## Current Implementation Status (Layer 1)

### Implemented

- Parsing of `async fn` / `await` (AST: `Decl::Fn { async: Some(bool) }`, `Expr::Await`)
- Type checking: `async fn` treated equivalently to `effect fn`. `await` unwraps `Result<T, E>` to `T`
- IR: `IrExprKind::Await`, `IrFunction { is_async }`
- Rust codegen: `async fn` -> Rust `async fn`. `await` -> `almide_block_on(expr)`
- TS codegen: `async fn` -> TS `async function`. `await` -> `await expr`
- HTTP stdlib has native async functions

### Known Issues

1. **`almide_block_on` is busy-wait**: dummy waker + `yield_now` loop. Wastes CPU and real async I/O does not work
2. **No `Future[T]` type**: Type system substitutes with `Result`. `await` type checking is incomplete
3. **Zero tests**: No async-related test files exist
4. **`async let` not implemented**: Parser, checker, codegen all not started

---

## Implementation Phases (Revised)

### Phase 0: Layer 1 Stabilization (Prerequisites)

Stabilize existing async/await before proceeding to Layer 2.

- [ ] **Replace `almide_block_on` with tokio**
  - `tokio::runtime::Builder::new_current_thread().enable_all().build().unwrap().block_on(future)`
  - Codegen change to add tokio dependency to generated `Cargo.toml`
  - Add branch to not use tokio for WASM target
- [ ] **Add Layer 1 tests** (`spec/lang/async_test.almd`)
  - `async fn` declaration and calling with `await`
  - Error propagation with `async fn` + `do` block
  - Calling stdlib async functions inside `async fn` (`http.get` etc.)
  - Verify formatter support for `async fn` / `await`
- [ ] **Fix `await` type checking**: Currently `Result<T, E> -> T` but also passes through non-Result types. Check correctly based on `async fn` return type

### Phase 1: `async let` + scope-based cancellation

**Parser**:
- [ ] Add `async let name = expr` as new `Stmt::AsyncLet`
  - Add `AsyncLet { name, value, span }` variant to `Stmt` enum
  - Detect `TokenType::Async` + `TokenType::Let` pair in `parse_stmt()`
  - Usable in braceless block / do block / braced block

**Type Checker**:
- [ ] `async let x = expr` -> `x`'s type is the return type `T` of `expr`
- [ ] Attach `awaited: bool` tracking flag to `x`
- [ ] `await x` -> change to `awaited = true`. Type is `T`
- [ ] Second `await x` -> compile error "future handle already consumed"
- [ ] Un-awaited `async let` bindings at scope exit -> warning "un-awaited async binding will be cancelled"
- [ ] `async let` only usable inside `async fn` (error otherwise)

**IR**:
- [ ] Add `IrStmtKind::AsyncLet { var: VarId, value: IrExpr }`
- [ ] Add `IrExprKind::AwaitHandle { var: VarId }` (separate from `Await`; joins the handle variable)

**Rust codegen**:
```rust
// async let a = fetch_a()
// ↓
let __handle_a = tokio::task::spawn_local(async move { fetch_a().await });

// await a
// ↓
let a = __handle_a.await.unwrap();

// Scope exit (do block) -- sibling cancellation
// ↓
// JoinSet::abort_all() + drop
```
- [ ] `tokio::task::LocalSet`-based spawn (avoiding `Send` constraint)
- [ ] Inside `do` block: `JoinSet::abort_all()` on first error
- [ ] Generate drop guard at scope exit
- [ ] `#[tokio::main]` or `LocalSet::new().run_until()` wrapper at main function entry point

**TS codegen**:
```typescript
// async let a = fetch_a()
// ↓
const __a_promise = fetch_a();

// await a
// ↓
const a = await __a_promise;

// Error propagation inside do block handled by existing try/catch
```
- [ ] `async let` -> convert to `const` binding that immediately starts a Promise
- [ ] `await x` -> `await __x_promise`
- [ ] Inside `do` block: cleanup with `Promise.allSettled` is best-effort

**WASM codegen**:
- [ ] Phase 1 uses eager sequential fallback: `async let a = f()` is equivalent to `let a = await f()`
- [ ] Compiler warning: "WASM target: async let runs sequentially"

**Tests** (`spec/lang/async_let_test.almd`):
- [ ] Basic: retrieve value with `async let` + `await`
- [ ] Multiple: start 3 `async let` simultaneously and `await`
- [ ] Consumption: calling `await x` twice causes compile error
- [ ] Error propagation: 1 failure in `do` -> rest cancelled
- [ ] Scope exit: warning for un-awaited binding

### Phase 2: `race` / `timeout` / `sleep` stdlib

- [ ] Add definitions to `stdlib/defs/async.toml`
- [ ] `race(futures...)`: return first to complete, cancel the rest
- [ ] `timeout(duration, future)`: `err("timeout")` if not complete within duration
- [ ] `sleep(duration)`: wait for specified time
- [ ] Parser support for Duration literals (`5s`, `100ms`) — or — simplify with `sleep(5000)` (ms as Int)

**Rust codegen**:
```rust
// race(a, b) → tokio::select! { v = a => v, v = b => v }
// timeout(5000, f) → tokio::time::timeout(Duration::from_millis(5000), f).await
// sleep(100) → tokio::time::sleep(Duration::from_millis(100)).await
```

**TS codegen**:
```typescript
// race(a, b) → Promise.race([a, b])
// timeout(5000, f) → Promise.race([f, new Promise((_, rej) => setTimeout(() => rej(new Error("timeout")), 5000))])
// sleep(100) → new Promise(r => setTimeout(r, 100))
```

- [ ] Tests (`spec/stdlib/async_test.almd`)

### Phase 3: Async streams (future)

- [ ] `Stream[T]` type
- [ ] `stream.for_each(fn(item) => ...)`, `stream.map(...)`, `stream.collect()`
- [ ] `for item in stream { }` -- existing `for...in` recognizes Stream
- [ ] Backpressure via bounded channels
- [ ] Note: `for await x in stream { }` syntax will not be added. Handle with existing `for...in` syntax

---

## Open Questions

### Q1: Should Duration literals be added to the language?

```almide
// Option A: Duration literals (new syntax)
await sleep(100ms)
await timeout(5s, fetch(url))

// Option B: Express as Int (milliseconds) (no new syntax)
await sleep(100)
await timeout(5000, fetch(url))
```

**Tentative decision: Option B**. Vocabulary Economy principle. Duration type can be defined in stdlib.

### Q2: Allow `async let` with `var` too?

```almide
async let x = fetch()     // immutable — OK
async var x = fetch()     // mutable? — semantically meaningless
```

**Decision: `async let` only.** `async var` is semantically contradictory (handles cannot be mutable).

### Q3: Capture constraints for `async let`

```almide
var count = 0
async let a = do {
  count = count + 1    // Can parent scope var be modified?
  fetch(url)
}
```

**Decision: Prohibited.** `async let` body cannot capture parent scope `var`. Only reading `let` bindings is allowed. Reason: structurally prevent data races in concurrent execution.

### Q4: Nested `async let`

```almide
async let a = do {
  async let b = fetch_inner()    // Can this be nested?
  await b
}
```

**Tentative decision: Allowed.** Same as Swift. Inner `async let` managed by inner scope.

---

## Dependencies

- Layer 1 (`async fn` / `await`) — DONE (stabilization needed)
- Phase 0: tokio introduction + test addition
- Phase 1: changes to parser, checker, IR, and codegen

## Status

Design revision complete. Implementation starts from Phase 0 (Layer 1 stabilization).
