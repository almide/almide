# C-006 — fan.timeout was removed (0.29.0)

> Prose companion to contract C-006 in [contracts.toml](contracts.toml).
> Status: **active** — the contract now certifies the *absence* of the
> operation: referencing `fan.timeout` is a check-time tombstone error (E027),
> identical on both targets. This retired the ledger's last
> `flagged-for-revision` entry; the equivalence claim carries no exceptions
> clause.

## History: why it was the sole exemption (0.24.0 – 0.28.x)

`fan.timeout(ms, thunk)` was a **wall-clock effect**. The two targets had
fundamentally different notions of time:

| Target | Mechanism | Result |
|--------|-----------|--------|
| native | a real thread + `recv_timeout(ms)` | the timeout could fire; the slow thunk was abandoned |
| wasm   | no clock, no scheduler, no threads | the thunk ran to completion; the timeout never elapsed |

There is no portable meaning for "elapsed wall-clock time" in the wasm
sandbox, so `fan.timeout` was excluded from the cross-target equivalence
guarantee, with a loud build-time warning when emitting wasm. It was the only
`fan.*` op left out: `fan.race` / `fan.any` / `fan.map` / `fan.settle` were
all made deterministic by list order (see C-004 / C-005) and DO hold the
equivalence promise.

## Why removal, not portable semantics (0.29.0)

Worse than the cross-target divergence: whether the deadline fired depended on
machine load, so the result was not a function of the program + its inputs
**even between two native runs** — the sole stdlib surface violating that
property. Inventing "portable" semantics (cooperative deadline checks) would
have kept the nondeterminism while adding new semantics to save one function.
An org-wide sweep found zero consumers. So the operation was removed outright:

- The type checker rejects `fan.timeout` with a dedicated tombstone (E027)
  carrying the migration hint, instead of a generic unknown-member error.
- Deadlines belong at the **host boundary** that invokes the program
  (`timeout 5 ./app`, a wasmtime host deadline, a supervisor).
- The native runtime (`almide_rt_fan_timeout`), the wasm emit arm, the v1 MIR
  desugar, and the CLI build-time warnings were all deleted in the same change.

## Evidence

- `tests/diagnostics/e027-fan-timeout-removed/` — broken/fixed pair: the
  broken file must produce `error[E027]` with the host-boundary hint; the
  fixed file (the same work via `fan.race`) must compile cleanly.
- `tests/diagnostic_harness_test.rs::broken_files_produce_expected_diagnostics`
  — the harness that executes every diagnostics case.

The tombstone is target-independent by construction (type check runs before
the target split), so no `spec/wasm_cross` fixture applies: there is no
program containing `fan.timeout` that reaches either backend.
