# Async JS imports on `--host js` (JSPI, #3353)

Status: implemented on the JS host only (`src/cli/js_host_async.rs`). The
module bytes do not change. Only the generated `<mod>.js` / `<mod>.d.ts` do.

**Provisional.** `--async-import` ships in v0.67.0-rc1 but may be replaced
before v0.67.0. #3371 proposes taking the JSPI boundary from `fan` instead:
imports reachable inside a `fan` are suspendable, exports that contain a `fan`
return a Promise, and no build flag is needed.

## The problem

A `@extern(wasm, "js", NAME)` hook has to return its value synchronously.
Workers KV, `fetch` and Cloud Storage are only available as Promises, so a JS
host could not do its storage I/O from Almide code. JSPI
(`WebAssembly.Suspending` / `WebAssembly.promising`) solves this:

- a Suspending import suspends the wasm stack until the hook's promise
  settles;
- an export entered through `promising` returns a Promise.

The reporter's prototype edited only the glue. It ran correctly on Node 24,
on workerd, and on the Cloudflare edge.

## Choices

**1. The marker is a build flag, not source syntax.**
`almide build app.almd --target wasm --host js --async-import kv_get,kv_count`.

- ADR-0024 N1 keeps `async`, `await` and `Future` out of the language surface,
  and the effect axis stays `fn` / `effect fn` "with no colour".
- ADR-0011 makes the execution substrate a free variable that must not change
  the observation. Whether a hook settles now or later is a property of the
  host's substrate. It is not a property of the program.
- The Almide declaration stays `fn kv_get(key: String) -> String`. Callers see
  an ordinary synchronous call, and the same source builds for the native
  target unchanged.
- Each name must be an extern import the program declares. Otherwise the
  build refuses, naming it. `--async-import` without `--host js` is also
  refused.

**2. Effects.** An async import gets no new effect category.

- Under ADR-0026 an extern body is foreign and is already inferred as ⊤ (every
  category; `pass_effect_inference.rs`). Its callers already carry it.
- Suspension is how the host runs the import. It is not something the import
  does to the program, so neither the type nor the category changes.

**3. Which exports become async.** Exactly the exports whose call graph can
reach an async import, read from the shipped bytes (after the optional
`--wasm-opt`).

- Measured on Node 24.21: a Suspending import that is called from an export
  not entered through `promising` throws `SuspendError`. This happens even
  when the hook returns a plain value. So "only the exports that might
  suspend" has to be a guarantee.
- Every `call_indirect` / `call_ref` is assumed to reach every function in the
  table. Over-approximating only makes an export async needlessly. It never
  leaves one synchronous that can suspend.
- Making every export async whenever any import is async would also be
  correct. It would force every caller to await pure helpers (`width`), so
  reachability is used instead.
- `run()` follows the same rule through `_start`.
- An async export returns `Promise<T>` in the `.d.ts`. A hook's type becomes
  `T | Promise<T>`.

**4. One call in the instance at a time.**

- JSPI lets a second export call enter while one is suspended. The structural
  runtime keeps per-instance state that has to stay stack-disciplined across
  a call:
  - the bump/heap head, which a region window rewinds at its close;
  - the line-buffer build cursor;
  - the deterministic meter's region globals.
- Interleaving two suspended stacks over that state is exactly what ADR-0023
  (§ re-entrancy) refuses for p3 handlers. It serialises them with
  backpressure.
- The glue serialises in the same way:
  - Async exports run through one FIFO queue (`serial`). Each runs to
    completion, including its suspensions, before the next one enters.
  - A synchronous export (or `run()`) called while an async call is suspended
    throws `almide: <f> was called while an async call is suspended …`
    instead of entering.
- The observation is that of the calls made one after another, in call order
  (ADR-0024's "as if sequential"). The fixture asserts that at most one hook
  is ever pending.

**5. Where JSPI is missing**, `init()` throws before instantiating. The error
names the async imports and what is missing (`WebAssembly.Suspending /
WebAssembly.promising`), and points at Node ≥ 24.20, Chrome ≥ 137 or workerd.
Under Node it also names the running version: 24.0 through 24.19 share a V8
with JSPI off by default, and only 24.20 turns it on (#3362). Managed "Node 24"
runtimes can lag behind; on 2026-10-04, Google Cloud Run functions' `nodejs24`
was 24.19.0. On those versions, `--experimental-wasm-jspi` works only on the
`node` command line: Node refuses it in `NODE_OPTIONS`, and
`v8.setFlagsFromString` at run time does not install the API.
Nothing is attempted half-way. A module without `--async-import` produces the
same glue as before, byte for byte, and runs where it ran before.

**6. Marshalling around a suspension.**

- An async import's arguments are decoded before the hook runs (inside the
  call expression). Its result is encoded after the promise settles. The
  reporter confirmed that calling `__alloc` from the resumed hook is fine.
- An export's arguments are built inside its queued turn, after the calls
  ahead of it have settled.
- Ownership and layouts are the same recorded export ABI as the synchronous
  wrappers (#3352, #3354). `memoryBytes()` stays flat over 1,000 async rounds
  in the fixture.

## Not covered

- **A hook that calls back into the module.** A synchronous export called from
  inside an async hook is refused by the busy guard. An async one would queue
  behind the call that is waiting for it, and never settle. Don't do it.
- **A hook that rejects** is handled like a throwing synchronous hook (#3356,
  docs/wasm/WASM-OUTPUT.md "JS host"): a fallible extern (`effect fn` or
  `Result[T, String]`) gets the rejection back as an err; an infallible one
  abandons the instance until `init()` runs again.
- **Flow / streams** (`docs/roadmap/on-hold/flow-design.md`, "JSPI での Flow
  実装"). This note covers single-value imports only.

## Gate

`spec/wasm_host_js/async_imports.almd`, run by `scripts/check-js-host.sh`. A
fixture's `// @host-flags:` line passes the flag. The fixture checks:

- which exports are async and which stay sync;
- overlapping calls are answered correctly, in order, one at a time;
- an async import reached through a closure, and an err from an effect fn;
- a rejecting infallible hook (abandons the instance; `init()` recovers);
- flat memory;
- the refusal without JSPI.

The gate needs a node with JSPI, so CI's JS host job runs Node 24 (setup-node
resolves the latest 24.x, which is past 24.20). Locally a node without JSPI skips
that fixture with a warning.
