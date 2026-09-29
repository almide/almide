# Almide WASM Documentation

Almide compiles to a standalone `wasm32-wasip1` module through **one
wasm leg**, the structural leg (entered through `render_wasm_module_routed` in
`src/cli/build.rs`). The unverified v0 emitter was retired in #782, and the
incumbent v1 leg (the MIR→WAT trust spine in `crates/almide-mir`) lost its last
route in #2752 and was deleted in #2761. A shape the structural leg does not
lower is an honest wall — `error[E082]` with a `wall: <reason>` line and a
`--> in <fn>` site line — never a silent fallback.

| Leg | Engine | Routed to it |
|---|---|---|
| **structural** (the only leg) | The commissioned greenfield engine: `almide::wasm_leg` front + the `crates/almide-wasm` direct emitter. Accepted at 610/610 byte-identical to native on the `wasm_cross` corpus; build artifacts ship in the WASI form (#1588) and run on stock runtimes. Ownership is compiler-placed reference counting with copy-on-write; no certificate is emitted yet — its evidence is the byte-exact corpus (grow-only floor) and the semantic-mutation net in `crates/almide-wasm` | Every program with a `main`, no external packages, and no host-variant I/O on the build path |

The switch that forced the incumbent was removed with its route (#2752);
`ALMIDE_WASM_STRUCTURAL=1` now only skips the stock-WASI host-op audit
on a build (the frontier-development probe).

## What ships today

| Property | Shipped behaviour |
|---|---|
| Target | `wasm32-wasip1`, one exported linear memory, `_start` entry |
| Memory | Bump allocation + **Perceus-style reference counting**. No per-build ownership certificate is re-verified for a shipped wasm byte (the structural leg's certificate is #2755–#2760); the runtime's bytes are checked against the Coq decoder model by `proofs/check-structural-bytes.sh` |
| Equivalence | Observable output (stdout, stderr, exit code) is byte-identical to the native leg, contract by contract |
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

`wasmtime serve` is not used: it creates a new instance per request (p2) or
reuses one only for a bounded number of requests (p3), and its own HTTP stack
writes the response head. A stock artifact from `almide build --target wasm`
has no listening socket, so `build` and `check --target wasm` still refuse
`http.serve` (E081); #2659 tracks that.

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

## Archive

The 2026-04 design notes written **before** the v1 trust spine became the sole
wasm path (memory model, WASM 3.0 features, agent container, hatch, bubblewrap,
ecosystem survey) were removed from the tree — they described an agent-container
product direction and a v0-era memory model (two linear memories, bump
allocation with no free) that the shipped compiler does not implement, and
keeping them in `docs/` made them findable as if they described what ships.
They remain in git history; `git log --diff-filter=D -- docs/wasm/archive/`
finds the removal commit.
