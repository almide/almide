# ALS §T — Text and Number Semantics (normative)

> Last updated: 2026-10-24

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

受理文法（**有限値のみ**。`inf` / `infinity` / `nan` の綴りは大文字小文字・符号を
問わず受理しない — C-413）:

```ebnf
float   := ws* sign? number ws*
number  := digits ("." digits?)? exponent? | "." digits exponent?
exponent:= ("e"|"E") sign? digits
ws      := U+0009 | U+000A | U+000C | U+000D | U+0020
```

`ws` は **ASCII の 5 文字だけ**である（C-413）。ALS-T1 の Unicode White_Space 集合
ではなく、U+000B（VT）も含まない。それ以外の空白は文法違反になる。

**値の規範**: 受理された 10 進表記に対し、**IEEE-754 binary64 の最近接偶数丸め
（round-half-to-even）における正確な最近値**を返す。これは桁数・指数の大きさに
よらない（denormal 最小値 4.9e-324、最大値 1.7976931348623157e308、19 桁超の
仮数を含む）。丸めた結果が ±inf になる表記（オーバーフロー）はエラー、アンダーフローは
±0（符号保存。`-0.0` は負のゼロを返す）。結果は常に有限である。

**エラー文言（規範）**: 空入力は `cannot parse float from empty string`、
文法違反（非有限値の綴りを含む）は `invalid float literal`、オーバーフローは
`float literal out of range`。exit code は通常のエラー伝播に従う。
Fixture: `spec/wasm_cross/float_parse.almd`、`float_parse_finite_only.almd`、
`number_parse_ascii_trim.almd`。

## ALS-T3 `json.parse`

受理文法は RFC 8259 に、次の**裁定**を加えたもの:

- 数値は ALS-T2 の値規範で binary64 化する。ただしオーバーフローはエラーにせず
  ±inf とする（`float.parse` だけが有限値に限られる — C-413）
- 文字列のサロゲートペア（`\uD800`–`\uDBFF` + `\uDC00`–`\uDFFF`）は合成する。
  不対サロゲートは**黙って落ちる**（寛容 — `json_string_span.almd` が
  両ターゲット同一で pin。厳格拒否は不採用）
- 先頭が値として読める入力は**前置 parse**で受理される（`"[1] x"` →
  `ok([1])` — RFC 8259 の全文一致要求への寛容拡張、0.59.1 実測）
- エラー報告は**文字単位の位置**（バイトでなくコードポイント index）を含む

Fixture: `spec/wasm_cross/json_*.almd` 群（`json_string_span.almd` 含む）。

## ALS-T4 `list.chunk` / `list.windows`

**裁定**: サイズ引数の非正値の挙動は次のとおり規範化する —

- `n <= 0`: **定義域エラー**。負のサイズは ALS-T16 の負の個数規則で 0 と
  読む（almide/almide#4147）ので、`n < 0` も `n == 0` と同じく — ALS-T6 の終了規約に従い、`chunk` は
  `Error: chunk size must be positive`、`windows` は
  `Error: window size must be positive` を stderr に 1 行出力し exit code 1 で
  停止する。生の Rust panic（exit 101）や wasm trap（exit 134）、無言の
  空/全ウィンドウ返しは不適合。

> 注記: 以前の `n < 0` の裁定（`chunk` は全体を 1 チャンク、`windows` は空）は
> v0 の Rust 実装詳細（`chunks(n as usize)` の usize 再解釈）を規範へ昇格した
> ものだったが、負の個数を 0 と読む ALS-T16 の規則（#4147）に合わせて撤回した。
> 負のサイズの abort は `spec/wasm_cross/list_chunk_negative.almd` が固定する。`n == 0` はその再解釈でも定義
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

**ASCII への畳み込み禁止**（C-412）: ASCII のスカラーは ASCII の規則で写す。非 ASCII の
スカラーは上記の対応で写すが、写した結果に ASCII のスカラーが一つでも含まれるなら、
そのスカラーは**変えずに残す**。Unicode 標準の対応そのものが U+0131→I、U+212A→k、
U+017F→S を含むため、この規則は標準からの意図的な逸脱である。現行の Unicode で
残るスカラーは、`to_upper` が U+00DF, U+0131, U+0149, U+017F, U+01F0,
U+1E96–U+1E9A, U+FB00–U+FB06、`to_lower` が U+0130, U+212A。Final_Sigma の文脈判定は
元の文字列に対して行う（残した U+212A も cased として数える）。`capitalize` の先頭文字も
同じ規則に従う。この写像はロケール非依存であり、識別子の同一性比較には使わない。

