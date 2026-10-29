<!-- description: Stdlib redesign L2/L3 — int8…uint64, float32, float64: one conversion matrix over twelve carriers, conversions on the source, parse/compare/to_string on every carrier, IEEE cells on every float carrier -->
# L2/L3: 固定幅の数（int8 int16 int32 int64 uint8 uint16 uint32 uint64 float32 float64）

> Last updated: 2026-10-21
> 前提: [L0](L0-conventions.md)・[L1](L1-categories.md)（R-L1-20: モジュールは統合しない）
> 関連: [L2-int](L2-int.md)・[L2-float](L2-float.md)
> 出典: Rust https://doc.rust-lang.org/std/primitive.i8.html と https://doc.rust-lang.org/std/convert/trait.TryFrom.html ・ Go https://go.dev/ref/spec#Conversions と https://pkg.go.dev/strconv ・ Python https://docs.python.org/3/library/struct.html ・ TS/JS https://developer.mozilla.org/en-US/docs/Web/JavaScript/Reference/Global_Objects/BigInt/asIntN ・ Gleam https://gleam-stdlib.hexdocs.pm/gleam/int.html (F) ・ Elixir https://elixir.hexdocs.pm/Integer.html (F) ・ OCaml https://ocaml.org/manual/latest/api/Int32.html ・ Kotlin https://kotlinlang.org/api/core/kotlin-stdlib/kotlin/-long/ (F) ・ Swift https://developer.apple.com/documentation/swift/binaryinteger/init(truncatingifneeded:)

(F) = 2026-10-21 に公式ページを取り寄せて確かめた綴り。Swift の `init(truncatingIfNeeded:)` / `init(clamping:)` / `init(exactly:)` と Kotlin `Double.toInt()` の飽和は [facts/naming-conventions.md](facts/naming-conventions.md) 11 節で (F) 済みです。

## L2: 役割の比較

| 役割 | Almide | Rust | Go | Python | TS/JS | Gleam | Elixir | OCaml | Kotlin | Swift |
|---|---|---|---|---|---|---|---|---|---|---|
| 固定幅の型 | 8 モジュール + `float32` `float64` | `i8`〜`u64` `f32` `f64` | `int8`〜`uint64` `float32` | —（`struct` の型コードだけ） | 型付き配列・`BigInt.asIntN` | — | — | `Int32` `Int64` | `Byte`〜`ULong` `Float` | `Int8`〜`UInt64` `Float` |
| 損失ありの変換（無印。Almide は R-L0-32 で abort、折り返しは R-L0-34 の `_wrapping`） | `to_<型>` | `as` | `T(x)` | — | `asIntN` | — | — | `of_int` | `toByte()` 等 | `init(truncatingIfNeeded:)` (F) |
| 範囲外で none | `to_<型>_checked` | `try_from` | — | — | — | — | — | Base `to_int_exn` 系 | — | `init(exactly:)` (F) |
| 範囲外は端に張り付く | `to_<型>_saturating` | 整数は —（演算の `saturating_*` だけ）、float→int の `as` | — | — | — | — | — | — | `coerceIn` | `init(clamping:)` (F) |
| 既定の `Int` / `Float` へ | `int.from_<型>`、`float.from_float32/64`（先の型の側） | `i64::from` / `as` | `int64(x)` | — | — | — | — | `Int32.to_int` | `toLong()` | `Int(x)` |
| 文字列へ | `to_string` | `to_string` | `FormatInt` | — | — | — | — | `Int32.to_string` | `toString` | `String(x)` |
| 文字列から | — | `parse::<i8>` | `ParseInt(s, 10, 8)` | — | — | — | — | `Int32.of_string_opt` | `toByteOrNull` | `Int8(s)` |
| 比較 | 演算子だけ | `cmp` | `cmp.Compare` | — | — | — | — | `Int32.compare` | `compareTo` | `<` |
| 下限・上限 | 整数は `min_value` `max_value`、浮動小数点は — | `MIN` `MAX` | `math.MaxInt8` 等 | — | — | — | — | `Int32.min_int` | `MIN_VALUE` | `.min` `.max` |
| ビット列（浮動小数点） | — | `f32::to_bits` `from_bits` | `Float32bits` | `struct` | `DataView` | — | — | `Int32.bits_of_float` | `toBits` | `bitPattern` |

### 完全性の規則（変換の行列）

運び手（carrier）は 12 個です: `Int`、`Int8`、`Int16`、`Int32`、`Int64`、`UInt8`、`UInt16`、`UInt32`、`UInt64`、`Float`、`Float32`、`Float64`。モジュールはそれぞれ `int`、`int8` … `uint64`、`float`、`float32`、`float64` です。

1. **向き**（R-L0-07）: 変換は元の型のモジュールに `to_<先>` で置きます。先の型の側の `from_<元>` は置きません（`int.from_int8`、`float.from_int`、`float.from_float32`、`float.from_float64` は撤去）。
2. **plain**: 異なる 2 つの運び手の順序対（12 × 11 = 132 対）すべてに `to_<先>` があります。
3. **`_checked`**: 元の値が先の型で正確に表せないことがある対（損失のある対）にだけ置きます。整数 → 整数は範囲が入らないとき、整数 → 浮動小数点は元の有効ビット数が仮数（Float32 は 24、Float64/Float は 53）を超えるとき、浮動小数点 → 整数は常に、Float64/Float → Float32 です。損失のない対には置きません（いつも some を返すだけの関数は雑音）。
4. **`_saturating`**: 整数 → 整数で範囲が入らない対にだけ置きます。浮動小数点 → 整数の `to_<先>` はすでに飽和する（Rust の `as`、Kotlin の `toInt()` と同じ。NaN は 0）ので、`_saturating` は置きません（R-sized-04）。整数 → 浮動小数点と浮動小数点同士は、溢れても `inf` という値があるので置きません。
5. **すべての運び手**: `to_string`、`parse`（`Result[T, String]`。範囲外は err）、`compare`（`-> Int`、浮動小数点は IEEE totalOrder）。
6. **整数の運び手**: `min_value`、`max_value`。
7. **浮動小数点の運び手**（`float`、`float32`、`float64`）: `to_bits`、`from_bits`、`infinity`、`nan`、`epsilon`、`max_value`、`min_value`。
8. **意図的に置かないもの**: 固定幅の型の算術関数（`abs`、`min`、`max`、`pow`、ビット演算など）。固定幅の型は格納と境界のための型で、演算子（`+` `-` `*` `/` `%` `<`）は使えます。それ以外の計算は `to_int` で `Int` に移してから `int` の関数を使います。

この規則で数えると、行列のセルは 52 個が足りず、8 個が余分です（下の表）。

| モジュール | 足りないセル（R-sized-01） | 余分なセル（R-sized-04） |
|---|---|---|
| `int` | `to_float_checked` `to_float64_checked` | — |
| `int8` `int16` `uint8` `uint16` | `to_int` `to_float` | — |
| `int32` `uint32` | `to_int` `to_float` `to_float32_checked` | — |
| `int64` | `to_int` `to_float` `to_float_checked` `to_float32_checked` `to_float64_checked` | — |
| `uint64` | `to_int` `to_int_checked` `to_int_saturating` `to_float` `to_float_checked` `to_float32_checked` `to_float64_checked` | — |
| `float` | `to_int_checked` | `to_int8_saturating` … `to_uint64_saturating`（8） |
| `float32` | `to_int` `to_float` と、整数 9 種への `_checked`（`to_int_checked` `to_int8_checked` … `to_uint64_checked`） | — |
| `float64` | `to_int` `to_float` `to_float32_checked` と、整数 9 種への `_checked` | — |

いまのゲート（`src/cli/docs_gen.rs` の `check_numeric_conversion_matrix`、#956）は、整数の 9 個の運び手しか見ていません。浮動小数点の 3 個が行列の外にあったことが、`float32`・`float64` に `_checked` が 1 つもない理由です。

### L2 の判定

- 向きの重複: `int.from_<型>` 10 関数と `float.from_*` 3 関数 → R-sized-01（R-int-01、R-float-01 と同じ issue）。
- family の穴: 上の表の 52 セル → R-sized-01。`parse`（10 モジュール）と `compare`（12 モジュール）→ R-sized-02。浮動小数点の運び手の 7 セル（float32・float64 で 14）→ R-sized-03。
- 同義語: `float.to_<整数>_saturating` 8 関数 → R-sized-04。
- 統合しない: R-L1-20 のとおり（R-sized-05 ×）。

## 案

### R-sized-01 変換の行列を 12 個の運び手に広げ、向きを元の型の側にそろえる — ○ 採用

- 内容: 上の表の 52 セルを足し、`int.from_int8` … `int.from_uint64`、`int.from_uint64_checked`、`int.from_uint64_saturating`、`float.from_int`、`float.from_float32`、`float.from_float64` を撤去します。意味は既存のセルと同じ規則です: plain は「整数は折り返し、浮動小数点 → 整数は 0 方向に切り捨てて飽和、NaN は 0」、`_checked` は「正確に表せないとき none」（整数 → 浮動小数点では、丸めが起きるとき none。いまの `int.to_float32_checked` と同じ）。
- 六巡目の注: 上の「plain は整数は折り返し」は採用したときの規則です。無印は R-L0-32 で「範囲外は abort」に、折り返しは R-L0-34 の `_wrapping` に変わりました。
- 根拠の L0 規則: R-L0-07、R-L0-08（行列の穴埋め）、R-L0-01。
- (a) Swift の 3 つの初期化子（`truncatingIfNeeded:`・`exactly:`・`clamping:`）が、この行列にいちばん近い形です (F)。Rust は `as`・`TryFrom`・`From` の 3 系統。ほかの 7 言語は 1〜2 種しか持たないので、多数派はありません（L0 R-L0-08 のとおり、接尾辞の行列を据え置きます）。
- (b) 純粋な計算です。新しいセルは、既存のセルの合成として `stdlib/<型>_convert.almd` に Almide で書けます（例: `float32.to_int16_checked(x) = float.to_int16_checked(float32.to_float(x))`）。intrinsic を足す必要はありません。両 target で同じ値になることは、既存の `spec/stdlib/sized_conversion_matrix_test.almd` と同じ境界値の行で確かめます。
- (c) いまは `int8.to_int16` は書けるのに `int8.to_int` は書けず、`float32.to_int8` はあるのに `float32.to_int8_checked` はありません。規則から推測した LLM の綴りが、行列の穴のぶんだけ確実に外れます。
- (d) 追加 52、撤去 13。撤去は `@deprecated(since=15, use="int8.to_int")` など、removal epoch 16。`almide fix` の書き換えは 1 対 1（`int.from_<T>(x)` → `<T>.to_int(x)`、`int.from_uint64_checked(x)` → `uint64.to_int_checked(x)`、`float.from_int(x)` → `int.to_float(x)`、`float.from_float32(x)` → `float32.to_float(x)`）。使用数（spec/tests/docs/stdlib）: `int.from_<型>` 8 関数の合計 70/38/47/362、`from_uint64_checked` 9/2/7/1、`from_uint64_saturating` 4/2/4/0、`float.from_int` 24/11/11/0、`from_float32` 4/2/6/29、`from_float64` 7/0/6/23。stdlib の数は sized モジュールの本体が自分で呼んでいるもので、移行で 0 にします。CHEATSHEET:1068 の「Widening is `int.from_<src>(x)`」を書き換えます。
- ゲート案: `check_numeric_conversion_matrix` の運び手の表に `float`・`float32`・`float64` を足し、損失の判定を「範囲」から「範囲または仮数のビット数」に広げます。`Int` 列の `from_<s>` の期待を `<s>.to_int` に置き換え、`int.from_*`・`float.from_*` が「存在してはならない」側に入ることを検査します。semantics 側は `sized_conversion_matrix_test.almd` に浮動小数点の行を足します。
- issue: #3935

### R-sized-02 すべての運び手に `parse` と `compare` を置く — ○ 採用

- 内容: `int8.parse(s: String) -> Result[Int8, String]` … `float64.parse`（10 関数。整数は `int.parse` と同じ文法で読み、範囲外は err。浮動小数点は `float.parse` と同じ文法）。`<mod>.compare(a: T, b: T) -> Int` を 12 個の運び手すべてに足します（負・0・正。浮動小数点は IEEE 754 totalOrder で、`list.sort` の順序と同じ）。
- 根拠の L0 規則: R-L0-16（順序のある型は `compare` を持つ）、R-L0-06 の 4（失敗しうる構築は Option/Result を返す）、R-L0-02（返す型のモジュールに置く）、R-L0-06（`parse` は理由のある失敗なので Result）。
- (a) 固定幅の型を持つ 5 言語（Rust、Go、OCaml、Kotlin、Swift）は、すべて型ごとに文字列から読めます。
- (b) 純粋な計算です。`int.parse` と `_checked` の合成で書けます。
- (c) `int.parse(s)! |> int.to_int8_checked` の 2 段を、1 つの関数で書けます。2 段だと「範囲外」の理由が消えます。
- (d) 追加 22。使用数 0。
- ゲート案: 行列のゲートに「すべての運び手に `to_string`・`parse`・`compare`」を足します。
- issue: #3902

### R-sized-03 `float32`・`float64` に浮動小数点の 7 セルを置く — ○ 採用

- 内容: `float32.to_bits(x: Float32) -> Int`（下位 32 bit）、`float32.from_bits(bits: Int) -> Float32`（下位 32 bit を読む）、`float32.infinity()`、`nan()`、`epsilon()`（2^-23）、`max_value()`、`min_value()`。`float64` も同じ 7 セル（`to_bits` は 64 bit）。`float` 側は [R-float-02](L2-float.md)・[R-float-03](L2-float.md)。
- 根拠の L0 規則: 完全性の規則 7（R-L0-08 の行列の考え方）、R-L0-07 の 2（ビット列から組み立てるのは先の型の `from_bits`）。
- (a) Rust・Go・Kotlin・Swift は 32 bit 浮動小数点のビット列と定数を型ごとに持っています。
- (b) 純粋な計算です。`float32.from_bits` は撤去する `int.bits_to_f32` の後継で、同じ intrinsic（下位 32 bit を binary32 として読む）を使います。
- (c) GGUF などの f32 の重みを読むコードで、`int.bits_to_f32` を名前から探せないことが今の罠です。
- (d) 追加 14。使用数 0（`int.bits_to_f32` の 5/3/5/1 が移ってきます）。
- ゲート案: R-sized-01 と同じゲートで 7 セルを検査します。
- issue: #3910

### R-sized-04 `float.to_<整数>_saturating` 8 関数を撤去する — × 取り消し（R-L0-32）

- 取り消し（五巡目）: R-L0-32 で無印の `float.to_int8` などが範囲外で abort になるので、`_saturating` は同義語でなくなり、いまの飽和の動きを持つ唯一のセルになります。8 関数は残します（`float.to_int_saturating` を足して 9 つにそろえます）。
- 内容: `float.to_int8_saturating` … `float.to_uint64_saturating` を撤去します。`float.to_int8` などがすでに同じ飽和をします。
- 根拠の L0 規則: R-L0-01（別の公開関数を 1 回呼ぶだけの公開関数も同義語に数える）。いまは `tests/stdlib_synonym_gate_test.rs:47-59` の許容リストで通しているので、この撤去で許容リストが 8 行減ります（減る方向の変更）。
- (a) Rust の `as`、Kotlin の `toInt()` は浮動小数点 → 整数で飽和する 1 つの綴りだけを持ちます。
- (c) 同じ結果の綴りが 2 つあると、「`to_int8` は飽和しない（折り返す）」と誤って読む余地が残ります。完全性の規則 4 に「浮動小数点 → 整数の plain は飽和する」と書くほうが、規則として当たります。
- (d) 撤去 8。`@deprecated(since=15, use="float.to_int8")` など、removal epoch 16。`almide fix`: `_saturating` を外すだけ。使用数: `to_int32_saturating` 5/0/3/0、`to_uint16_saturating` 4/0/3/0、ほかの 6 関数は 0/0/3/0。
- ゲート案: 行列のゲートで、浮動小数点 → 整数の `_saturating` を「存在してはならない」側に置きます。同義語ゲートの許容リストから 8 行を消します。
- issue: #3935

### R-sized-05 固定幅のモジュールをまとめる・算術関数を足す — × 不採用（算術の部分は → R-sized-10 で見直し）

- 内容: モジュールを統合しません（R-L1-20）。`int8.abs`、`uint32.max` などの算術関数も足しません（完全性の規則 8）。
- 根拠の L0 規則: R-L0-02（主語の型のモジュールに置く。Almide にはメソッドがないので、型ごとのモジュールになる）、R-L0-01（`Int` の関数と同じ計算の複製を 10 倍に増やさない）。
- (c) 固定幅の型で算術がしたいときの書き方は「`to_int` してから `int.*`」の 1 通りにします。行列の `to_int` がそろう（R-sized-01）ので、この書き方がどの型でも同じ綴りになります。
- issue: なし

### R-sized-06 整数の運び手すべてに `parse_radix`・`to_string_radix` を置く — ○ 採用

