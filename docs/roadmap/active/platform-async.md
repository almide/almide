# Platform Async: Transparent Async IO [ACTIVE]

## Vision

Users write `effect fn`. The compiler generates the right async code for each target. No `async`, no `await`, no `Promise`, no `tokio` in user code.

```almide
effect fn get_user(id: String) -> User = do {
  let resp = http.get("/users/{id}")
  json.parse(http.body(resp))
}
```

This single function compiles to:

| Target | Generated code | Runtime |
|--------|---------------|---------|
| `almide run` (native) | `async fn` + `.await` | tokio + reqwest |
| `--target ts` (Deno) | `async function` + `await` | fetch |
| `--target js` (Node) | `async function` + `await` | node-fetch |
| `--target wasm` (browser) | `async function` + `await` | fetch API |
| `--target wasm` (WASI) | `async fn` | wasi-http |

## Design Principle

**`effect fn` = IO function = automatically async on all platforms.**

The user never decides "is this sync or async?" — if it does IO, it's `effect fn`, and the compiler handles the rest. This eliminates:

- `async` keyword decisions
- `await` placement bugs
- `Promise` vs `Future` confusion
- `Send + 'static` lifetime errors (Rust async pain)
- sync/async function coloring problem

## User-Facing Syntax

### Sequential IO (no change from today)

```almide
effect fn main() -> Unit = do {
  let user = get_user("123")
  let posts = get_posts("123")
  println("{user.name}: {list.len(posts)} posts")
}
```

Calls execute one after another. Identical to current behavior.

### Parallel IO (new: `parallel` keyword)

```almide
effect fn main() -> Unit = do {
  let (user, posts) = parallel {
    get_user("123")
    get_posts("123")
  }
  println("{user.name}: {list.len(posts)} posts")
}
```

`parallel` block runs all expressions concurrently. Returns tuple when all complete. If any fails, the whole block fails (fail-fast).

### Timeout (new: stdlib function)

```almide
effect fn main() -> Unit = do {
  let result = timeout(5000, fn() => http.get("/slow-api"))
  match result {
    ok(resp) => println(http.body(resp))
    err(_) => println("Timed out after 5s")
  }
}
```

### HTTP Server

```almide
effect fn handle(req: Request) -> Response = do {
  let id = http.param(req, "id")
  let user = get_user(id)
  http.json(user)
}

effect fn main() -> Unit = do {
  http.serve(3000, fn(req) => handle(req))
}
```

Each request handler runs as an independent async task. The server handles thousands of concurrent connections.

### WebSocket

```almide
effect fn chat(url: String) -> Unit = do {
  let conn = websocket.connect(url)
  do {
    guard websocket.is_open?(conn) else break
    let msg = websocket.receive(conn)
    println("Got: {msg}")
    websocket.send(conn, "echo: {msg}")
  }
}
```

## Codegen Detail

### Rust Target

**Phase 1 (basic async):**

```rust
// effect fn get_user(id: String) -> User
async fn get_user(id: String) -> Result<User, String> {
    let resp = reqwest::get(&format!("/users/{}", id)).await
        .map_err(|e| e.to_string())?;
    let body = resp.text().await.map_err(|e| e.to_string())?;
    json_parse(&body)
}

#[tokio::main]
async fn main() -> Result<(), String> {
    let user = get_user("123".into()).await?;
    println!("{}", user.name);
    Ok(())
}
```

Key changes from current codegen:
- `fn` → `async fn` for all `effect fn`
- IO calls get `.await`
- `main` gets `#[tokio::main]`
- `ureq` → `reqwest` (async http client)

**Phase 2 (parallel):**

```rust
// parallel { get_user("1"), get_posts("1") }
let (user, posts) = tokio::try_join!(
    get_user("1".into()),
    get_posts("1".into()),
)?;
```

**Phase 3 (timeout):**

```rust
// timeout(5000, fn() => http.get("/slow"))
match tokio::time::timeout(
    std::time::Duration::from_millis(5000),
    http_get("/slow")
).await {
    Ok(result) => result,
    Err(_) => Err("timeout".into()),
}
```

### TypeScript Target

```typescript
// effect fn get_user(id: string) -> User
async function get_user(id: string): Promise<User> {
    const resp = await fetch(`/users/${id}`);
    return await resp.json();
}

// parallel { get_user("1"), get_posts("1") }
const [user, posts] = await Promise.all([
    get_user("1"),
    get_posts("1"),
]);

// timeout(5000, fn() => http.get("/slow"))
const result = await Promise.race([
    http_get("/slow"),
    new Promise((_, reject) => setTimeout(() => reject(new Error("timeout")), 5000)),
]);
```

