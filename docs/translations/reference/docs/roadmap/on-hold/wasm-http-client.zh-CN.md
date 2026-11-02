# WASM HTTP 客户端

[![English](https://img.shields.io/badge/English-d0d7de?style=flat-square)](wasm-http-client.md) [![日本語](https://img.shields.io/badge/%E6%97%A5%E6%9C%AC%E8%AA%9E-d0d7de?style=flat-square)](wasm-http-client.ja.md) [![简体中文](https://img.shields.io/badge/%E7%AE%80%E4%BD%93%E4%B8%AD%E6%96%87-24292f?style=flat-square)](wasm-http-client.zh-CN.md)

> 本文是[固定原文](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/on-hold/wasm-http-client.md)的非规范读者译文；原文仍是权威版本，本译文不会自动同步。




> 非规范性读者译本。固定于原文提交 `35297a612fb29e3f78f6587aba253aa91d51f466`，不会自动同步。状态、实现及测量描述均为原文时点的记录，并非翻译时的独立验证或现行保证；计划不代表已经实现。

说明：已提供的 wasm HTTP 客户端路径，以及尚余的默认组件迁移。

## 已提供的功能范围

每个函数的权威依据是 [target-availability.toml](../../../../../../proofs/target-availability.toml)；当前 API 见 [HTTP 参考](../../../../../stdlib/http.md)。不要根据旧的功能族名称或存在 wasm 目标就推断可用性。

- 完整响应客户端 `http.get_response` 和 `http.send` 可在 WASI 0.3 组件路径及 `almide run --target wasm` 的嵌入式宿主上运行。原来的公开名称 `http.request_response` 已由 `http.send(http.new_request(...))` 取代。
- 默认 stock-p1 产物仍不含 HTTP 导入，并以 E081 拒绝这些调用。p3 产物应使用 `ALMIDE_COMPONENT_P3=1` 和 `--component` 构建，再通过 `wasmtime run -S http=y` 运行。
- 可用性按操作区分：调用句柄和服务端功能需分别查看矩阵，不能把完整响应的结论扩展到所有 HTTP API。

依据：[http_response_cross_test.rs](../../../../../../tests/http_response_cross_test.rs) 将完整响应的状态、标头、正文和失败与原生目标对比；[target-availability.toml](../../../../../../proofs/target-availability.toml) 将 `http.get_response` 和 `http.send` 记录为仅在 `stock-p1` 上不可用。

## 剩余计划

客户端不是尚未实现的功能。剩余的部署迁移属于 [ADR-0023 的实施计划](../../../../../adr/0023-wasm-http-over-wasi-http.md#7-implementation-plan)，包括切换默认组件和停用私有宿主路径。本页仍保留在该 ADR 文档步骤引用的路径上，并非另一份实施清单。判断单个步骤时应遵循矩阵和当前测试，而不是本计划中的历史 API 拼写。
