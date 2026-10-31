# Almide WASM Documentation

Almide emits core Wasm modules and WASI components through **one
wasm leg**, the structural leg (entered through `render_wasm_module_routed` in
`src/cli/build.rs`). The unverified v0 emitter was retired in #782, and the
incumbent v1 leg (the MIR→WAT trust spine in `crates/almide-mir`) lost its last
route in #2752 and was deleted in #2761. A shape the structural leg does not
lower is an honest wall — `error[E082]` with a `wall: <reason>` line and a
`--> in <fn>` site line — never a silent fallback.

| Leg | Engine | Routed to it |
|---|---|---|
| **structural** (the only leg) | The commissioned greenfield engine: `almide::wasm_leg` front + the `crates/almide-wasm` direct emitter. Core modules are packaged for stock WASI or supported component routes. Ownership is compiler-placed reference counting with copy-on-write. The emitter records a certificate bundle; `almide verify` and the certified-corpus gates check covered properties of supported core artifacts, not components. Differential and mutation gates provide separate behavioral evidence | Programs and exported libraries within the structural emitter's supported shapes and the selected host route's capabilities |

The switch that forced the incumbent was removed with its route (#2752);
`ALMIDE_WASM_SKIP_STOCK_AUDIT=1` now only skips the stock-WASI host-op audit
on a build (the frontier-development probe).

## What ships today

| Property | Shipped behaviour |
|---|---|
| Target | Core `wasm32-wasip1` commands and supported WASI component routes; HTTP serve-shaped programs build as WASI 0.3 handler components |
| Memory | Bump allocation + **Perceus-style reference counting**. For supported core artifacts (not components), the structural certificate is checked by `almide verify` and certified-corpus gates, with [explicit coverage and trust limits](../design/MEMORY-SAFETY.md); the runtime's bytes are checked against the Coq decoder model by `proofs/check-structural-bytes.sh` |
| Equivalence | Observable output is compared contract by contract; declared host observations, resource limits and oracle-relative behavior retain their documented scope |
| Walls | Outside the lowering subset ⇒ `error[E082]` naming the wall and the function, surfaced to the user; under `almide test`, `ALMIDE_WALL_REASON=1` names which stage declined |

## Running a server on wasm

`almide run app.almd --target wasm` runs structural-leg modules on the
**embedded host** (wasmtime inside the `almide` binary), and that host serves
`http.serve` (C-367, #2650). The guest owns the serve loop: `main` runs once,
calls `http.serve`, and then takes one request at a time from the host and
hands back one response, all in the same instance. So a value `main` computed
before `http.serve` is the same on every request, as it is natively. The host
only moves bytes (fs_call ops 70..=72), through the server code the native
runtime uses (`crates/almide-rt-core/src/http_server_core.rs`), so the
response bytes are identical to native. While the program runs, its stderr
is unbuffered and its stdout follows native's rule (flushed on every write to
a terminal, 64 KiB-buffered otherwise), so a server's output shows up while it
runs.

The embedded route above is distinct from a stock server artifact. A supported
serve-shaped program built with `almide build --target wasm` becomes a
`wasi:http/handler@0.3.0` component for `wasmtime serve` (C-375, #2659).
The build validates that shape and the component's available operations;
`--host js` does not support this server export. See `src/cli/build_wasm.rs`
for route selection rather than treating every Wasm route as the same host.

## What the embedded host grants (it is not a sandbox)

`almide run --target wasm` runs a program with the authority of the user who
runs it, as the native binary does. Wasm bounds the guest's own memory
accesses. Manifest permissions can restrict declared capabilities and filesystem
roots, but these controls are not a general process-containment guarantee.
Do not treat the embedded host as isolation for arbitrary untrusted code.

| Lane | When | Files | Environment | Other host access | Memory |
|---|---|---|---|---|---|
| core module (`almide.*` imports, `crates/almide-wasm-run/src/host.rs`) | most programs | `[permissions] fs` roots when configured, enforced through scoped filesystem operations; without that key, paths remain unrestricted by this control | the process environment (read; `env.set` writes a per-run overlay, not the process) | process spawn, tcp / unix sockets, shared memory, http client and server, stdin, the clocks, all as native | ONE budget per run across the run's instance, its `fan` worker instances and a `testing.assert_aborts` child (#4135): the test harness's cap when it names one, otherwise the least of 4 GiB (the wasm32 address space), the host's cgroup memory limit and its physical memory, read from the environment (C-197) |
| p3 component (`crates/almide-wasm-run/src/component_host.rs`) | programs whose host ops the core lane serves only through WASI 0.3 (the http client family, ADR-0023 step 7) | configured `[permissions] fs` roots preopened read-write; without that key, the working directory and temporary directory (not `/`) | inherited in full, plus `ALMIDE_CWD` | stdio inherited; wasi:http through native's client; wasi:sockets as wasmtime-wasi provides | no store limit beyond wasm32's 4 GiB per memory |

Filesystem scoping is implemented (#4022): the core host checks and executes
filesystem operations beneath configured roots on 64-bit Linux/macOS; other
platforms retain a path-based check subject to check/use races
(`crates/almide-rt-core/src/fs_beneath_sys.rs`). The component host preopens
those roots, or cwd/temp when no roots are configured. These defaults differ
between lanes, and neither grants general process containment. The stock
artifact receives the authority its external runtime supplies, for example
`wasmtime run --dir=<dir>`. The authority is implemented in
`crates/almide-wasm-run/src/host_fs_scope.rs` and `component_host.rs`.

On the core-module lane, a guest pointer or length that falls outside the
guest's memory is a trap of that guest (`Error: wasm trap: out of bounds memory
access`, exit 1). It is never a host panic, and the host never allocates a
buffer sized by the forged length (#4135). On the component lane the canonical
ABI in wasmtime checks the same spans. Host output is written as the guest's bytes, never re-encoded
(C-399).

The authoritative references are:

- **Architecture** — [docs/ARCHITECTURE.md](../ARCHITECTURE.md) (compiler pipeline,
  including the wasm route)
  and [docs/roadmap/active/v1-mir-architecture.md](../roadmap/active/v1-mir-architecture.md)
  (why ownership and layout are decided once, in MIR — the design of the incumbent leg, retired by #2761; MIR now serves the native leg).
- **Cross-target equivalence** — [docs/contracts/](../contracts/): every observable
  native ⇄ wasm promise is a named contract traceable to an executable fixture.
- **Ownership certificates** — [docs/roadmap/active/certificate-format-v1.md](../roadmap/active/certificate-format-v1.md).

## Documents here

| Doc | What |
|-----|------|
| [Capability System](./capability-system.md) | Compile-time least-privilege enforcement — current |
| [JS host async imports](./JS-HOST-ASYNC-IMPORTS.md) | `@extern(wasm, "js", ..., returns: promise)` on `--host js`: JSPI suspension, which exports become async, serialisation, the no-JSPI refusal, the unmarked-Promise refusal (#3353, #3371) — current |

## Archive

The 2026-04 design notes written **before** the v1 trust spine became the sole
wasm path (memory model, WASM 3.0 features, agent container, hatch, bubblewrap,
ecosystem survey) were removed from the tree — they described an agent-container
product direction and a v0-era memory model (two linear memories, bump
allocation with no free) that the shipped compiler does not implement, and
keeping them in `docs/` made them findable as if they described what ships.
They remain in git history; `git log --diff-filter=D -- docs/wasm/archive/`
finds the removal commit.
