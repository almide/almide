# CI 警告清理

[![English](https://img.shields.io/badge/English-d0d7de?style=flat-square)](ci-warnings-cleanup.md) [![日本語](https://img.shields.io/badge/%E6%97%A5%E6%9C%AC%E8%AA%9E-d0d7de?style=flat-square)](ci-warnings-cleanup.ja.md) [![简体中文](https://img.shields.io/badge/%E7%AE%80%E4%BD%93%E4%B8%AD%E6%96%87-24292f?style=flat-square)](ci-warnings-cleanup.zh-CN.md)



> 原文：[docs/roadmap/active/ci-warnings-cleanup.md](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/active/ci-warnings-cleanup.md)，固定提交 `35297a612fb29e3f78f6587aba253aa91d51f466`。这是历史路线图快照的非规范性读者译文，不自动同步。日期、状态、估算、测量和计划均属于原记录，不代表新增实现或独立验证。历史审批请求和建议命令不是现在的执行授权。以原文为准。

> 原文描述元数据：持续记录 CI 容忍的编译器警告，以及将各类别降至零的计划。

> **状态**：Active（进行中）
> **观测版本**：v0.23.10，Windows 和 macOS CI
> **影响**：CI 日志噪声，4 条 ICE 消息（非致命）

## 概要

CI 输出在 spec 测试中包含约 40 条警告和 4 条 ICE 消息。所有测试均通过——这些是诊断噪声，而非失败。但 ICE 消息表明单态化存在真实缺口。

## 类别

### 严重：ICE——单态化后仍有 TypeVar（4 个文件）

```
[ICE] N TypeVar(s) remain after monomorphization. Generic params should be fully substituted.
```

| 文件 | 剩余 TypeVar 数 |
|------|--------------------|
| `spec/integration/codegen_patterns_test.almd` | 2 |
| `spec/lang/generics_test.almd` | 3 |
| `spec/lang/regression_v0_11_test.almd` | 6 |
| `spec/lang/type_system_test.almd` | 9 |

**根本原因**：用户定义的泛型函数（`first[T]`、`zip_with[A,B,C]`、`unwrap_or[T]`）没有完全单态化。mono 阶段没有替换所有类型参数。

**相关**：`spec/lang/function_test.almd` 也显示还有 1 个 TypeVar（泛型阶乘？）。

### 严重：POSTCONDITION VIOLATION——Lambda 参数未解析（1 个文件）

```
[POSTCONDITION VIOLATION] Lambda param VarId(30) still unresolved: ir=Unknown vt=Unknown
[POSTCONDITION VIOLATION] [ConcretizeTypes] 3 expressions remain with unresolved types.
```

文件：`spec/lang/single_quote_test.almd` 或附近（函数 `__test_almd_override`）

**根本原因**：参数类型为 Unknown 的 lambda 在 ConcretizeTypes 后仍然存在。覆盖／遮蔽模式可能干扰类型解析。

### 噪声：未使用变量警告（约 25 处）

spec 测试文件仅为副作用使用变量（例如 `for (k, v) in m { ... }` 只使用 `v`）。

**修复方案**：在 spec 文件中添加 `_` 前缀，或在测试模式中抑制。

### 噪声：未使用导入警告（4 处）

- block_comment_raw_string_test 中的 `value` 导入
- stdlib 测试中的 `fs`、`env`、`process` 导入

**修复方案**：删除未使用导入或添加 `_` 前缀。

### 噪声：E015——stdlib 遮蔽警告（5 处）

```
warning[E015]: fn 'factorial' has the same signature as stdlib `math.factorial`
```

文件：`codegen_stress_test`、`function_test`、`generics_test`、`regression_v0_11_test`、`type_system_test`

**不是缺陷**：这些是测试文件中有意的重新实现。可以考虑在测试块中抑制 E015。

## 建议行动

1. **修复 ICE**：调查用户定义泛型的 mono 阶段——TypeVar 替换不完整。
2. **修复 POSTCONDITION**：覆盖模式中的 lambda 参数类型解析。
3. **抑制测试噪声**：为未使用变量添加 `_` 前缀，删除未使用导入。
4. **考虑**：为 CI 测试步骤提供 `--quiet-warnings` 标志，或在 `test` 块中抑制 E015。
