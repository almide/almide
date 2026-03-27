<!-- description: Unified async/concurrency design using effect fn and fan syntax -->
<!-- done: 2026-03-17 -->
# Fan Concurrency

> **`effect fn` is the async boundary. `fan` is the concurrency syntax. You never write async/await.**

This document unifies the following into Almide's single specification for async and concurrency:
- structured-concurrency.md (`async let` / `await` design → replaced by `fan`)
- platform-async.md (`parallel` block design → replaced by `fan`, transparent async philosophy inherited)
- HTTP module async support

---

## 1. Design Principles

### 1.1 Transparent Async

Users never write `async` / `await` / `Promise` / `Future`.

```almide
effect fn get_user(id: String) -> User = {
  let text = http.get("/users/${id}")
  json.parse(text)
}
```

This code is automatically translated to each target:

| Target | Generated Code | Runtime |
|--------|---------------|---------|
| `almide run` (native) | `async fn` + `.await` | tokio |
| `--target ts` | `async function` + `await` | fetch |
| `--target js` | `async function` + `await` | fetch |
| `--target wasm` (browser) | `async function` + `await` | fetch API |
| `--target wasm` (WASI) | `async fn` | wasi-http |
| `--target py` (future) | `async def` + `await` | asyncio |
| `--target go` (future) | normal function | goroutine |
| `--target rb` (future) | normal method | Thread / Fiber |
| `--target c` (future) | normal function | pthread |

### 1.2 Eliminating Function Coloring

```
pure fn       → sync (no side effects)
effect fn     → async (I/O, side effects)
```

- `effect fn` automatically becomes async. No `async fn` keyword needed
- Calling an `effect fn` from within an `effect fn` causes the compiler to auto-insert `.await`
- Calling an `effect fn` from a `pure fn` → compile error (existing constraint)
- `main` → async entry point (`#[tokio::main]` / top-level await)

### 1.3 LLM Optimization

Structurally eliminates patterns where LLM coding agents fail with async/await.

| Mistake LLMs make | Almide's solution |
|--------------------|-------------------|
| Forgetting `await` → Promise stored in variable | `await` doesn't exist. Compiler auto-inserts |
| Parallelizing what should be sequential | Cannot reference other expressions' results inside fan |
| Sequentializing what should be parallel | Just put it in `fan { }` for parallelism |
| Creating a task and forgetting to join | Task handles are never exposed |
| Misplacing `async` / `await` | Keywords don't exist in the first place |

There is only one decision point: **"Do these two operations have a dependency?"**

- Yes → write two `let` lines (sequential)
- No → put them in `fan { }` (parallel)

---

## 2. `fan` — Unified Concurrency Syntax

### 2.1 `fan { }` — Static Fan-Out/Fan-In

Starts a fixed number of independent effects simultaneously and waits for all to complete.

```almide
effect fn dashboard(id: String) -> Dashboard = {
  let (user, posts) = fan {
    fetch_user(id)
    fetch_posts(id)
  }
  Dashboard { user, posts }
}
```

### 2.2 `fan.map(xs, f)` — Dynamic Fan-Out/Fan-In

Executes effects concurrently for each element in a collection and returns results as a list.

```almide
effect fn fetch_all(ids: List[String]) -> List[User] = {
  fan.map(ids, fn(id) { fetch_user(id) })
}
```

### 2.3 `fan.race(thunks)` — Fastest One Only

Starts all thunks (lazily-evaluated functions) simultaneously and returns the result of the first to complete. The rest are cancelled.

```almide
effect fn fast_fetch(id: String) -> String = {
  fan.race([
    fn() { http.get("https://primary.api/users/" ++ id) },
    fn() { http.get("https://replica.api/users/" ++ id) },
  ])
}
```

### 2.4 Staged Dependency Graph

Chaining `fan` blocks makes the dependency graph directly visible in the top-to-bottom code structure.

```almide
effect fn full_dashboard(user_id: String) -> Dashboard = {
  // Stage 1: two independent operations in parallel
  let (user, location) = fan {
    fetch_user(user_id)
    geo.current_location()
  }

  // Stage 2: three operations depending on Stage 1 results, in parallel
  let (weather, posts, recs) = fan {
    fetch_weather(location.city)
    fetch_posts(user.id)
    fetch_recommendations(user.id, location.country)
  }

  // Stage 3: sequential (writes require ordering)
  fs.mkdir_p(folder)
  fs.write(path, render(user, weather, posts))
}
```