実装は Unicode バージョンの更新に追随する義務を負う（現行の生成表は
`scripts/gen-case-tables.py` — 生成元がいずれの実装であっても、適合判定は本節と
fixture `spec/wasm_cross/string_case_unicode.almd`、`string_case_no_ascii_fold.almd`
に対して行う）。

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

呼び出しスタックの枯渇も同じ終了形に従う: native と組み込み wasm ホスト
（`almide run --target wasm`）は stderr に `Error: stack overflow` を1行出力し
**exit code 1 で停止**する。それまでに書かれた stdout は失われない。Rust の
`thread 'main' has overflowed its stack`（SIGABRT、exit 134）や
`Error: wasm trap: call stack exhausted` は不適合。枯渇する深さ（閾値）は統一せず、
ターゲットごとの構造的資源として C-196 が宣言する: native は枯渇したスレッド自身の
スタック（main スレッドはプラットフォーム既定の 8 MiB、`fan` の腕や並列ワーカーの
スレッドは Rust 既定の 2 MiB）、組み込みホストは 8 MiB の wasm スタック。stock
ランタイム（素の `wasmtime run` で実行する p1 モジュール・コンポーネント、ブラウザ）
での枯渇はホスト自身の trap であり（wasmtime では exit 134）、本規約の宣言された
残余である（C-196）。Fixtures: `spec/wasm_fail/stack_overflow_abort.almd`（abort）、
`spec/wasm_cross/recursion_depth_within_limits.almd`（両ターゲットの限界内の再帰は
byte 一致）。

## ALS-T7 トップレベル let の評価時機

モジュールのトップレベル `let` 初期化子は**宣言順に、プログラム開始時（main 実行前）に
評価される**。abort し得る初期化子（ALS-T6 の演算を含む等）は、その束縛が一度も
使用されない場合でも起動時に abort する。初期化子は先行するトップレベル束縛を
参照できる（宣言順の依存）。
Fixtures: `spec/wasm_cross/top_let_div_eager.almd`, `top_let_div_used.almd`。

## ALS-T8 整数パースの文法とエラー規範