### WASM Target

**Browser:**
```typescript
// Same as TS target — browser has native fetch + Promise
async function get_user(id) {
    const resp = await fetch(`/users/${id}`);
    return await resp.json();
}
```

**WASI:**
```rust
// Uses wasi-http proposal or async WASI APIs
// Falls back to sync if async WASI not available
```

## How `effect fn` Becomes Async

### Compiler Classification

The compiler classifies every function:

```
pure fn       → sync (no IO, no side effects)
effect fn     → async (IO, side effects)
```

During codegen:
1. All `effect fn` → `async fn` (Rust) or `async function` (TS)
2. All calls to `effect fn` from another `effect fn` → add `.await` (Rust) or `await` (TS)
3. Calls to `effect fn` from `pure fn` → compile error (already enforced)
4. `main` → async entry point (`#[tokio::main]` / top-level await)

### What About Non-IO Effect Functions?

Some `effect fn` functions don't actually do IO (e.g., random number generation, timestamps). These still get `async fn` codegen — it's a minor overhead but keeps the model simple. The alternative (analyzing which effect fns actually need async) adds complexity for minimal gain.

## Failure and Cancellation

`parallel` follows the same fail-fast semantics as `do` blocks:

```almide
effect fn main() -> Unit = do {
  let (a, b, c) = parallel {
    fetch_a()   // starts
    fetch_b()   // starts
    fetch_c()   // starts
  }
  // If a fails: b and c are cancelled. do propagates a's error.
  // If b fails: a and c are cancelled. do propagates b's error.
  // All succeed or all fail. Partial success is never observable.
}
```

**Rules:**
1. `do` exits on the first `Result` error → `parallel` + `do` exits on the first failed task
2. All sibling tasks are cancelled before error propagation
3. Scope exit cancels all pending tasks
4. Consistent with `do` — sequential and concurrent code follow the same rule

**Rationale:** AI doesn't need to write cleanup logic for partially-succeeded parallel operations. "All succeed or all fail" is the simplest mental model.

## `parallel` Capture Constraints

```almide
var count = 0
let (a, b) = parallel {
  do { count = count + 1; fetch_a() }   // ← COMPILE ERROR
  fetch_b()
}
```

**`parallel` bodies cannot capture mutable `var` from parent scope.** Read-only `let` bindings only. This prevents data races structurally.

## Key Design Decisions

### Decision 1: No `Future[T]` in the Type System

`Future[T]` is internal to codegen. Users never see it.

- `effect fn foo() -> Int` returns `Int` (not `Future[Int]`)
- The compiler tracks which functions are async internally
- This avoids LLM confusion with `Future[Future[T]]` or generic bounds

### Decision 2: Rust Executor — tokio

**tokio is the Rust async runtime.** Replaces the current `almide_block_on` busy-wait.

```rust
// Current (busy-wait — broken)
fn almide_block_on<F: Future>(future: F) -> F::Output {
    let waker = unsafe { Waker::from_raw(dummy_raw_waker()) };
    // ... poll loop with yield_now()
}

// After (tokio)
#[tokio::main]
async fn main() -> Result<(), String> { ... }
```

`parallel` uses `tokio::try_join!` (fail-fast). `Send` constraints avoided via `tokio::task::LocalSet` when needed.

Dependencies added to generated Cargo.toml:
- `tokio = { version = "1", features = ["rt", "time", "macros"] }`
- `reqwest` (replaces `ureq`)

### Decision 3: TypeScript — Native async/await

```typescript
// parallel { a(), b() }  →  Promise.all([a(), b()])
// timeout(5000, fn)      →  Promise.race([fn(), sleep_reject(5000)])
// sleep(100)             →  new Promise(r => setTimeout(r, 100))
```

