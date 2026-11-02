<a id="logical-time-race-決定ゲート--結果記録"></a>

# Logical-time race 决策门禁——结果记录

[![English](https://img.shields.io/badge/English-d0d7de?style=flat-square)](GATE.md) [![日本語](https://img.shields.io/badge/%E6%97%A5%E6%9C%AC%E8%AA%9E-d0d7de?style=flat-square)](GATE.ja.md) [![简体中文](https://img.shields.io/badge/%E7%AE%80%E4%BD%93%E4%B8%AD%E6%96%87-24292f?style=flat-square)](GATE.zh-CN.md)

> 非规范、固定原文的历史读者快照。原文仍为权威来源，不替换机器输入或可执行 gate。不自动同步。记录的测试结果、数量、实现状态及证明主张仅限原文语境，并未在此独立重跑或确立为当前保证。 这是 gate 结果记录的读者副本，不是活动 gate。74,898 种有限配置不构成普遍证明；须区分原文对 Lean 核心定理的主张与核心之外实现前提的测试。“分歧必然显现”保留为原文对该例的描述，不作为一般完备性保证。 [Source](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/research/spike/logical-time-race/GATE.md)

状态：**PASSED — 74,898 种配置，0 分歧（2026-08-01）**
规范：[docs/roadmap/active/logical-time-proofs.md](../../../../../roadmap/active/logical-time-proofs.md)
重新运行：`research/spike/logical-time-race/run-gate.sh`

<a id="ゲートが問うこと"></a>

## 门禁检验什么

在小范围内穷举检查 logical-time-async.md 的 race 语义——

> 所有分支每 1 tick 前进 1 fuel 的 lockstep 中，第一个决定性事件（Complete | Trap）决定结果。取消、剪枝、延迟 cap 检查都是优化，不改变观察。

——**对调度器选择具有合流性（confluent）**。

针对每种配置（分支轨迹元组 × 预算），比较 4 者：

| 实现 | 内容 |
|---|---|
| REF | 按 merge 顺序的决定性事件规则（参考语义本身） |
| SEQ | 按列表顺序串行 scan + 缩小 cap + 延迟 trap 判定 + 消耗量重构（wasm 策略） |
| ADV | 用 memoized DFS 枚举**全部物理调度**。除恰在 cap 处剪枝外，跨越 cap 的 **overrun**（实际机器周期性 fuel 检查的模型）也是对手可选行为 |
| 嵌套 | REF / SEQ 的 occurred stream 上，外层 bounded 的流式消耗一致 |

比较对象不只是 outcome（获胜者 / trap / exhausted），还包括 **consumed fuel** 和 **occurred charge stream**（外层 region 观察到的按 merge 顺序排列的消耗序列）。

<a id="スコープ"></a>

## 范围

- 分支 ≤ 3，每分支 charge ≤ 2（成本 1..2），终结 ∈ {Complete, Trap, Diverge}，预算 0..=5
- 分支 ≤ 2，charge ≤ 3，预算 0..=7（扩展轨迹的第二范围）
- 外层 bounded：预算 0..=9 × 额外 charge {0, 1}

合计 74,898 种配置 × ADV 全部调度（每个配置使用状态 memo 的穷举）。

<a id="開発中にモデルが捕まえたもの"></a>

## 开发中模型捕获的问题

- 参考语义的 occurred stream 遗漏了 Diverge 分支的 cost-1 尾部（立即以相对 SEQ 的 consumed 分歧检测到——虽然是模型一侧的 bug，但也演示了“分歧必然显现”的主张）。

<a id="機械化との関係"></a>

## 与机械化证明的关系

选择代数的核心（决定性事件唯一性、子集稳定性、cap 可见窗口保持、合流）作为 [crates/almide-race-belt/](../../../../../../crates/almide-race-belt/) 中的 Lean 定理经过 kernel-check（0 sorry，CI `lean-proofs` 作业）。此 spike 对核心之外的部分——轨迹生成、消耗量重构、嵌套 streaming，也就是“实现是否满足定理前提”——尝试穷举反证。fixture 用于反证，定理用于证明。
