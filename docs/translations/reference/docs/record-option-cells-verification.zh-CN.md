<a id="option-record-literals-2057-verification-2026-09-09"></a>

# Option 记录字面量：#2057 验证（2026-09-09）

[![English](https://img.shields.io/badge/English-d0d7de?style=flat-square)](record-option-cells-verification.md) [![日本語](https://img.shields.io/badge/%E6%97%A5%E6%9C%AC%E8%AA%9E-d0d7de?style=flat-square)](record-option-cells-verification.ja.md) [![简体中文](https://img.shields.io/badge/%E7%AE%80%E4%BD%93%E4%B8%AD%E6%96%87-24292f?style=flat-square)](record-option-cells-verification.zh-CN.md)

> 非规范、固定原文的历史读者快照。原文仍为权威来源，不替换机器输入或可执行 gate。不自动同步。记录的测试结果、数量、实现状态及证明主张仅限原文语境，并未在此独立重跑或确立为当前保证。 保留既有实现明确不支持的情况及 27→28 的上限变化，不据此推断这些主机支持完整字段矩阵。 [Source](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/record-option-cells-verification.md)

在 develop `cbf04dbc3` 上，结构化后端已经接纳所报告的直接 `none` 字面量。原始复现构建出验证式结构化 WASM 产物（2292 字节），输出 `0`，与 native 一致。无需更改 emitter：`crates/almide-wasm/src/data.rs::lower_sum` 接收期望的 Option 形状，记录 emitter 将各字段声明的类型提供给 `lower`。

新的 C-255 fixture 在返回 Result 的函数内，检验 Int、Float、Bool、String、List[Int] 及命名记录的 Option 字段为空和有值两种情况。native 和结构化 WASM 均输出 `none\nsome\n`，解释器运行语料也固定相同输出。Codec 字段矩阵测试再次将整个解码记录与含 `none` 字段的字面量比较，涵盖 null 输入和缺失输入。

分配台账和两个大小台账只添加了新 fixture 的实测行：heap watermark 为 69984，发射大小为 14022，出货 WASI 大小为 14556 字节。现有行不变。AST／诊断 manifest 还包含新 fixture 及修改后的 Codec fixture 的 AST hash。契约证据链接是对称的。

<a id="incumbent-determinism-domain"></a>

## 既有实现的确定性覆盖域

主机 CI 运行 34321896641 对所有参与比较的 fixture 发射相同字节，但新的 `record_option_none_cells.almd` fixture 在两个既有主机上均被拒绝，使其不支持输入数从 27 增至 28。主机和浏览器的确定性上限遵循已有的仅结构化路径 fixture 策略，按这一具体原因更新。这并非宣称既有实现支持此字段矩阵：执行它的是 native 和当前结构化路径，分配／大小台账覆盖的是这些结构化字节。没有排除任何先前参与比较的 fixture。
