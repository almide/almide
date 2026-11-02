# bool

[![English](https://img.shields.io/badge/English-d0d7de?style=flat-square)](bool.md) [![日本語](https://img.shields.io/badge/%E6%97%A5%E6%9C%AC%E8%AA%9E-d0d7de?style=flat-square)](bool.ja.md) [![简体中文](https://img.shields.io/badge/%E7%AE%80%E4%BD%93%E4%B8%AD%E6%96%87-24292f?style=flat-square)](bool.zh-CN.md)



> 原文：[bool.md @ 35297a6](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/stdlib/bool.md)。这是供读者参考的非规范性固定快照。API 索引取自固定版本原文，本译文不参与自动生成或自动同步。签名、类型、`@since` 及代码示例（包括注释）保持原文，API 说明译文列于文末。本次翻译未独立执行代码示例。

`Bool` 模块：读取、排序和打印布尔值。自动导入（无需 `import`）。

纯函数，无需能力，两种目标行为相同。`bool.to_string` 与 `bool.parse` 互为逆操作；`bool.compare` 是 `Bool` 上 `<` 的函数形式，其中 `false < true`。

### `bool.parse(s: String) -> Result[Bool, String]`

只接受完全匹配的 `"true"` 和 `"false"`；其他文本，包括 `"True"` 或 `" true"`，均为 `err`。

```almd run
fn main() -> Unit = {
  println("${bool.parse("true") ?? false} ${bool.parse("yes") ?? false}")
}
```
```output
true false
```

### `bool.compare(a: Bool, b: Bool) -> Int`

```almd run
fn main() -> Unit = {
  println("${bool.compare(false, true)} ${bool.compare(true, true)} ${bool.compare(true, false)}")
}
```
```output
-1 0 1
```



## 固定快照 API 索引（3个函数）

```
// "true" or "false"; bool.parse reads it back.
// @since unreleased
bool.to_string(b: Bool) -> String

// ok(true) for "true", ok(false) for "false", err for anything else.
// @since unreleased
bool.parse(s: String) -> Result[Bool, String]

// Negative if a < b, 0 if equal, positive if a > b; false < true.
// @since unreleased
bool.compare(a: Bool, b: Bool) -> Int
```




## API 说明译文（按上述签名顺序）

- `bool.to_string(b: Bool) -> String` — "true" 或 "false"；bool.parse 可读回。
- `bool.parse(s: String) -> Result[Bool, String]` — "true" 得到 ok(true)，"false" 得到 ok(false)，其他内容得到 err。
- `bool.compare(a: Bool, b: Bool) -> Int` — a < b 时为负，相等时为0，a > b 时为正；false < true。
