# 测试覆盖面 25×——迈向参考编译器规模

[![English](https://img.shields.io/badge/English-d0d7de?style=flat-square)](test-surface-25x.md) [![日本語](https://img.shields.io/badge/%E6%97%A5%E6%9C%AC%E8%AA%9E-d0d7de?style=flat-square)](test-surface-25x.ja.md) [![简体中文](https://img.shields.io/badge/%E7%AE%80%E4%BD%93%E4%B8%AD%E6%96%87-24292f?style=flat-square)](test-surface-25x.zh-CN.md)



> 原文：[docs/roadmap/active/test-surface-25x.md](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/active/test-surface-25x.md)，固定提交 `35297a612fb29e3f78f6587aba253aa91d51f466`。这是历史路线图快照的非规范性读者译文，不自动同步。日期、状态、估算、测量和计划均属于原记录，不代表新增实现或独立验证。历史审批请求和建议命令不是现在的执行授权。以原文为准。

> 原文描述元数据：将已提交的测试覆盖面扩大 25 倍，迈向参考编译器规模。

目标设于 2026-08-17，此前参考测试集挖掘调查（#1508）测出了差距：rust 有 26,515 个测试文件，swift 有 13,051 个，lean4 有 4,461 个，而我们约有 1,000 个已提交 .almd 程序及 2,198 个 `#[test]` 函数。目标：**当日已提交覆盖面的 25×**，按能提升正确性的顺序增长，而不是为了数量。

## 基线（2026-08-17）

| 层级 | 数量 |
|---|---|
| 跨目标差分 fixture（spec/wasm_cross） | 500 |
| 其他 .almd 程序（spec/integration 等） | 497 |
| rust `#[test]` 函数 | 2,198 |
| 诊断 fixture（broken/fixed 对） | 99 |
| wall 语料（固定的拒绝） | 16 |
| 契约 / 门禁 | 291 / 54 |

计量器：`bash scripts/test-surface-meter.sh`（用 `--update` 追加带日期的趋势行）。

## 哪些计入 25×（按优先级）

1. **负向／诊断测试**——相对 rust 最薄弱的层级（99 对，对方约 15k 个 ui 测试）。每个 E-code 都需要一个 fixture 家族：每种提示变体、每种 fix-it 判定、每种 span 形状。首要目标：在其他层级翻倍前，将此层扩大 10×。解析器底层错误（#1471、#1509、#1510）归于此处。
2. **跨目标 fixture**——继续通过提炼而非堆量增长：每项 fuzz 发现都落成 fixture（持续规则），每个跨越的 wall 都带着契约落地，每个变得可表达的参考测试集模式（or-patterns、string patterns——#1508 列表）都在该功能落地的版本移植。
3. **预期输出 RC 快照**（koka parc 模型）：针对对 RC 关键的形状语料，将 drop/dup 位置与运行结果一起提交，让当前无害的位置变化也能醒目地暴露。
4. **类型×操作矩阵**——推广嵌套容器矩阵（nested_*.almd）：生成（构造 / 索引 / 修改 / 迭代 / 判等 / 打印 / drop）×（List/Map/Option/Result/tuple/variant 嵌套）网格，提交 CLEAN 单元，固定 wall。生成器保留在仓库内，以便 wall 被跨越后网格重新增长。
5. **未提交层级已约为 ∞**：xtarget-fuzz 每次运行生成并差分检查数千程序；单纯已提交数量低估了有效覆盖面。已提交语料用于固定回归和评审，fuzzer 用于发现。两者都增长，只有前者计入 25×。

## 非目标

- 批量提交从未由失败提炼而来的机器生成 fixture（增加套件运行成本，没有评审价值）。
- 移植 Almide 尚无功能的参考测试（在 #1508 跟踪，随功能一起加入）。

## 趋势

| 日期 | wasm_cross | 其他 almd | #[test] | 诊断对 | walls |
|------|-----------|------------|---------|------------|-------|
| 2026-08-17 | 500 | 497 | 2198 | 99 | 16 |
| 2026-08-18 | 501 | 497 | 2199 | 335 | 16 |
| 2026-08-18 | 502 | 497 | 2199 | 341 | 16 |
| 2026-08-18 | 572 | 497 | 2199 | 717 | 16 |
| 2026-08-18 | 572 | 497 | 2199 | 722 | 16 |
| 2026-08-18 | 585 | 498 | 2199 | 728 | 18 |
