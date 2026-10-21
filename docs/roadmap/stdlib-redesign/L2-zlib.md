<!-- description: Stdlib redesign L2/L3 — zlib: checksums move to hash, the three decoders become pure Result fns, and the encoders wait for one byte-identical deflate -->
# L2/L3: zlib

> Last updated: 2026-10-21
> 前提: [L0](L0-conventions.md)・[L1](L1-categories.md)（R-L1-05）・関連 [L2-hash](L2-hash.md)
> 出典: Rust —（標準に無い。https://doc.rust-lang.org/std/ ）・ Go https://pkg.go.dev/compress/zlib ・ https://pkg.go.dev/compress/flate ・ https://pkg.go.dev/compress/gzip ・ Python https://docs.python.org/3/library/zlib.html ・ https://docs.python.org/3/library/gzip.html ・ TS/JS https://developer.mozilla.org/en-US/docs/Web/API/CompressionStream ・ https://nodejs.org/api/zlib.html ・ Gleam —（標準に無い）・ Elixir https://www.erlang.org/doc/apps/erts/zlib.html ・ OCaml —（標準に無い）・ Kotlin https://docs.oracle.com/en/java/javase/21/docs/api/java.base/java/util/zip/package-summary.html ・ Swift https://developer.apple.com/documentation/compression
> 実測: `almide 0.68.0 (dev, 437b8f127)`、native と `--target wasm`。プローブは scratchpad の `probe-hash/`。

## L2: 役割の比較

| 役割 | Almide | Rust | Go | Python | TS/JS | Gleam | Elixir | OCaml | Kotlin | Swift |
|---|---|---|---|---|---|---|---|---|---|---|
| zlib 形式（RFC 1950）で圧縮・展開 | `compress` / `decompress` | — | `zlib.NewWriter` / `NewReader` | `zlib.compress` / `decompress` | `CompressionStream("deflate")`、Node `deflateSync` / `inflateSync` | — | `:zlib.compress` / `uncompress` | — | `Deflater` / `Inflater`(JVM) | Compression(Apple) |
| 生の DEFLATE（RFC 1951） | `deflate` / `inflate` | — | `flate.NewWriter` / `NewReader` | `compressobj(wbits=-15)` | `"deflate-raw"`、Node `deflateRawSync` / `inflateRawSync` | — | `:zlib.zip` / `unzip` | — | `Deflater(nowrap)`(JVM) | — |
| gzip（RFC 1952） | `gzip` / `gunzip` | — | `gzip.NewWriter` / `NewReader` | `gzip.compress` / `decompress` | `"gzip"`、Node `gzipSync` / `gunzipSync` | — | `:zlib.gzip` / `gunzip` | — | `GZIPOutputStream`(JVM) | — |
| 圧縮の強さを指定 | `compress_level`、`deflate_level` | — | `NewWriterLevel` | `compress(data, level)` | Node `{ level }` | — | `:zlib.deflateInit(z, level)` | — | `Deflater(level)` | — |
| CRC-32・Adler-32 | `crc32`、`adler32` | — | `hash/crc32`・`hash/adler32` | `zlib.crc32`・`adler32` | — | — | `:erlang.crc32` | — | `java.util.zip.CRC32` | — |

DEFLATE を標準に持つのは 3 言語（● 3・◐ 3）で、R-L1 の「足す」基準には届きませんが、既存のモジュールなので L1 は残しています（[L1-categories.md](L1-categories.md) の 3 章）。

### 完全性の規則

1. 3 つの形式それぞれに、圧縮と展開の対を 1 つずつ置きます（`compress`/`decompress`、`deflate`/`inflate`、`gzip`/`gunzip`）。綴りは Erlang（`compress`・`gzip`・`gunzip`）と Node（`deflate`・`inflate`・`gzip`・`gunzip`）が近く、多数派はありません。据え置きます。
2. 強さの指定は、`gzip_level` を持たない今の形が穴です。ただし R-zlib-03 が決まるまで足しません（wasm では強さが使われていないため）。
3. 検査和は `hash` に置きます（R-L1-05）。

### L2 の判定

- 置き場所: `crc32`・`adler32` → R-zlib-01（`hash` へ）。
- effect の理由がない: 展開の 3 関数は両 target でバイト単位まで同じ結果です（C-331）。`effect` にしている理由は「native が host の zlib ライブラリを呼ぶから」だけで、ADR-0022 と R-L0-13 の「effect = 環境を読む」に当たりません。いまは純粋な関数から呼べません（実測: E006）→ R-zlib-02。
- target で結果が違う: 圧縮の出力は native と wasm で違います（実測: 64 バイトの `a` を強さ 9 で `compress` すると native 22 バイト、wasm 75 バイト。wasm は強さを使わず、stored block で書く）。R-L0-15 の「両方で同じように動く」に対して、往復だけを約束している状態です → R-zlib-03 保留。
- `!` が要る: 戻り型は `Result[Bytes, String]` です（`stdlib/zlib.almd:8`）。圧縮は失敗しませんが、呼び出し側は `!` を書きます。

## 案

### R-zlib-01 `crc32` と `adler32` を `hash` に移す — ○ 採用

