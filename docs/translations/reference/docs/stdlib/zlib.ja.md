# zlib

[![English](https://img.shields.io/badge/English-d0d7de?style=flat-square)](zlib.md) [![日本語](https://img.shields.io/badge/%E6%97%A5%E6%9C%AC%E8%AA%9E-24292f?style=flat-square)](zlib.ja.md) [![简体中文](https://img.shields.io/badge/%E7%AE%80%E4%BD%93%E4%B8%AD%E6%96%87-d0d7de?style=flat-square)](zlib.zh-CN.md)



> 原文：[zlib.md @ 35297a6](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/stdlib/zlib.md)。非規範的な読者向け固定スナップショットです。自動生成・自動同期されません。署名・コード例・API 索引のコメントは原文を保持し、説明の訳を末尾に併記しています。計測・動作・例をこの翻訳作業で独立に再検証していません。
>
> 原文上の注意: 圧縮率・圧縮バイトの一致は保証せず、原文は対応する圧縮/展開の往復を述べています。テスト経路の説明は英語原文の記録であり、この翻訳を同じゲートへ自動追加したものではありません。

三つのコンテナ形式による DEFLATE 圧縮。`import zlib`。

decoder（`decompress`、`inflate`、`gunzip` と各 `_max` 形）は、`Result[Bytes, String]` を返す通常の fn です。decode は両ターゲットで出力がバイト同一の純粋な変換なので、pure fn から呼べます（R-zlib-02）。compressor は `effect fn`s のままです。native はホスト zlib ライブラリを使い、圧縮バイト列が wasm 側と異なるため、出力は入力だけの純粋な関数ではありません。

**両ターゲット。** wasm 側はセルフホストの zlib（#1700、C-331 — `stdlib/zlib_inflate.almd` と `stdlib/zlib_deflate.almd`）を持ちます。対応する compressor/decoder 対は、native と wasm で元の payload バイト列を復元します。圧縮バイト列とサイズは異なる場合があります。wasm 側は stored block（縮小しないが有効な DEFLATE）を生成するため、約束するのは圧縮率ではなく往復です。`spec/stdlib/zlib_test.almd` は wasm test lane で動きます（#2179）。

三つのコンテナ形式は一つの DEFLATE core を共有しますが、相互交換はできません。対応する関数で展開してください。

| 形式 | 圧縮 | 展開 |
|---|---|---|
| zlib (RFC 1950) | `compress` | `decompress` |
| raw DEFLATE (RFC 1951) | `deflate` | `inflate` |
| gzip (RFC 1952) | `gzip` | `gunzip` |

### `effect zlib.compress(b: Bytes) -> Bytes`

zlib コンテナ、既定 level。

```almd check
import zlib

effect fn pack(b: Bytes) -> Bytes = zlib.compress(b)!
```

### `effect zlib.compress_level(b: Bytes, level: Int) -> Bytes`

zlib コンテナで level を明示します。0（store）から 9（最小）。

### `zlib.decompress(b: Bytes) -> Result[Bytes, String]`

`compress` の逆操作。

### `effect zlib.deflate(b: Bytes) -> Bytes`

raw DEFLATE。header も checksum もありません。外側の形式がすでにストリームの枠を提供する場合に使います。

### `effect zlib.deflate_level(b: Bytes, level: Int) -> Bytes`

level を明示する raw DEFLATE。

### `zlib.inflate(b: Bytes) -> Result[Bytes, String]`

`deflate` の逆操作。

### `effect zlib.gzip(b: Bytes) -> Bytes`

gzip コンテナ。ディスク上の `.gz` 形式、および `Content-Encoding: gzip` です。

### `zlib.gunzip(b: Bytes) -> Result[Bytes, String]`

`gzip` の逆操作。



<a id="signature-index-11-functions"></a>

## 固定スナップショットの API 索引 (11 関数)

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



## API 説明の訳（上記署名順）

- `effect zlib.compress(b: Bytes) -> Bytes` — b の zlib 枠付き DEFLATE。バイト列はターゲットにより異なります。
- `effect zlib.compress_level(b: Bytes, level: Int) -> Bytes` — compress と同様。level は 0..9 に制限し、wasm では使いません。
- `zlib.decompress(b: Bytes) -> Result[Bytes, String]` — 展開した zlib stream。入力破損、末尾の追加バイト、268435456 バイトを超える出力は err。
- `zlib.decompress_max(b: Bytes, max_len: Int) -> Result[Bytes, String]` — decompress と同様、出力上限は max_len バイト。max_len <= 0 なら abort。
- `effect zlib.deflate(b: Bytes) -> Bytes` — header なしの raw DEFLATE。バイト列はターゲットにより異なります。
- `effect zlib.deflate_level(b: Bytes, level: Int) -> Bytes` — deflate と同様。level は 0..9 に制限し、wasm では使いません。
- `zlib.inflate(b: Bytes) -> Result[Bytes, String]` — raw DEFLATE の decode。stream 破損、末尾の追加バイト、268435456 バイトを超える出力は err。
- `zlib.inflate_max(b: Bytes, max_len: Int) -> Result[Bytes, String]` — inflate と同様、出力上限は max_len バイト。max_len <= 0 なら abort。
- `effect zlib.gzip(b: Bytes) -> Bytes` — b の RFC 1952 gzip。バイト列はターゲットにより異なります。
- `zlib.gunzip(b: Bytes) -> Result[Bytes, String]` — gzip member を decode。不正 header、CRC、空入力、268435456 バイトを超える出力は err。
- `zlib.gunzip_max(b: Bytes, max_len: Int) -> Result[Bytes, String]` — gunzip と同様、出力上限は max_len バイト（全 member 合計）。max_len <= 0 なら abort。
