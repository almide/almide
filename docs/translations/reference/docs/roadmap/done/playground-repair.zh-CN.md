<a id="playground-repair-turn"></a>

# Playground 修复轮次

[![English](https://img.shields.io/badge/English-d0d7de?style=flat-square)](playground-repair.md) [![日本語](https://img.shields.io/badge/%E6%97%A5%E6%9C%AC%E8%AA%9E-d0d7de?style=flat-square)](playground-repair.ja.md) [![简体中文](https://img.shields.io/badge/%E7%AE%80%E4%BD%93%E4%B8%AD%E6%96%87-24292f?style=flat-square)](playground-repair.zh-CN.md)

> 非规范性的固定读者快照。以原文为准，不自动同步，未独立重新验证实现、测量或安全性。
> [Source / 原文](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/done/playground-repair.md)
> 完成元数据与未勾选任务、v0.4.7 状态快照并存，应保留其历史区别。拟议的平均 1.2 轮只是指标示例，一轮修复是演示成功条件，不是当前实测保证。


> 原文描述：Playground 的 AI 错误修复与类型检查器集成
> 原文完成元数据：2026-03-11

<a id="vision"></a>

## 愿景

用户在 Playground 编写代码、运行、收到错误后，点击“Fix with AI”。LLM 读取错误并修复，用户观看修复过程。

展示 Almide“错误信息足够清晰，LLM 能在一轮内修复”的演示。

<a id="current-state-v047"></a>

## 当前状态（v0.4.7）

<a id="done"></a>

### 已完成
- [x] 向 Playground 添加类型检查器（parse -> **check** -> emit）
- [x] AI 生成流程中的修复轮次（生成 -> 错误 -> 自动修复 x 3）
- [x] 手动 Run 的“Fix with AI”按钮（Run -> 错误 -> 按钮 -> 修复循环）
- [x] Anthropic / OpenAI / Gemini 流式支持
- [x] repair-log UI（error/fix/ok/fail 步骤显示）

<a id="not-yet"></a>

### 尚未完成
- [ ] 将 Playground 的 almide 依赖更新到 v0.4.7（PR #15 合并后）
- [ ] 在 CLAUDE.md（system prompt）加入 list.swap 与 immutable 模式
- [ ] 显示修复结果差异
- [ ] 比较修复前后代码（并排或行内差异）
- [ ] “Accept fix”／“Reject fix”按钮

<a id="architecture"></a>

## 架构

```
User writes code
       │
       ▼
   ┌──────────┐
   │   Run     │
   └────┬─────┘
        │
   ┌────▼─────┐     ok     ┌──────────┐
   │ compile   │───────────▶│  Output   │
   │ + run     │            └──────────┘
   └────┬─────┘
        │ error
   ┌────▼──────────┐
   │ Show error +   │
   │ "Fix with AI"  │
   └────┬──────────┘
        │ click
   ┌────▼──────────┐
   │ LLM repair    │◀──┐
   │ (stream)      │   │ error (max 3)
   └────┬──────────┘   │
        │              │
   ┌────▼─────┐   ┌───┴────┐
   │ compile   │──▶│ retry  │
   │ + run     │   └────────┘
   └────┬─────┘
        │ ok
   ┌────▼─────┐
   │ Output + │
   │ "Fixed!" │
   └──────────┘
```

<a id="repair-prompt-strategy"></a>

## 修复提示词策略

当前：
```
{phase} error:
{error message}

Fix the code and output ONLY the corrected .almd source. No explanations.
```

改进版（TODO）：
- 在系统提示词加入 Almide 语法概述及常见错误模式
- `cannot reassign immutable binding` -> 决定使用 `var` 还是元组返回模式
- `list.get returns Option` -> 建议 `list.get_or` 或 `list.swap`

<a id="roadmap"></a>

## 路线图

<a id="tier-1--completeness-short-term"></a>

### Tier 1 — 完整性（短期）

<a id="11-update-playground-almide-dependency"></a>

#### 1.1 更新 Playground 的 almide 依赖
PR #15 合并后，更新 `Cargo.toml` 中的 almide git ref。
确认检查器能在 WASM 工作。

<a id="12-system-prompt-optimization"></a>

#### 1.2 系统提示词优化
在 Playground 的 LLM 修复提示词加入 Almide 特有模式：
- `var` 与 `let` 的用法差异
- 原地算法使用 `list.swap`
- `list.get` 返回 nullable → 使用 `list.get_or`
- 修改并返回的函数使用元组返回

<a id="13-diff-display"></a>

#### 1.3 差异显示
显示修复前后的代码差异。用户可一眼理解“改了什么”。
简单行内差异即可（新增行为绿，删除行为红）。

<a id="tier-2--ux-improvements-medium-term"></a>

### Tier 2 — UX 改进（中期）

<a id="21-accept--reject"></a>

#### 2.1 接受／拒绝
“Fix with AI”后暂时显示修复代码，由用户选择 Accept 或 Reject。
Reject 恢复原代码。

<a id="22-partial-repair"></a>

#### 2.2 局部修复
只高亮错误位置，只修复该函数。
对 LLM 而言，比完整重写更准确。

<a id="23-repair-history"></a>

#### 2.3 修复历史
按时间顺序显示多轮修复。
展示“尝试了什么、修复了什么”。

<a id="tier-3--benchmark-integration-long-term"></a>

### Tier 3 — 基准集成（长期）

<a id="31-modification-survival-rate-visualization"></a>

#### 3.1 Modification survival rate 可视化
记录修复轮数，显示“Almide 平均 1.2 轮修复”等指标。
将与其他语言的比较数据嵌入 Playground。

<a id="32-auto-repair-mode"></a>

#### 3.2 自动修复模式
给 Run 按钮添加“Auto-repair”开关。打开后，发生错误即自动开始修复。
用户实时观看修复过程。

<a id="success-metric"></a>

## 成功指标

Playground 中的 quicksort（immutable 模式）：
1. 用户用 mutable 模式编写 -> `cannot reassign immutable binding` 错误
2. “Fix with AI” -> LLM 使用 `var`＋元组返回＋`list.swap` 修复
3. 一轮内完成修复，显示排序结果

如果成功，就完成了“Almide 错误足够清晰，可供 LLM 修复”的演示。
