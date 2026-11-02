<a id="logical-time-race-決定ゲート--結果記録"></a>

# Logical-time race decision gate — recorded results

[![English](https://img.shields.io/badge/English-24292f?style=flat-square)](GATE.md) [![日本語](https://img.shields.io/badge/%E6%97%A5%E6%9C%AC%E8%AA%9E-d0d7de?style=flat-square)](GATE.ja.md) [![简体中文](https://img.shields.io/badge/%E7%AE%80%E4%BD%93%E4%B8%AD%E6%96%87-d0d7de?style=flat-square)](GATE.zh-CN.md)

> Non-normative source-pinned historical reader snapshot. The original remains authoritative and is not replaced as a machine input or executable gate. No automatic synchronization. Recorded test results, counts, implementation status and proof claims are source-scoped, not independently rerun or established as current guarantees here. This is a reader copy of recorded gate results, not an active gate. The 74,898 bounded configurations are not a universal proof; distinguish the source’s Lean core theorem claims from testing implementation premises outside that core. The claim that divergence necessarily appears is retained as the source’s description of its example, not a general completeness guarantee. [Source](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/research/spike/logical-time-race/GATE.md)

Status: **PASSED — 74,898 configurations, 0 divergences (2026-08-01)**
Spec: [docs/roadmap/active/logical-time-proofs.md](../../../../../roadmap/active/logical-time-proofs.md)
Re-run: `research/spike/logical-time-race/run-gate.sh`

<a id="ゲートが問うこと"></a>

## What the gate asks

It exhaustively checks, within small scopes, that the race semantics of logical-time-async.md—

> The first decisive event (Complete | Trap) in lockstep, where every arm advances 1 fuel in 1 tick, determines the result. Cancellation, pruning, and delayed cap checks are optimizations and do not change observations.

—are **confluent with respect to scheduler choices**.

For each configuration (tuple of arm traces × budget), compare 4 parties:

| Implementation | Description |
|---|---|
| REF | The decisive-event rule in merge order (the reference semantics themselves) |
| SEQ | List-order sequential scan + shrinking cap + delayed trap decision + consumption reconstruction (the wasm strategy) |
| ADV | Enumerates **all physical schedules** using memoized DFS. In addition to pruning exactly at the cap, the adversary may choose **overrun** past the cap (a model of periodic fuel checks on actual machines) |
| Nested | Outer bounded streaming consumption agrees over the occurred streams of REF / SEQ |

The comparison covers not just outcome (winner / trap / exhausted) but also **consumed fuel** and the **occurred charge stream** (the merge-order consumption sequence observed by the outer region).

<a id="スコープ"></a>

## Scope

- Arms ≤ 3, charges per arm ≤ 2 (cost 1..2), termination ∈ {Complete, Trap, Diverge}, budget 0..=5
- Arms ≤ 2, charges ≤ 3, budget 0..=7 (a second scope with wider traces)
- Outer bounded: budget 0..=9 × additional charge {0, 1}

A total of 74,898 configurations × every ADV schedule (exhaustive enumeration with state memoization per configuration).

<a id="開発中にモデルが捕まえたもの"></a>

## What the model caught during development

- The reference semantics' occurred stream omitted the cost-1 tail of Diverge arms (immediately detected as a consumed divergence from SEQ—it was a model-side bug, but also demonstrates the claim that “divergence will necessarily appear”).

<a id="機械化との関係"></a>

## Relationship to mechanization

The core selection algebra (uniqueness of the decisive event, subset stability, preservation of the cap's visible window, and confluence) is kernel-checked as Lean theorems in [crates/almide-race-belt/](../../../../../../crates/almide-race-belt/) (0 sorry, CI `lean-proofs` job). This spike attempts exhaustive falsification outside that core: trace generation, consumption reconstruction, and nested streaming—the question of “whether the implementation satisfies the theorems' premises.” Fixtures falsify; theorems prove.
