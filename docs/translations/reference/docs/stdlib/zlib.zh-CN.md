# zlib

[![English](https://img.shields.io/badge/English-d0d7de?style=flat-square)](zlib.md) [![日本語](https://img.shields.io/badge/%E6%97%A5%E6%9C%AC%E8%AA%9E-d0d7de?style=flat-square)](zlib.ja.md) [![简体中文](https://img.shields.io/badge/%E7%AE%80%E4%BD%93%E4%B8%AD%E6%96%87-24292f?style=flat-square)](zlib.zh-CN.md)



> 原文：[zlib.md @ 35297a6](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/stdlib/zlib.md)。这是供读者参考的非规范性固定快照，不参与自动生成或自动同步。签名、代码示例和 API 索引注释保持原文，说明译文附于文末。本次翻译未独立复核测量、行为或示例。
>
> 原文注意：不保证压缩率或压缩字节一致，原文陈述的是匹配压缩/解压的往返。测试路径说明是英文原文记录，不表示此译文自动加入同一门禁。

三种容器格式的 DEFLATE 压缩。`import zlib`。

decoder（`decompress`、`inflate`、`gunzip` 及其 `_max` 形式）是返回 `Result[Bytes, String]` 的普通 fn：decode 是纯变换，两个目标输出逐字节相同，因此 pure fn 可调用它们（R-zlib-02）。compressor 仍是 `effect fn`s：native 使用宿主 zlib 库，压缩字节与 wasm 路径不同，因此输出不是仅由输入决定的纯函数。

**两个目标。** wasm 路径携带自托管 zlib（#1700、C-331 — `stdlib/zlib_inflate.almd` 与 `stdlib/zlib_deflate.almd`）：匹配的 compressor/decoder 对在 native 和 wasm 上恢复原始 payload 字节。压缩字节和大小可能不同：wasm 编码 stored block（有效但不缩小的 DEFLATE），因此承诺的是往返恢复，而非压缩率。`spec/stdlib/zlib_test.almd` 在 wasm test lane 运行（#2179）。

三种容器格式共享一个 DEFLATE core，但不能互换；请用对应函数解压。

| 格式 | 压缩 | 解压 |
|---|---|---|
| zlib (RFC 1950) | `compress` | `decompress` |
| raw DEFLATE (RFC 1951) | `deflate` | `inflate` |
| gzip (RFC 1952) | `gzip` | `gunzip` |

### `effect zlib.compress(b: Bytes) -> Bytes`

zlib 容器，默认 level。

```almd check
import zlib

effect fn pack(b: Bytes) -> Bytes = zlib.compress(b)!
```

### `effect zlib.compress_level(b: Bytes, level: Int) -> Bytes`

显式 level 的 zlib 容器，从 0（store）到 9（最小）。

### `zlib.decompress(b: Bytes) -> Result[Bytes, String]`

`compress` 的逆操作。

### `effect zlib.deflate(b: Bytes) -> Bytes`

raw DEFLATE，无 header、无 checksum；适合外层格式已经为 stream 提供框架的情况。

### `effect zlib.deflate_level(b: Bytes, level: Int) -> Bytes`

显式 level 的 raw DEFLATE。

### `zlib.inflate(b: Bytes) -> Result[Bytes, String]`

`deflate` 的逆操作。

### `effect zlib.gzip(b: Bytes) -> Bytes`

gzip 容器，即磁盘上的 `.gz` 格式和 `Content-Encoding: gzip`。

### `zlib.gunzip(b: Bytes) -> Result[Bytes, String]`

`gzip` 的逆操作。



<a id="signature-index-11-functions"></a>

## 固定快照 API 索引 (11 个函数)

```
// Zlib-framed DEFLATE of b; bytes vary by target.
// @since 0.20.0 or earlier
effect zlib.compress(b: Bytes) -> Bytes

// Like compress; level clamped to 0..9, unused on wasm.
// @since 0.20.0 or earlier
effect zlib.compress_level(b: Bytes, level: Int) -> Bytes

// Inflated zlib stream; err on corrupt input, bytes after it, or output over 268435456 bytes.
// @since 0.20.0 or earlier
zlib.decompress(b: Bytes) -> Result[Bytes, String]

// Like decompress, output bound max_len bytes; max_len <= 0 aborts.
// @since unreleased
zlib.decompress_max(b: Bytes, max_len: Int) -> Result[Bytes, String]

// Raw DEFLATE, no header; bytes vary by target.
// @since 0.20.0 or earlier
effect zlib.deflate(b: Bytes) -> Bytes

// Like deflate; level clamped to 0..9, unused on wasm.
// @since 0.20.0 or earlier
effect zlib.deflate_level(b: Bytes, level: Int) -> Bytes

// Raw DEFLATE decoded; err on a corrupt stream, bytes after it, or output over 268435456 bytes.
// @since 0.20.0 or earlier
zlib.inflate(b: Bytes) -> Result[Bytes, String]

// Like inflate, output bound max_len bytes; max_len <= 0 aborts.
// @since unreleased
zlib.inflate_max(b: Bytes, max_len: Int) -> Result[Bytes, String]

// RFC 1952 gzip of b; bytes vary by target.
// @since 0.20.0 or earlier
effect zlib.gzip(b: Bytes) -> Bytes

// Gzip members decoded; err on bad header, CRC, empty input or output over 268435456 bytes.
// @since 0.20.0 or earlier
zlib.gunzip(b: Bytes) -> Result[Bytes, String]

// Like gunzip, output bound max_len bytes (all members); max_len <= 0 aborts.
// @since unreleased
zlib.gunzip_max(b: Bytes, max_len: Int) -> Result[Bytes, String]
```



## API 说明译文（按上述签名顺序）

- `effect zlib.compress(b: Bytes) -> Bytes` — b 的 zlib 封装 DEFLATE；字节因目标而异。
- `effect zlib.compress_level(b: Bytes, level: Int) -> Bytes` — 类似 compress；level 限制到 0..9，wasm 不使用。
- `zlib.decompress(b: Bytes) -> Result[Bytes, String]` — 解压的 zlib stream；输入损坏、其后有额外字节、输出超过 268435456 字节均为 err。
- `zlib.decompress_max(b: Bytes, max_len: Int) -> Result[Bytes, String]` — 类似 decompress，输出上限 max_len 字节；max_len <= 0 则 abort。
- `effect zlib.deflate(b: Bytes) -> Bytes` — 无 header 的 raw DEFLATE；字节因目标而异。
- `effect zlib.deflate_level(b: Bytes, level: Int) -> Bytes` — 类似 deflate；level 限制到 0..9，wasm 不使用。
- `zlib.inflate(b: Bytes) -> Result[Bytes, String]` — decode raw DEFLATE；stream 损坏、其后有额外字节、输出超过 268435456 字节均为 err。
- `zlib.inflate_max(b: Bytes, max_len: Int) -> Result[Bytes, String]` — 类似 inflate，输出上限 max_len 字节；max_len <= 0 则 abort。
- `effect zlib.gzip(b: Bytes) -> Bytes` — b 的 RFC 1952 gzip；字节因目标而异。
- `zlib.gunzip(b: Bytes) -> Result[Bytes, String]` — decode gzip member；header 错误、CRC 错误、空输入或输出超过 268435456 字节均为 err。
- `zlib.gunzip_max(b: Bytes, max_len: Int) -> Result[Bytes, String]` — 类似 gunzip，输出上限 max_len 字节（所有 member 合计）；max_len <= 0 则 abort。
