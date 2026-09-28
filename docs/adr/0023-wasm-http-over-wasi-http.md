# ADR-0023: Wasm HTTP is `wasi:http@0.3` — a program that uses HTTP builds as a p3 component, the embedded host serves the same imports, and no host serves a custom HTTP op

- **Status**: Accepted. The maintainer set the target on 2026-09-28: WASI 0.3
  (`wasi:http@0.3`), no p2 stage. This record tests the proposed direction
  against measurement and prior art, and gives the implementation order. None
  of it is implemented yet. Where the evidence changed the proposal, the text
  says **[refined]**; the list is in §9.
- **Date**: 2026-09-28
- **Scope**: every `http` client function on `--target wasm`, `http.serve` on
  `--target wasm`, the embedded host (`almide run --target wasm`,
  `crates/almide-wasm-run`), the p3 shim (`crates/almide-wasm-run/src/wasi_p3*.rs`),
  the default shape of `almide build --target wasm`, and the contract rows
  C-328, C-366 and C-367.
- **Supersedes**: [ADR-0020](./0020-http-serving-and-handler-state.md) §5.3
  "Which export shape" (the P2 `wasi:http/incoming-handler@0.2` choice) and
  the ordering of its steps 7 and 9. Everything else in ADR-0020 stands (§3.3
  below says why its pillars are what make the P3 instance model safe).
- **Related**: #1710 (the wasi:http@0.3 client leg), #1628 (component model
  stages), #2659 (a stock artifact for a server), #2702 (ADR-0020 step 9),
  #2633 (the call handle), #2819–#2826 (the HTTP audit), #865 (the wasm VM),
  #1696 (retire the incumbent emitter), `proofs/wasi-pin-policy.toml`,
  `proofs/target-availability.toml`.
- **Evidence base**: the measurements in §2, run on 2026-09-28 with almide
  0.64.0 (5555cb251), the `wasmtime` CLI 47.0.2 and the crates
  wasmtime 47.0.4 / wasmtime-wasi 47.0.4 / wasmtime-wasi-http 47.0.3 /
  wasmparser 0.259.0 read from source; two prototypes (a `wasi:http/handler@0.3.0`
  component built with wasm-encoder + wit-component, adapted from the #2659
  probe on branch `stock-wasm-serve-2659`; and an embedding host, §2.4) kept
  outside the tree; `../almide-references/RESEARCH-component-model.md` and
  `RESEARCH-http-serve-concurrency.md`; the primary sources in §2.6.

## 1. Context

### 1.1 Three lanes, three HTTP mechanisms

The compiler emits one core module whose host surface is the `almide.*`
imports (`fs_call` with an op number, `host_read`, `print`, `exit`, …). What
happens next depends on the lane:

| Lane | What runs | How HTTP is served |
|---|---|---|
| native | a Rust binary | `crates/almide-rt-core` (the client and server cores), spliced into the runtime |
| embedded (`almide run --target wasm`) | the core module on wasmtime, `almide.*` served by `crates/almide-wasm-run/src/host.rs` | **the host** serves `fs_call` ops 43..=50 (client), 53..=59 (the call handle) and 70..=72 (serve) by calling the same `almide-rt-core` functions |
| stock p1 (`almide build --target wasm`, the default) | `to_wasi` rewrites `almide.*` into a WASI preview-1 shim | none: every HTTP function is E081 at build time |
| p3 (`ALMIDE_COMPONENT_P3=1 … --component`) | `to_p3` rewrites `almide.*` into a **guest-side** shim over WASI 0.3 and wraps it with wit-component | the shim serves ops 43..=50 over `wasi:http/client@0.3.0` |

Two different things are therefore called "custom ops":

- the **op numbers** at the boundary between the emitted code and the shim.
  Inside a stock artifact they never reach a host: the shim is guest code,
  exactly as the preview-1 adapter is guest code in a Rust p2 component;
- the **host-side service** of those numbers by the embedded host. This is
  what makes the embedded lane a different ABI from every stock runtime.

### 1.2 What is missing

From `proofs/target-availability.toml` (develop f4d5c063b):

- `get_response` / `post_response` / `put_response` / `patch_response` /
  `delete_response` / `request_response` (6) and `request_stream` wall on the
  structural, stock-p1 **and** embedded legs;
- `anthropic_streaming_call`, `openai_streaming_call` and their
  `_with_limits` forms (4) wall on every leg; they are native intrinsics
  (`@intrinsic("almide_rt_sse_*")`, `stdlib/http.almd:252`);
- the call handle (`start` / `poll` / `read_new` / `wait` / `cancel`,
  `request_stream_with_limits`) and `http.serve` are served by the embedded
  host only;