`int.parse(s)` は、まず **ASCII 空白**（U+0009, U+000A, U+000C, U+000D, U+0020 — ALS-T2 の
`ws`。VT と非 ASCII の空白は含まない、C-413）を先頭・末尾から除去し
（`"\u{00A0}99\u{3000}"` は `invalid digit found in string`）、残りを `sign? digit+`（10 進、下線なし）として
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
test "int.parse trims ASCII whitespace only; int.from_hex strips lowercase 0x repeatedly, sign after the prefix" {
  assert_eq(int.parse("\u{00A0}99\u{3000}") ?? -1, -1)
  assert_eq(int.parse("\t 99\r\n") ?? -1, 99)
  assert_eq(int.from_hex("0x0x0x10") ?? -1, 16)
  assert_eq(int.from_hex("0x-ff") ?? 0, -255)
  assert_eq(int.from_hex("0X10") ?? -1, -1)
  assert_eq(int.from_hex("-0xff") ?? -1, -1)
}
```

Fixture: `spec/wasm_cross/int_from_hex.almd`、`spec/wasm_cross/string_whitespace.almd`、
`spec/wasm_cross/number_parse_ascii_trim.almd`。
Contracts: C-028, C-029。

## ALS-T9 固定小数表示

`float.to_fixed(x, n)` は**正確な二進値に対する round-half-to-even**（銀行丸め）。
十進文字列経由の再丸めや half-up は不適合。n=0 の小数点無し、負数・境界値
（0.5 ちょうど等）も同規則。n の定義域は **0..=4096** で、域外は T6 形
（`Error: to_fixed requires decimals in 0..=4096`、exit 1）で停止する。
非有限値は表示形をそのまま返す: `inf`・`-inf`・`NaN`。ゼロ量の符号は保存
（`to_fixed(-0.4, 0)` は `-0`）。Fixtures:
`spec/wasm_cross/to_fixed_domain_abort.almd`、
`spec/wasm_cross/to_fixed_wide_precision.almd`。
Contracts: C-025。

## ALS-T10 数学関数の決定性

`math.sin/cos/tan/exp/log/pow` 等の超越関数は**全ターゲットで byte 一致**する
（実装は vendored libm を両ターゲットで共有 — ホスト libm 依存は不適合）。
真値からの距離（誤差上限）は ALS-T22 が定める。
Contracts: C-026。

## ALS-T11 バイナリテキスト符号化

`base64.encode/decode`（standard + URL-safe）と `hex.encode/decode` は RFC 4648
に従い、decode エラーは両ターゲット同文言（base64 は位置なし; 位置付きは hex 側
`invalid hex char at 0`、C-030）。大文字小文字の扱い・パディング規則・不正長の
検出を含む。

base64 の encode は `=` 付きの正規形を出し、`base64.decode`（`+` `/`）と
`base64.decode_url`（`-` `_`）は**自分の英字の正規形だけ**を受理する。一つのバイト列
の綴りは一つである（RFC 4648 §12 の可鍛性を持たない）。次の順に検査し、最初に
違反した規則の err を返す:

| 規則 | err |
|------|-----|
| UTF-8 のバイト長が 4 の倍数 | `invalid base64 length: <バイト長>` |
| 末尾の `=` は 2 個まで | `invalid base64 padding` |
| 残りのバイトはすべてその関数の英字（途中の `=`、他方の英字の 62/63 番を含め不可） | `invalid base64 character` |
| `=` が落とすビット（`=` 1 個なら最後の文字の下位 2 ビット、2 個なら下位 4 ビット）が 0 | `invalid base64 trailing bits` |

`QQ==` は `A`、`QR==` は `invalid base64 trailing bits`、`QQ` は
`invalid base64 length: 2`、`QUJD====` は `invalid base64 padding`、
`decode("-_8=")` と `decode_url("+/8=")` は `invalid base64 character`。

テスト: `spec/wasm_cross/encoding_base64.almd`、
`spec/wasm_cross/base64_decode_canonical.almd`、`spec/stdlib/encoding_test.almd`。
Contracts: C-027, C-030, C-411。

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
`u64::MAX` に**飽和**する（モジュロではない）。bits ≤ 0 は wrap 族では
**黙って全域**（`wrap_add(1,1,0)` → 0、`wrap_add(1,1,-1)` → 2 — 0.59.1
実測、abort しない）; abort するのは rotate 族のみ（`Error: rotate width
must be positive`、ALS-T6）。
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
不適合。クランプの向きは API の種類で決まる。**負の個数は 0 と読む**
（almide/almide#4147。以前は符号なしに再解釈して `take(xs, -1)` が全体を
返し、`?limit=-1` が件数の上限を外した）。fixture
`spec/wasm_cross/negative_count_rule.almd`、`string_count_truncation.almd`、
`list_count_index_truncation.almd` が固定する:

| 種類 | 規則 | 負値・巨大値の結果 |
|------|------|--------------------|
| 個数（`list.take/drop/take_end/drop_end/split_at`、`string.take/drop/take_end/drop_end/split_at`、`bytes.take/drop/take_end/drop_end/split_at`、`pad_start/pad_end` の幅）と範囲の端（`list.slice`・`bytes.slice`・`matrix.slice_rows` の start/end） | **符号あり** `max(n, 0).min(len)` | 負値は 0: `take(-1)` = 空、`drop(-1)` = 全体、`slice(xs, -2, 3)` = `slice(xs, 0, 3)`。`2^32` 以上は `len` に飽和（小さい値へラップしない） |
| サイズ（`list.chunk/windows/window`、`string.chunk/window`、`bytes.chunk/window`） | 定義域は 1 以上（ALS-T4） | 0 以下（負値を含む）は abort |
| 反復回数（`string.repeat` / `list.repeat` / `bytes.repeat`） | **符号あり**、負は 0 | `repeat(s, -1)` = 空（両ターゲット; panic / trap は不適合） |
| `string.slice` の start/end | **符号あり** `max(0).min(len)` | 負の start は 0 |
| 添字（`list.get/get_or/set/insert/remove_at/swap/update`） | **符号なし** `i as usize` | 負や `2^32` 以上の添字は範囲外として no-op / default / append の経路 |

`list.product` は `list.sum` と同じく i64 wrap（オーバーフローは 2^64 mod）。

```almide
test "a negative count is 0: take is empty, drop keeps everything, repeat is empty" {
  assert_eq(string.take("abcde", -1), "")
  assert_eq(string.drop("abcde", -1), "abcde")
  assert_eq(string.repeat("xy", -1), "")
  assert_eq(list.len(list.take([1, 2, 3], -1)), 0)
  assert_eq(list.slice([1, 2, 3], -2, 2), [1, 2])
}
```

Fixture: `spec/wasm_cross/negative_count_rule.almd`、
`spec/wasm_cross/string_count_truncation.almd`、
`spec/wasm_cross/list_count_index_truncation.almd`。Contracts: C-054, C-056。

## ALS-T17 datetime.format の指定子置換

`datetime.format(ts, pattern)` は strftime 系指定子 `%Y %m %d %H %M %S` を、
ゼロ埋めした暦フィールド（年 4 桁・他 2 桁）へ**逐次置換**する。native /
v0-wasm / 自己ホストの 3 バックエンドが同一の逐次 `string.replace` 列を走らせる
ため、出力はバイト一致。`%` は上記指定子の直前でのみ特別扱いされ、`%%` エス
ケープは存在しない（認識されない `%X` はそのまま素通り）。`%Y` の欄幅は
`to_iso` と同じ**可変幅**である — 最低 4 桁、ゼロ埋め、負の年は符号が 1 桁を
占める（`-292277022657`、`0000`）。固定 4 桁欄は、桁あふれの年で下位 4 桁だけを、
負の年で非 ASCII バイトを書いていた。Contracts: C-128、C-359。

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

`testing` モジュールの assert も、`test` ブロック外の直接呼び出しでは同じブロック
（見出しは `Error: assertion failed`、`at:` → `expected:` → `found:`）と exit code 1
で停止する。`expected:` には関係を、`found:` には被演算子を書く:

- `testing.assert_gt(a, b)` → `expected: > <b>`、`found: <a>`
- `testing.assert_lt(a, b)` → `expected: < <b>`、`found: <a>`
- `testing.assert_approx(a, b, t)` → `expected: within <t> of <b>`、`found: <a>`
  （合格は |a − b| < t のときだけ。NaN は不合格）
- `testing.assert_contains(h, n)` → `expected: contains <n>`、`found: <h>`
  （空の `n` は合格）
- `testing.assert_some(o)` → `expected: some(_)`、`found: none`
- `testing.assert_none(o)` → `expected: none`、`found: <o>`
- `testing.assert_ok(r)` → `expected: ok(_)`、`found: <r>`
- `testing.assert_err(r)` → `expected: err(_)`、`found: <r>`

関数値として渡した呼び出しは直接呼び出しではなく、本節の対象外である。
Fixtures: `spec/wasm_cross/testing_assert_abort_gt.almd`、`_lt`、`_approx`、
`_contains`、`_some`、`_none`、`_ok`、`_err`。Contracts: C-381。

## ALS-T19 数値決定性ファミリー

Float 計算の観測可能な結果（stdout・stderr・exit code、および `float.to_bits` の
値）は**プログラムと入力のみの関数**であり、ターゲット・ホスト CPU・OS・ビルド
フラグ・実行環境の浮動小数モードに依存しない。この一文が数値決定性の規範であり、
次の節群がその構成要素である — 既存: T2（`float.parse` 正確丸め）・T9（`to_fixed`）・
T10（超越関数の単一実装）・T13（最短往復表示）・T15（符号と NaN 無視）・
ALS-C9（totalOrder）・ALS-E3（`-0.0` 表示）・ALS-C3（等値）・ALS-R4（非有限
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
無視する（`0.0 == -0.0` は真、ALS-C3）。表示は符号を保つ（`-0.0` → `-0.0`、
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
`spec/stdlib/float_minmax_zero_order_test.almd`（±0 の順序 — 0.59.1 まで
旧規則のままなので、ピン前進までこのファイルが赤）、
`spec/stdlib/float_signed_zero_sum_test.almd`（リテラル `0.0` との和 —
0.58.0 native の `x + 0.0 -> x` 畳み込みをこのファイルの赤が発見し、
0.59.1 が符号厳密な恒等式だけ残す形で修正済み・現緑）。
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

## ALS-T25 matrix の超越計算と積の順序

`matrix` の関数のうち超越関数に届くもの（`matrix.pow`）は、ホストの libm ではなく
**同梱の musl libm**（T22）で計算する。両ターゲットで同じバイトを出し、同じ
プログラムはどのプラットフォームでビルドしても同じバイトを出す（C-223）。積
（`mul`・`linear_row`・`linear_row_no_bias`・`conv1d`）はどのレッグでも native の
スカラーループで計算する: 0.0 から累積し、k は昇順、乗算してから加算し、融合
積和は使わない。正準 fast-exp を通っていた活性化関数（`softmax_rows`・`gelu`・
`swiglu_gate`）と rope の逆周波数表は、dialect epoch 14 で標準ライブラリを離れ、
パッケージ almide/nnlib に移った（C-270）。Fixtures:
`spec/wasm_cross/matrix_pow_libm.almd`。Contracts: C-223。

## ALS-T26 datetime の暦フィールドと parse_iso の文法

Unix 秒 `ts` から暦フィールドを取り出す `datetime.year` / `month` / `day` /
`weekday`（および両者を通る `to_iso` / `format`）は、日番号を**床除算**で求める
— `ts` が負で日境界上にないときも、その秒が属する日を答える:
`to_iso(-1)` は `1969-12-31T23:59:59Z`、`weekday(-1)` は `Wednesday`。
時刻側（`hour` / `minute` / `second`）は従来どおり床剰余であり、両者は同じ日を
指す。`from_parts` は値域内で逆関数である（`from_parts(1969, 12, 31, 23, 59, 59)`
は `ok(-1)`）。

`datetime.from_parts(y, mo, d, h, mi, s)` は `Result[Int, String]` を返し、
`parse_iso` と**同じ値域・同じ文言**で範囲外の欄を err にする（年 → 月 → 日 →
時 → 分 → 秒の順で最初の範囲外の欄。almide/almide#4149）。繰り上がりはしない:
`from_parts(2026, 2, 30, 0, 0, 0)` は `err("day out of range: 30")` であって
2026-03-02 ではない。二つの構築子が同じ入力を別の日付として読むことはない。
Fixture: `spec/wasm_cross/datetime_from_parts_strict.almd`。

`datetime.parse_iso(s)` は前後の空白を除去し、次の文法を読む:

```ebnf
datetime := date "T" time zone?
date     := digit+ "-" digit+ "-" digit+
time     := digit+ ":" digit+ ":" digit+
zone     := "Z" | ("+" | "-") digit+ ":" digit+
```

各欄は ASCII 数字のみ（幅は任意 — `2024-1-5T1:2:3` は受理、符号・空白・英字は
不可）。`Z` と数値オフセットは排他である。オフセットは**適用**され、結果は UTC
の瞬間を指す: `2024-01-15T10:30:00+09:00` は 1705282200。値域は年 0..=9999
（RFC 3339 §5.6 の `date-fullyear = 4DIGIT`。範囲外の年は ok の巻き戻った瞬間では
なく err になる）、月 1..=12、
日 1..=その月の日数（先発グレゴリオ暦の閏年規則）、時 0..=23、分・秒 0..=59
（閏秒なし）、オフセットは時 0..=23・分 0..=59。

err の文字列は次のとおり（先に違反した欄が勝つ）:

| 入力 | err |
|------|-----|
| 年・月・日・時・分・秒が範囲外 | `<field> out of range: <value>`（`year` / `month` / `day` / `hour` / `minute` / `second`） |
| オフセットが不正または範囲外 | `invalid offset: <符号以降の文字列>` |
| その他の不正な綴り | `invalid datetime format` |
| `T` が無い | `expected YYYY-MM-DDTHH:MM:SSZ` |

テスト: `spec/wasm_cross/datetime_pre_epoch.almd`、
`spec/wasm_cross/datetime_parse_iso_strict.almd`、
`spec/wasm_cross/datetime_parse_iso_year_range.almd`、
`spec/stdlib/datetime_test.almd`。Contracts: C-359、C-360、C-410。

## ALS-T27 url.parse の authority 規範

`url.parse(s)` は `scheme://host[:port][/path][?query][#fragment]` を読み、
**host** を（authority ではなく）検査する。host が空なら
`url.parse: empty host` — `http://:8` のように port だけがある綴りも同じく空
host である。port は ASCII 数字のみ・0..=65535 で、数字以外は
`url.parse: invalid port: <text>`（`+80`・`-0`・` 80` を含む）、範囲外は
`url.parse: port out of range: <text>`。scope 外と文書化されている二つの形は、
host として読まずに名指しで拒否する: userinfo は
`url.parse: userinfo is not supported: <authority>`、IPv6 リテラルは
`url.parse: IPv6 hosts are not supported: <authority>`。RFC 3986 の reg-name
文字集合（unreserved / sub-delims / `%`）外を含む host は
`url.parse: invalid host: <host>`。受理された URL は `to_string` で往復する。

