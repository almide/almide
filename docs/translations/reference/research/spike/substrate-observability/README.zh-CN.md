<a id="substrate-observability--the-c-004-exception-reproduced"></a>

# 执行底层的可观察性——复现 C-004 EXCEPTION

[![English](https://img.shields.io/badge/English-d0d7de?style=flat-square)](README.md) [![日本語](https://img.shields.io/badge/%E6%97%A5%E6%9C%AC%E8%AA%9E-d0d7de?style=flat-square)](README.ja.md) [![简体中文](https://img.shields.io/badge/%E7%AE%80%E4%BD%93%E4%B8%AD%E6%96%87-24292f?style=flat-square)](README.zh-CN.md)

> 非规范、固定原文的历史读者快照。原文仍为权威来源，不替换机器输入或可执行 gate。不自动同步。记录的测试结果、数量、实现状态及证明主张仅限原文语境，并未在此独立重跑或确立为当前保证。 2026-08-07 测量与 Rung 1 的未来操作指示均保留为历史记录；本副本不执行这些指示，也不声称旧 fan 时间 API 仍是当前 API。 [Source](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/research/spike/substrate-observability/README.md)

[ADR-0011](../../../../../adr/0011-execution-substrate-is-a-free-variable.md) 及 [execution-inception.md](../../../../../roadmap/active/execution-inception.md) §1 的证据。

```
research/spike/substrate-observability/run-repro.sh [runs]
```

<a id="what-it-shows"></a>

## 它说明什么

会打印的 `fan {}` 分支是执行底层影响观察的唯一表面。台账将其记作跨目标分歧（C-004 的 EXCEPTION：“native 上按墙钟时间，wasm 上串行”）。测量表明分类错误——这是 **native ⇄ native** 分歧，正是导致 `fan.timeout` 在 0.29.0 被移除的性质，也是 C-006 所称“唯一违反该性质的 stdlib 表面”。

测量日期 2026-08-07（almide 0.56.0、wasmtime 47.0.2、14-core arm64 macOS）：

| 执行底层 | 运行次数 | 不同分支顺序数 |
|---|---|---|
| native（作用域 OS 线程） | 10 | **8–9** |
| wasm（串行 inline） | 3 | 1 (`ABCD`) |
| 参考解释器 | — | 按构造为 1 (`eval.rs:148`) |

三个 oracle 中有两个与 [#1000](../../../../../roadmap/active/concurrency-stance.md) 定义的列表顺序串行观察一致。native 是离群者。

<a id="notes-for-whoever-re-runs-this"></a>

## 重新运行时的注意事项

- **工作必须无法被常量折叠。** 较早的尝试向 spin loop 传入字面量；LLVM 在编译时就完成求值，每个分支瞬间结束，分歧隐藏在两个目标整齐的 `A B C D` 背后。因此 seed 来自 `env.args()`。
- **单次运行不能证明任何事情。** 一次会话的首个 native 运行常按分支顺序打印；只有跨多次运行才能看到缺陷。
- **`env.sleep_ms` 更直接显示并行性**（四个分支都报告相同的开始和结束毫秒，总时长 ≈ 单个分支时长），但它在 wasm 分支因缺少 capability 而被拒绝，因此不能承担比较中的跨目标部分。

<a id="when-rung-1-lands"></a>

## Rung 1 落地时

分支作用域输出事务（每分支单独缓冲，在 join 时按分支顺序 flush）使每种底层都打印 `ABCD`。届时脚本退出码的意义会反转：将其改为 gate，移出 `research/spike/`，并在同一个 PR 中删除 C-004 的 EXCEPTION 条款。
