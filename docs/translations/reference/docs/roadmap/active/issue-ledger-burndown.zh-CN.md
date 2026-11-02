# Issue 台账燃尽——完整性行动的剩余工作量表

[![English](https://img.shields.io/badge/English-d0d7de?style=flat-square)](issue-ledger-burndown.md) [![日本語](https://img.shields.io/badge/%E6%97%A5%E6%9C%AC%E8%AA%9E-d0d7de?style=flat-square)](issue-ledger-burndown.ja.md) [![简体中文](https://img.shields.io/badge/%E7%AE%80%E4%BD%93%E4%B8%AD%E6%96%87-24292f?style=flat-square)](issue-ledger-burndown.zh-CN.md)



> 原文：[docs/roadmap/active/issue-ledger-burndown.md](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/active/issue-ledger-burndown.md)，固定提交 `35297a612fb29e3f78f6587aba253aa91d51f466`。这是历史路线图快照的非规范性读者译文，不自动同步。日期、状态、估算、测量和计划均属于原记录，不代表新增实现或独立验证。历史审批请求和建议命令不是现在的执行授权。以原文为准。

> 原文描述元数据：Issue 台账燃尽指标，使未关闭 issue 数诚实反映剩余工作量。

> 截至 2026-06-11。台账→tracker 的单射已完成：**GitHub 未关闭 issue 数 = 已知的完整性剩余工作量**。关闭纪律 = 消灭整类问题（每个 issue 已写明结构性关闭条件，不以补丁关闭）。停止条件 = #535 的 loop-until-dry。

## 成果（2026-06-10～11 两天）

发布：v0.26.20 → **v0.27.3**（6 次）。关闭：#484-490 系列 + #500/501/502/505 + #522/523/524/525/526/527/517（全部通过消灭整类问题）。
PR：#487～#543。矩阵门禁 0→29 个单元，churn 门禁 0→3 个，契约 58→67（含 C-066），wasm 内存模型从“全部泄漏”到“默认 O(1)”。

## 剩余 15 项的分布

### 周级——8 项（1 项 = 1 次集中会话）

| issue | 内容 | 关闭后改变什么 |
|---|---|---|
| #514 | 解决 fan race/any/map-err 分歧（align or contract） | 清空 silent-wrong 监视槽 |
| #515 | §3 变形 binding 门禁 | 机器验证 let⟺assign 接受等价性 |
| #516 | §9 将 interp 第三裁判接入 fuzzer（Oracle trait） | 让“两目标同错”的缺陷可见 |
| #521 | §8 stdlib @semantics 清单 | 将 doc↔impl 一致性变成 regen-and-diff 门禁 |
| #531 | §4 2c CopyClass 规范化（含行为评审） | Copy 谓词 4 个 → 1 个 |
| #532 | §7 fmt 多模块重新类型检查 + §10 release 提升 | 完成门禁覆盖 |
| #533 | §13+PR-3 分歧队列 + []-typing | 已记录跨目标分歧归零 |
| #534 | 规范矛盾燃尽（??/Never/fan 顺序/空页面） | prose↔行为一致 |

### 数周级（骨架）——6 项

| issue | 内容 | 规模 |
|---|---|---|
| #528 | QualifiedRef newtype（将 #433 机械化，应用 §4 循环） | 1–3 周 |
| #529 | WasmIR 中间层（通过构造确保 emitter 结构不变量） | 2–3 周 |
| #530 | 提升 ALS 规范性规格（CG-1） | 1–2 周 |
| #518 | construct-from-temp 过量计数（Koka 式精细 dup/drop） | 偏研究 |
| #519 | 分配器策略（单调增长 acc × first-fit） | 偏研究 |
| #520 | 扩展 Lean 的运行时 frees（分配器状态机证明） | 偏研究 |

### 流程——1 项

- **#535 loop-until-dry**：轮换视角开展 hole-hunt，连续 2–3 轮没有发现后，才称“按当前知识已完整”。第 1 轮（multi-truth / silent-fallback / rule-bypass）发现 6 类 → 已全部处理。**第 2 轮未执行**。下一视角候选：pass 顺序假设、checker-accepts-lowering-reinterprets、interp 语义差异、诊断分歧、哈希顺序以外的宿主环境依赖。

## 估算

- 已知缺口归零（周级全部完成）：**4～7 天**（按实际速度换算）。
- 可称为“完整”的状态（3 条骨架 + hunt 无新发现）：**另加 3～6 周**。

## 建议顺序

#515/#516（已设计、可立即开始）→ #514（清空 silent-wrong 槽）→ #531/#532/#533/#534
→ hunt 第 2 轮 → 骨架（#528 → #529 → #530）。
