<a id="l2l3-uuid新しいモジュール"></a>

# L2/L3：uuid（新模块）

[![English](https://img.shields.io/badge/English-d0d7de?style=flat-square)](L2-uuid.md) [![日本語](https://img.shields.io/badge/%E6%97%A5%E6%9C%AC%E8%AA%9E-d0d7de?style=flat-square)](L2-uuid.ja.md) [![简体中文](https://img.shields.io/badge/%E7%AE%80%E4%BD%93%E4%B8%AD%E6%96%87-24292f?style=flat-square)](L2-uuid.zh-CN.md)

> 非规范、固定原文的历史读者快照。原路径仍为权威来源，为其读取方保持不变。不自动同步，也未独立执行。已采纳提案、完成勾选及剩余工作均保留原文时点状态，不是新核实的当前实现状态。 10月21日提案及 (F) 核实是原作者当时的工作，不是本翻译重新核实来源。原文对 LLM 选择、单调性和重复检查的主张不构成独立保证。零 UUID 示例没有另行解释全1值。 [Source](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/stdlib-redesign/L2-uuid.md)

> 原 metadata 摘要（非活动读者注记）：标准库重设计 L2/L3 — uuid（新增，R-L1-07）。基于 wasi:random、wasi:clocks 的 effect fn 生成 v4/v7，纯 parse/to_string/bytes 转换，以及因 v7 有序而提供 compare。

> 最后更新：2026-10-21
> 前提：[L0](../../../../../roadmap/stdlib-redesign/L0-conventions.md)、[L1](../../../../../roadmap/stdlib-redesign/L1-categories.md)（R-L1-07：`uuid.v4()`、`uuid.v7()`、`uuid.parse`、`uuid.to_string`）
> 来源：Rust https://docs.rs/uuid（不在标准库中，仅作参考）、Go https://pkg.go.dev/uuid、Python https://docs.python.org/3/library/uuid.html、TS/JS https://developer.mozilla.org/en-US/docs/Web/API/Crypto/randomUUID、Gleam —（无）、Elixir —（无）、OCaml —（无）、Kotlin https://kotlinlang.org/api/core/kotlin-stdlib/kotlin.uuid/-uuid/、Swift https://developer.apple.com/documentation/foundation/uuid
> (F) = 本次调查中获取并核实的单元。

<a id="l2-役割の比較"></a>

## L2：角色比较

| 角色 | Almide（提案） | Rust(uuid crate) | Go | Python | TS/JS | Gleam | Elixir | OCaml | Kotlin | Swift |
|---|---|---|---|---|---|---|---|---|---|---|
| 随机 v4 | `v4` | `Uuid::new_v4` | `NewV4`(F) | `uuid4`(F) | `crypto.randomUUID` | — | — | — | `Uuid.random` | `UUID()` |
| 时间顺序 v7 | `v7` | `Uuid::now_v7` | `NewV7`(F) | `uuid7`（3.14 起）(F) | — | — | — | — | — | — |
| 从字符串 | `parse` | `parse_str` | `Parse`(F) | `UUID(s)`(F) | — | — | — | — | `Uuid.parse` | `UUID(uuidString:)` |
| 转为字符串 | `to_string` | `to_string` | `String`(F) | `str(u)` | （返回字符串） | — | — | — | `toString` | `uuidString` |
| 从／转为 16 字节 | `from_bytes`、`to_bytes` | `from_bytes`、`as_bytes` | `[16]byte` | `UUID(bytes=)`、`.bytes`(F) | — | — | — | — | `fromByteArray`、`toByteArray` | `uuid` |
| 比较 | `compare` | `Ord` | `Compare`(F) | `<` | — | — | — | — | `compareTo` | — |
| 检查版本 | — | `get_version` | — | `.version`(F) | — | — | — | — | — | — |
| 全 0／全 1 | — | `nil`、`max` | `Nil`、`Max`(F) | `NIL`、`MAX` | — | — | — | — | `NIL` | — |

<a id="完全性の規則"></a>

### 完整性规则

- 创建（effect）：{`v4`, `v7`} 共 2 个。不提供 v1、v3、v5、v6、v8（9 种语言中多数具有 v4，v7 依 L1 裁定添加。命名空间 v3/v5 使用 MD5、SHA-1，若需要，用户可基于 `hash` 编写）。
- 表示转换（纯）：{字符串, 16 字节} × {从, 转为} 共 4 个单元。`parse` 和 `from_bytes` 是带原因的失败，因此返回 `Result[Uuid, String]`（R-L0-06 第 4 项：不返回标记失败的哨兵值）。
- 顺序：`Uuid` 按 16 字节从高位起的字典序排序。v7 在该顺序下即为时间顺序。遵循 R-L0-16，提供 `uuid.compare(a, b) -> Int`。
- 写法：从字符串创建使用 `parse`，与 L1 裁定及 `int.parse`、`url.parse` 一致。从字节序列创建遵循 R-L0-07 第 2 项，使用 `from_bytes`。
- 有意不提供：检查版本的函数、`nil`、`max`。可用字符串 `parse("00000000-0000-0000-0000-000000000000")` 表达，也未达到 5 种以上语言的多数。

<a id="l2-の判定"></a>

### L2 裁定

- 这是新模块。

<a id="案"></a>

## 提案

<a id="r-uuid-01-uuid-モジュールを足す---採用"></a>

### R-uuid-01 添加 `uuid` 模块——通过

- 内容：下方 L3 的 7 个函数与不透明类型 `Uuid`（内部为 16 字节）。
  - `v4` 在 122 个随机位中加入 4 个版本位（`4`）和 2 个 variant 位（`10`）。
  - `v7` 的前 48 位为毫秒 Unix 时间，接下来的 12 位为同一毫秒内的计数（RFC 9562 §6.2 方法 1），在单个进程内形成严格递增序列。Python 的 `uuid7` 也通过计数保证单调性 (F)。
  - `to_string` 使用小写 8-4-4-4-12。`parse` 接受该形式、大写以及 `{…}`、`urn:uuid:` 形式（与 Python 的范围相同 (F)）。
- 依据的 L0 规则：R-L0-06（第 4 项）、R-L0-07、R-L0-16（`compare`）、R-L0-15。
- (a) 5 种语言具有此功能（L1）。
- (b) `v4` 使用 `wasi:random/random` 的 `get-random-bytes`（用 Almide 基于 R-L1-04 添加的 `random.bytes(n)` 编写）。`v7` 在此基础上加入 `wasi:clocks/wall-clock`（由 R-L1-25 移入 `datetime` 的 `now_ms`）。两者都是 effect fn，其余 5 个函数是纯计算。不需要新 host op。通过 ADR-0027 D3 的 verdict 检验：`v4` 检查版本和 variant 位、m 次无重复；`v7` 检查时间位于 `[t₀, t₁]` 窗口内、连续生成的值按 `compare` 递增。
- (c) LLM 在“分配 ID”任务中会写 `uuid4()`、`randomUUID()` 形式。`v4`、`v7` 直接使用版本名称，因此不会混淆选用哪一个。
- (d) 新增（新模块；需要 `import uuid`，与 `json`、`random` 同样处理）。
- gate 提案：`parse(to_string(u)) == ok(u)` 和 `from_bytes(to_bytes(u)) == ok(u)` 的性质测试、v7 单调性 fixture，以及 native 与 wasm 的 verdict 行一致的 cross fixture。
- issue：#3934

<a id="l3-関数表"></a>

## L3：函数表

| 函数 | 签名（提案） | 其他语言的主要写法 | 裁定 | 提案 |
|---|---|---|---|---|
| `uuid.v4` | `effect fn v4() -> Uuid` | Go `NewV4`(F) / Python `uuid4`(F) / JS `randomUUID` / Kotlin `Uuid.random` | 新增 | R-uuid-01 |
| `uuid.v7` | `effect fn v7() -> Uuid` | Go `NewV7`(F) / Python `uuid7`(F) / Rust `now_v7` | 新增 | R-uuid-01 |
| `uuid.parse` | `fn parse(s: String) -> Result[Uuid, String]` | Go `Parse`(F) / Kotlin `Uuid.parse` / Python `UUID(s)`(F) | 新增 | R-uuid-01 |
| `uuid.to_string` | `fn to_string(u: Uuid) -> String` | Go `String`(F) / Kotlin `toString` / Swift `uuidString` | 新增 | R-uuid-01 |
| `uuid.from_bytes` | `fn from_bytes(b: Bytes) -> Result[Uuid, String]` | Rust `from_bytes` / Kotlin `fromByteArray` / Python `UUID(bytes=)`(F) | 新增 | R-uuid-01 |
| `uuid.to_bytes` | `fn to_bytes(u: Uuid) -> Bytes` | Rust `as_bytes` / Kotlin `toByteArray` / Python `.bytes`(F) | 新增 | R-uuid-01 |
| `uuid.compare` | `fn compare(a: Uuid, b: Uuid) -> Int` | Go `Compare`(F) / Kotlin `compareTo` | 新增 | R-uuid-01 |