- いま: `int` と `bigint` だけが `parse_radix(s, radix)`・`to_string_radix(n, radix)` を持ちます。
- 案: `int8`〜`uint64` の 8 つに `fn <T>.parse_radix(s: String, radix: Int) -> Result[<T>, String]` と `fn <T>.to_string_radix(x: <T>, radix: Int) -> String` を足します（16 関数）。文法と radix の範囲は `int.parse_radix` と同じです。
- 根拠の L0 規則: R-sized-02 と同じ理由（2 段に分けると「範囲外」の理由が消える）。
- R-sized-05（×）への新しい証拠: 規則 8 の「`to_int` してから `int.*`」は、`uint64` の 2^63 以上で成り立ちません。実測: `int.from_hex("ffffffffffffffff")` は err、`int.max(int.from_uint64(uint64.max_value()), 1)` は `1` です。
- (a) 符号なし 64 bit を基数つきで読み書きできるのは Rust `u64::from_str_radix` (F)、Go `ParseUint`/`FormatUint`、Kotlin `toULong(radix)`、Swift `UInt64(s, radix:)`、Python `int(s, 16)` の 5 言語です。
- (c) 64 bit のハッシュを 16 進で読む処理はよく書かれ、LLM は `uint64.parse_radix(s, 16)` と書きます。いまはほかの道が黙って値を失います。
- (d) 追加 16。
- 完全性の規則: 整数の運び手（`int`・`bigint`・固定幅の 8 つ）は、すべて `parse`・`to_string`・`parse_radix`・`to_string_radix` を持ちます。
- ゲート案: 数の変換の行列ゲートの列に 2 つを足し、`uint64` の `max_value` の往復を radix 2・16・36 で確かめます。
- issue: #3972

### R-sized-07 `float32`・`float64` に `is_nan` などの判定を足す — × 不採用（→ R-sized-08 で見直し）

- 理由: `float32.to_float` は値を失わないので、規則 8 の「`to_float` してから `float.*`」が正しく動きます。判定まで広げると、`floor`・`sqrt` も足すかという線引きを繰り返します。代わりに、E002 の案内（R-L0-19）に「`float32.to_float(x) |> float.is_nan`」の行を入れます。
- issue: なし

### R-sized-08 `Integer`・`FloatingPoint` を宣言し、選択と分類の関数を全型にそろえる — ○ 採用（R-sized-07 を見直し）

- いま: 比較・選択・分類の関数が型によって欠けています。`int.min`・`max`・`clamp` はあっても `uint64.min` はありません（`UInt64` の上半分は `Int` に移せないので、`to_int` して `int.min` で代わりにすることもできません）。`float32.nan`・`infinity` はあっても `float32.is_nan` はありません。2 つの外部レビューが続けて挙げています。
- 案: R-L0-31 の 2 つのプロトコルを宣言し、欠けているセルを埋めます。
  - `Integer`（`int` と 8 型）: `compare`・`min`・`max`・`clamp`・`parse`・`parse_radix`・`to_string`・`to_string_radix`・`min_value`・`max_value`。足すのは 8 型の `min`・`max`・`clamp`（24 関数）。
  - `FloatingPoint`（`float`・`float32`・`float64`）: `compare`・`min`・`max`・`clamp`・`parse`・`to_string`・`is_nan`・`is_infinite`・`is_finite`・`nan`・`infinity`・`epsilon`・`min_value`・`max_value`。足すのは 2 型の `min`・`max`・`clamp`・`is_nan`・`is_infinite`・`is_finite`（12 関数）。
  - 線引き: 値を選ぶ・比べる・分類する関数は、型の中で答えが閉じるので型ごとに持ちます。値を計算する関数（`abs`・`floor`・`round`・`sqrt` など）は今までどおり `to_float` してから `float.*` で書きます（R-sized-07 の残りの理由。精度を上げて計算して戻しても、丸めの結果は変わりません）。
- 根拠の L0 規則: R-L0-31、R-L0-12。
- (a) Swift `BinaryInteger`（`min`・`max` は総称の関数）、Rust の整数型と `f32` はどれも `min`・`max`・`clamp`・`is_nan` を持ちます。
- (d) 追加 36。
- issue: #3989

### R-sized-09 `float32`・`float64` に `to_int_saturating` を足す — ○ 採用

- いま: R-L0-32 で `float.to_int_saturating` を足しましたが、`float32` と `float64` の `Int` 宛てだけが抜けていました（`to_int`・`to_int_checked`・`to_int64_saturating` はあります）。
- 案: `float32.to_int_saturating(x: Float32) -> Int` と `float64.to_int_saturating(x: Float64) -> Int` を足します。
- 根拠の L0 規則: R-L0-32 の完全性の規則（浮動小数点からの損失のある変換は 3 つ）。
- (d) 追加（2 関数）。
- 完全性の規則: 行列のゲートが、浮動小数点 3 型 × 整数 9 型の 27 セルすべてに無印・`_checked`・`_saturating` があることを確かめます。
- issue: #3995

### R-sized-10 整数 8 型に `Integer`・`FixedWidthInteger` の要件をそろえる — ○ 採用（R-sized-05 を見直し）

- いま: `int` は `floor_div`・`modulo`・`pow`・`is_even`・`is_odd`・ビット演算（`band`・`bor`・`bxor`・`bnot`・`bshl`・`bshr`）・`count_leading_zeros`・`count_trailing_zeros`・`count_ones`・`byte_swap`・`abs`・`sign` を持ちますが、`int8`〜`uint64` は持ちません。
- 案: [protocol の階層](protocol-view.md)の要件として、8 型すべてに置きます。
  - `Integer`: `floor_div(a, b)`・`modulo(a, b)`（`floor_div(min_value, -1)` は `/` と同じく abort、R-int-13）・`pow(base, exp: Int)`・`is_even`・`is_odd`・`band`・`bor`・`bxor`・`bnot`・`bshl(a, n: Int)`・`bshr(a, n: Int)`。`bshr` は、符号つきの型では符号を保ち、符号なしの型では 0 で埋めます（Swift の `>>` と同じ）。
  - `FixedWidthInteger`: `count_leading_zeros`・`count_trailing_zeros`・`count_ones`・`byte_swap`（どれも型の幅で数える。`int8.count_leading_zeros(1)` は 7）。
  - `SignedInteger`（`int8`〜`int64`）: `abs`・`sign`。
  - 結果が型の範囲を出たときの扱いは、その型の `+`・`*` と同じです。
- R-sized-05（×）を見直す理由: 不採用の理由は「`Int` の関数と同じ計算を 10 倍に複製しない」でした。R-L0-37 では、これらは protocol の既定の実装 1 つで、型ごとに書きません。もう 1 つの理由「`to_int` してから `int.*`」は、`uint64` の 2^63 以上で成り立ちません（R-sized-06 の実測）。
- 意図した抜け: `int` の N bit の演算（`wrap_add(a, b, bits)`・`rotate_left(a, n, bits)`・`bit_width`・`bshr_logical`）は置きません。固定幅の型では幅が型で決まり、同じ名前で形が食い違うためです（protocol-view.md の「この階層で決めること」）。
- (a) Swift の `BinaryInteger`・`FixedWidthInteger` は、全部の整数型にこの関数群を持たせます。Rust の整数型も同じです（`div_euclid`・`rem_euclid`・`pow`・`count_ones`・`leading_zeros`・`swap_bytes`）。
- (d) 追加 128（8 型 × 15、符号つき 4 型 × 2）。
- issue: #4000

### R-sized-11 `float32`・`float64` に `FloatingPoint` の要件をそろえる — ○ 採用（R-sized-08 の線引きを見直し）

- いま: `float` は `abs`・`floor`・`ceil`・`round`・`trunc`・`signum` を持ちますが、`float32`・`float64` は持ちません。R-sized-08 は「値を計算する関数は `to_float` してから `float.*`」と線を引いていました。
- 案: protocol の階層の `Signed`（`abs`）と `FloatingPoint`（`floor`・`ceil`・`round`・`trunc`・`signum`）の要件として、2 型に置きます。意味は `float` の同じ関数と同じです（`round` は 0 から遠い側に丸める）。
- R-sized-08 の線引きを見直す理由: 線引きの心配は「どこまで足すかを繰り返し決める」ことでした。protocol の要件が線になるので、繰り返しません。`sqrt` などの数学関数は `math` に残します（`FloatingPoint` の要件ではない）。
- (a) Swift の `FloatingPoint` は `rounded`・`magnitude` を、Rust の `f32` は `abs`・`floor`・`ceil`・`round`・`trunc`・`signum` を持ちます。
- (d) 追加 12。
- issue: #4000

## L3: 関数表

### L3: `int8`

現行 20 関数と、追加する 4 関数です。

| 関数 | シグネチャ(現行, mut 表示) | 他言語の主な綴り | 判定 | 案 |
|---|---|---|---|---|
| `to_int16` | `int8.to_int16(x: Int8) -> Int16` | Rust `from`/`into` / Swift `init(_:)` / Kotlin `toLong()` 等 / Go `T(x)` | 撤去（→ `int16.from`） | R-L0-40 |
| `to_int32` | `int8.to_int32(x: Int8) -> Int32` | Rust `from`/`into` / Swift `init(_:)` / Kotlin `toLong()` 等 / Go `T(x)` | 撤去（→ `int32.from`） | R-L0-40 |
| `to_int64` | `int8.to_int64(x: Int8) -> Int64` | Rust `from`/`into` / Swift `init(_:)` / Kotlin `toLong()` 等 / Go `T(x)` | 撤去（→ `int64.from`） | R-L0-40 |
| `to_uint8` | `int8.to_uint8(x: Int8) -> UInt8` | Rust `as` / Swift `init(truncatingIfNeeded:)` (F) / Kotlin `toByte()` 等 / Go `T(x)` | 撤去（→ `uint8.from`） | R-L0-40 |
| `to_uint16` | `int8.to_uint16(x: Int8) -> UInt16` | Rust `as` / Swift `init(truncatingIfNeeded:)` (F) / Kotlin `toByte()` 等 / Go `T(x)` | 撤去（→ `uint16.from`） | R-L0-40 |
| `to_uint32` | `int8.to_uint32(x: Int8) -> UInt32` | Rust `as` / Swift `init(truncatingIfNeeded:)` (F) / Kotlin `toByte()` 等 / Go `T(x)` | 撤去（→ `uint32.from`） | R-L0-40 |
| `to_uint64` | `int8.to_uint64(x: Int8) -> UInt64` | Rust `as` / Swift `init(truncatingIfNeeded:)` (F) / Kotlin `toByte()` 等 / Go `T(x)` | 撤去（→ `uint64.from`） | R-L0-40 |
| `to_float32` | `int8.to_float32(x: Int8) -> Float32` | Rust `as` / Kotlin `toDouble()` / Swift `init(_:)` / Gleam `to_float` (F) | 撤去（→ `float32.from`） | R-L0-40 |
| `to_float64` | `int8.to_float64(x: Int8) -> Float64` | Rust `as` / Kotlin `toDouble()` / Swift `init(_:)` / Gleam `to_float` (F) | 撤去（→ `float64.from`） | R-L0-40 |
| `to_string` | `int8.to_string(x: Int8) -> String` | Rust `to_string` / Kotlin `toString` / Swift `String(x)` / Go `strconv.FormatInt(int64(x),10)` | 据え置き | — |
| `to_uint8_checked` | `int8.to_uint8_checked(x: Int8) -> Option[UInt8]` | Rust `try_from` / Swift `init(exactly:)` (F) / OCaml Base `to_int_exn`系 / 他 6 言語 — | 撤去（→ `uint8.from_checked`） | R-L0-40 |
| `to_uint8_saturating` | `int8.to_uint8_saturating(x: Int8) -> UInt8` | Swift `init(clamping:)` (F) / Kotlin `coerceIn` / 他 7 言語 — | 撤去（→ `uint8.from_saturating`） | R-L0-40 |
| `to_uint16_checked` | `int8.to_uint16_checked(x: Int8) -> Option[UInt16]` | Rust `try_from` / Swift `init(exactly:)` (F) / OCaml Base `to_int_exn`系 / 他 6 言語 — | 撤去（→ `uint16.from_checked`） | R-L0-40 |
| `to_uint16_saturating` | `int8.to_uint16_saturating(x: Int8) -> UInt16` | Swift `init(clamping:)` (F) / Kotlin `coerceIn` / 他 7 言語 — | 撤去（→ `uint16.from_saturating`） | R-L0-40 |
| `to_uint32_checked` | `int8.to_uint32_checked(x: Int8) -> Option[UInt32]` | Rust `try_from` / Swift `init(exactly:)` (F) / OCaml Base `to_int_exn`系 / 他 6 言語 — | 撤去（→ `uint32.from_checked`） | R-L0-40 |
| `to_uint32_saturating` | `int8.to_uint32_saturating(x: Int8) -> UInt32` | Swift `init(clamping:)` (F) / Kotlin `coerceIn` / 他 7 言語 — | 撤去（→ `uint32.from_saturating`） | R-L0-40 |
| `to_uint64_checked` | `int8.to_uint64_checked(x: Int8) -> Option[UInt64]` | Rust `try_from` / Swift `init(exactly:)` (F) / OCaml Base `to_int_exn`系 / 他 6 言語 — | 撤去（→ `uint64.from_checked`） | R-L0-40 |
| `to_uint64_saturating` | `int8.to_uint64_saturating(x: Int8) -> UInt64` | Swift `init(clamping:)` (F) / Kotlin `coerceIn` / 他 7 言語 — | 撤去（→ `uint64.from_saturating`） | R-L0-40 |
| `min_value` | `int8.min_value() -> Int8` | Rust `MIN` / Go `math.Min…` / Kotlin `MIN_VALUE` / Swift `.min` | 据え置き | — |
| `max_value` | `int8.max_value() -> Int8` | Rust `MAX` / Go `math.Max…` / Kotlin `MAX_VALUE` / Swift `.max` | 据え置き | — |
| `parse` | （追加）`int8.parse(s: String) -> Result[Int8, String]` | Rust `str::parse::<T>` / Go `ParseInt(s,10,bits)` / OCaml `Int32.of_string_opt` / Kotlin `toByteOrNull` 等 / Swift `T(s)` | 追加 | R-sized-02 |
| `compare` | （追加）`int8.compare(a: Int8, b: Int8) -> Int` | Rust `cmp`/`total_cmp` / Go `cmp.Compare` / Kotlin `compareTo` / Swift `<` | 追加 | R-sized-02 |
| `floor_div` | （追加）`int8.floor_div(a: Int8, b: Int8) -> Int8` | Rust `div_euclid` / Swift — / Python `//` | 追加 | R-sized-10 |
| `modulo` | （追加）`int8.modulo(a: Int8, b: Int8) -> Int8` | Rust `rem_euclid` / Python `%` | 追加 | R-sized-10 |
| `pow` | （追加）`int8.pow(base: Int8, exp: Int) -> Int8` | Rust `pow` / Swift — | 追加 | R-sized-10 |
| `is_even` | （追加）`int8.is_even(n: Int8) -> Bool` | Swift `isMultiple(of: 2)` / Kotlin — | 追加 | R-sized-10 |
| `is_odd` | （追加）`int8.is_odd(n: Int8) -> Bool` | Swift `!isMultiple(of: 2)` | 追加 | R-sized-10 |
| `count_leading_zeros` | （追加）`int8.count_leading_zeros(n: Int8) -> Int` | Rust `leading_zeros` / Swift `leadingZeroBitCount` | 追加 | R-sized-10 |
| `count_trailing_zeros` | （追加）`int8.count_trailing_zeros(n: Int8) -> Int` | Rust `trailing_zeros` / Swift `trailingZeroBitCount` | 追加 | R-sized-10 |
| `count_ones` | （追加）`int8.count_ones(n: Int8) -> Int` | Rust `count_ones` / Swift `nonzeroBitCount` | 追加 | R-sized-10 |
| `byte_swap` | （追加）`int8.byte_swap(n: Int8) -> Int8` | Rust `swap_bytes` / Swift `byteSwapped` | 追加 | R-sized-10 |
| `abs` | （追加）`int8.abs(n: Int8) -> Int8` | Rust `abs` / Swift `abs` | 追加 | R-sized-10 |
| `sign` | （追加）`int8.sign(n: Int8) -> Int` | Rust `signum` / Swift `signum()` | 追加 | R-sized-10 |
| `from` | （追加）`int8.from[S: Numeric](x: S) -> Int8` | Rust `From`・`as` / Swift `init(_:)` / Python `int(x)` | 追加 | R-L0-40 |
| `from_checked` | （追加）`int8.from_checked[S: Numeric](x: S) -> Int8?` | Rust `TryFrom` / Swift `init(exactly:)` | 追加 | R-L0-40 |
| `from_saturating` | （追加）`int8.from_saturating[S: Numeric](x: S) -> Int8` | Swift `init(clamping:)` / Rust `as`（浮動小数点から） | 追加 | R-L0-40 |
| `from_wrapping` | （追加）`int8.from_wrapping[S: Integer](x: S) -> Int8` | Swift `init(truncatingIfNeeded:)` / Rust `as` | 追加 | R-L0-40 |
| `wrap_add` | （追加）`int8.wrap_add(a: Int8, b: Int8) -> Int8` | Swift `&+` / Rust `wrapping_add` | 追加 | R-int-12 |
| `wrap_sub` | （追加）`int8.wrap_sub(a: Int8, b: Int8) -> Int8` | Swift `&-` / Rust `wrapping_sub` | 追加 | R-int-12 |
| `wrap_mul` | （追加）`int8.wrap_mul(a: Int8, b: Int8) -> Int8` | Swift `&*` / Rust `wrapping_mul` | 追加 | R-int-12 |
| `rotate_left` | （追加）`int8.rotate_left(a: Int8, n: Int) -> Int8` | Rust `rotate_left` / Go `bits.RotateLeft` | 追加 | R-int-12 |
| `rotate_right` | （追加）`int8.rotate_right(a: Int8, n: Int) -> Int8` | Rust `rotate_right` | 追加 | R-int-12 |
| `bit_length` | （追加）`int8.bit_length(n: Int8) -> Int` | Python `bit_length` / Go `bits.Len` | 追加 | R-int-11 |
| `band` | （追加）`int8.band(a: Int8, b: Int8) -> Int8` | Elixir `band` / Rust・Go・Swift 演算子 `&` | 追加 | R-sized-10 |
| `bor` | （追加）`int8.bor(a: Int8, b: Int8) -> Int8` | Elixir `bor` / Rust・Go・Swift 演算子 `\|` | 追加 | R-sized-10 |
| `bxor` | （追加）`int8.bxor(a: Int8, b: Int8) -> Int8` | Elixir `bxor` / Rust・Go・Swift 演算子 `^` | 追加 | R-sized-10 |
| `bnot` | （追加）`int8.bnot(a: Int8) -> Int8` | Elixir `bnot` / Swift `~` | 追加 | R-sized-10 |
| `bshl` | （追加）`int8.bshl(a: Int8, n: Int) -> Int8` | Elixir `bsl` / 演算子 `<<` | 追加 | R-sized-10 |
| `bshr` | （追加）`int8.bshr(a: Int8, n: Int) -> Int8` | Elixir `bsr` / 演算子 `>>` | 追加 | R-sized-10 |

