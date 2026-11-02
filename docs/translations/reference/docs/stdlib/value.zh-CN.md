# value

[![English](https://img.shields.io/badge/English-d0d7de?style=flat-square)](value.md) [![日本語](https://img.shields.io/badge/%E6%97%A5%E6%9C%AC%E8%AA%9E-d0d7de?style=flat-square)](value.ja.md) [![简体中文](https://img.shields.io/badge/%E7%AE%80%E4%BD%93%E4%B8%AD%E6%96%87-24292f?style=flat-square)](value.zh-CN.md)



> 原文：[value.md @ 35297a6](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/stdlib/value.md)。这是供读者参考的非规范性固定快照，不参与自动生成或自动同步。签名、代码示例和 API 索引注释保持原文，说明译文附于文末。本次翻译未独立复核测量、行为或示例。

动态 `Value` **数据模型**：构造器和访问器。自动导入。`json` 模块是其上的格式（`json.parse` / `json.stringify`），参见 [json.md](../../../../stdlib/json.md)。

### `value.as_string(v: Value) -> Result[String, String]`

从 Value 提取 String，不是 Str 则返回 err。

### `value.as_int(v: Value) -> Result[Int, String]`

从 Value 提取 Int，不是 Int 则返回 err。

### `value.as_float(v: Value) -> Result[Float, String]`

从 Value 提取 Float，将 Int payload 拓宽为 Float；其他种类返回 err。

### `value.as_bool(v: Value) -> Result[Bool, String]`

从 Value 提取 Bool，不是 Bool 则返回 err。

### `value.as_array(v: Value) -> Result[List[Value], String]`

从 Value 提取 List[Value]，不是 Array 则返回 err。

### `value.string(s: String) -> Value`

从 String 创建 Value。

### `value.int(n: Int) -> Value`

从 Int 创建 Value。

### `value.float(f: Float) -> Value`

从 Float 创建 Value。

### `value.bool(b: Bool) -> Value`

从 Bool 创建 Value。

### `value.object(pairs: List[(String, Value)]) -> Value`

从键值对列表创建 Value object。任何 object 都不会持有重复键（C-401、#4112）：列表重复的键保留首次出现的位置与最后的值（C-380），所以 `value.get`、`value.as_object` 和每个 serializer 都只看到一个 entry。`json.stringify(value.object([("role", value.string("user")), ("role", value.string("admin"))]))` 是 `{"role":"admin"}`，绝不会是含两个 `role` 的文档。

### `value.array(items: List[Value]) -> Value`

从 Value 列表创建 Value array。

### `value.null() -> Value`

创建 null Value。

### `value.pick(v: Value, keys: List[String]) -> Value`

从 Object 选取指定键，丢弃其余键。非 object 值不含任何这些键，因此选取结果为 `{}`（C-401）：非 object 值上的 allowlist 不放行任何内容。

### `value.omit(v: Value, keys: List[String]) -> Value`

从 Object 移除指定键。

### `value.merge(a: Value, b: Value) -> Value`

合并两个 Object，b 的键覆盖 a 的键。



<a id="signature-index-29-functions"></a>

## 固定快照 API 索引 (29 个函数)

```
// Wraps xs as an Array Value.
// @since 0.6.0 or earlier
value.array(xs: List[Value]) -> Value

// Array items; err if v is another kind.
// @since 0.6.0 or earlier
value.as_array(v: Value) -> Result[List[Value], String]

// Bool payload; err otherwise (no truthiness).
// @since 0.6.0 or earlier
value.as_bool(v: Value) -> Result[Bool, String]

// Float payload (Int widens); err otherwise.
// @since 0.6.0 or earlier
value.as_float(v: Value) -> Result[Float, String]

// Int payload; err on Float, never truncated.
// @since 0.6.0 or earlier
value.as_int(v: Value) -> Result[Int, String]

// Str payload; err otherwise (no coercion).
// @since 0.6.0 or earlier
value.as_string(v: Value) -> Result[String, String]

// Value holding the Bool b.
// @since 0.6.0 or earlier
value.bool(b: Bool) -> Value

// Value holding the Float f.
// @since 0.6.0 or earlier
value.float(f: Float) -> Value

// Value holding the Int n.
// @since 0.6.0 or earlier
value.int(n: Int) -> Value

// Shallow merge, b's keys win; b if not both objects.
// @since 0.6.0 or earlier
value.merge(a: Value, b: Value) -> Value

// The JSON null Value.
// @since 0.6.0 or earlier
value.null() -> Value

// Wraps the pairs xs as an Object; a repeated key keeps its first position, last value.
// @since 0.6.0 or earlier
value.object(xs: List[(String, Value)]) -> Value

// v minus the listed keys; non-objects as-is.
// @since 0.6.0 or earlier
value.omit(v: Value, keys: List[String]) -> Value

// Only the listed keys of v; a non-object picks to {}.
// @since 0.6.0 or earlier
value.pick(v: Value, keys: List[String]) -> Value

// Value holding the String s.
// @since 0.6.0 or earlier
value.string(s: String) -> Value

// Object pairs in order; err if v is another kind.
// @since unreleased
value.as_object(v: Value) -> Result[List[(String, Value)], String]

// Value at key; none if absent or v is not an Object.
// @since unreleased
value.get(v: Value, key: String) -> Option[Value]

// String at key; none if absent or another kind.
// @since unreleased
value.get_string(v: Value, key: String) -> Option[String]

// Int at key; none if absent or not an Int (a Float is never truncated).
// @since unreleased
value.get_int(v: Value, key: String) -> Option[Int]

// Float at key (an Int widens); none if absent or another kind.
// @since unreleased
value.get_float(v: Value, key: String) -> Option[Float]

// Bool at key; none if absent or another kind.
// @since unreleased
value.get_bool(v: Value, key: String) -> Option[Bool]

// Array items at key; none if absent or another kind.
// @since unreleased
value.get_array(v: Value, key: String) -> Option[List[Value]]

// Object pairs at key; none if absent or another kind.
// @since unreleased
value.get_object(v: Value, key: String) -> Option[List[(String, Value)]]

// The empty path: the whole value.
// @since unreleased
value.root() -> Path

// p extended by the object key name.
// @since unreleased
value.key(p: Path, name: String) -> Path

// p extended by the array index i; a negative i is out of range.
// @since unreleased
value.index(p: Path, i: Int) -> Path

// Value at p, or none if a step misses.
// @since unreleased
value.get_path(v: Value, p: Path) -> Option[Value]

// v with x at p; missing keys are created, a missed index leaves v as is; err when the result would nest past 1000.
// @since unreleased
value.set_path(v: Value, p: Path, x: Value) -> Result[Value, String]

// v minus the entry at p; unchanged if absent; null for the root.
// @since unreleased
value.remove_path(v: Value, p: Path) -> Value
```



## API 说明译文（按上述签名顺序）

- `value.array(xs: List[Value]) -> Value` — 将 xs 包装为 Array Value。
- `value.as_array(v: Value) -> Result[List[Value], String]` — Array 元素；v 为其他种类则 err。
- `value.as_bool(v: Value) -> Result[Bool, String]` — Bool payload；否则 err（没有 truthiness）。
- `value.as_float(v: Value) -> Result[Float, String]` — Float payload（Int 拓宽）；否则 err。
- `value.as_int(v: Value) -> Result[Int, String]` — Int payload；Float 为 err，从不截断。
- `value.as_string(v: Value) -> Result[String, String]` — Str payload；否则 err（不强制转换）。
- `value.bool(b: Bool) -> Value` — 持有 Bool b 的 Value。
- `value.float(f: Float) -> Value` — 持有 Float f 的 Value。
- `value.int(n: Int) -> Value` — 持有 Int n 的 Value。
- `value.merge(a: Value, b: Value) -> Value` — 浅合并，b 的键优先；若不都是 object，则返回 b。
- `value.null() -> Value` — JSON null Value。
- `value.object(xs: List[(String, Value)]) -> Value` — 将 pairs xs 包装为 Object；重复键保留首次位置、最后值。
- `value.omit(v: Value, keys: List[String]) -> Value` — v 去掉列出的键；非 object 保持原样。
- `value.pick(v: Value, keys: List[String]) -> Value` — 仅 v 中列出的键；非 object 选取为 {}。
- `value.string(s: String) -> Value` — 持有 String s 的 Value。
- `value.as_object(v: Value) -> Result[List[(String, Value)], String]` — 按顺序排列的 Object 对；v 为其他种类则 err。
- `value.get(v: Value, key: String) -> Option[Value]` — key 处的 Value；不存在或 v 不是 Object 则 none。
- `value.get_string(v: Value, key: String) -> Option[String]` — key 处的 String；不存在或为其他种类则 none。
- `value.get_int(v: Value, key: String) -> Option[Int]` — key 处的 Int；不存在或非 Int 则 none（Float 从不截断）。
- `value.get_float(v: Value, key: String) -> Option[Float]` — key 处的 Float（Int 拓宽）；不存在或为其他种类则 none。
- `value.get_bool(v: Value, key: String) -> Option[Bool]` — key 处的 Bool；不存在或为其他种类则 none。
- `value.get_array(v: Value, key: String) -> Option[List[Value]]` — key 处的 Array 元素；不存在或为其他种类则 none。
- `value.get_object(v: Value, key: String) -> Option[List[(String, Value)]]` — key 处的 Object 对；不存在或为其他种类则 none。
- `value.root() -> Path` — 空 path，即整个值。
- `value.key(p: Path, name: String) -> Path` — 用 object 键 name 扩展 p。
- `value.index(p: Path, i: Int) -> Path` — 用 array 索引 i 扩展 p；负 i 为越界。
- `value.get_path(v: Value, p: Path) -> Option[Value]` — p 处的 Value；任一步未命中则 none。
- `value.set_path(v: Value, p: Path, x: Value) -> Result[Value, String]` — 在 p 处设为 x 的 v；创建缺失键，索引未命中则保持 v；结果嵌套会超过 1000 时 err。
- `value.remove_path(v: Value, p: Path) -> Value` — v 去掉 p 处的 entry；不存在则不变；root 得到 null。