テスト: `spec/wasm_cross/url_authority_edges.almd`、
`spec/stdlib/url_test.almd`。Contracts: C-361。

## ALS-T28 Float32 演算の binary32 丸め

`Float32` の各演算（`+` `-` `*` `/` `%` `**`、単項 `-`）の結果は IEEE-754 **binary32**
の最近接偶数丸めで **1 演算ごとに** f32 に丸まり、native の `f32` 演算と同じ値に
なる。より広いスロット（f64）で `Float32` を運ぶ実装も、各演算の直後に binary32 へ
丸めなければならない — 丸めない値は `float32.to_float64` による拡幅、比較、後続の
演算のすべてで native と異なる。`Float32` への変換（`float.to_float32`、
`int.to_float32` と sized 整数の `to_float32`）は 1 回丸める。比較（`==` `!=` `<`
`<=` `>` `>=`）は丸めた binary32 の値を比べる。`Float32` 文脈の裸の float
リテラルは生まれたときに binary32 へ狭まる（ALS-E3）。`math.*` は `Float` のみを
受け取り、`Float32` は明示の拡幅（`float32.to_float64`）を経て渡る。`Float32` の
`a ** b` は、拡幅した両辺の `Float` の `**`（ALS-T10 の単一の pow 実装）を 1 回
binary32 へ丸めた値である。

