<!-- description: Stdlib redesign L2/L3 — float: move conversions to their source, pair to_bits with from_bits, add the IEEE constants, trunc, is_finite and pow -->
# L2/L3: float

> Last updated: 2026-10-21
> 前提: [L0](L0-conventions.md)・[L1](L1-categories.md)
> 関連: [L2-sized-numbers](L2-sized-numbers.md)（変換の行列と float32/float64）・[L2-math](L2-math.md)・[L2-int](L2-int.md)
> 出典: Rust https://doc.rust-lang.org/std/primitive.f64.html (F) ・ Go https://pkg.go.dev/math ・ Python https://docs.python.org/3/library/math.html ・ TS/JS https://developer.mozilla.org/en-US/docs/Web/JavaScript/Reference/Global_Objects/Number ・ Gleam https://gleam-stdlib.hexdocs.pm/gleam/float.html (F) ・ Elixir https://hexdocs.pm/elixir/Float.html ・ OCaml https://ocaml.org/manual/latest/api/Float.html (F) ・ Kotlin https://kotlinlang.org/api/core/kotlin-stdlib/kotlin/-double/ ・ Swift https://developer.apple.com/documentation/swift/double

(F) = 2026-10-21 に公式ページを取り寄せて綴りを確かめたセル（Rust f64 の定数とメソッド、Gleam float と OCaml Float の関数の一覧）。

実測（`almide 0.68.0 dev 437b8f127`、native と wasm で同じ）: `1.0 / 0.0` は `inf`、`float.min(0.0 / 0.0, 1.0)` は `1.0`（NaN を無視）、`float.sign(0.0)` は `1.0`、`float.to_int8(0.0 / 0.0)` は `0`。無限大や NaN を名前で書く方法はなく、`1.0 / 0.0` か `float.parse("inf")!` と書くしかありません。

## L2: 役割の比較

| 役割 | Almide | Rust | Go | Python | TS/JS | Gleam | Elixir | OCaml | Kotlin | Swift |
|---|---|---|---|---|---|---|---|---|---|---|
| 文字列へ・から | `to_string` `parse` | `to_string` `parse` | `FormatFloat` `ParseFloat` | `str` `float` | `toString` `parseFloat` | `to_string` `parse` (F) | `to_string` `parse` | `to_string` `of_string_opt` (F) | `toString` `toDoubleOrNull` | `String` `Double(s)` |
| 小数点以下の桁を決めて文字列へ | `to_fixed` | `format!("{:.2}")` | `FormatFloat(f,'f',2,64)` | `f"{x:.2f}"` | `toFixed` | `to_precision`（Float）(F) | `Float.round` | `Printf` | `"%.2f".format` | `String(format:)` |
| Int へ（切り捨て） | `to_int` | `as i64` | `int64(f)` | `int(f)` | `Math.trunc` | `truncate` (F) | `trunc` | `to_int` (F) | `toLong` | `Int(f)` |
| Int から | `from_int`（`int.to_float` と双子） | `as f64` | `float64(n)` | `float(n)` | — | `int.to_float` (F) | — | `of_int` (F) | `toDouble` | `Double(n)` |
| ビット列との変換 | `to_bits` と `int.bits_to_float` | `to_bits` `from_bits` (F) | `Float64bits` `Float64frombits` | `struct` | `DataView` | — | — | `Int64.bits_of_float` | `toBits` `fromBits` | `bitPattern` `init(bitPattern:)` |
| 丸め（床・天井・四捨五入） | `floor` `ceil` `round` | `floor` `ceil` `round` (F) | `Floor` `Ceil` `Round` | `math.floor` 等 | `Math.*` | `floor` `ceiling` `round` (F) | `floor` `ceil` `round` | `floor` `ceil` `round` (F) | `floor` `ceil` `round` | `rounded(_:)` |
| 0 方向への丸め（Float のまま） | — | `trunc` (F) | `Trunc` | `math.trunc` | `Math.trunc` | `truncate`（Int を返す）(F) | — | `trunc` (F) | `truncate` | `rounded(.towardZero)` |
| 絶対値・最小・最大・範囲 | `abs` `min` `max` `clamp`（`math.fmin`/`fmax` が双子） | `abs` `min` `max` `clamp` (F) | `Abs` `min` `max` | `abs` `min` `max` | `Math.*` | `absolute_value` `min` `max` `clamp` (F) | `abs` `min` `max` | `abs` `min` `max` (F) | `absoluteValue` `coerceIn` | `abs` `min` `max` |
| 平方根・累乗 | `sqrt`（`math.sqrt` が双子）、累乗は `math.fpow` | `sqrt` `powf` (F) | `Sqrt` `Pow` | `math.sqrt` `math.pow` | `Math.sqrt` `Math.pow` | `square_root` `power` (F) | `:math.sqrt` `:math.pow` | `sqrt` `pow` (F) | `sqrt` `pow` | `squareRoot()` `pow` |
| 符号 | `sign` | `signum` (F) | — | `math.copysign` | `Math.sign` | — | — | `sign_bit` (F) | `sign` | `sign` |
| NaN・無限の判定 | `is_nan` `is_infinite` | `is_nan` `is_infinite` `is_finite` (F) | `IsNaN` `IsInf` | `isnan` `isinf` `isfinite` | `isNaN` `isFinite` | — | — | `is_nan` `is_infinite` `is_finite` (F) | `isNaN` `isInfinite` `isFinite` | `isNaN` `isInfinite` `isFinite` |
| 無限大・NaN・機械イプシロン・最大値 | — | `INFINITY` `NAN` `EPSILON` `MAX` `MIN` (F) | `Inf` `NaN` `MaxFloat64` | `math.inf` `math.nan` `sys.float_info` | `Infinity` `NaN` `EPSILON` `MAX_VALUE` | — | — | `infinity` `nan` `epsilon` `max_float` (F) | `POSITIVE_INFINITY` `NaN` `MAX_VALUE` | `.infinity` `.nan` `.ulpOfOne` `.greatestFiniteMagnitude` |
| 固定幅の型との変換 | `to_<型>`・`_checked`・`_saturating` と `from_float32`/`from_float64` | `as` | `T(x)` | — | — | — | — | — | `toInt()` 等 | `init(exactly:)` 等 |