- the p3 shim serves the five verbs and the framed family (ops 43..=50) and
  nothing else of `http`; it also refuses `env.get` (host op 26), measured in
  §2.3.

### 1.3 The question

What is the ideal way for a wasm Almide program to be an HTTP client and an
HTTP server, so that native and wasm answer the same? The proposed direction
was: (1) no more custom host ops, the missing functions in the guest over
`wasi:http`; (2) the embedded host implements `wasi:http` instead of custom
ops, so one artifact runs on the embedded lane and on stock runtimes;
(3) programs that use HTTP build as components by default; (4) servers
export the handler; (5) retire the custom ops after migration. The owner
fixed the version to WASI 0.3.

## 2. Measured evidence

All commands ran in a scratch directory with a local echo server
(`python3 echo.py 18777`, a `ThreadingHTTPServer` that answers
`"<METHOD> <path> len=<body length>"` with a content-length).

### 2.1 What wasmtime 47 needs for p3, and why

`wasmtime 47.0.2`, `wasmtime run -S help` / `-W help`, then runs:

| Artifact | Command | Result |
|---|---|---|
| `hello_p3.wasm` (Almide, `println` only, p3) | `wasmtime run hello_p3.wasm` | refused: `synchronous 'stream.write' requires the component model more async builtins feature` |
| same | `wasmtime run -W component-model-more-async-builtins=y hello_p3.wasm` | `hello` (no `-S p3` needed: p3 is on by default since wasmtime 46) |
| `get_p3.wasm` (Almide, `http.get`, p3) | `wasmtime run -W component-model-more-async-builtins=y get_p3.wasm` | refused: `component imports instance 'wasi:http/types@0.3.0', but a matching implementation was not found` |
| same | `… -S http get_p3.wasm` | `GET /hello len=0` |
| `h_sync.wasm` (the #2659 handler probe: sync `stream.write` / `future.write`) | `wasmtime serve -S p3 -S http h_sync.wasm` | refused: `synchronous 'stream.write' requires …` |
| `h_async.wasm` (the same probe, the two writes changed to `[async-lower]` plus a `waitable-set.wait` when the write answers BLOCKED) | `wasmtime serve -S p3 -S http h_async.wasm` | `HTTP/1.1 418 I'm a teapot`, `x-count: 1`, body `hello from p3 #1`, then `#2` on the next request |
| same | `wasmtime serve h_async.wasm` (**no flags at all**) | the same answers |

The reason is in wasmparser 0.259.0 `src/validator/component.rs`: the only
canonical builtins gated on `cm_more_async_builtins` (🚝) are the
**synchronous** `stream.read` / `stream.write` / `future.read` /
`future.write` and the **async** `stream.cancel-*` / `future.cancel-*`.
`waitable-set.new/wait/poll/drop`, `waitable.join`, `subtask.cancel/drop`,
`backpressure.inc/dec`, `task.return` and the async stream/future ops need
only `cm_async`, and wasmtime 47 turns `cm_async` on by default
(`wasmtime-47.0.4/src/config.rs:2526-2531`).

**Finding 1.** On the pinned wasmtime, a p3 component needs `-W` only because
Almide's shim uses the 🚝 synchronous builtins. A shim built from the
baseline async builtins runs as `wasmtime serve app.wasm` and
`wasmtime run -S http app.wasm`, where `-S http` is the capability grant,
like `--dir`. **No wasmtime bump is needed.** The same shape is what the
flag-less hosts will accept: 🚝 is not part of the WASI 0.3 baseline
(RESEARCH-component-model.md, "Built-ins under the baseline gate").

ADR-0020 §5.3 chose P2 because "on the pinned wasmtime, P3 needs `-W
component-model-more-async-builtins`". That premise came from the #2659 probe's
synchronous writes, not from P3.

### 2.2 The embedded host can serve the same imports (prototype)

`wasmtime-wasi-http 47.0.3` has a `p3` feature (`src/p3/mod.rs`, marked
"experimental, unstable"; p3-only bug fixes get no patch releases). Its
outgoing transport is a trait method: `WasiHttpHooks::send_request(request,
options, fut)`. Without the `default-send-request` feature the embedder must
supply it, and no rustls / webpki-roots / tokio-rustls is pulled in by the
crate.

The prototype host (≈80 lines) builds an `Engine` with
`wasm_component_model_async(true)`, links `wasmtime_wasi::p3` and
`wasmtime_wasi_http::p3`, and implements `send_request` by collecting the
request body and calling `almide_rt_core::http_client_core::request_response`
on a blocking thread, the same function native and the embedded host call
today. It compiled against the pinned crates on the first try
(`cargo build --release -j2 --offline`: 3 min 41 s cold) and runs the
Almide-built p3 components unchanged:

```
$ MORE=1 embedhost post_p3.wasm        # http.post, then http.get to a refused port
POST /p len=6
err: http request failed (p3 transport)
```

| The same `post.almd` on | stdout |
|---|---|
| native (`almide run`) | `POST /p len=6` / `err: connection failed: Connection refused (os error 61)` |
| embedded, host ops (`almide run --target wasm`) | identical to native |
| `wasmtime run … -S http post_p3.wasm` | `POST /p len=6` / `err: http request failed (p3 transport)` |
| prototype host (wasi-http p3 + rt-core hook) | identical to the wasmtime CLI line |

**Finding 2.** Embedding is feasible at the pinned version, and one artifact
runs on the embedded host and on the stock CLI with the same answers. The
successful exchange already agrees on all four lanes. The error text does
not, for a reason that is in the **guest**, not the host: the prototype hook
returned `internal-error(some("connection failed: Connection refused (os
error 61)"))`, and the shim collapses every `error-code` to one static text
(C-328 already records that the p3 wording is not promised). §4.2 decides
how the text becomes the same.

Dependency cost of the embedding: the prototype's dependency tree has 213
packages against 166 for `almide-wasm-run` today; the 64 added names are
`wasmtime-wasi`, `wasmtime-wasi-http`, `wasmtime-wasi-io`, `wiggle`, `tokio`,
`hyper`, `h2`, `http`, `http-body(-util)`, `cap-std` and their transitive
crates. The prototype binary is 25 MB; the installed `almide` binary is 50 MB.

### 2.3 The p3 lane today refuses what an HTTP program usually needs

```
$ ALMIDE_COMPONENT_P3=1 almide build envget.almd --target wasm --component
error[E081]: env.get (host op 26) is unavailable in the direct WASI 0.3 component.
$ ALMIDE_COMPONENT_P3=1 almide build resp.almd --target wasm --component
error[E081]: `http.get_response` is not available on --target wasm
```

`component_availability::check` serves, on p3, ops 1..=9, 13..=16, 30..=32,
34..=35, 40..=50, 60 and 73. An LLM client reads its key with `env.get`; it
cannot build on p3 today.

### 2.4 Cost: size

`almide build` of the same programs (structural leg, no wasm-opt):

| Program | p1 core module | p2 component (`--component`) | p3 component |
|---|---|---|---|
| `println("hello")` | 1,322 B | 6,107 B | 18,421 B (core module 7,521 B) |
| `http.get` + `println` | E081 | E081 | 30,278 B (core module 12,735 B) |

Most of a p3 component's size is component type information (the
`wasi:http/types` interface), not code. The difference is kilobytes, and only
HTTP programs pay it (§3.3).

### 2.5 Cost: time

`bench.py`: a program doing N sequential `http.get` to the loopback echo,
median of 9 runs after 2 warm-ups, N = 1 and N = 500. The slope is the
per-request cost.

| Lane | N=1 (ms) | N=500 (ms) | per request (µs) |
|---|---|---|---|
| native binary | 2.4 | 52.3 | 100 |
| embedded, host ops (`almide run --target wasm`, compile included) | 102.2 | 153.2 | 102 |
| `wasmtime run -W … -S p3 -S http` (hyper transport) | 4.8 | 66.3 | 123 |
| prototype host (wasi-http p3 + rt-core transport, compile uncached: 8 ms) | 11.8 | 77.5 | 132 |

`drive.py`: 500 sequential requests, one connection each, against a
`http.response(200, …)` server:

| Server | median | p90 |
|---|---|---|
| native `http.serve` | 88 µs | 135 µs |
| embedded `http.serve` (host ops 70..=72) | 69 µs | 83 µs |
| `wasmtime serve` p3 handler (instance reuse, the default) | 93 µs | 113 µs |
| `wasmtime serve --max-instance-reuse-count 1` (a fresh instance per request) | 107 µs | 129 µs |

**Finding 3.** The component route costs 20–30 µs per loopback request over
the host ops and about 10 ms of one-time compile in an uncached embedding.
Both are small against any real network round trip; neither is a reason to
keep a second ABI.

### 2.6 How others do it (2026)

Nobody who targets stock runtimes ships a private host ABI for HTTP. Every
toolchain lowers to `wasi:http`, and the host owns the transport:

- **Rust**: `wasm32-wasip2` (Tier 2) emits components; the `wasip3` crate and
  wit-bindgen's async mode target `wasi:http@0.3`; `wasm32-wasip3` is Tier 3.
  `wstd` / `waki` / `wasi-fetch` are thin client/handler libraries over
  those bindings.
- **Go**: componentize-go maps goroutines onto the callback ABI
  (golang/go#77141 is the proposal); TinyGo `wasip2` uses `wasi:http`.
- **Python** (componentize-py, asyncio) and **JS** (jco / ComponentizeJS,
  StarlingMonkey's `fetch` over `wasi:http`): shipped; jco supports all of
  WASI 0.3.
- **Hosts**: wasmtime 46+ has WASI 0.3 and component-model async on by
  default; Spin 3.6 reuses WASIp3 instances "both concurrently and serially";
  wasmCloud 2.5 runs on wasmtime 46.
- MoonBit's p3 async is "planned"; Grain has none.

WASI 0.3.0 was ratified on 2026-06-11 and 0.3.1 on 2026-08-11
(`proofs/wasi-pin-policy.toml` [sources]). Sources:
[WASI 0.3 launch](https://bytecodealliance.org/articles/WASI-0.3),
[wasi.dev WASI 0.3](https://wasi.dev/releases/wasi-p3),
[Spin 3.6](https://spinframework.dev/blog/announcing-spin-3-6),
[wasmCloud on WASI P3](https://wasmcloud.com/blog/wasi-p3-on-wasmcloud/),
[wasip3 crate](https://crates.io/crates/wasip3),
[rustc wasm32-wasip3](https://doc.rust-lang.org/rustc/platform-support/wasm32-wasip3.html),
and `../almide-references/RESEARCH-component-model.md` §"Reference-compiler
wasm backends".

### 2.7 What a stock host does with the audit's issues (source reading)

`wasmtime-wasi-http 47.0.3`, the default transport (`src/p3/request.rs`):

- it connects directly (`TcpStream::connect(&authority)`); there is no proxy
  support (#2819);
- it trusts `webpki_roots::TLS_SERVER_ROOTS` only (#2820);
- connect, first-byte and between-bytes timeouts come from the guest's
  `request-options`, and each defaults to 600 s (#2825);
- `fields.append` validates with `http::HeaderValue::from_bytes`, so a CR or LF
  in a value is `header-error.invalid-syntax` (#2822);
- nine header names are forbidden (`DEFAULT_FORBIDDEN_HEADERS`, `src/lib.rs:142`):
  `connection`, `keep-alive`, `proxy-authenticate`, `proxy-authorization`,
  `proxy-connection`, `transfer-encoding`, `upgrade`, `host`,
  `http2-settings`. Native accepts all of them today;
- hyper parses the response, so 1xx interim responses and chunk extensions
  (#2824) are handled;
- header names reach the guest in lowercase (the #2659 probe: `x-Count` →
  `x-count`).

## 3. Decision

### 3.1 The five points

| # | Proposed | Verdict |
|---|---|---|
| 1 | No more custom host ops; the missing client functions in the guest over `wasi:http` | **Confirmed, refined.** No new op is served by a host. An op number is allowed only as a guest-internal call between the emitted code and the p3 shim, which the shim serves over `wasi:http/client@0.3.0` (the adapter pattern, §1.1). The 6 `*_response`, `request_stream` and the call handle go into the shim; the 4 streaming LLM functions are **self-hosted in Almide over the call handle**, so they need no shim or host code at all. |
| 2 | The embedded host implements `wasi:http` (wasmtime-wasi-http) instead of custom ops | **Confirmed, refined** (§2.2). The host links `wasmtime_wasi::p3` and `wasmtime_wasi_http::p3` at the pinned 47, without `default-send-request`, and its `send_request` hook is `almide-rt-core`. The transport stays the shared native code, which is how C-328 keeps holding by construction. The embedded **server** loop keeps `http_server_core` for accept, parse and limits, and calls the guest's `wasi:http/handler.handle` export (§3.4). |
| 3 | Programs that use HTTP build as components by default | **Confirmed for p3, with two preconditions.** A program that calls any `http` function builds as a WASI 0.3 component by default. First, the shim uses only the baseline async builtins (Finding 1), so the artifact runs with no `-W`. Second, the shim serves the ops HTTP programs use: env and args (`wasi:cli/environment@0.3.0`), sleep, and the rest in §7 step 3. **[refined]** A program that does **not** use HTTP stays a preview-1 core module: 1.3 KB instead of 18 KB, it still runs on wasmtime LTS 36 (no default p3) and on the #865 VM. The default is decided per program by its op set, the way `wants_http` already routes `to_p3`. |
| 4 | Export the handler for servers | **Confirmed, re-targeted to `wasi:http/handler@0.3.0`.** This supersedes ADR-0020 §5.3's `incoming-handler@0.2` (§3.3). |
| 5 | Retire the custom ops after migration | **Confirmed for HTTP.** Host ops 43..=50, 53..=59 and 70..=72 leave `almide-wasm-run` once the embedded lane runs HTTP programs as the p3 component. The non-HTTP `almide.*` surface of the embedded host (fs, print, alloc counters, capped memory) is out of scope. The embedded lane still runs core modules for programs without HTTP, and the alloc and size ledgers measure those. |

### 3.2 One artifact, two runners

For a program that uses HTTP:

```
almide build app.almd --target wasm -o app.wasm   # a WASI 0.3 component
wasmtime run -S http app.wasm                     # a client, on any wasmtime ≥ 46
wasmtime serve app.wasm                           # a server
almide run app.almd --target wasm                 # the SAME bytes, on the embedded host
```

The embedded lane stops being a separate ABI. It becomes a runner of the
stock artifact whose transport, TLS and error classification are native's.

### 3.3 Why P3 for the server, superseding ADR-0020 §5.3

ADR-0020 §5.3 gave three reasons for P2:

1. "P2 loads on the pinned wasmtime with no experimental flag." Refuted by
   Finding 1: an async-builtin P3 handler loads with **no flag at all**,
   while the synchronous one fails with or without `-S p3`.
2. "P2 is the instance model under which the guest never interleaves." True,
   but ADR-0020's own pillars already make the program's state unobservable
   across instances: no reachable `var` (pillar 1), cross-request state only
   in `kv` (pillar 2), and an instance-closed app (§5.2). What remains is the
   runtime's own re-entrancy (allocator, lazy-`let` slots, scratch buffers).
   The P3 shell serializes handlers within an instance with
   `backpressure.inc` on entry and `backpressure.dec` after the response body
   is done. That builtin is in the WASI 0.3 baseline (wasmparser gates it on
   `cm_async` only), and a conforming host must honour it. This is the spec's
   mechanism, where ADR-0020 planned a guest-side queue. If the step-8
   measurement shows a host that ignores backpressure, the guest queue is the
   fallback, as ADR-0020 wrote it.
3. "P2 is the interface name the direction names." The direction is now
   WASI 0.3 (owner ruling, 2026-09-28).

There are also reasons for P3 beyond the ruling. Spin 3.6 and `wasmtime
serve` reuse P3 instances, so an app is built once per instance instead of
once per request. A P2 artifact would be a second component world next to
the p3 client (P2's `wasi:http@0.2` client and P3's are different
interfaces), which is a second shim to keep in sync.

### 3.4 The embedded server loop

The embedded host does **not** use `wasmtime serve`'s hyper server. It keeps
`http_server_core` from `almide-rt-core` for accept, request parsing,
`max_body_bytes`, `request_timeout_ms` and shutdown (ADR-0020 §5.5–5.7). For
each request it builds a `wasi:http` request resource and calls the
component's `handle` export. Native and embedded therefore keep sharing the
server code, which is what C-367 stands on. `wasmtime serve` is a stock host
under C-367's HTTP-semantic comparison (status, header set, body), where
hyper's reason phrase and `date:` are host decoration.

## 4. Semantics: who owns what

### 4.1 The rule

- **The guest owns what the program can observe as a value**: URL
  decomposition, header validity, body limits, deadlines, cancellation, error
  **text**. These are written once, in the stdlib or in the shim, and run on
  every lane.
- **The host owns the transport and its trust**: DNS, sockets, proxies, TLS
  and trust roots, connection reuse. On stock hosts this is host
  configuration. On native and embedded it is `almide-rt-core`, fixed once
  for both lanes.

### 4.2 Error text

Each transport failure is classified into a `wasi:http` `error-code` case
(`DNS-error`, `connection-refused`, `connection-timeout`,
`TLS-certificate-error`, `HTTP-response-body-size`, …). One renderer turns a
case into the `err` string. The renderer is called by native (rt-core) and by
the shim, which receives the case from any host. The same failure then reads
the same on native, embedded and every stock host. Only `internal-error(some(msg))`
carries free text, and its text is guaranteed equal only between native and
embedded. This changes native's wording (today
`connection failed: Connection refused (os error 61)` includes an OS-specific
errno), so C-328 is amended in als first (§7 step 0).

### 4.3 Per concern

| Concern (audit issue) | Native / embedded (`almide-rt-core`) | Stock p3 host | Parity mechanism |
|---|---|---|---|
| Streaming bodies | rt-core read loop | `consume-body` stream | the shim reads with async `stream.read` into the same call-handle state machine |
| `total_ms` (C-366) | rt-core deadline | none in `wasi:http` | guest-side: a `wasi:clocks/monotonic-clock` `wait-for` raced with `send` on one waitable set; when it fires, `subtask.cancel` and drop the body. The guest writes `request timeout: total_ms <n> exceeded` on every lane |
| `idle_ms` | rt-core per-read timeout | `request-options` first-byte / between-bytes timeouts, if the host supports them | the guest timer is authoritative; the options are a hint (a host may answer `not-supported`) |
| Connect timeout, unbounded response (#2825) | fix in rt-core (`connect_timeout`, size cap) | the host's defaults (wasmtime: 600 s) | the guest `total_ms` deadline and the guest body-size limit bound every lane |
| Cancellation (C-366) | rt-core shutdown | `subtask.cancel` / drop of the response stream | the server observes the close; this is a fixture on each lane |
| Proxies (#2819) | fix in rt-core (`HTTPS_PROXY` / `HTTP_PROXY` / `ALL_PROXY` / `NO_PROXY`) | host configuration; wasmtime has none | promised on native and embedded; stock is out of contract, stated in the row |
| TLS trust roots (#2820) | fix in rt-core (`SSL_CERT_FILE` / `SSL_CERT_DIR`, OS store) | host configuration; wasmtime has webpki-roots only | as above. A wasm guest cannot, and should not, carry its own trust store |
| CR/LF in headers (#2822) | refused | refused (`invalid-syntax`) | the stdlib checks names and values before any lane sends, so the `err` text is the same everywhere; `http.serve` responses get the same check |
| Forbidden headers | accepted today | refused (the nine names in §2.7) | the stdlib refuses the same nine names on every lane. This is a runtime `err`, not a compile rejection, so it is not a dialect epoch |
| URL parsing (#2821) | rt-core `parse_url` today | the guest must split into scheme / authority / path-with-query | one parser in the stdlib, on the `url` module; the parts go to rt-core and to `request.set-*` |
| 1xx, chunk extensions (#2824) | fix in rt-core | hyper handles them | fixtures on native and embedded |
| Response header names | as sent by the server | lowercase | lowercase on every lane (native normalizes) |
| Server body limits (#2823) | `http_server_core` | the shell counts while it reads the request stream (ADR-0020 §5.7) | `413` on every lane |

## 5. Contracts

- **C-328** (the client) widens from "native ⇄ embedded" to "native ⇄
  embedded ⇄ stock p3 host": status, lowercased header set, body and
  error-code class are equal on all three. The rendered text is equal on all
  three for classified errors, and between native and embedded for
  `internal-error`. Its evidence gains a p3 fixture run under
  `wasmtime run -S http`.
- **C-366** (the call handle) gains the stock p3 lane. `total_ms`, `idle_ms`
  and `cancel` are guest-enforced there, and the text is identical.
- **C-367** (serve) keeps ADR-0020's HTTP-semantic wording and adds
  `wasmtime serve` running the same artifact as a third leg.
- A new contract states the header refusals (CR/LF, the nine forbidden names)
  as the same `err` on every lane.

Each new or reworded statement lands in almide/als first, then the pin moves,
then the implementation (CLAUDE.md, "A new C-NNN lands in almide/als FIRST").

## 6. Migration

### 6.1 What breaks for deployed artifacts

- **Programs that do not use HTTP**: nothing. They stay preview-1 core
  modules, byte for byte.
- **Programs that use HTTP**: nothing built before, because
  `almide build --target wasm` refused them (E081). They now build, as a
  component.
- **`ALMIDE_COMPONENT_P3=1` users**: the artifact stops needing
  `-W component-model-async=y,component-model-more-async-builtins=y`. The
  old flags still work. The env switch keeps its meaning for non-HTTP
  programs (opt into p3) and is a no-op for HTTP programs.
- **Runtimes**: an HTTP artifact needs a WASI 0.3 host: wasmtime ≥ 46, jco,
  Spin 3.6+, wasmCloud 2.5+. wasmtime LTS 36 cannot run it. The build line
  says so, and the E081 note for a p1-only runtime no longer applies.

### 6.2 `proofs/target-availability.toml`

- The `p3-component` leg (already in the header vocabulary, "no rows declare
  it yet") becomes a measured leg, and it is the **build** leg for programs
  that use HTTP.
- The stock-p1 rows of `http.*` are replaced by p3-component rows. They
  shrink to empty as §7 steps 3–5 land, and the `pending_port_ceiling` drops
  with them.
- The embedded leg's `http.*` rows follow the p3-component leg, because it is
  the same artifact.
- `http.serve`'s row moves from "a stock p1 artifact has no listening
  socket" to served by the handler export.

### 6.3 Diagnostics

- **E081** stays the code for "no build path serves this function". For an
  HTTP program, its reason names the WASI 0.3 component, not preview 1.
  Today `component_availability` phrases it as "unavailable in the direct
  WASI 0.3 component. … `almide run --target wasm` uses the embedded host".
  The second sentence is removed, because the embedded host then serves
  exactly what the component serves: there is no longer a lane to send the
  user to.
- **E082** (both emitter legs refuse the program's shape) is unchanged.
- `almide check --target wasm` routes the same way as the build, so E081
  appears at check time.

### 6.4 The #865 VM

`almide-wasm-vm` runs the shipped preview-1 artifact of a Critical-profile
program with a closed instruction set. A Critical-profile program is granted
no network capability, so it never contains HTTP, and it keeps building as a
p1 core module. The VM needs no component support, and its closed
instruction set is untouched. If the VM ever runs components, that is its own
record.

### 6.5 Pins

`proofs/wasi-pin-policy.toml` [runtime].flags becomes `-S http=y`. The `-W`
pair and `-S p3=y` go, because p3 is on by default on the floor 46.
`scripts/check-wasi-pins.sh` and `tests/component_p3_test.rs` move in the same
PR. The embedded host's `wasmtime-wasi` and `wasmtime-wasi-http` pins join
[runtime].crate_major (47), in lockstep with `wasmtime`. p3 in
wasmtime-wasi-http is marked experimental, and p3-only fixes are not
backported, so the pin advances with the major rather than by patch.

## 7. Implementation plan

Each step is one PR with its gate, in this order.

| # | Step | Gate |
|---|---|---|
| 0 | als: reword C-328 / C-366 / C-367 (§5) and add the header-refusal contract; advance `proofs/als-pin.txt` | `scripts/check-als-pin.sh` |
| 1 | **Shim without 🚝**: every synchronous `stream.read/write` and `future.read/write` in `to_p3` becomes `[async-lower]` plus one shared "wait until done" helper over `waitable-set.wait` (the fs prefetch and the http body pump already have that loop). Pin flags become `-S http=y` | `component_p3_test` green with **no `-W`**; `check-wasi-pins.sh`; a wasmparser validation with `cm_more_async_builtins` off in the test |
| 2 | **Error classification**: rt-core maps failures to `error-code` cases, with one renderer in rt-core and its twin in the shim; the shim renders the host's case instead of the static `(p3 transport)` text | the refused-port / DNS / TLS fixtures read identically native ⇄ embedded ⇄ `wasmtime run -S http` |
| 3 | **p3 shim coverage an HTTP program needs**: `env.get` / `env.args` / `env.cwd` (`wasi:cli/environment@0.3.0`), `env.sleep_ms` (monotonic `wait-for`), `env.os` / `env.temp_dir` as defined answers or E081 with a reason | p3-component rows for those ops removed |
| 4 | **Client completeness in the shim**: the 6 `*_response` (`response.get-headers` → `fields.entries`), `request_stream`, and the call handle (`start` / `poll` / `read_new` / `wait` / `cancel`, `request_stream_with_limits`) on async `send`, async body reads and a monotonic deadline (§4.3) | cross fixtures native ⇄ p3 for each; the rows shrink; `pending_port_ceiling` drops |
| 5 | **Self-host the SSE family** (`anthropic_streaming_call`, `openai_streaming_call` and the two `_with_limits`) in Almide over the call handle; delete the `almide_rt_sse_*` intrinsics | the 4 rows leave every leg; the interp bridge covers them (3-way oracle) |
| 6 | **The audit in the shared layers**: stdlib URL parser (#2821), CR/LF and forbidden-header refusal (#2822), guest body limits (#2823 client side); rt-core proxies (#2819), trust roots (#2820), connect timeout and size cap (#2825), 1xx and chunk extensions (#2824) | each issue's repro as a fixture on native and embedded; the stdlib checks also on p3 |
| 7 | **Embedded host on wasi:http**: `almide-wasm-run` links `wasmtime_wasi::p3` + `wasmtime_wasi_http::p3` with the rt-core `send_request` hook (§2.2); `almide run --target wasm` runs an HTTP program's p3 component | `tests/embedded_cross_test.rs` green with ops 43..=50 and 53..=59 **unregistered** |
| 8 | **Server export** (ADR-0020 step 7, re-targeted): `wasi:http/handler@0.3.0` with the ADR-0020 §5.4 shape rule, `backpressure` serialization, and the shell's `max_body_bytes`; the embedded serve loop drives the export through `http_server_core` (§3.4) | `tests/http_serve_cross_test.rs` three legs: native, embedded, `wasmtime serve app.wasm` (no flags); a 16-concurrent probe shows one handler in flight per instance |
| 9 | **Default flip**: `almide build --target wasm` builds a program that uses HTTP as a p3 component without `--component` or the env switch; restructure `target-availability.toml` (§6.2); E081 wording (§6.3) | `scripts/check-target-availability.sh`; the E081 diagnostic fixtures |
| 10 | **Retire** host ops 43..=50, 53..=59 and 70..=72: delete `http_call_host.rs` and `host_serve.rs` and their `host.rs` arms; move C-328 / C-366 / C-367 evidence to the component fixtures | a grep gate that `almide-wasm-run` registers no HTTP op |
| 11 | **Document** deployment (`wasmtime run -S http`, `wasmtime serve`) in CHEATSHEET / llms.txt / `docs/stdlib/http.md`, and close `docs/roadmap/on-hold/wasm-http-client.md` | `scripts/check-llm-surface.sh` |

Steps 1, 2 and 6 are independent of each other. Step 7 needs 1 and 2.
Step 8 needs ADR-0020 steps 0a–2 (#2664, #2696, #2697, #2698). Step 9 needs
3, 4 and 7. Step 10 needs 7, 8 and 9. Nothing here adds incumbent-emitter
code (#1696): the shim works on the structural leg's module.

## 8. Alternatives

- **Keep the host ops and add the missing functions as more ops** (status
  quo). It gives two ABIs for one program. The embedded lane would keep
  answering what no stock host can run, and each new function is written
  twice: once in the host, once in the shim.
- **Use wasmtime-wasi-http's default transport (hyper + rustls) in the
  embedded host.** One fewer hook, but native and embedded would stop sharing
  a client. C-328's error texts, and the fixes for #2819/#2820/#2824/#2825,
  would diverge between two lanes that are supposed to be one lane.
- **A P2 stage first** (`wasi:http@0.2`, `incoming-handler`). Refused by the
  owner ruling. It is also a second interface to shim: P2's client
  (`outgoing-handler`, `wasi:io` pollables) shares nothing with P3's
  (`client.send`, component-model streams). Its only advantage, loading
  without `-W`, is gone (Finding 1).
- **Bump wasmtime** to get p3 without flags. Not needed: the flags were the
  guest's (Finding 1). The floor stays 46, and the pin stays 47.
- **Build every program as a component.** It costs 1.3 KB → 18 KB for
  programs that gain nothing, drops wasmtime LTS 36, and takes the #865 VM's
  input format away from it.
- **`wasi:sockets` for the server** (#2659 option 1). Rejected in ADR-0020 §10
  for serving platforms. The reasons stand.

## 9. Refinements the evidence forced

1. **"P3 needs experimental flags" was a property of Almide's shim, not of
   P3.** The 🚝 synchronous builtins are the only gated ones; an async-builtin
   handler serves with no flags on wasmtime 47 (§2.1). This removed the
   reason for a wasmtime bump and the premise of ADR-0020 §5.3.
2. **"No custom ops" means no host-served ops.** Op numbers between the
   emitted code and the guest shim are an internal ABI that never leaves the
   artifact (§1.1).
3. **The embedded host keeps rt-core as its transport.** It implements
   `wasi:http` through the `send_request` hook, not through hyper, so native
   and embedded still share one client (§2.2, §8).
4. **Error parity moves into the guest**: one `error-code` classification and
   one renderer, with native adopting it (§4.2).
5. **Components by default only for programs that use HTTP** (§3.1 point 3).
6. **The p3 shim must serve env and args first**, or LLM clients cannot build
   (§2.3).
7. **Per-instance serialization uses `backpressure.inc/dec`**, the baseline
   builtin, not a guest queue (§3.3).
8. **The SSE functions become Almide over the call handle**, not shim code
   (§3.1 point 1).

## 10. Consequences

What we gain:

- One HTTP artifact for `wasmtime run`, `wasmtime serve`, Spin, wasmCloud,
  jco and `almide run --target wasm`.
- Every `http` function, including servers, becomes buildable for wasm, and
  no function is served on one wasm lane but not another.
- The audit fixes land once per layer: the stdlib for what the program
  observes, rt-core for the transport.
- `almide-wasm-run` loses three op families and ~320 lines of host code
  (`http_call_host.rs`, `host_serve.rs`).

What it costs:

- About 64 more crates in the embedded host (tokio, hyper, wasmtime-wasi,
  wasmtime-wasi-http), and a dependency on wasmtime's experimental p3 module.
- HTTP artifacts are ~30 KB components and need a WASI 0.3 host; LTS
  runtimes are out for them.
- Native's transport error wording changes once (§4.2).
- Proxies and trust roots on stock hosts are the host's, not Almide's, and
  the contracts say so instead of promising them.