例: `1/3` は `0.3333333432674408` に拡幅され、`2^24 + 1` は `2^24` に丸まり、
`0.1` を 10 回足した和は `1.0000001192092896`。

テスト: `spec/wasm_cross/float32_arithmetic_rounds.almd`。Contracts: C-371。

## ALS-T29 Float32 の表示

`Float32` は、**同じ binary32 に往復する最短の十進表現**（Rust `f32` の Display、
位置記法・指数なし）で表示される。`0.1` は `0.1` であり、拡幅した f64 の
`0.10000000149011612` ではない。二つの最短候補がちょうど等距離なら大きさを
切り上げる（`2^-12` は `0.00024414063`）。文字列補間とコンテナ内（リスト・
レコード・Option・タプル・variant のペイロード）は整数値の `.0` を落とし
（ALS-R2 / C-011 の規則）、`float32.to_string` は保つ。`-0` の符号を保ち、
非有限値は `inf` / `-inf` / `NaN`。

テスト: `spec/wasm_cross/float32_display_shortest.almd`。Contracts: C-372。

## ALS-T30 testing.assert_aborts — 子で走る abort 表明

`testing.assert_aborts(s, f)` は `test` ブロックの中で、`f` **だけ**を子で実行する。
native では同じテストバイナリを新しい子プロセスとして起動し、wasm では同じ
モジュールの新しいインスタンスを作る。子はプログラムの初期状態から始まる:
トップレベルの `let` と `var` は初期値を持ち、親の状態は一切見えない。cwd・環境
変数・argv はテストと同じである。`f` は effect closure でもよく、その値は捨てる。