### 完全性の規則

1. **変換の行列**は [L2-sized-numbers](L2-sized-numbers.md) に従います。`float` に置くのは「`Float` から出ていく」変換だけで、`Float` へ入る変換は元の型の `to_float` です（R-L0-07）。`Float` から整数への変換は損失があるので `to_T` と `to_T_checked` を置きます。`to_T` そのものが Rust の `as` と同じく 0 方向に切り捨てて飽和する（NaN は 0）ので、`to_T_saturating` は置きません（R-L0-01: 別の公開関数を 1 回呼ぶだけの関数は同義語。R-sized-04）。`to_int` もこの規則に入ります。
2. **浮動小数点の運び手**（`float`、`float32`、`float64`）は、どれも同じ 7 セルを持ちます: `to_bits`、`from_bits`、`infinity`、`nan`、`epsilon`、`max_value`、`min_value`。`min_value` は Rust の `MIN` と同じ「最も小さい有限の値（= `-max_value()`）」です。`Kotlin`/`JS` の `MIN_VALUE`（最小の正の数）とは違うことを、doc の 1 行目に書きます。
3. **型の双子は置かない**: `sqrt`・`min`・`max`・`pow` は `float` だけに置きます（[R-math-01](L2-math.md)）。
4. **順序のある型は `compare` を持つ**（R-L0-16）: `float.compare(a, b) -> Int`（負・0・正）。順序は IEEE 754 の totalOrder（`list.sort` と同じ。`-0.0 < 0.0`、NaN は正の無限大より大きい）で、`<` 演算子（NaN との比較は false）とは NaN と `-0.0` の扱いだけが違います。追加は [R-sized-02](L2-sized-numbers.md) の 1 セルです。
5. 定数は引数なしの関数（`float.infinity()`）にします。いまの `int.min_value()` と `math.pi()` と同じ形です。

### L2 の判定

- 抜けている役割: 0 方向への丸め `trunc`（8/9）、`is_finite`（7/9）、無限大・NaN・イプシロン・最大値の定数（7/9）→ R-float-03、R-float-04。累乗は `math.fpow` から移す → R-math-01。
- 重複: `float.from_int` と `int.to_float`（R-L0-07 の 3）、`float.from_float32`/`from_float64` と（追加する）`float32.to_float`/`float64.to_float` → R-float-01。`int.bits_to_float` は `float.to_bits` の逆向きなのに `int` にある → R-float-02。
- family の穴: `float.to_int_checked` がない（`to_int8` などにはある）→ [R-sized-01](L2-sized-numbers.md)。`compare` がない → [R-sized-02](L2-sized-numbers.md)。
- 同義語: `float.to_int8_saturating` など 8 関数は `float.to_int8` などを 1 回呼ぶだけ（`tests/stdlib_synonym_gate_test.rs:47-59` が許容リストで通している）→ [R-sized-04](L2-sized-numbers.md) で撤去としましたが、R-L0-32 で無印が abort になるので取り消しました。

## 案

### R-float-01 `float.from_int`・`from_float32`・`from_float64` を撤去する — ○ 採用

