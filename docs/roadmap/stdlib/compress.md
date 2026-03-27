<!-- description: Compression/decompression module (gzip, zstd, deflate) -->
# stdlib: compress [Tier 3]

圧縮・展開。ファイル操作やネットワーク通信で必要。

## 他言語比較

| アルゴリズム | Go | Python | Rust | Deno |
|------------|-----|--------|------|------|
| gzip | `compress/gzip` | `gzip` | `flate2` crate | `@std/compress` (Wasm) |
| zstd | `github.com/klauspost/compress/zstd` | `zstandard` | `zstd` crate | external |
| deflate | `compress/flate` | `zlib` | `flate2` crate | built-in `CompressionStream` |
| brotli | `github.com/andybalholm/brotli` | `brotli` | `brotli` crate | built-in |
| tar | `archive/tar` | `tarfile` | `tar` crate | `@std/tar` |
| zip | `archive/zip` | `zipfile` | `zip` crate | `@nicco/zip` |

## 追加候補 (~6 関数)

### P0 (gzip)
- `compress.gzip(data) -> List[Int]` — gzip 圧縮
- `compress.gunzip(data) -> Result[List[Int], String]` — gzip 展開

### P1 (zstd)
- `compress.zstd_compress(data) -> List[Int]`
- `compress.zstd_decompress(data) -> Result[List[Int], String]`

### P2 (アーカイブ)
- `compress.tar_create(files) -> List[Int]` — tar アーカイブ作成
- `compress.tar_extract(data) -> Result[List[{ name: String, data: List[Int] }], String]`

## 実装戦略

@extern。Rust: `flate2` (gzip), `zstd` (zstd)。TS: `CompressionStream` API / Wasm。
self-host は非現実的（アルゴリズムが複雑）。
