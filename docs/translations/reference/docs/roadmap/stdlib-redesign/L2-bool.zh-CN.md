> 原文描述：标准库重设计 L2/L3 — bool：新增模块，包含 bool.parse、bool.to_string 和 bool.compare（Bool 有序，与九种语言中的七种相同）
<a id="l2l3-bool"></a>

# L2/L3：bool

[![English](https://img.shields.io/badge/English-d0d7de?style=flat-square)](L2-bool.md) [![日本語](https://img.shields.io/badge/%E6%97%A5%E6%9C%AC%E8%AA%9E-d0d7de?style=flat-square)](L2-bool.ja.md) [![简体中文](https://img.shields.io/badge/%E7%AE%80%E4%BD%93%E4%B8%AD%E6%96%87-24292f?style=flat-square)](L2-bool.zh-CN.md)

> 非规范性的固定读者快照。以原文为准，不自动同步，未独立重新验证实现、测量或安全性。
> [Source / 原文](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/stdlib-redesign/L2-bool.md)
> 这是原文时点的重设计决策记录。采纳的新增方案及跨语言比较并非经独立验证的当前实现主张。保留第九／十一轮判定变更及三项新增方案的历史。


> 最后更新：2026-10-22
> 前提：[L0](../../../../../roadmap/stdlib-redesign/L0-conventions.md)、[L1](../../../../../roadmap/stdlib-redesign/L1-categories.md)（R-L1-30）
> 来源：Rust https://doc.rust-lang.org/std/primitive.bool.html · Go https://pkg.go.dev/strconv#ParseBool · Python —（`bool("false")` 为 True）· TS/JS —（`JSON.parse`）· Gleam —（标准库中没有）· Elixir —（`String.to_existing_atom`）· OCaml https://ocaml.org/manual/latest/api/Stdlib.html#VALbool_of_string_opt · Kotlin https://kotlinlang.org/api/core/kotlin-stdlib/kotlin.text/to-boolean-strict-or-null.html · Swift https://developer.apple.com/documentation/swift/bool/init(_:)-4dsgl

<a id="l2-役割の比較"></a>

## L2：角色比较

| 角色 | Almide | Rust | Go | Python | TS/JS | Gleam | Elixir | OCaml | Kotlin | Swift |
|---|---|---|---|---|---|---|---|---|---|---|
| 从字符串读取（无效则失败） | `bool.parse` | `parse::<bool>` | `ParseBool` | — | — | — | — | `bool_of_string_opt` | `toBooleanStrictOrNull` | `Bool(s)` |
| 比较（三值） | `bool.compare`（R-bool-02） | `cmp`（`Ord`） | — | `<`（`False < True`） | — | `bool.compare` | `<`（项的顺序） | `Bool.compare` | `compareTo` | —（不遵循 `Comparable`） |
| 转为字符串 | `bool.to_string`（`Repr`。R-bool-03） | `to_string` | `FormatBool` | `str` | `String` | `bool.to_string` | `to_string` | `string_of_bool` | `toString` | `description` |

<a id="完全性の規則"></a>

### 完整性规则

1. `parse` 家族：可从字符串读取的基本类型（`Int`、`Float`、固定位宽数、`Bool`）分别具有 `<mod>.parse(s) -> Result[T, String]`。
2. `compare` 家族（R-L0-16）：`Bool` 是有序类型（`Ord`），满足 `false < true`。提供 `bool.compare(a, b) -> Int`（R-bool-02）。
3. 有意省略：不提供 `not`、`and` 等函数（使用 `not`、`and`、`or` 运算符）。也不提供 `to_int`（可写作 `if b then 1 else 0`；只有 Gleam 和 OCaml 有专门函数）。

<a id="案"></a>

## 方案

<a id="r-bool-01-boolparse-を足す---採用"></a>

### R-bool-01 添加 `bool.parse` — ○ 采纳

- 内容：`fn bool.parse(s: String) -> Result[Bool, String]`。`"true"` → `ok(true)`、`"false"` → `ok(false)`，其他均为 err。与 R-L1-30 一致。
- (b) 纯计算。两个 target 相同。
- (d) 新模块及 1 项新增。将 `bool` 与 `int` 一样，加入无需 import 即可使用的列表（`AUTO_IMPORT_BUNDLED`）。
- issue：#3980

<a id="r-bool-02-boolcompare-を足す---採用十一巡目に保留から戻す9-言語中-7-つが順序をつける"></a>

### R-bool-02 添加 `bool.compare` — ○ 采纳（第十一轮从暂缓恢复；九种语言中七种赋予顺序）

- 内容：`fn bool.compare(a: Bool, b: Bool) -> Int`。按 `false < true` 的顺序，小于时返回负数，相等为 0，大于为正数。
- (a) Rust（`Ord`）、Gleam、OCaml、Kotlin 均提供。
- (b) 纯计算。两个 target 相同。与当前 `<` 和 `list.sort` 的顺序（`false` 在前）一致。
- (c) R-L0-16 规定“为有序类型提供 `compare`”，但对象列表漏掉了 `Bool`。R-L1-30 新建了 `bool` 模块，因此在此补齐。
- 实现：不另写函数。它就是 `Bool` 的 `Ord` 中的 `compare` 本身（R-L0-22 完整性规则）。`a.compare(b)` 与 `bool.compare(a, b)` 是同一个函数。
- (d) 新增 1 项。
- 判定经过：第九轮以 Swift 为参照组织 protocol 层级时，因为 Swift 的 `Bool` 不是 `Comparable`，将此项暂缓。第十一轮重新排列九种语言后，有序的七种为 Rust（`Ord`）、Python（`False < True`）、TS/JS（转为数值再比较）、Gleam（`bool.compare`）、Elixir（项的顺序）、OCaml（`Bool.compare`）、Kotlin（`Comparable`）；不提供顺序的两种为 Go（`<` 编译报错）和 Swift。各类型的行为按多数决定（R-L0-00）。层级骨架以 Swift 为范本，但具体类型归属哪个 protocol，则逐类型按多数决定。
- (c) Almide 当前接受 `false < true`，并可用 `list.sort` 排序 `List[Bool]`。将其移出 `Ord` 会破坏当前通过的代码。
- issue：#3980

<a id="r-bool-03-boolto_string-を置く---採用"></a>

### R-bool-03 提供 `bool.to_string` — ○ 采纳

- 内容：`fn bool.to_string(b: Bool) -> String`。返回 `"true"`、`"false"`，与 `bool.parse` 往返。它是 R-L0-38 的 `Repr` 函数，内容与 `"${b}"` 相同。
- (a) 九种语言全部有函数形式（Rust `to_string`、Go `FormatBool`、Python `str`、TS/JS `String`、Gleam `bool.to_string`、Elixir `to_string`、OCaml `string_of_bool`、Kotlin `toString`、Swift `description`）。Swift 在 `LosslessStringConvertible` 中将 `Bool(s)` 和 `description` 配对。
- 修订第一轮“已有 `Repr`，因此不提供”的决定（在 R-L0-38 中它成为 `Repr` 函数本身，不另行实现）。
- (d) 新增 1 项。
- issue：#3980

<a id="l3-関数表"></a>

## L3：函数表

| 函数 | 签名（当前，显示 mut） | 其他语言的主要拼写 | 判定 | 方案 |
|---|---|---|---|---|
| `parse` | （新增）`bool.parse(s: String) -> Result[Bool, String]` | Rust `parse::<bool>` / Go `ParseBool` / Kotlin `toBooleanStrictOrNull` | 添加 | R-bool-01 |
| `compare` | （新增）`bool.compare(a: Bool, b: Bool) -> Int` | Rust `cmp` / Gleam `bool.compare` / OCaml `Bool.compare` / Kotlin `compareTo` | 添加 | R-bool-02 |
| `to_string` | （新增）`bool.to_string(b: Bool) -> String` | Rust `to_string` / Go `FormatBool` / Swift `description` | 添加 | R-bool-03 |
