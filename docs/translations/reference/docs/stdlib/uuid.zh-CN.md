# uuid

[![English](https://img.shields.io/badge/English-d0d7de?style=flat-square)](uuid.md) [![日本語](https://img.shields.io/badge/%E6%97%A5%E6%9C%AC%E8%AA%9E-d0d7de?style=flat-square)](uuid.ja.md) [![简体中文](https://img.shields.io/badge/%E7%AE%80%E4%BD%93%E4%B8%AD%E6%96%87-24292f?style=flat-square)](uuid.zh-CN.md)



> 原文：[uuid.md @ 35297a6](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/stdlib/uuid.md)。这是供读者参考的非规范性固定快照，不参与自动生成或自动同步。签名、代码示例和 API 索引注释保持原文，说明译文附于文末。本次翻译未独立复核测量、行为或示例。
>
> 原文存在不一致：开头称所有函数均可在 native/wasm 运行，v7 段末却称“目前仅 native”。译文保留两者，未独立核实哪个描述符合当前实现。

RFC 9562 标识符：随机的 version 4、按时间排序的 version 7、8-4-4-4-12 文本形式和 16 个原始字节。`import uuid`。

`uuid.Uuid` 就是其 16 个字节。它是不透明类型，只有本模块能构造，因此每个 `Uuid` 恰含 16 字节。生成是 `effect fn`（`v4` 读取随机源，`v7` 还读取时钟）；转换和比较是普通 `fn`s。

`v4()` 和 `v7()` 返回普通 `Uuid`，但作为 `effect fn`s，其调用产生 `Result`：写作 `uuid.v4()!`。

**目标。** 每个函数都在 native 和 wasm 上运行。在 wasm 上，`v7` 将最后的 stamp 保存在模块实例中，因此与 native 一样，同次运行中生成的每个 `Uuid` 都大于前一个。

<a id="making-one"></a>

## 生成

### `uuid.v4() -> Uuid`

随机（version 4）`Uuid`：来自 OS 密码学随机源的 122 个随机位。

```almd check
import uuid

effect fn main() -> Unit = {
  let id = uuid.v4()!
  println(uuid.to_string(id))
}
```

### `uuid.v7() -> Uuid`

按时间排序的（version 7）`Uuid`：Unix 毫秒时间、计数器，再加随机位。在同一进程内，每个 `v7` 都大于前一个，即使同一毫秒生成两个也如此，所以用 `compare` 排序就是生成顺序，适合随时间插入的行的键。目前仅 native。

```almd check
import uuid

effect fn main() -> Unit = {
  let a = uuid.v7()!
  let b = uuid.v7()!
  println(if uuid.compare(a, b) < 0 then "ordered" else "not ordered")
}
```

<a id="text-and-bytes"></a>

## 文本与字节

### `uuid.parse(s: String) -> Result[Uuid, String]`

`s` 表示的 `Uuid`：8-4-4-4-12 十六进制数字，大小写均可，可选放在 `{...}` 内或 `urn:uuid:` 后。其他情况为 `err`（`uuid.parse: not an 8-4-4-4-12 UUID: "..."`）。

### `uuid.to_string(u: Uuid) -> String`

将 `u` 表为 8-4-4-4-12 小写十六进制数字。

### `uuid.from_bytes(b: Bytes) -> Result[Uuid, String]`

恰好 16 字节构成的 `Uuid`；其他长度为 `err`。

### `uuid.to_bytes(u: Uuid) -> Bytes`

`u` 的 16 个字节。

```almd run
import uuid

effect fn main() -> Unit = {
  let u = uuid.parse("{550E8400-E29B-41D4-A716-446655440000}")!
  println(uuid.to_string(u))
  println("${bytes.len(uuid.to_bytes(u))} bytes")
}
```
```output
550e8400-e29b-41d4-a716-446655440000
16 bytes
```

<a id="ordering"></a>

## 排序

### `uuid.compare(a: Uuid, b: Uuid) -> Int`

从第一个字节起逐字节比较，`a` 位于 `b` 之前、相同或之后时分别为负、零或正。对 `v7` 值，这就是生成顺序。



<a id="signature-index-7-functions"></a>

## 固定快照 API 索引 (7 个函数)

```
// A random (version 4) Uuid: 122 bits from the OS CSPRNG.
// @since unreleased
effect uuid.v4() -> Uuid

// A time-ordered (version 7) Uuid: Unix ms, a counter, random bits; each is greater than the last.
// @since unreleased
effect uuid.v7() -> Uuid

// The Uuid s spells: 8-4-4-4-12 hex digits, either case, optionally in {…} or after urn:uuid:.
// @since unreleased
uuid.parse(s: String) -> Result[Uuid, String]

// u as 8-4-4-4-12 lowercase hex digits.
// @since unreleased
uuid.to_string(u: Uuid) -> String

// The Uuid of exactly 16 bytes b; err for any other length.
// @since unreleased
uuid.from_bytes(b: Bytes) -> Result[Uuid, String]

// The 16 bytes of u.
// @since unreleased
uuid.to_bytes(u: Uuid) -> Bytes

// Negative, zero or positive as a orders before, with or after b, byte by byte.
// @since unreleased
uuid.compare(a: Uuid, b: Uuid) -> Int
```



## API 说明译文（按上述签名顺序）

- `effect uuid.v4() -> Uuid` — 随机（version 4）Uuid：来自 OS CSPRNG 的 122 位。
- `effect uuid.v7() -> Uuid` — 按时间排序的（version 7）Uuid：Unix ms、计数器、随机位；每个都大于上一个。
- `uuid.parse(s: String) -> Result[Uuid, String]` — s 表示的 Uuid：8-4-4-4-12 十六进制数字，大小写均可，可选置于 {…} 内或 urn:uuid: 后。
- `uuid.to_string(u: Uuid) -> String` — u 的 8-4-4-4-12 小写十六进制表示。
- `uuid.from_bytes(b: Bytes) -> Result[Uuid, String]` — 恰好 16 字节 b 的 Uuid；其他长度为 err。
- `uuid.to_bytes(u: Uuid) -> Bytes` — u 的 16 个字节。
- `uuid.compare(a: Uuid, b: Uuid) -> Int` — 逐字节比较，a 在 b 之前、相同、之后时分别为负、零、正。