`f` はその場に書いたラムダでなければならず、テストのローカル（`let`・`var`・
`where` の束縛・外側のラムダの引数）を捕捉してはならない。捕捉と、ラムダ
リテラルでない関数値は検査時のエラー E098 である。参照できるのはトップレベルの
`fn`・`let`・`var` と import したモジュールだけである。`test` ブロックの外
（`fn` や `main`）での呼び出し、関数値としての使用、別の `assert_aborts` の
closure の中での呼び出しは検査時のエラー E099 である。

子の中で `f` は**プログラム本体として**降ろされる。`assert` 系と `testing.assert_*`
は ALS-T18 のブロック形で停止し、`panic` は C-219 の `PANIC: <msg>` で停止する。

子が**定義された abort** で終わったとき、すなわち exit status が 1 で、stderr に
`Error: ` または `PANIC: ` で始まる行（abort の見出し）があるときだけ、abort と
数える。**abort テキスト**は、最後の見出し行から stderr の末尾までである。
`assert_aborts` は abort テキストが `s` を部分文字列として含む（バイト単位、大文字
小文字を区別）ときに合格する。それより前の `eprintln` は照合に使わない。`panic`・
範囲外の添字・ゼロ除算・スタック溢れ・失敗した assert のどれも abort である。

普通に戻った場合、ほかの exit status（`process.exit(n)` で n ≠ 1、生の panic、
trap）、見出しのない exit 1 は不合格である。不合格は ALS-T18 の項目順のブロックで
報告する: `Error: assertion failed`、`at: line <N>`、
`expected: abort containing "<s>"`、`found: <理由>`。`<理由>` は
`returned normally`、`abort "<abort テキスト>"`、`exit code <n>`、
`exit code 101 (raw panic)`、`trap (exit 134)` のいずれかである。子が何か出力して
いれば、そのあとに `child stdout:` と `child stderr:` の節が続く。子の出力は
この報告にだけ現れ、合格時には転送しない。

