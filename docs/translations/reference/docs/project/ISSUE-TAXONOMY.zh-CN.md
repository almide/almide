<a id="issue-label-taxonomy-1482"></a>

# Issue 标签分类（#1482）

[![English](https://img.shields.io/badge/English-d0d7de?style=flat-square)](ISSUE-TAXONOMY.md) [![日本語](https://img.shields.io/badge/%E6%97%A5%E6%9C%AC%E8%AA%9E-d0d7de?style=flat-square)](ISSUE-TAXONOMY.ja.md) [![简体中文](https://img.shields.io/badge/%E7%AE%80%E4%BD%93%E4%B8%AD%E6%96%87-24292f?style=flat-square)](ISSUE-TAXONOMY.zh-CN.md)



> 原文：[docs/project/ISSUE-TAXONOMY.md](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/project/ISSUE-TAXONOMY.md)（固定提交 `35297a612fb29e3f78f6587aba253aa91d51f466`）。这是面向读者的非规范性快照译文，不会自动同步。保留原文的时间、适用范围与主张；翻译时未进行独立测量或实现验证。

在本文件出现之前，跟踪器已有类别标签（`bug`、wasm 类别、`fuzz` 等），却**没有严重性类别**，因此无法查询“是否还有未关闭的发布阻塞问题？”——每次发布都靠记忆回答。本文件定义封闭的严重性集合、各类别的准入条件，以及使用它们的一条规则；下文还定义领域轴，用来回答另一个无法查询的问题：**这个 issue 属于哪个 crate？**

<a id="the-closed-set-severity"></a>

## 封闭集合（严重性）

| 标签 | 精确适用范围 | 示例 |
|---|---|---|
| `I-unsound` | 违反保证主干：被 `almide check` + v1 证书接受的内容破坏内存安全或经过验证的所有权契约（use-after-free、double-free、证明了错误事项的证书）。 | 释放仍存活句柄的 drop 路径 |
| `I-miscompile` | 被接受的程序在某个目标上算出**错误的值**——包括两个目标一致但都错误的情况（interp 第三判定器类别）。wall 不是误编译：如实拒绝意味着不属于此类别。 | `5 \|> half ?? -1` 打印 `0` |
| `I-divergence` | 对于被接受的程序，native / wasm / interp 的**可观察输出**（stdout、stderr、退出码）不一致。契约台账的逐字节一致承诺被破坏。 | 只有一端的 `list.unique` 在 `-0.0` 之后保留 `0.0` |
| `regression` | 在**已发布**版本中正常的行为在 HEAD 上损坏。若适用，应与 `I-*` 类别配对。 | 测试夹具在 v0.57.0 通过，在 develop 失败 |
| `P-critical` / `P-high` / `P-low` | 排期优先级。与 `I-*` 轴正交，本身**没有阻塞效力**。 | — |

使用规则：

- `I-*` 标签表示类别**已确认**（已复现，或由失败的测试夹具固定）——在验证前，怀疑不加标签。
- 一个 issue 可以有多个类别（`I-divergence` + `regression`）。
- wall、缺失功能、性能差距和文档漂移**不是** `I-*` 类别；它们使用 `enhancement` / `documentation` 加领域标签（见下文）。

<a id="the-blocking-rule"></a>

## 阻塞规则

**只要带有 `I-unsound`、`I-miscompile`、`I-divergence` 或 `regression` 标签的 issue 仍未关闭，就不得发布正式版标签。**

- 由 [`scripts/count-release-blockers.sh`](../../../../../scripts/count-release-blockers.sh) 计数；发布 workflow 在创建正式发布之前使用 `--gate` 运行它，因此规则由机制执行，而不是靠记忆。它打印的是**不同**未关闭 issue 的数量：一个同时带两个类别的 issue 是一个阻塞项，不是两个（#2400）。总数上方的按标签分项会显示重叠。
- `-rc` 预发布标签例外（RC 的存在正是为了让一个代码树接受 soak 测试——参见 `CLAUDE.md` 中的 RC 流程）；仍然打印计数。
- 退出途径是**降级，而非豁免**：若判定某个阻塞项对本次发布可接受，应由 mob 决策，在 issue 上记录理由并移除或更改标签，同时在 release seal 的 `known_problems` 字段记录处理结果。门禁本身没有跳过它的参数。

<a id="the-area-axis-which-crate"></a>

## 领域轴（哪个 crate）

严重性说明问题伤害有多大，却从未说明**工作在哪里**。第一次打开跟踪器的读者，不读完 77 个正文就无法询问“wasm emitter 中还有什么未解决？”或“frontend 中还有什么未解决？”。`A-*` 轴回答这一问题，每个标签的 GitHub 描述都列出对应 crate，所以映射不需要第二份文档。

| 标签 | crate / 路径 |
|---|---|
| `A-frontend` | `crates/almide-frontend`、`almide-syntax` — 解析、检查、诊断、import 表 |
| `A-ir` | `crates/almide-ir`、`almide-mir`、`almide-optimize`、`almide-spine` |
| `A-codegen` | `crates/almide-codegen` — nanopass、Rust lowering、native 借用/RC 推断 |
| `A-wasm` | `crates/almide-wasm`、`almide-wasm-run`、`almide-wasm-vm` |
| `A-runtime` | `runtime/rs`、`crates/almide-rt-core` — native intrinsic |
| `A-interp` | `crates/almide-interp` — 第三判定器 |
| `A-stdlib` | `stdlib/*.almd`、`crates/almide-types` 标准库注册表 |
| `A-driver` | `crates/almide-driver`、`src/` — CLI：run/build/test/check/fmt/bench |
| `A-perf` | `research/benchmark/perf`、`scripts/check-perf-ratio.sh` |
| `A-ci` | `.github/workflows`、`scripts/*.sh` 门禁、台账和棘轮 |
| `A-modules` | 模块与包系统 — 解析、子模块、MVS、方言 epoch |

使用规则：

- 每个 issue 只有**一个** `A-*`；仅当工作确实跨越两个 crate 时才用两个。如果没有合适标签，是 issue 没有说明自己的位置——这是 issue 的缺口，不是缺少标签。
- 标签必须与 issue 自己的 `触る場所` 行（见下文）一致。它们是同一个主张写了两遍，不一致就意味着其中一个已过时。
- `A-wasm` 是原来的 `wasm-codegen`，原地重命名，因此保留了其 issue。没有脚本或 workflow 使用旧名称；而 `fuzz`、`fuzz-findings`、`fuzz-perf` 确实被 `.github/workflows/fuzz-nightly.yml` 使用，必须保留拼写。
- 领域轴**不像**严重性集合那样封闭：向仓库添加 crate 时可以添加标签。移除或调整范围仍须写入本文件，并与 `gh label` 的变更在同一个提交中完成。

<a id="the-issue-header-what-to-do-in-the-issue"></a>

## Issue 头部（在 issue 中说明做什么）

每个未关闭 issue 都在正文最上方、原始文字之前放置一个引用块，让只打开这一个 issue 的读者知道该做什么：

```
> **状態** — <BLOCKER / 実装 / 計測器・ゲート / 設計判断 / 追跡 / 調査>・<着手可 / 前提待ち: #NNNN / 判断待ち>
> **一行で** — one sentence
>
> **やること**
> 1. the first concrete action
>
> **完了条件** — a verifiable condition: a gate green, a measured number, a fixture, a diagnostic
> **根拠** — the measurement / file:line / PR that makes this real, or `未測定`
> **触る場所** — the 2-4 paths a fix would edit
```

`状態` 取六个值之一，每个值都有具体准入范围——目的是让浏览跟踪器的读者在阅读正文之前就能分辨 issue 的工作类型：

| 状態 | 精确适用范围 |
|---|---|
| `BLOCKER` | 带有 `I-unsound` / `I-miscompile` / `I-divergence` / `regression`：在未关闭时拒绝正式发布标签。与标签双向一致——没有这些标签之一的 `BLOCKER`，或没有 `BLOCKER` 的这些标签之一，都意味着其中一方已过时 |
| `実装` | 目标形态已知的编译器、标准库或运行时代码变更 |
| `計測器・ゲート` | 工作本身就是门禁、棘轮、manifest 或测量工具——包括修复不如实反映结果的此类工具 |
| `設計判断` | 编写任何代码之前需要裁决（通常也标有 `mob`） |
| `追跡` | 子项为其他 issue 的总括 issue，或 bot 维护的台账 |
| `調査` | 对象尚未确定；第一步是测量，不是编辑 |

该行后半部分是开始工作的门槛：`着手可`、`前提待ち: #NNNN`（仅指真正的前置条件——*相关*不算），或 `判断待ち`。

- `根拠` 是支撑判断的字段：它说明 issue 建立在测量之上，还是仅仅建立在看似合理的一句话上。`未測定` 是合法值，而且比自信猜测更有用——它告诉下一位读者，第一步是测量。
- `完了条件` 绝不写“感觉完成了”。如果 issue 无法给出可验证的终点，那么它的完了条件就是需要什么才能*做出决定*——这就是工作。
- 该块是包装层：不添加正文尚未包含的主张。新证据仍像以往一样放进评论。

<a id="amending-the-set"></a>

## 修改集合

严重性集合有意保持封闭——随意增长的分类最终会失去意义。添加类别、更改准入条件或更改哪些类别会阻塞，都是 mob 决策，必须在同一提交中修改本文件与 `scripts/count-release-blockers.sh` 来记录。
