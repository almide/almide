<a id="c-006--fantimeout-was-removed-0290"></a>

# C-006 — fan.timeout 已删除（0.29.0）

[![English](https://img.shields.io/badge/English-d0d7de?style=flat-square)](C-006-fan-timeout-removed.md) [![日本語](https://img.shields.io/badge/%E6%97%A5%E6%9C%AC%E8%AA%9E-d0d7de?style=flat-square)](C-006-fan-timeout-removed.ja.md) [![简体中文](https://img.shields.io/badge/%E7%AE%80%E4%BD%93%E4%B8%AD%E6%96%87-24292f?style=flat-square)](C-006-fan-timeout-removed.zh-CN.md)

> 非规范性、固定原文的读者快照；未修改原本仍为权威来源。不自动同步，未独立执行、重新测量或新验证契约保证。当前／历史描述均属于固定原文时点。C-006 区分旧 timeout 形式删除与后来协作式块形式。C-033 保留修复前诊断及关闭指示作为历史，不执行旧删除／门禁修改指示。repr helper／export 细节描述历史 emitter，不是当前公开 ABI。代码注释保持原样。 [Source](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/contracts/C-006-fan-timeout-removed.md)

> [contracts.toml](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/contracts/contracts.toml) 中契约 C-006 的文字说明。
> 状态：**active，在0.47.0中部分被取代**——C-006 记录0.29.0删除旧 `fan.timeout(ms, thunk)` 形式。当前 `fan.timeout(duration) { body }` 形式是 C-208 的协作式 oracle-tier deadline；参阅[当前 fan 规范](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/SPEC.md#136-fantimeout--the-oracle-tier-wall-clock-deadline)。
> 以下各节解释历史删除，不表示当前语言中没有 `fan.timeout`。

<a id="history-why-it-was-the-sole-exemption-0240--028x"></a>

## 历史：为何它是唯一例外（0.24.0 – 0.28.x）

`fan.timeout(ms, thunk)` 是**墙钟时间 effect**。两个目标的时间概念根本不同：

| 目标 | 机制 | 结果 |
|--------|-----------|--------|
| native | 真实线程 + `recv_timeout(ms)` | timeout 可能触发，较慢的 thunk 被放弃 |
| wasm | 没有时钟、调度器、线程 | thunk 运行至完成，timeout 永不届满 |

wasm 沙箱中的“经过的墙钟时间”没有可移植含义，因此 `fan.timeout` 被排除在跨目标等价性保证之外，生成 wasm 时会给出醒目的构建期警告。它是唯一被排除的 `fan.*` 操作：`fan.race` / `fan.any` / `fan.map` / `fan.settle` 均通过列表顺序实现确定性（见 C-004 / C-005），并确实满足等价性承诺。

<a id="why-removal-not-portable-semantics-0290"></a>

## 为何删除而不是采用可移植语义（0.29.0）

比跨目标差异更严重的是：deadline 是否触发依赖机器负载，因此**即使在两次原生运行之间**，结果也不是程序加输入的函数——它是唯一违反这一属性的标准库 API。发明“可移植”语义（协作式 deadline 检查）会保留非确定性，同时为挽救一个函数而引入新语义。组织范围排查发现零使用方，因此直接删除了该操作：

- 类型检查器以携带迁移提示的专用 tombstone（E027）拒绝 `fan.timeout`，而非通用未知成员错误。
- deadline 属于调用程序的**宿主边界**（`timeout 5 ./app`、wasmtime 宿主 deadline、supervisor）。
- 原生运行时（`almide_rt_fan_timeout`）、wasm emit 分支、v1 MIR desugar 及 CLI 构建期警告在同一变更中全部删除。

<a id="old-form-diagnostic-evidence"></a>

## 旧形式的诊断证据

- `tests/diagnostics/e027-fan-timeout-removed/`——broken/fixed 对：错误的旧形式调用产生 `error[E027]`（`fan.timeout changed signature`），附带 `fan.timeout(duration.ms(5000)) { work(x) }` 迁移提示。fixed 文件采用这一当前块形式，必须干净地编译通过。
- `tests/diagnostic_harness_test.rs::broken_files_produce_expected_diagnostics`——执行每个诊断案例的框架。

类型检查先于目标分流，因此旧形式 tombstone 与目标无关。当前块形式 timeout 行为及其跨目标限制由 C-208 单独覆盖，不属于这份删除记录。