- 内容: 撤去 3 つ。代わりは `int.to_float`（既存）、`float32.to_float`、`float64.to_float`（R-sized-01 で追加）。
- 根拠の L0 規則: R-L0-07 の 1 と 3（L0 が `float.from_int` を名指しで撤去と決めています）、R-L0-01。
- (a) Gleam は `int.to_float` だけを持ち、`float.from_int` はありません (F)。OCaml は `Float.of_int` で先の型の側ですが、多数派（5/9）は元の型の側です。
- (b) 同じ intrinsic です。両 target で変わりません。
- (c) `int.to_float` と `float.from_int` の両方が spec に出てくるので、LLM はどちらでも書きますが、片方が消えるとヒントで直せます。
- (d) 撤去。`@deprecated(since=15, use="int.to_float")` など、removal epoch 16。`almide fix`: `float.from_int(x)` → `int.to_float(x)`、`float.from_float32(x)` → `float32.to_float(x)`、`float.from_float64(x)` → `float64.to_float(x)`。使用数: `from_int` 24/11/11/0（+26 research）、`from_float32` 4/2/6/29、`from_float64` 7/0/6/23（stdlib の分は float32/float64 モジュールの本体）。llms.txt:420 を書き換えます。
- ゲート案: R-sized-01 のゲート（浮動小数点の運び手を行列に入れる）。`float.from_<T>` が存在しないことも検査します。
- issue: #3935

### R-float-02 ビット列の変換を `to_bits` / `from_bits` の対にそろえる — ○ 採用

- 内容: `float.from_bits(bits: Int) -> Float` を足し、`int.bits_to_float` を撤去します。`int.bits_to_f32`（下位 32 bit を binary32 として読み、Float に広げる）は撤去し、`float32.from_bits(bits) |> float32.to_float` に置き換えます（`float32.from_bits` は R-sized-03）。
- 根拠の L0 規則: R-L0-07 の 2（表現から値を組み立てるのは先の型のモジュールの `from_<元>`）、R-L0-01。
- (a) `from_bits` は Rust (F)、Kotlin `Double.fromBits`、Swift `init(bitPattern:)`、Go `Float64frombits` と、どれも Float の側にあります。
- (b) 同じ intrinsic（`almide_rt_int_bits_to_float`）を呼ぶので両 target で同じです。
- (c) `float.to_bits` を見た LLM は `float.from_bits` を書きます。いまはそれが未定義で、`int.bits_to_float` は名前から探せません。
- (d) 追加 1、撤去 2。`@deprecated(since=15, use="float.from_bits")`、removal epoch 16。`almide fix`: `int.bits_to_float(b)` → `float.from_bits(b)`、`int.bits_to_f32(b)` → `float32.to_float(float32.from_bits(b))`。使用数: `bits_to_float` 26/10/8/3（+8 research）、`bits_to_f32` 5/3/5/1（llms.txt:388 にあり）、`to_bits` 30/4/20/8。
- ゲート案: 浮動小数点の運び手の 7 セル（完全性の規則 2）を R-sized-01 のゲートに入れます。`from_bits(to_bits(x)) == x` を NaN 以外の境界値で検査する spec。
- issue: #3910

### R-float-03 IEEE の定数 `infinity`・`nan`・`epsilon`・`max_value`・`min_value` を足す — ○ 採用

- 内容: `float.infinity() -> Float`、`float.nan() -> Float`（正準の quiet NaN、`to_bits` で `float.to_bits` と同じ値）、`float.epsilon() -> Float`（2^-52）、`float.max_value() -> Float`、`float.min_value() -> Float`（= `-max_value()`）。`float32`・`float64` にも同じセル（R-sized-03）。
- 根拠の L0 規則: L1 の追加基準（7/9 が名前で持つ）、R-L0-02（返す型のモジュール）、R-L0-08 の行列の考え方（整数の運び手はすべて `min_value`/`max_value` を持つので、浮動小数点の運び手にもそろえる）。
- (a) Rust・Go・Python・JS・OCaml・Kotlin・Swift の 7 言語が持っています。Gleam と Elixir は持ちません（Elixir は無限大を表せません）。`min_value` の意味は言語で割れています（Rust `MIN` は最も小さい有限値、Kotlin・JS の `MIN_VALUE` は最小の正の数）。Almide は整数の `min_value` と同じ「いちばん小さい値」にそろえます。
- (b) 純粋な定数です。NaN のビット列は `float.to_bits` がすでに正準化しているので、両 target で同じです（ADR-0016）。
- (c) いまは `1.0 / 0.0` と書くしかなく、LLM は `float.infinity` や `math.inf` を書いて未定義になります。
- (d) 追加。使用数 0。
- ゲート案: R-sized-01 のゲートで 7 セルの存在を検査し、`spec/stdlib/float_consts_test.almd` で値（`is_infinite(infinity())`、`is_nan(nan())`、`1.0 + epsilon() > 1.0`）を固定します。
- issue: #3910

