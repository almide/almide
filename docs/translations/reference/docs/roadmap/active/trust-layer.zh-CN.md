<a id="trust-layer--機械が書くソフトウェアの信頼層"></a>

# Trust Layer——机器编写软件的信任层

[![English](https://img.shields.io/badge/English-d0d7de?style=flat-square)](trust-layer.md) [![日本語](https://img.shields.io/badge/%E6%97%A5%E6%9C%AC%E8%AA%9E-d0d7de?style=flat-square)](trust-layer.ja.md) [![简体中文](https://img.shields.io/badge/%E7%AE%80%E4%BD%93%E4%B8%AD%E6%96%87-24292f?style=flat-square)](trust-layer.zh-CN.md)

> 非规范固定原文读者快照。未改动原文仍为读取方的权威来源。不自动同步，也未独立执行。历史主张、已采纳提案、实现状态及测量证据保留原范围；本翻译不独立验证。内联代码保持原样，包括原语言字符串。 MWS L0-L4 是拟议信任阶梯，不是 edit-locality L1-L3，也不是认证。3行配置和可复现主张是退出目标，不是在此验证的交付结果。保留所有 host/native/FFI、fixture、decline、trusted base 与外部 Dojo 限制。独有性、竞争者评价和市场必然性是历史作者的战略判断，不是独立确立的事实。 [Source](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/active/trust-layer.md)

> 原metadata非活动注记：类别战略——赢得“机器编写软件的信任层”：MWS Trust Levels、收据、关键路径。

> **目标**：为“以接近无需人工审查的信任水平运行代理编写的代码”这一类别定义测量标准（MWS Trust Levels），让 Almide 成为其参考实现。
> **活动范围**：公开 Levels 定义 + L1（[capability Phase 1-2](../../../../../roadmap/active/effect-system-capability.md)）+ receipts harness。
> **退出条件**：第三方可用 `git clone && make verify` 重现 L1-L2 全部 claim，且从 MCP 客户端只用 3 行配置即可运行 capability-bounded Almide agent。

<a id="なぜこのカテゴリが立つか"></a>

## 为什么会形成这一类别

代码编写者转为 agent 后，语言选择者从个人开发者变成**代理运营者**，选择标准随之变化：

| 人类编写时代的标准 | 机器编写时代的标准 |
|---|---|
| 熟悉度、招聘、表达力 | 修改存活率（MSR） |
| 库的广度 | 影响范围的可证明性 |
| 构建速度 | 构建与执行的可复现性 |
| 峰值性能 | 能否无需审查地运行 |

右列正是 Almide 的设计指标。没有其他语言让设计目标函数与采购标准一致。在此类别取胜，不靠语言间比较，而靠**成为定义测量标准的一方**——类似 SLSA 在供应链安全中所做的事。当他人开始用该标准自评时，类别就成立，其参考实现将成为默认选择。

<a id="mws-trust-levels-l0l4"></a>

## MWS Trust Levels（L0–L4）

每个级别按“运营者*无需验证*即可信任什么”定义。**不要为自己量身操纵标准**——即使没有 Almide，各级别也必须对行业有意义。任意设定的标准不会被采用，不被采用的标准无法形成类别。

| Level | 目标 | 当前依据与限制 |
|---|---|---|
| **L0 Contained** | 运行时隔离 | 依赖 Wasm host 的 import／配置／权限。不声称会自动 sandbox 化 native build 或外部代码 |
| **L1 Bounded** | 限制声明的能力 | 结合 capability checker 与 artifact/host 边界。不能仅凭 import 是否存在就证明共享 import 的操作或 host 实现 |
| **L2 Reproducible** | 指定条件下复现与观察比较 | 参见 `proofs/receipt.sh` 的 C-REPRO/C-SAFE 和 fixture/decline 范围。不是任意主机、任意输入下的不变性 |
| **L3 Verified** | 对明确安全模型的机械证明 | 当前 Rocq/structural RC 证据和 trusted boundary 以 receipt 为准。不是对全部编译器路径或全部 stdlib 的证明 |
| **L4 Measured** | 测量 LLM 编辑质量 | 归 Dojo 所有。限于公开结果的模型／任务／版本，不能只凭此 repo 断定外部测量的最新状态 |

> **这里的 L0-L4 是信任阶梯，与 `docs/specs/edit-locality.md` 的 L1-L3（编辑框架／跨目标一致／诊断局部性）不同。** 编号相同，但主张无关，彼此也不是前提。上下文不明确时，应写明是哪一套阶梯。

设计要点：**L1 是信任的技术核心**。L0 是“不能越过墙”，而 L1 的目标是核对 declared capability 与实际 artifact/host 配置。manifest 比较帮助运营者判断，但不能替代包括 host、native dependency、FFI 语义在内的所有审查。L2-L4 也都附带各自证据的 scope 和 trusted base。

<a id="既存-roadmap-とのマッピング"></a>

## 与既有 roadmap 的映射

此类别战略**不增加新的实现 roadmap**，而是规定既有 roadmap 完成顺序和发布形式的元层。

| Level | 负责 roadmap | 剩余工作 |
|---|---|---|
| L1 | [effect-system-capability](../../../../../roadmap/active/effect-system-capability.md) | 区分已有 compile check 与 artifact/host 边界剩余提案 |
| L2 | [wasm-platform-frontier](../../../../../roadmap/active/wasm-platform-frontier.md) | 明文规定 deterministic profile（NaN 审计 + 不使用 relaxed SIMD 的声明） |
| L3 | `proofs/receipt.sh` / `TRUSTED_BASE.md` | 明确当前 Rocq/structural 证明范围与未证明连接 |
| L4 | almide-dojo（独立仓库） | MSR 公开形式与第三方复现步骤 |
| L2/L3 的认证级加固 | [certification-grade](../../../../../roadmap/active/certification-grade.md) | CG-1～CG-5（ALS + 规范语义／coverage／Critical profile／translation validation／dossier） |
| 性能测量（非保证） | [wasm-optimization-roadmap](../../../../../roadmap/done/wasm-optimization-roadmap.md) | 4 losses burndown → 11/11 |

性能比较仅限已测工作负载、版本与设置。原来的 11/11 结果不能推出安全机制始终零成本，或所有级别的主张都无条件成立。

<a id="critical-path"></a>

## 关键路径

按依赖顺序。perf burndown 与 Perceus Phase A 可以独立并行。

1. **capability Phase 1+2**——L1 本体。`manifest.json` 是运营者 load 前首次可以验证的“收据”。仅 Phase 1 就能说明“声明能力的编译期检查”，但不能保证整个执行／构建 sandbox。
2. **明确 deterministic profile**——就是 frontier 文档的 TODO。工作量小，可取得 L2 的规范词汇。
3. **receipts harness（`make verify-trust` / `make receipt`，既有）**——在各自声明范围内重新导出 receipt 所列检查与证据：每项 claim 的依据、目标 fixture、decline、trusted boundary。原有 62 files / 11 bench / oracle registry 数值不是当前覆盖清单。**以可验证性为卖点的语言，其 claim 本身必须可验证。**
4. **向外公开 Trust Levels**——将此文件变成标准文档并发布，让其他工具链可用于自评（让别人使用标准而取胜）。
5. **MCP stdlib module**（[capability Phase 5](../../../../../roadmap/active/effect-system-capability.md)）——分发切入口。“Claude Code 配置 3 行就能运行 capability-bounded agent”是类别的首个 demo，也是面向买方（代理基础设施开发者）的入口。
6. （并行）**perf burndown** → 按工作负载测量确认改善。
7. （并行）**扩大证明范围** → 更新 L3 对象与未证明边界，不称为整体完成。

<a id="何をしないか"></a>

## 不做什么

- **通用生态竞争**——不只靠库的广度竞争。当前 API 参见 [stdlib index](../../../../../stdlib/README.md)，不使用固定的旧函数数量。
- **FFI/native 边界**——`ffi` 和 native dependency 实际存在，不能把“没有用户 FFI”当作 L1-L3 的前提。使用时的权限和信任范围遵循 [package system](../../../../../specs/package-system.md) 与当前 CLI 规范。
- **操纵标准迎合自身**——各级别维持竞争者也能部分达成的诚实定义。在表中写出 Deno 满足 L1 一半这一事实，本身就是标准可信度所在。

<a id="リスク-正直に"></a>

## 风险（坦诚列出）

| 风险 | 性质 | 应对 |
|---|---|---|
| 数据逆风（LLM 在大量 Python/TS 上训练） | 整个项目的赌注 | dojo 用数字展示赌注成败。持续测量 in-context（CHEATSHEET + skill）能否胜过预训练熟悉度 |
| 类别未成立（无监督运行市场不出现） | 时机风险 | 按代理基础设施当前轨迹，是“何时”的问题。视为抢先积累收据的时间竞赛 |
| 实验室推出 agent 语言 | 最大竞争情景 | 功能可模仿，但补装方法论（oracle pairing / ratchet / daily MSR / Lean）成本高。先行收据本身就是防御 |
