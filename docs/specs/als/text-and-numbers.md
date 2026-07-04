# ALS §T — Text and Number Semantics (normative)

> **Status**: normative. これらの節は実装から独立した**規範**であり、v0（native）と
> v1（MIR/wasm）の両実装がこの節に適合する義務を負う。適合の証拠は
> `spec/wasm_cross/` の対応 fixture（3点観測: stdout・stderr・exit code）。
> **oracle 循環の解消**（flight-evidence-gaps F1）: 本節の制定以前、これらの挙動の
> 「正しさ」は v0 実装そのものだった。以後、v0 も本節に対する一実装である。

## ALS-T1 `string.trim`

`string.trim(s)` は s の先頭・末尾から **Unicode `White_Space` プロパティを持つ
コードポイント**の最長連続列を除去する。規範は Unicode 標準の White_Space
（PropList.txt）であり、2026 年時点で次の 25 コードポイント:
U+0009–U+000D, U+0020, U+0085, U+00A0, U+1680, U+2000–U+200A, U+2028, U+2029,
U+202F, U+205F, U+3000。

**裁定**: ASCII のみの高速判定（U+0009–U+000D, U+0020）は不適合。Unicode
バージョン更新で White_Space 集合が変わった場合、本節が追随し実装が従う。
Fixture: `spec/wasm_cross/string_whitespace.almd`。

## ALS-T2 `float.parse`

受理文法（大文字小文字不問の `inf` / `infinity` / `nan` を含む）:

```
float   := ws* sign? (number | "inf" | "infinity" | "nan") ws*
number  := digits ("." digits?)? exponent? | "." digits exponent?
exponent:= ("e"|"E") sign? digits
```

**値の規範**: 受理された 10 進表記に対し、**IEEE-754 binary64 の最近接偶数丸め
（round-half-to-even）における正確な最近値**を返す。これは桁数・指数の大きさに
よらない（denormal 最小値 4.9e-324、最大値 1.7976931348623157e308、19 桁超の
仮数を含む）。オーバーフローは ±inf、アンダーフローは ±0（符号保存。`-0.0` は
負のゼロを返す）。

**エラー文言（規範）**: 空入力は `cannot parse float from empty string`、
文法違反は `invalid float literal`。exit code は通常のエラー伝播に従う。
Fixture: `spec/wasm_cross/float_parse.almd`。

## ALS-T3 `json.parse`

受理文法は RFC 8259 に、次の**裁定**を加えたもの:

- 数値は ALS-T2 の値規範で binary64 化する
- 文字列のサロゲートペア（`\uD800`–`\uDBFF` + `\uDC00`–`\uDFFF`）は合成する。
  不対サロゲートはエラー
- エラー報告は**文字単位の位置**（バイトでなくコードポイント index）を含む

Fixture: `spec/wasm_cross/json_*.almd` 群、read_message roundtrip。

## ALS-T4 `list.chunk` / `list.windows`

**裁定**: サイズ引数 `n <= 0` の挙動は次のとおり規範化する —
`chunk(xs, n<=0)` は**全体を 1 チャンク**として返し、`windows(xs, n<=0)` は
**空リスト**を返す。

> 注記: この裁定は歴史的に v0 の Rust 実装詳細（`chunks(n as usize)` の usize
> 再解釈）から生まれた挙動を**明示的に規範へ昇格**したものである。以後この挙動の
> 根拠は本節であり、Rust の型変換ではない。
> Fixture: `spec/wasm_cross/list_chunk_windows.almd`。

## ALS-T5 `string.to_upper` / `string.to_lower`

**規範は Unicode 標準の full case mapping**（UnicodeData.txt の単純対応 +
SpecialCasing.txt の 1:N 対応、例: ß→SS）。`to_lower` は **Final_Sigma 文脈規則**
（Unicode 標準 3.13: 語末の Σ→ς）を適用する。ロケール依存規則（トルコ語 İ/ı 等）は
**適用しない**（ロケール非依存の裁定）。

実装は Unicode バージョンの更新に追随する義務を負う（現行の生成表は
`scripts/gen-case-tables.py` — 生成元がいずれの実装であっても、適合判定は本節と
fixture `spec/wasm_cross/string_case_unicode.almd` に対して行う）。
