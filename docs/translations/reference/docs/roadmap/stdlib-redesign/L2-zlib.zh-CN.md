原文描述元数据：Stdlib 重设计 L2/L3——zlib：校验和移至 hash，三个解码器改为纯 Result 函数，编码器等待统一的逐字节一致 deflate。

<a id="l2l3-zlib"></a>

# L2/L3：zlib

[![English](https://img.shields.io/badge/English-d0d7de?style=flat-square)](L2-zlib.md) [![日本語](https://img.shields.io/badge/%E6%97%A5%E6%9C%AC%E8%AA%9E-d0d7de?style=flat-square)](L2-zlib.ja.md) [![简体中文](https://img.shields.io/badge/%E7%AE%80%E4%BD%93%E4%B8%AD%E6%96%87-24292f?style=flat-square)](L2-zlib.zh-CN.md)

> 固定原文提交的非规范阅读快照；以原文为准，不自动同步。未独立执行、验证实现、检查证明或测量。历史命令与设计指令是资料，不是已执行的指令。
> [Original source / 原文](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/stdlib-redesign/L2-zlib.md)

> 原文注记：已采纳方案是原文时点的设计决策，不是独立验证的实现状态。压缩仍暂缓且保留 effect，等待所列性能/压缩率测量；往返一致不等于压缩字节一致。提议的输出上限错误不同于非正上限导致的 abort。L3 当前签名显示 Bytes，而正文称 Result 且返回类型不变，两者保留不统一。使用数、语言比较及测量大小是历史原文主张，未独立复核外部文档或压缩测量。

> 最后更新：2026-10-21
> 前提：[L0](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/stdlib-redesign/L0-conventions.md)、[L1](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/stdlib-redesign/L1-categories.md)（R-L1-05）；相关：[L2-hash](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/stdlib-redesign/L2-hash.md)
> 来源：Rust —（标准库没有；https://doc.rust-lang.org/std/ ）；Go https://pkg.go.dev/compress/zlib / https://pkg.go.dev/compress/flate / https://pkg.go.dev/compress/gzip ；Python https://docs.python.org/3/library/zlib.html / https://docs.python.org/3/library/gzip.html ；TS/JS https://developer.mozilla.org/en-US/docs/Web/API/CompressionStream / https://nodejs.org/api/zlib.html ；Gleam —（标准库没有）；Elixir https://www.erlang.org/doc/apps/erts/zlib.html ；OCaml —（标准库没有）；Kotlin https://docs.oracle.com/en/java/javase/21/docs/api/java.base/java/util/zip/package-summary.html ；Swift https://developer.apple.com/documentation/compression
> 实测：`almide 0.68.0 (dev, 437b8f127)`，native 与 `--target wasm`。探测位于 scratchpad 的 `probe-hash/`。

<a id="l2-役割の比較"></a>

## L2：职责比较

| 职责 | Almide | Rust | Go | Python | TS/JS | Gleam | Elixir | OCaml | Kotlin | Swift |
|---|---|---|---|---|---|---|---|---|---|---|
| zlib 格式（RFC 1950）的压缩/解压 | `compress` / `decompress` | — | `zlib.NewWriter` / `NewReader` | `zlib.compress` / `decompress` | `CompressionStream("deflate")`、Node `deflateSync` / `inflateSync` | — | `:zlib.compress` / `uncompress` | — | `Deflater` / `Inflater` (JVM) | Compression (Apple) |
| 原始 DEFLATE（RFC 1951） | `deflate` / `inflate` | — | `flate.NewWriter` / `NewReader` | `compressobj(wbits=-15)` | `"deflate-raw"`、Node `deflateRawSync` / `inflateRawSync` | — | `:zlib.zip` / `unzip` | — | `Deflater(nowrap)` (JVM) | — |
| gzip（RFC 1952） | `gzip` / `gunzip` | — | `gzip.NewWriter` / `NewReader` | `gzip.compress` / `decompress` | `"gzip"`、Node `gzipSync` / `gunzipSync` | — | `:zlib.gzip` / `gunzip` | — | `GZIPOutputStream` (JVM) | — |
| 指定压缩强度 | `compress_level`、`deflate_level` | — | `NewWriterLevel` | `compress(data, level)` | Node `{ level }` | — | `:zlib.deflateInit(z, level)` | — | `Deflater(level)` | — |
| CRC-32 / Adler-32 | `crc32`、`adler32` | — | `hash/crc32` / `hash/adler32` | `zlib.crc32` / `adler32` | — | — | `:erlang.crc32` | — | `java.util.zip.CRC32` | — |

标准提供 DEFLATE 的有 3 种语言（● 3、◐ 3），未达到 R-L1 的“添加”标准，但因它是既有 module，L1 予以保留（[L1-categories.md](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/stdlib-redesign/L1-categories.md) 第 3 章）。

<a id="完全性の規則"></a>

### 完整性规则

1. 对 3 种格式各放一对压缩/解压函数（`compress`/`decompress`、`deflate`/`inflate`、`gzip`/`gunzip`）。拼写接近 Erlang（`compress`、`gzip`、`gunzip`）和 Node（`deflate`、`inflate`、`gzip`、`gunzip`），没有多数派，保持不变。
2. 指定强度方面，当前没有 `gzip_level` 是缺口。但在 R-zlib-03 决定前不添加（因为 wasm 未使用强度）。
3. 校验和放在 `hash`（R-L1-05）。

<a id="l2-の判定"></a>

### L2 判定

- 位置：`crc32` / `adler32` → R-zlib-01（移至 `hash`）。
- 没有 effect 理由：三个解压函数在两种 target 上结果逐字节相同（C-331）。标为 `effect` 的唯一理由是“native 调用 host 的 zlib 库”，不属于 ADR-0022 与 R-L0-13 的“effect = 读取环境”。目前不能从纯函数调用（实测：E006）→ R-zlib-02。
- 结果因 target 不同：压缩输出在 native 与 wasm 上不同（实测：对 64 字节 `a` 用强度 9 `compress`，native 为 22 字节、wasm 为 75 字节。wasm 不使用强度，写 stored block）。相对于 R-L0-15 的“两侧行为相同”，当前只承诺往返 → R-zlib-03 暂缓。
- 需要 `!`：返回类型是 `Result[Bytes, String]`（`stdlib/zlib.almd:8`）。压缩不会失败，但调用方要写 `!`。

<a id="案"></a>

## 方案

<a id="r-zlib-01-crc32-と-adler32-を-hash-に移す---採用"></a>

### R-zlib-01 将 `crc32` 与 `adler32` 移至 `hash`——○ 采纳

- 内容：移除 `zlib.crc32` 与 `zlib.adler32`，归并至 `hash.crc32` / `hash.adler32`（[L2-hash](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/stdlib-redesign/L2-hash.md) 的 R-hash-02）。
- 依据的 L0 规则：R-L1-05、R-L0-02、R-L0-14。
- (b) 纯计算，值不变（实测：`"123456789"` 的 CRC-32 = 3421780262 = `0xCBF43926`，native 与 wasm 相同）。
- (d) 移除 2 个（hash 侧添加 2 个）。为 `zlib.crc32` 添加 `@deprecated(since=15, use="hash.crc32")`，为 `zlib.adler32` 添加 `@deprecated(since=15, use="hash.adler32")`，在 `removal_epoch = 16` 移除。`almide fix` 一对一重写并添加 `import hash`。使用数（2 个函数）：spec 15 / tests 5 / docs 17 / stdlib 0。
- 关卡提案：R-hash-03 的族关卡。
- issue：#3914

<a id="r-zlib-02-展開の-3-関数から-effect-を外す---採用"></a>

### R-zlib-02 从 3 个解压函数移除 `effect`——○ 采纳

- 内容：将 `decompress`、`inflate`、`gunzip` 从 `effect fn ... -> Result[Bytes, String]` 改成 `fn ... -> Result[Bytes, String]`，名称和返回类型不变。
- 依据的 L0 规则：R-L0-13 / ADR-0022（effect 是读取环境的函数，调用库不等于读取环境）、R-L0-15。
- (a) 9 种语言中，没有将解压视作副作用的语言。
- (b) 解压在 native 与 wasm 上逐字节相同（C-331）。即便 native 继续调用 host zlib，确定性也不改变。
- (c) 目前在读取数据的纯函数中无法解压 gzip，必须把整个函数改为 `effect`。
- (d) 语义变更（只扩展可调用的位置）。返回类型仍是同一 `Result`，因此 `effect fn` 中的 `zlib.decompress(d)!` 原样通过。使用数：`decompress` 13/21/5/8、`inflate` 10/16/5/3、`gunzip` 11/36/5/3（spec/tests/docs/stdlib）。
- 关卡提案：向 `spec/stdlib/zlib_test.almd` 添加从纯 `fn` 调用 3 个函数的测试。
- issue：#3956

<a id="r-zlib-03-圧縮の出力を両-target-で同じにしeffect-を外す--保留"></a>

### R-zlib-03 让压缩输出在两个 target 上相同，并移除 `effect`——暂缓

- 内容：让 5 个压缩函数（`compress`、`compress_level`、`deflate`、`deflate_level`、`gzip`）在两种 target 上都运行单一 self-host DEFLATE 编码器（固定/动态霍夫曼），wasm 也使用强度，输出逐字节相同。在此基础上移除 `effect`。
- 暂缓理由：native 从 host zlib 改成 self-host 编码器，预计速度和压缩率会下降，尚未测量下降多少。暂缓不是因为缺少 L0 规则，而是等待测量。
- 决定判定的测量：在 `research/benchmark/perf/` 放置压缩测量（相同输入组上，host zlib 与 self-host 编码器的 native 实际耗时比，以及强度 1、6、9 的压缩率）。速度比若在 `scripts/check-perf-ratio.sh` 容许范围内，压缩率差也在测量 issue 所定上限内，则 ○；否则 ×（维持当前契约）。此前保持当前契约（C-331）：“往返相同，不承诺压缩后的字节序列”，也保留 `effect`（将结果因 target 而异的函数改成纯函数，会破坏纯函数的确定性承诺）。
- 依据的 L0 规则：R-L0-15、ADR-0016（数值确定性）。
- issue：#3892

<a id="r-zlib-04-展開の出力に既定の上限を置き上限を引数にとる関数を足す---採用"></a>

### R-zlib-04 为解压输出设默认上限，并添加以上限为参数的函数——○ 采纳

- 当前：`decompress` / `inflate` / `gunzip` 的输出无上限，几 KB 的压缩炸弹会持续解压直到耗尽内存（`runtime/rs/src/zlib.rs:98-112`）。
- 方案：
  - 三个函数在输出超过 256 MiB 时返回 `err("zlib: output exceeds 268435456 bytes")`（在那里停止解压）。
  - 添加可更改上限的函数：`fn decompress_max(data: Bytes, max_len: Int) -> Result[Bytes, String]`、`inflate_max`、`gunzip_max`。`max_len <= 0` 为 abort（与 R-L0-33 的“构造数值”相同，是调用方违反契约）。
- 依据的 L0 规则：R-L0-47 的 2、3，R-L0-06。
- (a) Python `zlib.decompressobj().decompress(data, max_length)`、Node `zlib.inflateSync(buf, { maxOutputLength })`，Go 则组合 `io.LimitReader`。以上限为参数的形式有 Python、Node 两种，无名称多数派。Almide 用 `_max` 后缀表示有上限的版本。
- (b) 令 native 与 wasm 的上限判定和 err 文本相同。
- (d) 语义变更（超过 256 MiB 的解压变为 err）及添加 3 个函数。合法的超过 256 MiB 的解压可用 `_max`。纳入 epoch 15 的 `breaks`。
- issue：#4020

<a id="l3-関数表"></a>

## L3：函数表

| 函数 | 签名（当前，mut 显示） | 其他语言主要拼写 | 判定 | 方案 |
|---|---|---|---|---|
| `compress` | `effect zlib.compress(data: Bytes) -> Bytes` | Python `zlib.compress` / Erlang `:zlib.compress` / Node `deflateSync` | 保持 | R-zlib-03 |
| `compress_level` | `effect zlib.compress_level(data: Bytes, level: Int) -> Bytes` | Python `compress(data, level)` / Go `NewWriterLevel` | 保持 | R-zlib-03 |
| `decompress` | `effect zlib.decompress(data: Bytes) -> Bytes` | Python `zlib.decompress` / Erlang `:zlib.uncompress` / Node `inflateSync` | 语义变更（移除 effect，改为 `fn … -> Result[Bytes, String]`；输出超过 256 MiB 时 err） | R-zlib-02、R-zlib-04 |
| `deflate` | `effect zlib.deflate(data: Bytes) -> Bytes` | Node `deflateRawSync` / Erlang `:zlib.zip` / Go `flate.NewWriter` | 保持 | R-zlib-03 |
| `deflate_level` | `effect zlib.deflate_level(data: Bytes, level: Int) -> Bytes` | Go `flate.NewWriter(w, level)` | 保持 | R-zlib-03 |
| `inflate` | `effect zlib.inflate(data: Bytes) -> Bytes` | Node `inflateRawSync` / Erlang `:zlib.unzip` / Go `flate.NewReader` | 语义变更（移除 effect，改为 `fn … -> Result[Bytes, String]`；输出超过 256 MiB 时 err） | R-zlib-02、R-zlib-04 |
| `gzip` | `effect zlib.gzip(data: Bytes) -> Bytes` | Python `gzip.compress` / Node `gzipSync` / Erlang `:zlib.gzip` | 保持 | R-zlib-03 |
| `gunzip` | `effect zlib.gunzip(data: Bytes) -> Bytes` | Python `gzip.decompress` / Node `gunzipSync` / Erlang `:zlib.gunzip` | 语义变更（移除 effect，改为 `fn … -> Result[Bytes, String]`；输出超过 256 MiB 时 err） | R-zlib-02、R-zlib-04 |
| `crc32` | `zlib.crc32(data: Bytes) -> Int` | Go `crc32.ChecksumIEEE` / Python `zlib.crc32` / Erlang `crc32` | 移除 → `hash.crc32` | R-zlib-01 |
| `adler32` | `zlib.adler32(data: Bytes) -> Int` | Go `adler32.Checksum` / Python `zlib.adler32` | 移除 → `hash.adler32` | R-zlib-01 |
| `decompress_max` | （添加）`fn zlib.decompress_max(data: Bytes, max_len: Int) -> Result[Bytes, String]` | Python `max_length` / Node `maxOutputLength` | 添加 | R-zlib-04 |
| `inflate_max` | （添加）`fn zlib.inflate_max(data: Bytes, max_len: Int) -> Result[Bytes, String]` | Python `max_length` / Node `maxOutputLength` | 添加 | R-zlib-04 |
| `gunzip_max` | （添加）`fn zlib.gunzip_max(data: Bytes, max_len: Int) -> Result[Bytes, String]` | Python `max_length` / Node `maxOutputLength` | 添加 | R-zlib-04 |
