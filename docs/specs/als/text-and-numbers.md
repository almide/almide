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

## ALS-T6 整数演算の終了規約（termination convention）

整数の `/`・`%` は**全域**である: ゼロ除数は stderr に `Error: division by zero`、
符号付き最小値 ÷ −1（各ビット幅の真の MIN）は `Error: integer overflow` を1行出力し
**exit code 1 で停止**する。ハードウェア trap（wasm unreachable、exit 134 等）や
無言の wrap は不適合。同じ規約は `math.pow` の負指数（`Error: negative exponent`）、
`int.rotate_*` の非正幅（`Error: rotate width must be positive`）、リスト添字の
範囲外（`Error: index out of bounds`）に適用される。
Fixtures: `spec/wasm_cross/int_div_by_zero*.almd`, `int_mod_*`, `int8_div_overflow.almd`,
`int_pow_negative_exponent.almd`, `int_rotate_nonpositive_width.almd`, `index_bounds.almd`。

## ALS-T7 トップレベル let の評価時機

モジュールのトップレベル `let` 初期化子は**宣言順に、プログラム開始時（main 実行前）に
評価される**。abort し得る初期化子（ALS-T6 の演算を含む等）は、その束縛が一度も
使用されない場合でも起動時に abort する。初期化子は先行するトップレベル束縛を
参照できる（宣言順の依存）。
Fixtures: `spec/wasm_cross/top_let_div_eager.almd`, `top_let_div_used.almd`。

## ALS-T8 整数パースのエラー規範

`int.parse` のエラーメッセージは Rust `ParseIntError` の Display と byte 一致する:
空入力は `cannot parse integer from empty string`、不正文字は
`invalid digit found in string`、範囲外は `number too large to fit in target type` /
`number too small to fit in target type`。`int.from_hex` は `i64::from_str_radix(s, 16)`
と観測等価（`+`/`-` 接頭辞・大文字小文字・オーバーフローの native 特性を含む）。
Contracts: C-028, C-029。

## ALS-T9 固定小数表示

`float.to_fixed(x, n)` は**正確な二進値に対する round-half-to-even**（銀行丸め）。
十進文字列経由の再丸めや half-up は不適合。n=0 の小数点無し、負数・境界値
（0.5 ちょうど等）も同規則。
Contracts: C-025。

## ALS-T10 数学関数の決定性

`math.sin/cos/tan/exp/log/pow` 等の超越関数は**全ターゲットで byte 一致**する
（実装は vendored libm を両ターゲットで共有 — ホスト libm 依存は不適合）。
Contracts: C-026。

## ALS-T11 バイナリテキスト符号化

`base64.encode/decode`（standard + URL-safe）と `hex.encode/decode` は RFC 4648
に従い、decode エラーは**位置情報込みで**両ターゲット同文言。大文字小文字の
扱い・パディング規則・不正長の検出を含む。
Contracts: C-027, C-030。

## ALS-T12 非 abort 整数除算の一致

abort に至らない `/`・`%` は Rust の `i64` truncating division / remainder と
byte 一致する（負数の丸め方向・余りの符号を含む: `-7 / 2 == -3`、`-7 % 2 == -1`）。
Contracts: C-003。

## ALS-T13 浮動小数の文字列化

`float.to_string` は**最短往復十進表現**（shortest round-tripping decimal、
Dragon4/Ryū 等価）: `parse(to_string(x)) == x` かつ、それを満たす最短の桁数。
整数値は `.0` を保持（ALS-R2 の Display と区別）。
Contracts: C-023。

## ALS-T14 wrap / rotate のマスク飽和

`int.wrap_*` / `int.rotate_*` の bits 引数が 64 を超える場合、マスクは
`u64::MAX` に**飽和**する（モジュロではない）。bits ≤ 0 は ALS-T6 の abort。
Contracts: C-048。

## ALS-T15 符号と min/max の NaN 規則

`float.sign` は `f64::signum`（NaN → NaN、±0 → ±1）。`float.min/max`・
`math.min/max` は **NaN を無視**する（片方が NaN なら他方を返す — IEEE-754
minNum/maxNum 系、Rust `f64::min/max` と一致）。
Contracts: C-049。

## ALS-T16 長さ・添字の i64 クランプ

List / String の長さ・添字を受け取る API は、i64 値を内部幅へ**先に clamp**
してから使う（負→0、上限超→len）。ラップや符号化けによる誤アクセスは不適合。
`list.product` は `list.sum` と同じく i64 wrap（オーバーフローは 2^64 mod）。
Contracts: C-054, C-056。
