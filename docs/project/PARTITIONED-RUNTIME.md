# Partitioned-runtime requirements (#568)

What a partition-style host (ARINC 653-shaped time/space partitioning, or
any environment that budgets memory and CPU per component) must provide to
run a Critical-profile Almide wasm artifact, and what the artifact itself
guarantees. Every guarantee row names its enforcing gate — this document
describes machinery, it does not add any.

## What the artifact guarantees (space)

| Property | Mechanism | Gate |
|---|---|---|
| Fixed heap budget | `almide build --heap-cap <bytes>` bakes a bump-frontier ceiling into the module; exceeding it is the DEFINED abort (`Error: out of memory`, exit 1) at a deterministic point — never silent growth | `tests/static_memory_test.rs::fixed_heap_budget_suffices_and_is_enforced` |
| Steady-state allocation discipline | `almide check --profile critical` (#567) rejects allocation inside counted loops, runtime-length heap constructors, closures, recursion and `while` — a critical-clean program's loop bodies allocate at most bounded transients (scalar-to-string conversion for the declared print capability) | `tests/static_memory_test.rs::reference_app_is_critical_clean` |
| Allocation-free kernel arithmetic | The control-law fns' emitted Rust carries no heap tokens (no Vec/Box/String/format!/clone/RcCow) | `tests/static_memory_test.rs::kernel_fns_emit_allocation_free_rust` (and `tests/wcet_kernel_test.rs` for the Float kernel) |
| Single non-shared linear memory, no thread imports | The structural leg emits one memory, never shared, and imports only the five-op host surface (p1: `wasi_snapshot_preview1`; p2/p3 components: the vendored WIT worlds) | `tests/static_memory_test.rs::artifact_is_partition_shaped` |
| Bounded stack | Under `--profile critical` the call graph is a DAG (recursion rejected, E073), so stack depth is bounded by the static call-graph depth; wasm's own stack is host-bounded on top | the profile gate (`tests/critical_profile_test.rs`) |

## What the host must provide

1. **A WASI host for the artifact's declared surface only** — console
   out, exit codes, stdin, entropy, wall clock. Everything else is the
   defined refusal inside the artifact (message + exit 1), so the host
   needs no filesystem, network, or process surface. The three artifact
   forms: preview1 core module (default `--target wasm`), WASI 0.2
   component (`--component`), WASI 0.3 component (experimental,
   `ALMIDE_COMPONENT_P3=1` — #1628 stage 2).
2. **Memory partitioning**: the linear memory declares its initial pages;
   a host enforcing a per-partition memory quota can additionally set a
   wasmtime `StoreLimits` ceiling — the artifact's own `--heap-cap` abort
   fires FIRST when the budget is the tighter bound, keeping the failure
   the defined one.
3. **Time partitioning**: the artifact never blocks except in host I/O
   calls and contains no timers, signals or threads, so a fuel- or
   epoch-based preemption hook (wasmtime `-W epoch-interruption`) is
   sufficient for a time quantum. The deterministic compute meter
   (C-320, `compute.*` budgets) is the in-language quantum — a program
   that meters its hot loop yields deterministically, independent of
   host preemption.
4. **No ambient concurrency**: the guest is single-threaded by
   construction; the host must not require thread imports (none exist to
   satisfy).

## The qualification boundary

A qualified/minimal execution environment for this artifact class —
which host, qualified how — is #865's scope, not this document's. This
document is the artifact-side half of that contract: everything the host
has to provide is enumerated above, and everything above it is gated in
this repo's CI.
