# Almide WASM Documentation

Almide compiles to a standalone `wasm32-wasip1` module. There is exactly one
wasm path: the **v1 MIR trust spine** in `crates/almide-mir` — the unverified v0
emitter was retired in #782, so a shape the spine cannot lower is an honest wall
(a clean error), never a silent fallback into unverified codegen.

## What ships today

| Property | Shipped behaviour |
|---|---|
| Target | `wasm32-wasip1`, one exported linear memory, `_start` entry |
| Memory | Bump allocation + **Perceus reference counting**, with a per-function ownership certificate the kernel-proven checker re-verifies |
| Equivalence | Observable output (stdout, stderr, exit code) is byte-identical to the native leg, contract by contract |
| Walls | Outside the lowering subset ⇒ `Unsupported(...)`, surfaced to the user; `ALMIDE_WALL_REASON=1` names which stage declined |

The authoritative references are:

- **Architecture** — [docs/ARCHITECTURE.md](../ARCHITECTURE.md) (compiler pipeline)
  and [docs/roadmap/active/v1-mir-architecture.md](../roadmap/active/v1-mir-architecture.md)
  (why ownership and layout are decided once, in MIR).
- **Cross-target equivalence** — [docs/contracts/](../contracts/): every observable
  native ⇄ wasm promise is a named contract traceable to an executable fixture.
- **Ownership certificates** — [docs/roadmap/active/certificate-format-v1.md](../roadmap/active/certificate-format-v1.md).

## Documents here

| Doc | What |
|-----|------|
| [Capability System](./capability-system.md) | Compile-time least-privilege enforcement — current |

## Archive

[`archive/`](./archive/) holds the 2026-04 design notes written **before** the v1
trust spine became the sole wasm path. They describe an agent-container product
direction and a v0-era memory model (two linear memories, bump allocation with
no free) that the shipped compiler does not implement. They are kept for design
history and are banner-marked as archived; do not read them as a description of
what ships.

| Archived doc | Why it is archived |
|---|---|
| [Memory Model](./archive/memory-model.md) | Describes the v0 multi-memory, no-free model; the shipped model is single-memory with Perceus reference counting |
| [WASM 3.0 Features](./archive/wasm3-features.md) | Predates the v1 renderer's actual instruction selection |
| [Agent Container](./archive/agent-container.md) | Product direction, never built |
| [hatch Design](./archive/hatch-design.md) | MCP bridge tool, never built |
| [Bubblewrap](./archive/bubblewrap.md) | WASM-on-WASM orchestration, never built |
| [Ecosystem](./archive/ecosystem.md) | 2026-04 landscape survey, now stale |