### R-float-04 `trunc` と `is_finite` を足す — ○ 採用

- 内容: `float.trunc(n: Float) -> Float`（0 方向への丸め。`-0.5` は `-0.0`）、`float.is_finite(n: Float) -> Bool`（NaN と ±inf で false）。
- 根拠の L0 規則: L1 の追加基準（`trunc` 8/9、`is_finite` 7/9）、R-L0-09（Bool を返すので `is_`）。
- (a) 綴りは `trunc` が Rust・Go・Python・JS・OCaml、`is_finite` が Rust・OCaml（区切りなしの `isFinite` が JS・Kotlin・Swift）。
- (b) 純粋な計算です。wasm には `f64.trunc` 命令があります。
- (c) `float.floor` で負の数を切り捨てて 1 ずれる誤りの逃げ道になります。`is_finite` は `not is_nan(x) and not is_infinite(x)` を毎回書く手間を除きます。
- (d) 追加。使用数 0。
- ゲート案: spec に負の数・`-0.0`・NaN・inf の行を置きます。
- issue: #3911

### R-float-05 いまの `float.sign` を `float.signum` に改名する — ○ 採用

- いま: `fn float.sign(n: Float) -> Float` は Rust の `signum` と同じ動きで、`sign(0.0) = 1.0`、`sign(-0.0) = -1.0` です（契約 C-049）。一方、`int.sign(0)` と `bigint.sign` は 0 を返します。同じ `sign` という名前で、0 の扱いが型によって違います。
- 案: いまの動きのまま `float.signum` に改名し、`float.sign` を撤去します。動きは変えないので、契約 C-049 は関数名だけを直します。
- 根拠の L0 規則: R-L0-00 の 1（同じ動詞がモジュールによって違う意味）、R-L0-14.4（同じ名前のまま意味を変えない。新しい名前に移す）。
- (a) 0 を 0 として返すのは JS `Math.sign` (F)・Kotlin `sign` (F)。1.0 を返すのは Rust `f64::signum` だけで、その綴りが `signum` です。
- (c) `float.sign(dx) * speed`（dx が 0 のとき止まるつもり）が黙って違う値になる誤りを防げます。`signum` は Rust の綴りなので、動きの違いが名前に表れます。
- (d) 改名。`@deprecated(since=15, use="float.signum")`、epoch 16 で撤去、`almide fix` で 1 対 1。als の C-049 の文の関数名を先に直します。
- 完全性の規則: `sign` という名前は、すべての数の型で「0 は 0」を返します。
- issue: #3972

### R-float-06 `float.min`/`max` の NaN の扱いを `list.min`/`max` とそろえる — 保留