### L3: `int16`

現行 22 関数と、追加する 4 関数です。

| 関数 | シグネチャ(現行, mut 表示) | 他言語の主な綴り | 判定 | 案 |
|---|---|---|---|---|
| `to_int8` | `int16.to_int8(x: Int16) -> Int8` | Rust `as` / Swift `init(truncatingIfNeeded:)` (F) / Kotlin `toByte()` 等 / Go `T(x)` | 撤去（→ `int8.from`） | R-L0-40 |
| `to_int32` | `int16.to_int32(x: Int16) -> Int32` | Rust `from`/`into` / Swift `init(_:)` / Kotlin `toLong()` 等 / Go `T(x)` | 撤去（→ `int32.from`） | R-L0-40 |
| `to_int64` | `int16.to_int64(x: Int16) -> Int64` | Rust `from`/`into` / Swift `init(_:)` / Kotlin `toLong()` 等 / Go `T(x)` | 撤去（→ `int64.from`） | R-L0-40 |
| `to_uint8` | `int16.to_uint8(x: Int16) -> UInt8` | Rust `as` / Swift `init(truncatingIfNeeded:)` (F) / Kotlin `toByte()` 等 / Go `T(x)` | 撤去（→ `uint8.from`） | R-L0-40 |
| `to_uint16` | `int16.to_uint16(x: Int16) -> UInt16` | Rust `as` / Swift `init(truncatingIfNeeded:)` (F) / Kotlin `toByte()` 等 / Go `T(x)` | 撤去（→ `uint16.from`） | R-L0-40 |
| `to_uint32` | `int16.to_uint32(x: Int16) -> UInt32` | Rust `as` / Swift `init(truncatingIfNeeded:)` (F) / Kotlin `toByte()` 等 / Go `T(x)` | 撤去（→ `uint32.from`） | R-L0-40 |
| `to_uint64` | `int16.to_uint64(x: Int16) -> UInt64` | Rust `as` / Swift `init(truncatingIfNeeded:)` (F) / Kotlin `toByte()` 等 / Go `T(x)` | 撤去（→ `uint64.from`） | R-L0-40 |
| `to_float32` | `int16.to_float32(x: Int16) -> Float32` | Rust `as` / Kotlin `toDouble()` / Swift `init(_:)` / Gleam `to_float` (F) | 撤去（→ `float32.from`） | R-L0-40 |
| `to_float64` | `int16.to_float64(x: Int16) -> Float64` | Rust `as` / Kotlin `toDouble()` / Swift `init(_:)` / Gleam `to_float` (F) | 撤去（→ `float64.from`） | R-L0-40 |
| `to_string` | `int16.to_string(x: Int16) -> String` | Rust `to_string` / Kotlin `toString` / Swift `String(x)` / Go `strconv.FormatInt(int64(x),10)` | 据え置き | — |
| `to_int8_checked` | `int16.to_int8_checked(x: Int16) -> Option[Int8]` | Rust `try_from` / Swift `init(exactly:)` (F) / OCaml Base `to_int_exn`系 / 他 6 言語 — | 撤去（→ `int8.from_checked`） | R-L0-40 |
| `to_int8_saturating` | `int16.to_int8_saturating(x: Int16) -> Int8` | Swift `init(clamping:)` (F) / Kotlin `coerceIn` / 他 7 言語 — | 撤去（→ `int8.from_saturating`） | R-L0-40 |
| `to_uint8_checked` | `int16.to_uint8_checked(x: Int16) -> Option[UInt8]` | Rust `try_from` / Swift `init(exactly:)` (F) / OCaml Base `to_int_exn`系 / 他 6 言語 — | 撤去（→ `uint8.from_checked`） | R-L0-40 |
| `to_uint8_saturating` | `int16.to_uint8_saturating(x: Int16) -> UInt8` | Swift `init(clamping:)` (F) / Kotlin `coerceIn` / 他 7 言語 — | 撤去（→ `uint8.from_saturating`） | R-L0-40 |
| `to_uint16_checked` | `int16.to_uint16_checked(x: Int16) -> Option[UInt16]` | Rust `try_from` / Swift `init(exactly:)` (F) / OCaml Base `to_int_exn`系 / 他 6 言語 — | 撤去（→ `uint16.from_checked`） | R-L0-40 |
| `to_uint16_saturating` | `int16.to_uint16_saturating(x: Int16) -> UInt16` | Swift `init(clamping:)` (F) / Kotlin `coerceIn` / 他 7 言語 — | 撤去（→ `uint16.from_saturating`） | R-L0-40 |
| `to_uint32_checked` | `int16.to_uint32_checked(x: Int16) -> Option[UInt32]` | Rust `try_from` / Swift `init(exactly:)` (F) / OCaml Base `to_int_exn`系 / 他 6 言語 — | 撤去（→ `uint32.from_checked`） | R-L0-40 |
| `to_uint32_saturating` | `int16.to_uint32_saturating(x: Int16) -> UInt32` | Swift `init(clamping:)` (F) / Kotlin `coerceIn` / 他 7 言語 — | 撤去（→ `uint32.from_saturating`） | R-L0-40 |
| `to_uint64_checked` | `int16.to_uint64_checked(x: Int16) -> Option[UInt64]` | Rust `try_from` / Swift `init(exactly:)` (F) / OCaml Base `to_int_exn`系 / 他 6 言語 — | 撤去（→ `uint64.from_checked`） | R-L0-40 |
| `to_uint64_saturating` | `int16.to_uint64_saturating(x: Int16) -> UInt64` | Swift `init(clamping:)` (F) / Kotlin `coerceIn` / 他 7 言語 — | 撤去（→ `uint64.from_saturating`） | R-L0-40 |
| `min_value` | `int16.min_value() -> Int16` | Rust `MIN` / Go `math.Min…` / Kotlin `MIN_VALUE` / Swift `.min` | 据え置き | — |
| `max_value` | `int16.max_value() -> Int16` | Rust `MAX` / Go `math.Max…` / Kotlin `MAX_VALUE` / Swift `.max` | 据え置き | — |
| `parse` | （追加）`int16.parse(s: String) -> Result[Int16, String]` | Rust `str::parse::<T>` / Go `ParseInt(s,10,bits)` / OCaml `Int32.of_string_opt` / Kotlin `toByteOrNull` 等 / Swift `T(s)` | 追加 | R-sized-02 |
| `compare` | （追加）`int16.compare(a: Int16, b: Int16) -> Int` | Rust `cmp`/`total_cmp` / Go `cmp.Compare` / Kotlin `compareTo` / Swift `<` | 追加 | R-sized-02 |
| `floor_div` | （追加）`int16.floor_div(a: Int16, b: Int16) -> Int16` | Rust `div_euclid` / Swift — / Python `//` | 追加 | R-sized-10 |
| `modulo` | （追加）`int16.modulo(a: Int16, b: Int16) -> Int16` | Rust `rem_euclid` / Python `%` | 追加 | R-sized-10 |
| `pow` | （追加）`int16.pow(base: Int16, exp: Int) -> Int16` | Rust `pow` / Swift — | 追加 | R-sized-10 |
| `is_even` | （追加）`int16.is_even(n: Int16) -> Bool` | Swift `isMultiple(of: 2)` / Kotlin — | 追加 | R-sized-10 |
| `is_odd` | （追加）`int16.is_odd(n: Int16) -> Bool` | Swift `!isMultiple(of: 2)` | 追加 | R-sized-10 |
| `count_leading_zeros` | （追加）`int16.count_leading_zeros(n: Int16) -> Int` | Rust `leading_zeros` / Swift `leadingZeroBitCount` | 追加 | R-sized-10 |
| `count_trailing_zeros` | （追加）`int16.count_trailing_zeros(n: Int16) -> Int` | Rust `trailing_zeros` / Swift `trailingZeroBitCount` | 追加 | R-sized-10 |
| `count_ones` | （追加）`int16.count_ones(n: Int16) -> Int` | Rust `count_ones` / Swift `nonzeroBitCount` | 追加 | R-sized-10 |
| `byte_swap` | （追加）`int16.byte_swap(n: Int16) -> Int16` | Rust `swap_bytes` / Swift `byteSwapped` | 追加 | R-sized-10 |
| `abs` | （追加）`int16.abs(n: Int16) -> Int16` | Rust `abs` / Swift `abs` | 追加 | R-sized-10 |
| `sign` | （追加）`int16.sign(n: Int16) -> Int` | Rust `signum` / Swift `signum()` | 追加 | R-sized-10 |
| `from` | （追加）`int16.from[S: Numeric](x: S) -> Int16` | Rust `From`・`as` / Swift `init(_:)` / Python `int(x)` | 追加 | R-L0-40 |
| `from_checked` | （追加）`int16.from_checked[S: Numeric](x: S) -> Int16?` | Rust `TryFrom` / Swift `init(exactly:)` | 追加 | R-L0-40 |
| `from_saturating` | （追加）`int16.from_saturating[S: Numeric](x: S) -> Int16` | Swift `init(clamping:)` / Rust `as`（浮動小数点から） | 追加 | R-L0-40 |
| `from_wrapping` | （追加）`int16.from_wrapping[S: Integer](x: S) -> Int16` | Swift `init(truncatingIfNeeded:)` / Rust `as` | 追加 | R-L0-40 |
| `wrap_add` | （追加）`int16.wrap_add(a: Int16, b: Int16) -> Int16` | Swift `&+` / Rust `wrapping_add` | 追加 | R-int-12 |
| `wrap_sub` | （追加）`int16.wrap_sub(a: Int16, b: Int16) -> Int16` | Swift `&-` / Rust `wrapping_sub` | 追加 | R-int-12 |
| `wrap_mul` | （追加）`int16.wrap_mul(a: Int16, b: Int16) -> Int16` | Swift `&*` / Rust `wrapping_mul` | 追加 | R-int-12 |
| `rotate_left` | （追加）`int16.rotate_left(a: Int16, n: Int) -> Int16` | Rust `rotate_left` / Go `bits.RotateLeft` | 追加 | R-int-12 |
| `rotate_right` | （追加）`int16.rotate_right(a: Int16, n: Int) -> Int16` | Rust `rotate_right` | 追加 | R-int-12 |
| `bit_length` | （追加）`int16.bit_length(n: Int16) -> Int` | Python `bit_length` / Go `bits.Len` | 追加 | R-int-11 |
| `band` | （追加）`int16.band(a: Int16, b: Int16) -> Int16` | Elixir `band` / Rust・Go・Swift 演算子 `&` | 追加 | R-sized-10 |
| `bor` | （追加）`int16.bor(a: Int16, b: Int16) -> Int16` | Elixir `bor` / Rust・Go・Swift 演算子 `\|` | 追加 | R-sized-10 |
| `bxor` | （追加）`int16.bxor(a: Int16, b: Int16) -> Int16` | Elixir `bxor` / Rust・Go・Swift 演算子 `^` | 追加 | R-sized-10 |
| `bnot` | （追加）`int16.bnot(a: Int16) -> Int16` | Elixir `bnot` / Swift `~` | 追加 | R-sized-10 |
| `bshl` | （追加）`int16.bshl(a: Int16, n: Int) -> Int16` | Elixir `bsl` / 演算子 `<<` | 追加 | R-sized-10 |
| `bshr` | （追加）`int16.bshr(a: Int16, n: Int) -> Int16` | Elixir `bsr` / 演算子 `>>` | 追加 | R-sized-10 |

### L3: `int32`

現行 24 関数と、追加する 5 関数です。

