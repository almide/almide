# json

[![English](https://img.shields.io/badge/English-d0d7de?style=flat-square)](json.md) [![日本語](https://img.shields.io/badge/%E6%97%A5%E6%9C%AC%E8%AA%9E-d0d7de?style=flat-square)](json.ja.md) [![简体中文](https://img.shields.io/badge/%E7%AE%80%E4%BD%93%E4%B8%AD%E6%96%87-24292f?style=flat-square)](json.zh-CN.md)



> 原文：[json.md @ 35297a6](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/stdlib/json.md)。这是供读者参考的非规范性固定快照。API 索引取自固定版本原文，本译文不参与自动生成或自动同步。签名、类型、`@since` 及代码示例（包括注释）保持原文，API 说明译文列于文末。本次翻译未独立执行代码示例。

> 翻译说明：关于与前端解析器之间解释差异的表述来自固定原文，不表示本译文验证或证明了所有解析器的正确性或相互兼容性。文末把 Float 到 Int 的转换称为“拓宽”也沿用原文，译者未自行订正规范或统一其表述。

JSON **格式**：解析和打印 `Value` 树。import json。

数据模型本身位于 `value.*`（自动导入）：构造器（`value.null` / `value.int` / `value.string` / `value.object` / …）、访问器（`value.as_*` / `value.get` / `value.as_object`）及路径（`value.root` / `value.key` / `value.get_path`），见 [value.md](../../../../stdlib/value.md)。这些操作原来的 `json.*` 写法是已退役的别名，在一个版本的 E040 弃用期后删除；迁移对照表见文末[已更名的操作](#renamed-operations)。

通过 `value.root` / `value.key` / `value.index` 构造的路径类型是 `value.Path`；处理路径的辅助函数应使用此类型注解。

### `json.parse(text: String) -> Result[Value, String]`

将 JSON 字符串解析为 Value。

```almd run
import json

fn main() -> Unit = {
  let v = json.parse("{\"name\": \"Alice\"}")
  match v {
    ok(j) => println(json.stringify(j)),
    err(e) => println("parse error: ${e}"),
  }
}
```
```output
{"name":"Alice"}
```

语法是 RFC 8259 加 I-JSON（RFC 7493），采用**严格**解析（ALS-T3、C-400 / C-401）：整个文本是 JSON 空白（空格、制表符、LF、CR）之间的一个值，其他输入均为 `err`，每种情况对应一种错误文本，所有目标一致。`N` 是问题起始处的字符索引。

| 输入 | 错误 |
|---|---|
| `"ad\ud800min"`（不是高低代理对的代理码元） | `json: lone surrogate at pos N` |
| `"ad\qmin"`（不是 `\" \\ \/ \b \f \n \r \t \u` 的转义） | `json: unknown escape at pos N` |
| `"\u12"`（`\u` 后未跟随四个有效的十六进制数字） | `json: invalid unicode escape at pos N` |
| 字符串中的原始 U+0000–U+001F | `json: control character in string at pos N` |
| U+FDD0–U+FDEF、U+xFFFE、U+xFFFF（转义或原始形式） | `json: noncharacter at pos N` |
| `"abc` | `json: unterminated string at pos N` |
| `{"a":1,"a":2}` | `json: duplicate key at pos N` |
| `[1,2] [3]`, `{} garbage` | `json: trailing data at pos N` |
| `{x: 1}` | `json: unquoted key at pos N` |
| `[1,]`, `{"a":1,}` | `json: trailing comma at pos N` |
| `{"a" 1}` | `json: expected ':' at pos N` |
| `[1 2]` / `{"a":1 "b":2}` | `json: expected ',' or ']' at pos N` / `json: expected ',' or '}' at pos N` |
| `01`, `1.`, `-`, `1e` | `json: invalid number at pos N` |

不丢弃、不猜测：孤立代理码元或未知转义从不删除（`ad\ud800min` 不会变成 `admin`），重复键也不会采用先者优先或后者优先，因此位于程序前端的解析器不能从相同字节读出不同文档。

```almd run
import json

fn main() -> Unit = {
  for text in ["{\"user\":\"ad\\ud800min\"}", "{\"user\":\"alice\",\"user\":\"admin\"}", "[1,2] [3]"] {
    match json.parse(text) {
      ok(j) => println(json.stringify(j)),
      err(e) => println(e),
    }
  }
}
```
```output
json: lone surrogate at pos 11
json: duplicate key at pos 16
json: trailing data at pos 6
```

### `json.stringify(v: Value) -> String`

将 Value 转换为 JSON 字符串。非有限 Float（NaN、+inf、-inf）没有 JSON 写法，会写成 `null`，因此输出总能重新解析；`json.stringify_pretty` 同样遵循此规则。

```almd run
import json

type Person: Codec = { name: String, age: Int }

fn main() -> Unit = {
  let person = Person { name: "Alice", age: 30 }
  println(json.stringify(person.encode()))
}
```
```output
{"name":"Alice","age":30}
```

### 带类型解码：`T.decode` 和 `T.decode_strict`

`: Codec` 记录类型 `T` 派生 `T.decode`、`T.decode_strict`（以及 `T.encode`）。`T.decode` 忽略未知键；`T.decode_strict` 递归拒绝未知键，包括嵌套记录和容器。它在每个记录中先检查键再解码字段；嵌套错误包括到失败值的路径。标有 `@skip_decode` 的字段从不读取输入（保留默认值），但仍编码；`@skip` 既不读取也不写入。对于不可信输入，最好解码到仅包含客户端可设置内容的类型（CHEATSHEET，JSON Rule 5）。

```almd run
import json

type User: Codec = { name: String, @skip_decode is_admin: Bool = false }

fn main() -> Unit = {
  let body = json.parse("{\"name\": \"m\", \"is_admin\": true}") ?? value.null()
  match User.decode(body) {
    ok(u) => println("${u.name} admin=${u.is_admin}"),
    err(e) => println(e),
  }
  match User.decode_strict(body) {
    ok(u) => println("${u.name} admin=${u.is_admin}"),
    err(e) => println(e),
  }
}
```
```output
m admin=false
unknown key 'is_admin'
```

### `json.stringify_pretty(j: Value) -> String`

将 Json 值转换为带缩进的美化 JSON 字符串。非有限 Float 与 `json.stringify` 一样写成 `null`。

```almd run
import json

fn main() -> Unit = {
  let j = json.parse("{\"name\": \"Alice\", \"tags\": [\"a\", \"b\"]}") ?? value.null()
  println(json.stringify_pretty(j))
}
```
```output
{
  "name": "Alice",
  "tags": [
    "a",
    "b"
  ]
}
```



## 固定快照 API 索引（3个函数）

```
// Strict JSON Value or err; trailing non-whitespace text and nesting over 1000 are errors.
// @since 0.5.0 or earlier
json.parse(text: String) -> Result[Value, String]

// Compact JSON; a Float 1.0 prints as 1, NaN/inf as null.
// @since 0.5.0 or earlier
json.stringify(v: Value) -> String

// JSON with 2-space indent; [] and {} inline, NaN/inf null.
// @since 0.5.0 or earlier
json.stringify_pretty(v: Value) -> String
```



<a id="renamed-operations"></a>

## 已更名的操作

经过一个版本的弃用期（E040，v0.53.1 – v0.53.4）后，在 #1078 中删除。已退役写法现在产生普通的 E002 未定义函数错误。

| 已退役 | 现行 |
|---|---|
| `json.null` / `json.object` / `json.array` | `value.null` / `value.object` / `value.array` |
| `json.from_string` / `json.from_int` / `json.from_bool` / `json.from_float` | `value.string` / `value.int` / `value.bool` / `value.float` |
| `json.as_string` … `json.as_array`（曾返回 `Option`） | `value.as_string(v)?` … `value.as_array(v)?` — `?` 将 `Result` 转为相同的 `Option` |
| `json.get(j, k)`（曾返回 `Option`） | `value.get(j, k)` |

注意一处行为收紧：已删除的 `json.as_int` 曾将 Float 值拓宽为 Int；`value.as_int` 不会这样做（`err("expected Int")`）。拓宽方向由接受 Int 的 `value.as_float` 提供。


## API 说明译文（按上述签名顺序）

- `json.parse(text: String) -> Result[Value, String]` — 严格 JSON Value 或 err；尾部非空白文本及超过1000层的嵌套都是错误。
- `json.stringify(v: Value) -> String` — 紧凑 JSON；Float 1.0 打印为1，NaN/inf 为 null。
- `json.stringify_pretty(v: Value) -> String` — 2空格缩进的 JSON；[] 和 {} 行内显示，NaN/inf 为 null。
