# ALS §T — Text and Number Semantics (normative)

> Last updated: 2026-08-21

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

```ebnf
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

**裁定**: サイズ引数の非正値の挙動は次のとおり規範化する —

- `n < 0`: `chunk(xs, n)` は**全体を 1 チャンク**として返す（空リストは空のまま）。
  `windows(xs, n)` は**空リスト**を返す。
- `n == 0`: **定義域エラー** — ALS-T6 の終了規約に従い、`chunk` は
  `Error: chunk size must be positive`、`windows` は
  `Error: window size must be positive` を stderr に 1 行出力し exit code 1 で
  停止する。生の Rust panic（exit 101）や wasm trap（exit 134）、無言の
  空/全ウィンドウ返しは不適合。

> 注記: `n < 0` の裁定は歴史的に v0 の Rust 実装詳細（`chunks(n as usize)` の
> usize 再解釈）から生まれた挙動を**明示的に規範へ昇格**したものである。以後この
> 挙動の根拠は本節であり、Rust の型変換ではない。`n == 0` はその再解釈でも定義
> されず（Rust は panic）、v0.28.4 で T6 形式の abort に規範化した — それ以前は
> native が生 panic、wasm は `windows(xs, 0)` が **len+1 個の空ウィンドウを
> 無言で返していた**（silent-wrong）。
> Fixtures: `spec/wasm_cross/list_chunk_windows.almd`（値ケース）、
> `list_chunk_zero.almd` / `list_windows_zero.almd`（abort ケース）。
> Contracts: C-129。

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
範囲外（`Error: index out of bounds`）、`int.clamp`/`float.clamp` の不正範囲
（lo > hi、float は NaN 境界も — `Error: clamp requires min <= max`）に適用される。
Fixtures: `spec/wasm_cross/int_div_by_zero*.almd`, `int_mod_*`, `int8_div_overflow.almd`,
`int_pow_negative_exponent.almd`, `int_rotate_nonpositive_width.almd`, `index_bounds.almd`。

## ALS-T7 トップレベル let の評価時機

モジュールのトップレベル `let` 初期化子は**宣言順に、プログラム開始時（main 実行前）に
評価される**。abort し得る初期化子（ALS-T6 の演算を含む等）は、その束縛が一度も
使用されない場合でも起動時に abort する。初期化子は先行するトップレベル束縛を
参照できる（宣言順の依存）。
Fixtures: `spec/wasm_cross/top_let_div_eager.almd`, `top_let_div_used.almd`。

## ALS-T8 整数パースの文法とエラー規範

`int.parse(s)` は、まず ALS-T1 の **Unicode White_Space 集合**を先頭・末尾から除去し
（`"\u{00A0}99\u{3000}"` → 99）、残りを `sign? digit+`（10 進、下線なし）として
読む。エラーメッセージは Rust `ParseIntError` の Display と byte 一致する:
空入力（除去後に空）は `cannot parse integer from empty string`、不正文字は
`invalid digit found in string`、範囲外は `number too large to fit in target type` /
`number too small to fit in target type`。

`int.from_hex(s)` は `i64::from_str_radix(…, 16)` **そのもの**ではなく、次の文法である
（fixture `spec/wasm_cross/int_from_hex.almd` が全辺を固定する）:

```ebnf
hex     := ws* ("0x")* sign? hexdigit+ ws*      (* "0x" は小文字のみ、何回でも剥がす *)
sign    := "+" | "-"
hexdigit:= [0-9a-fA-F]                          (* 下線は不可 *)
```

- 接頭辞 `0x` は**小文字のみ**認識し、**繰り返し**剥がす（`0x0x0x10` = 16）。
  `0X10` は `invalid digit found in string`。
- 符号は接頭辞の**後**に置く（`0x-ff` = -255）。接頭辞の前の符号（`-0xff`）は
  `invalid digit found in string`。
- 接頭辞だけ（`0x`、`0x0x`）は除去後に空なので `cannot parse integer from empty string`。
- 桁の大文字小文字は不問、`f_f` のような下線は不正文字。オーバーフローは
  `int.parse` と同じ 2 文言。

```almide
test "int.parse trims Unicode whitespace; int.from_hex strips lowercase 0x repeatedly, sign after the prefix" {
  assert_eq(int.parse("\u{00A0}99\u{3000}") ?? -1, 99)
  assert_eq(int.from_hex("0x0x0x10") ?? -1, 16)
  assert_eq(int.from_hex("0x-ff") ?? 0, -255)
  assert_eq(int.from_hex("0X10") ?? -1, -1)
  assert_eq(int.from_hex("-0xff") ?? -1, -1)
}
```

Fixture: `spec/wasm_cross/int_from_hex.almd`、`spec/wasm_cross/string_whitespace.almd`。
Contracts: C-028, C-029。

## ALS-T9 固定小数表示

`float.to_fixed(x, n)` は**正確な二進値に対する round-half-to-even**（銀行丸め）。
十進文字列経由の再丸めや half-up は不適合。n=0 の小数点無し、負数・境界値
（0.5 ちょうど等）も同規則。
Contracts: C-025。

## ALS-T10 数学関数の決定性

`math.sin/cos/tan/exp/log/pow` 等の超越関数は**全ターゲットで byte 一致**する
（実装は vendored libm を両ターゲットで共有 — ホスト libm 依存は不適合）。
真値からの距離（誤差上限）は ALS-T22 が定める。
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
minNum/maxNum 系）。`float.round` はゼロ結果の
符号を保つ（round(-0.0) = -0.0、half away from zero は不変）。±0 同士の
min/max の順序は ALS-T23（IEEE-754-2019 minimum/maximum）。
Contracts: C-049, C-140。

## ALS-T16 個数・添字の i64 クランプ

List / String の **個数（count）や添字（index）** を受け取る API は、i64 値を内部幅へ
**狭める前に**、i64 全体の上でクランプする。ラップや符号化けによる誤アクセスは
不適合。クランプの向きは API の種類で決まり、**負の個数は 0 に丸められない**
（fixture `spec/wasm_cross/string_count_truncation.almd`、
`list_count_index_truncation.almd` が固定する）:

| 種類 | 規則 | 負値・巨大値の結果 |
|------|------|--------------------|
| 個数（`list.take/drop/take_end/drop_end/chunk/windows`、`list.slice` の start/end、`string.take/drop/take_end/drop_end`、`pad_start/pad_end` の幅） | **符号なし**として `min(n as usize, len)` | 負値は巨大な符号なし値として `len` に飽和: `take(-1)` = 全体、`drop(-1)` = 空、`chunk(-1)` = 1 チャンク、`windows(-1)` = 空。`2^32` 以上も `len` に飽和（小さい値へラップしない） |
| 反復回数（`string.repeat` / `list.repeat` / `bytes.repeat`） | **符号あり**、負は 0 | `repeat(s, -1)` = 空（両ターゲット; panic / trap は不適合） |
| `string.slice` の start/end | **符号あり** `max(0).min(len)` | 負の start は 0 |
| 添字（`list.get/get_or/set/insert/remove_at/swap/update`） | **符号なし** `i as usize` | 負や `2^32` 以上の添字は範囲外として no-op / default / append の経路 |

`list.product` は `list.sum` と同じく i64 wrap（オーバーフローは 2^64 mod）。

```almide
test "negative counts saturate, negative repeat is empty, negative slice start is 0" {
  assert_eq(string.take("abcde", -1), "abcde")
  assert_eq(string.drop("abcde", -1), "")
  assert_eq(string.repeat("xy", -1), "")
  assert_eq(list.take([1, 2, 3], -1), [1, 2, 3])
}
```

Fixture: `spec/wasm_cross/string_count_truncation.almd`、
`spec/wasm_cross/list_count_index_truncation.almd`。Contracts: C-054, C-056。

## ALS-T17 datetime.format の指定子置換

`datetime.format(ts, pattern)` は strftime 系指定子 `%Y %m %d %H %M %S` を、
ゼロ埋めした暦フィールド（年 4 桁・他 2 桁）へ**逐次置換**する。native /
v0-wasm / 自己ホストの 3 バックエンドが同一の逐次 `string.replace` 列を走らせる
ため、出力はバイト一致。`%` は上記指定子の直前でのみ特別扱いされ、`%%` エス
ケープは存在しない（認識されない `%X` はそのまま素通り）。SCOPE: 年 0..9999
（5 桁年は 4 桁欄を超える — `to_iso` と同じ文書化済みの端）。Contracts: C-128。

## ALS-T18 assert の abort 形（非 test 位置）

`test` ブロック外の `assert` 族の失敗は、**stderr の構造化ブロック + exit code 1**
で停止する（T6 の終了規約ファミリ）。生の Rust panic（exit 101）や wasm trap
（exit 134）、値情報なしの出力は不適合。ブロックは `  key: value` を1行ずつ並べた
形（値の表示は ALS-R2 の補間 Display と同一）:

- `assert_eq(l, r)` →
  `Error: assertion failed\n  at: line <N>\n  expected: <r>\n  found: <l>`
- `assert_ne(l, r)` →
  `Error: assertion failed\n  at: line <N>\n  expected: != <l>\n  found: <l>`
- `assert(c)` → `Error: assertion failed\n  at: line <N>`
- `assert(c, msg)` → `Error: assertion failed: <msg>\n  at: line <N>`

フィールド順は固定で、**終端子を持たない `found` が必ず最後**（値が複数行に
またがっても曖昧にならない）。`at:` 行は呼び出しに span が無いときだけ省略され、
これは共有 frontend lowering の性質なのでターゲット間では常に一致する。

被演算子は**一度だけ評価**される（失敗メッセージは束縛済み temp を再参照する）。
`test` ブロック内はテストハーネスの報告形式に従う（本節の対象外）。
実装は frontend lowering の単一脱糖（desugar once）で、native / v0-wasm /
v1-wasm / interp の全系統が同じ IR を継ぐ。
Fixtures: `spec/wasm_cross/assert_abort_eq.almd`, `assert_abort_ne.almd`,
`assert_abort_msg.almd`, `assert_abort_multiline.almd`。Contracts: C-153。

## ALS-T19 数値決定性ファミリー

Float 計算の観測可能な結果（stdout・stderr・exit code、および `float.to_bits` の
値）は**プログラムと入力のみの関数**であり、ターゲット・ホスト CPU・OS・ビルド
フラグ・実行環境の浮動小数モードに依存しない。この一文が数値決定性の規範であり、
次の節群がその構成要素である — 既存: T2（`float.parse` 正確丸め）・T9（`to_fixed`）・
T10（超越関数の単一実装）・T13（最短往復表示）・T15（符号と NaN 無視）・
ALS-C9（totalOrder）・ALS-E3（`-0.0` 表示）・ALS-M10（等値）・ALS-R4（非有限
定数表示）・C-210（NaN の正準観測）; 本ファミリーで新設: T20（丸めと縮約禁止）・
T21（非正規数の保存）・T22（超越関数の誤差上限）・T23（符号付きゼロ）・
T24（Float → Int 変換）。

**NaN の観測境界**は `float.to_bits`・`float.to_string`・文字列補間・JSON / Value
符号化・Map / Set のキー比較であり、どの境界でも NaN は単一の正準値として観測
される（C-210: `to_bits` は `0x7FF8000000000000`）。

```almide
test "a Float result is a function of the program alone" {
  let x = float.parse("0.1") ?? 0.0
  assert_eq(float.to_bits(x + 0.2), 4599075939470750516)
  assert_eq(float.to_string(x + 0.2), "0.30000000000000004")
}
```

Fixture: 本ファミリーの fixture（`spec/wasm_cross/float_no_contraction.almd`、
`float_subnormal_preserved.almd`、`math_transcendental_bits.almd`、
`float_signed_zero_minmax.almd`、`float_to_int_edges.almd`）はすべて C-302 を
併記する。Contracts: C-302。

## ALS-T20 丸めと縮約の禁止

Float の各演算（`+` `-` `*` `/`、`math.sqrt`、型変換）は IEEE-754 binary64 の
**最近接偶数丸め**（round-to-nearest, ties-to-even）で **1 演算ごとに**丸める。
他の丸めモードは観測できない。**縮約（contraction）は不適合**: `a * b + c` は
乗算の丸めと加算の丸めの 2 回であり、融合積和（FMA）の 1 回丸めに置き換えては
ならない — wasm に融合命令は無く、native バックエンドも生成してはならない。
融合演算はこの言語に存在しない（必要になれば名前付き関数として別節で規範化する
ものであり、暗黙のモードにはならない）。

例: `a = 1 + 2^-52`、`b = 1 - 2^-53` のとき `a * b` の正確値は `1 + 2^-53 - 2^-105`
で、binary64 では `1.0` に丸まる。ゆえに `a * b - 1.0 = 0.0`。縮約されていれば
`2^-53 - 2^-105 ≈ 1.1e-16` が残り、両ターゲットの一致も壊れる。

```almide
test "a*b+c rounds twice — no fused multiply-add" {
  let a = float.parse("1.0000000000000002") ?? 0.0
  let b = float.parse("0.9999999999999999") ?? 0.0
  assert_eq(a * b - 1.0, 0.0)
  assert_eq(a * b + (0.0 - 1.0), 0.0)
}
```

Fixture: `spec/wasm_cross/float_no_contraction.almd`、
`spec/stdlib/float_determinism_test.almd`。Contracts: C-303。

## ALS-T21 非正規数の保存

binary64 の非正規数（`|x| < 2^-1022`、最小 `2^-1074 ≈ 4.9e-324`）は、どの
ターゲット・ホストでも**値として保存**される。flush-to-zero・denormals-are-zero
（入力側・出力側のいずれも）は不適合: `2.2250738585072014e-308 / 2.0` は 0 ではなく
`2^-1023` であり、`float.to_bits` は `2251799813685248`（= `2^51`）を返す。
parse（T2）・表示（T13）・算術・`to_bits` のどの境界でも非正規数は消えない。

```almide
test "subnormals survive arithmetic and observation" {
  let tiny = float.parse("2.2250738585072014e-308") ?? 0.0
  assert_eq(float.to_bits(tiny / 2.0), 2251799813685248)
  assert(tiny / 2.0 != 0.0)
  assert_eq(float.to_bits(float.parse("5e-324") ?? 0.0), 1)
}
```

Fixture: `spec/wasm_cross/float_subnormal_preserved.almd`、
`spec/stdlib/float_determinism_test.almd`。Contracts: C-304。

## ALS-T22 超越関数の誤差上限

T10 が全ターゲットでの byte 一致（単一の vendored 実装、ホスト libm 不使用）を
定めるのに対し、本節は**真値からの距離**を宣言する。距離は「正確丸め結果との
ulp 差」（同符号の binary64 同士では `float.to_bits` の差の絶対値）で測る。

| 関数 | 上限 |
|------|------|
| `math.sqrt` / `float.sqrt` | 正確丸め（0 ulp） — IEEE-754 が要求する |
| `math.exp` `math.log` `math.log2` `math.log10` | ≤ 1 ulp |
| `math.sin` `math.cos` `math.tan` | ≤ 1 ulp |
| `math.fpow` / Float `**` | ≤ 1 ulp |
| `math.log_gamma` | 宣言なし — byte 一致（T10）のみが規範で、精度は主張しない |

表にない関数の精度は主張されない。上限は標本点で実行検証される（下の test と
`spec/stdlib/math_accuracy_test.almd`: 参照値は 70 桁十進で計算し正確丸めした
binary64）。

```almide
fn ulp_from(x: Float, reference_bits: Int) -> Int = math.abs(float.to_bits(x) - reference_bits)

