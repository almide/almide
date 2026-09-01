# zlib

DEFLATE compression in its three container formats. `import zlib`, `effect fn`.

Every function is an `effect fn` returning `Bytes`, so calls sit in an
`effect fn` and propagate failure with `?`. Compression itself is
deterministic, but the calls are effectful because they go through the host
zlib library rather than a self-hosted implementation.

**Native only.** There is no wasm floor for zlib, so a program using it builds
and runs natively but walls on `--target wasm`
(`spec/stdlib/zlib_test.almd` is marked `// wasm:skip`).

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

## Signature index (10 functions)

```
effect zlib.compress(data: Bytes) -> Bytes
effect zlib.compress_level(data: Bytes, level: Int) -> Bytes
effect zlib.decompress(data: Bytes) -> Bytes
effect zlib.deflate(data: Bytes) -> Bytes
effect zlib.deflate_level(data: Bytes, level: Int) -> Bytes
effect zlib.inflate(data: Bytes) -> Bytes
effect zlib.gzip(data: Bytes) -> Bytes
effect zlib.gunzip(data: Bytes) -> Bytes
zlib.crc32(data: Bytes) -> Int
zlib.adler32(data: Bytes) -> Int
```

<!-- END GENERATED SIGNATURE INDEX -->
