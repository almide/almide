<a id="file-backed-snapshot-testing--on-hold"></a>

# 基于文件的快照测试 — 搁置

[![English](https://img.shields.io/badge/English-d0d7de?style=flat-square)](snapshot-testing.md) [![日本語](https://img.shields.io/badge/%E6%97%A5%E6%9C%AC%E8%AA%9E-d0d7de?style=flat-square)](snapshot-testing.ja.md) [![简体中文](https://img.shields.io/badge/%E7%AE%80%E4%BD%93%E4%B8%AD%E6%96%87-24292f?style=flat-square)](snapshot-testing.zh-CN.md)



> 这是[固定原文](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/on-hold/snapshot-testing.md)的非规范读者译文，是不自动同步的历史固定快照。状态、计划和数量均为原文时点的记录，不证明当前实现或完成情况。本次翻译未独立执行或验证这些主张。

> 原文说明：在已提供的源码字面量快照断言之外，可选的基于文件的快照。

<a id="already-available"></a>

## 已经可用的功能

`testing.assert_snapshot(actual, expected)` 将输出与预期的**源码字面量**比较。`almide test --update-snapshots <file>` 通过重写该字面量接受新值或发生变化的值；普通测试运行不写入，CI 模式拒绝接受更新。没有附属快照文件。

当前 API 和接受规则见 [CLI 参考](../../../../../specs/cli.md#スナップショット-testingassert_snapshot)。实现：`stdlib/testing.almd` 和 `src/cli/commands.rs::cmd_test_update_snapshots`。证据：`spec/stdlib/testing_snapshot_test.almd` 和 `tests/snapshot_update_test.rs`（精确重写、重复接受、普通失败和 CI 拒绝写入）。

<a id="remaining-design-question"></a>

## 剩余设计问题

原提案将预期输出保存在单独文件中。这与已提供的字面量断言是不同功能，仍处于搁置状态。只有具体用例需要外部快照文件时才重新考虑；届时应定义文件命名、可审查的接受流程、过期文件处理和 CI 拒绝写入，而不改变现有字面量测试。

早期的 `snapshot(result)` 草案是提案，不是已实现的 API。设计参考仍为 MoonBit 的 `inspect` 预期值及 Gleam 的文件式测试快照。两者都不意味着 Almide 已具有附属文件契约。