| 関数 | シグネチャ(現行, mut 表示) | 他言語の主な綴り | 判定 | 案 |
|---|---|---|---|---|
| `to_int8` | `int32.to_int8(x: Int32) -> Int8` | Rust `as` / Swift `init(truncatingIfNeeded:)` (F) / Kotlin `toByte()` 等 / Go `T(x)` | 撤去（→ `int8.from`） | R-L0-40 |
| `to_int16` | `int32.to_int16(x: Int32) -> Int16` | Rust `as` / Swift `init(truncatingIfNeeded:)` (F) / Kotlin `toByte()` 等 / Go `T(x)` | 撤去（→ `int16.from`） | R-L0-40 |
| `to_int64` | `int32.to_int64(x: Int32) -> Int64` | Rust `from`/`into` / Swift `init(_:)` / Kotlin `toLong()` 等 / Go `T(x)` | 撤去（→ `int64.from`） | R-L0-40 |
| `to_uint8` | `int32.to_uint8(x: Int32) -> UInt8` | Rust `as` / Swift `init(truncatingIfNeeded:)` (F) / Kotlin `toByte()` 等 / Go `T(x)` | 撤去（→ `uint8.from`） | R-L0-40 |
| `to_uint16` | `int32.to_uint16(x: Int32) -> UInt16` | Rust `as` / Swift `init(truncatingIfNeeded:)` (F) / Kotlin `toByte()` 等 / Go `T(x)` | 撤去（→ `uint16.from`） | R-L0-40 |
| `to_uint32` | `int32.to_uint32(x: Int32) -> UInt32` | Rust `as` / Swift `init(truncatingIfNeeded:)` (F) / Kotlin `toByte()` 等 / Go `T(x)` | 撤去（→ `uint32.from`） | R-L0-40 |
| `to_uint64` | `int32.to_uint64(x: Int32) -> UInt64` | Rust `as` / Swift `init(truncatingIfNeeded:)` (F) / Kotlin `toByte()` 等 / Go `T(x)` | 撤去（→ `uint64.from`） | R-L0-40 |
| `to_float32` | `int32.to_float32(x: Int32) -> Float32` | Rust `as` / Kotlin `toDouble()` / Swift `init(_:)` / Gleam `to_float` (F) | 撤去（→ `float32.from`） | R-L0-40 |
| `to_float64` | `int32.to_float64(x: Int32) -> Float64` | Rust `as` / Kotlin `toDouble()` / Swift `init(_:)` / Gleam `to_float` (F) | 撤去（→ `float64.from`） | R-L0-40 |
| `to_string` | `int32.to_string(x: Int32) -> String` | Rust `to_string` / Kotlin `toString` / Swift `String(x)` / Go `strconv.FormatInt(int64(x),10)` | 据え置き | — |
| `to_int8_checked` | `int32.to_int8_checked(x: Int32) -> Option[Int8]` | Rust `try_from` / Swift `init(exactly:)` (F) / OCaml Base `to_int_exn`系 / 他 6 言語 — | 撤去（→ `int8.from_checked`） | R-L0-40 |
| `to_int8_saturating` | `int32.to_int8_saturating(x: Int32) -> Int8` | Swift `init(clamping:)` (F) / Kotlin `coerceIn` / 他 7 言語 — | 撤去（→ `int8.from_saturating`） | R-L0-40 |
| `to_int16_checked` | `int32.to_int16_checked(x: Int32) -> Option[Int16]` | Rust `try_from` / Swift `init(exactly:)` (F) / OCaml Base `to_int_exn`系 / 他 6 言語 — | 撤去（→ `int16.from_checked`） | R-L0-40 |
| `to_int16_saturating` | `int32.to_int16_saturating(x: Int32) -> Int16` | Swift `init(clamping:)` (F) / Kotlin `coerceIn` / 他 7 言語 — | 撤去（→ `int16.from_saturating`） | R-L0-40 |
| `to_uint8_checked` | `int32.to_uint8_checked(x: Int32) -> Option[UInt8]` | Rust `try_from` / Swift `init(exactly:)` (F) / OCaml Base `to_int_exn`系 / 他 6 言語 — | 撤去（→ `uint8.from_checked`） | R-L0-40 |
| `to_uint8_saturating` | `int32.to_uint8_saturating(x: Int32) -> UInt8` | Swift `init(clamping:)` (F) / Kotlin `coerceIn` / 他 7 言語 — | 撤去（→ `uint8.from_saturating`） | R-L0-40 |
| `to_uint16_checked` | `int32.to_uint16_checked(x: Int32) -> Option[UInt16]` | Rust `try_from` / Swift `init(exactly:)` (F) / OCaml Base `to_int_exn`系 / 他 6 言語 — | 撤去（→ `uint16.from_checked`） | R-L0-40 |
| `to_uint16_saturating` | `int32.to_uint16_saturating(x: Int32) -> UInt16` | Swift `init(clamping:)` (F) / Kotlin `coerceIn` / 他 7 言語 — | 撤去（→ `uint16.from_saturating`） | R-L0-40 |
| `to_uint32_checked` | `int32.to_uint32_checked(x: Int32) -> Option[UInt32]` | Rust `try_from` / Swift `init(exactly:)` (F) / OCaml Base `to_int_exn`系 / 他 6 言語 — | 撤去（→ `uint32.from_checked`） | R-L0-40 |
| `to_uint32_saturating` | `int32.to_uint32_saturating(x: Int32) -> UInt32` | Swift `init(clamping:)` (F) / Kotlin `coerceIn` / 他 7 言語 — | 撤去（→ `uint32.from_saturating`） | R-L0-40 |
| `to_uint64_checked` | `int32.to_uint64_checked(x: Int32) -> Option[UInt64]` | Rust `try_from` / Swift `init(exactly:)` (F) / OCaml Base `to_int_exn`系 / 他 6 言語 — | 撤去（→ `uint64.from_checked`） | R-L0-40 |
| `to_uint64_saturating` | `int32.to_uint64_saturating(x: Int32) -> UInt64` | Swift `init(clamping:)` (F) / Kotlin `coerceIn` / 他 7 言語 — | 撤去（→ `uint64.from_saturating`） | R-L0-40 |
| `min_value` | `int32.min_value() -> Int32` | Rust `MIN` / Go `math.Min…` / Kotlin `MIN_VALUE` / Swift `.min` | 据え置き | — |
| `max_value` | `int32.max_value() -> Int32` | Rust `MAX` / Go `math.Max…` / Kotlin `MAX_VALUE` / Swift `.max` | 据え置き | — |
| `parse` | （追加）`int32.parse(s: String) -> Result[Int32, String]` | Rust `str::parse::<T>` / Go `ParseInt(s,10,bits)` / OCaml `Int32.of_string_opt` / Kotlin `toByteOrNull` 等 / Swift `T(s)` | 追加 | R-sized-02 |
| `compare` | （追加）`int32.compare(a: Int32, b: Int32) -> Int` | Rust `cmp`/`total_cmp` / Go `cmp.Compare` / Kotlin `compareTo` / Swift `<` | 追加 | R-sized-02 |
| `floor_div` | （追加）`int32.floor_div(a: Int32, b: Int32) -> Int32` | Rust `div_euclid` / Swift — / Python `//` | 追加 | R-sized-10 |
| `modulo` | （追加）`int32.modulo(a: Int32, b: Int32) -> Int32` | Rust `rem_euclid` / Python `%` | 追加 | R-sized-10 |
| `pow` | （追加）`int32.pow(base: Int32, exp: Int) -> Int32` | Rust `pow` / Swift — | 追加 | R-sized-10 |
| `is_even` | （追加）`int32.is_even(n: Int32) -> Bool` | Swift `isMultiple(of: 2)` / Kotlin — | 追加 | R-sized-10 |
| `is_odd` | （追加）`int32.is_odd(n: Int32) -> Bool` | Swift `!isMultiple(of: 2)` | 追加 | R-sized-10 |
| `count_leading_zeros` | （追加）`int32.count_leading_zeros(n: Int32) -> Int` | Rust `leading_zeros` / Swift `leadingZeroBitCount` | 追加 | R-sized-10 |
| `count_trailing_zeros` | （追加）`int32.count_trailing_zeros(n: Int32) -> Int` | Rust `trailing_zeros` / Swift `trailingZeroBitCount` | 追加 | R-sized-10 |
| `count_ones` | （追加）`int32.count_ones(n: Int32) -> Int` | Rust `count_ones` / Swift `nonzeroBitCount` | 追加 | R-sized-10 |
| `byte_swap` | （追加）`int32.byte_swap(n: Int32) -> Int32` | Rust `swap_bytes` / Swift `byteSwapped` | 追加 | R-sized-10 |
| `abs` | （追加）`int32.abs(n: Int32) -> Int32` | Rust `abs` / Swift `abs` | 追加 | R-sized-10 |
| `sign` | （追加）`int32.sign(n: Int32) -> Int` | Rust `signum` / Swift `signum()` | 追加 | R-sized-10 |
| `from` | （追加）`int32.from[S: Numeric](x: S) -> Int32` | Rust `From`・`as` / Swift `init(_:)` / Python `int(x)` | 追加 | R-L0-40 |
| `from_checked` | （追加）`int32.from_checked[S: Numeric](x: S) -> Int32?` | Rust `TryFrom` / Swift `init(exactly:)` | 追加 | R-L0-40 |
| `from_saturating` | （追加）`int32.from_saturating[S: Numeric](x: S) -> Int32` | Swift `init(clamping:)` / Rust `as`（浮動小数点から） | 追加 | R-L0-40 |
| `from_wrapping` | （追加）`int32.from_wrapping[S: Integer](x: S) -> Int32` | Swift `init(truncatingIfNeeded:)` / Rust `as` | 追加 | R-L0-40 |
| `wrap_add` | （追加）`int32.wrap_add(a: Int32, b: Int32) -> Int32` | Swift `&+` / Rust `wrapping_add` | 追加 | R-int-12 |
| `wrap_sub` | （追加）`int32.wrap_sub(a: Int32, b: Int32) -> Int32` | Swift `&-` / Rust `wrapping_sub` | 追加 | R-int-12 |
| `wrap_mul` | （追加）`int32.wrap_mul(a: Int32, b: Int32) -> Int32` | Swift `&*` / Rust `wrapping_mul` | 追加 | R-int-12 |
| `rotate_left` | （追加）`int32.rotate_left(a: Int32, n: Int) -> Int32` | Rust `rotate_left` / Go `bits.RotateLeft` | 追加 | R-int-12 |
| `rotate_right` | （追加）`int32.rotate_right(a: Int32, n: Int) -> Int32` | Rust `rotate_right` | 追加 | R-int-12 |
| `bit_length` | （追加）`int32.bit_length(n: Int32) -> Int` | Python `bit_length` / Go `bits.Len` | 追加 | R-int-11 |
| `band` | （追加）`int32.band(a: Int32, b: Int32) -> Int32` | Elixir `band` / Rust・Go・Swift 演算子 `&` | 追加 | R-sized-10 |
| `bor` | （追加）`int32.bor(a: Int32, b: Int32) -> Int32` | Elixir `bor` / Rust・Go・Swift 演算子 `\|` | 追加 | R-sized-10 |
| `bxor` | （追加）`int32.bxor(a: Int32, b: Int32) -> Int32` | Elixir `bxor` / Rust・Go・Swift 演算子 `^` | 追加 | R-sized-10 |
| `bnot` | （追加）`int32.bnot(a: Int32) -> Int32` | Elixir `bnot` / Swift `~` | 追加 | R-sized-10 |
| `bshl` | （追加）`int32.bshl(a: Int32, n: Int) -> Int32` | Elixir `bsl` / 演算子 `<<` | 追加 | R-sized-10 |
| `bshr` | （追加）`int32.bshr(a: Int32, n: Int) -> Int32` | Elixir `bsr` / 演算子 `>>` | 追加 | R-sized-10 |

### L3: `int64`

現行 26 関数と、追加する 7 関数です。

| 関数 | シグネチャ(現行, mut 表示) | 他言語の主な綴り | 判定 | 案 |
|---|---|---|---|---|
| `to_int8` | `int64.to_int8(x: Int64) -> Int8` | Rust `as` / Swift `init(truncatingIfNeeded:)` (F) / Kotlin `toByte()` 等 / Go `T(x)` | 撤去（→ `int8.from`） | R-L0-40 |
| `to_int16` | `int64.to_int16(x: Int64) -> Int16` | Rust `as` / Swift `init(truncatingIfNeeded:)` (F) / Kotlin `toByte()` 等 / Go `T(x)` | 撤去（→ `int16.from`） | R-L0-40 |
| `to_int32` | `int64.to_int32(x: Int64) -> Int32` | Rust `as` / Swift `init(truncatingIfNeeded:)` (F) / Kotlin `toByte()` 等 / Go `T(x)` | 撤去（→ `int32.from`） | R-L0-40 |
| `to_uint8` | `int64.to_uint8(x: Int64) -> UInt8` | Rust `as` / Swift `init(truncatingIfNeeded:)` (F) / Kotlin `toByte()` 等 / Go `T(x)` | 撤去（→ `uint8.from`） | R-L0-40 |
| `to_uint16` | `int64.to_uint16(x: Int64) -> UInt16` | Rust `as` / Swift `init(truncatingIfNeeded:)` (F) / Kotlin `toByte()` 等 / Go `T(x)` | 撤去（→ `uint16.from`） | R-L0-40 |
| `to_uint32` | `int64.to_uint32(x: Int64) -> UInt32` | Rust `as` / Swift `init(truncatingIfNeeded:)` (F) / Kotlin `toByte()` 等 / Go `T(x)` | 撤去（→ `uint32.from`） | R-L0-40 |
| `to_uint64` | `int64.to_uint64(x: Int64) -> UInt64` | Rust `as` / Swift `init(truncatingIfNeeded:)` (F) / Kotlin `toByte()` 等 / Go `T(x)` | 撤去（→ `uint64.from`） | R-L0-40 |
| `to_float32` | `int64.to_float32(x: Int64) -> Float32` | Rust `as` / Kotlin `toDouble()` / Swift `init(_:)` / Gleam `to_float` (F) | 撤去（→ `float32.from`） | R-L0-40 |
| `to_float64` | `int64.to_float64(x: Int64) -> Float64` | Rust `as` / Kotlin `toDouble()` / Swift `init(_:)` / Gleam `to_float` (F) | 撤去（→ `float64.from`） | R-L0-40 |
| `to_string` | `int64.to_string(x: Int64) -> String` | Rust `to_string` / Kotlin `toString` / Swift `String(x)` / Go `strconv.FormatInt(int64(x),10)` | 据え置き | — |
| `to_int8_checked` | `int64.to_int8_checked(x: Int64) -> Option[Int8]` | Rust `try_from` / Swift `init(exactly:)` (F) / OCaml Base `to_int_exn`系 / 他 6 言語 — | 撤去（→ `int8.from_checked`） | R-L0-40 |
| `to_int8_saturating` | `int64.to_int8_saturating(x: Int64) -> Int8` | Swift `init(clamping:)` (F) / Kotlin `coerceIn` / 他 7 言語 — | 撤去（→ `int8.from_saturating`） | R-L0-40 |
| `to_int16_checked` | `int64.to_int16_checked(x: Int64) -> Option[Int16]` | Rust `try_from` / Swift `init(exactly:)` (F) / OCaml Base `to_int_exn`系 / 他 6 言語 — | 撤去（→ `int16.from_checked`） | R-L0-40 |
| `to_int16_saturating` | `int64.to_int16_saturating(x: Int64) -> Int16` | Swift `init(clamping:)` (F) / Kotlin `coerceIn` / 他 7 言語 — | 撤去（→ `int16.from_saturating`） | R-L0-40 |
| `to_int32_checked` | `int64.to_int32_checked(x: Int64) -> Option[Int32]` | Rust `try_from` / Swift `init(exactly:)` (F) / OCaml Base `to_int_exn`系 / 他 6 言語 — | 撤去（→ `int32.from_checked`） | R-L0-40 |
| `to_int32_saturating` | `int64.to_int32_saturating(x: Int64) -> Int32` | Swift `init(clamping:)` (F) / Kotlin `coerceIn` / 他 7 言語 — | 撤去（→ `int32.from_saturating`） | R-L0-40 |
| `to_uint8_checked` | `int64.to_uint8_checked(x: Int64) -> Option[UInt8]` | Rust `try_from` / Swift `init(exactly:)` (F) / OCaml Base `to_int_exn`系 / 他 6 言語 — | 撤去（→ `uint8.from_checked`） | R-L0-40 |
| `to_uint8_saturating` | `int64.to_uint8_saturating(x: Int64) -> UInt8` | Swift `init(clamping:)` (F) / Kotlin `coerceIn` / 他 7 言語 — | 撤去（→ `uint8.from_saturating`） | R-L0-40 |
| `to_uint16_checked` | `int64.to_uint16_checked(x: Int64) -> Option[UInt16]` | Rust `try_from` / Swift `init(exactly:)` (F) / OCaml Base `to_int_exn`系 / 他 6 言語 — | 撤去（→ `uint16.from_checked`） | R-L0-40 |
| `to_uint16_saturating` | `int64.to_uint16_saturating(x: Int64) -> UInt16` | Swift `init(clamping:)` (F) / Kotlin `coerceIn` / 他 7 言語 — | 撤去（→ `uint16.from_saturating`） | R-L0-40 |
| `to_uint32_checked` | `int64.to_uint32_checked(x: Int64) -> Option[UInt32]` | Rust `try_from` / Swift `init(exactly:)` (F) / OCaml Base `to_int_exn`系 / 他 6 言語 — | 撤去（→ `uint32.from_checked`） | R-L0-40 |
| `to_uint32_saturating` | `int64.to_uint32_saturating(x: Int64) -> UInt32` | Swift `init(clamping:)` (F) / Kotlin `coerceIn` / 他 7 言語 — | 撤去（→ `uint32.from_saturating`） | R-L0-40 |
| `to_uint64_checked` | `int64.to_uint64_checked(x: Int64) -> Option[UInt64]` | Rust `try_from` / Swift `init(exactly:)` (F) / OCaml Base `to_int_exn`系 / 他 6 言語 — | 撤去（→ `uint64.from_checked`） | R-L0-40 |
| `to_uint64_saturating` | `int64.to_uint64_saturating(x: Int64) -> UInt64` | Swift `init(clamping:)` (F) / Kotlin `coerceIn` / 他 7 言語 — | 撤去（→ `uint64.from_saturating`） | R-L0-40 |
| `min_value` | `int64.min_value() -> Int64` | Rust `MIN` / Go `math.Min…` / Kotlin `MIN_VALUE` / Swift `.min` | 据え置き | — |
| `max_value` | `int64.max_value() -> Int64` | Rust `MAX` / Go `math.Max…` / Kotlin `MAX_VALUE` / Swift `.max` | 据え置き | — |
| `parse` | （追加）`int64.parse(s: String) -> Result[Int64, String]` | Rust `str::parse::<T>` / Go `ParseInt(s,10,bits)` / OCaml `Int32.of_string_opt` / Kotlin `toByteOrNull` 等 / Swift `T(s)` | 追加 | R-sized-02 |
| `compare` | （追加）`int64.compare(a: Int64, b: Int64) -> Int` | Rust `cmp`/`total_cmp` / Go `cmp.Compare` / Kotlin `compareTo` / Swift `<` | 追加 | R-sized-02 |
| `floor_div` | （追加）`int64.floor_div(a: Int64, b: Int64) -> Int64` | Rust `div_euclid` / Swift — / Python `//` | 追加 | R-sized-10 |
| `modulo` | （追加）`int64.modulo(a: Int64, b: Int64) -> Int64` | Rust `rem_euclid` / Python `%` | 追加 | R-sized-10 |
| `pow` | （追加）`int64.pow(base: Int64, exp: Int) -> Int64` | Rust `pow` / Swift — | 追加 | R-sized-10 |
| `is_even` | （追加）`int64.is_even(n: Int64) -> Bool` | Swift `isMultiple(of: 2)` / Kotlin — | 追加 | R-sized-10 |
| `is_odd` | （追加）`int64.is_odd(n: Int64) -> Bool` | Swift `!isMultiple(of: 2)` | 追加 | R-sized-10 |
| `count_leading_zeros` | （追加）`int64.count_leading_zeros(n: Int64) -> Int` | Rust `leading_zeros` / Swift `leadingZeroBitCount` | 追加 | R-sized-10 |
| `count_trailing_zeros` | （追加）`int64.count_trailing_zeros(n: Int64) -> Int` | Rust `trailing_zeros` / Swift `trailingZeroBitCount` | 追加 | R-sized-10 |
| `count_ones` | （追加）`int64.count_ones(n: Int64) -> Int` | Rust `count_ones` / Swift `nonzeroBitCount` | 追加 | R-sized-10 |
| `byte_swap` | （追加）`int64.byte_swap(n: Int64) -> Int64` | Rust `swap_bytes` / Swift `byteSwapped` | 追加 | R-sized-10 |
| `abs` | （追加）`int64.abs(n: Int64) -> Int64` | Rust `abs` / Swift `abs` | 追加 | R-sized-10 |
| `sign` | （追加）`int64.sign(n: Int64) -> Int` | Rust `signum` / Swift `signum()` | 追加 | R-sized-10 |
| `from` | （追加）`int64.from[S: Numeric](x: S) -> Int64` | Rust `From`・`as` / Swift `init(_:)` / Python `int(x)` | 追加 | R-L0-40 |
| `from_checked` | （追加）`int64.from_checked[S: Numeric](x: S) -> Int64?` | Rust `TryFrom` / Swift `init(exactly:)` | 追加 | R-L0-40 |
| `from_saturating` | （追加）`int64.from_saturating[S: Numeric](x: S) -> Int64` | Swift `init(clamping:)` / Rust `as`（浮動小数点から） | 追加 | R-L0-40 |
| `from_wrapping` | （追加）`int64.from_wrapping[S: Integer](x: S) -> Int64` | Swift `init(truncatingIfNeeded:)` / Rust `as` | 追加 | R-L0-40 |
| `wrap_add` | （追加）`int64.wrap_add(a: Int64, b: Int64) -> Int64` | Swift `&+` / Rust `wrapping_add` | 追加 | R-int-12 |
| `wrap_sub` | （追加）`int64.wrap_sub(a: Int64, b: Int64) -> Int64` | Swift `&-` / Rust `wrapping_sub` | 追加 | R-int-12 |
| `wrap_mul` | （追加）`int64.wrap_mul(a: Int64, b: Int64) -> Int64` | Swift `&*` / Rust `wrapping_mul` | 追加 | R-int-12 |
| `rotate_left` | （追加）`int64.rotate_left(a: Int64, n: Int) -> Int64` | Rust `rotate_left` / Go `bits.RotateLeft` | 追加 | R-int-12 |
| `rotate_right` | （追加）`int64.rotate_right(a: Int64, n: Int) -> Int64` | Rust `rotate_right` | 追加 | R-int-12 |
| `bit_length` | （追加）`int64.bit_length(n: Int64) -> Int` | Python `bit_length` / Go `bits.Len` | 追加 | R-int-11 |
| `band` | （追加）`int64.band(a: Int64, b: Int64) -> Int64` | Elixir `band` / Rust・Go・Swift 演算子 `&` | 追加 | R-sized-10 |
| `bor` | （追加）`int64.bor(a: Int64, b: Int64) -> Int64` | Elixir `bor` / Rust・Go・Swift 演算子 `\|` | 追加 | R-sized-10 |
| `bxor` | （追加）`int64.bxor(a: Int64, b: Int64) -> Int64` | Elixir `bxor` / Rust・Go・Swift 演算子 `^` | 追加 | R-sized-10 |
| `bnot` | （追加）`int64.bnot(a: Int64) -> Int64` | Elixir `bnot` / Swift `~` | 追加 | R-sized-10 |
| `bshl` | （追加）`int64.bshl(a: Int64, n: Int) -> Int64` | Elixir `bsl` / 演算子 `<<` | 追加 | R-sized-10 |
| `bshr` | （追加）`int64.bshr(a: Int64, n: Int) -> Int64` | Elixir `bsr` / 演算子 `>>` | 追加 | R-sized-10 |

