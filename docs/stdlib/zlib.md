# zlib

DEFLATE compression in its three container formats. `import zlib`, `effect fn`.

Every function is an `effect fn` returning `Bytes`, so calls sit in an
`effect fn` and propagate failure with `?`. Compression itself is
deterministic, but the calls are effectful because they go through the host
zlib library rather than a self-hosted implementation.

**Both targets.** The wasm legs carry a self-hosted zlib (#1700, C-331 —
`stdlib/zlib_inflate.almd` and `stdlib/zlib_deflate.almd`): every entry point
round-trips byte-identically on native and wasm, and the checksums agree. The
one difference is the compressed size — the wasm legs encode stored blocks
(valid DEFLATE that does not shrink), so the ratio is not a promise; the round
trip is. `spec/stdlib/zlib_test.almd` runs on the wasm test lane (#2179).

Three container formats share one DEFLATE core, and they are NOT interchangeable
— decompress with the matching function:

| Format | Compress | Decompress |
|---|---|---|
| zlib (RFC 1950) | `compress` | `decompress` |
| raw DEFLATE (RFC 1951) | `deflate` | `inflate` |
| gzip (RFC 1952) | `gzip` | `gunzip` |

### `effect zlib.compress(data: Bytes) -> Bytes`

zlib container, default level.

```almd
effect fn pack(b: Bytes) -> Result[Bytes, String] = ok(zlib.compress(b))
```

### `effect zlib.compress_level(data: Bytes, level: Int) -> Bytes`

zlib container at an explicit level, 0 (store) to 9 (smallest).

### `effect zlib.decompress(data: Bytes) -> Bytes`

Inverse of `compress`.

### `effect zlib.deflate(data: Bytes) -> Bytes`

Raw DEFLATE — no header, no checksum. What you want when an outer format
already frames the stream.

### `effect zlib.deflate_level(data: Bytes, level: Int) -> Bytes`

Raw DEFLATE at an explicit level.

### `effect zlib.inflate(data: Bytes) -> Bytes`

Inverse of `deflate`.

### `effect zlib.gzip(data: Bytes) -> Bytes`

gzip container — the on-disk `.gz` format, and `Content-Encoding: gzip`.

### `effect zlib.gunzip(data: Bytes) -> Bytes`

Inverse of `gzip`.

<!-- BEGIN GENERATED SIGNATURE INDEX (make stdlib-docs) — do not edit by hand -->

## Signature index (13 functions)

```
// Zlib-framed DEFLATE of data; bytes vary by target.
// @since 0.20.0 or earlier
effect zlib.compress(data: Bytes) -> Bytes

// Like compress; level clamped to 0..9, unused on wasm.
// @since 0.20.0 or earlier
effect zlib.compress_level(data: Bytes, level: Int) -> Bytes

// Inflated zlib stream; err on corrupt input or output over 268435456 bytes.
// @since 0.20.0 or earlier
effect zlib.decompress(data: Bytes) -> Bytes

// Like decompress, output bound max_len bytes; max_len <= 0 aborts.
// @since unreleased
zlib.decompress_max(b: Bytes, max_len: Int) -> Result[Bytes, String]

// Raw DEFLATE, no header; bytes vary by target.
// @since 0.20.0 or earlier
effect zlib.deflate(data: Bytes) -> Bytes

// Like deflate; level clamped to 0..9, unused on wasm.
// @since 0.20.0 or earlier
effect zlib.deflate_level(data: Bytes, level: Int) -> Bytes

// Raw DEFLATE decoded; err on a corrupt stream or output over 268435456 bytes.
// @since 0.20.0 or earlier
effect zlib.inflate(data: Bytes) -> Bytes

// Like inflate, output bound max_len bytes; max_len <= 0 aborts.
// @since unreleased
zlib.inflate_max(b: Bytes, max_len: Int) -> Result[Bytes, String]

// RFC 1952 gzip of data; bytes vary by target.
// @since 0.20.0 or earlier
effect zlib.gzip(data: Bytes) -> Bytes

// Gzip members decoded; err on bad header, CRC, empty input or output over 268435456 bytes.
// @since 0.20.0 or earlier
effect zlib.gunzip(data: Bytes) -> Bytes

// Like gunzip, output bound max_len bytes (all members); max_len <= 0 aborts.
// @since unreleased
zlib.gunzip_max(b: Bytes, max_len: Int) -> Result[Bytes, String]

// CRC-32 (IEEE) as an unsigned Int; 0 for empty data.
// @since 0.62.0
zlib.crc32(data: Bytes) -> Int

// Adler-32 as an unsigned Int; 1 for empty data.
// @since 0.62.0
zlib.adler32(data: Bytes) -> Int
```

<!-- END GENERATED SIGNATURE INDEX -->