```
Stage 1:  [fetch_user]  [get_location]     <- fan (parallel)
                |              |
Stage 2:  [fetch_posts] [fetch_weather] [fetch_recs]  <- fan (parallel)
                |              |             |
Stage 3:  [write file]                       <- let (sequential)
```

---

## 3. Semantics

### 3.1 `fan { e1; e2; ...; en }`

- **Type**: `(T1, T2, ..., Tn)` — a tuple of the **success value** types of each expression
- **Execution**: all expressions start simultaneously, waits for all to complete
- **Result order**: declaration order (not completion order)
- **Result propagation**: if an expression returns `Result[T, E]`, the success value `T` goes into the tuple. If `Err`, the entire fan becomes an effect failure + remaining expressions are cancelled
- **Syntax restriction**: expressions only. `let` / `var` / `for` / `match` are prohibited
- **External variables**: `let` bindings from outer scope are readable. `var` capture is prohibited (data race prevention)
- **Statement position**: `fan { ... }` without assignment is allowed. Result is collapsed to `Unit`

### 3.2 `fan.map(xs, f)`

- **Type**: `List[T]` — a list of the **success value** type of `f`
- **Execution**: applies `f` to each element concurrently
- **Result order**: input order (not completion order)
- **Result propagation**: if `f` returns `Result[T, E]`, on `Err` the whole operation fails + remaining are cancelled
- **Future extension**: `fan.map(xs, limit: 16, f)` for concurrency limiting

### 3.3 `fan.race(thunks)`

- **Type**: `T` — the return type of the thunks
- **Arguments**: `List[Fn[] -> T]` — a list of thunks
- **Execution**: all thunks start simultaneously
- **Result**: **the first to complete** (whether success or failure)
- **Remaining**: cancelled
- **Empty list**: compile error

### 3.4 Why Thunks Are Needed

`fan.race` is a function (not syntax), so arguments are evaluated eagerly by normal evaluation rules. Wrapping in `fn() { ... }` lets `fan.race` control when execution starts. `fan { }` is syntax, so the compiler controls it and thunks are unnecessary.

### 3.5 Design Rationale for Result Propagation

**fan auto-unwraps Results.** If `Err` is returned, the entire fan fails.

```almide
let (user, posts) = fan {
  fetch_user(id)    // Result[User, String] → User on success
  fetch_posts(id)   // Result[List[Post], String] → List[Post] on success
}
// user: User, posts: List[Post]
// If either returns Err, the entire fan becomes an effect failure, remaining are cancelled
```

Rationale:

1. **Same semantics as effect fn's auto-`?`**. Consistent.
2. **Matches native behavior across all targets**. `Promise.all` propagates reject, `tokio::try_join!` propagates Err, `asyncio.gather` propagates exceptions
3. **Simplest mental model for LLMs**. Put it in fan and success values come out; on failure the entire fan fails
4. **Almide has no distinction between effect failure and Result Err**. The only error channel is `Result`

