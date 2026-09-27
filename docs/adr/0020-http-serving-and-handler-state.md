# ADR-0020: A served handler cannot reach a `var`; state lives in a host-independent KV store; the app is a value and every host is a thin shell

- **Status**: Accepted. The maintainer set the direction on 2026-09-27 (the four
  pillars below). This record makes the direction precise, backs it with
  evidence, and gives the implementation order. None of it is implemented yet.
  Refinements that the evidence forced are marked **[refined]** where they
  appear, and are listed together in §9.
- **Date**: 2026-09-27
- **Scope**: `http.serve` and the `http` router family, `fan` bodies, a new `kv`
  stdlib module, the `almide build --target wasm` shape of a server, and the
  contract ledger rows for these (C-367 and new rows).
- **Related**: #2665 (native serve is sequential), #2659 (a stock wasm
  artifact for a server), #2664 (effect fn values on the structural leg, a
  prerequisite, PR #2693), #2692 (SIGTERM loses stdout), #2588 (router, closed;
  #2665 and #2659 are its split-outs), #2594 (`fan.map` effect callbacks run
  sequentially), #1696 (retire the incumbent wasm emitter),
  [ADR-0011](./0011-execution-substrate-is-a-free-variable.md) (the substrate
  is a free variable), [ADR-0002](./0002-fallibility-effect-orthogonal.md) §D6
  (an effect is fallible).
- **Evidence base**: `../almide-references/RESEARCH-http-serve-concurrency.md`
  (23 measured configurations M1–M23 and 18 systems read from source S1–S18,
  2026-09-27; probes and raw outputs in
  `../almide-references/research-2026/http-serve-concurrency/`), the
  primary-source reading of wasi:keyvalue, wasmtime, Spin, Workers KV and
  Durable Objects cited in §4, the #2659 prototype (branch
  `stock-wasm-serve-2659`, `research/spike/p3-http-handler-probe/`), and the
  migration measurement in §6 (`research/spike/concurrent-var-reach/reach.py`).

## 1. Context

### 1.1 One handler, four answers

The probe handler increments a counter kept wherever the language keeps mutable
state, sleeps 2 s on `/slow`, and reads the counter again. The driver sends
`/warm`, then `/slow` at t=0, `/fast` at t=0.3 s, then three sequential `/seq`.

| Observable answer | `/slow` n→after | `/fast` latency | Where measured |
|---|---|---|---|
| shared, sequential | 2→2 | **1.70 s** | Almide 0.64.0 native, captured and top-level `var` (M1, M2); the embedded wasm lane (M3); Node busy-wait (M5); Python `HTTPServer` (M9); asyncio + `time.sleep` (M12); Rust std, no threads (M17) |
| shared, concurrent | 2→**6** | 0.00 s | Node `await` (M4); Go `net/http` (M6, and `-race` reports `DATA RACE` at run time, M7); Python threading (M8); asyncio (M11); Rust `Arc<Mutex>` (M16); axum (M19, M20) |
| isolated | **1→1**, and every request sees `n=1` | 0.00 s | Python `ForkingMixIn` (M10); Rust `thread_local!` + a thread per connection (M14), which is exactly Almide's top-level-`var` lowering; `wasmtime serve` P2 and Spin P2, which run a fresh instance per request (S12, S13) |
| split per worker | 1, 1, 2, 2, depending on which worker ran the request | 0.00 s | Rust `thread_local!` + a pool of 4 (M15); wado `serve` (S9); Actix (S16); Workers globals (S11) |

Almide's own hosts already cover three of these rows:

- native and the embedded lane (`almide run --target wasm`, C-367) are sequential;
- `wasmtime serve` with a P2 component is isolated;
- `wasmtime serve` with a P3 component reuses one instance for up to 128
  requests, 16 at a time (S12; measured by the #2659 prototype: a guest
  counter went 1, 2, 3 under wasmtime 47.0.2);
- Workers reuses an isolate "including concurrent requests" with no routing
  guarantee (S11).

**Only a handler that cannot observe mutable state gives the same answer on all
of them.** A counter in a `var` is not portable across Almide's own targets.

### 1.2 What the close peers do

None of the 41 systems changes its serving mode based on what the handler body
touches (research §4.3). The languages Almide resembles either make shared
mutable state inexpressible, or make it a compile error:

- **inexpressible**: Gleam (S1), Erlang/Elixir (S2), Roc (S4: "Durable mutable
  state belongs in SQLite or an external service"), aver (S7: "No shared
  mutable state by design"), and Koka (S5: a local `var` cannot escape);
- **a compile error**: Rust (E0277 on `Rc<Cell>` into a thread, M13, and on
  axum `State<Rc<Cell>>`, M18) and Swift 6 (`mutation of captured var 'counter'
  in concurrently-executing code`, M21; `main actor-isolated var ... can not be
  mutated from a nonisolated context`, M22).

Rust's `Send` check does **not** catch Almide's top-level-`var` shape
(`thread_local!`, M14/M15): that shape compiles and silently splits.

### 1.3 What Almide does today

- `fan { … }` refuses a captured local `var` with E008 (the research probe
  `raw/almide-fan-var.txt`). E008 does not reach top-level vars or vars
  reached through a called fn.
- `fan.map` with a callback that reaches a `var` (captured, top-level, or
  through a called `effect fn`) is **accepted and runs sequentially**. The
  lowering only goes parallel for a pure lambda literal with Send-safe
  scalar captures (`runtime/rs/src/fan.rs:23-40`, #2044). This is a content-based
  switch; it is safe only because both twins give the same results. #2594 is
  the cost: effect callbacks are always sequential.
- Native `http.serve` answers one connection at a time
  (`runtime/rs/src/http.rs:736-743`). The handler is an `Rc<dyn Fn>`, which is
  not `Send`.

Almide's mission metric is modification survival. The worst possible outcome
is a behaviour change that is silent on edit. The provisional plan, "concurrent
unless the handler reaches a `var`, then sequential with a warning", is such a
change: one added `counter = counter + 1` would flip a server's latency, and
the only signal would be a warning. No peer does this.

## 2. Decision

Four pillars.

1. **A function handed to a concurrent host cannot reach a `var`, and the
   checker enforces it.** This covers `http.serve` / `http.route` /
   `http.mount` / `http.wrap` handlers and middleware, and every `fan` body.
   Captured, top-level, and transitively reached vars are all refused (§3). The
   rule generalizes E008 and keeps its code. Effects (IO, random, the clock,
   the http client) stay allowed. The rule closes the `fan.map`
   silent-sequential hole.
2. **Cross-request state lives only in an explicit, host-independent key-value
   store** (§4): the `kv` module, with atomic `update(key, f)` for a pure `f`,
   linearizable per key. Native keeps it in an in-process map behind a lock;
   the embedded lane keeps it in a host map. **[refined]** A stock wasm host
   backs it only if it passes the conformance fixture. None does today, so
   `kv` is E081 on `almide build --target wasm` until one does (§4.4).
3. **The app is a value; entry points are thin host shells** (§5).
   `let app = http.router([...])` is a top-level value, and
   `http.serve(port, app!)` in `main` is the native and embedded-lane shell.
   `almide build --target wasm` exports the same app as
   `wasi:http/incoming-handler`, which replaces the p3-sockets plan for #2659.
   **[refined]** The served expression must be closed over top-level items: it
   cannot capture a local of `main` (§5.2). On an export host `main` is not an
   entry point (§5.4). Testing an app is a plain call:
   `let h = app!` then `h(http.new_request(...))!`.
4. **The contract is HTTP-semantic, not byte-level** (§7). C-367 changes from
   "response bytes identical" to "status, header set, body identical". Log
   lines are atomic, and their order across concurrent requests is
   unspecified. Native serve is concurrent: a worker pool, one app instance per
   worker, and per-worker RNG reseeding (§5.5). SIGTERM drains in-flight
   requests and flushes output (§5.6). Limits come as a record (§5.7).

## 3. The checker rule (pillar 1)

### 3.1 Sites

A **concurrent slot** is a parameter whose argument may run at the same time as
other invocations of itself or of its siblings. The stdlib declares them:

| Surface | Concurrent slot(s) |
|---|---|
| `http.serve(port, f)` and `http.serve_with_limits(port, f, limits)` | `f` |
| `http.route(pattern, handler)` | `handler` |
| `http.mount(prefix, sub)` | `sub` |
| `http.wrap(handler, middleware)` | `handler`, every element of `middleware` |
| `http.router(routes)` | none of its own. Its handlers entered through `route` / `mount`, which are already sites. |
| `fan { a, b, … }` | every arm |
| `fan.map` / `fan.settle` / `fan.any` / `fan.any_map` / `fan.race` / `fan.timeout` / `fan.bounded` | every fn-valued argument (callback or thunk list) |

The declaration is a stdlib attribute on the parameter (spelled by the
implementing PR; `@concurrent` is the working name). Nothing is keyed on a
function name. A **site** is one argument expression `E` in a concurrent slot.

**Slot inference through user wrappers.** If `E`, after following `let`s, is a
parameter `p` of the enclosing fn, then `p` becomes a concurrent slot of that
fn. The check moves to every call site of that fn, and this repeats to a
fixpoint. Example: `fn serve_on(port: Int, h: HttpHandler) = http.serve(port, h)`
makes `h` a concurrent slot of `serve_on`. The inferred slot is part of the
fn's interface (`almide compile`), so it crosses package boundaries.

### 3.2 The executable closure of a site

A **body** is a fn body, a lambda body, or a `fan` arm.

A lambda is **folded** into the body that syntactically contains it when it is
a direct argument of a callback parameter that is *not* a concurrent slot of a
stdlib function. Such a function calls the callback before it returns and never
stores it: `list.map`, `list.fold`, `option.map`, `map.fold` and the like. A
folded lambda runs inside the invocation of the enclosing body. Every other
lambda is its own body.

The **executable closure** `X(E)` is the least set of bodies such that:

1. every unfolded lambda that syntactically occurs in `E` (and `E` itself, if
   `E` is a `fan` arm) is in `X`;
2. if a body in `X` names a top-level fn `g`, whether it calls `g` or uses it as
   a value, then `g`'s body is in `X`;
3. if a body in `X`, or `E` itself, names a top-level `let` `x`, then the
   unfolded lambdas in `x`'s initializer, and the bodies of the top-level fns it
   names, are in `X`. This is how closures stored in records and lists are
   reached: `let app = http.router([http.route("GET /", h)])` puts `h`'s body in
   `X` through `app`;
4. the same holds for a local `let` bound outside the body that names it;
5. every unfolded lambda that syntactically occurs inside a body in `X` is in
   `X`. It may be called, so the rule is conservative.

### 3.3 Violation

`E` **reaches** a `var` `v` if some body `B` in `X(E)` names `v`, as a read or
an assignment, and `v` is declared outside `B`.

This single clause covers each shape:

| Shape | Why it is outside `B` |
|---|---|
| `var c` in `main`, captured by the handler lambda | `c` is declared in `main`'s body, and the lambda is its own body |
| a top-level `var` | it is declared outside every body |
| a top-level `var` written inside `effect fn bump()`, which the handler calls | `bump`'s body is in `X` (rule 2), and the var is outside it |
| a factory `effect fn mk() -> HttpHandler = { var c = 0; (req) => { c = c + 1 … } }` | the returned lambda is unfolded (rule 5), so it is its own body, and `c` is outside it |
| a `fan` arm reading an enclosing `var` | the arm is its own body |

Not violations:

- a `var` declared inside the handler, or inside a fn the handler calls, is
  per invocation;
- a `var` used by a folded lambda inside such a fn, for example
  `var c = 0; xs |> list.map((x) => x + c)`, is also per invocation;
- a `let` copy of a var's value taken outside the site, such as
  `let scale = scale0`, is a snapshot, not a reach. This is exactly E008's
  existing fix-it (`tests/diagnostics/e008-lambda-callback-capture/fixed.almd`).

Effects are orthogonal. A site may do IO, draw random numbers, read the clock,
call the http client, and use `kv`.

The rule is intra-package with interface summaries. Each top-level fn carries
two facts in its compiled interface: "reaches a top-level var" (with a witness
path) and its inferred concurrent slots. A package that exposes an effect fn
touching its own top-level `var` therefore makes a downstream handler that
calls it fail with a witness naming that fn.

### 3.4 Diagnostic

The code stays **E008**. Its title generalizes from "fan block captures mutable
variable" to "a concurrently-run body reaches a `var`". Three variants, each
naming the site and a witness path:

```
error[E008]: this handler can reach `hits`, a `var` — handlers run concurrently
  --> app.almd:14:3
  in the handler passed to http.route (reached through fn `bump` at app.almd:4)
  hint: a handler cannot share mutable state. Keep state that must outlive a
        request in a kv store: `kv.incr(counts, "hits", 1)!`. For a value fixed
        before serving, use a top-level `let` instead of `var`.
```

```
error[E008]: this fan.map callback can reach `total`, a `var` — fan bodies run concurrently
  hint: return each element's contribution and combine the results after the fan,
        e.g. `fan.map(xs, f)! |> list.sum`.
```

The `fan { … }` block wording stays as it is today:

```
error[E008]: cannot capture mutable variable 'count' inside fan block
  hint: Use a `let` binding instead of `var` for values shared across fan expressions
```

Fix-it verdict:

- **mechanical** when the reached var is assigned nowhere in the program: the
  fix is to declare it `let`;
- **conditional** otherwise: bind a `let` copy before the site (read-only use),
  or move the state into `kv`.

`docs/diagnostics/E008.md` is rewritten in the implementing PR.

**Interaction with existing codes.** E011 ("mutable variable mutated inside a
closure in a pure function") stays. It is why the factory shape above must be
an `effect fn`, and it runs before E008. The new rule subsumes every current
E008 case: the analyzer flags all 12 broken sites of the nine
`tests/diagnostics/e008-*` fixtures and none of the fixed ones (§6). The E008
check therefore moves from the `fan` arm of the inferrer
(`crates/almide-frontend/src/check/infer_calls_closures.rs:215-234`) to one
post-inference pass over sites.

### 3.5 Staging

The rule refuses programs that compile today. The dialect ledger's criterion
("a previously-accepted program rejected", `proofs/dialect-epochs.toml:13-15`)
makes that an epoch. **Decision: it is an error at a new dialect epoch 6, with
no warning release.** Reasons:

1. The measured impact is zero written programs (§6).
2. Epochs 4 and 5 set the precedent: both became errors at their epoch in 0.63.0.
3. A warning window would ship at least one release in which a concurrent
   native serve (step 4 in §8) could silently give the M14/M15 answers. The
   rule must be an error **before** serving becomes concurrent. That fixes the
   order of the plan.
4. The research's own verdict: a warning is the signal most likely to be lost
   in an LLM edit loop (research §6).

The epoch entry's `breaks` line reads: "a `var` reachable from an http handler
or a `fan` body (captured, top-level, or through a called fn) is E008; keep
cross-request state in `kv`, bind a `let` copy for read-only use".

## 4. The store (pillar 2)

### 4.1 What the backends actually offer (primary sources)

| Backend | API | Stated guarantees | Can it carry a linearizable per-key `update`? |
|---|---|---|---|
| **wasi:keyvalue**, Phase 2 (README.md:7). On main: `@0.2.0-draft2`; the only tag is `v0.2.0-draft` | `store.open(identifier) -> result<bucket, error>`, and on `bucket`: `get`, `set`, `delete`, `exists`, `list-keys(cursor)`. draft2's `atomics` adds `resource cas { new(bucket, key); current() }`, `swap(cas, value) -> result<_, cas-error>`, and `increment(bucket, key, delta: s64) -> result<s64, error>`. The tagged draft has only `increment(…, delta: u64) -> u64`. `batch`: get/set/delete-many | store.wit:1 "eventually consistent key-value operations"; store.wit:14-25 "must have enough consistency to guarantee 'reading your writes' … These guarantees only apply to the same client"; atomic.wit:3-5 "either completed successfully or did nothing at all"; batch.wit:10-11 "does not guarantee atomicity". Open PR #56 would weaken the store guarantee to eventual consistency only; open PR #59 says mixing `increment` with other operations on one key is "implementation-dependent" | Only through a draft2 `cas` loop, and the proposal never says "linearizable" |
| **`wasmtime serve` 47** (`-S keyvalue`) | `wasi:keyvalue@0.2.0-draft` (wasmtime `crates/wasi-keyvalue/wit/deps/keyvalue/world.wit:1`); in-memory backend, identifier `""` only (lib.rs:164-171) | `open` returns `Bucket { in_memory_data: self.ctx.in_memory_data.clone() }` (lib.rs:166-167): every bucket is a **private copy** of the preset data. `serve` builds a fresh context per instance (`src/commands/serve.rs:443-457`), and P2 reuse defaults to 1 | **No.** A write is invisible to the next request, and even to a second `open` in the same request. No CAS |
| **Spin 3.x / 4.1** | `wasi:keyvalue@0.2.0-draft2` (`wit/deps/keyvalue-2024-10-17/`), the `spin:key-value@3.0.0` store (no atomics), and the older `fermyon:spin/key-value`. The `"default"` store is SQLite (runtime-config lib.rs:459-503) | The docs make no consistency or atomicity statement (kv-store-api-guide). The SQLite `increment` is transactional under a `Mutex<Connection>` and stores **8-byte i64 LE**; it panics the host on a key of another length (`crates/key-value-spin/src/store.rs:326-359`). The CAS `swap` compares by value (l.376-457), but when `current()` saw no key it does an **unconditional upsert** (l.436-446) | For existing keys, yes. **Not on first insert**: two racing creators both "succeed" |
| **Cloudflare Workers KV** | `get` / `put` / `delete` / `list` | "KV achieves high performance by being eventually-consistent"; changes "may take up to 60 seconds or more to be visible in other global network locations"; "KV is not ideal for applications where you need support for atomic operations"; "a maximum of 1 write to the same key per second" (developers.cloudflare.com/kv/concepts/how-kv-works/, /kv/api/write-key-value-pairs/) | **No.** No increment, no CAS, last write wins |
| **Cloudflare Durable Objects** | `ctx.storage.kv.get/put/delete/list`, `transaction`, `transactionSync`, `sql.exec`; routed by `idFromName(name)` | "durable, transactional, and strongly consistent storage"; "single-threaded and cooperatively multi-tasked"; "a series of reads followed by a series of writes (with no other intervening I/O) are automatically atomic" (developers.cloudflare.com/durable-objects/api/sqlite-storage-api/, /concepts/what-are-durable-objects/) | **Yes**, but only through a JS Durable Object class, not a wasm host import. Workers has no official wasi:http component support either ("WASI support is experimental … only some syscalls implemented", /workers/runtime-apis/webassembly/) |
| **Deno KV** (design reference only) | `atomic().check(...).set(...).commit()` with versionstamps; `sum` on `KvU64` | "will only commit if the specified versionstamps match"; writes "always use strong consistency" (docs.deno.com/deploy/kv/transactions/) | Yes. A check with `versionstamp: null` makes "absent" a version, which avoids Spin's first-insert hole |

Two facts decide the design:

- `wasmtime serve` cannot hold handler state through wasi:keyvalue at all.
- The two stock wasi:keyvalue hosts implement **different package versions**
  (`@0.2.0-draft` against `@0.2.0-draft2`). A component that imports one does
  not link on the other.

### 4.2 The Almide surface

```almide
import kv

/// A handle naming a store. Pure: no IO, so it can be a top-level `let` and
/// captured by any handler.
fn store(name: String) -> KvStore

effect fn get(s: KvStore, key: String) -> String?
effect fn set(s: KvStore, key: String, value: String) -> Unit
effect fn delete(s: KvStore, key: String) -> Unit
/// Atomic read-modify-write, linearizable per key. `f` gets the current value
/// (none if absent) and returns the new one (none deletes the key). The result
/// is the value now stored. `f` is PURE (a plain arrow type), so a backend that
/// retries may call it more than once, and nobody can tell.
effect fn update(s: KvStore, key: String, f: (String?) -> String?) -> String?
/// `update` over the decimal text of an Int, defined in Almide over `update`
/// and never mapped to a host `increment` (the encodings differ between hosts,
/// §4.1). A missing key counts as 0; a non-Int value is err and is left
/// unchanged; the arithmetic wraps like `+`.
effect fn incr(s: KvStore, key: String, delta: Int) -> Int
```

Every operation is an effect and fallible (ADR-0002 §D6). Callers write `!`.
The errors mirror wasi:keyvalue's `error` (`no-such-store`, `access-denied`,
`other(string)`) as `err` strings.

**[refined] Naming.** The module is `kv` and the handle type is `KvStore`. It is
not `store` / `Store`, for two measured reasons:

- a local named like a module shadows it silently (`let json = "x"` under
  `import json` passes `almide check` with only E060), and `store` is a common
  local name;
- `http` already avoids bare nominal names that user types collide with
  (`stdlib/http.almd:4-6`: "NOT named `Request`/`Response`").

The WIT-level shape that a conforming stock host must serve is the
wasi:keyvalue draft2 subset below. The Almide shell maps `update` to a CAS loop
over it.

```wit
// imported from wasi:keyvalue@0.2.0-draft2
store.open: func(identifier: string) -> result<bucket, error>;
bucket.get: func(key: string) -> result<option<list<u8>>, error>;
bucket.set: func(key: string, value: list<u8>) -> result<_, error>;
bucket.delete: func(key: string) -> result<_, error>;
atomics.cas.new: static func(bucket: borrow<bucket>, key: string) -> result<cas, error>;
atomics.cas.current: func() -> result<option<list<u8>>, error>;
atomics.swap: func(cas: cas, value: list<u8>) -> result<_, cas-error>;
```

Two things are deliberately out:

- `increment`: its encoding is host-defined (decimal text in wasmtime, 8-byte
  LE in Spin), and mixing it with `get` is implementation-dependent (PR #59);
- `list-keys`, batch and TTL: none of them is needed for correct counters,
  sessions or caches (below). `list-keys` "MAY show an out-of-date list"
  (store.wit), batch "does not guarantee atomicity", and TTL exists on only one
  of the backends above. Adding any of them is a separate record.

### 4.3 Why this is the minimal set

- **Counter**: `kv.incr(hits, "total", 1)!`. It is linearizable, so N
  concurrent requests end at exactly N.
- **Session**: `kv.set(sessions, id, data)!` on login, `kv.get(sessions, id)!`
  on each request, `kv.delete(sessions, id)!` on logout. Each key has a single
  writer at a time, so no update is needed.
- **Cache**: compute the value outside `f` (effects stay outside), then
  `kv.update(cache, url, (cur) => some(cur ?? fetched))!`. The first writer
  wins, and every racing request returns the stored value.

A full program, and its test:

```almide
import http
import kv

let counts = kv.store("counts")

effect fn hit(req: HttpRequest) -> HttpResponse = {
  let n = kv.incr(counts, "hits", 1)!
  http.response(200, "n=${n}")
}

let app = http.router([http.route("GET /hit", hit)])

effect fn main() -> Unit = http.serve(8080, app!)!

test "two hits count to two" {
  let h = app!
  let _ = h(http.new_request("GET", "/hit", "", map.new()))!
  let r = h(http.new_request("GET", "/hit", "", map.new()))!
  assert_eq(http.body(r), "n=2")
}
```

### 4.4 Per-backing semantics

| Where the program runs | Backing | Consistency | Durability | Visibility |
|---|---|---|---|---|
| native (`almide run`, a built binary) | a process-global map, sharded, with one `Mutex` per shard; `f` runs under its key's shard lock, exactly once | linearizable per key, for every operation | the process lifetime; gone at exit | every worker thread, at once |
| embedded lane (`almide run --target wasm`) | a map in the almide host process, reached through new host ops | the same | the same | the same |
| `almide test`, on both lanes | a fresh, empty map for **each `test` block**; every name starts empty | the same | the block | only that block. An app called with `http.new_request` inside the block sees the block's map |
| stock wasm (`almide build --target wasm`) | **[refined]** wasi:keyvalue draft2 on a host that passes the conformance fixture (a concurrent first-write race, a CAS-retry race, and persistence across instances) | linearizable per key on a conforming host | the host's (Spin's default SQLite file is durable) | the host's |

**Stock availability today: none passes.**

- wasmtime serve 47 keeps no state across requests or `open`s, and has no CAS.
- Spin has the first-insert hole.
- Workers has no wasi:keyvalue, and its KV is not atomic.

So `kv.*` is **E081 on the stock-p1 and component legs** in
`proofs/target-availability.toml`. The reason names this record and the missing
conformance. A stateless app exports everywhere. A stateful app runs natively
or on the embedded lane, and exports once a host passes. This is the
"correct or refused" reading of "correct on every host". The other readings
(a counter that silently resets per request on `wasmtime serve`, or loses
updates on Workers KV) are the M10/M15 failure modes that pillar 1 exists to
exclude.

## 5. The app, the hosts, and `main` (pillar 3)

### 5.1 One program, every host

```almide
let app = http.router([ http.route("GET /users/{id}", get_user), … ])   // Result[HttpHandler, String]
effect fn main() -> Unit = http.serve(8080, app!)!                        // the socket hosts' shell
```

A top-level `let` initializer cannot call an effect fn (measured: E006 on
`let port = env.millis()`). So `app` is a pure function of the program text,
and evaluating it once per worker or once per instance is unobservable. The
Rust lowering already makes a top-level `let` holding an `Rc` value a
**per-thread lazy slot**. The emitted code for `let app = http.router(...)` is
`thread_local! { static SLOT … = Box::leak(Box::new(almide_rt_http_router(...))) }`
behind `__AlmideTl_APP`, and a `String` `let` is a shared `LazyLock` (measured
on 0.64.0 with `--target rust`). Per-worker app instances therefore cost
nothing new in codegen.

`router` returns `Result`, and `!` is not allowed in a top-level initializer
(E022, measured). So the shell writes `app!`:

- native and the embedded lane evaluate it once before binding, so a broken
  route table exits 1 before listening, as today;
- an export host evaluates it at instance initialization, where an `err`
  fails the initialization with the message on stderr.

### 5.2 **[refined]** The served expression is closed over top-level items

The argument of `http.serve` / `http.serve_with_limits` must be
**instance-closed**: every name that its executable closure (§3.2) reads, and
that none of those bodies binds, must be a top-level item (a fn, a `let`, a
type or a constructor). Main's locals do not exist on an export host (§5.4),
and native worker threads build their own instance (§5.5). A new code,
allocated by the implementing PR, reads:

```
error[E0NN]: the app passed to http.serve captures `boot`, a local of main
  hint: the app runs in several instances — one per worker natively, one per
        request on a wasi:http host — and main's locals do not exist there.
        Make `boot` a top-level `let`, compute it inside the handler, or keep it
        in a kv store.
```

This retires the one promise in C-367 that no instance host can keep: "a value
main computed before `serve` (a random draw, a clock read) … is the same on
every request of the run". The research names it as the open P2 divergence
(research §6, "Out of scope for F"). Its fixture
(`spec/serve_cross/http_serve_replay.almd:52-56`, `boot`) migrates to a
handler-local value, or to `kv.update(meta, "boot", (b) => some(b ?? drawn))!`.

### 5.3 Hosts and what the program can observe

| Host | Instance model | What the program can observe (pillars 1–2 hold) |
|---|---|---|
| native (`almide run`, a binary) | one process; a pool of worker threads, each with its own app instance, at most `max_in_flight` at once (§5.5) | responses; `kv`; whole log lines, in an unspecified order across requests; timing |
| embedded lane (`almide run --target wasm`) | one instance, sequential, in accept order (unchanged from C-367) | the same answers; a slow request delays the next one (timing only) |
| `wasmtime serve` 47, P2 component (`wasi:http/incoming-handler@0.2.x`; wasmtime 47 carries `wasi:http@0.2.12`) | a fresh instance per request, never concurrent (`--max-instance-reuse-count` defaults to 1 for P2, and "setting it to more than 1 will have no effect for WASIp2 components since they cannot be called concurrently", `serve --help`, serve.rs:141-155) | the same answers; `kv` is E081 (§4.4); hyper writes the head (§7) |
| `wasmtime serve` 47, P3 component (`wasi:http/handler@0.3.0`) | reuse up to 128, up to 16 concurrent per instance, dropped after 1 s idle; needs `-W component-model-more-async-builtins -S p3 -S http` (measured by the #2659 prototype: it does not load without the `-W` flag) | the same answers, provided the guest shell serializes handlers within an instance (see below) |
| Spin 3.x / 4.x | P2: a fresh instance per request; 3.6 P3: reused "both concurrently and serially" (spinframework.dev/blog/announcing-spin-3-6) | the same answers; not measured here |
| Cloudflare Workers | no official component or wasi:http support; only an unofficial route (`jco transpile` plus a JS fetch handler), which is unverified | out of scope for the artifact this record specifies |

**Which export shape.** The stock artifact exports
**`wasi:http/incoming-handler@0.2`** (P2), not the P3 handler.

- P2 loads on the pinned wasmtime with no experimental flag.
- P2 is the instance model under which the guest never interleaves.
- P2 is the interface name the direction names.

P3 (`wasi:http/handler@0.3.0`) is a later step (§8, step 9), for when wasmtime
serves it without `-W`. Under P3, up to 16 handlers interleave on one linear
memory and one RC heap whenever a handler blocks in a stream builtin (the
#2659 prototype). Pillar 1 makes the *program's* state unshareable, but the
*runtime's* allocator, lazy-`let` slots and scratch buffers are not proven
re-entrant across suspension points. So the P3 shell **serializes handlers
within an instance** (a guest-side queue with backpressure), until a gate
proves re-entrancy at every suspension point. Neither option was measured; the
serializing shell is chosen because it is correct on every host, whereas
`--max-instance-concurrent-reuse-count 1` is a wasmtime flag that no other host
honours.

### 5.4 What `main` means

- **Socket hosts** (native, embedded lane): `main` runs once, as today.
  `http.serve` binds, and serves until a shutdown signal (§5.6). Then it
  **returns** `()`, the statements after it run, and the exit code is main's.
  Main's statements before `serve` run once. They cannot influence the app,
  because of §5.2.
- **Export hosts**: `main` is **not** an entry point. The component exports
  `incoming-handler.handle`, which evaluates the app (per instance, lazily),
  converts the `incoming-request` into an `HttpRequest`, calls the app, and
  writes the response.
- **The export build's shape rule.** A program that `almide build --target wasm`
  exports must have a `main` whose body is exactly one call
  `http.serve[_with_limits](port, APP[, limits])`, optionally preceded by
  `let`s used only in `port` / `limits`. Those arguments are not evaluated on an
  export host: the host owns the socket and its own limits. Any other statement
  in `main` is a check-time error on `--target wasm` (a code allocated by the
  implementing PR). Its fix-it: "this runs on native but not on a
  wasi:http host; move setup into a top-level `let` or into the handler, and
  drop readiness lines (the host prints its own)". The rule is new surface and
  refuses nothing that builds today: `http.serve` is E081 on the stock build
  until then.

We did not choose "main's pre-serve effects run at every instance init and
must be idempotent". Idempotency cannot be checked, the instance count is
host-chosen, and a `kv` write in such a prologue would repeat observably per
request under P2.

### 5.5 Native concurrency (closes #2665)

The model is the cheapest correct one once pillars 1–3 hold:

- **An accept thread and a pool of worker threads.** A worker is started on
  demand, up to `max_in_flight`, and each accepted `TcpStream`, which is
  `Send`, goes to an idle worker. When all are busy, accept stops, and the
  kernel backlog is the backpressure.
- **Each worker builds its own app instance.** The compiler lowers the
  instance-closed served expression (§5.2) to a plain `fn() -> Rc<dyn Fn(…)>`
  **factory**, which is `Send` because it is a fn pointer. A worker calls the
  factory once. The factory reads top-level lets through their existing
  per-thread slots. Main first evaluates the expression once itself, so an
  `err` exits before binding.
- **`kv` is process-global** (§4.4). Its values are `String`s, owned and
  `Send`; no Almide `Rc` crosses a thread.
- **stdout becomes process-global.** Today `ALMIDE_STDOUT_BUF` is a
  `thread_local!` 64 KiB `BufWriter` (`crates/almide-codegen/src/lib.rs:346-359`),
  and only the main thread's copy is flushed at exit. Worker output would be
  lost. It becomes one buffer behind a `Mutex`. Each `print` / `println` call
  appends its bytes under the lock, so **a line is atomic**, and the
  terminal-flush and 64 KiB rules stay. stderr stays unbuffered; each
  `eprintln` is one `write` under Rust's stderr lock.
- **RNG reseeding per worker.** Today the xorshift state is `thread_local!`,
  seeded from `SystemTime` nanos (`runtime/rs/src/random.rs:6-10`), so two
  workers that start in the same tick draw the same stream. Each worker's seed
  mixes OS entropy (`getrandom` where available, else the clock) with the
  worker index and the thread id. Randomness makes no reproducibility promise,
  so only distinctness is specified: no two live workers share a seed.
- **Fork was rejected**, although it needs no `Send`: the in-process `kv` map
  would be per child, the unflushed parent stdout buffer would be duplicated
  into every child, and the RNG would continue the same stream after fork
  (research §4.2).
- **A `Send` value representation (Rc→Arc everywhere) was rejected** as a
  whole-runtime cost (research §5, option C).

The embedded lane stays sequential. Its answers equal native's, and only
timing differs, which the new C-367 does not promise.

### 5.6 Shutdown (closes #2692)

On the first SIGTERM or SIGINT, a socket host:

1. stops accepting;
2. lets the in-flight requests finish, bounded by `request_timeout_ms`;
3. flushes stdout;
4. returns from `http.serve`.

A second signal during the drain skips the wait: the host flushes and exits
with code 1. The embedded lane does the same in the host, where the guest's
serve loop returns. The exit codes stay inside C-350's `0..=125`, so there is
no 128+signal code.

### 5.7 Limits

```almide
type ServeLimits = { max_in_flight: Int, request_timeout_ms: Int, max_body_bytes: Int }
effect fn serve_with_limits(port: Int, f: effect (HttpRequest) -> HttpResponse, limits: ServeLimits) -> Unit
```

The name follows the `*_with_limits` family (`request_stream_with_limits`,
`HttpLimits`). `http.serve(port, f)` uses the defaults: 64 in flight, a 30 s
request timeout, 1 MiB of body.

| Field | Native | Embedded lane | Export hosts |
|---|---|---|---|
| `max_body_bytes` | enforced by the shared server core: a larger body is `413 Payload Too Large`, and the handler is not called | same | same, enforced by the guest shell while it reads the body stream |
| `request_timeout_ms` | enforced: after it, `503` and the connection closes, and the handler's result is dropped | same | the host's own |
| `max_in_flight` | enforced: the pool size | the lane is sequential, so this is 1 | the host's own |

The export host's limits are host configuration (for example `wasmtime serve
--max-concurrent-requests`). This is timing and admission, not an answer to an
admitted request.

## 6. Migration: measured impact

`research/spike/concurrent-var-reach/reach.py` implements §3.1–3.3 (including
the escape and let-snapshot clauses) over `almide --emit-ast`, run with almide
0.64.0 on 2026-09-27.

It was validated first:

- it flags every var-reaching research probe: the four `serve` probes
  (captured and top-level, native and wasm variants) and the five `fan`
  probes (block, captured, read-only captured, top-level, and through `bump`);
- it flags a factory probe (`mk()` returning a closure over its own `var`);
- it passes the `list.map` probe with a fn-local var;
- on `tests/diagnostics/e008-*` it flags exactly the 12 broken sites, and none
  of the fixed ones.

| Corpus (commit) | `.almd` files | Files with a site | Sites | Sites that reach a `var` |
|---|---|---|---|---|
| `spec/` (develop b0d48b31b) | 1,446 | 58 | 282 (`fan` arms 55, `fan.map` 122, `fan.any` 56, `fan.settle` 18, `http.serve` 2, `http.route` 21, `http.mount` 2, `http.wrap` 6) | **0** |
| `stdlib/` | 317 | 6 | 0; stdlib has **no top-level `var`**, so no stdlib fn carries a transitive reach | **0** |
| `research/benchmark/exercises/` | 25 | 0 | 0 | 0 |
| almai (316db10) | 50 | 0 | 0 | 0 |
| comide (fb9d654) | 20 | 0 | 0 | 0 |
| golemide (1f7c7f7) | 26 | 0 | 0 | 0 |
| gramide-cli (5f67679) | 1 | 1 | 8 `fan` arms calling dependency packages; the 7 gramide dependencies contain no top-level `var` (the only `^var ` matches are inside heredoc test strings of Go and JS source) | **0** |
| almide-dojo bank: `tasks/`, `msr/`, `src/` (f58aaa2) | 123 | 1 | 0 | 0 |
| the other almide-org repos (playground 123, porta 36, bindgen 24, docs 16, web 6, lander 4, sqlite 3, js 1) | 213 | 0 | 0 | 0 |
| `tests/diagnostics/` (negative fixtures) | — | 99 | 69 | 12, all in the existing `e008-*` broken fixtures: already errors today |

**The var rule breaks zero written programs.**

**The instance-closure rule (§5.2)** breaks one of the two `http.serve` sites:
`spec/serve_cross/http_serve_replay.almd`, whose `boot` capture *is* the
retired C-367 promise. Every serve example in the docs (`docs/stdlib/http.md` ×7,
`docs/CHEATSHEET.md:351`) is already closed.

**The export shape rule** breaks nothing, because the stock build refuses
`http.serve` today.

Hence §3.5: an error at epoch 6, no warning window.

## 7. Contract changes (pillar 4)

These are proposals. Each lands in almide/als first, then the pin advances,
then the implementation (CLAUDE.md, Behavior Contracts). The IDs are allocated
there.

### 7.1 C-367, rewritten: HTTP-semantic identity across every serving host

> `http.serve(port, app)` answers every admitted request with the same
> **status code, header set and body** on native, on the embedded wasm lane
> (`almide run --target wasm`) and, for a program `almide build --target wasm`
> accepts, on a `wasi:http/incoming-handler` host. The **header set** is the
> response's header fields as (name, value) pairs, where names compare
> ASCII-case-insensitively. Fields with different names are unordered, and
> fields with the same name keep their relative order (RFC 9110 §5.3). The
> fields a host manages are excluded: `date`, `connection`, `keep-alive`,
> `transfer-encoding`, and `content-length` (the framing; the body bytes are
> compared de-framed). The **reason phrase is not part of the contract**:
> HTTP/2 and HTTP/3 carry none (RFC 9113 §8.3.2, RFC 9114 §4.3.2), and hyper
> writes the canonical phrase where the native core writes its table (`418 OK`
> natively, `418 I'm a teapot` under `wasmtime serve`, measured by the #2659
> prototype). The request readers (`req_method`, `req_path` with the query
> included, `req_body`, `req_header` (first match, ASCII case-insensitive),
> `query_params`, and `param`) answer the same values on every host. A handler
> `err(m)` is a `500` with body `Internal error: <m>` and `Content-Type:
> text/plain`. On a socket host a bind failure aborts with `Error: bind failed:
> <os message>` and exit 1. The app is evaluated per instance (per worker
> thread natively, per `wasi:http` instance on an export host). Because it is
> instance-closed and reaches no `var`, the number of instances is
> unobservable. Timing, and the order in which concurrent requests are served,
> are not promised. NOT covered: HTTP framing and connection reuse (the
> host's), and a `kv` store on a stock host (see the store contract).

The fixture keeps its replay script, drops `boot`, and compares under the
header-set rule on all three legs, adding `wasmtime serve` once §8 step 7
lands.

### 7.2 New rows (IDs allocated in als)

- **Serving concurrency and logs.** "Native `http.serve` serves up to
  `max_in_flight` requests concurrently, so a handler that sleeps does not
  delay a request that arrives during it. Each `print` / `println` / `eprintln`
  call's bytes reach their stream contiguously. The order of lines from
  different in-flight requests is unspecified, and lines from one request keep
  program order. No two worker threads share an RNG seed." Evidence: a
  loopback fixture with a 2 s `/slow` and a `/fast` sent 0.3 s later (`/fast`
  answers before `/slow`), and a many-writers line-integrity fixture.
  (ADR-0011 D1 requires `fan` arms' output to equal sequential evaluation. That
  requirement has a program-defined reference order, the arm order.
  Concurrent requests have none, because arrival order is an input chosen by
  the network, so serve's log order is honestly unspecified rather than
  transactional.)
- **The kv store.** "`kv.update(s, k, f)` is linearizable per key on native and
  the embedded lane: N concurrent `kv.incr(s, k, 1)` leave `N`. `get` after a
  completed `set` or `update` on any worker returns that value or a later one.
  A store lives for the process. Each `test` block starts with every store
  empty. On `--target wasm` stock builds `kv` is refused at check time until a
  host passes the conformance fixture." Evidence: a 64-writer increment
  fixture (native and embedded), a per-test isolation fixture, and the E081
  availability row.
- **Shutdown.** "On SIGTERM or SIGINT a socket host stops accepting, completes
  in-flight requests within `request_timeout_ms`, flushes stdout, and returns
  from `http.serve`. A second signal flushes and exits 1." Evidence: a fixture
  that prints, receives a request, is sent SIGTERM with stdout redirected, and
  finds every line (#2692's repro).
- **Limits.** "A body over `max_body_bytes` is `413` without calling the
  handler; a request over `request_timeout_ms` is `503`." Evidence: two
  fixtures, native and embedded.
- **The export artifact.** "`almide build --target wasm` of a serve program
  whose main is a single serve call emits a component exporting
  `wasi:http/incoming-handler@0.2`; `main` is not run; the app is evaluated per
  instance." Evidence: the C-367 replay run under the pinned `wasmtime serve`.

## 8. Implementation plan

Each step is one PR with its gate, in this order. Steps 1 and 2 must land
before step 4 (§3.5). Step 0 items are prerequisites already found.

Issues, one per step: 0a #2664, 0b #2696, 1 #2697, 2 #2698, 3 #2699,
4 #2665, 5 #2692, 6 #2700, 7 #2659, 8 #2701, 9 #2702. Tracking issue: #2705.

| # | Step | Gate | Closes / relates |
|---|---|---|---|
| 0a | Serve effect fn values through a declared fn type on the structural wasm leg | the router rows of `proofs/target-availability.toml` lose their structural wall | #2664 (in flight: PR #2693) |
| 0b | Make `http.router` plus a handler call run on wasm: today it is E082 on **both** legs even without `serve` (incumbent: "match over an UNTRACKED subject …"; structural: `ty-mismatch:Fn`, and `call:__parts` in the #2659 prototype, `stdlib/http.almd:410`). Fix it on the **structural** leg only | `h(http.new_request(...))!` (with `let h = app!`) runs byte-identically on native and `--target wasm` in `spec/stdlib/http_router_test.almd` | #2696; needed by 7 and by "an app is tested by calling it" on wasm |
| 1 | The generalized E008 (§3): the concurrent-slot attribute, slot inference, the executable-closure pass, the escape and snapshot clauses, witness paths, interface summaries; dialect epoch 6 with `CURRENT_DIALECT = 6` | a `tests/diagnostics/` broken/fixed pair per §3.3 shape (captured, top-level, through a fn, through a local `let`, through a top-level `let`, through a record/list, factory escape, inferred wrapper slot, `fan.map`, `fan.settle`), a fixed pair for the folded `list.map` case, `scripts/check-dialect-epochs.sh`, and reach.py over `spec/` staying at 0 | closes the `fan.map` silent-sequential hole; unblocks #2594 (effect callbacks may then run in parallel, a separate decision) |
| 2 | The instance-closure rule (§5.2) with its code; migrate `spec/serve_cross/http_serve_replay.almd` off `boot`. **Before it:** the als PR for the new C-367 wording (§7.1) and a pin advance | a diagnostics pair; the replay fixture green on both legs under the header-set comparison | prerequisite of 4 and 7 |
| 3 | `kv` module (§4.2), native backing, the per-test fresh store, the interp bridge (3-way oracle), embedded-lane host ops, and E081 rows for stock legs. **Before it:** the als store contract | the 64-writer fixture on native and embedded, the per-test isolation fixture, the availability ratchet | pillar 2 |
| 4 | Native concurrent serve (§5.5): the accept thread and worker pool, the serve-argument factory lowering, the global stdout buffer, per-worker RNG seeds, `serve_with_limits` and `ServeLimits` (§5.7). **Before it:** the als concurrency and limits contracts | the loopback slow/fast fixture (done-when of #2665), the line-integrity fixture, the 413 and 503 fixtures, the perf ratio unchanged for non-serve benches | **closes #2665** |
| 5 | Shutdown (§5.6) on native and the embedded lane. **Before it:** the als shutdown contract | #2692's repro as a fixture on both lanes | **closes #2692** |
| 6 | An HTTP-semantic comparison in `tests/http_serve_cross_test.rs` (the header-set rule, de-framing) | the replay compares native ⇄ embedded under the new C-367 | prerequisite of 7 |
| 7 | `almide build --target wasm` exports `wasi:http/incoming-handler@0.2` for serve programs (§5.3–5.4): the export shape rule and its code; the component shell (request/response conversion, `max_body_bytes`, app init per instance). Built on the **structural** leg and the component path (`crates/almide-wasm-run/src/wasi_p2.rs`), never the incumbent. **Before it:** the als export contract | the C-367 replay under the pinned `wasmtime serve` (P2, no `-W`), a fixture for the shape-rule diagnostics pair, the `http.serve` stock row flipped from E081 | **closes #2659** (replaces its p3-sockets option) |
| 8 | Document the surface: CHEATSHEET / llms.txt / `docs/stdlib/http.md` / `docs/diagnostics/E008.md`, with `almide check` fences for §4.3's program | `scripts/check-llm-surface.sh` | only after 1–7; this record does not touch that surface |
| 9 | (later) the P3 `wasi:http/handler@0.3.0` export, with guest-side per-instance serialization (§5.3), once wasmtime serves it without `-W`; a stock `kv` binding over wasi:keyvalue draft2 on the first host that passes the conformance fixture | the replay under P3; the conformance fixture | follow-ups of #2659 |

**Relationship to #1696 (retiring the incumbent wasm emitter).** Nothing here
adds incumbent code:

- steps 0b and 7 are specified on the structural leg and the component path;
- the embedded-lane `kv` host ops (step 3) are host-side, in `almide-wasm-run`,
  plus structural-leg lowering;
- the incumbent's router service today is a stopgap that step 0b makes
  unnecessary, which removes a reason to keep the incumbent.

Every new row in `proofs/target-availability.toml` names the structural leg.

**Side findings, filed as bugs #2703 and #2704** (verified on 0.64.0, 2026-09-27):

- **The clock on the embedded lane.** A clock read (`datetime.now()` or
  `env.millis()`) in **any** program that calls `http.serve` is E082 on
  `almide run --target wasm`. This is not limited to the handler: the same
  read in `main` before `serve` is refused too. The structural leg walls
  `call:datetime.now` / `call:env.millis`, and the incumbent walls
  `http.serve`. Without `serve`, the same clock read runs on the lane. This
  refines the research's side observation, which located it in the handler.
- **`!` on an intrinsic effect fn inside an effect-slot lambda.**
  `random.int(0, 999)!` or `env.millis()!` inside a lambda checked against an
  `effect (…) -> …` slot (the `http.serve` handler, or
  `let f: effect (Int) -> Int = …`) is **E022**. `fs.read_text(p)!` (which
  returns a `Result`) and a user `effect fn … -> Int` are accepted in the same
  position, and `random.int(0, 9)!` is accepted in an effect fn body. This
  contradicts `stdlib/http.almd:19-25` ("effect-fn body ergonomics (`!` with
  propagation …)") and CHEATSHEET:342-354.

## 9. Refinements the evidence forced

1. **"Stock wasm uses a host KV" became "a conforming host KV, and none
   conforms today".**
   - `wasmtime serve` 47 clones the bucket on every `open` and builds a fresh
     context per instance (§4.1).
   - Spin's CAS upserts unconditionally on first insert.
   - Workers KV is eventually consistent, with no atomics.
   - `kv` is therefore E081 on stock builds until a host passes the
     conformance fixture.
2. **The served app must be instance-closed.** A handler cannot capture main's
   locals, not only its vars. The export host never runs `main`, and native
   workers build their own instance. This retires C-367's "a value main
   computed before `serve` is the same on every request", which is its own
   fixture's `boot`.
3. **`main` is not an entry point on export hosts.** The export build accepts
   a `main` that is a single serve call. The idempotent-prologue alternative
   was rejected, because idempotency cannot be checked and a `kv` write would
   repeat per request under P2.
4. **The stock export is P2 `incoming-handler@0.2`, not P3.** On the pinned
   wasmtime, P3 needs `-W component-model-more-async-builtins`, and it
   interleaves up to 16 handlers on one heap. The P3 shell will serialize
   per instance when it comes.
5. **The rule needs an escape clause and a snapshot clause.**
   - A factory fn returning a closure over its own `var` shares that var.
     E011 forces the factory to be an `effect fn`, but does not stop it.
   - A `let` copy of a var's value is not a reach. Without that clause, the
     analyzer flagged E008's own `fixed.almd`.
6. **The native model is a worker pool of per-thread app instances, not
   fork.**
   - The existing per-thread lazy slot for top-level `Rc` lets makes the
     instances nearly free.
   - Fork would split `kv` and duplicate unflushed stdout.
   - The thread-local stdout buffer must become global, or worker output is
     lost.
7. **Names.** The module is `kv` and its type is `KvStore` (§4.2). The limits
   entry point is `serve_with_limits`, following the existing
   `*_with_limits` family.
8. **The export artifact has two new prerequisites:** router calls are walled
   on both wasm legs today (0b), alongside #2664.

## 10. Alternatives

- **Sequential serving with a warning when the handler reaches a `var`**
  (research option E), the provisional plan.
  - No measured or sourced system switches its serving mode on handler
    contents (§1.2).
  - The one Almide precedent, `fan.map`, is safe only because its twins give
    identical results; for `serve` the switch flips latency, which is what
    #2665 is about: 1.70 s against 0.00 s (M1 against M6/M16/M19).
  - The trigger is a single line an LLM adds while "just adding a request
    counter", and the only signal is a warning.
- **Isolation per request** (fork or deep copy, research option B).
  - The counter silently answers 1 forever (M10, M14), and the embedded lane
    cannot match it (2..6 against 1).
  - After fork, children continue the parent's RNG stream (§5.5).
  - Fork splits an in-process `kv`.
- **A shared heap under a lock** (research option C, or a global lock).
  - Unlocked, races are silent: Go only reports them at run time under
    `-race` (M6/M7).
  - A global lock is sequential serving again (the 1.70 s of M1/M5/M9/M12/M17).
  - Making every value `Arc` is a whole-runtime cost.
  - The embedded lane cannot produce `after=6`, and a P2 or Workers host
    cannot share memory at all.
- **p3 `wasi:sockets` for the stock artifact** (#2659's option 1).
  - It works: the #2659 prototype, commit e63493268, passes a three-leg
    byte-identical replay locally.
  - It is rejected anyway. `wasmtime run -S p3 -S inherit-network` is a CLI
    runner, not a serving platform, and the serving platforms (`wasmtime
    serve`, Spin, Workers) speak incoming-handler.
  - It moves request parsing and response writing into the guest, so byte
    identity becomes transcription.
  - The one thing it preserved, native's single instance, is unobservable
    once pillars 1–2 hold.
- **Opt-in concurrency** (`http.serve` stays sequential, and a separate
  `serve_concurrent` form gets the rule; research option G). The default,
  which is what a model writes first, would still block (#2665).
- **An explicit state fold** (`http.serve_state(port, init, (s, req) => (s2,
  resp))`, research option H).
  - A fold is sequential by construction, so the fast request waits again.
  - Its state is in the instance's memory, so it resets per request on P2 and
    splits on P3 and Workers.
  - A per-key store gives the same explicitness without either problem.
- **Mapping `kv.incr` to host `increment`.** The encodings differ (decimal
  text in wasmtime, 8-byte LE in Spin), and mixing with `get` is
  implementation-dependent (wasi-keyvalue PR #59).

## 11. Consequences

What we gain:

- One program means one set of answers on every host Almide serves from. No
  edit can silently flip a server's serving mode or its counter semantics.
- `/fast` behind a 2 s `/slow` answers at once natively (#2665).
- The stock artifact becomes an ordinary `wasi:http` component (#2659).
- `fan.map`'s silent-sequential hole closes, which clears #2594's semantic
  obstacle.
- Apps are tested by calling them, on both lanes once 0b lands.

What we pay:

- A new dialect epoch.
- A new stdlib module.
- A shared runtime stdout buffer, which takes a lock per print.
- Per-worker evaluation of top-level `Rc` lets: memory and startup per worker.
- On export hosts, the loss of main-computed per-process values.
- Stateful apps cannot export to stock hosts until one conforms (§4.4).

## 12. Falsifier

1. If Dojo measures model-written handler or `fan` tasks hitting E008 and then
   producing a *wrong* program more often than a correct `kv` or `let` rewrite
   across two releases, the diagnostic and fix-it are wrong, and are revisited.
   The rule itself stays unless a conforming alternative is found.
2. If a written program appears whose correct behaviour needs a `var`
   reachable from a concurrent body and cannot be expressed with `kv` (for
   example, a per-connection streaming state machine), the rule's reach is
   revisited, not its existence.
3. If per-worker app instances make native serving slower than today's
   sequential loop for the spec serve fixtures at one client, the
   instance-per-worker model is revisited (for example, a shared immutable
   app through a `Send` representation of pure values).
4. If wasi:keyvalue stabilizes without a CAS, or with weaker-than-per-key
   guarantees (the direction of open PR #56), the stock `kv` binding is
   redesigned against whatever atomic primitive the standard offers. The
   Almide surface is kept.

## 13. References

- Research note: `../almide-references/RESEARCH-http-serve-concurrency.md`
  (M1–M23, S1–S18, §4–§6), probes and raw outputs under
  `../almide-references/research-2026/http-serve-concurrency/`.
- wasi-keyvalue: https://github.com/WebAssembly/wasi-keyvalue (README.md:7,
  55-57, 82-86; wit/store.wit, wit/atomic.wit, wit/batch.wit at `aa972c86e`;
  tag `v0.2.0-draft`; PRs #56, #59, #60).
- wasmtime 47: `src/commands/serve.rs` (lines 44-68, 141-155, 380-460, 535-545)
  and `crates/wasi-keyvalue/src/lib.rs` (lines 100-171, 226-246) at tag
  v47.0.3, https://github.com/bytecodealliance/wasmtime/tree/v47.0.3;
  `wasmtime serve --help` / `-S help` (47.0.2); the `wasi:http@0.2.12` WIT in
  `wasmtime-wasi-http-47.0.3/wit/deps/http.wit` (`interface incoming-handler`,
  line 636).
- Spin: https://github.com/spinframework/spin (`wit/deps/keyvalue-2024-10-17/`,
  `crates/key-value-spin/src/store.rs:326-457`,
  `crates/factor-key-value/src/host.rs:706-745`); https://spinframework.dev/v4/dynamic-configuration;
  https://spinframework.dev/blog/announcing-spin-3-6.
- Cloudflare: https://developers.cloudflare.com/kv/concepts/how-kv-works/,
  https://developers.cloudflare.com/kv/api/write-key-value-pairs/,
  https://developers.cloudflare.com/durable-objects/api/sqlite-storage-api/,
  https://developers.cloudflare.com/durable-objects/concepts/what-are-durable-objects/,
  https://developers.cloudflare.com/durable-objects/api/namespace/,
  https://developers.cloudflare.com/workers/reference/how-workers-works/,
  https://developers.cloudflare.com/workers/runtime-apis/webassembly/.
- Deno KV: https://docs.deno.com/deploy/kv/transactions/.
- RFC 9110 §5.3 (field order), https://www.rfc-editor.org/rfc/rfc9110#section-5.3;
  RFC 9113 §8.3.2, https://www.rfc-editor.org/rfc/rfc9113#section-8.3.2;
  RFC 9114 §4.3.2, https://www.rfc-editor.org/rfc/rfc9114#section-4.3.2.
- In-tree: `runtime/rs/src/http.rs:723-743`, `runtime/rs/src/fan.rs:3-40`,
  `runtime/rs/src/random.rs:6-10`, `crates/almide-codegen/src/lib.rs:346-359`,
  `crates/almide-frontend/src/check/infer_calls_closures.rs:215-234`,
  `stdlib/http.almd:19-30, 355-626`, `proofs/dialect-epochs.toml`,
  `docs/contracts/contracts.toml` (C-367, C-368), C-350.
- Numbering: 0018 is held by an in-flight branch (the value-strings record),
  so this record takes 0020.