### L3: `uint8`

現行 14 関数と、追加する 4 関数です。

| 関数 | シグネチャ(現行, mut 表示) | 他言語の主な綴り | 判定 | 案 |
|---|---|---|---|---|
| `to_int8` | `uint8.to_int8(x: UInt8) -> Int8` | Rust `as` / Swift `init(truncatingIfNeeded:)` (F) / Kotlin `toByte()` 等 / Go `T(x)` | 撤去（→ `int8.from`） | R-L0-40 |
| `to_int16` | `uint8.to_int16(x: UInt8) -> Int16` | Rust `from`/`into` / Swift `init(_:)` / Kotlin `toLong()` 等 / Go `T(x)` | 撤去（→ `int16.from`） | R-L0-40 |
| `to_int32` | `uint8.to_int32(x: UInt8) -> Int32` | Rust `from`/`into` / Swift `init(_:)` / Kotlin `toLong()` 等 / Go `T(x)` | 撤去（→ `int32.from`） | R-L0-40 |
| `to_int64` | `uint8.to_int64(x: UInt8) -> Int64` | Rust `from`/`into` / Swift `init(_:)` / Kotlin `toLong()` 等 / Go `T(x)` | 撤去（→ `int64.from`） | R-L0-40 |
| `to_uint16` | `uint8.to_uint16(x: UInt8) -> UInt16` | Rust `from`/`into` / Swift `init(_:)` / Kotlin `toLong()` 等 / Go `T(x)` | 撤去（→ `uint16.from`） | R-L0-40 |
| `to_uint32` | `uint8.to_uint32(x: UInt8) -> UInt32` | Rust `from`/`into` / Swift `init(_:)` / Kotlin `toLong()` 等 / Go `T(x)` | 撤去（→ `uint32.from`） | R-L0-40 |
| `to_uint64` | `uint8.to_uint64(x: UInt8) -> UInt64` | Rust `from`/`into` / Swift `init(_:)` / Kotlin `toLong()` 等 / Go `T(x)` | 撤去（→ `uint64.from`） | R-L0-40 |
| `to_float32` | `uint8.to_float32(x: UInt8) -> Float32` | Rust `as` / Kotlin `toDouble()` / Swift `init(_:)` / Gleam `to_float` (F) | 撤去（→ `float32.from`） | R-L0-40 |
| `to_float64` | `uint8.to_float64(x: UInt8) -> Float64` | Rust `as` / Kotlin `toDouble()` / Swift `init(_:)` / Gleam `to_float` (F) | 撤去（→ `float64.from`） | R-L0-40 |
| `to_string` | `uint8.to_string(x: UInt8) -> String` | Rust `to_string` / Kotlin `toString` / Swift `String(x)` / Go `strconv.FormatInt(int64(x),10)` | 据え置き | — |
| `to_int8_checked` | `uint8.to_int8_checked(x: UInt8) -> Option[Int8]` | Rust `try_from` / Swift `init(exactly:)` (F) / OCaml Base `to_int_exn`系 / 他 6 言語 — | 撤去（→ `int8.from_checked`） | R-L0-40 |
| `to_int8_saturating` | `uint8.to_int8_saturating(x: UInt8) -> Int8` | Swift `init(clamping:)` (F) / Kotlin `coerceIn` / 他 7 言語 — | 撤去（→ `int8.from_saturating`） | R-L0-40 |
| `min_value` | `uint8.min_value() -> UInt8` | Rust `MIN` / Go `math.Min…` / Kotlin `MIN_VALUE` / Swift `.min` | 据え置き | — |
| `max_value` | `uint8.max_value() -> UInt8` | Rust `MAX` / Go `math.Max…` / Kotlin `MAX_VALUE` / Swift `.max` | 据え置き | — |
| `parse` | （追加）`uint8.parse(s: String) -> Result[UInt8, String]` | Rust `str::parse::<T>` / Go `ParseInt(s,10,bits)` / OCaml `Int32.of_string_opt` / Kotlin `toByteOrNull` 等 / Swift `T(s)` | 追加 | R-sized-02 |
| `compare` | （追加）`uint8.compare(a: UInt8, b: UInt8) -> Int` | Rust `cmp`/`total_cmp` / Go `cmp.Compare` / Kotlin `compareTo` / Swift `<` | 追加 | R-sized-02 |
| `floor_div` | （追加）`uint8.floor_div(a: UInt8, b: UInt8) -> UInt8` | Rust `div_euclid` / Swift — / Python `//` | 追加 | R-sized-10 |
| `modulo` | （追加）`uint8.modulo(a: UInt8, b: UInt8) -> UInt8` | Rust `rem_euclid` / Python `%` | 追加 | R-sized-10 |
| `pow` | （追加）`uint8.pow(base: UInt8, exp: Int) -> UInt8` | Rust `pow` / Swift — | 追加 | R-sized-10 |
| `is_even` | （追加）`uint8.is_even(n: UInt8) -> Bool` | Swift `isMultiple(of: 2)` / Kotlin — | 追加 | R-sized-10 |
| `is_odd` | （追加）`uint8.is_odd(n: UInt8) -> Bool` | Swift `!isMultiple(of: 2)` | 追加 | R-sized-10 |
| `count_leading_zeros` | （追加）`uint8.count_leading_zeros(n: UInt8) -> Int` | Rust `leading_zeros` / Swift `leadingZeroBitCount` | 追加 | R-sized-10 |
| `count_trailing_zeros` | （追加）`uint8.count_trailing_zeros(n: UInt8) -> Int` | Rust `trailing_zeros` / Swift `trailingZeroBitCount` | 追加 | R-sized-10 |
| `count_ones` | （追加）`uint8.count_ones(n: UInt8) -> Int` | Rust `count_ones` / Swift `nonzeroBitCount` | 追加 | R-sized-10 |
| `byte_swap` | （追加）`uint8.byte_swap(n: UInt8) -> UInt8` | Rust `swap_bytes` / Swift `byteSwapped` | 追加 | R-sized-10 |
| `from` | （追加）`uint8.from[S: Numeric](x: S) -> UInt8` | Rust `From`・`as` / Swift `init(_:)` / Python `int(x)` | 追加 | R-L0-40 |
| `from_checked` | （追加）`uint8.from_checked[S: Numeric](x: S) -> UInt8?` | Rust `TryFrom` / Swift `init(exactly:)` | 追加 | R-L0-40 |
| `from_saturating` | （追加）`uint8.from_saturating[S: Numeric](x: S) -> UInt8` | Swift `init(clamping:)` / Rust `as`（浮動小数点から） | 追加 | R-L0-40 |
| `from_wrapping` | （追加）`uint8.from_wrapping[S: Integer](x: S) -> UInt8` | Swift `init(truncatingIfNeeded:)` / Rust `as` | 追加 | R-L0-40 |
| `wrap_add` | （追加）`uint8.wrap_add(a: UInt8, b: UInt8) -> UInt8` | Swift `&+` / Rust `wrapping_add` | 追加 | R-int-12 |
| `wrap_sub` | （追加）`uint8.wrap_sub(a: UInt8, b: UInt8) -> UInt8` | Swift `&-` / Rust `wrapping_sub` | 追加 | R-int-12 |
| `wrap_mul` | （追加）`uint8.wrap_mul(a: UInt8, b: UInt8) -> UInt8` | Swift `&*` / Rust `wrapping_mul` | 追加 | R-int-12 |
| `rotate_left` | （追加）`uint8.rotate_left(a: UInt8, n: Int) -> UInt8` | Rust `rotate_left` / Go `bits.RotateLeft` | 追加 | R-int-12 |
| `rotate_right` | （追加）`uint8.rotate_right(a: UInt8, n: Int) -> UInt8` | Rust `rotate_right` | 追加 | R-int-12 |
| `bit_length` | （追加）`uint8.bit_length(n: UInt8) -> Int` | Python `bit_length` / Go `bits.Len` | 追加 | R-int-11 |
| `band` | （追加）`uint8.band(a: UInt8, b: UInt8) -> UInt8` | Elixir `band` / Rust・Go・Swift 演算子 `&` | 追加 | R-sized-10 |
| `bor` | （追加）`uint8.bor(a: UInt8, b: UInt8) -> UInt8` | Elixir `bor` / Rust・Go・Swift 演算子 `\|` | 追加 | R-sized-10 |
| `bxor` | （追加）`uint8.bxor(a: UInt8, b: UInt8) -> UInt8` | Elixir `bxor` / Rust・Go・Swift 演算子 `^` | 追加 | R-sized-10 |
| `bnot` | （追加）`uint8.bnot(a: UInt8) -> UInt8` | Elixir `bnot` / Swift `~` | 追加 | R-sized-10 |
| `bshl` | （追加）`uint8.bshl(a: UInt8, n: Int) -> UInt8` | Elixir `bsl` / 演算子 `<<` | 追加 | R-sized-10 |
| `bshr` | （追加）`uint8.bshr(a: UInt8, n: Int) -> UInt8` | Elixir `bsr` / 演算子 `>>` | 追加 | R-sized-10 |

### L3: `uint16`

現行 18 関数と、追加する 4 関数です。