Alternatives considered but rejected:
- **Path A** (treat effect failure and Result as separate): requires a second error channel. Design becomes complex
- **Path B** (don't cancel on Err): inefficient for cases where "if one fails, the rest are useless"

### 3.6 Effect Constraint

- `fan { }` can only be used inside `effect fn`
- `fan.map` / `fan.race` are also effects
- fan inside pure fn → compile error

---

## 4. `fan`'s Position in the Language

- `fan` is a **reserved word** (`let fan = 123` is not allowed)
- `fan { }` is **special syntax** (a clause list, not a block)
- `fan.map` / `fan.race` are a **compiler-known namespace**
- User-defined `fan` modules are not allowed

---

## 5. Interaction with Existing Features

| Feature | Impact |
|---------|--------|
| `effect fn` | Becomes async. No syntax change |
| `do` block | Same behavior. auto-`?` still works |
| `guard` | Works inside async context |
| `for...in` | Sequential iteration. await at each step |
| Result / Option | No change. `?` propagation still works |
| Pipe `\|>` | For effect fn, await at each step |
| UFCS | `x.method()` awaits if effect fn |
| Lambda | effect fn lambdas allowed. Can capture inside fan |
| `fan` | New. Only inside `effect fn` |

---

## 6. Node Promise Expressiveness Mapping

| Node.js | Almide | Behavior |
|---------|--------|----------|
| `Promise.all()` | `fan { }` / `fan.map` | All succeed or reject on first failure |
| `Promise.race()` | `fan.race` | First to complete (success or failure) |
| `Promise.any()` | `fan.any` (future) | First success. AggregateError if all fail |
| `Promise.allSettled()` | `fan.settle` (future) | Collect all results (including failures) |

Full fan family:

| API | Behavior | Return value | Initial release |
|-----|----------|-------------|-----------------|
| `fan { }` | Run all, wait for all (static) | Tuple | Yes |
| `fan.map` | Run all, wait for all (dynamic) | List | Yes |
| `fan.race` | Run all, take the fastest | Single value | Yes |
| `fan.any` | Take the first success | Single value | Later |
| `fan.settle` | Collect all results (including failures) | List | Later |
| `fan.timeout` | Execute with timeout | Single value | Later |

---

## 7. Integration with HTTP Module

### 7.1 Current State

- HTTP client functions (`http.get`, `http.post`, etc.) have `effect = true`
- Rust: synchronous blocking I/O via `std::net::TcpStream`
- TS: async I/O via `await fetch(...)`
- `http.serve` handler is a pure context (cannot call effect fn)

### 7.2 After Fan Integration

**Client**: automatically becomes async as effect fn. No change needed.

```almide
effect fn load_data() -> (User, List[Post]) = {
  // concurrent requests via fan
  let (user, posts) = fan {
    http.get_json("/users/1")
    http.get_json("/posts?user=1")
  }
  (parse_user(user), parse_posts(posts))
}
```

**Server**: make handlers into effect fn (process each request as an independent task).

```almide
// Future: handler becomes an effect context
effect fn handle(req: Request) -> Response = {
  let id = http.req_path(req)
  // Can now call other effect fn from within handlers
  let (user, prefs) = fan {
    fetch_user(id)
    fetch_preferences(id)
  }
  http.json(200, json.stringify(render(user, prefs)))
}

effect fn main() -> Unit = {
  http.serve(3000, handle)
}
```

### 7.3 HTTP Async Migration Steps

| Stage | Rust | TS |
|-------|------|----|
| Current | `std::net` sync I/O | `await fetch()` |
| Phase 0 | tokio + `reqwest` async I/O | No change |
| Phase 1 | `http.serve` → `tokio::spawn` per request | No change |
| Future | connection pooling, graceful shutdown | No change |

---

## 8. Codegen Details

### 8.1 `effect fn` Codegen

| Target | `effect fn` | I/O calls | Entry point |
|--------|-------------|-----------|-------------|
| Rust | `async fn -> Result<T, String>` | `.await` auto-inserted | `#[tokio::main]` |
| TS/JS | `async function` | `await` auto-inserted | top-level await |
| Python | `async def` | `await` auto-inserted | `asyncio.run()` |
| Go | normal function | synchronous call | `func main()` |
| Ruby | normal method | synchronous call | normal execution |
| C | normal function | synchronous call | `int main()` |

Non-I/O effect fn (random number generation, timestamps, etc.) also get async codegen. Overhead is minimal. Simplicity of the model takes priority.

### 8.2 `fan { fetch_a(); fetch_b() }`

**TypeScript**:
```typescript
const [a, b] = await Promise.all([
  fetchA(),
  fetchB(),
]);
```

**Rust (tokio)**:
```rust
let (a, b) = tokio::try_join!(
    fetch_a(),
    fetch_b(),
)?;
```

**Python**:
```python
a, b = await asyncio.gather(
    fetch_a(),
    fetch_b(),
)
```

**Go**:
```go
var a TypeA
var b TypeB
var wg sync.WaitGroup
var errOnce sync.Once
var firstErr error
wg.Add(2)
go func() { defer wg.Done(); v, e := fetchA(); if e != nil { errOnce.Do(func(){firstErr=e}) } else { a = v } }()
go func() { defer wg.Done(); v, e := fetchB(); if e != nil { errOnce.Do(func(){firstErr=e}) } else { b = v } }()
wg.Wait()
if firstErr != nil { return firstErr }
```

**Ruby**:
```ruby
results = [
  Thread.new { fetch_a },
  Thread.new { fetch_b },
].map(&:value)
a, b = results
```

**C**:
```c
pthread_t t1, t2;
void *r1, *r2;
pthread_create(&t1, NULL, fetch_a_wrapper, args);
pthread_create(&t2, NULL, fetch_b_wrapper, args);
pthread_join(t1, &r1);
pthread_join(t2, &r2);
```

### 8.3 `fan.map(xs, f)`

| Target | Generated Code |
|--------|---------------|
| TS | `await Promise.all(xs.map(x => f(x)))` |
| Rust | `futures::future::try_join_all(xs.iter().map(\|x\| f(x))).await?` |
| Python | `await asyncio.gather(*[f(x) for x in xs])` |
| Go | `WaitGroup` + goroutine per element |
| Ruby | `xs.map { \|x\| Thread.new { f(x) } }.map(&:value)` |
| C | `pthread_create` per element + `pthread_join` all |

### 8.4 `fan.race(thunks)`

| Target | Generated Code |
|--------|---------------|
| TS | `await Promise.race(thunks.map(f => f()))` |
| Rust | `tokio::select! { v = f1() => v, v = f2() => v }` |
| Python | `asyncio.wait(..., return_when=FIRST_COMPLETED)` + cancel pending |
| Go | buffered channel + goroutines, first send wins |
| Ruby | `Thread` array, first `.value` wins |
| C | `pthread_create` + shared flag for first completion |

---

## 9. Per-Target Design Decisions

### 9.1 Rust: tokio

**Decision: use tokio as the default executor.**

```rust
// Current almide_block_on (busy-wait — to be removed)
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
#[tokio::main]
async fn main() -> Result<(), String> { ... }
```

- `Send` constraint avoidance: use `tokio::task::LocalSet` (single-threaded executor)
- `fan { }`: `tokio::try_join!` — fail-fast
- `fan.map`: `futures::future::try_join_all`
- `fan.race`: `tokio::select!`
- HTTP client: migrate from `std::net` to `reqwest` (async http client)

Additions to generated `Cargo.toml`:
```toml
[dependencies]
tokio = { version = "1", features = ["rt", "time", "macros"] }
reqwest = { version = "0.12", features = ["json"] }
futures = "0.3"
```

Binary size increase: ~a few hundred KB. WASM target does not use tokio (separate path).

### 9.2 TypeScript / JavaScript

**Decision: use native async/await directly.**

- `effect fn` → `async function`
- effect fn 呼び出し → `await`
- `fan { }` → `Promise.all([...])`
- `fan.race` → `Promise.race([...])`
- Cancellation is **best-effort** (JS limitation. `Promise.race` does not abort losers)
- Additional dependencies: none (`fetch` and `Promise` are built-in)

### 9.3 WASM

**Decision: JSPI-based. Sequential execution fallback in Phase 0.**

| WASM Spec | Phase | Impact on Almide |
|-----------|-------|-----------------|
| **JSPI** (JS Promise Integration) | Phase 4 (standardized) | Most important. Automatic WASM↔JS Promise bridge. Chrome 137+, Firefox 139+ |
| **Asyncify** (Binaryen) | Available | Fallback for environments without JSPI support. Code size +50% |
| **Threads + SharedArrayBuffer** | Standardized | Not needed for Phase 0 |
| **Stack Switching** | Phase 3 | Future: cooperative scheduling within WASM |

Key insight: **All WASM "concurrency" is cooperative multitasking on a single thread.** True parallelism does not exist. This works in Almide's favor — no data race issues.

Phase 0: `fan { }` degrades to sequential execution on WASM (correct but slow. No deadlocks, same results).
Phase 1: Delegate to JS side via JSPI + `Promise.all` for true concurrency.

### 9.4 WASM Async Support in Other Languages (Reference Survey)

| Language | WASM async | Constraints |
|----------|-----------|-------------|
| Rust | wasm-bindgen-futures | Future↔Promise bridge. Single-threaded |
| SwiftWasm | Two executors | cooperative (CLI) / JS event loop (browser) |
| AssemblyScript | Not supported | Waiting for Stack Switching |
| Kotlin/Wasm | Beta | GC proposal required |
| Gleam | JS target only | concurrency is at the library layer |

### 9.5 Python / Go / Ruby / C (Future Targets)

| Target | effect fn | fan | Dependencies |
|--------|-----------|-----|-------------|
| Python | `async def` + `await` | `asyncio.gather` / `asyncio.wait` | asyncio (stdlib) |
| Go | normal function | goroutine + WaitGroup / channel | none |
| Ruby | normal method | Thread / Async gem | none |
| C | normal function | pthread | pthread (POSIX) |

---

## 10. Type System Design Decisions

### 10.1 `Future[T]` Is Not Exposed in the Type System

**Decision: not exposed. Handled implicitly.**

- The return type of `effect fn foo() -> Int` is `Int` (not `Future[Int]`)
- The compiler tracks async internally
- Exposing `Future[T]` would cause LLMs to get confused with `Future[Future[T]]`

### 10.2 No Duration Literals

**Decision: represent as Int (milliseconds).**

```almide
fan.timeout(5000, fn() { http.get(url) })
env.sleep_ms(100)
```

A Duration type can be defined in stdlib in the future. Vocabulary Economy principle.

### 10.3 Prohibition of `var` Capture

```almide
var count = 0
let (a, b) = fan {
  do { count = count + 1; fetch_a() }   // <- compile error
  fetch_b()
}
```

Parent scope `var` cannot be modified from inside fan. Only `let` reads are allowed. Structurally prevents data races.

---

## 11. Current Implementation Status

### 11.1 Implemented

- Parsing of `async fn` / `await` (AST: `Decl::Fn { async: Some(bool) }`, `Expr::Await`)
- Type checking: `async fn` treated as equivalent to `effect fn`
- IR: `IrExprKind::Await`, `IrFunction { is_async }`
- Rust codegen: `async fn` → Rust `async fn`, `await` → `almide_block_on(expr)`
- TS codegen: `async fn` → TS `async function`, `await` → `await expr`
- HTTP stdlib: 22 client/server functions implemented (Rust: `std::net` sync I/O)
- **`fan { }` foundation implementation complete** (2026-03-16):
  - Lexer: `fan` reserved word
  - AST: `Expr::Fan { exprs }`
  - Parser: `fan { expr; expr; ... }` parsing
  - Checker: only allowed inside effect fn, Result auto-unwrap, type is tuple
  - IR: `IrExprKind::Fan { exprs }`
  - Rust codegen: `std::thread::scope` + `spawn` per expr (no tokio needed)
  - TS codegen: `await Promise.all([...])`
  - Formatter: `fan { }` support
  - E2E verified (`examples/fan_demo.almd`)
- **Effect isolation (Layer 1 security)** (2026-03-16):
  - Calling effect fn from pure fn is now a compile error
  - fan block inside pure fn is also an error

### 11.2 Known Issues

1. **`almide_block_on` is busy-wait**: dummy waker + `yield_now` loop. Wastes CPU, true async I/O doesn't work
2. **No `Future[T]` type**: type system uses `Result` as substitute. `await` type checking is incomplete
3. **Zero async tests**: no async-related test files created
4. **`http.serve` handler is pure context**: cannot call effect fn
5. ~~**fan unimplemented constraints**~~: `var` capture prohibition check is done

---

## 12. Implementation Phases

### Phase 0: `fan { }` Foundation — sync/thread backend

**Design policy change**: no tokio dependency. `effect fn` stays synchronous. `fan` parallelizes via `std::thread::scope`.

**Parser**:
- [x] Add `fan` as reserved word
- [x] `fan { expr; expr; ... }` → `Expr::Fan { exprs: Vec<Expr> }`
- [x] `let` / `var` / `for` / `while` inside fan → parse error

**Type checker**:
- [x] Type of `fan { e1; ...; en }` → `(T1, ..., Tn)` (Result auto-unwrap)
- [x] Only allowed inside effect fn
- [ ] Verify no variable sharing between expressions
- [x] Prohibit external `var` capture

**IR**:
- [x] Add `IrExprKind::Fan { exprs: Vec<IrExpr> }`

**Rust codegen**:
- [x] Transform to `std::thread::scope` + `spawn` per expr

**TS codegen**:
- [x] Transform to `await Promise.all([e1, e2, ...])`

**Formatter**:
- [x] Format support for `fan { }`

**Tests**:
- [x] Rust unit tests (checker_test.rs — fan in pure fn / fan in effect fn)
- [x] E2E verification (`examples/fan_demo.almd` — `almide run` execution success)
- [x] spec tests (`spec/lang/fan_test.almd` — 5 pass)

### Phase 1: async backend (future)

tokio to be added later as one backend implementation. Language spec is runtime-agnostic.

- [ ] `effect fn` → Rust `async fn` codegen (opt-in backend)
- [ ] `fan` → `tokio::try_join!` (async backend)
- [ ] Abstract spawn/join/sleep via runtime trait

### Phase 2: `fan.map` ✅

- [x] Register `fan.map(xs, f)` as compiler-known function
- [x] Type: `(List[A], Fn(A) -> B) -> List[B]` (Result auto-unwrap)
- [x] Rust: `std::thread::scope` + `spawn` per item
- [x] TS: `await Promise.all(xs.map(f))`
- [x] Tests (`spec/lang/fan_map_test.almd` — 4 pass)

### Phase 3: `fan.race` ✅

- [x] Register `fan.race(thunks)` as compiler-known function
- [x] Type: `(List[Fn() -> T]) -> T` (Result auto-unwrap)
- [x] Rust: `std::thread::scope` + `mpsc::channel` (get first completed value)
- [x] TS: `await Promise.race(thunks.map(f => f()))`
- [x] Tests (`spec/lang/fan_race_test.almd` — 2 pass)

### Phase 4: Server Async

- [ ] Make `http.serve` handler an effect context
- [ ] Rust: make each request an independent task via `tokio::spawn`
- [ ] connection pooling
- [ ] graceful shutdown
- [ ] Tests

### Phase 5: Extensions ✅ (major APIs complete)

- [x] `fan.any` — returns first success. Equivalent to `Promise.any` (Rust: `mpsc::channel`, TS: `Promise.any`)
- [x] `fan.settle` — returns all results. Equivalent to `Promise.allSettled` (Rust: thread + collect, TS: `Promise.allSettled`)
- [x] `fan.timeout(ms, thunk)` — execute with timeout (Rust: deadline loop, TS: `Promise.race` + setTimeout)
- [x] Tests (`spec/lang/fan_ext_test.almd` — 4 pass)
- [ ] `fan.map(xs, limit: n, f)` — concurrency limit (future)
- [ ] Consider moving `env.sleep_ms` to `fan.sleep` (future)

### Phase 6 (future): Streaming

- [ ] `websocket` module (connect, send, receive, close)
- [ ] `http.stream` for SSE
- [ ] Rust: `tokio-tungstenite` / `reqwest` streaming
- [ ] TS: `WebSocket` API / `ReadableStream`
- [ ] `for item in stream { }` — existing `for...in` recognizes Stream

---

## 13. What This Design Replaces

| Previous Design | Fan Replacement |
|----------------|-----------------|
| `async fn` (structured-concurrency) | Not needed. `effect fn` is async as-is |
| `await expr` (structured-concurrency) | Not needed. Compiler auto-inserts |
| `async let` (structured-concurrency) | Replaced by `fan { }` |
| `parallel { }` (platform-async) | Replaced by `fan { }` |
| `race()` stdlib | Merged into `fan.race` |
| `timeout()` stdlib | `fan.timeout` (future) |
| `sleep()` stdlib | Remains as `env.sleep_ms` / future `fan.sleep` |

## 14. What We Are Not Adding

| Feature | Reason |
|---------|--------|
| `async` keyword | Not needed — `effect fn` is the replacement |
| `await` keyword | Not needed — compiler auto-inserts |
| `Future[T]` / `Promise` type | Internal only — users see `Result[T, E]` |
| Manual task spawn | Use `fan` |
| Channels / message passing | Deferred to supervision-and-actors.md |
| Actor model | Deferred to supervision-and-actors.md |

## Keyword Addition

| Keyword | Purpose |
|---------|---------|
| `fan` | Concurrency block + namespace (the only addition) |

---

## Status

Design unification complete. Implementation starts from Phase 0 (async foundation).
