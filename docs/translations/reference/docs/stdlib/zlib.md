# zlib

[![English](https://img.shields.io/badge/English-24292f?style=flat-square)](zlib.md) [![日本語](https://img.shields.io/badge/%E6%97%A5%E6%9C%AC%E8%AA%9E-d0d7de?style=flat-square)](zlib.ja.md) [![简体中文](https://img.shields.io/badge/%E7%AE%80%E4%BD%93%E4%B8%AD%E6%96%87-d0d7de?style=flat-square)](zlib.zh-CN.md)

> This is a non-normative reader copy of the [pinned original](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/stdlib/zlib.md), not a new authority, generator output or automatically synchronized document. Original sources and machine consumers are unchanged. Historical claims and examples have not been independently revalidated here.


DEFLATE compression in its three container formats. `import zlib`.

The decoders (`decompress`, `inflate`, `gunzip` and their `_max` forms) are
plain fns returning `Result[Bytes, String]`: decoding is a pure transform whose
output is byte-identical on both targets, so a pure fn may call them (R-zlib-02).
The compressors stay `effect fn`s: native goes through the host zlib library,
whose compressed bytes differ from the wasm legs', so their output is not a
pure function of the input.

**Both targets.** The wasm legs carry a self-hosted zlib (#1700, C-331 —
`stdlib/zlib_inflate.almd` and `stdlib/zlib_deflate.almd`): the matching compressor/decoder pairs
recover the original payload bytes on native and wasm. Compressed bytes and
sizes can differ — the wasm legs encode stored blocks
(valid DEFLATE that does not shrink), so the ratio is not a promise; the round
trip is. `spec/stdlib/zlib_test.almd` runs on the wasm test lane (#2179).

Three container formats share one DEFLATE core, and they are NOT interchangeable
— decompress with the matching function:

| Format | Compress | Decompress |
|---|---|---|
| zlib (RFC 1950) | `compress` | `decompress` |
| raw DEFLATE (RFC 1951) | `deflate` | `inflate` |
| gzip (RFC 1952) | `gzip` | `gunzip` |

### `effect zlib.compress(b: Bytes) -> Bytes`

zlib container, default level.

```almd check
import zlib

effect fn pack(b: Bytes) -> Bytes = zlib.compress(b)!
```

### `effect zlib.compress_level(b: Bytes, level: Int) -> Bytes`

zlib container at an explicit level, 0 (store) to 9 (smallest).

### `zlib.decompress(b: Bytes) -> Result[Bytes, String]`

Inverse of `compress`.

### `effect zlib.deflate(b: Bytes) -> Bytes`

Raw DEFLATE — no header, no checksum. What you want when an outer format
already frames the stream.

### `effect zlib.deflate_level(b: Bytes, level: Int) -> Bytes`

Raw DEFLATE at an explicit level.

### `zlib.inflate(b: Bytes) -> Result[Bytes, String]`

Inverse of `deflate`.

### `effect zlib.gzip(b: Bytes) -> Bytes`

gzip container — the on-disk `.gz` format, and `Content-Encoding: gzip`.

### `zlib.gunzip(b: Bytes) -> Result[Bytes, String]`

Inverse of `gzip`.



## Signature index (11 functions)

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


