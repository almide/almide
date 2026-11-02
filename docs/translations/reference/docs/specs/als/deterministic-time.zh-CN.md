<a id="als--決定的時間deterministic-time"></a>

# ALS — 确定性时间（deterministic time）

[![English](https://img.shields.io/badge/English-d0d7de?style=flat-square)](deterministic-time.md) [![日本語](https://img.shields.io/badge/%E6%97%A5%E6%9C%AC%E8%AA%9E-d0d7de?style=flat-square)](deterministic-time.ja.md) [![简体中文](https://img.shields.io/badge/%E7%AE%80%E4%BD%93%E4%B8%AD%E6%96%87-24292f?style=flat-square)](deterministic-time.zh-CN.md)

> [固定原文](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/specs/als/deterministic-time.md)的非规范读者快照。下文的规范地位和要求属于原文，不属于本副本。不自动同步；本次翻译未独立复核历史实现、符合性及安全主张。代码与规则标识符保持不变。

> Last updated: 2026-08-27

规范化来源：[ADR-0001](../../../../../adr/0001-deterministic-time-units.md)（S1–S8）、[SPEC.md §13](../../../../../SPEC.md)。确定性时间是以“charge unit × CM-1（带版本的校准常量）”度量的逻辑时钟，不是墙钟。本章全部契约都包含 native == wasm 的观测一致性（stdout / stderr / exit code）。

<a id="als-dt1-時間構築子と代数"></a>

## ALS-DT1 时间构造器与代数

时间量只能由封闭单位集合（`ns / us / ms / s / min / h`）的模块限定构造器创建（`compute.*` = 确定性计算时间，`duration.*` = 墙钟），裸 `Int` 是类型错误。负的构造器参数、负的缩放系数按 §13 终止约定确定性 abort（`Error: negative time...` + exit 1）；构造溢出时饱和到最大值。
代数仅允许 `T + T`（饱和）、`T - T`（饱和到 0）、`T × Int`（饱和，负系数 abort）以及同类型比较。`T × T`、混合时钟、`/` 均为类型错误。
夹具：`spec/wasm_cross/time_negative_trap.almd`、`time_negative_scale.almd`、
`time_saturate.almd`、`time_ops_algebra.almd`。

<a id="als-dt2-決定的予算fanbounded"></a>

## ALS-DT2 确定性预算（fan.bounded）

`fan.bounded(c) { body }` 的判定仅是程序及其输入的函数：只有消耗的 charge unit 超过预算 unit（`ns / CM-1`，向下取整）时才为 Err，消息为台账常量 `fan.bounded: budget exhausted`。相同程序在任意目标、任意宿主上，都在相同声明纳秒值处发生 Ok ⇄ Err 转换（严格的 unit 边界）。嵌套采用 min-cap（EIP-150 式；当前发布版本以 E007 拒绝 region 嵌套本身，因此这一规则的可执行证据仅存在于参考求值器实现中）。bind 语句 charge 为 0。

**charge 表（CM-1 v0.3 = 3ns/unit，C-207）**：charge site 为 (a) fn 入口——1 unit，但没有循环且不递归的被调用函数会由共享 MIR 内联器消除（连同入口 charge 一起变为 0 unit，C-294）；(b) 循环头判定——每次求值 1 unit（`while` 连最后的假判定也计数：n 轮 = n+1 unit）；(c) 批量字符串连接——`1 + 結果バイト長/16` unit（结果字节长度；按结果计费，C-204 的 T3-5）。bind 语句、直线表达式、分支为 0。判定采用 **check-then-charge**：每个 site 若 `spent + cost > budget` 就 cut（在第一个 charge 前已完成的表达式，即使预算为 0 也成功）。`heavy(1000)`（入口 1 + 循环头 1001）= 1002 unit = 3006ns 是校准示例；`spec/wasm_cross/fuel_bounded_boundary.almd` 与 `fuel_dyn_charge.almd`（252 unit，在 750/760ns 转换）以 unit 精度固定边界。

**cut 记账（2026-08-25 裁定，C-320）**：因预算耗尽而 cut 时，先执行与普通退出相同的计量器记账（region exit），再退出。因此：

1. 耗尽预算的 region 判定**必定为 Err**。包含中间值或零值的 Ok（stale verdict）不符合规范。
2. region 的判定和值**不依赖同一程序内的其他 region**（计量器状态不在 region 间泄漏；不会观测到前一个 region 的 cut 改变后一个 region 的判定）。
3. cut 在 **arm 直属的循环中**触发，还是在**被调用函数内**触发，均不可观测地区分（相同声明纳秒值给出相同判定）。arm 的值是 body 块的值，不依赖循环形式（`for` / `while`）。
4. 计量器只计费预算范围内 body 的计算。使观测得以呈现的运行时机制（字符串格式化、`??` 回退路径）既不计费，也不被 cut；Err 后的回退及后续输出始终保持健全。

夹具：`spec/wasm_cross/fuel_bounded_boundary.almd`、`fuel_block_body.almd`、
`fuel_bare_result.almd`；cut 记账由 `spec/wasm_cross/fuel_cut_in_arm_loop.almd` 覆盖（0.58.0 在两个目标上以相同方式违反规则 1–2：直属循环的 cut 产生 stale Ok(0)，污染此后的所有 region。这种情况不会体现在一致性判定中；在参考路径固定值之前，以 agreement 方式承载。almide/almide#1572）。

<a id="als-dt3-決定的-racefanmin"></a>

## ALS-DT3 确定性 race（fan.min）

胜者是在“预算内完成，且自身未返回 Err”的 arm 中，`(spend, index)` 字典序最小者：成本最低的 arm 获胜，同分按源码顺序。Result arm 的 Err 将该 arm 从候选中移除（不传播）。全部淘汰时返回台账常量 Err。
夹具：`spec/wasm_cross/fuel_race_boundary.almd`、`fuel_race_err_skip.almd`、
`fuel_bare_result.almd`。

<a id="als-dt4-settle-の-tuple-契約"></a>

## ALS-DT4 settle 的 tuple 契约

`fan.settle { a; b; … }` 的值是按 arm 顺序排列的 tuple `(Result[A, String], …)`：允许异构 arm，普通 arm 包装为 Ok，效应 arm 的 Err 捕获在该 slot 中（不传播）。求值按 arm 顺序确定性进行。
夹具：`spec/wasm_cross/fan_settle_tuple.almd`。

<a id="als-dt5-壁時計期限fantimeoutoracle-層"></a>

## ALS-DT5 墙钟期限（fan.timeout、oracle 层）

`fan.timeout(duration.ms(n)) { body }` 在 **charge site 协作式检查**墙钟期限（统一中断点原则：绝不在操作中途切断，与 Go context 相同的协作模型）。判定相对于 ω（R_Ω）：在哪次检查中超时属于宿主输入；**固定 ω 的重放会逐字节再现观测**（将 native 记录的 ω 在 wasm 上重放，按定义也成立）。能够确定性主张的仅为两个端点：足够大的期限总能完成，发散的 body + 极短期限总会在 charge site 切断。
夹具：`spec/wasm_cross/fuel_timeout_ends.almd`（两端）+ record/replay 门禁（`tests/charge_probe_test.rs::timeout_deterministic_ends_and_replay`）。

> 原文背景：本文日期为 2026-08-27，保留历史 fan 写法、特定发布版本对嵌套 region 的拒绝、0.58.0 缺陷及未来更新参考路径的预期。这不是对当前实现的新验证。足够大期限的说法按原文保留，不扩展为发散 body 的完成保证。
