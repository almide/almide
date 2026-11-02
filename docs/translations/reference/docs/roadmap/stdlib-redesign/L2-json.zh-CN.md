<!-- description: Stdlib重设计L2/L3 — json仅保留文本codec(parse,stringify,stringify_pretty)；typed getter和JsonPath移至value；parse拒绝尾随文本 -->
<a id="l2l3-json"></a>

# L2/L3: json

[![English](https://img.shields.io/badge/English-d0d7de?style=flat-square)](L2-json.md) [![日本語](https://img.shields.io/badge/%E6%97%A5%E6%9C%AC%E8%AA%9E-d0d7de?style=flat-square)](L2-json.ja.md) [![简体中文](https://img.shields.io/badge/%E7%AE%80%E4%BD%93%E4%B8%AD%E6%96%87-24292f?style=flat-square)](L2-json.zh-CN.md)

> 非规范性带日期重设计快照，固定原文权威且不变，不自动同步、无新使用量测量或实现核实。区分采纳提案、旧实测行为及迁移gate。保留九语言均拒绝的宽泛表述与后文有标准/官方pkg的七语言说明。不静默调和R-json-01迁移正文与L3 result.map组合及旧参数名表。深度限制提案不是一般资源安全保证。 [Source](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/stdlib-redesign/L2-json.md)

> 最后更新：2026-10-21
> 前提：[L0](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/stdlib-redesign/L0-conventions.md)、[L1](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/stdlib-redesign/L1-categories.md)（R-L1-27）、对应页[L2-value](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/stdlib-redesign/L2-value.md)。
> 来源：Rust —（标准库没有，serde_json外部 https://docs.rs/serde_json ）；Go https://pkg.go.dev/encoding/json ；Python https://docs.python.org/3/library/json.html (F)；TS/JS https://developer.mozilla.org/en-US/docs/Web/JavaScript/Reference/Global_Objects/JSON ；Gleam —（官方pkg https://hexdocs.pm/gleam_json/ ）；Elixir https://elixir.hexdocs.pm/JSON.html ；OCaml —（标准库没有）；Kotlin —（官方pkg https://kotlinlang.org/api/kotlinx.serialization/kotlinx-serialization-json/ ）；Swift https://developer.apple.com/documentation/foundation/jsonserialization
> 实测：`almide 0.68.0 (dev, 437b8f127)`，native和`--target wasm`；probe在scratchpad `probe-json/p1.almd`。

<a id="l2-役割の比較"></a>

## L2：角色比较

| 角色 | Almide | Rust | Go | Python | TS/JS | Gleam | Elixir | OCaml | Kotlin | Swift |
|---|---|---|---|---|---|---|---|---|---|---|
| 文本 → 值 | `json.parse` | pkg `from_str` | `Unmarshal` | `loads` (F) | `JSON.parse` | pkg `json.parse` | `JSON.decode` | — | pkg `Json.parseToJsonElement` | `JSONSerialization.jsonObject` |
| 值 → 文本（紧凑） | `json.stringify`、`value.stringify`（输出相同） | pkg `to_string` | `Marshal` | `dumps` (F) | `JSON.stringify` | pkg `json.to_string` | `JSON.encode!` | — | pkg `toString()` | `JSONSerialization.data` |
| 值 → 文本（缩进） | `json.stringify_pretty` | pkg `to_string_pretty` | `MarshalIndent` | `dumps(indent=)` (F) | `JSON.stringify(v, null, 2)` | — | — | — | pkg `prettyPrint` | `.prettyPrinted` |
| 带多余尾随字符的文本 | **接受**（`"1 garbage"` → `1`） | pkg err | err | err（`Extra data`）(F) | SyntaxError | pkg err | err | — | pkg err | err |
| 按key提取带类型值 | `json.get_string` 及另5个 | pkg `v["k"].as_str()` | — | `d.get("k")` | `o.k` | — | — | — | pkg `jsonObject["k"]?.jsonPrimitive` | — |
| 按路径读写深处位置 | `json.root`/`field`/`index` + `get_path`/`set_path`/`remove_path` | pkg `pointer` | — | — | — | — | `get_in`/`put_in`/`pop_in` + `Access.key`/`Access.at` (F) | — | — | — |

<a id="完全性の規則"></a>

### 完整性规则

1. **`json`仅与文本互转**（R-L1-27）：`parse`（文本→`Value`）、`stringify`、`stringify_pretty`（`Value`→文本）。构造、读取值树的函数，不涉及文本就放在`value`。
2. `parse`遵循RFC 8259语法，文档后非空白字符报err。
3. `stringify`和`stringify_pretty`按同样值规则写出：非有限Float为`null`，`1.0`为`1`。

<a id="l2-の判定"></a>

### L2判断

- 位置：`get_string`、`get_int`、`get_float`、`get_bool`、`get_array`，`root`、`field`、`index`、`get_path`、`set_path`、`remove_path`，以及`to_map`共12个函数是无文本的树操作 → R-json-01（R-L1-27）。
- 重复：`value.stringify`和`json.stringify`输出相同，两target实测`eq true`。撤去`value`侧，见[L2-value](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/stdlib-redesign/L2-value.md) R-value-01。
- 接受范围：`json.parse("1 garbage")` → `ok(1)`；`json.parse("{\"a\":1} {")` → `ok({"a":1})`，两target实测。九种语言均不接受 → R-json-02。
- `json.field(path, name)`构造路径，`value.field(v, key)`读取值，同写法异义。迁移时区分名称，R-json-01。

<a id="案"></a>

## 提案

<a id="r-json-01-型つきの取り出しと道筋の操作を-value-に移す---採用"></a>

### R-json-01 将Typed Getter和路径操作移到`value` — ○ 采纳

- 内容：新写法在[L2-value](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/stdlib-redesign/L2-value.md) R-value-02/03决定。

  | 撤去写法 | 目的地 |
  |---|---|
  | `json.get_string`・`get_int`・`get_float`・`get_bool`・`get_array` | `value.get_string`・`get_int`・`get_float`・`get_bool`・`get_array`（同一签名） |
  | `json.root()` | `value.root()` |
  | `json.field(path, name)` | `value.key(path, name)`（与`value.field`区分名称） |
  | `json.index(path, i)` | `value.index(path, i)`（负索引越界，不从末尾计数） |
  | `json.get_path`・`set_path`・`remove_path` | `value.get_path`・`set_path`・`remove_path`（同一签名） |
  | `type json.JsonPath` | `type value.Path` |
  | `json.to_map(j)` → `Option[Map[String, String]]` | `value.as_object(v)`与`map.from_list`（值仍为`Value`） |

- L0依据：R-L1-27、R-L0-02（主语是`Value`与路径，非文本）、R-L0-00条件1（json/value中`field`异义）、R-L0-12（负索引越界）、R-L0-14。
- (a) Kotlin以`Json` / `JsonElement`分离文本转换和值树，Gleam用`gleam_json` / `dynamic`。路径部件名称近似Elixir `Access.key` / `Access.at` (F)。
- (b) 纯计算，原样移动本体。
- (c) `value`自动import，无需`import json`即可用。目前仅用`json.get_int`也需`import json`。
- (d) 撤去12函数、改名1类型。各函数添加指向目的地的`@deprecated(since=15, use="value.get_int")`等注解，`use`按表中目的地，`removal_epoch = 16`撤去。`almide fix`一对一改写并删除不再使用的`import json`。向`json.index`传负索引的调用及`json.to_map`形态会变，用hint说明。使用数：五个`get_*`函数spec 54 / tests 0 / docs 52 / stdlib 3 / research 19；六个路径函数spec 262 / tests 20 / docs 76 / stdlib 0；`to_map` spec 4 / tests 0 / docs 15 / stdlib 3。
- Gate提案：R-L0-G的R-L0-02检查确认`json`公开函数接收或返回`String`。
- Issue：#3953。

<a id="r-json-02-parse-は文書の後ろの余計な文字を-err-にする---採用"></a>

### R-json-02 `parse`对文档尾部多余文本报err — ○ 采纳

- 内容：`json.parse`若值后有非空白字符，返回`err("unexpected trailing text at pos N")`；名称及类型不变。
- L0依据：R-L0-06第4项，可失败转换不以值隐藏失败。这是收窄定义域，不改变正确JSON答案，故不属R-L0-14第4项“同名不同结果”；按L0，收窄定义域不升epoch。
- (a) 九种语言中，标准库或官方pkg有JSON的七种均拒绝，Python报`Extra data` (F)。
- (b) 纯计算，两target parser是同一self-host实现，会同时改变。
- (c) 当时拼接两个JSON的文本或截断log行，都会只取首个值静默成功。
- (d) 语义改变、收窄定义域。不列epoch `breaks`，记录dialect epoch 15 `notes`。变更前统计spec/tests/下游中用`parse`读取尾部有残余字符文本的位置。`json.parse`本身使用数：spec 203 / tests 41 / docs 77 / stdlib 3 / research 29。
- Gate提案：在`spec/stdlib/json_test.almd`两target放两例：尾部仅空白→ok，尾部字符→err。将JSONTestSuite `n_structure_trailing_*`系列加入differential fuzz种子。
- Issue：#3921。

<a id="r-json-03-stringify-と-stringify_pretty-を据え置く---採用変更なし"></a>

### R-json-03 保持`stringify`与`stringify_pretty` — ○ 采纳，不变

- 内容：保留两函数；`stringify_pretty`参数名`j`统一为`v`，不改变签名含义，仅文档改变。
- L0依据：R-L0-00，`stringify`与JS同写法，无多数但也无冲突。
- Issue：无。

<a id="r-json-04-parse-の入れ子の深さに上限を置く---採用"></a>

### R-json-04 限制`parse`嵌套深度 — ○ 采纳

- 当时：`json.parse`递归下降，无嵌套深度上限（`runtime/rs/src/json.rs:47-223`、`stdlib/json_parse.almd`）。深层`[[[[…]]]]`文本以栈溢出abort结束，C-196。
- 提案：array/object嵌套超过1000层，返回`err("json: nesting deeper than 1000")`。
- L0依据：R-L0-47第2项。不提供调整上限函数，属R-L0-47第3项例外。所比语言仅Rust `serde_json`的`disable_recursion_limit`能改变深度，且只是移除上限。
- (a) Go `encoding/json`为10000，Rust `serde_json`为128，Python `json`依递归上限、默认约1000。
- (d) 语义改变：超过1000层由abort变err。当时超过1000层仍成功的输入，本已可能依native栈大小而abort。
- Issue：#4020。

<a id="関連-codec-で型に-decode-するときr-l0-48"></a>

### 相关：Codec解码到类型，R-L0-48

将`json.parse`值读入类型的是Codec导出的`T.decode`和`T.decode_strict`，而非json模块函数。`decode_strict`拒绝未知key，`@skip_decode`标记不从输入读取的field。规则见L0 R-L0-48。

<a id="l3-関数表"></a>

## L3：函数表

| 函数 | 签名（当时，显示mut） | 其他语言主要写法 | 判断 | 提案 |
|---|---|---|---|---|
| `parse` | `json.parse(text: String) -> Result[Value, String]` | JS `JSON.parse` / Python `loads` (F) / Go `Unmarshal` / Elixir `JSON.decode` | 语义改变：尾随文本err，嵌套超过1000层err | R-json-02、R-json-04 |
| `stringify` | `json.stringify(v: Value) -> String` | JS `JSON.stringify` / Python `dumps` (F) / Go `Marshal` / Elixir `JSON.encode!` | 保留 | R-json-03 |
| `stringify_pretty` | `json.stringify_pretty(j: Value) -> String` | Go `MarshalIndent` / Python `dumps(indent=2)` (F) / serde `to_string_pretty` | 保留 | R-json-03 |
| `get_string` | `json.get_string(j: Value, key: String) -> Option[String]` | Kotlin pkg `jsonPrimitive.contentOrNull` / serde `as_str()` | 撤去 → `value.get_string` | R-json-01 |
| `get_int` | `json.get_int(j: Value, key: String) -> Option[Int]` | serde `as_i64()` / Kotlin pkg `intOrNull` | 撤去 → `value.get_int` | R-json-01 |
| `get_float` | `json.get_float(j: Value, key: String) -> Option[Float]` | serde `as_f64()` / Kotlin pkg `doubleOrNull` | 撤去 → `value.get_float` | R-json-01 |
| `get_bool` | `json.get_bool(j: Value, key: String) -> Option[Bool]` | serde `as_bool()` / Kotlin pkg `booleanOrNull` | 撤去 → `value.get_bool` | R-json-01 |
| `get_array` | `json.get_array(j: Value, key: String) -> Option[List[Value]]` | serde `as_array()` / Kotlin pkg `jsonArray` | 撤去 → `value.get_array` | R-json-01 |
| `root` | `json.root() -> JsonPath` | serde `pointer("")` | 撤去 → `value.root` | R-json-01 |
| `field` | `json.field(path: JsonPath, name: String) -> JsonPath` | Elixir `Access.key` (F) | 撤去 → `value.key` | R-json-01 |
| `index` | `json.index(path: JsonPath, i: Int) -> JsonPath` | Elixir `Access.at` (F) | 撤去 → `value.index`（负索引越界） | R-json-01 |
| `get_path` | `json.get_path(j: Value, path: JsonPath) -> Option[Value]` | Elixir `get_in` (F) / serde `pointer` | 撤去 → `value.get_path` | R-json-01 |
| `set_path` | `json.set_path(j: Value, path: JsonPath, value: Value) -> Result[Value, String]` | Elixir `put_in` (F) / serde `pointer_mut` | 撤去 → `value.set_path` | R-json-01 |
| `remove_path` | `json.remove_path(j: Value, path: JsonPath) -> Value` | Elixir `pop_in` (F) | 撤去 → `value.remove_path` | R-json-01 |
| `to_map` | `json.to_map(j: Value) -> Option[Map[String, String]]` | serde `as_object()` / Kotlin pkg `jsonObject` | 撤去 → `value.as_object(v) \|> result.map(map.from_list)` | R-json-01 |
