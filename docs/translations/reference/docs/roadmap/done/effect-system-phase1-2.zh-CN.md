<a id="effect-system--phase-1-2"></a>

# 效应系统 — 阶段1–2

[![English](https://img.shields.io/badge/English-d0d7de?style=flat-square)](effect-system-phase1-2.md) [![日本語](https://img.shields.io/badge/%E6%97%A5%E6%9C%AC%E8%AA%9E-d0d7de?style=flat-square)](effect-system-phase1-2.ja.md) [![简体中文](https://img.shields.io/badge/%E7%AE%80%E4%BD%93%E4%B8%AD%E6%96%87-24292f?style=flat-square)](effect-system-phase1-2.zh-CN.md)



> 这是[固定原文](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/done/effect-system-phase1-2.md)的非规范读者译文，是不自动同步的历史固定快照。状态、计划和数量均为原文时点的记录，不证明当前实现或完成情况。本次翻译未独立执行或验证这些主张。

> 原文说明：包含7个类别的效应推断引擎及检查器集成。
> 原文完成元数据：2026-03-19。

**完成：** 2026-03-19
**PR：** #49

<a id="implementation"></a>

## 实现

<a id="phase-1-effect-inference-engine"></a>

### 阶段1：效应推断引擎
- `EffectInferencePass` Nanopass — 7类效应（IO、Net、Env、Time、Rand、Fan、Log）
- stdlib 模块 → 效应映射（fs→IO、http→Net、env→Env 等）
- 直接效应收集（Module call + Named call + fan 表达式）
- 构建调用图，通过不动点迭代推断传递效应
- 在 IrProgram 中添加 `effect_map` 字段
- `almide check --effects <file>` CLI 命令
- 用 ALMIDE_DEBUG_EFFECTS=1 输出分析

<a id="phase-2-self-package-restriction-security-layer-2"></a>

### 阶段2：自身包限制（安全层2）
- 解析 `almide.toml [permissions] allow = ["IO", "Net"]`
- 通过 `almide check --effects` 检测违规并显示提示
- 集成到常规 `almide check` — 存在 [permissions] 时自动检测违规
- 在 `project.rs` 的 `Project` 结构体添加 `permissions` 字段

<a id="remaining-at-completion-2026-03-19"></a>

## 完成时剩余工作（2026-03-19）

原来的 `active/effect-system.md` 计划已不存在。当前能力路线图见[效应系统与能力](../../../../../roadmap/active/effect-system-capability.md)。以下阶段编号属于本完成记录。

- 阶段3：依赖限制（`[dependencies.X].allow`）→ 2.x
- 阶段4：类型层集成（与 HKT Foundation 阶段4合并）→ 2.x

<a id="tests"></a>

## 测试
- 5项内部测试（module_to_effect、runtime_name_to_effect、format_effects）
- 110/110 项 almide 测试
