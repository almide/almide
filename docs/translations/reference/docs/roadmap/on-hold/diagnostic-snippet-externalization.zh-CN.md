<a id="externalize-try-snippets-from-rust-literals"></a>

# 将 `try:` 片段从 Rust 字面量中外部化

[![English](https://img.shields.io/badge/English-d0d7de?style=flat-square)](diagnostic-snippet-externalization.md) [![日本語](https://img.shields.io/badge/%E6%97%A5%E6%9C%AC%E8%AA%9E-d0d7de?style=flat-square)](diagnostic-snippet-externalization.ja.md) [![简体中文](https://img.shields.io/badge/%E7%AE%80%E4%BD%93%E4%B8%AD%E6%96%87-24292f?style=flat-square)](diagnostic-snippet-externalization.zh-CN.md)

> 本文是[固定原文](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/on-hold/diagnostic-snippet-externalization.md)的非规范读者译文；原文仍是权威版本，本译文不会自动同步。




> 非规范性读者译本。固定于原文提交 `35297a612fb29e3f78f6587aba253aa91d51f466`，不会自动同步。状态、实现及测量描述均为原文时点的记录，并非翻译时的独立验证或现行保证；计划不代表已经实现。

说明：将诊断修复片段从 Rust 字面量迁移到经过检查的源码模板，尚余的工作。

<a id="current-state"></a>

## 当前状态

前置条件 [随附 Almide 分派](../../../../../roadmap/done/bundled-almide-dispatch.md) 已于 2026-04-16 提供，不再是阻碍。片段迁移本身仍是提案：目前不存在 `stdlib/diagnostics/` 模板目录。

修复文本仍在 Rust 中构造，例如：

- [`check/calls.rs`](../../../../../../crates/almide-frontend/src/check/calls.rs)：`with_try` 构造参数数量及重命名修复。
- [`check/solving.rs`](../../../../../../crates/almide-frontend/src/check/solving.rs)：`unit_leak_snippet` 提供结果表达式修复。
- [`stdlib.rs`](../../../../../../crates/almide-frontend/src/stdlib.rs)：`try_snippet_for_alias` 提供更丰富的别名修复。

这些只是示例，不是固定的文件／数量清单。偏离风险仍然存在：脱离语言解析器的文本可能教给读者过时的修复方式。

<a id="remaining-design-and-work"></a>

## 剩余设计与工作

1. 盘点当前修复生成位置，决定哪些完整片段可以成为经过检查的源码模板，而不是动态 Rust 格式化。
2. 引入模板目录（提议为 `stdlib/diagnostics/*.almd`）、明确的占位符替换以及编译器加载路径。随附分派可用本身并不意味着这个加载路径已经实现。
3. 迁移选定的修复，不改变其诊断含义，也不使片段可由用户扩展或本地化。
4. 添加测试框架：代入具体值、解析每个模板，并对每个被称为完整修复的模板进行类型检查。明确标记有意保留的片段，不要将其计为通过类型检查的程序。

例如，模板可以解释缺失的末尾值：

```almide
// placeholders: {fn_name}, {binding_name}, {ret_ty}
// Add {binding_name} as the trailing expression so the fn returns {ret_ty}:
//
//   let {binding_name} = <computation>
//   {binding_name}
```

这是提议的模板表示法，不是现有加载器或已通过的测试。只有模板和验证框架一同落地，迁移才算完成；仅完成分派前置条件并不能证明迁移完成。