test "sqrt is correctly rounded; exp/log/sin stay within 1 ulp at sample points" {
  assert_eq(ulp_from(math.sqrt(2.0), 4609047870845172685), 0)
  assert(ulp_from(math.exp(1.0), 4613303445314885481) <= 1)
  assert(ulp_from(math.log(2.0), 4604418534313441775) <= 1)
  assert(ulp_from(math.sin(1.0), 4605754516372524270) <= 1)
}
```

Fixture: `spec/wasm_cross/math_transcendental_bits.almd`、
`spec/stdlib/math_accuracy_test.almd`。Contracts: C-305。

## ALS-T23 符号付きゼロ

ゼロの符号は IEEE-754 の規則で伝播する: `-0.0 * 1.0 = -0.0`、`-0.0 + 0.0 = 0.0`
（最近接偶数丸めでは異符号ゼロの和は `+0` — `+0.0` がリテラルでも同じで、
`x + 0.0 → x` は `x = -0.0` に対して成り立たない恒等式なので、実装は畳み込んでは
ならない。`x - 0.0 → x` と `x * 1.0 → x` は成り立つ）、`1.0 / -inf = -0.0`。等値は符号を
無視する（`0.0 == -0.0` は真、ALS-M10）。表示は符号を保つ（`-0.0` → `-0.0`、
ALS-E3）。`float.min/max`・`math.fmin/fmax` は **IEEE-754-2019 `minimum`/`maximum`
のゼロ順序**に従う: `-0.0 < +0.0` として扱い、`min(-0.0, 0.0) = min(0.0, -0.0) = -0.0`、
`max(-0.0, 0.0) = max(0.0, -0.0) = +0.0` — 引数順に依存せず（可換）、ALS-C9 の
totalOrder と整合する。NaN は T15 のとおり無視される（片方が NaN なら他方、両方
NaN なら NaN）。

**裁定（2026-08-21, ADR-0016）**: 従来の「±0 の同値では第 1 引数を返す」規則
（C-049 の旧文）は撤回し、本節が置き換える。

```almide
fn neg_zero() -> Float = (0.0 - float.abs(-1.0)) * 0.0