| 関数 | シグネチャ(現行, mut 表示) | 他言語の主な綴り | 判定 | 案 |
|---|---|---|---|---|
| `to_int8` | `uint16.to_int8(x: UInt16) -> Int8` | Rust `as` / Swift `init(truncatingIfNeeded:)` (F) / Kotlin `toByte()` 等 / Go `T(x)` | 撤去（→ `int8.from`） | R-L0-40 |
| `to_int16` | `uint16.to_int16(x: UInt16) -> Int16` | Rust `as` / Swift `init(truncatingIfNeeded:)` (F) / Kotlin `toByte()` 等 / Go `T(x)` | 撤去（→ `int16.from`） | R-L0-40 |
| `to_int32` | `uint16.to_int32(x: UInt16) -> Int32` | Rust `from`/`into` / Swift `init(_:)` / Kotlin `toLong()` 等 / Go `T(x)` | 撤去（→ `int32.from`） | R-L0-40 |
| `to_int64` | `uint16.to_int64(x: UInt16) -> Int64` | Rust `from`/`into` / Swift `init(_:)` / Kotlin `toLong()` 等 / Go `T(x)` | 撤去（→ `int64.from`） | R-L0-40 |
| `to_uint8` | `uint16.to_uint8(x: UInt16) -> UInt8` | Rust `as` / Swift `init(truncatingIfNeeded:)` (F) / Kotlin `toByte()` 等 / Go `T(x)` | 撤去（→ `uint8.from`） | R-L0-40 |
| `to_uint32` | `uint16.to_uint32(x: UInt16) -> UInt32` | Rust `from`/`into` / Swift `init(_:)` / Kotlin `toLong()` 等 / Go `T(x)` | 撤去（→ `uint32.from`） | R-L0-40 |
| `to_uint64` | `uint16.to_uint64(x: UInt16) -> UInt64` | Rust `from`/`into` / Swift `init(_:)` / Kotlin `toLong()` 等 / Go `T(x)` | 撤去（→ `uint64.from`） | R-L0-40 |
| `to_float32` | `uint16.to_float32(x: UInt16) -> Float32` | Rust `as` / Kotlin `toDouble()` / Swift `init(_:)` / Gleam `to_float` (F) | 撤去（→ `float32.from`） | R-L0-40 |
| `to_float64` | `uint16.to_float64(x: UInt16) -> Float64` | Rust `as` / Kotlin `toDouble()` / Swift `init(_:)` / Gleam `to_float` (F) | 撤去（→ `float64.from`） | R-L0-40 |
| `to_string` | `uint16.to_string(x: UInt16) -> String` | Rust `to_string` / Kotlin `toString` / Swift `String(x)` / Go `strconv.FormatInt(int64(x),10)` | 据え置き | — |
| `to_int8_checked` | `uint16.to_int8_checked(x: UInt16) -> Option[Int8]` | Rust `try_from` / Swift `init(exactly:)` (F) / OCaml Base `to_int_exn`系 / 他 6 言語 — | 撤去（→ `int8.from_checked`） | R-L0-40 |
| `to_int8_saturating` | `uint16.to_int8_saturating(x: UInt16) -> Int8` | Swift `init(clamping:)` (F) / Kotlin `coerceIn` / 他 7 言語 — | 撤去（→ `int8.from_saturating`） | R-L0-40 |
| `to_int16_checked` | `uint16.to_int16_checked(x: UInt16) -> Option[Int16]` | Rust `try_from` / Swift `init(exactly:)` (F) / OCaml Base `to_int_exn`系 / 他 6 言語 — | 撤去（→ `int16.from_checked`） | R-L0-40 |
| `to_int16_saturating` | `uint16.to_int16_saturating(x: UInt16) -> Int16` | Swift `init(clamping:)` (F) / Kotlin `coerceIn` / 他 7 言語 — | 撤去（→ `int16.from_saturating`） | R-L0-40 |
| `to_uint8_checked` | `uint16.to_uint8_checked(x: UInt16) -> Option[UInt8]` | Rust `try_from` / Swift `init(exactly:)` (F) / OCaml Base `to_int_exn`系 / 他 6 言語 — | 撤去（→ `uint8.from_checked`） | R-L0-40 |
| `to_uint8_saturating` | `uint16.to_uint8_saturating(x: UInt16) -> UInt8` | Swift `init(clamping:)` (F) / Kotlin `coerceIn` / 他 7 言語 — | 撤去（→ `uint8.from_saturating`） | R-L0-40 |
| `min_value` | `uint16.min_value() -> UInt16` | Rust `MIN` / Go `math.Min…` / Kotlin `MIN_VALUE` / Swift `.min` | 据え置き | — |
| `max_value` | `uint16.max_value() -> UInt16` | Rust `MAX` / Go `math.Max…` / Kotlin `MAX_VALUE` / Swift `.max` | 据え置き | — |
| `parse` | （追加）`uint16.parse(s: String) -> Result[UInt16, String]` | Rust `str::parse::<T>` / Go `ParseInt(s,10,bits)` / OCaml `Int32.of_string_opt` / Kotlin `toByteOrNull` 等 / Swift `T(s)` | 追加 | R-sized-02 |
| `compare` | （追加）`uint16.compare(a: UInt16, b: UInt16) -> Int` | Rust `cmp`/`total_cmp` / Go `cmp.Compare` / Kotlin `compareTo` / Swift `<` | 追加 | R-sized-02 |
| `floor_div` | （追加）`uint16.floor_div(a: UInt16, b: UInt16) -> UInt16` | Rust `div_euclid` / Swift — / Python `//` | 追加 | R-sized-10 |
| `modulo` | （追加）`uint16.modulo(a: UInt16, b: UInt16) -> UInt16` | Rust `rem_euclid` / Python `%` | 追加 | R-sized-10 |
| `pow` | （追加）`uint16.pow(base: UInt16, exp: Int) -> UInt16` | Rust `pow` / Swift — | 追加 | R-sized-10 |
| `is_even` | （追加）`uint16.is_even(n: UInt16) -> Bool` | Swift `isMultiple(of: 2)` / Kotlin — | 追加 | R-sized-10 |
| `is_odd` | （追加）`uint16.is_odd(n: UInt16) -> Bool` | Swift `!isMultiple(of: 2)` | 追加 | R-sized-10 |
| `count_leading_zeros` | （追加）`uint16.count_leading_zeros(n: UInt16) -> Int` | Rust `leading_zeros` / Swift `leadingZeroBitCount` | 追加 | R-sized-10 |
| `count_trailing_zeros` | （追加）`uint16.count_trailing_zeros(n: UInt16) -> Int` | Rust `trailing_zeros` / Swift `trailingZeroBitCount` | 追加 | R-sized-10 |
| `count_ones` | （追加）`uint16.count_ones(n: UInt16) -> Int` | Rust `count_ones` / Swift `nonzeroBitCount` | 追加 | R-sized-10 |
| `byte_swap` | （追加）`uint16.byte_swap(n: UInt16) -> UInt16` | Rust `swap_bytes` / Swift `byteSwapped` | 追加 | R-sized-10 |
| `from` | （追加）`uint16.from[S: Numeric](x: S) -> UInt16` | Rust `From`・`as` / Swift `init(_:)` / Python `int(x)` | 追加 | R-L0-40 |
| `from_checked` | （追加）`uint16.from_checked[S: Numeric](x: S) -> UInt16?` | Rust `TryFrom` / Swift `init(exactly:)` | 追加 | R-L0-40 |
| `from_saturating` | （追加）`uint16.from_saturating[S: Numeric](x: S) -> UInt16` | Swift `init(clamping:)` / Rust `as`（浮動小数点から） | 追加 | R-L0-40 |
| `from_wrapping` | （追加）`uint16.from_wrapping[S: Integer](x: S) -> UInt16` | Swift `init(truncatingIfNeeded:)` / Rust `as` | 追加 | R-L0-40 |
| `wrap_add` | （追加）`uint16.wrap_add(a: UInt16, b: UInt16) -> UInt16` | Swift `&+` / Rust `wrapping_add` | 追加 | R-int-12 |
| `wrap_sub` | （追加）`uint16.wrap_sub(a: UInt16, b: UInt16) -> UInt16` | Swift `&-` / Rust `wrapping_sub` | 追加 | R-int-12 |
| `wrap_mul` | （追加）`uint16.wrap_mul(a: UInt16, b: UInt16) -> UInt16` | Swift `&*` / Rust `wrapping_mul` | 追加 | R-int-12 |
| `rotate_left` | （追加）`uint16.rotate_left(a: UInt16, n: Int) -> UInt16` | Rust `rotate_left` / Go `bits.RotateLeft` | 追加 | R-int-12 |
| `rotate_right` | （追加）`uint16.rotate_right(a: UInt16, n: Int) -> UInt16` | Rust `rotate_right` | 追加 | R-int-12 |
| `bit_length` | （追加）`uint16.bit_length(n: UInt16) -> Int` | Python `bit_length` / Go `bits.Len` | 追加 | R-int-11 |
| `band` | （追加）`uint16.band(a: UInt16, b: UInt16) -> UInt16` | Elixir `band` / Rust・Go・Swift 演算子 `&` | 追加 | R-sized-10 |
| `bor` | （追加）`uint16.bor(a: UInt16, b: UInt16) -> UInt16` | Elixir `bor` / Rust・Go・Swift 演算子 `\|` | 追加 | R-sized-10 |
| `bxor` | （追加）`uint16.bxor(a: UInt16, b: UInt16) -> UInt16` | Elixir `bxor` / Rust・Go・Swift 演算子 `^` | 追加 | R-sized-10 |
| `bnot` | （追加）`uint16.bnot(a: UInt16) -> UInt16` | Elixir `bnot` / Swift `~` | 追加 | R-sized-10 |
| `bshl` | （追加）`uint16.bshl(a: UInt16, n: Int) -> UInt16` | Elixir `bsl` / 演算子 `<<` | 追加 | R-sized-10 |
| `bshr` | （追加）`uint16.bshr(a: UInt16, n: Int) -> UInt16` | Elixir `bsr` / 演算子 `>>` | 追加 | R-sized-10 |

### L3: `uint32`

現行 22 関数と、追加する 5 関数です。

| 関数 | シグネチャ(現行, mut 表示) | 他言語の主な綴り | 判定 | 案 |
|---|---|---|---|---|
| `to_int8` | `uint32.to_int8(x: UInt32) -> Int8` | Rust `as` / Swift `init(truncatingIfNeeded:)` (F) / Kotlin `toByte()` 等 / Go `T(x)` | 撤去（→ `int8.from`） | R-L0-40 |
| `to_int16` | `uint32.to_int16(x: UInt32) -> Int16` | Rust `as` / Swift `init(truncatingIfNeeded:)` (F) / Kotlin `toByte()` 等 / Go `T(x)` | 撤去（→ `int16.from`） | R-L0-40 |
| `to_int32` | `uint32.to_int32(x: UInt32) -> Int32` | Rust `as` / Swift `init(truncatingIfNeeded:)` (F) / Kotlin `toByte()` 等 / Go `T(x)` | 撤去（→ `int32.from`） | R-L0-40 |
| `to_int64` | `uint32.to_int64(x: UInt32) -> Int64` | Rust `from`/`into` / Swift `init(_:)` / Kotlin `toLong()` 等 / Go `T(x)` | 撤去（→ `int64.from`） | R-L0-40 |
| `to_uint8` | `uint32.to_uint8(x: UInt32) -> UInt8` | Rust `as` / Swift `init(truncatingIfNeeded:)` (F) / Kotlin `toByte()` 等 / Go `T(x)` | 撤去（→ `uint8.from`） | R-L0-40 |
| `to_uint16` | `uint32.to_uint16(x: UInt32) -> UInt16` | Rust `as` / Swift `init(truncatingIfNeeded:)` (F) / Kotlin `toByte()` 等 / Go `T(x)` | 撤去（→ `uint16.from`） | R-L0-40 |
| `to_uint64` | `uint32.to_uint64(x: UInt32) -> UInt64` | Rust `from`/`into` / Swift `init(_:)` / Kotlin `toLong()` 等 / Go `T(x)` | 撤去（→ `uint64.from`） | R-L0-40 |
| `to_float32` | `uint32.to_float32(x: UInt32) -> Float32` | Rust `as` / Kotlin `toDouble()` / Swift `init(_:)` / Gleam `to_float` (F) | 撤去（→ `float32.from`） | R-L0-40 |
| `to_float64` | `uint32.to_float64(x: UInt32) -> Float64` | Rust `as` / Kotlin `toDouble()` / Swift `init(_:)` / Gleam `to_float` (F) | 撤去（→ `float64.from`） | R-L0-40 |
| `to_string` | `uint32.to_string(x: UInt32) -> String` | Rust `to_string` / Kotlin `toString` / Swift `String(x)` / Go `strconv.FormatInt(int64(x),10)` | 据え置き | — |
| `to_int8_checked` | `uint32.to_int8_checked(x: UInt32) -> Option[Int8]` | Rust `try_from` / Swift `init(exactly:)` (F) / OCaml Base `to_int_exn`系 / 他 6 言語 — | 撤去（→ `int8.from_checked`） | R-L0-40 |
| `to_int8_saturating` | `uint32.to_int8_saturating(x: UInt32) -> Int8` | Swift `init(clamping:)` (F) / Kotlin `coerceIn` / 他 7 言語 — | 撤去（→ `int8.from_saturating`） | R-L0-40 |
| `to_int16_checked` | `uint32.to_int16_checked(x: UInt32) -> Option[Int16]` | Rust `try_from` / Swift `init(exactly:)` (F) / OCaml Base `to_int_exn`系 / 他 6 言語 — | 撤去（→ `int16.from_checked`） | R-L0-40 |
| `to_int16_saturating` | `uint32.to_int16_saturating(x: UInt32) -> Int16` | Swift `init(clamping:)` (F) / Kotlin `coerceIn` / 他 7 言語 — | 撤去（→ `int16.from_saturating`） | R-L0-40 |
| `to_int32_checked` | `uint32.to_int32_checked(x: UInt32) -> Option[Int32]` | Rust `try_from` / Swift `init(exactly:)` (F) / OCaml Base `to_int_exn`系 / 他 6 言語 — | 撤去（→ `int32.from_checked`） | R-L0-40 |
| `to_int32_saturating` | `uint32.to_int32_saturating(x: UInt32) -> Int32` | Swift `init(clamping:)` (F) / Kotlin `coerceIn` / 他 7 言語 — | 撤去（→ `int32.from_saturating`） | R-L0-40 |
| `to_uint8_checked` | `uint32.to_uint8_checked(x: UInt32) -> Option[UInt8]` | Rust `try_from` / Swift `init(exactly:)` (F) / OCaml Base `to_int_exn`系 / 他 6 言語 — | 撤去（→ `uint8.from_checked`） | R-L0-40 |
| `to_uint8_saturating` | `uint32.to_uint8_saturating(x: UInt32) -> UInt8` | Swift `init(clamping:)` (F) / Kotlin `coerceIn` / 他 7 言語 — | 撤去（→ `uint8.from_saturating`） | R-L0-40 |
| `to_uint16_checked` | `uint32.to_uint16_checked(x: UInt32) -> Option[UInt16]` | Rust `try_from` / Swift `init(exactly:)` (F) / OCaml Base `to_int_exn`系 / 他 6 言語 — | 撤去（→ `uint16.from_checked`） | R-L0-40 |
| `to_uint16_saturating` | `uint32.to_uint16_saturating(x: UInt32) -> UInt16` | Swift `init(clamping:)` (F) / Kotlin `coerceIn` / 他 7 言語 — | 撤去（→ `uint16.from_saturating`） | R-L0-40 |
| `min_value` | `uint32.min_value() -> UInt32` | Rust `MIN` / Go `math.Min…` / Kotlin `MIN_VALUE` / Swift `.min` | 据え置き | — |
| `max_value` | `uint32.max_value() -> UInt32` | Rust `MAX` / Go `math.Max…` / Kotlin `MAX_VALUE` / Swift `.max` | 据え置き | — |
| `parse` | （追加）`uint32.parse(s: String) -> Result[UInt32, String]` | Rust `str::parse::<T>` / Go `ParseInt(s,10,bits)` / OCaml `Int32.of_string_opt` / Kotlin `toByteOrNull` 等 / Swift `T(s)` | 追加 | R-sized-02 |
| `compare` | （追加）`uint32.compare(a: UInt32, b: UInt32) -> Int` | Rust `cmp`/`total_cmp` / Go `cmp.Compare` / Kotlin `compareTo` / Swift `<` | 追加 | R-sized-02 |
| `floor_div` | （追加）`uint32.floor_div(a: UInt32, b: UInt32) -> UInt32` | Rust `div_euclid` / Swift — / Python `//` | 追加 | R-sized-10 |
| `modulo` | （追加）`uint32.modulo(a: UInt32, b: UInt32) -> UInt32` | Rust `rem_euclid` / Python `%` | 追加 | R-sized-10 |
| `pow` | （追加）`uint32.pow(base: UInt32, exp: Int) -> UInt32` | Rust `pow` / Swift — | 追加 | R-sized-10 |
| `is_even` | （追加）`uint32.is_even(n: UInt32) -> Bool` | Swift `isMultiple(of: 2)` / Kotlin — | 追加 | R-sized-10 |
| `is_odd` | （追加）`uint32.is_odd(n: UInt32) -> Bool` | Swift `!isMultiple(of: 2)` | 追加 | R-sized-10 |
| `count_leading_zeros` | （追加）`uint32.count_leading_zeros(n: UInt32) -> Int` | Rust `leading_zeros` / Swift `leadingZeroBitCount` | 追加 | R-sized-10 |
| `count_trailing_zeros` | （追加）`uint32.count_trailing_zeros(n: UInt32) -> Int` | Rust `trailing_zeros` / Swift `trailingZeroBitCount` | 追加 | R-sized-10 |
| `count_ones` | （追加）`uint32.count_ones(n: UInt32) -> Int` | Rust `count_ones` / Swift `nonzeroBitCount` | 追加 | R-sized-10 |
| `byte_swap` | （追加）`uint32.byte_swap(n: UInt32) -> UInt32` | Rust `swap_bytes` / Swift `byteSwapped` | 追加 | R-sized-10 |
| `from` | （追加）`uint32.from[S: Numeric](x: S) -> UInt32` | Rust `From`・`as` / Swift `init(_:)` / Python `int(x)` | 追加 | R-L0-40 |
| `from_checked` | （追加）`uint32.from_checked[S: Numeric](x: S) -> UInt32?` | Rust `TryFrom` / Swift `init(exactly:)` | 追加 | R-L0-40 |
| `from_saturating` | （追加）`uint32.from_saturating[S: Numeric](x: S) -> UInt32` | Swift `init(clamping:)` / Rust `as`（浮動小数点から） | 追加 | R-L0-40 |
| `from_wrapping` | （追加）`uint32.from_wrapping[S: Integer](x: S) -> UInt32` | Swift `init(truncatingIfNeeded:)` / Rust `as` | 追加 | R-L0-40 |
| `wrap_add` | （追加）`uint32.wrap_add(a: UInt32, b: UInt32) -> UInt32` | Swift `&+` / Rust `wrapping_add` | 追加 | R-int-12 |
| `wrap_sub` | （追加）`uint32.wrap_sub(a: UInt32, b: UInt32) -> UInt32` | Swift `&-` / Rust `wrapping_sub` | 追加 | R-int-12 |
| `wrap_mul` | （追加）`uint32.wrap_mul(a: UInt32, b: UInt32) -> UInt32` | Swift `&*` / Rust `wrapping_mul` | 追加 | R-int-12 |
| `rotate_left` | （追加）`uint32.rotate_left(a: UInt32, n: Int) -> UInt32` | Rust `rotate_left` / Go `bits.RotateLeft` | 追加 | R-int-12 |
| `rotate_right` | （追加）`uint32.rotate_right(a: UInt32, n: Int) -> UInt32` | Rust `rotate_right` | 追加 | R-int-12 |
| `bit_length` | （追加）`uint32.bit_length(n: UInt32) -> Int` | Python `bit_length` / Go `bits.Len` | 追加 | R-int-11 |
| `band` | （追加）`uint32.band(a: UInt32, b: UInt32) -> UInt32` | Elixir `band` / Rust・Go・Swift 演算子 `&` | 追加 | R-sized-10 |
| `bor` | （追加）`uint32.bor(a: UInt32, b: UInt32) -> UInt32` | Elixir `bor` / Rust・Go・Swift 演算子 `\|` | 追加 | R-sized-10 |
| `bxor` | （追加）`uint32.bxor(a: UInt32, b: UInt32) -> UInt32` | Elixir `bxor` / Rust・Go・Swift 演算子 `^` | 追加 | R-sized-10 |
| `bnot` | （追加）`uint32.bnot(a: UInt32) -> UInt32` | Elixir `bnot` / Swift `~` | 追加 | R-sized-10 |
| `bshl` | （追加）`uint32.bshl(a: UInt32, n: Int) -> UInt32` | Elixir `bsl` / 演算子 `<<` | 追加 | R-sized-10 |
| `bshr` | （追加）`uint32.bshr(a: UInt32, n: Int) -> UInt32` | Elixir `bsr` / 演算子 `>>` | 追加 | R-sized-10 |

