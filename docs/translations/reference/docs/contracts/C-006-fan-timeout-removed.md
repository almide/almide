# C-006 — fan.timeout was removed (0.29.0)

[![English](https://img.shields.io/badge/English-24292f?style=flat-square)](C-006-fan-timeout-removed.md) [![日本語](https://img.shields.io/badge/%E6%97%A5%E6%9C%AC%E8%AA%9E-d0d7de?style=flat-square)](C-006-fan-timeout-removed.ja.md) [![简体中文](https://img.shields.io/badge/%E7%AE%80%E4%BD%93%E4%B8%AD%E6%96%87-d0d7de?style=flat-square)](C-006-fan-timeout-removed.zh-CN.md)

> Non-normative, source-pinned reader snapshot; unchanged original remains authoritative. No automatic synchronization, independent execution, measurements or fresh verification of contract guarantees. Current and historical statements refer to the pinned source. C-006 distinguishes removal of the old timeout form from its later cooperative block form. C-033 preserves pre-fix diagnosis and closure instructions as history; do not execute its old deletion/gate-edit instructions. Repr helper/export details describe the historical emitter, not the current public ABI. Code and comments remain verbatim. [Source](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/contracts/C-006-fan-timeout-removed.md)

> Prose companion to contract C-006 in [contracts.toml](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/contracts/contracts.toml).
> Status: **active, superseded in part in 0.47.0** — C-006 records the
> removal of the old `fan.timeout(ms, thunk)` form in 0.29.0. The current
> `fan.timeout(duration) { body }` form is the cooperative oracle-tier
> deadline of C-208; see [the current fan specification](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/SPEC.md#136-fantimeout--the-oracle-tier-wall-clock-deadline).
> The sections below explain the historical removal, not an absence of
> `fan.timeout` from the current language.

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

## Old-form diagnostic evidence

- `tests/diagnostics/e027-fan-timeout-removed/` — broken/fixed pair: the
  broken old-form call produces `error[E027]` (`fan.timeout changed signature`)
  with the `fan.timeout(duration.ms(5000)) { work(x) }` migration hint.
  The fixed file uses that current block form and must compile cleanly.
- `tests/diagnostic_harness_test.rs::broken_files_produce_expected_diagnostics`
  — the harness that executes every diagnostics case.

The old-form tombstone is target-independent because type checking precedes
the target split. Current block-form timeout behavior and its cross-target
limits are covered separately by C-208, not by this removal record.
