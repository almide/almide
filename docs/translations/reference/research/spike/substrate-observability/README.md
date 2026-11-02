# Substrate observability — the C-004 EXCEPTION, reproduced

[![English](https://img.shields.io/badge/English-24292f?style=flat-square)](README.md) [![日本語](https://img.shields.io/badge/%E6%97%A5%E6%9C%AC%E8%AA%9E-d0d7de?style=flat-square)](README.ja.md) [![简体中文](https://img.shields.io/badge/%E7%AE%80%E4%BD%93%E4%B8%AD%E6%96%87-d0d7de?style=flat-square)](README.zh-CN.md)

> Non-normative source-pinned historical reader snapshot. The original remains authoritative and is not replaced as a machine input or executable gate. No automatic synchronization. Recorded test results, counts, implementation status and proof claims are source-scoped, not independently rerun or established as current guarantees here. The 2026-08-07 measurements and Rung 1 future instructions remain historical; this copy neither executes those instructions nor asserts that the old fan timing API is current. [Source](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/research/spike/substrate-observability/README.md)

Evidence for [ADR-0011](../../../../../adr/0011-execution-substrate-is-a-free-variable.md)
and [execution-inception.md](../../../../../roadmap/active/execution-inception.md) §1.

```
research/spike/substrate-observability/run-repro.sh [runs]
```

<a id="what-it-shows"></a>

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
[#1000](../../../../../roadmap/active/concurrency-stance.md) defines. Native is
the outlier.

<a id="notes-for-whoever-re-runs-this"></a>

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

<a id="when-rung-1-lands"></a>

## When Rung 1 lands

Arm-scoped output transactions (buffer per arm, flush in arm order at join)
make every substrate print `ABCD`. At that point the script's exit codes invert
in meaning: flip it into a gate, move it out of `research/spike/`, and delete
C-004's EXCEPTION clause in the same PR.
