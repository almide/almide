历史描述：DO-178C 飞行级差距分析：6 个支柱的状态 + 2 个工程关键支点。
<a id="flight-profile--航空品質への接地分析と2つのキーストーン"></a>

# 飞行配置 — 航空品质的接地分析与两个关键支点

[![English](https://img.shields.io/badge/English-d0d7de?style=flat-square)](flight-profile.md) [![日本語](https://img.shields.io/badge/%E6%97%A5%E6%9C%AC%E8%AA%9E-d0d7de?style=flat-square)](flight-profile.ja.md) [![简体中文](https://img.shields.io/badge/%E7%AE%80%E4%BD%93%E4%B8%AD%E6%96%87-24292f?style=flat-square)](flight-profile.zh-CN.md)

> 非规范性固定原文历史读者快照。未改动的固定原本是翻译依据；当前有界配置的规范权威是 ALS §B，而非历史 @flight 设计。未自动同步，也未独立验证证明、测量、实现状态或标准映射。下文飞行级/航空品质、certified、全球罕见、O(1)、WCET及比较措辞是原分析/提案记录，不表示航空认证、工具鉴定、当局接受或新保证。计数、旧源码行号、提议 gate 和后续修订各自保留日期/状态。分配次数本身不能确立字节上限或物理 WCET，技术证明资产不等于适航应用资格。此处未独立验证原文的 MISRA/Ravenscar/DO-178C 比较及对各标准义务的主张。命令为复现参考，不是执行指令。 [Source](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/active/flight-profile.md)

> **目标**：依据实际代码，确定 [v1-proof-architecture](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/active/v1-proof-architecture.md) 的 PCC 主干
> **已经覆盖 flight-grade（DO-178C Level A / DO-333 / DO-330）的哪些支柱，还缺什么**，
> 并定义**能靠工程闭合的两个关键支点**。航空是 [trust-layer](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/active/trust-layer.md) 的北极星，而非近期产品
> （参见 [v1-kgi-kpi](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/active/v1-kgi-kpi.md) 的 GTM 阶梯）；不过这里定义的飞行配置
> 与面向 agent 市场的 wasm 主权**共享同一个经过认证的 MIR witness**，
> 所以不是二选一，而是“为同一 witness 增加第二个消费者”的问题。
> **相关**：[certification-grade](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/active/certification-grade.md)（标准 5 个机制的拆解）/
> [certificate-format-v1](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/active/certificate-format-v1.md) / [v1-mir-architecture](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/active/v1-mir-architecture.md)（§1 wasm 主权、§9 被否决方案）。

---

<a id="1-結論一行"></a>

## 1. 结论（一行）

> **PCC 主干对飞行品质六大支柱中的“支柱①产物安全”覆盖到全球罕见的深度。
> 其余五项中，能靠工程闭合的是两个——（甲）把循环提升到 Coq 以限定 WCET；
> （乙）将 MIR→Rust 产品化，把 cert 绑定到 Ferrocene 资格化的飞行目标。
> 二者独立，都是“扩展”而非“重做”，并汇合为一个飞行栈。**

---

<a id="2-当初の6本柱分析--現行の認証状態ではない"></a>

## 2. 最初的六支柱分析 — 不是当前认证状态

此表中的定理数、覆盖率和未着手状态，是原始设计的快照。
当前证明/测量范围参见 `proofs/receipt.sh`，规范性 bounded 配置参见
[ALS §B](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/specs/als/bounded.md)，native renderer 参见
`crates/almide-mir/src/render_native.rs`。
技术资产的存在不推导出资格化、飞行适用性或当局接受。

DO-178C 要求的不是“正确的产物”，而是“**产生证据体系的开发流程**”
（与 [certification-grade](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/active/certification-grade.md) 对标准 5 个机制的拆解一致）。

| 支柱 | DO-178C 要求 | v1 状态 |
|---|---|---|
| **① 产物安全性** | 内存安全、资源有界、类型健全、capability 有界 | 🟢 **强**——PCC checker + 43 个定理，每次构建重新推导。全球罕见 |
| **② 目标代码忠实性** | 实际飞行的机器码 = 源码语义 | 🟡 **一半**——字节绑定仍只是“受信”（§3 契约），未证明。而且证明对象是 **wasm**（飞机不靠 wasm 飞行） |
| **③ 功能需求可追踪性** | 需求 ↔ 代码 ↔ 验证双向关联 + MC/DC 结构覆盖 | 🔴 **零**——证明安全，却没有追踪“是否计算正确控制律”的机制 |
| **④ 执行环境资格** | RTOS / HW / 引擎也达到 DAL-A，WCET 分析 | 🟡 **一半**——#865（2026-09-18，PR #2304）把 wasmtime 移出信任基础：`crates/almide-wasm-vm` 通过闭合指令集、加载时验证、固定容量栈、fuel 执行 Critical-profile 的 `to_wasi` 产物；需求绑定到 `REQUIREMENTS.md` 的 REQ-VM-N。WCET 使用 #569 的计数循环设计（[flight-wcet-loops](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/active/flight-wcet-loops.md)）。剩余 = VM 自身资格化及 RTOS / HW 侧（本文范围外） |
| **⑤ 认证档案 + 工具资格化** | PSAC〜SAS、71 项目标、DO-330 TQL | 🔴 证书是技术产物，而非认证产物。没有目标映射 |
| **⑥ 当局接受 + 实绩** | DER / SOI 审计、service history | 🔴 此 repo 没有展示 Almide 的当局接受/飞行实绩。不主张全球都不存在先例 |

**本文范围是②和④中能靠工程闭合的部分**（支柱③⑤⑥属于标准流程和多年当局工作，
在 [certification-grade](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/active/certification-grade.md) 与 GTM 范畴）。

---

<a id="3-フロントあ--wcet--静的確保"></a>

## 3. 前线（甲）— WCET / 静态分配

<a id="31-驚き朗報アロケータは既に-wcet-フレンドリー"></a>

### 3.1 惊喜（好消息）：分配器已对 WCET 友好

固定 64KiB、单次探测的原前提属于旧 renderer。
当前 structural allocator 在 `crates/almide-wasm/src/alloc_inline.rs` 中有 size-class/free-list，
heap cap 和增长处理也依赖实际 build/host 条件。
抽象分配次数或 O(1) 渐近性质，并不会自然给出目标机器上的物理 WCET。
必须指定 allocator、生成代码和 host 范围进行评估。

<a id="32-本当の-wcet-ギャップはアロケータではなくループ"></a>

### 3.2 真正的 WCET 缺口是**循环，而非分配器**

WCET 本质上是**循环迭代次数**的问题。这里有结构性空洞：

- **Coq 模型仍是“无循环”片段**。`proofs/Termination.v:16-18` 明确写出，
  `OwnershipChecker.Op` 没有循环构造器。Rust 侧 MIR 明明有循环标记
  （`crates/almide-mir/src/lib.rs:240` `LoopStart`/`LoopBreakUnless`/`LoopEnd`），
  **证明主干却完全不知道循环**（证书将循环标记视为 no-op）。
- `LoopBreakUnless` 是**任意运行时条件**（`lib.rs:242`）——没有任何地方表达静态迭代次数上限。
- RC 字母 i/a/d/m **抽象掉字节量**（`Op::Alloc` = 无论内容都只有一个 `'i'`，
  `lib.rs:129`）——所以**当前模型无法表达字节基准 WCET**，次数基准则可以。

<a id="33-キーストーンループを-coq-op-型に持ち上げる"></a>

### 3.3 关键支点：把循环提升到 Coq 的 `Op` 类型

这一个改动可**同时**解锁多个飞行性质：

1. 循环的**终止性证明**（当前只有 `Termination.v` 的无循环片段）
2. **no-alloc-in-loop**（可以用与 `proofs/Subset.v` 同形的结构谓词证明。
   当前只能通过 lowering 的 `scalar_loop_depth` gate `crates/almide-mir/src/lower/control.rs:446`
   在**编译器侧 = 未证明**地实现）
3. **计数循环迭代上限 → 总分配次数上限**（`try_lower_scalar_for_range` 在
   `control.rs:589` 的 desugar 已知道迭代次数——在那里捕获）

<a id="34-スロットイン箇所自然な順"></a>

### 3.4 插入位置（自然顺序）

| # | 位置 | 性质 | 等级 |
|---|---|---|---|
| 1 | lowering 的 loop-subset gate（`control.rs:426,589`，已有 `scalar_loop_depth`） | **在拒绝时**强制 no-alloc-in-loop | 编译器侧（未证明）、最便宜 |
| 2 | 新 cert 性质 + Coq 证明（`NoAllocInLoop.v` / `AllocBound.v`，以 `Subset.v` 为模板） | checker **每次构建重验** | 正式 flight-grade，**前提：循环进入 Coq Op 类型** |
| 3 | 新 MIR pass（仅计数循环 + 推导迭代上限） | 逐循环 / 整个程序的分配次数上限 | WCET 上限的主体 |

`Repr`/`LayoutId` 已携带大小信息（`lib.rs`），所以 static pool / arena-reset 可以通过替换
`render_wasm.rs` 的 `$alloc` 加入。**注意碎片化**：仅 exact-fit 的 free-list 在混合大小反复分配释放时，
会退化为 bump 膨胀 → 飞行配置需要 size-class pool 或 arena-reset。

<a id="35-飛行プロファイル--サブセット弱みではない"></a>

### 3.5 飞行配置 = 子集（不是弱点）

只用计数范围、no-alloc-in-loop、static pool——这正是 **DO-178C 实务中的 C 已在执行的纪律
（禁止递归、禁止动态分配、循环有界）**。不同之处在于将“靠评审遵守”改成“靠证书机械强制”。
**配置限制不是弱点，凭证明强制飞行代码已有的形式，反而成为卖点。**

→ **具体实现设计（Op 扩展、Coq 定理语句、连接点、降低风险的顺序）**：
[flight-wcet-loops](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/active/flight-wcet-loops.md)

---

<a id="4-フロントい--rust--ferrocene-忠実性"></a>

## 4. 前线（乙）— Rust → Ferrocene 忠实性

<a id="41-原設計の再利用見積もり割合工数は未評価"></a>

### 4.1 原始设计的复用估计（比例、工时未评估）

主干在 wasm 边界处清楚分开：

| 分类 | 文件 | 对飞行的含义 |
|---|---|---|
| **目标无关（原样复用）** | `OwnershipChecker.v`（`check_sound`）/ 证书 i/a/d/m/r/b（= **MIR 所有权事实**，而非 wasm，`OwnershipChecker.v:155-168`）/ `certificate.rs`（完全不感知 target）/ `RuntimeModel.v`（抽象 `Mem = Z→Z`）/ `NameTotality` / `CapabilityBound` / `TypeConcretization` / `Subset` / `Termination` / `FreeList` / `CowSafety` / `ALS` | 抽象事件模型的引理是复用候选。与生成 Rust 的语义对应是另一项义务 |
| **wasm 专属（重做）** | `Translation.v`（op→wasm 表）/ `WasmEncode.v` / `WasmExec.v` / `WasmRcDec.v` + `translation_validation.rs` 中的 V validator | 4/16 个文件，**最薄、最靠后处理的一层** |

<a id="42-native-経路で残る証明境界"></a>

### 4.2 Native 路径上仍存在的证明边界

- `OwnershipChecker` 或 eager 事件序列的定理，没有直接陈述生成 Rust 的语义保持。
  不把只有 `rust_pattern` 字符串表当作全部 Almide→Rust 翻译忠实性已证明。
- 采用 Ferrocene 的方针，与实际用已资格化的工具链验证过，是两回事。
  当前 `.github/workflows/ferrocene-lane.yml` 明确使用 stock rustc。
- 后段工具资格化不会自动消除前段翻译义务。即使选择 Wasm 之外的路径，
  也需要连接该路径模型/生成代码/机器码的证据，低成本尚未确立。

<a id="43-原設計の-renderer-工数評価現在は-native-renderer-が実在"></a>

### 4.3 原始设计的 renderer 工时评估（现在 native renderer 已存在）

以下是当初 demo 与 walker 的比较，并不意味着当前 renderer 尚未着手：

- **清晰的所有权保持映射** = `crates/almide-mir/src/render_native.rs`（368 行 demo）。
  但全部使用 Vec<i64>、未连接 CLI、`v0`/`v1` 编号局部变量，**未达可评审等级**。
- **生产 Rust** = `crates/almide-codegen/src/walker/` + `crates/almide-codegen/templates/rust.toml`
  （v0/legacy）。符合惯例、**可评审，满足 DO-178C“源码可读”**，但
  **没有 MIR-op ↔ Rust 片段的对应对象、不由 v1 MIR 驱动、未证明**。

→ 转向的实际成本 = **建成生产级 v1 MIR→Rust renderer**
（`render_wasm.rs` 的全语言版本，约 5 倍规模），使**同一经过认证的 MIR witness 驱动两个目标**。

<a id="44-v1-mir-architecture-9-との和解"></a>

### 4.4 与 v1-mir-architecture §9 的协调

[v1-mir-architecture](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/active/v1-mir-architecture.md) §1 说“wasm = 有主权的验证路径，Rust = 几乎平凡地 lowering 的踏板”；
§9 否决“以 Rust 为唯一真相 = 建立在没有形式规格的 Rust 语义上”。**飞行配置不与此否决矛盾**：

> 飞行**不建立在 Rust 语义上**。它绑定到经过认证的 **MIR 事实**，Rust 仍只是 renderer，
> Rust→机器码的信任由 Ferrocene 接手。MIR 仍是唯一真相。
> 唯一改变是把 Rust renderer 提升为**证明绑定的第二个生产目标**——主权仍留在 MIR。

§9 否决的是“把 Rust 当作**真相**”。本配置是“把 Rust 当作**证明绑定的 renderer**”，两者不同。

→ **具体实现设计（rust_pattern 表、忠实性定理、信任基础台账、Ferrocene 实证路径）**：
[flight-rust-ferrocene](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/active/flight-rust-ferrocene.md)

---

<a id="5-合流--一貫した飛行スタック"></a>

## 5. 汇合 — 一致的飞行栈

```
Almide 飛行サブセット            counted loop / no-alloc-in-loop / static pool  ← (あ)§3.5
   ↓  lower + checker
認証済み MIR(唯一の真)          既存4性質 + 新: bounded-alloc / counted-loop    ← (あ)§3.3
   ↓  render_rust(本番版を新設)                                                 ← (い)§4.3
可読 Rust                        rust_pattern 忠実性(eager instance 既証明)     ← (い)§4.2
   ↓  Ferrocene(資格化済み)      Rust→機械語の信頼を肩代わり = Gap 1 迂回         ← (い)§4.2
飛行 HW で走る機械語 + WCET 有界
```

各层信任都已列账。**战略解放**：健全性核心与目标无关，
所以**不必在 wasm 主权（agent 市场）与 Rust 飞行之间二选一**——
二者成为**同一经过认证的 MIR witness 的不同消费者**。
决定不是“放弃哪一个”，而是“**是否为同一 witness 增加 Rust 飞行 renderer 作为第二个消费者**”。

---

<a id="6-2つのキーストーン"></a>

## 6. 两个关键支点

| | 关键支点 | 类型 | 解锁什么 | 前提 |
|---|---|---|---|---|
| **（甲）WCET** | 将循环提升到 Coq `Op` 类型 + 计数循环上限 | 证明扩展 | 终止性 / no-alloc-in-loop / 分配次数上限——三个飞行性质同时 | 无（不依赖 Rust 决定） |
| **（乙）Ferrocene** | 生产 v1 MIR→Rust renderer + `rust_pattern` 忠实性层 | 翻译/runtime 对应的证据及工程（工时未定） | 绕过 Gap 1 / 飞行产物 / Rust 市场 | 确定 §4.4 方针 |

**先后**：二者独立。若排顺序，**先做（甲）的循环提升**——确定飞行子集的“形式”，
也解锁本身就有价值的终止性证明。**（乙）先记录 §4.4 的战略决定
（协调 wasm 主权与 Rust 飞行），再开始生产 renderer。**

---

<a id="7-flight-readiness-ladder--近い-の-exit-criteria"></a>

## 7. 飞行就绪阶梯 — 🔴→接近的退出条件

[v1-kgi-kpi](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/active/v1-kgi-kpi.md) / GTM 的 A、B 层状态当前为 🔴（尚不可行）。
依次定义将其翻为**接近**的 gate。在“达到 flight-grade”（支柱①〜⑥全部满足、当局接受）之前，
设置**使销售成立的技术前提已齐备**这一中间目标。

<a id="71-近いの定義誇張しない線"></a>

### 7.1 “接近”的定义（不夸大的边界）

> **接近** = 对 A、B 层认证/安全工程师而言，可作为“可资格化的实现基底”被接纳，
> 且技术前提已齐备，使**试点/合作现实可行**。
> **≠** 实机完全认证、当局（FAA/EASA）接受、量产飞行。
> 那些是客户侧多年流程，在本阶梯之外（§8）。

<a id="72-gate近い順序つき"></a>

### 7.2 Gate（🔴→接近，有顺序）

| Gate | 内容 | 为什么作为可信度 gate | 对应设计 |
|---|---|---|---|
| **G-F0 执行对等** | 实际程序运行到底，与 v0/oracle 字节一致（完成 Gap 3） | 不能运行的语言会被拒之门外。一切的基础 | self-host runtime（进行中） |
| **G-F1 飞行配置定义+强制** | 规格化飞行子集（计数循环 / no-alloc-in-loop / 有界静态分配），checker 强制边界 | 没有向安全工程师展示的“形式”，就无从谈起 | §3 关键支点（甲）前半 |
| **G-F2 WCET 有界性证明化** | 把循环提升到 Coq + 计数循环上限 + 总分配上限变为已证明性质 | “WCET by construction”是 A、B 层基本门槛 | §3.3 关键支点（甲）主体 |
| **G-F3 飞行目标确定+忠实性绑定** | 确定 Rust→Ferrocene 为飞行目标，证明飞行子集上生产 MIR→Rust + `rust_pattern` 忠实性，实证 Ferrocene 编译 | 目标代码信任（支柱②④）没有落地，就只是纸面 | §4 关键支点（乙） |
| **G-F4 参考应用端到端通过 make verify** | 飞行配置中的安全关键形状小模块（控制律内核/状态机/watchdog）：运行→oracle 一致→签发证书→可读 Rust→Ferrocene | 让认证工程师感兴趣的**素材**。把幻灯片变成产物 | `make verify` 杀手级演示（[v1-kgi-kpi](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/active/v1-kgi-kpi.md)） |
| **G-F5 证书↔认证目标映射（档案种子）** | 草拟证书/receipt 覆盖哪些 DO-178C 目标 / DO-330 工具资格论证 / DO-333 形式方法 credit，形成 DER 可评审形式 | “cert 以后再说”不能支持“可资格化” | [certification-grade](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/active/certification-grade.md) 的 CG 领域 |
| **G-F6 资格化套件产品定义（AbsInt 模型）** | 确定评估套件提案范围（checker 证据、验证 harness、可追踪性、档案模板）。定义不表示已完成资格化或接受 | 没有销售对象，“接近”不会成为销售 | GTM（[v1-kgi-kpi](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/active/v1-kgi-kpi.md)） |

**G-F0..G-F6 全部齐备时，将 A、B 层状态从 🔴 翻为接近。**

<a id="73-当初の進捗マーカー現行-verdict-は-receipt-を参照"></a>

### 7.3 最初进度标记（当前 verdict 参见 receipt）

- **G-F0**：🔄 进行中——通过 self-host stdlib 攀登执行对等（string/list/math 转为 Almide）
- **G-F1**：📐 有规范规格、未强制——[flight-subset-spec](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/active/flight-subset-spec.md)（特性分类、开放问题定案、`@flight` 强制设计）
- **G-F2/G-F3**：📐 已设计、未实现——本文 §3/§4 的两个关键支点是**最大的技术高山**（[flight-wcet-loops](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/active/flight-wcet-loops.md) / [flight-rust-ferrocene](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/active/flight-rust-ferrocene.md)）
- **G-F4**：📐 已设计、未实现——[flight-reference-app](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/active/flight-reference-app.md)（PID 控制律内核 + make verify 7 阶段 + receipt。Slice 0 当前 green）
- **G-F5/G-F6**：📐 已设计、未着手——[flight-qualification](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/active/flight-qualification.md)（DO-178C/330/333 映射 + 资格化套件）

→ 阶梯上**仍在 G-F0 途中**（所有 gate 已有设计）。关键路径 = 完成 G-F0 → 关键支点（甲）（乙）
→ 参考应用 → 档案种子 + 套件定义。**所有 gate 从“未着手”升到“已设计、未实现”**
（全部 7 个 gate 都有具体设计文档）。

<a id="74-近いに不要誇張防止後回し"></a>

### 7.4 达到“接近”**不需要**的内容（防夸大、延后）

虽与 §8 重叠，但明确阶梯边界。以下**不纳入**“接近”（纳入会过度建设）：

- 实机完整 DO-178C 认证（客户侧、多年）
- 当局接受“PCC = 资格化替代”的论证（零先例、监管）
- **整个语言**的 MC/DC 结构覆盖（以飞行子集的 DO-333 形式方法 credit 替代）
- **整个语言**的 WCET（只要求飞行子集）
- service history / 飞行实绩

---

<a id="8-この文書が変えないもの正直な範囲"></a>

## 8. 本文不改变什么（坦诚范围）

工程只能闭合支柱②④的一部分。**以下仍远离 flight-grade，且不在本配置范围内**：

- **支柱③ 功能需求可追踪性 + MC/DC**——安全证明不能替代（应用责任 + 需要追踪/覆盖机制）。
  [certification-grade](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/active/certification-grade.md) 的 CG 领域。
- **支柱⑤ 认证档案 + DO-330 目标映射**——尚未生成认证形式产物。
- **支柱⑥ 当局接受 + service history**——“PCC 替代资格化”零先例，需要多年监管工作。飞行实绩为零。

→ 因此，正如 [v1-kgi-kpi](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/active/v1-kgi-kpi.md) / GTM 所言，**航空是北极星、信誉之锚，而非近期产品**才正确。
agent 市场、CRA 市场**无需闭合**本配置的关键支点就能解锁。
本配置定义的是“**认真接近航空**的两条工程最短路径”，不是“拿下航空”的全部流程。

---

<a id="9-関連"></a>

## 9. 相关

- [flight-subset-spec](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/active/flight-subset-spec.md)——**G-F1 规范规格**（飞行子集特性 IN/OUT、开放问题定案、`@flight` 强制）
- [flight-wcet-loops](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/active/flight-wcet-loops.md)——**关键支点（甲）G-F2 的具体实现设计**（计数循环 + Coq 提升 + 分配上限）
- [flight-rust-ferrocene](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/active/flight-rust-ferrocene.md)——**关键支点（乙）G-F3 的具体实现设计**（生产 MIR→Rust + rust_pattern 忠实性）
- [flight-reference-app](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/active/flight-reference-app.md)——**G-F4**（PID 控制律内核 + make verify 7 阶段 + receipt）
- [flight-qualification](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/active/flight-qualification.md)——**G-F5/G-F6**（DO-178C/330/333 映射 + 资格化套件）
- [v1-proof-architecture](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/active/v1-proof-architecture.md)——PCC 主干落地形态、Gap 1/2
- [certificate-format-v1](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/active/certificate-format-v1.md)——i/a/d/m/r/b 所有权字母表
- [v1-mir-architecture](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/active/v1-mir-architecture.md)——§1 wasm 主权、§9 否决方案（§4.4 协调）
- [certification-grade](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/active/certification-grade.md)——标准 5 个机制、支柱③⑤领域
- [v1-kgi-kpi](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/active/v1-kgi-kpi.md)——性质覆盖 4/8、Gap 台账、GTM 阶梯
- [trust-layer](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/active/trust-layer.md)——L0-L4，航空 = 北极星

保留的日文图示说明：它描绘原提案链条，从 Almide 飞行子集（计数循环 / 循环内不分配 / 静态池，前线甲 §3.5），经 lowering 和 checker，到作为唯一真相的已认证 MIR（已有 4 项性质，加有界分配/计数循环，甲 §3.3）。新建的生产 render_rust（乙 §4.3）生成可读 Rust，具有 rust_pattern 忠实性（图称 eager instance 已证明，乙 §4.2）；图中标为已资格化的 Ferrocene 承担 Rust→机器码信任、绕过 Gap 1（乙 §4.2），最终成为在飞行硬件运行且 WCET 有界的机器码。这是原图设计主张，不是独立验证已实现或已资格化的栈。

历史一致性注记：§2 明确不主张全球不存在先例，但后面的 §7.4/§8 保留更强的“零先例”措辞。两者原样保留，不整合成新事实主张。旧图的资格/证明措辞与 §4.2 的 stock-rustc/未证明翻译边界也分别保留。§7.3 和 §9 将旧子集文档称为规范；如上所述，当前有界配置的规范权威仍是 ALS §B。
