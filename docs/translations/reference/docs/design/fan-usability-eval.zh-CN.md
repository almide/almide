<a id="fan-usability-evaluation-can-a-writer-pick-the-right-fan-and-does-it-misread-cancellation"></a>

# Fan 易用性评估：编写者能否选对 fan，是否误读取消行为？

[![English](https://img.shields.io/badge/English-d0d7de?style=flat-square)](fan-usability-eval.md) [![日本語](https://img.shields.io/badge/%E6%97%A5%E6%9C%AC%E8%AA%9E-d0d7de?style=flat-square)](fan-usability-eval.ja.md) [![简体中文](https://img.shields.io/badge/%E7%AE%80%E4%BD%93%E4%B8%AD%E6%96%87-24292f?style=flat-square)](fan-usability-eval.zh-CN.md)

> 非规范性历史阅读快照。以固定原文为准，不自动同步。未独立重跑测量、证明 gate 或当前实现检查。未执行引用命令。 [Source](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/design/fan-usability-eval.md)
> 这是评估设计，不是完成结果。开头称绝不读说明文字，但 misconception score 明确用短语列表检查注释／解释；保留两处原文陈述。不将历史 fan 名称和条件改写成当前 API 建议。

> 最后更新：2026-10-17 · 仅设计。与全部 MSR 工作一样，评估在 [almide-dojo](https://github.com/almide/almide-dojo) 运行（CLAUDE.md，“Repo Boundary”）。其结果决定 [ADR-0028](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/adr/0028-fan-selection-names-and-cost-contract.md) 的证伪条件。

<a id="what-is-measured"></a>

## 测量内容

评估回答两个问题。

1. **选择。** 给定用普通语言描述的任务，编写者（模型或人）是否选取选择类型匹配的 fan 形式？类型为 [fan-cross-language.md](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/design/fan-cross-language.md) 中 K1–K5。
2. **取消。** 编写者是否相信某事已停止或撤销，实际却并非如此？例如认为 `fan.map` 在首个 Err 停止、`fan.any` 取消已发出的请求，或 `fan.race` 提前停止败者。

两者都作为由 oracle 判定的*所写程序行为*来测量，绝不通过阅读编写者生成的文字来测量。

<a id="task-bank"></a>

## 任务库

每个任务由简短说明、starter file 和 hidden oracle 构成。说明从不点名 fan 形式。六个 family，每个六项任务（总计 36），一半“编写”，一半“修改”（MSR）。

| Family | 说明形态（示例） | 正确答案 | 典型错误答案 |
|---|---|---|---|
| F1 K1 | “按优先级尝试 mirror，使用首个回答者。” | `fan.any(mirrors, f)` | `fan.race`（期待时间上的最先） |
| F2 K4 | “几个 solver 都纯；选工作量最少的，在每台机器上答案相同。” | `fan.race { … }` | `fan.any`（取第一个而非最便宜的） |
| F3 K5 全部或错误 | “获取每一页；任何页面失败则失败，并报告是哪一页。” | `fan.map(xs, f)!` | 说明要求尝试每一页，却提前停止的 `for` loop |
| F4 K5 全部结果 | “运行全部检查，打印含各检查结果的报告。” | `fan.settle(xs, f)` | `fan.map`（丢失逐元素结果） |
| F5 不可表达 | “使用最先回答的 server，忽略慢者。”（K2），或“任一 job 完成就返回，即使失败。”（K3） | 说明不可表达，并说明权衡后使用 `fan.any` | 含 I/O arm 的 `fan.race`（E006），或声称慢 call 已取消 |
| F6 cancellation | “一个失败后停止剩余 upload。” | `for … !` loop，因为 `fan.map` 从不提前停止 | `fan.map`，并主张、注释或测试后续 upload 没有发生 |

修改任务从该 family 的正确程序开始，改变一个要求。例如：“错误必须是列表中首个页面，而非最早失败的页面”（原本已成立，无须修改），或“也保留失败结果”（`fan.map` → `fan.settle`）。

<a id="oracle"></a>

## Oracle

每个任务的 oracle 都是采用 `tests/fan_acceptance_test.rs` 形式的确定性 program：

- **每条 leg 输出相同。** 提交在四条 leg 运行：native、带 `ALMIDE_FAN_COST_OFF=1` 的 native、带 `ALMIDE_FAN_SEQUENTIAL=1` 的 native，以及 wasm。stdout 必须一致。
- **预期输出。** stdout 必须等于任务说明要求的预期输出。
- **慢与快的元素。** oracle 的 helper fn 让部分元素慢、部分元素快，使依赖完成顺序的答案失败。预期输出绝不依赖 wall time。
- **记录副作用。** 每个 effect 都是带 tag 的 `println`，oracle 因而能识别哪些元素运行过（F1、F6）。

每个任务有两个分数：

- **选择正确。** program 在两个 target 编译，oracle 的 stdout 匹配。
- **误解。** 当 program、其注释或编写者解释声称以下任一内容时记为发现：声称元素未开始或停止，但 oracle 显示它运行了；已取消请求；或 K2/K3 结果。Dojo 既有 malicious-hint detector 已读取注释，这里用固定短语列表（`cancel`、`abort the others`、`whichever finishes first`、`stops the rest`）复用它。

<a id="conditions"></a>

## 条件

| 条件 | 提供给编写者的 context |
|---|---|
| C0 | 当前 CHEATSHEET fan 节原文 |
| C1 | C0 加 ADR-0028 的 D-a hint 文本及 D-c gloss |
| C2 | 已接受的 ADR-0028：按顺序首个 Ok 使用 `??` / `list.find_map`，要求 clock 的 `fan.min(clock)`，带 D-a/D-c |

编写者使用当天 Dojo model roster。每个（task、condition、model）运行 5 个 sample。每行结果记录 model、sample 数及日期。

<a id="procedure"></a>

## 步骤

1. 在 Dojo 的 `tasks/fan/` 下冻结带版本的 task bank 和 oracle。
2. 对每种 condition 运行全部 task × 全部 model × 5 sample。在 CI 外进行，使用 operator 机器上的 API key（Dojo rule）。
3. 按 selection 和 misconception 为每次 run 评分，按 family 和 condition 报告，每个比率附总数（“总共多少个”）。
4. 比较 C0 → C1，查看诊断和 gloss 本身的收益；比较 C1 → C2，查看重命名的额外收益。
5. 应用 ADR-0028 证伪条件。如果 C2 未将 F1 + F2 + F5 误读率相对 C1 至少降低三分之一，则在 removal epoch 前重新考虑保留名称（option A）。

<a id="what-this-evaluation-does-not-claim"></a>

## 本评估不声称什么

- 测量所写 program，不是理解本身。
- 不测 latency；更慢但正确的 F1 答案仍通过。
- 每个 condition 评分 run 少于 30 的 family 仍报告，但不用于证伪条件。