Cancellation is best-effort (JS limitation — `Promise.race` doesn't abort losers).

### Decision 4: WASM — JSPI-Based

| WASM Spec | Phase | Almide Impact |
|-----------|-------|---------------|
| **JSPI** (JS Promise Integration) | Phase 4 (standardized) | Primary approach. WASM↔JS Promise bridge. Chrome 137+, Firefox 139+ |
| **Asyncify** (Binaryen) | Available | Fallback for older browsers. +50% code size |
| **Threads + SharedArrayBuffer** | Standardized | Not needed for Phase 1 |
| **Stack Switching** | Phase 3 | Future: cooperative scheduling in WASM |

**Phase 1: `parallel` runs sequentially on WASM** (correct but slow). Phase 2: JSPI + `Promise.all` for true concurrency.

WASM concurrency is always single-threaded cooperative multitasking. No data race issues.

### Decision 5: Duration Literals

```almide
// No Duration literals. Use milliseconds as Int.
timeout(5000, fn() => http.get(url))
sleep(100)
```

Keeps syntax simple. Duration type can be added to stdlib later if needed.

## Dependencies

### Rust
- `tokio` (async runtime) — added to generated Cargo.toml
- `reqwest` (async HTTP client) — replaces `ureq`
- No user-facing dependency changes

### TypeScript
- No new dependencies — `fetch` and `Promise` are built-in
- Deno: native `fetch`
- Node: `node-fetch` or built-in `fetch` (Node 18+)

### WASM
- Browser: JSPI (Chrome 137+, Firefox 139+), Asyncify fallback
- WASI: depends on WASI async proposal maturity

## Migration Path

### Phase 1: Async Codegen (no user code changes)

- [ ] `effect fn` → `async fn` in Rust codegen
- [ ] Add `.await` to all effect fn calls in Rust codegen
- [ ] `main` → `#[tokio::main]`
- [ ] `ureq` → `reqwest` in http_runtime.txt
- [ ] TS codegen: `function` → `async function` for effect fns
- [ ] TS codegen: add `await` to effect fn calls
- [ ] All existing `.almd` code works without changes
- [ ] `almide test` passes on both targets

### Phase 2: `parallel` Block

- [ ] Parser: `parallel { expr1; expr2 }` syntax
- [ ] AST: `Expr::Parallel { exprs: Vec<Expr> }`
- [ ] IR: `IrExprKind::Parallel { exprs: Vec<IrExpr> }`
- [ ] Checker: all exprs must be `effect fn` calls, return types form tuple
- [ ] Rust codegen: `tokio::try_join!(...)`
- [ ] TS codegen: `Promise.all([...])`
- [ ] Error semantics: fail-fast (one error → whole parallel fails)

### Phase 3: Timeout + Utilities

- [ ] `timeout(ms, fn)` stdlib function
- [ ] `sleep(ms)` stdlib function (currently `env.sleep_ms`, move to stdlib)
- [ ] Rust: `tokio::time::timeout` / `tokio::time::sleep`
- [ ] TS: `Promise.race` / `setTimeout`

### Phase 4: Server Async

- [ ] `http.serve` handler → async (each request is a tokio task)
- [ ] Connection pooling in generated code
- [ ] Graceful shutdown
- [ ] Integrate with web-framework.md design

### Phase 5: Streaming + WebSocket

- [ ] `websocket` module (connect, send, receive, close)
- [ ] `http.stream` for SSE
- [ ] Rust: `tokio-tungstenite` / `reqwest` streaming
- [ ] TS: `WebSocket` API / `ReadableStream`

## New Keywords

| Keyword | Purpose | Required? |
|---------|---------|-----------|
| `parallel` | Concurrent execution block | Yes (Phase 2) |

That's it. One new keyword. Everything else uses existing `effect fn` and `do` blocks.

## Interaction with Existing Features

| Feature | Impact |
|---------|--------|
| `effect fn` | Becomes async. No syntax change. |
| `do` block | Works the same. Auto-`?` still works. |
| `guard` | Works the same inside async context. |
| `for...in` | Sequential iteration, each step awaited. |
| Result/Option | Unchanged. `?` propagation still works. |
| Pipe `\|>` | Works. Each step awaited if effect fn. |
| UFCS | Works. `x.method()` awaited if effect fn. |
| Lambda | Closures can be `effect fn`. Captured in parallel blocks. |
| `parallel` | New. Only valid inside `effect fn`. |

## What This Does NOT Include

| Feature | Reason |
|---------|--------|
| User-visible `async` keyword | Unnecessary — `effect fn` covers it |
| User-visible `await` keyword | Unnecessary — compiler inserts it |
| `Future` / `Promise` types | Internal only — users see `Result[T, E]` |
| Manual task spawning | Use `parallel` instead |
| Channels / message passing | Deferred to supervision-and-actors.md |
| Actor model | Deferred to supervision-and-actors.md |

## Relationship to Other Roadmaps

- **structured-concurrency.md**: This replaces the `async let` / `await` design with transparent async. `parallel` replaces `async let`.
- **web-framework.md**: Server handlers become automatically async.
- **websocket stdlib**: Phase 5 of this roadmap.
- **supervision-and-actors.md** (on hold): Channels and actors are a separate layer on top of this.