### L3: `uint64`

現行 26 関数と、追加する 9 関数です。

| 関数 | シグネチャ(現行, mut 表示) | 他言語の主な綴り | 判定 | 案 |
|---|---|---|---|---|
| `to_int8` | `uint64.to_int8(x: UInt64) -> Int8` | Rust `as` / Swift `init(truncatingIfNeeded:)` (F) / Kotlin `toByte()` 等 / Go `T(x)` | 撤去（→ `int8.from`） | R-L0-40 |
| `to_int16` | `uint64.to_int16(x: UInt64) -> Int16` | Rust `as` / Swift `init(truncatingIfNeeded:)` (F) / Kotlin `toByte()` 等 / Go `T(x)` | 撤去（→ `int16.from`） | R-L0-40 |
| `to_int32` | `uint64.to_int32(x: UInt64) -> Int32` | Rust `as` / Swift `init(truncatingIfNeeded:)` (F) / Kotlin `toByte()` 等 / Go `T(x)` | 撤去（→ `int32.from`） | R-L0-40 |
| `to_int64` | `uint64.to_int64(x: UInt64) -> Int64` | Rust `as` / Swift `init(truncatingIfNeeded:)` (F) / Kotlin `toByte()` 等 / Go `T(x)` | 撤去（→ `int64.from`） | R-L0-40 |
| `to_uint8` | `uint64.to_uint8(x: UInt64) -> UInt8` | Rust `as` / Swift `init(truncatingIfNeeded:)` (F) / Kotlin `toByte()` 等 / Go `T(x)` | 撤去（→ `uint8.from`） | R-L0-40 |
| `to_uint16` | `uint64.to_uint16(x: UInt64) -> UInt16` | Rust `as` / Swift `init(truncatingIfNeeded:)` (F) / Kotlin `toByte()` 等 / Go `T(x)` | 撤去（→ `uint16.from`） | R-L0-40 |
| `to_uint32` | `uint64.to_uint32(x: UInt64) -> UInt32` | Rust `as` / Swift `init(truncatingIfNeeded:)` (F) / Kotlin `toByte()` 等 / Go `T(x)` | 撤去（→ `uint32.from`） | R-L0-40 |
| `to_float32` | `uint64.to_float32(x: UInt64) -> Float32` | Rust `as` / Kotlin `toDouble()` / Swift `init(_:)` / Gleam `to_float` (F) | 撤去（→ `float32.from`） | R-L0-40 |
| `to_float64` | `uint64.to_float64(x: UInt64) -> Float64` | Rust `as` / Kotlin `toDouble()` / Swift `init(_:)` / Gleam `to_float` (F) | 撤去（→ `float64.from`） | R-L0-40 |
| `to_string` | `uint64.to_string(x: UInt64) -> String` | Rust `to_string` / Kotlin `toString` / Swift `String(x)` / Go `strconv.FormatInt(int64(x),10)` | 据え置き | — |
| `to_int8_checked` | `uint64.to_int8_checked(x: UInt64) -> Option[Int8]` | Rust `try_from` / Swift `init(exactly:)` (F) / OCaml Base `to_int_exn`系 / 他 6 言語 — | 撤去（→ `int8.from_checked`） | R-L0-40 |
| `to_int8_saturating` | `uint64.to_int8_saturating(x: UInt64) -> Int8` | Swift `init(clamping:)` (F) / Kotlin `coerceIn` / 他 7 言語 — | 撤去（→ `int8.from_saturating`） | R-L0-40 |
| `to_int16_checked` | `uint64.to_int16_checked(x: UInt64) -> Option[Int16]` | Rust `try_from` / Swift `init(exactly:)` (F) / OCaml Base `to_int_exn`系 / 他 6 言語 — | 撤去（→ `int16.from_checked`） | R-L0-40 |
| `to_int16_saturating` | `uint64.to_int16_saturating(x: UInt64) -> Int16` | Swift `init(clamping:)` (F) / Kotlin `coerceIn` / 他 7 言語 — | 撤去（→ `int16.from_saturating`） | R-L0-40 |
| `to_int32_checked` | `uint64.to_int32_checked(x: UInt64) -> Option[Int32]` | Rust `try_from` / Swift `init(exactly:)` (F) / OCaml Base `to_int_exn`系 / 他 6 言語 — | 撤去（→ `int32.from_checked`） | R-L0-40 |
| `to_int32_saturating` | `uint64.to_int32_saturating(x: UInt64) -> Int32` | Swift `init(clamping:)` (F) / Kotlin `coerceIn` / 他 7 言語 — | 撤去（→ `int32.from_saturating`） | R-L0-40 |
| `to_int64_checked` | `uint64.to_int64_checked(x: UInt64) -> Option[Int64]` | Rust `try_from` / Swift `init(exactly:)` (F) / OCaml Base `to_int_exn`系 / 他 6 言語 — | 撤去（→ `int64.from_checked`） | R-L0-40 |
| `to_int64_saturating` | `uint64.to_int64_saturating(x: UInt64) -> Int64` | Swift `init(clamping:)` (F) / Kotlin `coerceIn` / 他 7 言語 — | 撤去（→ `int64.from_saturating`） | R-L0-40 |
| `to_uint8_checked` | `uint64.to_uint8_checked(x: UInt64) -> Option[UInt8]` | Rust `try_from` / Swift `init(exactly:)` (F) / OCaml Base `to_int_exn`系 / 他 6 言語 — | 撤去（→ `uint8.from_checked`） | R-L0-40 |
| `to_uint8_saturating` | `uint64.to_uint8_saturating(x: UInt64) -> UInt8` | Swift `init(clamping:)` (F) / Kotlin `coerceIn` / 他 7 言語 — | 撤去（→ `uint8.from_saturating`） | R-L0-40 |
| `to_uint16_checked` | `uint64.to_uint16_checked(x: UInt64) -> Option[UInt16]` | Rust `try_from` / Swift `init(exactly:)` (F) / OCaml Base `to_int_exn`系 / 他 6 言語 — | 撤去（→ `uint16.from_checked`） | R-L0-40 |
| `to_uint16_saturating` | `uint64.to_uint16_saturating(x: UInt64) -> UInt16` | Swift `init(clamping:)` (F) / Kotlin `coerceIn` / 他 7 言語 — | 撤去（→ `uint16.from_saturating`） | R-L0-40 |
| `to_uint32_checked` | `uint64.to_uint32_checked(x: UInt64) -> Option[UInt32]` | Rust `try_from` / Swift `init(exactly:)` (F) / OCaml Base `to_int_exn`系 / 他 6 言語 — | 撤去（→ `uint32.from_checked`） | R-L0-40 |
| `to_uint32_saturating` | `uint64.to_uint32_saturating(x: UInt64) -> UInt32` | Swift `init(clamping:)` (F) / Kotlin `coerceIn` / 他 7 言語 — | 撤去（→ `uint32.from_saturating`） | R-L0-40 |
| `min_value` | `uint64.min_value() -> UInt64` | Rust `MIN` / Go `math.Min…` / Kotlin `MIN_VALUE` / Swift `.min` | 据え置き | — |
| `max_value` | `uint64.max_value() -> UInt64` | Rust `MAX` / Go `math.Max…` / Kotlin `MAX_VALUE` / Swift `.max` | 据え置き | — |
| `parse` | （追加）`uint64.parse(s: String) -> Result[UInt64, String]` | Rust `str::parse::<T>` / Go `ParseInt(s,10,bits)` / OCaml `Int32.of_string_opt` / Kotlin `toByteOrNull` 等 / Swift `T(s)` | 追加 | R-sized-02 |
| `compare` | （追加）`uint64.compare(a: UInt64, b: UInt64) -> Int` | Rust `cmp`/`total_cmp` / Go `cmp.Compare` / Kotlin `compareTo` / Swift `<` | 追加 | R-sized-02 |
| `floor_div` | （追加）`uint64.floor_div(a: UInt64, b: UInt64) -> UInt64` | Rust `div_euclid` / Swift — / Python `//` | 追加 | R-sized-10 |
| `modulo` | （追加）`uint64.modulo(a: UInt64, b: UInt64) -> UInt64` | Rust `rem_euclid` / Python `%` | 追加 | R-sized-10 |
| `pow` | （追加）`uint64.pow(base: UInt64, exp: Int) -> UInt64` | Rust `pow` / Swift — | 追加 | R-sized-10 |
| `is_even` | （追加）`uint64.is_even(n: UInt64) -> Bool` | Swift `isMultiple(of: 2)` / Kotlin — | 追加 | R-sized-10 |
| `is_odd` | （追加）`uint64.is_odd(n: UInt64) -> Bool` | Swift `!isMultiple(of: 2)` | 追加 | R-sized-10 |
| `count_leading_zeros` | （追加）`uint64.count_leading_zeros(n: UInt64) -> Int` | Rust `leading_zeros` / Swift `leadingZeroBitCount` | 追加 | R-sized-10 |
| `count_trailing_zeros` | （追加）`uint64.count_trailing_zeros(n: UInt64) -> Int` | Rust `trailing_zeros` / Swift `trailingZeroBitCount` | 追加 | R-sized-10 |
| `count_ones` | （追加）`uint64.count_ones(n: UInt64) -> Int` | Rust `count_ones` / Swift `nonzeroBitCount` | 追加 | R-sized-10 |
| `byte_swap` | （追加）`uint64.byte_swap(n: UInt64) -> UInt64` | Rust `swap_bytes` / Swift `byteSwapped` | 追加 | R-sized-10 |
| `from` | （追加）`uint64.from[S: Numeric](x: S) -> UInt64` | Rust `From`・`as` / Swift `init(_:)` / Python `int(x)` | 追加 | R-L0-40 |
| `from_checked` | （追加）`uint64.from_checked[S: Numeric](x: S) -> UInt64?` | Rust `TryFrom` / Swift `init(exactly:)` | 追加 | R-L0-40 |
| `from_saturating` | （追加）`uint64.from_saturating[S: Numeric](x: S) -> UInt64` | Swift `init(clamping:)` / Rust `as`（浮動小数点から） | 追加 | R-L0-40 |
| `from_wrapping` | （追加）`uint64.from_wrapping[S: Integer](x: S) -> UInt64` | Swift `init(truncatingIfNeeded:)` / Rust `as` | 追加 | R-L0-40 |
| `wrap_add` | （追加）`uint64.wrap_add(a: UInt64, b: UInt64) -> UInt64` | Swift `&+` / Rust `wrapping_add` | 追加 | R-int-12 |
| `wrap_sub` | （追加）`uint64.wrap_sub(a: UInt64, b: UInt64) -> UInt64` | Swift `&-` / Rust `wrapping_sub` | 追加 | R-int-12 |
| `wrap_mul` | （追加）`uint64.wrap_mul(a: UInt64, b: UInt64) -> UInt64` | Swift `&*` / Rust `wrapping_mul` | 追加 | R-int-12 |
| `rotate_left` | （追加）`uint64.rotate_left(a: UInt64, n: Int) -> UInt64` | Rust `rotate_left` / Go `bits.RotateLeft` | 追加 | R-int-12 |
| `rotate_right` | （追加）`uint64.rotate_right(a: UInt64, n: Int) -> UInt64` | Rust `rotate_right` | 追加 | R-int-12 |
| `bit_length` | （追加）`uint64.bit_length(n: UInt64) -> Int` | Python `bit_length` / Go `bits.Len` | 追加 | R-int-11 |
| `band` | （追加）`uint64.band(a: UInt64, b: UInt64) -> UInt64` | Elixir `band` / Rust・Go・Swift 演算子 `&` | 追加 | R-sized-10 |
| `bor` | （追加）`uint64.bor(a: UInt64, b: UInt64) -> UInt64` | Elixir `bor` / Rust・Go・Swift 演算子 `\|` | 追加 | R-sized-10 |
| `bxor` | （追加）`uint64.bxor(a: UInt64, b: UInt64) -> UInt64` | Elixir `bxor` / Rust・Go・Swift 演算子 `^` | 追加 | R-sized-10 |
| `bnot` | （追加）`uint64.bnot(a: UInt64) -> UInt64` | Elixir `bnot` / Swift `~` | 追加 | R-sized-10 |
| `bshl` | （追加）`uint64.bshl(a: UInt64, n: Int) -> UInt64` | Elixir `bsl` / 演算子 `<<` | 追加 | R-sized-10 |
| `bshr` | （追加）`uint64.bshr(a: UInt64, n: Int) -> UInt64` | Elixir `bsr` / 演算子 `>>` | 追加 | R-sized-10 |

### L3: `float32`

現行 10 関数と、追加する 20 関数です。

| 関数 | シグネチャ(現行, mut 表示) | 他言語の主な綴り | 判定 | 案 |
|---|---|---|---|---|
| `to_int8` | `float32.to_int8(x: Float32) -> Int8` | Rust `as`（飽和） / Kotlin `toInt()`（飽和）(F) / Swift `init(_:)`（trap） / Go `T(x)` | 撤去（→ `int8.from`） | R-L0-40 |
| `to_int16` | `float32.to_int16(x: Float32) -> Int16` | Rust `as`（飽和） / Kotlin `toInt()`（飽和）(F) / Swift `init(_:)`（trap） / Go `T(x)` | 撤去（→ `int16.from`） | R-L0-40 |
| `to_int32` | `float32.to_int32(x: Float32) -> Int32` | Rust `as`（飽和） / Kotlin `toInt()`（飽和）(F) / Swift `init(_:)`（trap） / Go `T(x)` | 撤去（→ `int32.from`） | R-L0-40 |
| `to_int64` | `float32.to_int64(x: Float32) -> Int64` | Rust `as`（飽和） / Kotlin `toInt()`（飽和）(F) / Swift `init(_:)`（trap） / Go `T(x)` | 撤去（→ `int64.from`） | R-L0-40 |
| `to_uint8` | `float32.to_uint8(x: Float32) -> UInt8` | Rust `as`（飽和） / Kotlin `toInt()`（飽和）(F) / Swift `init(_:)`（trap） / Go `T(x)` | 撤去（→ `uint8.from`） | R-L0-40 |
| `to_uint16` | `float32.to_uint16(x: Float32) -> UInt16` | Rust `as`（飽和） / Kotlin `toInt()`（飽和）(F) / Swift `init(_:)`（trap） / Go `T(x)` | 撤去（→ `uint16.from`） | R-L0-40 |
| `to_uint32` | `float32.to_uint32(x: Float32) -> UInt32` | Rust `as`（飽和） / Kotlin `toInt()`（飽和）(F) / Swift `init(_:)`（trap） / Go `T(x)` | 撤去（→ `uint32.from`） | R-L0-40 |
| `to_uint64` | `float32.to_uint64(x: Float32) -> UInt64` | Rust `as`（飽和） / Kotlin `toInt()`（飽和）(F) / Swift `init(_:)`（trap） / Go `T(x)` | 撤去（→ `uint64.from`） | R-L0-40 |
| `to_float64` | `float32.to_float64(x: Float32) -> Float64` | Rust `as` / Kotlin `toFloat()` / Swift `init(_:)` | 撤去（→ `float64.from`） | R-L0-40 |
| `to_string` | `float32.to_string(x: Float32) -> String` | Rust `to_string` / Kotlin `toString` / Swift `String(x)` / Go `strconv.FormatInt(int64(x),10)` | 据え置き | — |
| `parse` | （追加）`float32.parse(s: String) -> Result[Float32, String]` | Rust `str::parse::<T>` / Go `ParseInt(s,10,bits)` / OCaml `Int32.of_string_opt` / Kotlin `toByteOrNull` 等 / Swift `T(s)` | 追加 | R-sized-02 |
| `compare` | （追加）`float32.compare(a: Float32, b: Float32) -> Int`（IEEE totalOrder） | Rust `cmp`/`total_cmp` / Go `cmp.Compare` / Kotlin `compareTo` / Swift `<` | 追加 | R-sized-02 |
| `to_bits` | （追加）`float32.to_bits(x: Float32) -> Int` | Rust `to_bits` (F) / Go `math.Float32bits` / Kotlin `toBits` | 追加 | R-sized-03 |
| `from_bits` | （追加）`float32.from_bits(bits: Int) -> Float32` | Rust `from_bits` (F) / Go `math.Float32frombits` / Kotlin `fromBits` | 追加 | R-sized-03 |
| `infinity` | （追加）`float32.infinity() -> Float32` | Rust `INFINITY` / Swift `.infinity` | 追加 | R-sized-03 |
| `nan` | （追加）`float32.nan() -> Float32` | Rust `NAN` / Swift `.nan` | 追加 | R-sized-03 |
| `epsilon` | （追加）`float32.epsilon() -> Float32` | Rust `EPSILON` / Swift `.ulpOfOne` | 追加 | R-sized-03 |
| `max_value` | （追加）`float32.max_value() -> Float32` | Rust `MAX` / Go `math.MaxFloat32` / Kotlin `MAX_VALUE` | 追加 | R-sized-03 |
| `min_value` | （追加）`float32.min_value() -> Float32` | Rust `MIN` / Swift `-.greatestFiniteMagnitude` | 追加 | R-sized-03 |
| `abs` | （追加）`float32.abs(x: Float32) -> Float32` | Rust `abs` / Swift `magnitude` | 追加 | R-sized-11 |
| `floor` | （追加）`float32.floor(x: Float32) -> Float32` | Rust `floor` / Swift `rounded(.down)` | 追加 | R-sized-11 |
| `ceil` | （追加）`float32.ceil(x: Float32) -> Float32` | Rust `ceil` / Swift `rounded(.up)` | 追加 | R-sized-11 |
| `round` | （追加）`float32.round(x: Float32) -> Float32` | Rust `round` / Swift `rounded()` | 追加 | R-sized-11 |
| `trunc` | （追加）`float32.trunc(x: Float32) -> Float32` | Rust `trunc` / Swift `rounded(.towardZero)` | 追加 | R-sized-11 |
| `signum` | （追加）`float32.signum(x: Float32) -> Float32` | Rust `signum` | 追加 | R-sized-11 |
| `from` | （追加）`float32.from[S: Numeric](x: S) -> Float32` | Rust `From`・`as` / Swift `init(_:)` / Python `int(x)` | 追加 | R-L0-40 |
| `from_checked` | （追加）`float32.from_checked[S: Numeric](x: S) -> Float32?` | Rust `TryFrom` / Swift `init(exactly:)` | 追加 | R-L0-40 |

