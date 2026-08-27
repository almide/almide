# Almide WASM Documentation

Almide compiles to a standalone `wasm32-wasip1` module. Since commissioning
(#1599) there are **two verified wasm legs behind one router**
(`render_wasm_module_routed` in `src/cli/build.rs`). The unverified v0 emitter
was retired in #782, so a shape neither leg lowers is an honest wall (a clean
error), never a silent fallback into unverified codegen.

| Leg | Engine | Routed to it |
|---|---|---|
| **structural** (default) | The commissioned greenfield engine: `almide::wasm_leg` front + the `crates/almide-wasm` direct emitter. Accepted at 610/610 byte-identical to native on the `wasm_cross` corpus; build artifacts ship in the WASI form (#1588) and run on stock runtimes. Ownership is compiler-placed reference counting with copy-on-write; no certificate is emitted yet — its evidence is the byte-exact corpus (grow-only floor) and the semantic-mutation net in `crates/almide-wasm` | Every program with a `main`, no external packages, and no host-variant I/O on the build path |
| **incumbent v1** | The MIR trust spine in `crates/almide-mir`: certified MIR → direct emit, with the per-function ownership certificate the kernel-proven checker re-verifies on every build | Main-less library modules (#881 export mode), dependency-bearing projects, host-variant programs on the build path, `ALMIDE_FUEL_PROBE` runs, and any shape the structural leg walls on — a verified-to-verified handover, named under `ALMIDE_VERIFIED_DEBUG=1` |

The `Built …` line every wasm build prints names the leg that produced the
bytes. `ALMIDE_WASM_INCUMBENT=1` forces the incumbent (a reversible switch kept
for one release); `ALMIDE_WASM_STRUCTURAL=1` forces the structural leg and turns
its walls into hard errors (the frontier-development probe).

## What ships today

| Property | Shipped behaviour |
|---|---|
| Target | `wasm32-wasip1`, one exported linear memory, `_start` entry |
| Memory | Bump allocation + **Perceus-style reference counting** on both legs; the incumbent leg additionally emits a per-function ownership certificate the kernel-proven checker re-verifies |
| Equivalence | Observable output (stdout, stderr, exit code) is byte-identical to the native leg, contract by contract |
| Walls | Outside the lowering subset ⇒ `Unsupported(...)`, surfaced to the user; `ALMIDE_WALL_REASON=1` names which stage declined |

The authoritative references are:

- **Architecture** — [docs/ARCHITECTURE.md](../ARCHITECTURE.md) (compiler pipeline,
  including the leg router)
  and [docs/roadmap/active/v1-mir-architecture.md](../roadmap/active/v1-mir-architecture.md)
  (why ownership and layout are decided once, in MIR — the incumbent leg).
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
