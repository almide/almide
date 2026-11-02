> 说明（原本索引元数据的非活动副本）：标准库重设计 L2/L3 — value：动态树负责构造与读取（类型化 getter 和路径从 json 移入，as_object 补齐 as_ 族），并移除 stringify、keys 与键名大小写转换
<a id="l2l3-value"></a>

# L2/L3: value

[![English](https://img.shields.io/badge/English-d0d7de?style=flat-square)](L2-value.md) [![日本語](https://img.shields.io/badge/%E6%97%A5%E6%9C%AC%E8%AA%9E-d0d7de?style=flat-square)](L2-value.ja.md) [![简体中文](https://img.shields.io/badge/%E7%AE%80%E4%BD%93%E4%B8%AD%E6%96%87-24292f?style=flat-square)](L2-value.zh-CN.md)

> 非规范性历史重设计参照版，固定原本权威且不变。不自动同步、重新核查来源、独立probe或核实现行实现。(F)及实测是原作者历史报告。保留str改名初始拒绝与后来采纳、field/Result与后续get/Option、路径负下标新旧行为。初始族/门禁计数与后扩展get族并存，不擅自统一。保留对象顺序和重复以及Float不转Int语义。保留伪代码中的種類意为种类。 [Source](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/stdlib-redesign/L2-value.md)

> 最后更新：2026-10-21
> 前提：[L0](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/stdlib-redesign/L0-conventions.md)・[L1](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/stdlib-redesign/L1-categories.md)（R-L1-27）・配对页面 [L2-json](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/stdlib-redesign/L2-json.md)
> 来源：Rust —（非标准，serde_json 的 `Value` 是外部库 https://docs.rs/serde_json/latest/serde_json/enum.Value.html ）・Go https://pkg.go.dev/encoding/json （解析为 `any`）・Python https://docs.python.org/3/library/json.html (F)（解析为 dict/list）・TS/JS https://developer.mozilla.org/en-US/docs/Web/JavaScript/Reference/Global_Objects/JSON ・Gleam https://gleam-stdlib.hexdocs.pm/gleam/dynamic.html ・https://gleam-stdlib.hexdocs.pm/gleam/dynamic/decode.html ・Elixir https://elixir.hexdocs.pm/Access.html (F) ・OCaml —（非标准）・Kotlin —（官方 pkg https://kotlinlang.org/api/kotlinx.serialization/kotlinx-serialization-json/kotlinx.serialization.json/-json-element/ ）・Swift https://developer.apple.com/documentation/foundation/jsonserialization
> 实测：`almide 0.68.0 (dev, 437b8f127)`，native 与 `--target wasm`。probe 位于 scratchpad 的 `probe-json/`。

读法：`?`（问号，将 `Result` 转为 `Option`）、`??`（空值合并）、`|>`（管道）。

<a id="l2-役割の比較"></a>

## L2：角色比较

| 角色 | Almide | Rust（serde_json，外部） | Go | Python | TS/JS | Gleam | Elixir | OCaml | Kotlin（pkg） | Swift |
|---|---|---|---|---|---|---|---|---|---|---|
| 构造值 | `null`、`bool`、`int`、`float`、`str`、`array`、`object` | `json!`、`Value::String` | `any` 字面量 | dict/list 字面量 | 对象字面量 | pkg `json.string` 等 | 项本身 | — | `JsonPrimitive`、`buildJsonObject` | `Any` |
| 核对种类并提取（失败附理由） | `as_string`・`as_int`・`as_float`・`as_bool`・`as_array`（Result） | `as_str`・`as_i64`・`as_object`（Option） | 类型断言 `v.(string)` | `isinstance` | `typeof` | `decode.string` 等（Result） | 模式匹配 | — | `jsonPrimitive.content`、`jsonObject` | `as? String` |
| 提取为对象 | **—**（`keys` + `field` 组合构建） | `as_object` | `v.(map[string]any)` | dict | `Object.entries` | `decode.dict` | map | — | `jsonObject` | `as? [String: Any]` |
| 按键读子值 | `field`（Result） | `get`（Option） | `m["k"]` | `d.get("k")` | `o.k` | `decode.field` | `m["k"]` / `Access.key` (F) | — | `obj["k"]` | `d["k"]` |
| 按键读取类型化值 | `json.get_int` 等（在 json 中） | `v["k"].as_i64()` | — | — | — | `decode.field("k", decode.int)` | — | — | `obj["k"]?.jsonPrimitive?.intOrNull` | — |
| 键列表 | `keys`（非对象则 `[]`） | `as_object()?.keys()` | `maps.Keys` | `d.keys()` | `Object.keys` | — | `Map.keys` | — | `obj.keys` | `d.keys` |
| 按路径读写深层位置 | `json.get_path` 等（在 json 中） | `pointer` / `pointer_mut` | — | — | — | `decode.at` | `get_in`/`put_in`/`pop_in` + `Access.key`/`Access.at` (F) | — | — | — |
| 选择／排除／合并部分 | `pick`、`omit`、`merge` | — | — | dict 推导式 | 展开 | — | `Map.take`/`drop`/`merge` | — | — | — |
| 转为文本 | `stringify`（`json.stringify` 相同） | `to_string` | `Marshal` | `dumps` (F) | `JSON.stringify` | pkg `to_string` | `JSON.encode!` | — | `toString` | `JSONSerialization.data` |
| 更改键名样式 | `to_camel_case`、`to_snake_case`（仅第 1 层） | serde 的 `rename_all`（类型属性） | — | — | — | — | — | — | `JsonNamingStrategy` | `keyDecodingStrategy` |

<a id="完全性の規則"></a>

### 完整性规则

1. **`value` 负责值树的构造与读取**（R-L1-27）。与文本互转仅由 `json` 负责。
2. **种类族**：对含内容的每种 k ∈ {`bool`、`int`、`float`、`string`、`array`、`object`} 提供以下 3 个单元：
   - 构造函数：`bool`、`int`、`float`、`str`、`array`、`object`（当前拼写）
   - `as_<k>(v) -> Result[T, String]`：种类不符则 err（附理由）
   - `get_<k>(v, key) -> Option[T]`：无键、非对象或种类不符则 none
   `object` 的 `T` 与构造函数 `object` 接收的 `List[(String, Value)]` 同形（保留顺序和重复）。`null` 无内容，故没有 `as_`・`get_`，用 `v == value.null()` 检查。
3. **路径（`Path`）**：用 `root()`、`key(p, name)`、`index(p, i)` 构造，用于 `get_path`（Option）、`set_path`（Result）、`remove_path`（不存在则不变）。负下标越界（R-L0-12）。
4. **不以哨兵隐藏失败**（R-L0-06 的 4）：读取非对象值返回 Option 或 Result。`pick`・`omit`・`merge` 是转换而非读取，因此维持非对象原样返回的现有形式。
5. **有意遗漏**：构造函数 `str` 与 `as_string`・`get_string` 拼写不同，但不改名（→ R-value-08 重新审议，改名 `string`）（R-L0-00：不是 Almide 内部同角色冲突，也无多数）。

<a id="l2-の判定"></a>

### L2 判定

- 重复：`value.stringify` ≡ `json.stringify`（实测：两个 target 同字符串）→ R-value-01。
- 族缺口：缺 `as_object`。因此遍历对象需组合 `keys` 与 `field`，催生了 `json.to_map` 之类专用函数 → R-value-02。
- 归属：`json.get_*` 与路径操作是树操作 → R-value-02・R-value-03（[L2-json](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/stdlib-redesign/L2-json.md) R-json-01）。
- 哨兵：`value.keys(value.int(1))` 返回 `[]`（实测），无法区分非对象与空对象（R-L0-06 的 4）→ R-value-04。
- 单用途函数：`to_camel_case`・`to_snake_case` 只改第 1 层键，`userID` 变为 `user_i_d`（实测）。与 CHEATSHEET“wire 类型用 wire 名称，不设改名层”冲突 → R-value-05（#3852）。

<a id="案"></a>

## 提案

<a id="r-value-01-valuestringify-を撤去する---採用"></a>

### R-value-01 移除 `value.stringify` — ○ 采纳

- 内容：移除 `value.stringify(v)`，统一到 `json.stringify(v)`。
- 依据的 L0 规则：R-L1-27（转文本归 json）、R-L0-01（同输出 2 拼写）、R-L0-14。
- (b) 输出相同（`1.0` → `1`，非有限值 → `null`，两个 target 实测）。
- (c) `value` 自动 import，`json` 需 import，所以替换时添加 `import json`。
- (d) 移除。`@deprecated(since=15, use="json.stringify")`、`removal_epoch = 16`。`almide fix` 1 对 1 改写并添加 `import json`。使用数：spec 50 / tests 2 / docs 27 / stdlib 5 / research 2。
- 拟议门禁：#3851 F 同义词门禁规则 5（跨模块同函数体）。
- issue: #3953

<a id="r-value-02-種類の族を閉じる-as_object-と-get_種類-を置く---採用"></a>

### R-value-02 补齐种类族：提供 `as_object` 和 `get_<種類>` — ○ 采纳

- 内容：
  - 新增 `as_object(v) -> Result[List[(String, Value)], String]`。
  - 从 `json` 移入 `get_string`・`get_int`・`get_float`・`get_bool`・`get_array`（签名不变；与 `json.get_int` 一样不把 Float 变为 Int）。
  - 新增 `get_object(v, key) -> Option[List[(String, Value)]]`（族的最后一个单元）。
- 依据的 L0 规则：R-L1-27、R-L0-02、R-L0-06（键读取用 `get` 返回 Option，带理由失败用 Result）、CLAUDE.md“以矩阵扩展族”。
- (a) 提取为对象的函数有 serde `as_object`、Kotlin pkg `jsonObject`、Gleam `decode.dict`。
- (b) 纯计算。
- (c) 知道 `as_array` 的 LLM 会写 `as_object`，目前导致未定义错误。
- (d) 新增 2・移动 5（与 json 侧移除配对。json 侧 `@deprecated(since=15, use="value.get_<種類>")` 和 `removal_epoch = 16` 见 [L2-json](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/stdlib-redesign/L2-json.md) R-json-01）。
- 拟议门禁：族门禁 `tests/stdlib_value_family_test.rs` 将规则 2 的 6 种 × 3 单元名称与类型对照 interface JSON。
- issue: #3955

<a id="r-value-03-道筋の操作を-json-から移す---採用"></a>

### R-value-03 从 `json` 移入路径操作 — ○ 采纳

- 内容：`type value.Path`、`value.root() -> Path`、`value.key(p: Path, name: String) -> Path`、`value.index(p: Path, i: Int) -> Path`、`value.get_path(v, p) -> Option[Value]`、`value.set_path(v, p, x) -> Result[Value, String]`、`value.remove_path(v, p) -> Value`。
  - `json.field` 变为 `value.key`，因为不能与 `value.field(v, key)`（读取值）同名。
  - `value.index` 的负下标越界（当前 `json.index` 从末尾计数；实测 `index(root, -1)` 得 `some(3)`）。名称为新，因此不触及 R-L0-14 的 4。
- 依据的 L0 规则：R-L1-27、R-L0-00 条件 1、R-L0-12、R-L0-14。
- (a) 用 `key` 和 `at`/`index` 表达路径组件的是 Elixir（`Access.key`、`Access.at`）(F)。
- (b) 纯计算。
- (d) 新增 6 函数・1 类型（与 json 侧移除 6 配对；json 侧加 `@deprecated(since=15, use="value.root")` 等，在 `removal_epoch = 16` 移除）。使用数（json 的 6 函数）：spec 262 / tests 20 / docs 76。`almide fix` 1 对 1 改写（只有 `json.index` 负下标给提示）。
- 拟议门禁：将 `value.index` 纳入 R-L0-12 边界矩阵。
- issue: #3953

<a id="r-value-04-keys-を撤去する---採用"></a>

### R-value-04 移除 `keys` — ○ 采纳

- 内容：移除 `value.keys(v) -> List[String]`（非对象返回 `[]`），替换为 `value.as_object(v)`。
- 依据的 L0 规则：R-L0-06 的 4（可能失败的读取不返回哨兵）、R-L0-01（同读取不用 2 拼写）。R-L0-14 的 4 允许将 `keys` 返回类型改为 `Result`，但如此就与族单元 `as_object` 构成相同读取的 2 拼写，因此不保留 `keys`，统一为 `as_object`。
- (a) 取键列表前以类型确认对象的是 serde（`as_object()?.keys()`）和 Kotlin pkg（`jsonObject.keys`）。
- (c) 当前形式无法发现传入非对象；用 `keys` 遍历、`field` 读取还会对每个键重新查找。`as_object` 可 1 次遍历二元组。
- (d) 移除。`@deprecated(since=15, use="value.as_object")`、`removal_epoch = 16`。`almide fix` 的同义改写：`value.keys(v)` → `(value.as_object(v) ?? []) |> list.map(((k, _)) => k)`。提示“更宜直接遍历 `as_object` 二元组”。使用数：spec 25 / tests 1 / docs 10 / stdlib 0 / research 1。
- issue: #3954

<a id="r-value-05-to_camel_case-と-to_snake_case-を撤去する---採用"></a>

### R-value-05 移除 `to_camel_case` 与 `to_snake_case` — ○ 采纳

- 内容：按 #3852 提案移除 2 函数，不提供替代函数。
- 依据的 L0 规则：R-L0-01、R-L0-14；CHEATSHEET Rule 1（wire 类型用 wire 名称）。
- (a) 9 种语言均不把名称样式转换作为值树函数（serde・Kotlin・Swift 为类型序列化设置）。
- (c) 仅改第 1 层、`userID` → `user_i_d` 的行为无法按名称推断（实测）。
- (d) 移除。`@deprecated(since=15, use="Codec")`（无替代函数）、`removal_epoch = 16`。不能机械改写，提示“在 `Codec` 类型中写 wire 名称”。使用数（2 函数）：spec 14 / tests 0 / docs 12 / stdlib 2。
- issue: #3852（已有）

<a id="r-value-06-構築子-str-を-string-に改名する---不採用-r-value-08-で採用に変更"></a>

### R-value-06 构造函数 `str` 改名 `string` — × 不采纳（→ R-value-08 改为采纳）

- 理由：`str`（构造）与 `as_string`（读取）角色不同，不适用 R-L0-00 条件 1（同角色 2 拼写），也无多数拼写。使用量大（spec 111 / tests 20 / docs 29 / stdlib 4 / research 11），判断改名带来的新旧混杂期危害更大。
- issue：无

<a id="r-value-07-valuefield-を-valuegetoptionにしてget_種類-の族の頭にする---採用"></a>

### R-value-07 将 `value.field` 改为 `value.get`（Option），作为 `get_<種類>` 族的首项 — ○ 采纳

- 现状：`fn value.field(v: Value, key: String) -> Result[Value, String]`，以及包括 `fn value.get_int(v: Value, key: String) -> Option[Int]` 在内的 6 个函数。没有 `value.get`。
- 提案：新增 `fn value.get(v: Value, key: String) -> Value?`，移除 `value.field`。键读取族为 `get` 和 `get_<種類>` 共 7 个，全部 Option。需要理由时用 `as_<種類>`（Result）。
- 依据的 L0 规则：R-L0-06 的 1（可能失败的提取用无后缀名称、Option；`list.get`・`map.get`・`bytes.get`）、R-L0-01（键读取角色有 `field` 和 `get_` 两动词）。
- (a) 按键读子值的函数有 Rust JSON 库 `Value::get`（Option）、Python `dict.get`、Elixir `Map.get`。只有 Gleam `decode.field` 使用 `field`。
- (c) 实测：`value.get(v, "a")` 得 E002，提示“是否想写 `value.int`？”LLM 会按 `map.get` 同拼写书写。
- (d) 新增 1・移除 1。`@deprecated(since=15, use="value.get")`，epoch 16 移除。`almide fix` 把 `value.field(v, k)` 改为 `value.get(v, k)`；原先用 `!` 传播处提示“若需理由，使用 `value.get(v, k) ?? …` 或 `match`”。使用数：spec 24 / tests 17 / docs 14 / stdlib 24。
- 完整性规则：`get(v, key)` 与 `get_<k>(v, key)` 全部 Option，`as_<k>(v)` 全部 Result。
- 拟议门禁：在 R-value-02 族门禁添加 `get` 单元。
- issue: #3971

<a id="r-value-08-valuestr-を-valuestring-に改名する完全性の規則-5-の見直し---採用"></a>

### R-value-08 `value.str` 改名为 `value.string`（重新审议完整性规则 5）— ○ 采纳

- 现状：构造函数 `value.int`・`float`・`bool`・`array`・`object` 都是类型名，唯独字符串用 `str`。提取侧是 `as_string`・`get_string`。2 次外部评审都指出会误写成 `value.string`。
- 提案：`value.str` → `value.string`。
- 依据的 L0 规则：R-L0-00（Almide 内部同角色拼写分裂。规则 5 曾称“不冲突”，但实为构造函数族中唯独 1 个拼写不同的分裂）。
- (a) gleam_json `json.string`、serde `Value::String`。
- (d) 改名。`@deprecated(since=15, use="value.string")`、`removal_epoch = 16`，用 `almide fix` 改写。
- issue: #3985

<a id="l3-関数表"></a>

## L3：函数表

| 函数 | 签名（现行，mut 标注） | 其他语言的主要拼写 | 判定 | 提案 |
|---|---|---|---|---|
| `array` | `value.array(items: List[Value]) -> Value` | serde `Value::Array` / Kotlin pkg `JsonArray` / gleam_json `json.array` | 保持 | R-value-02 |
| `as_array` | `value.as_array(v: Value) -> Result[List[Value], String]` | serde `as_array` / Gleam `decode.list` / Kotlin pkg `jsonArray` | 保持 | R-value-02 |
| `as_bool` | `value.as_bool(v: Value) -> Result[Bool, String]` | serde `as_bool` / Gleam `decode.bool` | 保持 | R-value-02 |
| `as_float` | `value.as_float(v: Value) -> Result[Float, String]` | serde `as_f64` / Gleam `decode.float` | 保持 | R-value-02 |
| `as_int` | `value.as_int(v: Value) -> Result[Int, String]` | serde `as_i64` / Gleam `decode.int` | 保持 | R-value-02 |
| `as_string` | `value.as_string(v: Value) -> Result[String, String]` | serde `as_str` / Gleam `decode.string` | 保持 | R-value-02 |
| `bool` | `value.bool(b: Bool) -> Value` | serde `Value::Bool` / gleam_json `json.bool` | 保持 | R-value-02 |
| `float` | `value.float(f: Float) -> Value` | gleam_json `json.float` / Kotlin pkg `JsonPrimitive` | 保持 | R-value-02 |
| `int` | `value.int(n: Int) -> Value` | gleam_json `json.int` / Kotlin pkg `JsonPrimitive` | 保持 | R-value-02 |
| `merge` | `value.merge(a: Value, b: Value) -> Value` | Elixir `Map.merge` / JS `{...a, ...b}` / Python `a \| b` | 保持 | — |
| `null` | `value.null() -> Value` | serde `Value::Null` / gleam_json `json.null` / Kotlin pkg `JsonNull` | 保持 | — |
| `object` | `value.object(pairs: List[(String, Value)]) -> Value` | gleam_json `json.object` / Kotlin pkg `buildJsonObject` | 保持 | R-value-02 |
| `omit` | `value.omit(v: Value, keys: List[String]) -> Value` | Elixir `Map.drop` / lodash `omit` | 保持 | — |
| `pick` | `value.pick(v: Value, keys: List[String]) -> Value` | Elixir `Map.take` / lodash `pick` | 保持 | — |
| `str` | `value.str(s: String) -> Value` | gleam_json `json.string` / serde `Value::String` | 改名 → `string` | R-value-08 |
| `stringify` | `value.stringify(v: Value) -> String` | JS `JSON.stringify` / Python `dumps` (F) | 移除 → `json.stringify` | R-value-01 |
| `to_camel_case` | `value.to_camel_case(v: Value) -> Value` | Kotlin pkg `JsonNamingStrategy` / Swift `keyEncodingStrategy`（类型设置） | 移除 | R-value-05 |
| `to_snake_case` | `value.to_snake_case(v: Value) -> Value` | 同上 | 移除 | R-value-05 |
| `field` | `value.field(v: Value, key: String) -> Result[Value, String]` | serde `get`（Option） / Gleam `decode.field` / Elixir `Access.key` (F) | 移除 → `value.get`（Option） | R-value-07 |
| `keys` | `value.keys(v: Value) -> List[String]` | serde `as_object()?.keys()` / JS `Object.keys` / Python `d.keys()` | 移除 → `as_object` | R-value-04 |
| `as_object` | `value.as_object(v: Value) -> Result[List[(String, Value)], String]`（提案） | serde `as_object` / Kotlin pkg `jsonObject` / Gleam `decode.dict` | 新增 | R-value-02 |
| `get_string` | `value.get_string(v: Value, key: String) -> Option[String]`（提案） | serde `v["k"].as_str()` | 新增（`json.get_string` 移入） | R-value-02 |
| `get_int` | `value.get_int(v: Value, key: String) -> Option[Int]`（提案） | serde `v["k"].as_i64()` | 新增（`json.get_int` 移入） | R-value-02 |
| `get_float` | `value.get_float(v: Value, key: String) -> Option[Float]`（提案） | serde `v["k"].as_f64()` | 新增（`json.get_float` 移入） | R-value-02 |
| `get_bool` | `value.get_bool(v: Value, key: String) -> Option[Bool]`（提案） | serde `v["k"].as_bool()` | 新增（`json.get_bool` 移入） | R-value-02 |
| `get_array` | `value.get_array(v: Value, key: String) -> Option[List[Value]]`（提案） | serde `v["k"].as_array()` | 新增（`json.get_array` 移入） | R-value-02 |
| `get_object` | `value.get_object(v: Value, key: String) -> Option[List[(String, Value)]]`（提案） | serde `v["k"].as_object()` | 新增 | R-value-02 |
| `root` | `value.root() -> Path`（提案） | serde `pointer("")` | 新增（`json.root` 移入） | R-value-03 |
| `key` | `value.key(p: Path, name: String) -> Path`（提案） | Elixir `Access.key` (F) | 新增（`json.field` 移入） | R-value-03 |
| `index` | `value.index(p: Path, i: Int) -> Path`（提案） | Elixir `Access.at` (F) | 新增（`json.index` 移入；负下标越界） | R-value-03 |
| `get_path` | `value.get_path(v: Value, p: Path) -> Option[Value]`（提案） | Elixir `get_in` (F) / serde `pointer` | 新增（`json.get_path` 移入） | R-value-03 |
| `set_path` | `value.set_path(v: Value, p: Path, x: Value) -> Result[Value, String]`（提案） | Elixir `put_in` (F) / serde `pointer_mut` | 新增（`json.set_path` 移入） | R-value-03 |
| `remove_path` | `value.remove_path(v: Value, p: Path) -> Value`（提案） | Elixir `pop_in` (F) | 新增（`json.remove_path` 移入） | R-value-03 |
| `get` | `value.get(v: Value, key: String) -> Option[Value]`（提案） | Rust JSON `Value::get` / Python `dict.get` / Elixir `Map.get` | 新增 | R-value-07 |