- 内容: `zlib.crc32` と `zlib.adler32` を撤去し、`hash.crc32`・`hash.adler32` に寄せます（[L2-hash](L2-hash.md) の R-hash-02）。
- 根拠の L0 規則: R-L1-05、R-L0-02、R-L0-14。
- (b) 純粋な計算です。値は変わりません（実測: `"123456789"` の CRC-32 = 3421780262 = `0xCBF43926`、native と wasm で同じ）。
- (d) 撤去 2（hash 側は追加 2）。`zlib.crc32` に `@deprecated(since=15, use="hash.crc32")`、`zlib.adler32` に `@deprecated(since=15, use="hash.adler32")` を付け、`removal_epoch = 16` で撤去します。`almide fix` は 1 対 1 で書き換え、`import hash` を足します。使用数（2 関数）: spec 15 / tests 5 / docs 17 / stdlib 0。
- ゲート案: R-hash-03 の族ゲート。
- issue: #3914

### R-zlib-02 展開の 3 関数から `effect` を外す — ○ 採用

- 内容: `decompress`、`inflate`、`gunzip` を `effect fn ... -> Result[Bytes, String]` から `fn ... -> Result[Bytes, String]` にします。名前と戻り型は変えません。
- 根拠の L0 規則: R-L0-13・ADR-0022（effect は環境を読む関数。ライブラリを呼ぶことは環境を読むことではない）、R-L0-15。
- (a) 9 言語で、展開を副作用として扱う言語はありません。
- (b) 展開は native と wasm でバイト単位まで同じです（C-331）。native が host の zlib を呼び続けても、決定性は変わりません。
- (c) いまは、データを読む純粋な関数の中で gzip を展開できず、関数全体を `effect` にすることになります。
- (d) 意味変更（呼べる場所が広がるだけ）。戻り型は同じ `Result` なので、`effect fn` の中の `zlib.decompress(d)!` はそのまま通ります。使用数: `decompress` 13/21/5/8、`inflate` 10/16/5/3、`gunzip` 11/36/5/3（spec/tests/docs/stdlib）。
- ゲート案: `spec/stdlib/zlib_test.almd` に、純粋な `fn` から 3 関数を呼ぶ test を足します。
- issue: #3956

### R-zlib-03 圧縮の出力を両 target で同じにし、`effect` を外す — 保留

- 内容: 圧縮の 5 関数（`compress`、`compress_level`、`deflate`、`deflate_level`、`gzip`）を、self-host の 1 本の DEFLATE 符号化器（固定・動的ハフマン）で両 target とも動かし、強さを wasm でも使い、出力をバイト単位で同じにします。そのうえで `effect` を外します。
- 保留の理由: native を host の zlib から self-host の符号化器に替えると、速さと圧縮率が落ちる見込みです。どれだけ落ちるかをまだ測っていません。L0 の規則が足りないための保留ではなく、計測を待つ保留です。
- 判定を決める計測: `research/benchmark/perf/` に圧縮の計測（同じ入力の組で、host の zlib と self-host の符号化器の native の実時間の比と、強さ 1・6・9 の圧縮率）を置きます。速さの比が `scripts/check-perf-ratio.sh` の許容の範囲に入り、圧縮率の差が計測の issue で決める上限の内に入れば ○、入らなければ ×（今の契約のまま）にします。それまでは「往復は同じ、圧縮後のバイト列は約束しない」という今の契約（C-331）のまま、`effect` も残します（target で結果が違う関数を純粋にすると、純粋な関数の決定性の約束が崩れるため）。
- 根拠の L0 規則: R-L0-15、ADR-0016（数値の決定性）。
- issue: #3892

## L3: 関数表

| 関数 | シグネチャ(現行, mut 表示) | 他言語の主な綴り | 判定 | 案 |
|---|---|---|---|---|
| `compress` | `effect zlib.compress(data: Bytes) -> Bytes` | Python `zlib.compress` / Erlang `:zlib.compress` / Node `deflateSync` | 据え置き | R-zlib-03 |
| `compress_level` | `effect zlib.compress_level(data: Bytes, level: Int) -> Bytes` | Python `compress(data, level)` / Go `NewWriterLevel` | 据え置き | R-zlib-03 |
| `decompress` | `effect zlib.decompress(data: Bytes) -> Bytes` | Python `zlib.decompress` / Erlang `:zlib.uncompress` / Node `inflateSync` | 意味変更（effect を外す） | R-zlib-02 |
| `deflate` | `effect zlib.deflate(data: Bytes) -> Bytes` | Node `deflateRawSync` / Erlang `:zlib.zip` / Go `flate.NewWriter` | 据え置き | R-zlib-03 |
| `deflate_level` | `effect zlib.deflate_level(data: Bytes, level: Int) -> Bytes` | Go `flate.NewWriter(w, level)` | 据え置き | R-zlib-03 |
| `inflate` | `effect zlib.inflate(data: Bytes) -> Bytes` | Node `inflateRawSync` / Erlang `:zlib.unzip` / Go `flate.NewReader` | 意味変更（effect を外す） | R-zlib-02 |
| `gzip` | `effect zlib.gzip(data: Bytes) -> Bytes` | Python `gzip.compress` / Node `gzipSync` / Erlang `:zlib.gzip` | 据え置き | R-zlib-03 |
| `gunzip` | `effect zlib.gunzip(data: Bytes) -> Bytes` | Python `gzip.decompress` / Node `gunzipSync` / Erlang `:zlib.gunzip` | 意味変更（effect を外す） | R-zlib-02 |
| `crc32` | `zlib.crc32(data: Bytes) -> Int` | Go `crc32.ChecksumIEEE` / Python `zlib.crc32` / Erlang `crc32` | 撤去 → `hash.crc32` | R-zlib-01 |
| `adler32` | `zlib.adler32(data: Bytes) -> Int` | Go `adler32.Checksum` / Python `zlib.adler32` | 撤去 → `hash.adler32` | R-zlib-01 |
