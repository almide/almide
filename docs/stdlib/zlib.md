# zlib

DEFLATE compression in its three container formats. `import zlib`.

The decoders (`decompress`, `inflate`, `gunzip` and their `_max` forms) are
plain fns returning `Result[Bytes, String]`: decoding is a pure transform whose
output is byte-identical on both targets, so a pure fn may call them (R-zlib-02).
The compressors stay `effect fn`s: native goes through the host zlib library,
whose compressed bytes differ from the wasm legs', so their output is not a
pure function of the input.

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

### `effect zlib.compress(b: Bytes) -> Bytes`

zlib container, default level.

```almd
effect fn pack(b: Bytes) -> Result[Bytes, String] = ok(zlib.compress(b))
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

<!-- BEGIN GENERATED SIGNATURE INDEX (make stdlib-docs) — do not edit by hand -->

## Signature index (11 functions)

```
// Zlib-framed DEFLATE of b; bytes vary by target.
// @since 0.20.0 or earlier
effect zlib.compress(b: Bytes) -> Bytes

// Like compress; level clamped to 0..9, unused on wasm.
// @since 0.20.0 or earlier
effect zlib.compress_level(b: Bytes, level: Int) -> Bytes

// Inflated zlib stream; err on corrupt input or output over 268435456 bytes.
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

// Raw DEFLATE decoded; err on a corrupt stream or output over 268435456 bytes.
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

<!-- END GENERATED SIGNATURE INDEX -->