test "signed zero propagates, compares equal, and orders -0 < +0 in min/max" {
  let mz = neg_zero()
  assert_eq(float.to_string(mz), "-0.0")
  assert(mz == 0.0)
  assert_eq(float.to_string(mz + 0.0), "0.0")
  assert_eq(float.sign(float.min(mz, 0.0)), -1.0)
  assert_eq(float.sign(float.min(0.0, mz)), -1.0)
  assert_eq(float.sign(float.max(mz, 0.0)), 1.0)
  assert_eq(float.sign(float.max(0.0, mz)), 1.0)
  assert_eq(float.sign(math.fmin(0.0, mz)), -1.0)
  assert_eq(float.sign(math.fmax(mz, 0.0)), 1.0)
}
```

Fixture: `spec/wasm_cross/float_signed_zero_minmax.almd`、
`spec/wasm_cross/float_sign_minmax_ieee.almd`、
`spec/stdlib/float_determinism_test.almd`（伝播・等値）、
`spec/stdlib/float_minmax_zero_order_test.almd`（±0 の順序 — 0.58.0 リリースは
旧規則のままなので、ピン前進までこのファイルが赤）、
`spec/stdlib/float_signed_zero_sum_test.almd`（リテラル `0.0` との和 — 0.58.0 の
native は `x + 0.0` を `x` に畳み込んでおり、このファイルの赤がその発見）。
Contracts: C-306。

## ALS-T24 Float → Int 変換

`float.to_int(x)` は**ゼロ方向切り捨て**（truncation）で、範囲外は **i64 の両端に
飽和**し、NaN は `0`: `to_int(2.7) = 2`、`to_int(-2.7) = -2`、`to_int(+inf)` は
`9223372036854775807`、`to_int(-inf)` は `-9223372036854775808`、`to_int(9.3e18)`
は `9223372036854775807`（飽和）、`to_int(NaN) = 0`、`to_int(-0.0) = 0`。
checked 族（`float.to_{int8,int16,int32,int64,uint8,uint16,uint32,uint64}_checked`、
`float.to_float32_checked`）は**値が目標型で正確に表現できるときだけ `some`**:
小数部を持つ値・範囲外・NaN・±inf は `none`（`to_int64_checked(2.7) = none`、
`to_int64_checked(-0.0) = some(0)`、`to_float32_checked(0.1) = none`）。
`float.floor/ceil/round` は Float → Float であり本節の対象外（round の ±0 は T15）。

```almide
test "float.to_int truncates toward zero, saturates, and maps NaN to 0" {
  let inf = 1.0 / 0.0
  assert_eq(float.to_int(2.7), 2)
  assert_eq(float.to_int(-2.7), -2)
  assert_eq(float.to_int(inf), 9223372036854775807)
  assert_eq(float.to_int(0.0 - inf), -9223372036854775808)
  assert_eq(float.to_int(0.0 / 0.0), 0)
  assert_eq(float.to_int64_checked(2.7), none)
  assert_eq(float.to_int64_checked(-0.0), some(0))
}
```

Fixture: `spec/wasm_cross/float_to_int_edges.almd`、
`spec/stdlib/float_determinism_test.almd`。Contracts: C-307。
