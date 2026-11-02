# bool

[![English](https://img.shields.io/badge/English-24292f?style=flat-square)](bool.md) [![日本語](https://img.shields.io/badge/%E6%97%A5%E6%9C%AC%E8%AA%9E-d0d7de?style=flat-square)](bool.ja.md) [![简体中文](https://img.shields.io/badge/%E7%AE%80%E4%BD%93%E4%B8%AD%E6%96%87-d0d7de?style=flat-square)](bool.zh-CN.md)

> This is a non-normative reader copy of the [pinned original](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/stdlib/bool.md), not a new authority, generator output or automatically synchronized document. Original sources and machine consumers are unchanged. Historical claims and examples have not been independently revalidated here.


The `Bool` module: read, order and print booleans. Auto-imported (no `import`).

Pure — no capability, identical on both targets. `bool.to_string` and
`bool.parse` are each other's inverse; `bool.compare` is the function form of
`<` on `Bool`, where `false < true`.

### `bool.parse(s: String) -> Result[Bool, String]`

`"true"` and `"false"` exactly; any other text, including `"True"` or `" true"`,
is an `err`.

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



## Signature index (3 functions)

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


