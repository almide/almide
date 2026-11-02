<a id="fan-v2--reference-examples"></a>

# Fan v2——参考示例

[![English](https://img.shields.io/badge/English-d0d7de?style=flat-square)](fan-v2-examples.md) [![日本語](https://img.shields.io/badge/%E6%97%A5%E6%9C%AC%E8%AA%9E-d0d7de?style=flat-square)](fan-v2-examples.ja.md) [![简体中文](https://img.shields.io/badge/%E7%AE%80%E4%BD%93%E4%B8%AD%E6%96%87-24292f?style=flat-square)](fan-v2-examples.zh-CN.md)



> 原文：[docs/roadmap/active/fan-v2-examples.md](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/active/fan-v2-examples.md)，固定提交 `35297a612fb29e3f78f6587aba253aa91d51f466`。这是历史路线图快照的非规范性读者译文，不自动同步。日期、状态、估算、测量和计划均属于原记录，不代表新增实现或独立验证。历史审批请求和建议命令不是现在的执行授权。以原文为准。

> 原文描述元数据：fan v2 表面的参考示例，涵盖每种头部、边界语义和诊断。

> 此索引保存引入 fan v2 时的设计示例及转录为 fixture 的计划。
> `fan-v2-examples/` 中的 `.almd` 是历史草图，不是当前语法或诊断的权威来源。不要把旧 `any` / `race` 示例复制进当前程序。

<a id="1-現在の参照先"></a>

## 1. 当前参考来源

- 语法及语义：[SPEC §13](../../../../../SPEC.md#13-concurrency-fan)。
- 选择名称和 clock：[ADR-0028](../../../../../adr/0028-fan-selection-names-and-cost-contract.md)——`fan.any` 改用 `??` / `list.find_map`，`fan.race` 改用必须有 clock 的 `fan.min`。
- 已实现表面及拒绝形式：`tests/fan_surface_matrix_test.rs`。
- 回归示例：`spec/wasm_cross/fuel_bounded_*.almd`、`fuel_race_*.almd` 等。文件名保留旧 `race` 与旧语法仍可使用，是两回事。
- `fan.timeout(duration.ms(n)) { body }` 作为 body 形式存在。下文 Stage 4/tombstone 是当初的实现顺序，不是当前未实现清单。

此处不重新抄录代码，以可执行 fixture 为准。

<a id="2-歴史的な設計例の索引"></a>

## 2. 历史设计示例索引

| 文件 | 内容 | 当初阶段 |
|---|---|---|
| [bounded.almd](../../../../../roadmap/active/fan-v2-examples/bounded.almd) | 全域化惯用法、可写 `let` 的单一 body、`0ms`/发散边界、min-cap 嵌套 | Stage 2 |
| [race.almd](../../../../../roadmap/active/fan-v2-examples/race.almd) | 组合求解、**全部 8 种情况的行为表**（并列、trap 可见窗口、耗尽）、双语法解析、mapper 形式 | Stage 3 |
| [any.almd](../../../../../roadmap/active/fan-v2-examples/any.almd) | 配置回退链、依次尝试镜像；“不是并发而是回退”的语义注记 | Wave 1 |
| [settle.almd](../../../../../roadmap/active/fan-v2-examples/settle.almd) | 异构 tuple 的 block 形式、收集所有错误的 mapper 形式 | Wave 1 |
| [fan_all_unchanged.almd](../../../../../roadmap/active/fan-v2-examples/fan_all_unchanged.almd) | 引入时不变表面的对照；C-004/C-199/C-200 的存续参照点 | 引入时的既有表面 |
| [diagnostics.almd](../../../../../roadmap/active/fan-v2-examples/diagnostics.almd) | 6 个**不能编译才是正确的**示例及预期诊断（pure 约束、ticks 标签、thunk tombstone、E008 等） | Wave 1～Stage 3 |
| [migration.almd](../../../../../roadmap/active/fan-v2-examples/migration.almd) | v1 → v2 对照（any/settle/race/timeout）；funcref wall 消除注记 | Wave 1～Stage 4 |

<a id="3-当初の-fixture-転写計画現在の網羅性一覧ではない"></a>

## 3. 当初的 fixture 转录计划（不是当前覆盖清单）

| 示例 | 转录目标 | 固定的契约 |
|---|---|---|
| bounded.almd 基本形式及边界 | `spec/wasm_cross/fuel_bounded_*.almd` | bounded 耗尽等价（新 C-NNN）+ consumed ratchet |
| race.almd 行为表的 8 行 | `spec/wasm_cross/fuel_race_*.almd`（每行 1 个 fixture） | 胜者等价、trap 可见窗口、并列时按源码顺序 |
| bounded.almd 嵌套 | `spec/wasm_cross/fuel_nest_*.almd` | min-cap 与 streaming 消耗 |
| any.almd / settle.almd | `spec/lang/fan_any_block_test.almd` 等（Wave 1，不需 fuel） | any/settle 的 form 契约 |
| diagnostics.almd 全部 6 项 | `tests/diagnostics/` 中的 fixture（每个 E0xx 一项） | 诊断文本 |
| fan_all_unchanged.almd | 既有 fixture 保持不变本身就是回归门禁 | C-004 / C-199 / C-200 |

新增或更新当前示例时，应更新 `spec/` 的 fixture 及当前 SPEC。
只向这组历史草图追加内容，不能成为当前语法或契约的接受证据。
