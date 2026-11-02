说明：对“2026 年最佳 async”主张的审计：方向、对手与拉开差距的五招。
<a id="the-2026-async-claim--audit-and-the-runaway-plan"></a>

# 2026 年 Async 主张 — 审计与拉开差距的计划

[![English](https://img.shields.io/badge/English-d0d7de?style=flat-square)](async-world-claim.md) [![日本語](https://img.shields.io/badge/%E6%97%A5%E6%9C%AC%E8%AA%9E-d0d7de?style=flat-square)](async-world-claim.ja.md) [![简体中文](https://img.shields.io/badge/%E7%AE%80%E4%BD%93%E4%B8%AD%E6%96%87-24292f?style=flat-square)](async-world-claim.zh-CN.md)

> 固定原文提交的非规范阅读快照；以原文为准，不自动同步。未独立执行、验证实现、检查证明或测量。历史命令与设计指令是资料，不是已执行的指令。
> [Original source / 原文](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/active/async-world-claim.md)

> 原文注记：历史宣传、竞争假设及已被替代计划与后续明确限制并存；不是独立验证的2026年排名、产品比较或唯一性证据。开头状态、后续判定与旧未来式 lane 有时层差异：此处已实现仅 timeout 事件 replay，不是所提 host 响应 tape 或一般 record/replay；Duration clock min 未实现。旧 fan.race/fuel、timeout(ms:) sketch 保留，不转换为新语法。Lean 选择代数定理、有限模型枚举、指定跨 target fixture 与含 retry 的 dojo 7/7 是不同证据，不是全 runtime 符合性、任意程序确定性或所有 bug 复现。正文更强的唯一性、结构不可破坏、低成本接线、CI 存在证明措辞为原文历史论述，不是新增保证。旧 arm 输出 buffering 前提与另文记录的 transactional buffering 撤回冲突，作为历史而非当前 I/O 隔离保留。tape 敏感性及 redaction 仅保留所述设计意图；类型不变本身不证明脱敏后 replay 等价。未运行竞争产品、编译器、证明、benchmark、命令或网络实验。

> 宪章：[async-inception.md](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/active/async-inception.md)。本文审计其主张。
>
> 审计**能否说**“2026 年世界最好的异步语法”，以及**遥遥领先**所需的五招。设计见 [logical-time-async.md](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/active/logical-time-async.md) / [fan-v2.md](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/active/fan-v2.md)，证明见 [logical-time-proofs.md](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/active/logical-time-proofs.md)。本文处理主张一侧——哪一句能经得起审查，哪一句仍是虚假。

> **实现状态（2026-08-03，branch `worktree-stage1-charge-probe`——有出入以 BURNDOWN/契约台账为准）**：下方执行顺序保留起草时“未来要做”的表述。当前情况如下。
>
> - **Lane 1 完成**：Wave 1（b817e3dc）→ Stage 1（8ece4153 + 4db109aa）→ Stage 2（4152da7b）→ Stage 3（113feee2）。CM-1 v0.3 = 3ns/unit，边界在 ns 精度 exact flip，契约 C-202..C-207 + 3-way corpus 常设——**claim 1–3 的启用条件成立**。
> - **Lane 2 B2 完成**：`fan.timeout(duration.ms(n)) { body }`（f8d760ba，C-208 + ALS-D5）。
> - **Lane 2 B1 仅 timeout-ω 切片**：ω = 期限到期时 wall check 的序号。`ALMIDE_OMEGA_RECORD=1` 采录 → `ALMIDE_OMEGA=<n>` 编译时 bake 回放；常设 gate 检查 record（native）→ replay（native + wasm）字节一致。**效果响应 tape（fs/http/random/process/env 的 ω schema + `--record` / `--replay` + redaction）未开始**——claim 4 若限于“timeout 事件的确定性复现”，今天可以说。完整表述需 B1 完成后启用（下方 sketch 是规格）。
> - **Lane 3 D 的 Almide 侧完成**：dojo async bank 7 个任务 **7/7 pass**（cli:claude，含 retry——dojo runs/2026-08-02/summary.md）。**claim 5 完整启用仍需同模型比较 lang-bench(async)**。
> - **补充**：race mapper 形也已实现（T7-1）——fan v2 matrix 所有单元均已确定。
> - **C（KPN）**：按设计尚未开始（仍在地平线）。

<a id="判定先に結論"></a>

## 判定（先说结论）

**本台账无法确立无条件的“世界最好”或全球唯一性。即便五招全部实现，也不能据此省去比较对象、版本和测量范围。**

- 可以说：有选择代数的 Lean 定理，以及指定 fixture 的跨 target 检查。前者不是整个实现的证明，后者不是所有程序的证明。
- 当前实现：有 Compute clock 的 `fan.min`、`fan.bounded` 和 Duration body 形 `fan.timeout`。Duration clock 的 `fan.min` 已决定但未实现（#3788）。
- timeout-ω replay 的 `tests/fan_timeout_replay_test.rs`（#3781）在多个 cut 检查 native/Wasm 输出一致和 cut 差异。这是限定 replay fixture 的依据，不是任意 fs/http/random/process/env 响应记录/回放的证明。
- 尚不能说：async 总体最优性、成熟度的世界排名。不应在没有固定比较对象和条件的实测时，把设计或有限 fixture 替换为排名、所有用途的保证。

<a id="戦場の名指しと勝算"></a>

## 点名战场与胜算

五句话是武器清单，不是战场。只点名一个战场：

> **大部分代码由 LLM 编写的时代的并发。** 在这里，agent 编写的并发代码每次运行都返回相同结果、出错后总能复现的性质具有价值。

**不在**由人运营容错服务器的世界（Erlang/Go 战场）作战——失利表中的 Erlang 行是主动让出的失利。五句话的职责：1–3 = 可验证武器，4 = 可复现武器，5 = 记分牌。

| claim | 胜算 | 依据 | 剩余风险 |
|---|---|---|---|
| 1 机器证明 | 已赢 | 7 定理 0 sorry | 单独无法带动市场（信任担保，而非需求） |
| 2 观察等价 | 高 | 199+ 契约与 gate 的现有基础 | Stage 1 charge-trace 保持（已定位到一点） |
| 3 计算成本 race | 默认取胜 | 未找到正在构建此功能的语言 | 发现先例（反证条件）。是学术旗帜，日常需求较弱 |
| 4 record/replay | **最大取胜路线** | Temporal 商业成功证明需求。护城河是结构性的（非确定性只经过一处 registered surface） | 实现范围（ω schema、redaction） |
| 5 LLM 实测 | 有利但有真实风险 | lang-bench 先行数据（2026-07-15，Almide 100% vs Gleam 19/20） | LLM 大量训练过 Go/TS async。结构性消除错误类别能否超越，由测量决定 |

此战场真正对手不是表中五位，而是**“runtime 就够了”论**（Temporal 型 replay 普及）。反驳立足结构：Temporal 以规约要求确定性，违规在运行时暴露。**LLM 会破坏规约，无法破坏结构**——与 MSR 第一行同样的逻辑。战场市价在上升：AI 编码是增长市场，对 deterministic simulation（Antithesis / FoundationDB 式）的关注也是顺风。概括胜算：在眼下小、未来升值的土地上，以结构性护城河率先立足。

<a id="当初検討した差別化の軸世界唯一性の証拠ではない"></a>

## 最初考虑的差异化方向（不是全球唯一性的证据）

<a id="軸-1-命令粒度の論理時計"></a>

### 方向 1：指令粒度逻辑时钟

| 系统 | 逻辑时钟粒度 | 纯计算之间能否 race |
|---|---|---|
| Verse | event / simulation tick | **应该不能**——不含 suspension 的表达式在同一 tick 内完成，因此纯计算 race 总是退化为源顺序第一项（据 docs 推论，未实机验证） |
| Lingua Franca | timestamp（事件标签） | 不能——时间来自事件，计算成本不进入时钟 |
| Esterel 系 | 同步 tick | 不能（同上） |
| EVM | **指令粒度（gas）** | 本来就没有并发语法 |
| **Almide（设计）** | **指令粒度（fuel）** | **可以——正是 fan.race(fuel:) 存在的理由** |

“将 Verse 的 race 运行在 EVM gas 上”创造两者单独都没有的分辨率。**此表没有任何一方具备按计算成本决定胜者的 race。**

<a id="軸-2-クロスターゲット観測等価の内側での-async"></a>

### 方向 2：跨 target 观察等价内部的 async

Go/Rust/Kotlin/Swift async 是单 target 语义，确定性由外部工具（loom、race detector、rr）在测试时近似。Almide 将 async 放在 native ⇄ wasm 观察等价契约（199+ 契约、带 ratchet）**内部**，并设计为连中断点（charge site）都跨两 target 一致。Swift 6 strict concurrency 消除的是 data race（类型级），不是非确定性。

<a id="軸-3-実装に先行する機械証明--全数合流ゲート"></a>

### 方向 3：**先于**实现的机器证明 + 全数合流 gate

- Lean kernel-check 7 定理、0 sorry（选择唯一性、cap 可见窗口保持、合流）
- 74,898 种配置 × 所有物理调度（含延迟 fuel 检查的 overrun）的 confluence 检查
- 证明工作在实现前发现并修复 3 个设计 bug 的记录

loom（Rust）、Coyote（.NET）、P 语言（AWS 实际生产使用）是在**测试时探索调度**的装置；在代表性系统中，尚未发现把合流作为语言语义证明并在发布前固定的实例。Verse 有 core calculus 形式化（Verse calculus，ICFP 2023），但未确认 concurrency/transactional 层机器证明已公开。

<a id="軸-4-最高を外部指標で測る文化"></a>

### 方向 4：以外部指标衡量“最好”的文化

MSR（modification survival rate）和 lang-bench 使用同模型、同任务实测，将“LLM 能最准确编写”从主张变成测量。async 任务库就是下方 D 招。

<a id="負けている軸--正面から認める表"></a>

## 失利方向——正面承认的表

| 对手 | 领先点 | Almide 现状 |
|---|---|---|
| Erlang/OTP | 容错性（supervision、let-it-crash）、30 年生产运行 | 没有结构性恢复。trap 只是确定性死亡 |
| Go | 生产 async 成熟度、goroutine 易用性 | oracle 层未实现，无法编写“真正并发 I/O race” |
| Verse | transactional effects 已发布，Fortnite 生产使用 | 效果隔离只有 Rung 0（pure）设计 |
| Lingua Franca | 分布式（federated）确定性、学术积累 | 分布式不在视野内 |
| Temporal 等 durable execution | replay 的持久执行已商业存在（但不是语言，而是 runtime） | **这里是五招 B 的目标** |

隐去此表而声称“最好”，会在 audit（本 repo academic-rigor 规范）中失败。

Lingua Franca 行包含审查必问：“logical time 不是 LF 更早吗？”一句话答案：**LF 时钟排列事件，Almide 时钟在计算内部计时**。LF 是组合 C / Python / TS / Rust 的协调层，不拥有 reaction 内容（target 语言代码），因此不能可移植计量计算——计量要求拥有 codegen（与 record/replay 护城河同构）。反过来，分布式确定性协调确实由 LF 先行；若 Almide 将来走出地平线（KPN channel → 分布式），LF 在那里是借鉴来源而非竞争对手。这是“地平线上的失利”——记录为第四类，区别于不追赶的 Erlang、只是时间问题的 Go/Verse、以及要夺回的 Temporal。

<a id="ぶっちぎりの五手"></a>

## 拉开差距的五招

<a id="a-stage-13-を実装して出荷する"></a>

### A. 实现并发布 Stage 1–3

纸面无法竞争。只有 fuel、bounded、race 在两后端带 gate 运行，方向 1–3 才从“设计讨论”变成“语言性质”。**五招中唯一的前提条件。**

<a id="b-oracle-層をrecordreplayとして設計する--決定打"></a>

### B. 将 oracle 层设计为“record/replay”——决定性一招

将 Stage 4 设计为**语言内建的确定性 record/replay**，而不是“重新引入 timeout”。

**主张形式**：确定层确立 `observe = f(program, input)`。oracle 层为 `observe = f(program, input, ω)`——把环境响应序列 ω 作为**显式输入**。于是：

    almide run app.almd --record trace.ω     # ω を採録
    almide run app.almd --replay trace.ω     # 同一実行を再現 — 別ターゲットでも

原命令注释译文：采录 ω；复现同一次执行——即使换到另一 target。

Replay 正确性不需新定理——只是重新应用函数（T9 的推论）。**不稳定的 async bug 会变成可复现产物。**

**为何只有 Almide 能低成本获得（护城河结构）**：对手的非确定性散布语言各处（Go goroutine 调度本身非确定，rr 不得不采录 syscall+调度。Temporal **以规约**要求 workflow 代码确定，违规运行时才发现）。Almide 确定层已固定所有内容，非确定性只能经 oracle 效果的**狭窄 registered surface**（fs/http/process/random/env——通过 self-host registry）。结构上只有一处可以贴 tape。**B 对 Almide 是接线工程，对对手是架构变更**，这就是“遥遥领先”的实质。

**设计 sketch**（在 Stage 4 设计文档正式化）：

- 按效果模块定义 ω schema（http：status/headers/body；fs：内容/错误；random：值序列；datetime.now：值序列；process：exit/stdout/stderr）。采录**响应值而非时刻**——时间只在 timeout 裁决进入语义，记录为“在哪个 charge site 到期”的**离散事件**。
- 消费点是 charge site（中断点统一原则），因此 ω 消费顺序已由确定层固定。所以 replay 与 target 无关——**native 采录、wasm replay** 由定义成立，并成为 R_Ω 契约可执行形式（executable oracle contract）。
- `fan.timeout(ms:)` 成为读取“ω 中 timeout 事件在哪个 site 出现”的语法。record 时真实时钟产生事件，replay 时 tape 回放。
- fan{} 并发 I/O 与 join-all + arm 输出缓冲（#1026 系）组合——ω 为每 arm 的响应序列，interleaving 不进入观察，无需采录。
- 契约：将“用记录的 ω replay，以字节一致复现观察”做成 C-NNN + fixture。等价于把 R_Ω 存在证明常驻 CI。
- 同时明确边界：ω 含网络响应等，tape 可能敏感。将 redaction 设计为 schema 侧职责（保留值类型、隐藏内容）。

<a id="c-決定的チャネルの地平を名指しでロードマップに載せる"></a>

### C. 将确定性 channel 地平线点名列入路线图

逻辑时钟上的 KPN（Kahn process network）型 channel + tick 顺序 select，为 #1000 保留的重访条件“structured concurrency 先有契约”提供具体路径。不追求 Erlang 容错本身（不同产品），但可将“没有 channel”这一评价缺口从**设计空白转为时机问题**。

<a id="d-async-版の実測を-dojo-に積む"></a>

### D. 将 async 实测加入 dojo

用 fan v2 任务库（fan-out、fallback、推测执行、用 bounded 使计算全域化）测量 MSR，提供与 Verse / Go / Rust / TS 同模型同任务的 lang-bench 比较。沿用 lang-bench 先例（2026-07-15，Almide 100% vs Gleam 19/20）。**把“最适合 LLM 的 async”作为测量值来说。**

<a id="e-claim-の文言を-evidence-つきで固定する"></a>

### E. 以 evidence 固定 claim 措辞

对外表述需如下限定依据范围。不混淆原 7 定理、模型枚举、fixture、Dojo 实验：

1. “逻辑时间的**选择代数**有 Lean 检查的定理”——不是整个执行系统或所有并发语法的符合性证明。
2. “对目标 fixture，通过契约 gate 检查 native / wasm 观察差异”——明确有限测量范围，与任意程序观察等价区分。
3. “`fan.min` 可在 Compute clock 下用逻辑成本作选择依据”——clock 必填，cost 契约限于 meter version。不声称全球唯一。
4. “对记录范围内的 oracle 事件有 replay 检查”——列举记录格式、目标 host、fixture，不保证复现所有 async bug。
5. “在指定模型、任务、版本上测量 async 编辑成功率”——仅伴随 Dojo 比较条件与结果报告，不泛化为所有语言中最准确。
每个主张都带对象和限制。即便组合起来，也不保证无条件世界最好、任意实现正确或所有 bug 可复现。

<a id="実行順--2-レーン並列--地平-1-本"></a>

## 执行顺序——2 条并行 lane + 1 条地平线

解开依赖后不是 A→B→C 串行。B 大半（B1）不依赖 fuel，可以并行。

**Lane 1（主线，串行——worktree：主）**

1. **fan v2 Wave 1**——any/settle block/mapper 形 + thunk tombstone + 8 个 spec 文件迁移 + matrix gate。不需 fuel，可立即开始。为 race 一开始就采用 v2 形而进行的前置工程。
2. **Stage 1：CM-1 + charge-trace 保持**——是整个程序的 falsifier，因此紧接 Wave 1。**此处不做 region 特化**：以隐藏标志（如 `--fuel-probe`）生成整体计量 build，先建立 fixture 三点比较（result / consumed / trace）和 validator。特化机制延后到 surface 到来的 Stage 2。
3. **Stage 2：fan.bounded**——v2 block form + region 特化 + 确定 CM-1 常数并登记台账（同一 PR）。
4. **Stage 3：fan.race**——剪枝 cap、trap 延迟裁决、嵌套 streaming、E027 修订。把 race belt / spike gate 接为 CI 受理条件。→ **claim 1–3 启用**。

**Lane 2（可并行——分离 worktree）**

- **B1：record/replay 基础**——不依赖 fuel。当前语言除 oracle 效果外全确定，所以在现有 surface 上采录/回放 ω 已有意义。盘点效果 surface（枚举 registered surface oracle 效果）→ ω schema → tape 格式 → `--record` / `--replay` → replay 等价契约 + fixture。→ **claim 4 启用**（B2 之前也能作为“effectful 程序确定性复现”主张）。
- **B2：fan.timeout(ms:)**——在 charge site 读取 ω 事件。仅此依赖 Stage 1，因此 Lane 1 第 2 步完成后汇入 Lane 2。

**Lane 3（dojo 侧）**

- **D：async 任务库**——v0 可用 Wave 1 后 surface（fan{} / map / any / settle）构建。Stage 3 后追加 race / bounded 任务。→ **claim 5 启用**。

**地平线（不开始）**

- **C：KPN channel**——Stage 3 完成后只起草设计文档。满足 #1000 重访条件（各语法契约优先）后再实现。

<a id="反証条件"></a>

## 反证条件

- 方向 1 反例：发现具有指令粒度确定性 race 的语言/实现（含研究语言）。→ 修正表格，撤回 claim 3。与 D 比较工作同时搜索。
- Verse 纯计算 race 推论错误（实机有分辨率）。→ 修正方向 1 的 Verse 行。表中已明确是推论。
- B 的 redaction 不实用（ω 本质敏感而无法共享）。→ replay 降为本地/CI 内调试功能，缩窄 claim 4 范围。
- D 测量显示 Almide async MSR 无法击败对手。→ 若是语法问题，转入 fan v2 反证条件（重新设计 block/mapper 二选一）；若是诊断问题，转入诊断改进 lane。测量优先。