テストは各呼び出しのあとも続き、一つのテストの複数の呼び出しはそれぞれ自分の
子を持つ。

テスト: `spec/stdlib/coverage_assert_aborts_test.almd`、
`spec/stdlib/testing_polarity_test.almd`、
`spec/lang/assert_aborts_continues_test.almd`。Contracts: C-384。

## ALS-T31 HMAC と定数時間比較

`hash.hmac_sha256(key, b)`・`hash.hmac_sha512(key, b)` は SHA-256・SHA-512 上の
HMAC（RFC 2104）で、**鍵が第一引数**である。ブロック長（64・128 バイト）より長い
鍵は先にハッシュし、短い鍵はブロック長まで 0 で埋める。結果は 32・64 バイトで、
`_hex` は同じ値の小文字 16 進（64・128 文字）。RFC 4231 のテストケース 1〜7 に
両ダイジェストで一致する。`hash.is_valid_hmac_sha256(key, b, tag)`・
`is_valid_hmac_sha512` は `tag` が計算した MAC と等しいときだけ true で、比較は
`bytes.is_equal_constant_time` を通る。長さの違う tag は false。16 進で届く
tag は `hex.decode` で戻してから渡す（大小どちらの 16 進も受ける）。
`hex.decode(sig) ?? bytes.from_list([])` は奇数長・16 進でない文字・空文字列を
空の tag にし、検証は abort せず false を返す。

`bytes.is_equal_constant_time(a, b)`・`string.is_equal_constant_time(a, b)` は
長さとバイト列が等しいときだけ true（String は UTF-8 のバイトを比べる）。長さ
の違いは直ちに false（長さは公開情報）。長さが等しければ全バイト対を読み、
`a[i] xor b[i]` の OR を走査後に一度だけ 0 と比べる。内容に依存する分岐・添字・
脱出はない。

テスト: `spec/wasm_cross/hash_hmac_rfc4231.almd`、
`spec/wasm_cross/bytes_is_equal_constant_time.almd`。Contracts: C-422, C-423。
