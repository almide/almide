# Partitioned-runtime requirements (#568)

What a partition-style host (ARINC 653-shaped time/space partitioning, or
any environment that budgets memory and CPU per component) must provide to
run a Critical-profile Almide wasm artifact, and what the artifact itself
guarantees. The concrete artifact checks below exercise the PID reference
application in `tests/static_memory_test.rs`, not every Critical-profile program
or every Wasm component route. Capability grants and actual imports must be
reviewed for each deployed artifact. Every row names its gate.

## What the artifact guarantees (space)

| Property | Mechanism | Gate |
|---|---|---|
| Fixed heap budget | `almide build --heap-cap <bytes>` bakes a bump-frontier ceiling into the module; exceeding it is the DEFINED abort (`Error: out of memory`, exit 1) at a deterministic point — never silent growth | `tests/static_memory_test.rs::fixed_heap_budget_suffices_and_is_enforced` |
| Steady-state allocation discipline | `almide check --profile critical` (#567) rejects allocation inside counted loops, runtime-length heap constructors, closures, recursion and `while` — a critical-clean program's loop bodies allocate at most bounded transients (scalar-to-string conversion for the declared print capability) | `tests/static_memory_test.rs::reference_app_is_critical_clean` |
| Allocation-free kernel arithmetic | The control-law fns' emitted Rust carries no heap tokens (no Vec/Box/String/format!/clone/RcCow) | `tests/static_memory_test.rs::kernel_fns_emit_allocation_free_rust` (and `tests/wcet_kernel_test.rs` for the Float kernel) |
| Single non-shared linear memory, no thread imports | The reference core artifact has one non-shared memory and no import whose name/module contains `thread`; this test does not assert a universal five-operation import set | `tests/static_memory_test.rs::artifact_is_partition_shaped` |
| Bounded stack | Under `--profile critical` the call graph is a DAG (recursion rejected, E073), so stack depth is bounded by the static call-graph depth; wasm's own stack is host-bounded on top | the profile gate (`tests/critical_profile_test.rs`) |

## What the host must provide

1. **A host implementing the actual artifact imports** — inspect the built
   module/component and its granted capabilities. Other programs can require
   filesystem, network or private process operations; they are not all
   converted into the five-operation reference surface. Packaging selects a
   core or component route (`src/cli/build_wasm.rs`); serve-shaped HTTP builds
   use a WASI 0.3 handler component.
2. **Memory partitioning**: the linear memory declares its initial pages;
   a host enforcing a per-partition memory quota can additionally set a
   wasmtime `StoreLimits` ceiling — the artifact's own `--heap-cap` abort
   fires FIRST when the budget is the tighter bound, keeping the failure
   the defined one.
3. **Time partitioning**: the host must supply and validate its preemption
   and host-call timeout policy. The deterministic compute meter (C-320)
   limits modeled charges cooperatively; it does not preempt a blocking host
   call or by itself prove a hardware time quantum.
4. **Concurrency policy**: validate the selected route and host services.
   The reference artifact's no-thread-import check is not a proof that every
   host service or every other Almide artifact is single-threaded.

## The qualification boundary

A qualified/minimal execution environment for this artifact class —
which host, qualified how — is #865's scope, not this document's. This
document is the artifact-side half of that contract: the reference artifact properties are gated, while a deployment
must establish its own imports, resource budgets, and host guarantees.
