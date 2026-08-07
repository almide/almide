# Substrate observability — the C-004 EXCEPTION, reproduced

Evidence for [ADR-0011](../../../docs/adr/0011-execution-substrate-is-a-free-variable.md)
and [execution-inception.md](../../../docs/roadmap/active/execution-inception.md) §1.

```
research/spike/substrate-observability/run-repro.sh [runs]
```

## What it shows

`fan {}` arms that print are the one surface where the execution substrate
reaches the observation. The ledger records this as a cross-target divergence
(C-004's EXCEPTION: "wall-clock on native and sequential on wasm"). The
measurement shows the classification is wrong — it is a **native ⇄ native**
divergence, the property that got `fan.timeout` removed in 0.29.0 and that
C-006 calls "the sole stdlib surface violating that property".

Measured 2026-08-07 (almide 0.56.0, wasmtime 47.0.2, 14-core arm64 macOS):

| substrate | runs | distinct arm orders |
|---|---|---|
| native (scoped OS threads) | 10 | **8–9** |
| wasm (sequential inline) | 3 | 1 (`ABCD`) |
| reference interpreter | — | 1 by construction (`eval.rs:148`) |

Two of the three oracles agree with the list-order sequential observation that
[#1000](../../../docs/roadmap/active/concurrency-stance.md) defines. Native is
the outlier.

## Notes for whoever re-runs this

- **The work must be unfoldable.** An earlier attempt passed a literal to the
  spin loop; LLVM evaluated it at compile time, every arm finished instantly,
  and the divergence hid behind a clean `A B C D` on both targets. The seed
  comes from `env.args()` for this reason.
- **A single run proves nothing.** The first native run of a session often
  prints in arm order. The defect is visible only across runs.
- **`env.sleep_ms` shows the parallelism more directly** (all four arms report
  the same start and end millisecond, total ≈ one arm's duration), but it walls
  on the wasm leg as a missing capability, so it cannot carry the cross-target
  half of the comparison.

## When Rung 1 lands

Arm-scoped output transactions (buffer per arm, flush in arm order at join)
make every substrate print `ABCD`. At that point the script's exit codes invert
in meaning: flip it into a gate, move it out of `research/spike/`, and delete
C-004's EXCEPTION clause in the same PR.
