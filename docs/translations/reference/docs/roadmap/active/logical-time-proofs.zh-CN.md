原文 description 元数据：逻辑时间异步语义的证明台账：定理、Lean 核心、模型门禁
<a id="logical-time-async--the-proof-ledger"></a>

# 逻辑时间异步 — 证明台账

[![English](https://img.shields.io/badge/English-d0d7de?style=flat-square)](logical-time-proofs.md) [![日本語](https://img.shields.io/badge/%E6%97%A5%E6%9C%AC%E8%AA%9E-d0d7de?style=flat-square)](logical-time-proofs.ja.md) [![简体中文](https://img.shields.io/badge/%E7%AE%80%E4%BD%93%E4%B8%AD%E6%96%87-24292f?style=flat-square)](logical-time-proofs.zh-CN.md)

> 固定原文的非规范读者快照；原文仍为权威，不自动同步。未独立验证实现或执行引用指令。历史状态、计划及比较主张保留原范围。
> [Original source / 原文](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/active/logical-time-proofs.md)

> 原文注记：台账九项包含纸面/模型论证，报告 Lean kernel 检查的仅为指定的七个选择代数核心定理。74,898 配置是列出的有限集合之并，不是更大笛卡尔积或全部程序。W1/W2 及原文 charge 模型是前提，语义一致不等于 renderer 符合性。未重跑历史计数及 proof gate。T9 把墙钟选择归到 oracle 层，并不永久禁止 timeout 语法或证明 runtime。此快照 Duration min 仍仅为设计项，与已实现 timeout body 不同。


> 宪章：[async-inception.md](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/active/async-inception.md)。本文是其证明台账。
>
> 证明 [logical-time-async.md](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/active/logical-time-async.md) 的语义能够构成异步功能的台账。证据分3层：
>
> 1. **纸面定理**（本文）— 形式模型上的主张与证明
> 2. **Lean kernel-check** — 选择代数核心7个定理，[crates/almide-race-belt/](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/crates/almide-race-belt)（0 sorry、CI `lean-proofs`）
> 3. **穷举模型检查** — 74,898种配置 × 全部调度，[research/spike/logical-time-race/](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/research/spike/logical-time-race)（`run-gate.sh`，0偏离）
>
> 本文还记录证明工作迫使设计做出的 **3项纠正** — 证明不是仪式的证据。

<a id="何を証明し何を証明しないか"></a>

## 证明什么，不证明什么

- **证明**：语义自身的性质 — 选择唯一性、调度无关性（合流）、剪枝和延迟 fuel 检查的不可观察性、消耗确定性、有界片段有限可观察性，以及“按环境时间选胜者的语法不能存在于确定层”这一边界的必然性。
- **不证明**：实现（两个 renderer）符合语义。这属于 charge-trace validator + `spec/wasm_cross` fixture，按 [proven-vs-trusted](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/contracts/proven-vs-trusted.md) 的做法置于 **trusted 侧**。这里的定理固定“语义一致”，实现门禁尝试反驳“实现映射了该语义”。两者角色不交叉。
- **明确前提**（well-formedness，CM-1 实现时的接受条件）：
  - **W1**：CFG 全部环和递归调用都经过 ≥1 个 charge site。
  - **W2**：相邻 charge site 之间的工作有限（若在 basic block 入口 charge，可由 block 长度约束）。

<a id="形式モデル"></a>

## 形式模型

- **分支** = 确定性 charge trace：成本序列 `c₁, c₂, …` 和终止（`Complete` | `Trap` | 发散）。在纯片段（Rung 0）中是（程序，输入）的函数。
- **预算 n 下的分支结局** `end_j`（check-then-charge：剩余 < c 时 Exhaust）：`Complete(t)` | `Trap(t)` | `Exhaust(t)`，`t` = 累计消耗。
- **事件** = `(time, idx)`（累计消耗，分支编号）。**merge 顺序** `≺` 为 (time, idx) 字典序 — 无需调度器表述“每 tick 1 fuel，同刻按源码顺序”的 lockstep。
- **决定性事件** = Complete / Trap 终止中 merge 顺序最小者。Complete 则为胜者，Trap 则程序 trap，不存在则 `Err(exhausted)`。
- **occurred stream** = 在 ≺ 下先于决定性事件的 charge 事件（+ 决定分支自身的 charge）。外层 region 按 merge 顺序将此序列观察为消耗。

Rust 模型（spike）逐字实现这一定义；Lean 直接形式化事件及 `≺` 的代数。

<a id="定理台帳"></a>

## 定理台账

<a id="t1--枝の決定性と全域性紙"></a>

### T1 — 分支确定性与全域性（纸面）

**主张**：在 W1、W2 下，预算 n 的分支执行在最多 n 次 charge 和有限工作内必然到达 `Complete(t) | Trap(t) | Exhaust(t)`（t ≤ n），结果是（trace，n）的函数。

**证明**：纯片段 small-step 是确定的。由 W1，无限执行会无限次通过 charge site，但第 n+1 次通过不可能（剩余额度耗尽）。由 W2，site 间工作有限。∎

这就是 `fan.bounded` 的全域性，补齐 deterministic-bounds.md 有条件主张“有界片段中成立 2-safety 框架”的条件（T8）。

<a id="t2--lockstep--least-spend紙--lean-decisive_subset"></a>

### T2 — lockstep ≡ least-spend（纸面 + Lean `decisive_subset`）

**主张**：决定性事件为 Complete 时，该分支在完成分支中按 (spend, index) 字典序最小。

**证明**：完成事件的 time 等于该分支 spend。merge 顺序为 (time, idx) 字典序，因此若全体候选的最小值是 Complete，它在 Complete 子集中也最小（Lean：将 `decisive_subset` 应用于完成子集）。∎

<a id="t3--選択の一意性lean-decisive_unique"></a>

### T3 — 选择唯一性（Lean `decisive_unique`）

**主张**：候选分支编号互异时（分支至多终止一次），决定性事件唯一。

双向 `¬prec` 推出 time 和 idx 相等（omega）。“谁赢了”在代数上不可能有2个答案。∎（kernel-checked）

<a id="t4--cap-の許容性lean-cap_admits_decisive--cap_admits_window"></a>

### T4 — cap 的可容许性（Lean `cap_admits_decisive` / `cap_admits_window`）

**主张**：剪枝 cap（对已记录候选 d'，可将分支 k 截在 `d'.time − (k > d'.idx ? 1 : 0)`）**无论由哪个候选计算**（顺序扫描暂时 best，或并行调度恰好先记录的候选），都 (a) 允许真正决定分支运行到其时刻，(b) 保留在 ≺ 下先于决定性事件的全部事件 — 包括 trap 可见窗口。

∎（kernel-checked）。推论：**全部运行至静止的调度都会记录真正决定性事件**（决定分支不能因 cap 而 park，也不能在终止前 Exhaust）。

<a id="t5--合流lean-decide_stable--モデル-adv"></a>

### T5 — 合流（Lean `decide_stable` + 模型 ADV）

**主张**：实现若 (i) 记录是真正候选集合的子集、(ii) 包含真正决定性事件、(iii) 返回记录中 merge 顺序最小者，就**必然**返回真正决定性事件。顺序扫描、并行执行、恰在 cap 剪枝、跨越 cap 的延迟 fuel 检查（overrun）都归约为 (i)–(iii)。

Lean 证明 (i)–(iii) ⇒ 一致（`decide_stable`）；模型通过穷举74,898种配置 × 全部调度，尝试反驳“实现满足 (i)–(iii)”，未发现反例。不仅胜者，**consumed fuel 和 occurred stream** 也一致（兼作 T6 检查）。

<a id="t6--消費量の決定性と再構成可能性紙--モデル"></a>

### T6 — 消耗确定性与可重建性（纸面 + 模型）

**主张**：occurred stream（因此 race 对外层施加的消耗）是 trace 的函数，仅用 cap 执行揭示的数据便可精确重建。

**证明**：stream 定义是相对决定性事件（T3 保证唯一）的 ≺ filter，因此是 trace 的函数。重建：cap 执行揭示 time ≤ cap 的全部事件，由 T4(b)，cap 覆盖窗口全部事件。额外揭示的事件（overrun 部分）被 ≺ filter 丢弃。∎ 模型还比较 stream 作为序列的一致性。

<a id="t7--入れ子の決定性紙--モデル"></a>

### T7 — 嵌套确定性（纸面 + 模型）

**主张**：外层 region 按 merge 顺序消费 occurred stream，中途余额耗尽则当时外层 Exhausted（放弃 race）。该裁定是（trace，外层剩余量）的函数，按 region 嵌套深度归纳可得整体确定。

**证明**：T6 保证 stream 确定，与 prefix-sum 比较是算术。若决定性事件为 Trap，程序 trap，消耗不可观察。∎

<a id="t8--有界断片の有限観測性紙"></a>

### T8 — 有界片段有限可观察性（纸面）

由 T1，有界片段所有执行在有限时间内到达观察。因此在此片段上，native ⇄ wasm 可观察等价可被有限 trace 对反驳 — 这确立了 deterministic-bounds.md 以“片段外不要称为 2-safety”所限制主张的片段内部。

<a id="t9--決定層の不可能性定理紙"></a>

### T9 — 确定层的不可能性定理（纸面）

**主张**：“按环境墙钟选择胜者”的语法不能存在于确定层。

**证明**：确定层定义是“观察是（程序，输入）的函数”。墙钟顺序是 host 速度的函数，不是（程序，输入）的函数。只要胜者可观察（分支返回可区分值），墙钟选择就与定义矛盾。∎

推论：I/O race、timeout **必然**属于 oracle 层（将环境作为已声明输入 ω，以 R_Ω 关联的契约类别）。这不是设计偏好，而是边界定理。最初将 `fan.timeout` 推迟至 Stage 4 是实现顺序判断，此推论并不证明永久禁止 timeout 语法。当前 body 形式及 clock 区分层的方式见 [SPEC §13](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/SPEC.md#13-concurrency-fan) 和 [ADR-0028](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/adr/0028-fan-selection-names-and-cost-contract.md)。

<a id="証明作業が設計に強いた訂正3-件--モデル自身の-1-件"></a>

## 证明工作迫使设计做出的纠正（3项 + 模型自身1项）

1. **无胜者时 trap 规则未定义。** 初稿可见窗口只说“胜者确定 tick 之前”，未定义无胜者情况。统一到决定性事件规则（Complete / Trap 中 merge 顺序最小者是唯一裁定者）后消除。
2. **消耗公式 Σ min(end_j, s\*) 在边界高估。** index 在胜者之后的分支，其 time = s\* 的 charge 在 merge 顺序上位于胜者完成之后，不会发生。正确的是 occurred stream（merge-prefix）之和。
3. **嵌套是 streaming，不是“race site 原子 charge”。** 外层按 merge 顺序观察 race 内部 charge，中途耗尽就放弃 race。原子 charge 无法定义“race 内部外层耗尽”时的裁定点。
4. 模型自身1个 bug：参考侧 occurred stream 遗漏发散分支的 cost-1 尾部，立即以与 SEQ 的 consumed 偏离被检测并修复（也演示了偏离确实能到达检测面）。

logical-time-async.md 已带纠正标记反映1–3。

<a id="非同期機能としての十全性adequacy-audit"></a>

## 作为异步功能的充分性（adequacy audit）

| 异步模式 | 与当前表面的对应 | 依据 / 范围 |
|---|---|---|
| 并发 fan-out / join（`Promise.all`） | `fan { }` / `fan.map`（可并行执行，按列表顺序观察） | C-004 / C-199 / C-200 |
| 全部收集（`allSettled`） | `fan.settle` | 已有 |
| fallback 链 | `??` / `list.find_map`（旧 `fan.any` 已删除） | ADR-0028 |
| 推测 / 组合计算 | `fan.min(compute.ms(n))`（必须 clock） | T2–T5 选择模型，实现符合性另论 |
| 计算量上限 / 全域化 | `fan.bounded(compute.ms(n))` | T1 / T8 模型范围 |
| I/O race（先响应者） | `fan.min(duration.ms(n))` 仅设计决定（#3788，未实现） | T9 边界；详见 ADR-0028 / SPEC |
| timeout | `fan.timeout(duration.ms(n)) { body }` | oracle 层、body 形式；T9 不是实现证明 |
| streaming / backpressure | `Flow[T]`（flow-design.md）范围 | 明确范围外 |
| fire-and-forget | 永久否决（rush / spawn — fan-v2 否决表） | 立场 |

诚实总结：异步核心用途（并发 I/O fan-out、全部收集、fallback）可整理在确定层。选择性 I/O race、timeout 属于 oracle 层契约；timeout body 已实现，但 Duration min 尚未实现（#3788）。区分层的决定和实现状态。T9 讨论此层区分，并非证明当前 runtime 的全部功能。

<a id="小スコープの但し書き"></a>

## 小范围说明

模型检查运行范围是 [GATE.md](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/research/spike/logical-time-race/GATE.md) 记录的两集合之并：

- 分支 ≤ 3、每分支 charge ≤ 2（成本1..2）、预算0..=5
- 分支 ≤ 2、charge ≤ 3、预算0..=7
- 嵌套外层 bounded 的预算0..=9 × 额外 charge {0, 1}

并未检查分支 ≤ 3 / charge ≤ 3 / 预算 ≤ 7 的完整笛卡尔积。Lean 与规模无关的定理覆盖的是选择代数（事件与 ≺ 的理论）；“trace → 候选集合”的连接（候选 idx 互异、记录 ⊆ 真值、记录决定事件）由模型逐配置检查。扩大范围后可用一次 `run-gate.sh` 重跑。

<a id="反証条件"></a>

## 反驳条件

- CM-1 实现要求无法满足 W1/W2 的布局（如某优化需要无 charge 的环）→ T1 不成立，撤回全域性主张。
- renderer 的 charge-trace 保持被破坏 → 定理群不受影响，但 validator / fixture 检测到**实现越出语义**（这就是门禁的工作）。
- 范围外反例 → 增大 `run-gate.sh` 范围常量，复现并添加，就会纠正此台账相应定理。按上节风格保持纠正可见。
