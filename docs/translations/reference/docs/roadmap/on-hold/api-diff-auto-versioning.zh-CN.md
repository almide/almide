<a id="api-diff--automatic-versioning"></a>

# API 差异与自动版本管理

[![English](https://img.shields.io/badge/English-d0d7de?style=flat-square)](api-diff-auto-versioning.md) [![日本語](https://img.shields.io/badge/%E6%97%A5%E6%9C%AC%E8%AA%9E-d0d7de?style=flat-square)](api-diff-auto-versioning.ja.md) [![简体中文](https://img.shields.io/badge/%E7%AE%80%E4%BD%93%E4%B8%AD%E6%96%87-24292f?style=flat-square)](api-diff-auto-versioning.zh-CN.md)



> 这是[固定原文](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/on-hold/api-diff-auto-versioning.md)的非规范读者译文，是不自动同步的历史固定快照。状态、计划和数量均为原文时点的记录，不证明当前实现或完成情况。本次翻译未独立执行或验证这些主张。

> 原文说明：在已提供的 stdlib 接口检查关卡之外，面向包的 API 差异与版本建议。

<a id="実装済みの土台"></a>

## 已实现的基础

标准库的公开 API 差异已经由 [`scripts/check-interface-diff.sh`](../../../../../../scripts/check-interface-diff.sh) 检查。该发布关卡比较 git ref 之间生成的签名索引，将差异分类为 `identical` / `additive` / `breaking`，并使未经明确允许的 breaking change 检查失败。

[`check-interface-diff-negative.sh`](../../../../../../scripts/check-interface-diff-negative.sh) 使用 pure / effect 函数的删除和添加作为负对照。此机制已存在，但不是面向一般包的 CLI，也不是自动版本更新。

<a id="残る提案"></a>

## 剩余提案

- 通过面向用户的 CLI 比较包的公开 API。
- 确定考虑类型变量重命名等因素的结构比较规则。
- 根据差异给出 MAJOR/MINOR/PATCH 建议。即使签名不变，行为也可能变化，因此仅凭 API 差异不能保证整体兼容性。

以下是尚未实现 CLI 的设计示例。请勿将其介绍为当前可用的命令。

```text
almide diff v0.1.0..v0.2.0
# Added: yaml.parse_all (MINOR)
# Changed: yaml.stringify return type String → Result[String, String] (MAJOR)
# Suggested version: v1.0.0
```

<a id="設計参考"></a>

## 设计参考

Elm 的 `Deps/Diff.hs` / `elm diff`：类型签名的结构比较、考虑类型变量名的等价判定，以及模块／函数添加和类型变化的分类。现有 stdlib 关卡与此面向包的提案应分别界定目标和完成条件。
