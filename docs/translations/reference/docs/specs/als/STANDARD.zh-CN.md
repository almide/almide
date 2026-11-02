<a id="als-requirements-standard"></a>

# ALS 需求编写标准

[![English](https://img.shields.io/badge/English-d0d7de?style=flat-square)](STANDARD.md) [![日本語](https://img.shields.io/badge/%E6%97%A5%E6%9C%AC%E8%AA%9E-d0d7de?style=flat-square)](STANDARD.ja.md) [![简体中文](https://img.shields.io/badge/%E7%AE%80%E4%BD%93%E4%B8%AD%E6%96%87-24292f?style=flat-square)](STANDARD.zh-CN.md)

> 权威原文：[docs/specs/als/STANDARD.md](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/specs/als/STANDARD.md)，固定提交 `35297a612fb29e3f78f6587aba253aa91d51f466`。本页是非规范性的读者快照，不替代原 ALS 或检查器、台账、生成器输入，不自动同步。需求、历史状态、证据和安全主张均保留原文范围。本版未独立执行、测量、认证或证明。原文语言版为读者副本，其他语言版为翻译。


> 最后更新：2026-08-21

规定规范性章节如何编写、标识及退役。这是 ALS 的 DO-178C Table A-1 工件，即编写需求所依据的标准。`scripts/check-als-style.sh` 强制检查这里机器能检查的所有内容，其余属于评审纪律。

> **实现检出说明（2026-10-31；仅作解释，不是新需求政策）。** 本文保留以下 2026-08-21 的编写和评审纪律。所提及的 `scripts/check-als-style.sh`、`scripts/check-als-validation.sh`、`proofs/als-validation.toml` 在本检出版本中不存在；描述的强制执行，以及所有已记录评审者均为作者的说法，不是当前验证报告。现有的 [pin 门禁](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/scripts/check-als-pin.sh) 核对契约 ID，以及声明／标题与固定判定者是否一致；[元素覆盖门禁](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/scripts/check-als-element-coverage.sh)检查语法元素覆盖。两者均不能替代实质性的独立评审，也不能确立资格认定。本说明不改变任何契约、pin、编写规则或验收标准。

<a id="section-identity"></a>

## 节的标识

- 节标题为 `## ALS-<PREFIX><n>[a-z]? <title>`，例如 `## ALS-ST3 式文とコメント`。
- **一个前缀恰好属于一个章文件**（一章可以拥有多个前缀，但前缀不能跨章）。这种唯一映射使 `spec = "ALS-…"` 键没有歧义：2026-08-20 发现并重命名了 12 个跨章重复 ID，其中一个有三种含义，其引用已分散到全部三种含义中。
- 注册表（样式门禁解析此表）：

| 前缀 | 章 | 主题 |
|---|---|---|
| B | bounded.md | 有界配置 — `@bounded` 函数（ADR-0017） |
| C | collections.md | list / map / set 语义 |
| D | data-formats.md | JSON、Value、regex、二进制解码 |
| DT | deterministic-time.md | 时间代数、fan 预算、确定性竞争 |
| DL | expressions.md | 声明（`Decl::*`） |
| E | expressions.md | 表达式（`ExprKind::*`） |
| I | implementation.md | 实现定义的限制 |
| M | semantics.md | 模式匹配、绑定、泛型、循环 |
| R | runtime.md | process、fs、http、流式处理、显示 |
| S | strings.md | `string.*` 的码点语义 |
| ST | expressions.md | 语句（`Stmt::*`） |
| T | text-and-numbers.md | 数值／文本转换、边界、assert |

- ID **永久有效**，绝不重新编号，绝不复用。拆分节时分配新 ID；合并时保留一个 ID，将另一个永久退役（退役含义不得以另一个 ID 复活。2026-08-20：expressions 章的 `ALS-S4`（`Stmt::Comment`）合并到 `ALS-ST3`；`ALS-S4` 现在只指它此前也一直指向的 strings 章对应节，而 `Stmt::Comment` 永远是 `ALS-ST3`）。

<a id="writing-a-section"></a>

## 编写一个节

- **每节只有一种可观察行为。** 可观察 = stdout、stderr、退出码，或检查器判定（接受／带固定代码的拒绝）。不得包含无法对实现进行测试的内容。
- **以陈述语气写出事实。** 无计划、无“应该”、无「予定」、无含糊保留。尚未实现的设计放在 ADR 或路线图中，不放在这里。
- **节中指明证据**：引用该节的契约负责佐证，节本身指明测试路径（`テスト:` 行）。没有可执行证据的规范陈述不存在。
- 每节至少被一个契约引用（`spec =` 键），每个契约必须引用真实存在的节；双向都有门禁（`scripts/check-contracts.sh`）。
- 有意的目标间差异必须是**已提交的数据**（具名契约下的各目标取值表），不能回避，也不能用文字含糊处理。
- 各章以 `> Last updated: YYYY-MM-DD` 开头。

<a id="validation-record-do-178c-a-3"></a>

## 验证记录（DO-178C A-3）

样式门禁负责机器能检查的部分。一个节是否**准确、一致、完整**是评审者的判断，未记录的判断就不存在。`proofs/als-validation.toml` 为每个已评审节保存一行：`id`、`hash`（评审覆盖的节文本；由 `bash scripts/check-als-validation.sh --stamp ALS-<id>` 打印）、`reviewed`（日期）、`by`、`independent`（仅当评审者不是作者时为 `yes`）、`verdict`（`accurate`，或带 `issue = "#N"` 的 `revise`）。文本改变会使该行过期，重新评审前门禁为红。没有记录行的节是**未验证**的，其数量上限只允许缩小：新节随记录行合入，或者在 PR 中说明理由后将上限恰好增加一。门禁为 `scripts/check-als-validation.sh`。目前记录中的每次评审都由作者完成（QUALIFICATION.md，限制 1）；台账让这一点可见，而不是假装并非如此。

<a id="ratchets-the-four-direction-law"></a>

## 棘轮（四向法则）

本仓库只允许缩减的台账必须在四个方向上失败，而非仅一个：(1) 数量不能在没有作者明确写出的具名理由时增长；(2) 理由已不成立的行失败；(3) 对象不再存在的行失败；(4) 测量数量为零的测量失败——损坏的测量工具不是通过。

<a id="forbidden-vocabulary-normative-chapters"></a>

## 禁用词汇（规范章）

`TODO`、`FIXME`、「たぶん」（大概）、「そのうち」（以后某时）、「予定」（计划）——需要这些词的规范句子尚未准备好成为规范。
