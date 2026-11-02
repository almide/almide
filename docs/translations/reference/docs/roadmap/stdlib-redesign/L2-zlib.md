Source description metadata: Stdlib redesign L2/L3 — zlib: checksums move to hash, the three decoders become pure Result fns, and the encoders wait for one byte-identical deflate.

<a id="l2l3-zlib"></a>

# L2/L3: zlib

[![English](https://img.shields.io/badge/English-24292f?style=flat-square)](L2-zlib.md) [![日本語](https://img.shields.io/badge/%E6%97%A5%E6%9C%AC%E8%AA%9E-d0d7de?style=flat-square)](L2-zlib.ja.md) [![简体中文](https://img.shields.io/badge/%E7%AE%80%E4%BD%93%E4%B8%AD%E6%96%87-d0d7de?style=flat-square)](L2-zlib.zh-CN.md)

> Non-normative reader snapshot pinned to the original commit; the original remains authoritative. No automatic synchronization or independent execution, implementation verification, proof checking or measurement. Historical commands and design instructions are documentation, not executed instructions.
> [Original source / 原文](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/stdlib-redesign/L2-zlib.md)

> Source caveat: adopted proposals are design decisions at the source date, not independently verified implementation status. Compression remains deferred and effectful pending the stated performance/ratio measurements; round-trip agreement is not compressed-byte equality. The proposed output-limit error is distinct from abort for a nonpositive limit. The L3 current-signature display says Bytes while prose says Result and unchanged return types; both are retained rather than normalized. Usage counts, language comparisons and measured sizes are historical source claims; no external documentation or compression measurements were independently revalidated.

> Last updated: 2026-10-21
> Prerequisites: [L0](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/stdlib-redesign/L0-conventions.md), [L1](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/stdlib-redesign/L1-categories.md) (R-L1-05); related: [L2-hash](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/stdlib-redesign/L2-hash.md)
> Sources: Rust — (not in the standard library; https://doc.rust-lang.org/std/ ); Go https://pkg.go.dev/compress/zlib / https://pkg.go.dev/compress/flate / https://pkg.go.dev/compress/gzip ; Python https://docs.python.org/3/library/zlib.html / https://docs.python.org/3/library/gzip.html ; TS/JS https://developer.mozilla.org/en-US/docs/Web/API/CompressionStream / https://nodejs.org/api/zlib.html ; Gleam — (not in the standard library); Elixir https://www.erlang.org/doc/apps/erts/zlib.html ; OCaml — (not in the standard library); Kotlin https://docs.oracle.com/en/java/javase/21/docs/api/java.base/java/util/zip/package-summary.html ; Swift https://developer.apple.com/documentation/compression
> Measurement: `almide 0.68.0 (dev, 437b8f127)`, native and `--target wasm`. Probes are in the scratchpad's `probe-hash/`.

<a id="l2-役割の比較"></a>

## L2: Comparison of roles

| Role | Almide | Rust | Go | Python | TS/JS | Gleam | Elixir | OCaml | Kotlin | Swift |
|---|---|---|---|---|---|---|---|---|---|---|
| Compress/decompress zlib format (RFC 1950) | `compress` / `decompress` | — | `zlib.NewWriter` / `NewReader` | `zlib.compress` / `decompress` | `CompressionStream("deflate")`, Node `deflateSync` / `inflateSync` | — | `:zlib.compress` / `uncompress` | — | `Deflater` / `Inflater` (JVM) | Compression (Apple) |
| Raw DEFLATE (RFC 1951) | `deflate` / `inflate` | — | `flate.NewWriter` / `NewReader` | `compressobj(wbits=-15)` | `"deflate-raw"`, Node `deflateRawSync` / `inflateRawSync` | — | `:zlib.zip` / `unzip` | — | `Deflater(nowrap)` (JVM) | — |
| gzip (RFC 1952) | `gzip` / `gunzip` | — | `gzip.NewWriter` / `NewReader` | `gzip.compress` / `decompress` | `"gzip"`, Node `gzipSync` / `gunzipSync` | — | `:zlib.gzip` / `gunzip` | — | `GZIPOutputStream` (JVM) | — |
| Specify compression strength | `compress_level`, `deflate_level` | — | `NewWriterLevel` | `compress(data, level)` | Node `{ level }` | — | `:zlib.deflateInit(z, level)` | — | `Deflater(level)` | — |
| CRC-32 / Adler-32 | `crc32`, `adler32` | — | `hash/crc32` / `hash/adler32` | `zlib.crc32` / `adler32` | — | — | `:erlang.crc32` | — | `java.util.zip.CRC32` | — |

3 languages provide DEFLATE as standard (● 3, ◐ 3), below R-L1's criterion for adding a module, but L1 retains it because it is an existing module (chapter 3 of [L1-categories.md](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/stdlib-redesign/L1-categories.md)).

<a id="完全性の規則"></a>

### Completeness rules

1. Provide one compression/decompression pair for each of the 3 formats (`compress`/`decompress`, `deflate`/`inflate`, `gzip`/`gunzip`). The spellings are close to Erlang (`compress`, `gzip`, `gunzip`) and Node (`deflate`, `inflate`, `gzip`, `gunzip`); there is no majority. Retain them.
2. Specifying strength has a gap in the current shape, which lacks `gzip_level`. Do not add it until R-zlib-03 is decided, however (wasm does not use strength).
3. Put checksums in `hash` (R-L1-05).

<a id="l2-の判定"></a>

### L2 decisions

- Placement: `crc32` / `adler32` → R-zlib-01 (to `hash`).
- No reason for an effect: the 3 decompression functions produce byte-identical results on both targets (C-331). The sole reason they are `effect` is “native calls the host zlib library,” which does not meet ADR-0022 and R-L0-13's “effect = reads the environment.” They cannot currently be called from pure functions (measured: E006) → R-zlib-02.
- Results differ by target: compressed output differs between native and wasm (measurement: `compress` on 64 bytes of `a` at strength 9 yields 22 bytes native and 75 bytes wasm. Wasm ignores strength and writes stored blocks). Against R-L0-15's “works the same on both,” only round-tripping is promised → R-zlib-03 on hold.
- `!` is required: the return type is `Result[Bytes, String]` (`stdlib/zlib.almd:8`). Compression does not fail, but callers write `!`.

<a id="案"></a>

## Proposals

<a id="r-zlib-01-crc32-と-adler32-を-hash-に移す---採用"></a>

### R-zlib-01 Move `crc32` and `adler32` to `hash` — ○ adopted

- Content: remove `zlib.crc32` and `zlib.adler32`, consolidating them into `hash.crc32` / `hash.adler32` (R-hash-02 in [L2-hash](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/stdlib-redesign/L2-hash.md)).
- Supporting L0 rules: R-L1-05, R-L0-02, R-L0-14.
- (b) Pure computation. Values do not change (measurement: CRC-32 of `"123456789"` = 3421780262 = `0xCBF43926`, identical on native and wasm).
- (d) Remove 2 (add 2 on the hash side). Add `@deprecated(since=15, use="hash.crc32")` to `zlib.crc32` and `@deprecated(since=15, use="hash.adler32")` to `zlib.adler32`, removing them at `removal_epoch = 16`. `almide fix` rewrites one-to-one and adds `import hash`. Usage counts (2 functions): spec 15 / tests 5 / docs 17 / stdlib 0.
- Proposed gate: R-hash-03's family gate.
- issue: #3914

<a id="r-zlib-02-展開の-3-関数から-effect-を外す---採用"></a>

### R-zlib-02 Remove `effect` from the 3 decompression functions — ○ adopted

- Content: change `decompress`, `inflate`, `gunzip` from `effect fn ... -> Result[Bytes, String]` to `fn ... -> Result[Bytes, String]`. Names and return types do not change.
- Supporting L0 rules: R-L0-13 / ADR-0022 (an effect is a function that reads the environment; calling a library is not reading the environment), R-L0-15.
- (a) None of the 9 languages treat decompression as a side effect.
- (b) Decompression is byte-identical on native and wasm (C-331). Determinism does not change even if native continues calling the host zlib.
- (c) Currently gzip cannot be decompressed inside a pure function that reads data; the entire function must become `effect`.
- (d) Semantic change (only broadens where calls are permitted). The return type is the same `Result`, so `zlib.decompress(d)!` inside an `effect fn` still passes unchanged. Usage counts: `decompress` 13/21/5/8, `inflate` 10/16/5/3, `gunzip` 11/36/5/3 (spec/tests/docs/stdlib).
- Proposed gate: add tests to `spec/stdlib/zlib_test.almd` calling all 3 functions from a pure `fn`.
- issue: #3956

<a id="r-zlib-03-圧縮の出力を両-target-で同じにしeffect-を外す--保留"></a>

### R-zlib-03 Make compressed output identical on both targets and remove `effect` — on hold

- Content: run the 5 compression functions (`compress`, `compress_level`, `deflate`, `deflate_level`, `gzip`) through a single self-hosted DEFLATE encoder (fixed/dynamic Huffman) on both targets, use strength on wasm too, and make output byte-identical. Then remove `effect`.
- Reason for hold: replacing native's host zlib with a self-hosted encoder is expected to reduce speed and compression ratio. How much has not yet been measured. The hold is awaiting measurement, not because L0 lacks a rule.
- Deciding measurement: place compression measurements in `research/benchmark/perf/` (using the same input set, the ratio of native wall-clock times for host zlib and the self-hosted encoder, and compression ratios at strengths 1, 6 and 9). Adopt (○) if the speed ratio is within the tolerance of `scripts/check-perf-ratio.sh` and compression-ratio differences stay within the upper bound to be set in the measurement issue; otherwise reject (×), keeping the current contract. Until then, retain the current contract (C-331): “round-tripping is the same; compressed byte sequences are not promised.” Also retain `effect` (making a function with target-dependent results pure would break the determinism promise of pure functions).
- Supporting L0 rules: R-L0-15, ADR-0016 (numeric determinism).
- issue: #3892

<a id="r-zlib-04-展開の出力に既定の上限を置き上限を引数にとる関数を足す---採用"></a>

### R-zlib-04 Set a default decompressed-output limit and add functions taking a limit argument — ○ adopted

- Now: `decompress` / `inflate` / `gunzip` have no output limit; a compression bomb of a few KB keeps expanding until memory is exhausted (`runtime/rs/src/zlib.rs:98-112`).
- Proposal:
  - The 3 functions return `err("zlib: output exceeds 268435456 bytes")` if output exceeds 256 MiB (stop decompression there).
  - Add functions to change the limit: `fn decompress_max(data: Bytes, max_len: Int) -> Result[Bytes, String]`, `inflate_max`, `gunzip_max`. `max_len <= 0` aborts (the same as R-L0-33's “construct a number”: a caller contract violation).
- Supporting L0 rules: R-L0-47 items 2 and 3, R-L0-06.
- (a) Python `zlib.decompressobj().decompress(data, max_length)`, Node `zlib.inflateSync(buf, { maxOutputLength })`; Go composes with `io.LimitReader`. Python and Node are the 2 forms taking a limit argument, with no majority name. Almide marks a limit-taking version with the `_max` suffix.
- (b) Make limit checks and err wording identical on native and wasm.
- (d) Semantic change (decompression beyond 256 MiB becomes err) and 3 additions. Legitimate decompression exceeding 256 MiB can use `_max`. Include this in epoch 15's `breaks`.
- issue: #4020

<a id="l3-関数表"></a>

## L3: Function table

| Function | Signature (current, mut display) | Main spellings in other languages | Decision | Proposal |
|---|---|---|---|---|
| `compress` | `effect zlib.compress(data: Bytes) -> Bytes` | Python `zlib.compress` / Erlang `:zlib.compress` / Node `deflateSync` | Retain | R-zlib-03 |
| `compress_level` | `effect zlib.compress_level(data: Bytes, level: Int) -> Bytes` | Python `compress(data, level)` / Go `NewWriterLevel` | Retain | R-zlib-03 |
| `decompress` | `effect zlib.decompress(data: Bytes) -> Bytes` | Python `zlib.decompress` / Erlang `:zlib.uncompress` / Node `inflateSync` | Semantic change (remove effect, becoming `fn … -> Result[Bytes, String]`; err if output exceeds 256 MiB) | R-zlib-02, R-zlib-04 |
| `deflate` | `effect zlib.deflate(data: Bytes) -> Bytes` | Node `deflateRawSync` / Erlang `:zlib.zip` / Go `flate.NewWriter` | Retain | R-zlib-03 |
| `deflate_level` | `effect zlib.deflate_level(data: Bytes, level: Int) -> Bytes` | Go `flate.NewWriter(w, level)` | Retain | R-zlib-03 |
| `inflate` | `effect zlib.inflate(data: Bytes) -> Bytes` | Node `inflateRawSync` / Erlang `:zlib.unzip` / Go `flate.NewReader` | Semantic change (remove effect, becoming `fn … -> Result[Bytes, String]`; err if output exceeds 256 MiB) | R-zlib-02, R-zlib-04 |
| `gzip` | `effect zlib.gzip(data: Bytes) -> Bytes` | Python `gzip.compress` / Node `gzipSync` / Erlang `:zlib.gzip` | Retain | R-zlib-03 |
| `gunzip` | `effect zlib.gunzip(data: Bytes) -> Bytes` | Python `gzip.decompress` / Node `gunzipSync` / Erlang `:zlib.gunzip` | Semantic change (remove effect, becoming `fn … -> Result[Bytes, String]`; err if output exceeds 256 MiB) | R-zlib-02, R-zlib-04 |
| `crc32` | `zlib.crc32(data: Bytes) -> Int` | Go `crc32.ChecksumIEEE` / Python `zlib.crc32` / Erlang `crc32` | Remove → `hash.crc32` | R-zlib-01 |
| `adler32` | `zlib.adler32(data: Bytes) -> Int` | Go `adler32.Checksum` / Python `zlib.adler32` | Remove → `hash.adler32` | R-zlib-01 |
| `decompress_max` | (Addition) `fn zlib.decompress_max(data: Bytes, max_len: Int) -> Result[Bytes, String]` | Python `max_length` / Node `maxOutputLength` | Add | R-zlib-04 |
| `inflate_max` | (Addition) `fn zlib.inflate_max(data: Bytes, max_len: Int) -> Result[Bytes, String]` | Python `max_length` / Node `maxOutputLength` | Add | R-zlib-04 |
| `gunzip_max` | (Addition) `fn zlib.gunzip_max(data: Bytes, max_len: Int) -> Result[Bytes, String]` | Python `max_length` / Node `maxOutputLength` | Add | R-zlib-04 |