- いま: `float.max(NaN, 1.0) = 1.0`（NaN を無視、C-049）、`list.max([NaN, 1.0]) = NaN`（totalOrder）。
- 保留の理由: NaN を伝える言語（Go・JS・Kotlin・OCaml）と無視する言語（Rust・Swift）のどちらも 5 言語に届かず、契約 C-049・C-306 で決めた動きを変える話です。同じ名前で NaN の入力だけ結果を変えることは R-L0-14.4 が許さないので、変えるなら新しい名前が要ります。判定の材料として、NaN を含む `fold(…, float.max)` と `list.max` の食い違いを LLM がどれだけ踏むかを、almide-dojo の測定（[almide-dojo#63](https://github.com/almide/almide-dojo/issues/63) と同じ枠）で見ます。
- issue: なし（測定の結果で起票します）

### R-float-07 `round` と `to_fixed` の同点の丸めの違いを文書とゲートで固定する — ○ 採用（関数の面は変えない）

- いま: `float.round(2.5) = 3.0`（0 から遠い側）、`float.to_fixed(2.5, 0) = "2"`（2 進の正確な値の最近接、同点は偶数）。JS の `toFixed` は `"3"` です。
- 案: 各関数の doc の 1 行目に丸めの規則を書き、CHEATSHEET の float の行に 1 行足します。`spec/stdlib` に `0.5`・`1.5`・`2.5`・`-0.5`・`0.125` を `round` と `to_fixed` の両方で固定します。
- (d) 文書とテストだけです。
- issue: #3972

### R-float-08 Float の床方向の剰余 `float.mod` を足す — × 不採用

- 理由: 床方向の剰余を Float で持つのは Python の `%`、Kotlin の `mod`、Gleam の `float.modulo` の 3 言語で、5 言語の基準に届きません。Go・JS・OCaml・Swift・Elixir は 0 方向の剰余です。
- issue: なし

### R-float-09 NaN は 1 つの値にそろえ、順序の約束を正確に書く — ○ 採用

- いま: 共通の約束は「順序は IEEE 754 の totalOrder（NaN は正の無限大より大きい）」としていました。totalOrder では、符号ビットの立った NaN は負の無限大より前に来ます。実測（`almide 0.68.0 dev 437b8f127`、native と wasm で同じ）:
  - バイト列から読んだ負の NaN（`bytes.read_f64_le` に `00 00 00 00 00 00 f8 ff`）と `0.0 / 0.0` を `list.sort` すると、片方が先頭、片方が末尾に並びました。
  - `float.to_bits` と `bytes.append_f64_le` は NaN を正の 1 つの値（`0x7ff8000000000000`）にして書き出すので、負の NaN のビットは往復しません。
  - `0.0 / 0.0` の符号ビットは CPU によって違います（x86 は負、ARM は正）。この Mac（ARM）では正でした。x86 の native ではまだ測っていません。
- 案: NaN を 1 つの値（正の quiet NaN、`0x7ff8000000000000`）にそろえます。演算の結果、`bytes.read_f*`・`get_f*`・`next_f*`、`float.from_bits` で NaN ができたときに、この値にします。
  - 順序の約束は「`-0.0 < 0.0`、NaN は正の無限大より大きく、NaN どうしは等しい」で、NaN が 1 つなので正確に成り立ちます。
  - NaN のビットは往復しません（どの NaN を読んでも、書き出すと `0x7ff8000000000000` です）。これを約束に書きます。
  - 数学関数の約束（「NaN は 1 つの決まった値」、ADR-0016）と同じ扱いになります。
- 根拠: R-L0-00（同じ値の扱いを 1 つに）、ADR-0016。
- (a) JavaScript の NaN は観測できる値が 1 つです（`Float64Array` で読むと正規化されない処理系もあります）。Rust の `total_cmp` は符号を区別します。Almide は両 target とどの CPU でも同じ結果を約束するので、1 つにそろえる側を選びます。
- (b) 両 target とどの CPU でも同じ順序になります。
- (d) 意味変更（負の NaN と NaN の並び順が変わる）。NaN の符号を見るコードは標準の面にありません。
- issue: #3990

## L3: 関数表

`float` の現行 46 関数と、追加する 11 関数です（`compare` を含む）。
| 関数 | シグネチャ(現行, mut 表示) | 他言語の主な綴り | 判定 | 案 |
|---|---|---|---|---|
| `to_string` | `float.to_string(n: Float) -> String` | Rust `to_string` / Go `strconv.FormatInt` / Python `str` / JS `toString` / Gleam `to_string` (F) / Elixir `to_string` (F) / OCaml `to_string` (F) / Kotlin `toString` / Swift `String(x)` | 据え置き | — |
| `to_int` | `float.to_int(n: Float) -> Int` | Rust `as`（飽和） / Kotlin `toInt()`（飽和）(F) / Swift `init(_:)`（trap） / Go `T(x)` / Gleam `truncate` (F) / OCaml `to_int` (F) | 意味変更（範囲外・NaN は abort） | R-L0-32 |
| `from_int` | `float.from_int(n: Int) -> Float` | OCaml `Float.of_int` (F) / Gleam は `int.to_float` (F) / Kotlin `toDouble` | 撤去 | R-float-01（`int.to_float` と同じ。R-L0-07 の 3） |
| `parse` | `float.parse(s: String) -> Result[Float, String]` | Rust `parse` / Go `ParseFloat` / Gleam `parse` (F) / OCaml `of_string_opt` (F) / Kotlin `toDoubleOrNull` / Swift `Double(s)` | 据え置き | — |
| `to_fixed` | `float.to_fixed(n: Float, decimals: Int) -> String` | JS `toFixed` / Gleam `to_precision`（Float を返す）(F) / Kotlin `"%.2f".format` / Rust `format!("{:.2}")` | 据え置き | — |
| `to_bits` | `float.to_bits(f: Float) -> Int` | Rust `to_bits` (F) / Go `math.Float64bits` / Kotlin `toBits` / Swift `bitPattern` | 据え置き | R-float-02（`from_bits` と対にする） |
| `sqrt` | `float.sqrt(n: Float) -> Float` | Rust `sqrt` (F) / Go `math.Sqrt` / Python `math.sqrt` / JS `Math.sqrt` / Gleam `square_root` (F) / OCaml `sqrt` (F) / Kotlin `sqrt` / Swift `squareRoot()` | 撤去 → `math.sqrt` | R-math-03 |
| `abs` | `float.abs(n: Float) -> Float` | Rust `abs` (F) / Go `math.Abs` / Python `abs` / JS `Math.abs` / Gleam `absolute_value` (F) / OCaml `abs` (F) / Kotlin `absoluteValue` / Swift `abs` | 据え置き | — |
| `floor` | `float.floor(n: Float) -> Float` | Rust `floor` (F) / Go `math.Floor` / JS `Math.floor` / Gleam `floor` (F) / OCaml `floor` (F) / Kotlin `floor` / Swift `rounded(.down)` | 据え置き | — |
| `ceil` | `float.ceil(n: Float) -> Float` | Rust `ceil` (F) / Go `math.Ceil` / JS `Math.ceil` / Gleam `ceiling` (F) / OCaml `ceil` (F) / Kotlin `ceil` / Swift `rounded(.up)` | 据え置き | — |
| `round` | `float.round(n: Float) -> Float` | Rust `round`（0 から遠い側）(F) / Go `math.Round` / JS `Math.round`（+∞ 側） / Gleam `round` (F) / OCaml `round` (F) / Kotlin `round`（偶数側） / Swift `rounded()` | 据え置き | — |
| `min` | `float.min(a: Float, b: Float) -> Float` | Rust `min` (F) / Go `min` / JS `Math.min` / Gleam `min` (F) / OCaml `min` (F) / Kotlin `minOf` / Swift `min` | 据え置き | R-math-01（`math.fmin` の残る側） |
| `max` | `float.max(a: Float, b: Float) -> Float` | Rust `max` (F) / Go `max` / JS `Math.max` / Gleam `max` (F) / OCaml `max` (F) / Kotlin `maxOf` / Swift `max` | 据え置き | R-math-01（`math.fmax` の残る側） |
| `clamp` | `float.clamp(n: Float, lo: Float, hi: Float) -> Float` | Rust `clamp` (F) / Gleam `clamp` (F) / Kotlin `coerceIn` | 据え置き | — |
| `sign` | `float.sign(n: Float) -> Float` | Rust `signum` (F) / JS `Math.sign` / Kotlin `sign` / OCaml `sign_bit` (F)（Bool） | 改名 → `signum`（動きは変えない） | R-float-05 |
| `is_nan` | `float.is_nan(n: Float) -> Bool` | Rust `is_nan` (F) / Go `math.IsNaN` / Python `math.isnan` / JS `Number.isNaN` / OCaml `is_nan` (F) / Kotlin `isNaN` / Swift `isNaN` | 据え置き | — |
| `is_infinite` | `float.is_infinite(n: Float) -> Bool` | Rust `is_infinite` (F) / Go `math.IsInf` / Python `math.isinf` / OCaml `is_infinite` (F) / Kotlin `isInfinite` / Swift `isInfinite` | 据え置き | — |
| `to_int8` | `float.to_int8(n: Float) -> Int8` | Rust `as`（飽和） / Kotlin `toInt()`（飽和）(F) / Swift `init(_:)`（trap） / Go `T(x)` | 意味変更（範囲外・NaN は abort） | R-L0-32 |
| `to_int16` | `float.to_int16(n: Float) -> Int16` | Rust `as`（飽和） / Kotlin `toInt()`（飽和）(F) / Swift `init(_:)`（trap） / Go `T(x)` | 意味変更（範囲外・NaN は abort） | R-L0-32 |
| `to_int32` | `float.to_int32(n: Float) -> Int32` | Rust `as`（飽和） / Kotlin `toInt()`（飽和）(F) / Swift `init(_:)`（trap） / Go `T(x)` | 意味変更（範囲外・NaN は abort） | R-L0-32 |
| `to_uint8` | `float.to_uint8(n: Float) -> UInt8` | Rust `as`（飽和） / Kotlin `toInt()`（飽和）(F) / Swift `init(_:)`（trap） / Go `T(x)` | 意味変更（範囲外・NaN は abort） | R-L0-32 |
| `to_uint16` | `float.to_uint16(n: Float) -> UInt16` | Rust `as`（飽和） / Kotlin `toInt()`（飽和）(F) / Swift `init(_:)`（trap） / Go `T(x)` | 意味変更（範囲外・NaN は abort） | R-L0-32 |
| `to_uint32` | `float.to_uint32(n: Float) -> UInt32` | Rust `as`（飽和） / Kotlin `toInt()`（飽和）(F) / Swift `init(_:)`（trap） / Go `T(x)` | 意味変更（範囲外・NaN は abort） | R-L0-32 |
| `to_uint64` | `float.to_uint64(n: Float) -> UInt64` | Rust `as`（飽和） / Kotlin `toInt()`（飽和）(F) / Swift `init(_:)`（trap） / Go `T(x)` | 意味変更（範囲外・NaN は abort） | R-L0-32 |
| `to_float32` | `float.to_float32(n: Float) -> Float32` | Rust `as` / Kotlin `toFloat()` / Swift `init(_:)` | 据え置き | R-sized-01（行列のセル） |
| `to_int64` | `float.to_int64(n: Float) -> Int64` | Rust `as`（飽和） / Kotlin `toInt()`（飽和）(F) / Swift `init(_:)`（trap） / Go `T(x)` | 意味変更（範囲外・NaN は abort） | R-L0-32 |
| `to_float64` | `float.to_float64(n: Float) -> Float64` | Rust `as` / Kotlin `toFloat()` / Swift `init(_:)` | 据え置き | R-sized-01（行列のセル） |
| `from_float32` | `float.from_float32(n: Float32) -> Float` | Rust `as` / Kotlin `toFloat()` / Swift `init(_:)` | 撤去 | R-float-01（`float32.to_float`） |
| `from_float64` | `float.from_float64(n: Float64) -> Float` | Rust `as` / Kotlin `toFloat()` / Swift `init(_:)` | 撤去 | R-float-01（`float64.to_float`） |
| `to_int8_checked` | `float.to_int8_checked(n: Float) -> Option[Int8]` | Rust `try_from` / Swift `init(exactly:)` (F) / OCaml Base `to_int_exn`系 / 他 6 言語 — | 据え置き | R-sized-01（行列のセル） |
| `to_int16_checked` | `float.to_int16_checked(n: Float) -> Option[Int16]` | Rust `try_from` / Swift `init(exactly:)` (F) / OCaml Base `to_int_exn`系 / 他 6 言語 — | 据え置き | R-sized-01（行列のセル） |
| `to_int32_checked` | `float.to_int32_checked(n: Float) -> Option[Int32]` | Rust `try_from` / Swift `init(exactly:)` (F) / OCaml Base `to_int_exn`系 / 他 6 言語 — | 据え置き | R-sized-01（行列のセル） |
| `to_int64_checked` | `float.to_int64_checked(n: Float) -> Option[Int64]` | Rust `try_from` / Swift `init(exactly:)` (F) / OCaml Base `to_int_exn`系 / 他 6 言語 — | 据え置き | R-sized-01（行列のセル） |
| `to_uint8_checked` | `float.to_uint8_checked(n: Float) -> Option[UInt8]` | Rust `try_from` / Swift `init(exactly:)` (F) / OCaml Base `to_int_exn`系 / 他 6 言語 — | 据え置き | R-sized-01（行列のセル） |
| `to_uint16_checked` | `float.to_uint16_checked(n: Float) -> Option[UInt16]` | Rust `try_from` / Swift `init(exactly:)` (F) / OCaml Base `to_int_exn`系 / 他 6 言語 — | 据え置き | R-sized-01（行列のセル） |
| `to_uint32_checked` | `float.to_uint32_checked(n: Float) -> Option[UInt32]` | Rust `try_from` / Swift `init(exactly:)` (F) / OCaml Base `to_int_exn`系 / 他 6 言語 — | 据え置き | R-sized-01（行列のセル） |
| `to_uint64_checked` | `float.to_uint64_checked(n: Float) -> Option[UInt64]` | Rust `try_from` / Swift `init(exactly:)` (F) / OCaml Base `to_int_exn`系 / 他 6 言語 — | 据え置き | R-sized-01（行列のセル） |
| `to_float32_checked` | `float.to_float32_checked(n: Float) -> Option[Float32]` | Rust `try_from` / Swift `init(exactly:)` (F) / OCaml Base `to_int_exn`系 / 他 6 言語 — | 据え置き | R-sized-01（行列のセル） |
| `to_int8_saturating` | `float.to_int8_saturating(n: Float) -> Int8` | Swift `init(clamping:)` (F) / Kotlin `coerceIn` / 他 7 言語 — | 据え置き | R-L0-32（R-sized-04 の撤去を取り消し） |
| `to_int16_saturating` | `float.to_int16_saturating(n: Float) -> Int16` | Swift `init(clamping:)` (F) / Kotlin `coerceIn` / 他 7 言語 — | 据え置き | R-L0-32（R-sized-04 の撤去を取り消し） |
| `to_int32_saturating` | `float.to_int32_saturating(n: Float) -> Int32` | Swift `init(clamping:)` (F) / Kotlin `coerceIn` / 他 7 言語 — | 据え置き | R-L0-32（R-sized-04 の撤去を取り消し） |
| `to_int64_saturating` | `float.to_int64_saturating(n: Float) -> Int64` | Swift `init(clamping:)` (F) / Kotlin `coerceIn` / 他 7 言語 — | 据え置き | R-L0-32（R-sized-04 の撤去を取り消し） |
| `to_uint8_saturating` | `float.to_uint8_saturating(n: Float) -> UInt8` | Swift `init(clamping:)` (F) / Kotlin `coerceIn` / 他 7 言語 — | 据え置き | R-L0-32（R-sized-04 の撤去を取り消し） |
| `to_uint16_saturating` | `float.to_uint16_saturating(n: Float) -> UInt16` | Swift `init(clamping:)` (F) / Kotlin `coerceIn` / 他 7 言語 — | 据え置き | R-L0-32（R-sized-04 の撤去を取り消し） |
| `to_uint32_saturating` | `float.to_uint32_saturating(n: Float) -> UInt32` | Swift `init(clamping:)` (F) / Kotlin `coerceIn` / 他 7 言語 — | 据え置き | R-L0-32（R-sized-04 の撤去を取り消し） |
| `to_uint64_saturating` | `float.to_uint64_saturating(n: Float) -> UInt64` | Swift `init(clamping:)` (F) / Kotlin `coerceIn` / 他 7 言語 — | 据え置き | R-L0-32（R-sized-04 の撤去を取り消し） |
| `to_int_checked` | （追加）`float.to_int_checked(x: Float) -> Option[Int]` | Rust `try_from` / Swift `init(exactly:)` (F) / OCaml Base `to_int_exn`系 / 他 6 言語 — | 追加 | R-sized-01 |
| `from_bits` | （追加）`float.from_bits(bits: Int) -> Float` | Rust `f64::from_bits` (F) / Go `math.Float64frombits` / Kotlin `Double.fromBits` / Swift `Double(bitPattern:)` | 追加 | R-float-02 |
| `trunc` | （追加）`float.trunc(n: Float) -> Float` | Rust `trunc` (F) / Go `math.Trunc` / Python `math.trunc` / JS `Math.trunc` / OCaml `trunc` (F) / Kotlin `truncate` / Swift `rounded(.towardZero)` | 追加 | R-float-04 |
| `is_finite` | （追加）`float.is_finite(n: Float) -> Bool` | Rust `is_finite` (F) / Python `math.isfinite` / JS `Number.isFinite` / OCaml `is_finite` (F) / Kotlin `isFinite` / Swift `isFinite` | 追加 | R-float-04 |
| `pow` | （追加）`float.pow(base: Float, exp: Float) -> Float` | Rust `powf` (F) / Go `math.Pow` / Python `math.pow` / JS `Math.pow` / Gleam `power` (F) / OCaml `pow` (F) / Kotlin `pow` / Swift `pow` | 追加 | R-math-01 |
| `infinity` | （追加）`float.infinity() -> Float` | Rust `INFINITY` (F) / Go `math.Inf(1)` / Python `math.inf` / JS `Infinity` / OCaml `infinity` (F) / Kotlin `POSITIVE_INFINITY` / Swift `.infinity` | 追加 | R-float-03 |
| `nan` | （追加）`float.nan() -> Float` | Rust `NAN` (F) / Go `math.NaN()` / Python `math.nan` / JS `NaN` / OCaml `nan` (F) / Kotlin `NaN` / Swift `.nan` | 追加 | R-float-03 |
| `epsilon` | （追加）`float.epsilon() -> Float` | Rust `EPSILON` (F) / Python `sys.float_info.epsilon` / JS `Number.EPSILON` / OCaml `epsilon` (F) / Swift `.ulpOfOne` | 追加 | R-float-03 |
| `max_value` | （追加）`float.max_value() -> Float` | Rust `MAX` (F) / Go `math.MaxFloat64` / JS `Number.MAX_VALUE` / OCaml `max_float` (F) / Kotlin `MAX_VALUE` / Swift `.greatestFiniteMagnitude` | 追加 | R-float-03 |
| `min_value` | （追加）`float.min_value() -> Float` | Rust `MIN`（最も小さい有限値）(F) / Swift `-.greatestFiniteMagnitude` / ※ Kotlin・JS の `MIN_VALUE` は最小の正の数 | 追加 | R-float-03 |
| `compare` | （追加）`float.compare(a: Float, b: Float) -> Int`（IEEE totalOrder） | Rust `total_cmp` (F) / Go `cmp.Compare` / Gleam `compare` (F) / OCaml `compare` (F) / Kotlin `compareTo` / Swift `isTotallyOrdered` | 追加 | R-sized-02 |
| `to_int_saturating` | （追加）`float.to_int_saturating(x: Float) -> Int` | Rust `as` / Swift `init(clamping:)` 相当 | 追加 | R-L0-32 |
