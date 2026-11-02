# WASM 异常处理

[![English](https://img.shields.io/badge/English-d0d7de?style=flat-square)](wasm-exception-handling.md) [![日本語](https://img.shields.io/badge/%E6%97%A5%E6%9C%AC%E8%AA%9E-d0d7de?style=flat-square)](wasm-exception-handling.ja.md) [![简体中文](https://img.shields.io/badge/%E7%AE%80%E4%BD%93%E4%B8%AD%E6%96%87-24292f?style=flat-square)](wasm-exception-handling.zh-CN.md)



> 这是[固定原文](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/on-hold/wasm-exception-handling.md)的非规范读者译文，是不自动同步的历史固定快照。状态、计划和数量均为原文时点的记录，不证明当前实现或完成情况。本次翻译未独立执行或验证这些主张。

> 原文说明：拟议的 Wasm 异常降级转换；上游默认值取决于版本和引擎。

## 拟议的降级转换

探索使用原生 `try_table` / `throw` / `exnref` 传播错误，取代当前 Result 表示和显式 `!` 处理。原计划中逐个 `?` 的术语属于旧语法。表示方式、分配和性能方面的收益需要实际实现转换并测量；本记录不声称 Almide 已实现 EH。

## 上游前提更新（2026-10-31）

“Wasmtime 默认关闭 EH”这一笼统障碍已过时。在为 Wasmtime 50.0.1 固定的源码中，`Config::wasm_exceptions` 默认开启，只有 `Strategy::Winch` 默认关闭。该 API 受 `gc` 构建特性控制，配置要求 GC 支持。见[固定的配置源码](https://github.com/bytecodealliance/wasmtime/blob/c307ec81de02852d815e9849f7c89d955c03c30d/crates/wasmtime/src/config.rs#L1438-L1453)。这是源码／配置观察，不是执行测试。

不要仅从 Wasmtime 推断 Spin、wasmCloud、Docker/Wasm 或其他嵌入式宿主的默认值：必须检查各宿主版本、引擎策略、构建特性和配置。更改交付路径之前，也需比较 Almide 自身固定的工具／依赖版本。

## 恢复条件

重新评估所需宿主覆盖范围、错误／catch 语义、跨目标测试夹具，以及是否需要回退路径。仅上游默认值改变，既不代表此功能完成，也不授权采用新的语言语义。[上游跟踪 issue](https://github.com/bytecodealliance/wasmtime/issues/3427) 是历史背景，不能代替固定源码和宿主测试。
