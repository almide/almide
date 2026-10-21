<!-- description: Stdlib redesign L2/L3 — hex: encode/encode_upper/decode stay as they are; the names mirror base64 -->
# L2/L3: hex

> Last updated: 2026-10-21
> 前提: [L0](L0-conventions.md)・[L1](L1-categories.md)・対になるページ [L2-base64](L2-base64.md)
> 出典: Rust —（標準に無い。https://doc.rust-lang.org/std/ ）・ Go https://pkg.go.dev/encoding/hex ・ Python https://docs.python.org/3/library/base64.html (F) ・ https://docs.python.org/3/library/stdtypes.html#bytes.hex ・ TS/JS https://developer.mozilla.org/en-US/docs/Web/JavaScript/Reference/Global_Objects/Uint8Array/toHex ・ Gleam https://gleam-stdlib.hexdocs.pm/gleam/bit_array.html (F) ・ Elixir https://elixir.hexdocs.pm/Base.html (F) ・ OCaml https://ocaml.org/manual/latest/api/Digest.html ・ Kotlin https://kotlinlang.org/api/core/kotlin-stdlib/kotlin.text/-hex-format/ ・ Swift —（標準に無い）
> 実測: `almide 0.68.0 (dev, 437b8f127)`、native と `--target wasm`。プローブは scratchpad の `probe-url/p1.almd`。

## L2: 役割の比較

| 役割 | Almide | Rust | Go | Python | TS/JS | Gleam | Elixir | OCaml | Kotlin | Swift |
|---|---|---|---|---|---|---|---|---|---|---|
| 小文字で符号化 | `encode` | — | `EncodeToString` | `b.hex()` | `toHex()` | — | `encode16(case: :lower)` (F) | `Digest.to_hex` | `toHexString()` | — |
| 大文字で符号化 | `encode_upper` | — | — | `b16encode` (F) | — | `base16_encode` (F) | `encode16`（既定が大文字） | — | `HexFormat { upperCase }` | — |
| 復号 | `decode`（大小どちらも） | — | `DecodeString` | `bytes.fromhex`、`b16decode`（既定は大文字だけ）(F) | `fromHex()` | `base16_decode` (F) | `decode16(case: :mixed)` (F) | `Digest.of_hex` | `hexToByteArray()` | — |
| 整数 ⇄ 16 進文字列 | `int.to_hex`・`int.from_hex`（int の側） | `format!("{:x}")` | `strconv.FormatInt(n, 16)` | `hex(n)` | `n.toString(16)` | `int.to_base16` | `Integer.to_string(n, 16)` | `Printf "%x"` | `toString(16)` | `String(n, radix: 16)` |

### 完全性の規則

1. 動詞は base64 と同じ `encode` / `decode`、変種は `encode_<変種>`（[L2-base64](L2-base64.md) の規則 1・2）。
2. 大文字・小文字は**書き方の違い**なので、`decode` が両方を受け取り、`decode_upper` は持ちません（実測: `hex.decode("FfA0")` → `[255, 160]`）。
3. 主語が `Int` の 16 進変換は `int` に置きます（R-L0-02）。`hex` は `Bytes` ⇄ `String` だけです。

### L2 の判定

- 抜けている役割・重複・穴はありません。`hex.encode`（Bytes）と `int.to_hex`（Int）は主語の型が違うので、R-L0-01 の重複に当たりません。
- `0x` の前置きは err です（実測: `hex.decode("0x01")` は `invalid hex char at 1`）。Go・JS・Elixir と同じです。据え置きます。

## 案

### R-hex-01 3 関数を据え置く — ○ 採用（変更なし）

- 内容: `encode`、`encode_upper`、`decode` を据え置きます。
- 根拠の L0 規則: R-L0-00（多数派の綴りが無く、Almide の中で衝突していない）、R-L1-23。
- (a) 9 言語の綴りは `EncodeToString`、`hex()`、`toHex`、`encode16`、`base16_encode` に割れています。
- (b) 純粋な計算です。両 target で同じ結果です（実測）。
- (c) base64 と同じ動詞なので、片方を知っていればもう片方を推測で書けます。
- issue: なし

### R-hex-02 `decode_upper` を足す — × 不採用

- 理由: 大文字・小文字を区別して拒む復号を標準の既定にしているのは Python の `b16decode` だけです(F)。規則 2 のとおり、書き方の違いは `decode` が受け取ります。
- issue: なし

## L3: 関数表

| 関数 | シグネチャ(現行, mut 表示) | 他言語の主な綴り | 判定 | 案 |
|---|---|---|---|---|
| `encode` | `hex.encode(b: Bytes) -> String` | Go `EncodeToString` / Python `b.hex()` / JS `toHex` / Elixir `encode16(case: :lower)` (F) / Kotlin `toHexString` | 据え置き | R-hex-01 |
| `encode_upper` | `hex.encode_upper(b: Bytes) -> String` | Python `b16encode` (F) / Gleam `base16_encode` (F) / Elixir `encode16` (F) | 据え置き | R-hex-01 |
| `decode` | `hex.decode(s: String) -> Result[Bytes, String]` | Go `DecodeString` / Python `bytes.fromhex` / JS `fromHex` / Gleam `base16_decode` (F) / Elixir `decode16` (F) | 据え置き | R-hex-01 |
