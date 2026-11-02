# LSP 代码操作 — 剩余重构

[![English](https://img.shields.io/badge/English-d0d7de?style=flat-square)](lsp-code-actions.md) [![日本語](https://img.shields.io/badge/%E6%97%A5%E6%9C%AC%E8%AA%9E-d0d7de?style=flat-square)](lsp-code-actions.ja.md) [![简体中文](https://img.shields.io/badge/%E7%AE%80%E4%BD%93%E4%B8%AD%E6%96%87-24292f?style=flat-square)](lsp-code-actions.zh-CN.md)

> 本文是[固定原文](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/on-hold/lsp-code-actions.md)的非规范读者译文；原文仍是权威版本，本译文不会自动同步。




> 非规范性读者译本。固定于原文提交 `35297a612fb29e3f78f6587aba253aa91d51f466`，不会自动同步。状态、实现及测量描述均为原文时点的记录，并非翻译时的独立验证或现行保证；计划不代表已经实现。

说明：在已提供的诊断快速修复之外，尚余的自动生成重构和导入管理覆盖范围。

## 已提供的基础

LSP 在 `src/cli/lsp.rs` 中声明并处理 `textDocument/codeAction`。`src/cli/lsp_code_actions.rs` 已提供：

- 面向有限已知模块列表的 E003 导入插入，而不是针对所有模块的通用自动导入解析。
- E006 将外围函数转换为 `effect fn`。
- 根据诊断的主要修复及备选方案生成快速修复编辑；可由机器应用的修复会标记为首选。

`tests/lsp_test.rs::lsp_try_fix_reaches_client_as_code_action` 和 `lsp_machine_applicable_repair_is_the_preferred_quickfix` 对修复的传递行为进行固定测试。添加另一项诊断修复，不需要再为同一编辑建立仅供 LSP 使用的第二份实现。

## 剩余提案 — 暂缓

- 根据编译器模块信息提供更广泛的导入管理，而不是依赖当前有限的 E003 列表。
- 安全的未使用绑定重构，保留初始化表达式中的任何效应。
- 根据穷尽性检查器的分支集合生成缺失的 match 分支。
- 根据已检查的类型插入函数签名类型注解。

这些是覆盖范围和自动生成重构的目标，并非声称所有代码操作都不存在。每项操作在被描述为可用之前，都需要编辑器协议回归测试以及接受／拒绝编辑测试。优先级和恢复此项工作的标准见 [arena 突破计划](../../../../../roadmap/active/arena-breakthroughs.md)。

## 设计参考

Gleam 的 `code_action.rs` 提供了导入管理、模式提取和注解插入示例；Nickel 的 `analysis.rs` 可供比较了解类型和契约的编辑器辅助功能。这些是设计参考，不是关于 Almide 当前覆盖范围的声明。
