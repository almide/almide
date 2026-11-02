说明：在 byte-identity 下使 fan.timeout 合法——fuel、确定性配额、胜者顺序、effect 隔离
<a id="deterministic-bounds--legalising-a-bound-on-computation-under-byte-identity"></a>

# 确定性边界——在 byte-identity 下使计算上限合法

[![English](https://img.shields.io/badge/English-d0d7de?style=flat-square)](deterministic-bounds.md) [![日本語](https://img.shields.io/badge/%E6%97%A5%E6%9C%AC%E8%AA%9E-d0d7de?style=flat-square)](deterministic-bounds.ja.md) [![简体中文](https://img.shields.io/badge/%E7%AE%80%E4%BD%93%E4%B8%AD%E6%96%87-24292f?style=flat-square)](deterministic-bounds.zh-CN.md)

> 固定原文的非规范读者快照；原文仍为权威，不自动同步。未独立验证实现或执行引用指令。历史状态、计划及比较主张保留原范围。
> [Original source / 原文](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/active/deterministic-bounds.md)

> 原文注记：这是历史设计论，后续答案和语法由链接的 logical-time 设计给出。代码 sketch 保留，不是当前语法验证或已实现证据。“先例从未组合”“library 并发不能保证”是原定位，不是独立确立的不可能性或优先权。Wasmtime/EVM/Verse/LVars/AARA 描述及 epoch 约10%未经新核验或 benchmark。有限观测/2-safety、fuel 终止论证限于所述片段及隔离前提，不扩张到任意 host call 或未计量工作。stdout/stderr/exit 观测相等不同于 artifact 字节。共享 fuel/共享 MIR 单独、总 fuel 单独、oracle 改述单独皆明确不充分。未执行实现、gate 或测量。


> **设计答案**：本文提出的四个开放问题（预算分配、胜者选择、取消、effect 隔离）已在 [logical-time-async.md](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/active/logical-time-async.md) 回答。本文保留为问题陈述与正确性论证，另一篇是文法设计。

**修订说明。** 初稿声称引入 fuel 本身会使 `fan` 取消确定。**这是错的**，其教训值得保留：**共享** fuel counter 仍是调度函数，因为哪个 sibling 先花预算决定谁耗尽。fuel 是必要成分，不是充分条件。另外四项夸大主张已就地订正，各有标记。

问题不是“能否有 timeout”，而是**计算上限能否在 byte-identity 契约内合法**，而不只在契约未覆盖处容忍它。

<a id="the-reframe-which-survives-review"></a>

## 经 review 仍成立的重述

`fan.timeout` 在0.29.0被移除，因为 wall-clock 上限令结果取决于 host 速度。重述是：**问题在所用数量，不在设界本身**。以程序自身决定的量替代 wall-clock，性质可以保留。

这不是猜想。Wasmtime 对两种中断机制正如此说明：**fuel** 在同程序、初态和 fuel 下、没有其他非确定性时，在相同位置中断；**epoch** 以 wall-clock 为基础，中断位置非确定。业界已为两侧命名。

**EVM gas** 是大规模的同一论点：每 node 必须对同一状态转换应用同一 gas schedule，得到同一 out-of-gas 判定，Yellow Paper 定义为 exceptional halt。上限的确定性是正确性论据，不是锦上添花。

<a id="what-the-relational-property-actually-is"></a>

## 关系性质实际是什么

*（订正：初稿无条件称其为2-safety。）*

关注的性质联系两个实现：

> ∀ p, i.  `observe(native(p, i)) = observe(wasm(p, i))`

称为 **2-safety**——最多两条有限 trace 可展示违反（Clarkson & Schneider）——仅在**每次运行都达到有限观测的片段**才清晰成立。含发散及共同终止性的完整观测等价不是无条件2-safety；反例可能需要展示两次运行都永不终止，任何有限 trace 对都无法显示。

这不是可绕过的技术细节，正是 bounded 片段重要的原因。**fuel 上限下每次运行都在有限时间落入 `Ok` 或 `Exhausted`**，该片段性质可有限反驳，2-safety 框架成立。诚实表述为：

> **bounded 片段**上的可观测相等是关系 safety 性质，在本文有限观测语义下是2-safety。片段之外主张更弱，不应说成2-safety。

本 repo 术语核对：以上为**可观测输出**相等（stdout、stderr、exit code，契约台账定义）。“Byte-identity”在 org-verify 扫查也指 artifact 字节相等。两者是不同主张，本文指前者。

<a id="path-a--fuel-and-the-part-that-is-genuinely-hard"></a>

## Path A——fuel，以及真正困难的部分

**机制。** 抽象 step 预算由程序自身执行扣减，是程序的函数。同程序、同输入、同预算 ⇒ 任意机器结果相同。

**精确陈述 Almide 的实际优势。** *（订正：初稿称共享 MIR 使两路径一致，单靠它并不能。）* 两 backend 的 fuel 需要双方同意何为 step。共享 MIR 使其**可表达**，不使其**为真**。需要更强要求：

> 两 renderer 必须**保留共享 MIR 的 charge event**——同事件、同顺序、各一次。

renderer 局部 peephole 融合或重排两个 MIR op，即使 MIR 共享也破坏此性质。因此门禁不变量不是“有一个 MIR”，而是“lowering 保留 charge trace”：

    chargeTrace(native(M)) = chargeTrace(wasm(M)) = chargeTrace(M)

**仅比较总消耗 fuel 不够。** 两路径合计相同仍可分歧：charge 顺序不同，一方可中途达界而另一方不达。所以 fixture 须比较三项：`result`、`consumed_fuel`、**charge-event trace**。

**fixture 反证，不证明。** 它是最廉价反证器，应先于功能编写。具有证明结构的证据是逐 charge event 精确保留一次的 validator（与既有 ownership certificate 同类对象），加上生成 MIR 上的 property testing。

<a id="the-cost-model-becomes-part-of-the-language-semantics"></a>

### cost model 成为语言语义一部分

一旦用户可观测 `Exhausted`，cost model 不再是实现细节。至少需要：

1. **版本化**。cost model 变化是语义变化，属于契约台账。
2. **优化级不变**。`-O` 不得改变哪些程序耗尽。
3. **renderer 优化不变**。target 专用 pass 不得移动 charge point。
4. **定义算术**。counter 溢出及嵌套 `bounded` 的含义。
5. **可变成本操作**。list copy/string concat 成本是大小函数，不是假称常数。
6. **host call、allocation、RC traffic 计量政策**——计入消耗还是免费，明确说明。

<a id="path-b--quasi-determinism-a-precedent-not-a-theorem-to-lean-on"></a>

## Path B——准确定性：先例，不是可倚赖的定理

*（订正：初稿引用了论文并不存在的“否定定理”。）*

**LVars / Freeze-After-Writing**（Kuper、Turon、Krishnaswami、Newton——POPL 2014）是最近设计先例。相关内容不是“非单调观测总破坏确定性”的一般定理，而是**公开 quiescence 等非单调、执行中途性质，可能把完全确定性削弱成准确定性**：全部终止运行值相同，或某运行抛错；若信息不许逃逸，就可避免弱化。

这是上限两结果的合适框架，也为 cost model 仅近似可移植时的诚实退路命名。采用**确定性** cost model，设计强于准确定性：`Exhausted` 是普通值。

**但仅 pure 片段。** *（订正：初稿说得松散。）* 要让 `Exhausted` 是普通值，bounded 计算不能泄露局部状态：

- 无外部 I/O。
- 无中间结果可逃逸的 mutable state（或事务 rollback）。
- 无捕获的 effectful closure。

所以 signature 不仅有函数型，还有 effect 约束：

    bounded(f: Pure[() -> T], n: Fuel) -> Result[T, Exhausted]

<a id="path-c--oracle-relative-contracts-and-what-they-do-not-buy"></a>

## Path C——oracle 相对契约，以及它不能提供什么

*（两处订正。）*

fuel 无助于 subprocess/socket。可用标准关系处理，将它们组织为**外部输入**而非例外。但只量化共享 `ω` 的朴素形式，暗中假定两实现于相同语义点、同顺序咨询 oracle。一般形式联系两个 oracle stream：

> ∀ p, i, ω_N, ω_W.  `R_Ω(ω_N, ω_W)` ⇒ `observe(native(p, i, ω_N)) = observe(wasm(p, i, ω_W))`

其中 `R_Ω` 联系对应 HTTP response、filesystem result、process output。

**必须直说限制**：把 wall-clock timeout 改说成 oracle event，**不会**使其兼容 byte-identity。两实现仍可在不同语义点收到 event。Path C 只把环境非确定性纳入同一框架，替代不断增长的例外清单（含 C-189 platform-reporting 豁免），仅此而已。

<a id="path-d--static-bounds-scoped-correctly"></a>

## Path D——静态上限，正确限定范围

*（订正：“cost 静态已知就无需 runtime bound”太强。）*

**AARA**（RAML——Hoffmann、Aehlig、Hofmann 及后继）为一阶函数式程序推导多项式 resource bound，涵盖 evaluation step、heap 或 user-defined tick。它不完备：多项式时间程序也可能令分析失败。

准确主张：

> AARA 证明上界且上界在 policy budget 内时，可**仅对该 call**省略 runtime metering。其他位置保留 runtime counter。

<a id="the-actual-question-can-fantimeout-be-legalised"></a>

## 真正问题：能否使 `fan.timeout` 合法

能，但不靠 fuel 单独完成，命名也重要。

    deterministic fan bound  =  fuel
                             +  deterministic budget allocation
                             +  logical winner selection
                             +  effect isolation

上式意为：确定性 fan 上限 = fuel + 确定性预算分配 + 逻辑胜者选择 + effect 隔离。

<a id="1-budget-allocation-must-not-be-a-race"></a>

### 1. 预算分配不能是一场竞赛

单一共享 counter 正是 bug。给定：

    fan.bounded(1000) { A  B }

先推进 A 900步的 schedule 与先推进 B 的 schedule，对谁耗尽给出不同答案：总预算同，结果不同，由 scheduler 决定。**分配须是 source 与 MIR 的函数**，或确定性分割（`A: 500`、`B: 500`），或各 branch budget 加 block 独立 logical-time cap。选哪规则是设计问题，必须有规则则不是可选项。

<a id="2-the-winner-is-chosen-by-logical-order-never-by-physical-completion"></a>

### 2. 胜者按逻辑顺序选择，绝非物理完成顺序

“OS thread 上谁先完成”正是 native 与 wasm 不同之处。选择规则必须全域且由语法确定：

- 确定性逻辑完成 step 较早者胜。
- 平手按 source 顺序。

所以 logical step250 完成者胜过300，与 wall-clock 无关；同 step 两 branch 按出现位置排序。

<a id="3-cancellation-is-an-optimisation-not-an-observable"></a>

### 3. 取消是优化，不是观测量

这是使整体可实现的部分。必须确定的是：

- 采用哪个 branch 结果。
- 哪些 branch 被视为 `Exhausted`。
- commit 哪些 effect。

native 实际**何时**停败者无需与 wasm 相同，前提是**不可观测**。native 可即刻停 A，wasm 继续让 A 跑；只要两者不泄露，观测行为相同。

这仅在 Path B 隔离下成立：pure branch，或 buffer effect 只 commit 胜者，或将 effect 限于 cancellation-safe 集。

<a id="4-two-constructs-because-the-user-buys-two-different-guarantees"></a>

### 4. 两种构造，因为用户要两种不同保证

*（采用拆分。）* 技术上 fuel 可以写作 `fan.timeout(1000)`，但了解 timeout 的用户会将 `timeout(1000)` 读作毫秒。“确定性计算预算”与“经过一段实时间放弃”不是同一产品：

    fan.bounded(1000)      // deterministic: logical budget, byte-identical outcome
    fan.timeout(5.seconds) // oracle-dependent: environment-relative, Path C contract

上面注释译文：bounded 为确定性逻辑预算、byte-identical 结果；timeout 为 oracle 依赖、环境相对、Path C 契约。

所以 `fan.timeout` 以具有 oracle 相对契约、**诚实环境依赖**的构造回归，确定性上限则有自己的名字和强得多的承诺。

<a id="prior-art--every-part-is-solved-somewhere-and-the-parts-have-never-been-joined"></a>

## 先例——每个部分在某处已解决，但尚未合并

设计部件并不新颖。明确借用哪些部件，会使真正主张更清晰；每个借用部分都是工作系统，而非仅论文。

<a id="verse--race-is-nearly-this-construct-already"></a>

### Verse——`race` 已接近此构造

Epic Verse 的 `race` 启动多个 async expression，取首个完成者：

```verse
Winner := race:
    A()
    B()
    Sleep(5.0)
```

其语义与 stage3 所需非常接近：首个完成 expression 胜、败者取消、lifetime 限在 `race` block；尤其值得注意，**多个在同一 simulation time 完成时，source 中最先写的获胜**。即上述逻辑完成 + source 顺序 tie-break，已在语言中交付。

Verse 还有 effect type（`transacts`、`decides`），可 rollback failure context 内的变化。

**剩两个 gap，恰是 Almide 增添的两点。** 第一，Verse 按**simulation time**而非确定性指令预算竞赛，不处理两个 backend 计算 cost 一致。第二，取消败者不等于保证**其 effect 永不可观测**：Verse 有胜者选择、structured cancellation、事务、effect type，但未融合成“败者 effect 不能逃逸”的单一契约。

<a id="esterel--lustre--lingua-franca--logical-time-done-properly"></a>

### Esterel / Lustre / Lingua Franca——正确使用逻辑时间

同步语言家族数十年研究“以逻辑而非物理时间赋予并发含义”。Lingua Franca 在显式 logical time 执行 reaction，同时间按声明顺序，刻意与已过物理时间解耦。仍是较早 logical step 胜、平手按 source 顺序。

gap：其 logical time 是 **event/timer** 时间，不是**计算成本**。该家族并不界定 reaction 的工作量。

<a id="evm--fuel-and-rollback-fused"></a>

### EVM——fuel 与 rollback 融合

EVM 是本文所需两半最强的既有组合：每条指令有 gas cost，耗尽确定性停止，out-of-gas transaction **回滚其状态变化**。fuel + effect 隔离，一体机制，大规模运行。

gap：没有 `fan`。EVM 解决**确定性有界逐次执行**，没有运行两 branch 并选逻辑先完成者的概念。

<a id="haskells-par-monad-and-lvars--determinism-under-a-free-scheduler"></a>

### Haskell `Par` monad 和 LVars——自由调度下的确定性

两者都通过限制并行代码能做什么达成确定性并行：`Par` 限可用操作，LVars 只许单调更新。因此 work-stealing scheduler 可自由调度，观测结果不变。

gap：都不围绕计算预算、首完成者采纳或上限组织。

<a id="the-map"></a>

### 对照表

| 成分 | 解决者 |
|---|---|
| 确定性 fuel | EVM gas、Wasmtime fuel |
| 逻辑胜者选择、source 顺序平手判定 | Verse `race`、Lingua Franca、Esterel |
| structured cancellation | Verse `race` |
| 败者 effect 永不可观测 | EVM rollback、Verse transaction、purity |
| 自由 scheduler 下确定性 | Haskell `Par`、LVars |

**每行有强答案，但没有一行答案覆盖另一行。** 所提设计的一句话描述是：

> **在 EVM gas clock 上运行的 Verse `race`。**

它能成为一个构造而非四个 library，理由同上：compiler 知道 `fan`，预算分配、胜者选择、effect 可见性都是 compiler 的决定，而非分散于 user code 和 scheduler。

这是诚实定位：不是新想法，而是**三个已很好解决、却未曾同时成立的子问题之汇合**，再加一项它们都没有的要求——两 backend 成本一致；这使结果 byte-identical，不仅每实现分别确定。

<a id="the-claim-pushed-to-its-real-strength"></a>

## 将主张推到真正强度

有趣的陈述不是“fuel 修好 timeout”，而是：

> **`fan` 是 compiler 已知构造而非 library，因此 compiler 拥有预算分配、胜者选择、effect 可见性——确定性上限在 fuel 之外所需三项。** 并发是 library 的语言无法给出此保证，因为这些决定位于 user code 和 scheduler。

这就是共享 MIR + first-class `fan` 带来的东西，关乎语言形状而非单个功能。

<a id="staged-plan-each-stage-falsifiable"></a>

## 分阶段计划，每阶段可反证

1. **MIR op cost model + 两 renderer charge-trace 保持**。反证器为 `spec/wasm_cross` fixture，比较 `result`、`consumed_fuel`、**charge-event trace**。差一个单位或位置就停止计划，意味着 charge 保持性质为假，本身也值得知道。
2. **`bounded(f: Pure[() -> T], n) -> Result[T, Exhausted]`** 用于逐次 pure 计算，cost model 作为版本化语义对象记入契约台账。
3. **确定性 `fan.bounded`**：预算分配规则、逻辑胜者选择、effect 隔离。以 fan 语义再设计而非 stage1 直接推论，重访 #1000 cancellation 排除。
4. **oracle 相对契约**：为 `process` / `http` / `fs` 用 `R_Ω`，纳入 #1040 effect-surface timeout 和 C-189，重新接受环境相对构造 `fan.timeout`。
5. **MIR 上 AARA**：证明的上限在预算内时省去 metering。

<a id="what-would-falsify-this"></a>

## 什么能反证

- **lowering 不保留 charge trace**——最可能在 renderer 局部 peephole。stage1 发现它，会使 stage2～3无效。
- **限定范围后 overhead 仍不可接受**。Wasmtime 说明 epoch 中断较便宜，报告约10%；fuel 更贵，没有自己的 benchmark 不应在此引用一概而论的数字。overhead 取决 workload、计量粒度，正因此 budget 用于显式 bounded 区域，而非全局。
- **用户实际想放在 bounded 区域的 effect 无法实用地隔离**。则 `bounded` 保持 pure-only，仍是 `fan.timeout` 从未具备的东西。
- **AARA 无法处理 stdlib 形状**。部分很可能如此；runtime counter 是 fallback，局部静态上限仍是实质主张。