### L3: `float64`

現行 10 関数と、追加する 21 関数です。

| 関数 | シグネチャ(現行, mut 表示) | 他言語の主な綴り | 判定 | 案 |
|---|---|---|---|---|
| `to_int8` | `float64.to_int8(x: Float64) -> Int8` | Rust `as`（飽和） / Kotlin `toInt()`（飽和）(F) / Swift `init(_:)`（trap） / Go `T(x)` | 撤去（→ `int8.from`） | R-L0-40 |
| `to_int16` | `float64.to_int16(x: Float64) -> Int16` | Rust `as`（飽和） / Kotlin `toInt()`（飽和）(F) / Swift `init(_:)`（trap） / Go `T(x)` | 撤去（→ `int16.from`） | R-L0-40 |
| `to_int32` | `float64.to_int32(x: Float64) -> Int32` | Rust `as`（飽和） / Kotlin `toInt()`（飽和）(F) / Swift `init(_:)`（trap） / Go `T(x)` | 撤去（→ `int32.from`） | R-L0-40 |
| `to_int64` | `float64.to_int64(x: Float64) -> Int64` | Rust `as`（飽和） / Kotlin `toInt()`（飽和）(F) / Swift `init(_:)`（trap） / Go `T(x)` | 撤去（→ `int64.from`） | R-L0-40 |
| `to_uint8` | `float64.to_uint8(x: Float64) -> UInt8` | Rust `as`（飽和） / Kotlin `toInt()`（飽和）(F) / Swift `init(_:)`（trap） / Go `T(x)` | 撤去（→ `uint8.from`） | R-L0-40 |
| `to_uint16` | `float64.to_uint16(x: Float64) -> UInt16` | Rust `as`（飽和） / Kotlin `toInt()`（飽和）(F) / Swift `init(_:)`（trap） / Go `T(x)` | 撤去（→ `uint16.from`） | R-L0-40 |
| `to_uint32` | `float64.to_uint32(x: Float64) -> UInt32` | Rust `as`（飽和） / Kotlin `toInt()`（飽和）(F) / Swift `init(_:)`（trap） / Go `T(x)` | 撤去（→ `uint32.from`） | R-L0-40 |
| `to_uint64` | `float64.to_uint64(x: Float64) -> UInt64` | Rust `as`（飽和） / Kotlin `toInt()`（飽和）(F) / Swift `init(_:)`（trap） / Go `T(x)` | 撤去（→ `uint64.from`） | R-L0-40 |
| `to_float32` | `float64.to_float32(x: Float64) -> Float32` | Rust `as` / Kotlin `toFloat()` / Swift `init(_:)` | 撤去（→ `float32.from`） | R-L0-40 |
| `to_string` | `float64.to_string(x: Float64) -> String` | Rust `to_string` / Kotlin `toString` / Swift `String(x)` / Go `strconv.FormatInt(int64(x),10)` | 据え置き | — |
| `parse` | （追加）`float64.parse(s: String) -> Result[Float64, String]` | Rust `str::parse::<T>` / Go `ParseInt(s,10,bits)` / OCaml `Int32.of_string_opt` / Kotlin `toByteOrNull` 等 / Swift `T(s)` | 追加 | R-sized-02 |
| `compare` | （追加）`float64.compare(a: Float64, b: Float64) -> Int`（IEEE totalOrder） | Rust `cmp`/`total_cmp` / Go `cmp.Compare` / Kotlin `compareTo` / Swift `<` | 追加 | R-sized-02 |
| `to_bits` | （追加）`float64.to_bits(x: Float64) -> Int` | Rust `to_bits` (F) / Go `math.Float64bits` / Kotlin `toBits` | 追加 | R-sized-03 |
| `from_bits` | （追加）`float64.from_bits(bits: Int) -> Float64` | Rust `from_bits` (F) / Go `math.Float64frombits` / Kotlin `fromBits` | 追加 | R-sized-03 |
| `infinity` | （追加）`float64.infinity() -> Float64` | Rust `INFINITY` / Swift `.infinity` | 追加 | R-sized-03 |
| `nan` | （追加）`float64.nan() -> Float64` | Rust `NAN` / Swift `.nan` | 追加 | R-sized-03 |
| `epsilon` | （追加）`float64.epsilon() -> Float64` | Rust `EPSILON` / Swift `.ulpOfOne` | 追加 | R-sized-03 |
| `max_value` | （追加）`float64.max_value() -> Float64` | Rust `MAX` / Go `math.MaxFloat64` / Kotlin `MAX_VALUE` | 追加 | R-sized-03 |
| `min_value` | （追加）`float64.min_value() -> Float64` | Rust `MIN` / Swift `-.greatestFiniteMagnitude` | 追加 | R-sized-03 |
| `parse_radix` | `int8.parse_radix(s: String, radix: Int) -> Result[Int8, String]`（案） | Rust `from_str_radix` / Go `ParseInt`・`ParseUint` | 追加 | R-sized-06 |
| `to_string_radix` | `int8.to_string_radix(x: Int8, radix: Int) -> String`（案） | Rust `format!("{:x}")` / Go `FormatInt`・`FormatUint` | 追加 | R-sized-06 |
| `parse_radix` | `int16.parse_radix(s: String, radix: Int) -> Result[Int16, String]`（案） | Rust `from_str_radix` / Go `ParseInt`・`ParseUint` | 追加 | R-sized-06 |
| `to_string_radix` | `int16.to_string_radix(x: Int16, radix: Int) -> String`（案） | Rust `format!("{:x}")` / Go `FormatInt`・`FormatUint` | 追加 | R-sized-06 |
| `parse_radix` | `int32.parse_radix(s: String, radix: Int) -> Result[Int32, String]`（案） | Rust `from_str_radix` / Go `ParseInt`・`ParseUint` | 追加 | R-sized-06 |
| `to_string_radix` | `int32.to_string_radix(x: Int32, radix: Int) -> String`（案） | Rust `format!("{:x}")` / Go `FormatInt`・`FormatUint` | 追加 | R-sized-06 |
| `parse_radix` | `int64.parse_radix(s: String, radix: Int) -> Result[Int64, String]`（案） | Rust `from_str_radix` / Go `ParseInt`・`ParseUint` | 追加 | R-sized-06 |
| `to_string_radix` | `int64.to_string_radix(x: Int64, radix: Int) -> String`（案） | Rust `format!("{:x}")` / Go `FormatInt`・`FormatUint` | 追加 | R-sized-06 |
| `parse_radix` | `uint8.parse_radix(s: String, radix: Int) -> Result[UInt8, String]`（案） | Rust `from_str_radix` / Go `ParseInt`・`ParseUint` | 追加 | R-sized-06 |
| `to_string_radix` | `uint8.to_string_radix(x: UInt8, radix: Int) -> String`（案） | Rust `format!("{:x}")` / Go `FormatInt`・`FormatUint` | 追加 | R-sized-06 |
| `parse_radix` | `uint16.parse_radix(s: String, radix: Int) -> Result[UInt16, String]`（案） | Rust `from_str_radix` / Go `ParseInt`・`ParseUint` | 追加 | R-sized-06 |
| `to_string_radix` | `uint16.to_string_radix(x: UInt16, radix: Int) -> String`（案） | Rust `format!("{:x}")` / Go `FormatInt`・`FormatUint` | 追加 | R-sized-06 |
| `parse_radix` | `uint32.parse_radix(s: String, radix: Int) -> Result[UInt32, String]`（案） | Rust `from_str_radix` / Go `ParseInt`・`ParseUint` | 追加 | R-sized-06 |
| `to_string_radix` | `uint32.to_string_radix(x: UInt32, radix: Int) -> String`（案） | Rust `format!("{:x}")` / Go `FormatInt`・`FormatUint` | 追加 | R-sized-06 |
| `parse_radix` | `uint64.parse_radix(s: String, radix: Int) -> Result[UInt64, String]`（案） | Rust `from_str_radix` / Go `ParseInt`・`ParseUint` | 追加 | R-sized-06 |
| `to_string_radix` | `uint64.to_string_radix(x: UInt64, radix: Int) -> String`（案） | Rust `format!("{:x}")` / Go `FormatInt`・`FormatUint` | 追加 | R-sized-06 |
| `min` | （追加）`int8.min(a: Int8, b: Int8) -> Int8` | Rust `min` / Swift `min` | 追加 | R-sized-08 |
| `max` | （追加）`int8.max(a: Int8, b: Int8) -> Int8` | Rust `max` / Swift `max` | 追加 | R-sized-08 |
| `clamp` | （追加）`int8.clamp(x: Int8, lo: Int8, hi: Int8) -> Int8` | Rust `clamp` | 追加 | R-sized-08 |
| `min` | （追加）`int16.min(a: Int16, b: Int16) -> Int16` | Rust `min` / Swift `min` | 追加 | R-sized-08 |
| `max` | （追加）`int16.max(a: Int16, b: Int16) -> Int16` | Rust `max` / Swift `max` | 追加 | R-sized-08 |
| `clamp` | （追加）`int16.clamp(x: Int16, lo: Int16, hi: Int16) -> Int16` | Rust `clamp` | 追加 | R-sized-08 |
| `min` | （追加）`int32.min(a: Int32, b: Int32) -> Int32` | Rust `min` / Swift `min` | 追加 | R-sized-08 |
| `max` | （追加）`int32.max(a: Int32, b: Int32) -> Int32` | Rust `max` / Swift `max` | 追加 | R-sized-08 |
| `clamp` | （追加）`int32.clamp(x: Int32, lo: Int32, hi: Int32) -> Int32` | Rust `clamp` | 追加 | R-sized-08 |
| `min` | （追加）`int64.min(a: Int64, b: Int64) -> Int64` | Rust `min` / Swift `min` | 追加 | R-sized-08 |
| `max` | （追加）`int64.max(a: Int64, b: Int64) -> Int64` | Rust `max` / Swift `max` | 追加 | R-sized-08 |
| `clamp` | （追加）`int64.clamp(x: Int64, lo: Int64, hi: Int64) -> Int64` | Rust `clamp` | 追加 | R-sized-08 |
| `min` | （追加）`uint8.min(a: UInt8, b: UInt8) -> UInt8` | Rust `min` / Swift `min` | 追加 | R-sized-08 |
| `max` | （追加）`uint8.max(a: UInt8, b: UInt8) -> UInt8` | Rust `max` / Swift `max` | 追加 | R-sized-08 |
| `clamp` | （追加）`uint8.clamp(x: UInt8, lo: UInt8, hi: UInt8) -> UInt8` | Rust `clamp` | 追加 | R-sized-08 |
| `min` | （追加）`uint16.min(a: UInt16, b: UInt16) -> UInt16` | Rust `min` / Swift `min` | 追加 | R-sized-08 |
| `max` | （追加）`uint16.max(a: UInt16, b: UInt16) -> UInt16` | Rust `max` / Swift `max` | 追加 | R-sized-08 |
| `clamp` | （追加）`uint16.clamp(x: UInt16, lo: UInt16, hi: UInt16) -> UInt16` | Rust `clamp` | 追加 | R-sized-08 |
| `min` | （追加）`uint32.min(a: UInt32, b: UInt32) -> UInt32` | Rust `min` / Swift `min` | 追加 | R-sized-08 |
| `max` | （追加）`uint32.max(a: UInt32, b: UInt32) -> UInt32` | Rust `max` / Swift `max` | 追加 | R-sized-08 |
| `clamp` | （追加）`uint32.clamp(x: UInt32, lo: UInt32, hi: UInt32) -> UInt32` | Rust `clamp` | 追加 | R-sized-08 |
| `min` | （追加）`uint64.min(a: UInt64, b: UInt64) -> UInt64` | Rust `min` / Swift `min` | 追加 | R-sized-08 |
| `max` | （追加）`uint64.max(a: UInt64, b: UInt64) -> UInt64` | Rust `max` / Swift `max` | 追加 | R-sized-08 |
| `clamp` | （追加）`uint64.clamp(x: UInt64, lo: UInt64, hi: UInt64) -> UInt64` | Rust `clamp` | 追加 | R-sized-08 |
| `min` | （追加）`float32.min(a: Float32, b: Float32) -> Float32` | Rust `min` / Swift `min` | 追加 | R-sized-08 |
| `max` | （追加）`float32.max(a: Float32, b: Float32) -> Float32` | Rust `max` / Swift `max` | 追加 | R-sized-08 |
| `clamp` | （追加）`float32.clamp(x: Float32, lo: Float32, hi: Float32) -> Float32` | Rust `clamp` | 追加 | R-sized-08 |
| `is_nan` | （追加）`float32.is_nan(x: Float32) -> Bool` | Rust `is_nan` / Swift `.isNaN` | 追加 | R-sized-08 |
| `is_infinite` | （追加）`float32.is_infinite(x: Float32) -> Bool` | Rust `is_infinite` / Swift `.isInFinite` | 追加 | R-sized-08 |
| `is_finite` | （追加）`float32.is_finite(x: Float32) -> Bool` | Rust `is_finite` / Swift `.isFinite` | 追加 | R-sized-08 |
| `min` | （追加）`float64.min(a: Float64, b: Float64) -> Float64` | Rust `min` / Swift `min` | 追加 | R-sized-08 |
| `max` | （追加）`float64.max(a: Float64, b: Float64) -> Float64` | Rust `max` / Swift `max` | 追加 | R-sized-08 |
| `clamp` | （追加）`float64.clamp(x: Float64, lo: Float64, hi: Float64) -> Float64` | Rust `clamp` | 追加 | R-sized-08 |
| `is_nan` | （追加）`float64.is_nan(x: Float64) -> Bool` | Rust `is_nan` / Swift `.isNaN` | 追加 | R-sized-08 |
| `is_infinite` | （追加）`float64.is_infinite(x: Float64) -> Bool` | Rust `is_infinite` / Swift `.isInFinite` | 追加 | R-sized-08 |
| `is_finite` | （追加）`float64.is_finite(x: Float64) -> Bool` | Rust `is_finite` / Swift `.isFinite` | 追加 | R-sized-08 |
| `abs` | （追加）`float64.abs(x: Float64) -> Float64` | Rust `abs` / Swift `magnitude` | 追加 | R-sized-11 |
| `floor` | （追加）`float64.floor(x: Float64) -> Float64` | Rust `floor` / Swift `rounded(.down)` | 追加 | R-sized-11 |
| `ceil` | （追加）`float64.ceil(x: Float64) -> Float64` | Rust `ceil` / Swift `rounded(.up)` | 追加 | R-sized-11 |
| `round` | （追加）`float64.round(x: Float64) -> Float64` | Rust `round` / Swift `rounded()` | 追加 | R-sized-11 |
| `trunc` | （追加）`float64.trunc(x: Float64) -> Float64` | Rust `trunc` / Swift `rounded(.towardZero)` | 追加 | R-sized-11 |
| `signum` | （追加）`float64.signum(x: Float64) -> Float64` | Rust `signum` | 追加 | R-sized-11 |
| `from` | （追加）`float64.from[S: Numeric](x: S) -> Float64` | Rust `From`・`as` / Swift `init(_:)` / Python `int(x)` | 追加 | R-L0-40 |
| `from_checked` | （追加）`float64.from_checked[S: Numeric](x: S) -> Float64?` | Rust `TryFrom` / Swift `init(exactly:)` | 追加 | R-L0-40 |
