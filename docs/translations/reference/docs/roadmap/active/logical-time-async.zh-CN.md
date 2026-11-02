历史描述：async 文法，以 fuel 为逻辑时钟、确定性 race、oracle 层。
<a id="logical-time-async--the-async-grammar-design"></a>

# 逻辑时间 Async — async 文法设计

[![English](https://img.shields.io/badge/English-d0d7de?style=flat-square)](logical-time-async.md) [![日本語](https://img.shields.io/badge/%E6%97%A5%E6%9C%AC%E8%AA%9E-d0d7de?style=flat-square)](logical-time-async.ja.md) [![简体中文](https://img.shields.io/badge/%E7%AE%80%E4%BD%93%E4%B8%AD%E6%96%87-24292f?style=flat-square)](logical-time-async.zh-CN.md)

> 非规范性固定原文历史设计读者版。固定原文为正本且不变；不自动同步、未新增实现或独立重跑benchmark/证明。原race/thunk提案、后续block形式、已失效预算可选化及当前fan.min/timeout引用分层保留；当前契约以原文指定的SPEC/ADR为准。新颖性、唯一性、确定性及全局计量开销为零是原设计主张，并非此处独立确立的保证。纸面/Lean/模型检查描述不证明整个当前运行时、Duration oracle选择，或跨meter版本和lowering变化的不变性。阶段计划、pure限制、stack排除及oracle边界原样保留。保留代码草图是历史材料，不是执行授权。 [Source](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/active/logical-time-async.md)

> 原设计章程：[async-inception.md](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/active/async-inception.md)。本文记录逻辑时间的设计理由及更正历史。当前表面和 clock 契约的正本是 [SPEC §13](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/SPEC.md#13-concurrency-fan) 和 [ADR-0028](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/adr/0028-fan-selection-names-and-cost-contract.md)。下文的 `race` 指原设计的逻辑成本选择模型，并不证明 Duration 的 oracle 选择或当前整个执行系统。
>
> 回答 [deterministic-bounds.md](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/active/deterministic-bounds.md) 提出的四个问题（预算分配、胜者选择、取消、效果隔离）。仅修改 [concurrency-stance.md](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/active/concurrency-stance.md)（#1000）的一项结论。这是设计文档，实现按 staging 节的顺序开展。
>
> **证明台账**：[logical-time-proofs.md](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/active/logical-time-proofs.md) 从三层（纸面定理 / Lean kernel-check / 穷举模型检查）固定本语义的成立。证明工作带来的三项更正（下方各更正标记）也记录在那里。

<a id="設計テーゼ"></a>

## 设计论点

**墙钟不决定观察。语言的时间基底是逻辑时间（fuel）。环境时间仅作为已声明的 oracle 输入进入。**

一句话概括：

> 让 Verse 的 `race` 在 EVM gas 时间上运行，将失败竞争的分支置于观察窗口之外，并使之在 native ⇄ wasm 观察等价契约内成立。

所有组件都有先例 — fuel 确定性中断（Wasmtime fuel / EVM gas）、逻辑时间和声明顺序决胜（Verse / Lingua Franca / Esterel）、通过限制获得确定性（Par monad / LVars）。缺少的是组合：**尚未找到一种语言，在跨目标观察等价契约中定义以 fuel 为度量的 race，同时包括不观察未获胜分支和嵌套预算**。因为 `fan` 是编译器已知的语法而非库，编译器可以掌握预算分配、胜者选择、效果可见性三者。这就是设计的全部依据，也是作为 2026 年语言规格提出的主张。

<a id="表面--追加は-2-つキーワードはゼロ"></a>

## 表面 — 增加两项，零个关键字

> **形式修订**：表面形式（接收 thunk 参数的函数形式）已被 [fan-v2.md](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/active/fan-v2.md) 的 block form — `fan.bounded(compute.ms(100)) { body }` / `fan.race { arm; arm }` — 取代。本文的语义（fuel、lockstep、trap 窗口、嵌套、CM-1）全部仍然适用。
>
> **补记（2026-08-01，后来被取代）**：当时将 race 预算改为**可选** — 选择不需要预算，预算仅用于防止发散（n = ∞ 的特化；终止性的处理与 fan {} 现有规格相同）。审计见 [ticks-interface-audit.md](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/active/ticks-interface-audit.md)。当前 `fan.min` 根据 ADR-0028 必须有 clock，这项可选化不是当前 API。

```almide
fan.bounded(compute.ms(100)) { body }   // Result[T, String] — n fuel 以内に完了すれば Ok
fan.race(compute.ms(500)) { arm; arm }     // Result[T, String] — 各枝に n fuel、最小消費の成功が勝つ
```

原设计的 oracle 层提案（现在是 `fan.timeout(duration.ms(n)) { body }`。以下 thunk 形式和 tombstone 状态是历史草图）：

```almide
fan.timeout(ms, thunk)        // 環境相対。R_Ω 契約クラス。それまで tombstone 維持
```

<a id="fan-族は選択メトリクスの代数になる"></a>

### fan 家族成为“选择度量的代数”

| 语法 | 选择规则 | 确定性依据 |
|---|---|---|
| `fan { }` / `fan.map` | 全部（列表顺序 join-all，第一个 Err） | C-004 / C-199 |
| `fan.any` | **index 最小**的成功 | 按列表顺序扫描 |
| `fan.race(d)` | **(spend, index) 字典序最小**的成功 | fuel + 源码顺序决胜 |
| `fan.settle` | 不选择（全部收集） | 列表顺序 |

`any` 最小化 index，`race` 最小化 fuel，再按 index 打破平局。家族只有“最小化什么”这一个轴，以整个表面闭合，而不是逐点添加。

**完备性规则**（按可纳入 matrix gate 的形式陈述）：fuel 维度通过与 `fan.bounded` 组合到达所有组合器（例如 `fan.map(xs, (x) => fan.bounded(compute.ms(100)) { f(x) })`）。内建集成 fuel 的**只有** `fan.race`，因为它的选择规则**使用**该度量（比较分支间的 spend）。用户组合无法表达分支间比较。不添加 `bounded × {map, any, settle}` 专用形式是有意省略，连同这条规则一起纳入门槛。

<a id="なぜ予算に単位を必須にするか"></a>

### 为什么预算必须带单位

0.29.0 的教训是，没有人会把 `fan.timeout(1000)` 中的 1000 理解为毫秒以外的东西。`fan.race(1000, ts)` 仍有同样风险。要求标签后，每个调用处都会出现单位名，用类型消除裸整数造成的单位误读（最终形式见 [ADR-0001](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/adr/0001-deterministic-time-units.md)）。

**更正（2026-08-01）**：初稿以 `fan.map(xs, limit: 16, f)` 为标签参数先例，这是**错误的**。`limit:` 未实现（checker 固定检查 fan.map 为两个参数），语言没有标签参数语法 — fan-concurrency-next.md 的 status 表（✅）已过时。标签来自 [fan-v2.md](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/active/fan-v2.md) 的 block 文法：fan head 是语法而非函数调用，可直接接收带单位的字面量，不引入通用标签参数机制。

<a id="なぜ-fanrace-の名を再利用するか"></a>

### 为什么重新使用 `fan.race` 这个名称

0.42.0 移除它的理由是“确定性模型下，无法给 race 这个名称赋予意义”。**没有度量时，这仍然成立。** fuel 提供度量后，race 才首次有确定性意义。不改名有两个理由：

1. **LLM 的先验分布。** 模型想写并发的最快选择时，一定会写 `fan.race`。新名（如 `fan.least`）是在对抗先验 — 与 MSR 方向相反。
2. **签名拒绝旧形式。** 旧 `fan.race(thunks)` 是一个参数，新形式是两个参数，包括带单位预算。E027 tombstone 从“已移除”改为“签名迁移提示”（添加带单位预算；语义是选择消耗最小的胜者）。也符合 tombstone 迁移目的地必须是有效表面的规则。

<a id="意味論"></a>

## 语义

<a id="論理時計--charge-event-と-cm-1"></a>

### 逻辑时钟 — charge event 与 CM-1

- **charge site** 是共享 MIR 的 basic block 入口。静态成本是块内语义 op 数。可变成本 intrinsic（list 拼接、string 拼接、大小 n 的构造等）在 site 内增加**动态 charge**（`1 + サイズ比例項`，所保留日文字面量意为 1 加上与大小成比例的项）。
- **charge 在优化前、MIR 诞生时赋予，之后所有 pass 保留注解**。renderer 侧 peephole 即使融合 op，charge 仍合计保留。由此：
  - 注解保留是本设计的要求，不是整个实现优化不变性的证明。当前 cost 契约限定于 meter version，inlining/lowering 改变可能改变成本或胜者（ADR-0028）。
  - RC 增减、MakeUnique、move、ANF 管理代码**免费**（它们是优化产物，不是源码语义成本）。与 EVM gas 不依赖 JIT 速度而定义是同样结构。
- **成本表作为带版本的语义对象**（`CM-1`）进入契约台账。常数变化属于 semantic change，可能改变哪个分支获胜、什么会 exhaust。因此版本化不是装饰。v1 常数草案：所有 op 权重为 1，call 为 1 + callee 本体，可变成本 intrinsic 为 `1 + ⌈size/16⌉`。**常数在 Stage 2 实现 + fixture 的同一个 PR 中确定**。这里只固定形式（大小线性，常数由台账管理）。
- **算术**：预算为 `Int`，内部计数器使用饱和运算。对 charge c，`remaining < c` 时 exhaustion。`n <= 0` 在首次 charge 时 exhaust（无验证分支的全域语义）。

<a id="中断点の統一原理"></a>

### 中断点的统一原则

**charge site 是语言唯一的中断点。** site 之间执行是原子的。fuel 语法在 site 读取计数器，未来 oracle 语法（`fan.timeout`）在同一 site 读取环境。确定层与 oracle 层的差别仅在于“中断点读取的是程序的函数（fuel），还是环境输入（时钟）”。两层共享中断点位置，且两个目标上的位置相同。

<a id="fanboundedcomputems100--body-"></a>

### `fan.bounded(compute.ms(100)) { body }`

- thunk 是 **pure**（效果约束：不得包含 effect fn 调用、oracle 效果）。语法本身仍按现有 fan 规则仅可用于 effect fn 内（维持单一规则）。
- 含义：thunk 消耗 fuel `s`，若 `s <= n`，则 `Ok(値)`，否则是带台账定义消息的 `Err`。所保留日文“値”指返回值。thunk 返回 Result 时遵循现有 auto-wrap 契约（与 race/any/settle 相同）。
- **trap 传播**。bounded 不是投机。div-by-zero 仍如常终止程序（与 C-200 理念一致）。
- 此 bounded/选择接口的 exhaustion 采用 String 错误契约。这并不意味着整个语言的 Result 错误类型都限于 String。消息在相应契约和 fixture 范围内检查。

<a id="fanracecomputems500--arm-arm---lockstep-定義"></a>

### `fan.race(compute.ms(500)) { arm; arm }` — lockstep 定义

**定义（lockstep）**：所有分支每个逻辑 tick 各前进 1 fuel。在 tick `s` 首次完成（返回成功值）的分支获胜。同一 tick 内的事件按源码顺序（列表 index 顺序）解决。race 最多运行 `n` 个 tick。

**刻画（实现采用的形式）**：令各分支 j 的终止为 `end_j = min(spend_j, trap_j, n)`（完成 / trap / 分支预算耗尽中的最早者），则：

- **胜者** = 已完成分支中 `(spend_j, j)` 的字典序最小者。不存在则返回台账定义的 `Err`
- 值为 `Err` 的分支及 exhaust 的分支不参与候选（与 `any` 跳过失败对称。race 是“最低成本的**成功**”）

lockstep 中的完成 tick 等于该分支的 spend，所以两个定义一致。lockstep 是给用户解释的图景（与 Verse 一样，“同时则源码顺序”），least-spend 是去掉调度器的计算式，**这种一致正是“没有物理时间的 race”的内容**。

**trap 可见窗口 — 决定性事件规则**（*更正 2026-08-01，由证明工作产生*：初稿窗口规则仅定义有胜者的情况，无胜者 + trap 未定义。统一为以下单一规则）：

> 将 Complete / Trap 终止事件按 merge 顺序（(累计 fuel, 分支 index) 字典序）排列，**第一个决定性事件是唯一裁决者**。Complete 则该分支获胜，Trap 则程序因该 trap 终止，无决定性事件则为 `Err(exhausted)`。

初稿窗口规则（`t_j < s*`，或 `t_j = s*` 且 `j <` 胜者 index）是有胜者情形下的推论。窗口外的 trap 视为从未发生（分支在决定性事件的 tick 消失）。投机不会悄悄吞掉 bug（窗口内一定终止），同时胜者确定后，未获胜分支连其存在都消失。与 C-200（fan{} 的 sibling trap 传播）的差别不在语法，而是**所购买保证不同：join-all 需要全部，race 只需其中一个**。这一差别直接成为 trap 规则的差别。

**取消是优化。** 观察对象只有“采用哪个分支的值”“trap 窗口”“（嵌套时的）fuel 消耗”。native 何时停止未获胜线程是自由的。健全剪枝：对当前最优 `(s*, i*)`，分支 `j < i*` 只需运行至预算 `min(n, s*)`，分支 `j > i*` 只需运行至 `min(n, s* - 1)`（平局由源码顺序较前者获胜）。该 cap 也恰好覆盖 trap 可见窗口 — cap 内触及的 trap 可见，cap 外不可见。剪枝不改变语义由构造得出。

<a id="予算の入れ子--evm-式-min-cap"></a>

### 预算嵌套 — EVM 式 min-cap

嵌套 `fan.bounded` / `fan.race` 的有效预算为 `min(自身の n, 外側の remaining)`（保留的日文字面量意为自身 n 与外层剩余预算的最小值）。内层 charge 同时从所有外层预算扣除（因为是同一次执行）。内层在来自外层的 cap 下 exhaust 时，外层以 remaining ~0 继续，并在下一次 charge 时 exhaust。不需要带 region 标签的传播机制 — 一切都是共享 charge trace 上的算术，两目标一致由 trace 一致推出。与 EVM 子调用 gas 上限（EIP-150）结构相同。

**race 向外层收取的消耗**（*更正 2026-08-01，由证明工作产生*：初稿的 `Σ_j min(end_j, s*)` 在边界高估 — 胜者之后 index 的分支在 time = s\* 的 charge，在 merge 顺序上晚于胜者完成，因此不会发生。此外，“在 race site 原子 charge”无法定义 race 内部耗尽外层预算时的裁决点）：

> 外层 region 将 race 的 **occurred stream** — merge 顺序中早于决定性事件的 charge 事件序列（+ 胜者自身的 charge）— **按 merge 顺序原样流式**观察为消耗。若途中外层余量耗尽，就在该点外层 Exhausted（放弃 race）。race 完成时，总消耗为该 stream 的总和。

这是各分支 trace 的函数，即使剪枝实现也能仅凭 cap 执行揭示的数据准确重建（cap 覆盖可见窗口全部事件 — Lean `cap_admits_window`）。**从计数器扣除的是语义消耗，而非实现实际付出的工作** — 与 RC 免费相同，应用“语义成本”原则。

<a id="効果隔離の梯子"></a>

### 效果隔离阶梯

- **Rung 0（v1）**：race / bounded 分支为 pure。可直接用现有 purity 机制检查。典型用途：算法组合（精确解 vs 启发式、用两种文法解析、SAT 式组合）。native 上带剪枝的并行也会改善墙钟时间。
- **Rung 1（后续）**：允许仅有 stdout/stderr 的分支，**决出胜者后只刷新胜者输出**（丢弃未获胜分支）。与 #1026 中搁置的 arm 输出缓冲是同一机制，与退役 C-004 的 EXCEPTION 节共享组件。
- **永久不允许**：在 race 分支写 oracle 效果（fs / http / process / random / env）是编译错误。**I/O race 的胜者由环境选择**，原则上不能进入确定层。诊断写道：“racing I/O is environment-dependent — restructure, or use the oracle-tier fan.timeout (Stage 4)”（I/O 竞争依赖环境，应重构或使用 oracle 层 fan.timeout）。

凭借 Almide 的效果纪律（var 捕获 E008、mut 参数 E007、单一错误通道），需隔离的效果一开始就分为 {pure | 输出 | oracle}。Verse 尝试用 transactional effect 解决的问题，大部分在这里**由类型使其一开始就不存在**。

<a id="deterministic-bounds-の-4-設問への回答"></a>

## 对 deterministic-bounds 四个问题的回答

| 问题 | 回答 | 决定因素 |
|---|---|---|
| 1. 预算分配 | **per-branch，每个分支 n**（不是共享计数器的确定性分割） | 分割会让新增分支改变已有分支的预算。LLM 一加第三个分支，第一个就开始 exhaust — 直接破坏 modification survival。per-branch 将分支语义局部化 |
| 2. 胜者选择 | **lockstep ≡ (spend, index) 字典序最小** | 调度器从定义消失。同时满足 Verse UX（同时则源码顺序）与 EVM 确定性的唯一形式 |
| 3. 取消 | **优化。只观察采用值、trap 窗口、消耗量** | 剪枝 cap（`s*` / `s* - 1`）与 trap 窗口一致，故剪枝不改变语义 |
| 4. 效果隔离 | **v1 pure，Rung 1 输出 transactional，oracle 永久拒绝** | 现有效果纪律通过类型完成大部分隔离 |

<a id="型付け診断表面変更"></a>

## 类型、诊断与表面变化

- `fan.race(compute.ms(500)) { arm; arm } -> Result[T, String]`、`fan.bounded(compute.ms(100)) { body } ->
  Result[T, String]`。thunk 的 auto-wrap 遵循 race/any/settle 现有契约。
- 新诊断：(a) race/bounded 分支中有效果调用 → “branches must be pure”（分支必须纯）+ 上述 oracle 提示；(b) 裸整数 → 单位提示；(c) 空 thunk 列表 → 与旧 race 一样编译错误。
- **E027 修订**：从“removed”变为“signature migration”，迁移目的地成为有效表面。`fan.timeout` tombstone 的提示在 Stage 4 前维持现状（引导到 host boundary）。
- unknown-fan-fn 的 Available 列表增加 `fan.race` / `fan.bounded`（在实现 PR 中）。

<a id="クロスターゲット-lowering"></a>

## 跨目标 lowering

- **仅在 bounded region 内计量**。将 region 可达调用图特化为 fuel 变体（与 mono 同一机制的额外维度）。全局计量开销为零。明确将 closure table 的 metered 槽处理留为实现时未决点（设计答案是“可能流入 region 的 closure 有 metered 变体”，成本与 race 目标代码大小成比例）。
- **wasm（顺序）**：按列表顺序 scan。更新 best `(s*, i*)`，给各分支加 cap 运行。途中 trap **记录**为 `(t_j, j)` **后继续扫描**，所有分支 end 确定后判定可见窗口并重新抛出（后续分支可能产生更小的 `s*`，使其落到窗口外，因此不可立即终止）。
- **native（并行）**：每分支一个线程，trap 用 checked lowering 捕获、记录（物理上未获胜分支可能在胜者确定前碰到 trap，故延迟至选择后判定窗口）。cap 随 best 更新缩小。
- 两者计算相同观察（采用值、trap 窗口、消耗量），这就是 fixture 比较对象。

<a id="契約とゲート"></a>

## 契约与门槛

1. 将 **CM-1** 作为带版本对象加入台账。成本常数改变 = semantic change。
2. 新契约（编号在实现 PR 分配）：bounded exhaustion 等价、race 胜者等价、trap 可见窗口、嵌套 min-cap 与 race 消耗。每项契约带 `spec/wasm_cross` fixture，比较 `result` + `consumed_fuel` + **charge-event trace** 三项（仅总量一致会漏掉顺序差异，正如 deterministic-bounds 所指出）。
3. **charge-trace 保留 validator**：检查所有 charge event 通过 lowering 恰好一次到达 emit，是与 ownership certificate 同类的对象。fixture 是反证器，validator 是证明形式的证据。
4. 生成 MIR 上的 property test（随机 MIR 的 trace 保留）。

<a id="既知の境界隠さず書く"></a>

## 已知边界（不隐藏）

- **栈**：fuel 通过 charge（调用也 charge）抑制递归深度，但巨大预算下，native / wasm 栈上限差异可能先于 fuel 耗尽出现。这是语言整体已有的边界，本设计既不改善也不恶化。明确列在契约观察范围之外。
- **不公开消耗 fuel**：不将 `spend` 作为值返回（否则 CM 常数修改会改变所有程序的值）。成本模型影响观察的途径仅限“谁获胜”“是否 exhaust”，fixture 通过 emit 时 Σ-probe（现有 evidence class）做内部比较。
- **单一错误通道**：识别 exhaustion 依赖消息常数。丰富错误类型是整个语言的问题，不逐点解决。

<a id="系譜との対応--何を借り何が新しいか"></a>

## 与前例的对应 — 借用什么，什么是新的

| 谱系 | 借用内容 | 不借用 / 差异 |
|---|---|---|
| Verse `race` | 胜者选择 UX、“同时则源码顺序”、结构化取消 | simulation time 源于墙钟，没有计算量确定性。transactional effect 在 Rung 1 有限采用。依据 Epic 文档描述（实现时重新确认平局规则） |
| EVM gas / EIP-150 | 确定性 out-of-gas、嵌套 min-cap、成本表 = 语义 | EVM 没有 race（只有顺序 transaction） |
| Wasmtime fuel | fuel vs epoch 区分（确定 / 墙钟两层） | CM-1 计量点及成本是独自的带版本契约。仅凭此比较不能证明整个实现优化不变性或优于其他方式 |
| Lingua Franca / Esterel | 逻辑时间、同一逻辑时刻按声明顺序解决 | 逻辑时间单位是事件而非计算量 |
| Par monad / LVars | 通过限制获得确定性，将 quasi-determinism 命名为诚实的退路 | 不处理 fuel、选择、取消 |
| Kahn process networks | （未来）确定性通道的理论基础 | 本设计范围外，见下方“未来视野” |

**新的是组合**：以 fuel 为度量的 race + 未获胜分支观察窗口 + 嵌套预算，与双 backend 观察等价契约及其验证装置（trace validator、三项 fixture）整体定义。

<a id="msr--この文法がllm-が最も正確に書けるに効く形"></a>

## MSR — 此文法如何服务“LLM 最准确地编写”

- **只增加一个决策点**：“这是投机吗？”投机用 race，顺序上限用 bounded。仍不存在 async/await/Future/task handle。零个新关键字。
- **per-branch 预算**为编辑生存率而设。添加、删除、重排分支不改变其他分支预算。源码顺序决胜以 diff 可见的方式固定胜负。
- **带单位字面量**通过类型消除裸整数单位误读这一类问题。
- **每次运行相同**：包含 race 的测试不会变得 flaky。并发语法可直接用于 benchmark、snapshot、3-way oracle。stance 中“确定性是正确性定义的一部分，而非性能特性”的措辞原样扩展到 race。

<a id="concurrency-stancemd-の改訂点1-点のみ"></a>

## concurrency-stance.md 的修订点（仅一项）

stance 中“确定性模型下无法给 race 这个名称赋予意义”，**在没有度量时仍然成立**。当且仅当 fuel 提供度量时，才能定义与列表顺序求值观察严格相同的 race（上方 scan 即该顺序求值）。stance 原则“无法赋予列表顺序意义的东西不纳入语言”得到**保留** — 本设计是其应用，不是例外。

<a id="将来の地平本設計の範囲外方向だけ記す"></a>

## 未来视野（本设计范围外，仅记方向）

拥有逻辑时钟的语言可以在其上定义**确定性并发**（而非并行）：逻辑 sleep、tick 顺序的确定性 select、charge 点 round-robin 的确定性交错、Kahn network 式确定性通道。如果有一天重访 structured concurrency，基础就是此时钟。这首次将满足 stance 所说“可重访，但 contract 先行”的路径具体化。

<a id="ステージングdeterministic-bounds-の-5-段に対応"></a>

## 分阶段实施（对应 deterministic-bounds 五阶段）

1. **CM-1 + charge-trace 保留**：给 MIR 赋予 charge，两个 renderer 保留 trace，validator、三项比较 fixture。差异达到 1 单位或 1 位置即停止程序。
2. **`fan.bounded`**：顺序 pure，CM-1 常数确定和台账登记放在同一 PR。
3. **`fan.race`**：lockstep 语义、剪枝、trap 窗口、嵌套消耗。修订 E027。
4. **oracle 层**：R_Ω 契约，重新引入 `fan.timeout`（在 charge 点读取环境），吸收 C-189。
5. **AARA over MIR**：对已证明上限在预算内的调用省略计量。

<a id="反証条件"></a>

## 反证条件

- charge trace 无法通过 lowering 保留（renderer peephole 最可疑）。由 Stage 1 检出，使 Stage 2 及之后失效。
- 即使限定 region，计量开销仍不可接受。→ 加粗 bounded region 粒度（合并 block 级 charge），或提前 Stage 5 的 AARA。
- fuel 特化的代码大小不实用。→ 退到诊断限制 race 目标只能是自包含函数。
- pure 限定的 race 缺少真实用途。→ 提前 Rung 1（输出 transactional），仍不足则重新评估 race 作为“比较 N 个 bounded 计算”的语法糖价值。

保留代码中日文注释的读者释义：bounded 在 n fuel 内完成则返回 Ok；race 给每个分支 n fuel，消耗最少的成功分支获胜。历史 timeout 草图相对于环境，属于 R_Ω 契约类别，在该层可用前保留 tombstone。
