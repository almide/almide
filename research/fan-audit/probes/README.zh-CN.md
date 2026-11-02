# Fan 语义审计探针

[![English](https://img.shields.io/badge/English-d0d7de?style=flat-square)](./README.md) [![日本語](https://img.shields.io/badge/%E6%97%A5%E6%9C%AC%E8%AA%9E-d0d7de?style=flat-square)](./README.ja.md) [![简体中文](https://img.shields.io/badge/%E7%AE%80%E4%BD%93%E4%B8%AD%E6%96%87-24292f?style=flat-square)](./README.zh-CN.md)

这些程序是 [docs/design/fan-semantics-audit.md](../../../docs/design/fan-semantics-audit.md) 中每处“probe”引用的依据，固定于 0746eb689。从仓库根目录在两个目标上运行：

```bash
almide run research/fan-audit/probes/B/race_order.almd
almide run research/fan-audit/probes/B/race_order.almd --target wasm
```

- `A/`：block、`fan.map` 和 `fan.settle` 形式。仅保留发现行为差异的两个探针；其他 block/map/settle 观测由 `tests/fan_acceptance_test.rs` 固定。
- `B/`：`fan.any` 和 `fan.race`。
- `C/`：`fan.bounded`、`fan.timeout`、时间构造器及 effect 准入判定。

这些是证据，而不是测试。因为不含 `test` 块，`almide test` 会跳过它们。有些预期会编译失败：这些探针检查程序是否被拒绝，例如产生 E006。
