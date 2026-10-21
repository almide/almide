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
| 損失ありの変換（整数は折り返し） | `to_<型>` | `as` | `T(x)` | — | `asIntN` | — | — | `of_int` | `toByte()` 等 | `init(truncatingIfNeeded:)` (F) |
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

### R-sized-04 `float.to_<整数>_saturating` 8 関数を撤去する — ○ 採用

- 内容: `float.to_int8_saturating` … `float.to_uint64_saturating` を撤去します。`float.to_int8` などがすでに同じ飽和をします。
- 根拠の L0 規則: R-L0-01（別の公開関数を 1 回呼ぶだけの公開関数も同義語に数える）。いまは `tests/stdlib_synonym_gate_test.rs:47-59` の許容リストで通しているので、この撤去で許容リストが 8 行減ります（減る方向の変更）。
- (a) Rust の `as`、Kotlin の `toInt()` は浮動小数点 → 整数で飽和する 1 つの綴りだけを持ちます。
- (c) 同じ結果の綴りが 2 つあると、「`to_int8` は飽和しない（折り返す）」と誤って読む余地が残ります。完全性の規則 4 に「浮動小数点 → 整数の plain は飽和する」と書くほうが、規則として当たります。
- (d) 撤去 8。`@deprecated(since=15, use="float.to_int8")` など、removal epoch 16。`almide fix`: `_saturating` を外すだけ。使用数: `to_int32_saturating` 5/0/3/0、`to_uint16_saturating` 4/0/3/0、ほかの 6 関数は 0/0/3/0。
- ゲート案: 行列のゲートで、浮動小数点 → 整数の `_saturating` を「存在してはならない」側に置きます。同義語ゲートの許容リストから 8 行を消します。
- issue: #3935

### R-sized-05 固定幅のモジュールをまとめる・算術関数を足す — × 不採用

- 内容: モジュールを統合しません（R-L1-20）。`int8.abs`、`uint32.max` などの算術関数も足しません（完全性の規則 8）。
- 根拠の L0 規則: R-L0-02（主語の型のモジュールに置く。Almide にはメソッドがないので、型ごとのモジュールになる）、R-L0-01（`Int` の関数と同じ計算の複製を 10 倍に増やさない）。
- (c) 固定幅の型で算術がしたいときの書き方は「`to_int` してから `int.*`」の 1 通りにします。行列の `to_int` がそろう（R-sized-01）ので、この書き方がどの型でも同じ綴りになります。
- issue: なし

## L3: 関数表

### L3: `int8`

現行 20 関数と、追加する 4 関数です。

| 関数 | シグネチャ(現行, mut 表示) | 他言語の主な綴り | 判定 | 案 |
|---|---|---|---|---|
| `to_int16` | `int8.to_int16(x: Int8) -> Int16` | Rust `from`/`into` / Swift `init(_:)` / Kotlin `toLong()` 等 / Go `T(x)` | 据え置き | R-sized-01（行列のセル） |
| `to_int32` | `int8.to_int32(x: Int8) -> Int32` | Rust `from`/`into` / Swift `init(_:)` / Kotlin `toLong()` 等 / Go `T(x)` | 据え置き | R-sized-01（行列のセル） |
| `to_int64` | `int8.to_int64(x: Int8) -> Int64` | Rust `from`/`into` / Swift `init(_:)` / Kotlin `toLong()` 等 / Go `T(x)` | 据え置き | R-sized-01（行列のセル） |
| `to_uint8` | `int8.to_uint8(x: Int8) -> UInt8` | Rust `as` / Swift `init(truncatingIfNeeded:)` (F) / Kotlin `toByte()` 等 / Go `T(x)` | 据え置き | R-sized-01（行列のセル） |
| `to_uint16` | `int8.to_uint16(x: Int8) -> UInt16` | Rust `as` / Swift `init(truncatingIfNeeded:)` (F) / Kotlin `toByte()` 等 / Go `T(x)` | 据え置き | R-sized-01（行列のセル） |
| `to_uint32` | `int8.to_uint32(x: Int8) -> UInt32` | Rust `as` / Swift `init(truncatingIfNeeded:)` (F) / Kotlin `toByte()` 等 / Go `T(x)` | 据え置き | R-sized-01（行列のセル） |
| `to_uint64` | `int8.to_uint64(x: Int8) -> UInt64` | Rust `as` / Swift `init(truncatingIfNeeded:)` (F) / Kotlin `toByte()` 等 / Go `T(x)` | 据え置き | R-sized-01（行列のセル） |
| `to_float32` | `int8.to_float32(x: Int8) -> Float32` | Rust `as` / Kotlin `toDouble()` / Swift `init(_:)` / Gleam `to_float` (F) | 据え置き | R-sized-01（行列のセル） |
| `to_float64` | `int8.to_float64(x: Int8) -> Float64` | Rust `as` / Kotlin `toDouble()` / Swift `init(_:)` / Gleam `to_float` (F) | 据え置き | R-sized-01（行列のセル） |
| `to_string` | `int8.to_string(x: Int8) -> String` | Rust `to_string` / Kotlin `toString` / Swift `String(x)` / Go `strconv.FormatInt(int64(x),10)` | 据え置き | — |
| `to_uint8_checked` | `int8.to_uint8_checked(x: Int8) -> Option[UInt8]` | Rust `try_from` / Swift `init(exactly:)` (F) / OCaml Base `to_int_exn`系 / 他 6 言語 — | 据え置き | R-sized-01（行列のセル） |
| `to_uint8_saturating` | `int8.to_uint8_saturating(x: Int8) -> UInt8` | Swift `init(clamping:)` (F) / Kotlin `coerceIn` / 他 7 言語 — | 据え置き | R-sized-01（行列のセル） |
| `to_uint16_checked` | `int8.to_uint16_checked(x: Int8) -> Option[UInt16]` | Rust `try_from` / Swift `init(exactly:)` (F) / OCaml Base `to_int_exn`系 / 他 6 言語 — | 据え置き | R-sized-01（行列のセル） |
| `to_uint16_saturating` | `int8.to_uint16_saturating(x: Int8) -> UInt16` | Swift `init(clamping:)` (F) / Kotlin `coerceIn` / 他 7 言語 — | 据え置き | R-sized-01（行列のセル） |
| `to_uint32_checked` | `int8.to_uint32_checked(x: Int8) -> Option[UInt32]` | Rust `try_from` / Swift `init(exactly:)` (F) / OCaml Base `to_int_exn`系 / 他 6 言語 — | 据え置き | R-sized-01（行列のセル） |
| `to_uint32_saturating` | `int8.to_uint32_saturating(x: Int8) -> UInt32` | Swift `init(clamping:)` (F) / Kotlin `coerceIn` / 他 7 言語 — | 据え置き | R-sized-01（行列のセル） |
| `to_uint64_checked` | `int8.to_uint64_checked(x: Int8) -> Option[UInt64]` | Rust `try_from` / Swift `init(exactly:)` (F) / OCaml Base `to_int_exn`系 / 他 6 言語 — | 据え置き | R-sized-01（行列のセル） |
| `to_uint64_saturating` | `int8.to_uint64_saturating(x: Int8) -> UInt64` | Swift `init(clamping:)` (F) / Kotlin `coerceIn` / 他 7 言語 — | 据え置き | R-sized-01（行列のセル） |
| `min_value` | `int8.min_value() -> Int8` | Rust `MIN` / Go `math.Min…` / Kotlin `MIN_VALUE` / Swift `.min` | 据え置き | — |
| `max_value` | `int8.max_value() -> Int8` | Rust `MAX` / Go `math.Max…` / Kotlin `MAX_VALUE` / Swift `.max` | 据え置き | — |
| `to_int` | （追加）`int8.to_int(x: Int8) -> Int` | Rust `from`/`into` / Swift `init(_:)` / Kotlin `toLong()` 等 / Go `T(x)` | 追加 | R-sized-01 |
| `to_float` | （追加）`int8.to_float(x: Int8) -> Float` | Rust `as` / Kotlin `toDouble()` / Swift `init(_:)` / Gleam `to_float` (F) | 追加 | R-sized-01 |
| `parse` | （追加）`int8.parse(s: String) -> Result[Int8, String]` | Rust `str::parse::<T>` / Go `ParseInt(s,10,bits)` / OCaml `Int32.of_string_opt` / Kotlin `toByteOrNull` 等 / Swift `T(s)` | 追加 | R-sized-02 |
| `compare` | （追加）`int8.compare(a: Int8, b: Int8) -> Int` | Rust `cmp`/`total_cmp` / Go `cmp.Compare` / Kotlin `compareTo` / Swift `<` | 追加 | R-sized-02 |

### L3: `int16`

現行 22 関数と、追加する 4 関数です。

| 関数 | シグネチャ(現行, mut 表示) | 他言語の主な綴り | 判定 | 案 |
|---|---|---|---|---|
| `to_int8` | `int16.to_int8(x: Int16) -> Int8` | Rust `as` / Swift `init(truncatingIfNeeded:)` (F) / Kotlin `toByte()` 等 / Go `T(x)` | 据え置き | R-sized-01（行列のセル） |
| `to_int32` | `int16.to_int32(x: Int16) -> Int32` | Rust `from`/`into` / Swift `init(_:)` / Kotlin `toLong()` 等 / Go `T(x)` | 据え置き | R-sized-01（行列のセル） |
| `to_int64` | `int16.to_int64(x: Int16) -> Int64` | Rust `from`/`into` / Swift `init(_:)` / Kotlin `toLong()` 等 / Go `T(x)` | 据え置き | R-sized-01（行列のセル） |
| `to_uint8` | `int16.to_uint8(x: Int16) -> UInt8` | Rust `as` / Swift `init(truncatingIfNeeded:)` (F) / Kotlin `toByte()` 等 / Go `T(x)` | 据え置き | R-sized-01（行列のセル） |
| `to_uint16` | `int16.to_uint16(x: Int16) -> UInt16` | Rust `as` / Swift `init(truncatingIfNeeded:)` (F) / Kotlin `toByte()` 等 / Go `T(x)` | 据え置き | R-sized-01（行列のセル） |
| `to_uint32` | `int16.to_uint32(x: Int16) -> UInt32` | Rust `as` / Swift `init(truncatingIfNeeded:)` (F) / Kotlin `toByte()` 等 / Go `T(x)` | 据え置き | R-sized-01（行列のセル） |
| `to_uint64` | `int16.to_uint64(x: Int16) -> UInt64` | Rust `as` / Swift `init(truncatingIfNeeded:)` (F) / Kotlin `toByte()` 等 / Go `T(x)` | 据え置き | R-sized-01（行列のセル） |
| `to_float32` | `int16.to_float32(x: Int16) -> Float32` | Rust `as` / Kotlin `toDouble()` / Swift `init(_:)` / Gleam `to_float` (F) | 据え置き | R-sized-01（行列のセル） |
| `to_float64` | `int16.to_float64(x: Int16) -> Float64` | Rust `as` / Kotlin `toDouble()` / Swift `init(_:)` / Gleam `to_float` (F) | 据え置き | R-sized-01（行列のセル） |
| `to_string` | `int16.to_string(x: Int16) -> String` | Rust `to_string` / Kotlin `toString` / Swift `String(x)` / Go `strconv.FormatInt(int64(x),10)` | 据え置き | — |
| `to_int8_checked` | `int16.to_int8_checked(x: Int16) -> Option[Int8]` | Rust `try_from` / Swift `init(exactly:)` (F) / OCaml Base `to_int_exn`系 / 他 6 言語 — | 据え置き | R-sized-01（行列のセル） |
| `to_int8_saturating` | `int16.to_int8_saturating(x: Int16) -> Int8` | Swift `init(clamping:)` (F) / Kotlin `coerceIn` / 他 7 言語 — | 据え置き | R-sized-01（行列のセル） |
| `to_uint8_checked` | `int16.to_uint8_checked(x: Int16) -> Option[UInt8]` | Rust `try_from` / Swift `init(exactly:)` (F) / OCaml Base `to_int_exn`系 / 他 6 言語 — | 据え置き | R-sized-01（行列のセル） |
| `to_uint8_saturating` | `int16.to_uint8_saturating(x: Int16) -> UInt8` | Swift `init(clamping:)` (F) / Kotlin `coerceIn` / 他 7 言語 — | 据え置き | R-sized-01（行列のセル） |
| `to_uint16_checked` | `int16.to_uint16_checked(x: Int16) -> Option[UInt16]` | Rust `try_from` / Swift `init(exactly:)` (F) / OCaml Base `to_int_exn`系 / 他 6 言語 — | 据え置き | R-sized-01（行列のセル） |
| `to_uint16_saturating` | `int16.to_uint16_saturating(x: Int16) -> UInt16` | Swift `init(clamping:)` (F) / Kotlin `coerceIn` / 他 7 言語 — | 据え置き | R-sized-01（行列のセル） |
| `to_uint32_checked` | `int16.to_uint32_checked(x: Int16) -> Option[UInt32]` | Rust `try_from` / Swift `init(exactly:)` (F) / OCaml Base `to_int_exn`系 / 他 6 言語 — | 据え置き | R-sized-01（行列のセル） |
| `to_uint32_saturating` | `int16.to_uint32_saturating(x: Int16) -> UInt32` | Swift `init(clamping:)` (F) / Kotlin `coerceIn` / 他 7 言語 — | 据え置き | R-sized-01（行列のセル） |
| `to_uint64_checked` | `int16.to_uint64_checked(x: Int16) -> Option[UInt64]` | Rust `try_from` / Swift `init(exactly:)` (F) / OCaml Base `to_int_exn`系 / 他 6 言語 — | 据え置き | R-sized-01（行列のセル） |
| `to_uint64_saturating` | `int16.to_uint64_saturating(x: Int16) -> UInt64` | Swift `init(clamping:)` (F) / Kotlin `coerceIn` / 他 7 言語 — | 据え置き | R-sized-01（行列のセル） |
| `min_value` | `int16.min_value() -> Int16` | Rust `MIN` / Go `math.Min…` / Kotlin `MIN_VALUE` / Swift `.min` | 据え置き | — |
| `max_value` | `int16.max_value() -> Int16` | Rust `MAX` / Go `math.Max…` / Kotlin `MAX_VALUE` / Swift `.max` | 据え置き | — |
| `to_int` | （追加）`int16.to_int(x: Int16) -> Int` | Rust `from`/`into` / Swift `init(_:)` / Kotlin `toLong()` 等 / Go `T(x)` | 追加 | R-sized-01 |
| `to_float` | （追加）`int16.to_float(x: Int16) -> Float` | Rust `as` / Kotlin `toDouble()` / Swift `init(_:)` / Gleam `to_float` (F) | 追加 | R-sized-01 |
| `parse` | （追加）`int16.parse(s: String) -> Result[Int16, String]` | Rust `str::parse::<T>` / Go `ParseInt(s,10,bits)` / OCaml `Int32.of_string_opt` / Kotlin `toByteOrNull` 等 / Swift `T(s)` | 追加 | R-sized-02 |
| `compare` | （追加）`int16.compare(a: Int16, b: Int16) -> Int` | Rust `cmp`/`total_cmp` / Go `cmp.Compare` / Kotlin `compareTo` / Swift `<` | 追加 | R-sized-02 |

### L3: `int32`

現行 24 関数と、追加する 5 関数です。

| 関数 | シグネチャ(現行, mut 表示) | 他言語の主な綴り | 判定 | 案 |
|---|---|---|---|---|
| `to_int8` | `int32.to_int8(x: Int32) -> Int8` | Rust `as` / Swift `init(truncatingIfNeeded:)` (F) / Kotlin `toByte()` 等 / Go `T(x)` | 据え置き | R-sized-01（行列のセル） |
| `to_int16` | `int32.to_int16(x: Int32) -> Int16` | Rust `as` / Swift `init(truncatingIfNeeded:)` (F) / Kotlin `toByte()` 等 / Go `T(x)` | 据え置き | R-sized-01（行列のセル） |
| `to_int64` | `int32.to_int64(x: Int32) -> Int64` | Rust `from`/`into` / Swift `init(_:)` / Kotlin `toLong()` 等 / Go `T(x)` | 据え置き | R-sized-01（行列のセル） |
| `to_uint8` | `int32.to_uint8(x: Int32) -> UInt8` | Rust `as` / Swift `init(truncatingIfNeeded:)` (F) / Kotlin `toByte()` 等 / Go `T(x)` | 据え置き | R-sized-01（行列のセル） |
| `to_uint16` | `int32.to_uint16(x: Int32) -> UInt16` | Rust `as` / Swift `init(truncatingIfNeeded:)` (F) / Kotlin `toByte()` 等 / Go `T(x)` | 据え置き | R-sized-01（行列のセル） |
| `to_uint32` | `int32.to_uint32(x: Int32) -> UInt32` | Rust `as` / Swift `init(truncatingIfNeeded:)` (F) / Kotlin `toByte()` 等 / Go `T(x)` | 据え置き | R-sized-01（行列のセル） |
| `to_uint64` | `int32.to_uint64(x: Int32) -> UInt64` | Rust `as` / Swift `init(truncatingIfNeeded:)` (F) / Kotlin `toByte()` 等 / Go `T(x)` | 据え置き | R-sized-01（行列のセル） |
| `to_float32` | `int32.to_float32(x: Int32) -> Float32` | Rust `as` / Kotlin `toDouble()` / Swift `init(_:)` / Gleam `to_float` (F) | 据え置き | R-sized-01（行列のセル） |
| `to_float64` | `int32.to_float64(x: Int32) -> Float64` | Rust `as` / Kotlin `toDouble()` / Swift `init(_:)` / Gleam `to_float` (F) | 据え置き | R-sized-01（行列のセル） |
| `to_string` | `int32.to_string(x: Int32) -> String` | Rust `to_string` / Kotlin `toString` / Swift `String(x)` / Go `strconv.FormatInt(int64(x),10)` | 据え置き | — |
| `to_int8_checked` | `int32.to_int8_checked(x: Int32) -> Option[Int8]` | Rust `try_from` / Swift `init(exactly:)` (F) / OCaml Base `to_int_exn`系 / 他 6 言語 — | 据え置き | R-sized-01（行列のセル） |
| `to_int8_saturating` | `int32.to_int8_saturating(x: Int32) -> Int8` | Swift `init(clamping:)` (F) / Kotlin `coerceIn` / 他 7 言語 — | 据え置き | R-sized-01（行列のセル） |
| `to_int16_checked` | `int32.to_int16_checked(x: Int32) -> Option[Int16]` | Rust `try_from` / Swift `init(exactly:)` (F) / OCaml Base `to_int_exn`系 / 他 6 言語 — | 据え置き | R-sized-01（行列のセル） |
| `to_int16_saturating` | `int32.to_int16_saturating(x: Int32) -> Int16` | Swift `init(clamping:)` (F) / Kotlin `coerceIn` / 他 7 言語 — | 据え置き | R-sized-01（行列のセル） |
| `to_uint8_checked` | `int32.to_uint8_checked(x: Int32) -> Option[UInt8]` | Rust `try_from` / Swift `init(exactly:)` (F) / OCaml Base `to_int_exn`系 / 他 6 言語 — | 据え置き | R-sized-01（行列のセル） |
| `to_uint8_saturating` | `int32.to_uint8_saturating(x: Int32) -> UInt8` | Swift `init(clamping:)` (F) / Kotlin `coerceIn` / 他 7 言語 — | 据え置き | R-sized-01（行列のセル） |
| `to_uint16_checked` | `int32.to_uint16_checked(x: Int32) -> Option[UInt16]` | Rust `try_from` / Swift `init(exactly:)` (F) / OCaml Base `to_int_exn`系 / 他 6 言語 — | 据え置き | R-sized-01（行列のセル） |
| `to_uint16_saturating` | `int32.to_uint16_saturating(x: Int32) -> UInt16` | Swift `init(clamping:)` (F) / Kotlin `coerceIn` / 他 7 言語 — | 据え置き | R-sized-01（行列のセル） |
| `to_uint32_checked` | `int32.to_uint32_checked(x: Int32) -> Option[UInt32]` | Rust `try_from` / Swift `init(exactly:)` (F) / OCaml Base `to_int_exn`系 / 他 6 言語 — | 据え置き | R-sized-01（行列のセル） |
| `to_uint32_saturating` | `int32.to_uint32_saturating(x: Int32) -> UInt32` | Swift `init(clamping:)` (F) / Kotlin `coerceIn` / 他 7 言語 — | 据え置き | R-sized-01（行列のセル） |
| `to_uint64_checked` | `int32.to_uint64_checked(x: Int32) -> Option[UInt64]` | Rust `try_from` / Swift `init(exactly:)` (F) / OCaml Base `to_int_exn`系 / 他 6 言語 — | 据え置き | R-sized-01（行列のセル） |
| `to_uint64_saturating` | `int32.to_uint64_saturating(x: Int32) -> UInt64` | Swift `init(clamping:)` (F) / Kotlin `coerceIn` / 他 7 言語 — | 据え置き | R-sized-01（行列のセル） |
| `min_value` | `int32.min_value() -> Int32` | Rust `MIN` / Go `math.Min…` / Kotlin `MIN_VALUE` / Swift `.min` | 据え置き | — |
| `max_value` | `int32.max_value() -> Int32` | Rust `MAX` / Go `math.Max…` / Kotlin `MAX_VALUE` / Swift `.max` | 据え置き | — |
| `to_int` | （追加）`int32.to_int(x: Int32) -> Int` | Rust `from`/`into` / Swift `init(_:)` / Kotlin `toLong()` 等 / Go `T(x)` | 追加 | R-sized-01 |
| `to_float` | （追加）`int32.to_float(x: Int32) -> Float` | Rust `as` / Kotlin `toDouble()` / Swift `init(_:)` / Gleam `to_float` (F) | 追加 | R-sized-01 |
| `to_float32_checked` | （追加）`int32.to_float32_checked(x: Int32) -> Option[Float32]` | Rust `try_from` / Swift `init(exactly:)` (F) / OCaml Base `to_int_exn`系 / 他 6 言語 — | 追加 | R-sized-01 |
| `parse` | （追加）`int32.parse(s: String) -> Result[Int32, String]` | Rust `str::parse::<T>` / Go `ParseInt(s,10,bits)` / OCaml `Int32.of_string_opt` / Kotlin `toByteOrNull` 等 / Swift `T(s)` | 追加 | R-sized-02 |
| `compare` | （追加）`int32.compare(a: Int32, b: Int32) -> Int` | Rust `cmp`/`total_cmp` / Go `cmp.Compare` / Kotlin `compareTo` / Swift `<` | 追加 | R-sized-02 |

### L3: `int64`

現行 26 関数と、追加する 7 関数です。

| 関数 | シグネチャ(現行, mut 表示) | 他言語の主な綴り | 判定 | 案 |
|---|---|---|---|---|
| `to_int8` | `int64.to_int8(x: Int64) -> Int8` | Rust `as` / Swift `init(truncatingIfNeeded:)` (F) / Kotlin `toByte()` 等 / Go `T(x)` | 据え置き | R-sized-01（行列のセル） |
| `to_int16` | `int64.to_int16(x: Int64) -> Int16` | Rust `as` / Swift `init(truncatingIfNeeded:)` (F) / Kotlin `toByte()` 等 / Go `T(x)` | 据え置き | R-sized-01（行列のセル） |
| `to_int32` | `int64.to_int32(x: Int64) -> Int32` | Rust `as` / Swift `init(truncatingIfNeeded:)` (F) / Kotlin `toByte()` 等 / Go `T(x)` | 据え置き | R-sized-01（行列のセル） |
| `to_uint8` | `int64.to_uint8(x: Int64) -> UInt8` | Rust `as` / Swift `init(truncatingIfNeeded:)` (F) / Kotlin `toByte()` 等 / Go `T(x)` | 据え置き | R-sized-01（行列のセル） |
| `to_uint16` | `int64.to_uint16(x: Int64) -> UInt16` | Rust `as` / Swift `init(truncatingIfNeeded:)` (F) / Kotlin `toByte()` 等 / Go `T(x)` | 据え置き | R-sized-01（行列のセル） |
| `to_uint32` | `int64.to_uint32(x: Int64) -> UInt32` | Rust `as` / Swift `init(truncatingIfNeeded:)` (F) / Kotlin `toByte()` 等 / Go `T(x)` | 据え置き | R-sized-01（行列のセル） |
| `to_uint64` | `int64.to_uint64(x: Int64) -> UInt64` | Rust `as` / Swift `init(truncatingIfNeeded:)` (F) / Kotlin `toByte()` 等 / Go `T(x)` | 据え置き | R-sized-01（行列のセル） |
| `to_float32` | `int64.to_float32(x: Int64) -> Float32` | Rust `as` / Kotlin `toDouble()` / Swift `init(_:)` / Gleam `to_float` (F) | 据え置き | R-sized-01（行列のセル） |
| `to_float64` | `int64.to_float64(x: Int64) -> Float64` | Rust `as` / Kotlin `toDouble()` / Swift `init(_:)` / Gleam `to_float` (F) | 据え置き | R-sized-01（行列のセル） |
| `to_string` | `int64.to_string(x: Int64) -> String` | Rust `to_string` / Kotlin `toString` / Swift `String(x)` / Go `strconv.FormatInt(int64(x),10)` | 据え置き | — |
| `to_int8_checked` | `int64.to_int8_checked(x: Int64) -> Option[Int8]` | Rust `try_from` / Swift `init(exactly:)` (F) / OCaml Base `to_int_exn`系 / 他 6 言語 — | 据え置き | R-sized-01（行列のセル） |
| `to_int8_saturating` | `int64.to_int8_saturating(x: Int64) -> Int8` | Swift `init(clamping:)` (F) / Kotlin `coerceIn` / 他 7 言語 — | 据え置き | R-sized-01（行列のセル） |
| `to_int16_checked` | `int64.to_int16_checked(x: Int64) -> Option[Int16]` | Rust `try_from` / Swift `init(exactly:)` (F) / OCaml Base `to_int_exn`系 / 他 6 言語 — | 据え置き | R-sized-01（行列のセル） |
| `to_int16_saturating` | `int64.to_int16_saturating(x: Int64) -> Int16` | Swift `init(clamping:)` (F) / Kotlin `coerceIn` / 他 7 言語 — | 据え置き | R-sized-01（行列のセル） |
| `to_int32_checked` | `int64.to_int32_checked(x: Int64) -> Option[Int32]` | Rust `try_from` / Swift `init(exactly:)` (F) / OCaml Base `to_int_exn`系 / 他 6 言語 — | 据え置き | R-sized-01（行列のセル） |
| `to_int32_saturating` | `int64.to_int32_saturating(x: Int64) -> Int32` | Swift `init(clamping:)` (F) / Kotlin `coerceIn` / 他 7 言語 — | 据え置き | R-sized-01（行列のセル） |
| `to_uint8_checked` | `int64.to_uint8_checked(x: Int64) -> Option[UInt8]` | Rust `try_from` / Swift `init(exactly:)` (F) / OCaml Base `to_int_exn`系 / 他 6 言語 — | 据え置き | R-sized-01（行列のセル） |
| `to_uint8_saturating` | `int64.to_uint8_saturating(x: Int64) -> UInt8` | Swift `init(clamping:)` (F) / Kotlin `coerceIn` / 他 7 言語 — | 据え置き | R-sized-01（行列のセル） |
| `to_uint16_checked` | `int64.to_uint16_checked(x: Int64) -> Option[UInt16]` | Rust `try_from` / Swift `init(exactly:)` (F) / OCaml Base `to_int_exn`系 / 他 6 言語 — | 据え置き | R-sized-01（行列のセル） |
| `to_uint16_saturating` | `int64.to_uint16_saturating(x: Int64) -> UInt16` | Swift `init(clamping:)` (F) / Kotlin `coerceIn` / 他 7 言語 — | 据え置き | R-sized-01（行列のセル） |
| `to_uint32_checked` | `int64.to_uint32_checked(x: Int64) -> Option[UInt32]` | Rust `try_from` / Swift `init(exactly:)` (F) / OCaml Base `to_int_exn`系 / 他 6 言語 — | 据え置き | R-sized-01（行列のセル） |
| `to_uint32_saturating` | `int64.to_uint32_saturating(x: Int64) -> UInt32` | Swift `init(clamping:)` (F) / Kotlin `coerceIn` / 他 7 言語 — | 据え置き | R-sized-01（行列のセル） |
| `to_uint64_checked` | `int64.to_uint64_checked(x: Int64) -> Option[UInt64]` | Rust `try_from` / Swift `init(exactly:)` (F) / OCaml Base `to_int_exn`系 / 他 6 言語 — | 据え置き | R-sized-01（行列のセル） |
| `to_uint64_saturating` | `int64.to_uint64_saturating(x: Int64) -> UInt64` | Swift `init(clamping:)` (F) / Kotlin `coerceIn` / 他 7 言語 — | 据え置き | R-sized-01（行列のセル） |
| `min_value` | `int64.min_value() -> Int64` | Rust `MIN` / Go `math.Min…` / Kotlin `MIN_VALUE` / Swift `.min` | 据え置き | — |
| `max_value` | `int64.max_value() -> Int64` | Rust `MAX` / Go `math.Max…` / Kotlin `MAX_VALUE` / Swift `.max` | 据え置き | — |
| `to_int` | （追加）`int64.to_int(x: Int64) -> Int` | Rust `from`/`into` / Swift `init(_:)` / Kotlin `toLong()` 等 / Go `T(x)` | 追加 | R-sized-01 |
| `to_float` | （追加）`int64.to_float(x: Int64) -> Float` | Rust `as` / Kotlin `toDouble()` / Swift `init(_:)` / Gleam `to_float` (F) | 追加 | R-sized-01 |
| `to_float_checked` | （追加）`int64.to_float_checked(x: Int64) -> Option[Float]` | Rust `try_from` / Swift `init(exactly:)` (F) / OCaml Base `to_int_exn`系 / 他 6 言語 — | 追加 | R-sized-01 |
| `to_float32_checked` | （追加）`int64.to_float32_checked(x: Int64) -> Option[Float32]` | Rust `try_from` / Swift `init(exactly:)` (F) / OCaml Base `to_int_exn`系 / 他 6 言語 — | 追加 | R-sized-01 |
| `to_float64_checked` | （追加）`int64.to_float64_checked(x: Int64) -> Option[Float64]` | Rust `try_from` / Swift `init(exactly:)` (F) / OCaml Base `to_int_exn`系 / 他 6 言語 — | 追加 | R-sized-01 |
| `parse` | （追加）`int64.parse(s: String) -> Result[Int64, String]` | Rust `str::parse::<T>` / Go `ParseInt(s,10,bits)` / OCaml `Int32.of_string_opt` / Kotlin `toByteOrNull` 等 / Swift `T(s)` | 追加 | R-sized-02 |
| `compare` | （追加）`int64.compare(a: Int64, b: Int64) -> Int` | Rust `cmp`/`total_cmp` / Go `cmp.Compare` / Kotlin `compareTo` / Swift `<` | 追加 | R-sized-02 |

### L3: `uint8`

現行 14 関数と、追加する 4 関数です。

| 関数 | シグネチャ(現行, mut 表示) | 他言語の主な綴り | 判定 | 案 |
|---|---|---|---|---|
| `to_int8` | `uint8.to_int8(x: UInt8) -> Int8` | Rust `as` / Swift `init(truncatingIfNeeded:)` (F) / Kotlin `toByte()` 等 / Go `T(x)` | 据え置き | R-sized-01（行列のセル） |
| `to_int16` | `uint8.to_int16(x: UInt8) -> Int16` | Rust `from`/`into` / Swift `init(_:)` / Kotlin `toLong()` 等 / Go `T(x)` | 据え置き | R-sized-01（行列のセル） |
| `to_int32` | `uint8.to_int32(x: UInt8) -> Int32` | Rust `from`/`into` / Swift `init(_:)` / Kotlin `toLong()` 等 / Go `T(x)` | 据え置き | R-sized-01（行列のセル） |
| `to_int64` | `uint8.to_int64(x: UInt8) -> Int64` | Rust `from`/`into` / Swift `init(_:)` / Kotlin `toLong()` 等 / Go `T(x)` | 据え置き | R-sized-01（行列のセル） |
| `to_uint16` | `uint8.to_uint16(x: UInt8) -> UInt16` | Rust `from`/`into` / Swift `init(_:)` / Kotlin `toLong()` 等 / Go `T(x)` | 据え置き | R-sized-01（行列のセル） |
| `to_uint32` | `uint8.to_uint32(x: UInt8) -> UInt32` | Rust `from`/`into` / Swift `init(_:)` / Kotlin `toLong()` 等 / Go `T(x)` | 据え置き | R-sized-01（行列のセル） |
| `to_uint64` | `uint8.to_uint64(x: UInt8) -> UInt64` | Rust `from`/`into` / Swift `init(_:)` / Kotlin `toLong()` 等 / Go `T(x)` | 据え置き | R-sized-01（行列のセル） |
| `to_float32` | `uint8.to_float32(x: UInt8) -> Float32` | Rust `as` / Kotlin `toDouble()` / Swift `init(_:)` / Gleam `to_float` (F) | 据え置き | R-sized-01（行列のセル） |
| `to_float64` | `uint8.to_float64(x: UInt8) -> Float64` | Rust `as` / Kotlin `toDouble()` / Swift `init(_:)` / Gleam `to_float` (F) | 据え置き | R-sized-01（行列のセル） |
| `to_string` | `uint8.to_string(x: UInt8) -> String` | Rust `to_string` / Kotlin `toString` / Swift `String(x)` / Go `strconv.FormatInt(int64(x),10)` | 据え置き | — |
| `to_int8_checked` | `uint8.to_int8_checked(x: UInt8) -> Option[Int8]` | Rust `try_from` / Swift `init(exactly:)` (F) / OCaml Base `to_int_exn`系 / 他 6 言語 — | 据え置き | R-sized-01（行列のセル） |
| `to_int8_saturating` | `uint8.to_int8_saturating(x: UInt8) -> Int8` | Swift `init(clamping:)` (F) / Kotlin `coerceIn` / 他 7 言語 — | 据え置き | R-sized-01（行列のセル） |
| `min_value` | `uint8.min_value() -> UInt8` | Rust `MIN` / Go `math.Min…` / Kotlin `MIN_VALUE` / Swift `.min` | 据え置き | — |
| `max_value` | `uint8.max_value() -> UInt8` | Rust `MAX` / Go `math.Max…` / Kotlin `MAX_VALUE` / Swift `.max` | 据え置き | — |
| `to_int` | （追加）`uint8.to_int(x: UInt8) -> Int` | Rust `from`/`into` / Swift `init(_:)` / Kotlin `toLong()` 等 / Go `T(x)` | 追加 | R-sized-01 |
| `to_float` | （追加）`uint8.to_float(x: UInt8) -> Float` | Rust `as` / Kotlin `toDouble()` / Swift `init(_:)` / Gleam `to_float` (F) | 追加 | R-sized-01 |
| `parse` | （追加）`uint8.parse(s: String) -> Result[UInt8, String]` | Rust `str::parse::<T>` / Go `ParseInt(s,10,bits)` / OCaml `Int32.of_string_opt` / Kotlin `toByteOrNull` 等 / Swift `T(s)` | 追加 | R-sized-02 |
| `compare` | （追加）`uint8.compare(a: UInt8, b: UInt8) -> Int` | Rust `cmp`/`total_cmp` / Go `cmp.Compare` / Kotlin `compareTo` / Swift `<` | 追加 | R-sized-02 |

### L3: `uint16`

現行 18 関数と、追加する 4 関数です。

| 関数 | シグネチャ(現行, mut 表示) | 他言語の主な綴り | 判定 | 案 |
|---|---|---|---|---|
| `to_int8` | `uint16.to_int8(x: UInt16) -> Int8` | Rust `as` / Swift `init(truncatingIfNeeded:)` (F) / Kotlin `toByte()` 等 / Go `T(x)` | 据え置き | R-sized-01（行列のセル） |
| `to_int16` | `uint16.to_int16(x: UInt16) -> Int16` | Rust `as` / Swift `init(truncatingIfNeeded:)` (F) / Kotlin `toByte()` 等 / Go `T(x)` | 据え置き | R-sized-01（行列のセル） |
| `to_int32` | `uint16.to_int32(x: UInt16) -> Int32` | Rust `from`/`into` / Swift `init(_:)` / Kotlin `toLong()` 等 / Go `T(x)` | 据え置き | R-sized-01（行列のセル） |
| `to_int64` | `uint16.to_int64(x: UInt16) -> Int64` | Rust `from`/`into` / Swift `init(_:)` / Kotlin `toLong()` 等 / Go `T(x)` | 据え置き | R-sized-01（行列のセル） |
| `to_uint8` | `uint16.to_uint8(x: UInt16) -> UInt8` | Rust `as` / Swift `init(truncatingIfNeeded:)` (F) / Kotlin `toByte()` 等 / Go `T(x)` | 据え置き | R-sized-01（行列のセル） |
| `to_uint32` | `uint16.to_uint32(x: UInt16) -> UInt32` | Rust `from`/`into` / Swift `init(_:)` / Kotlin `toLong()` 等 / Go `T(x)` | 据え置き | R-sized-01（行列のセル） |
| `to_uint64` | `uint16.to_uint64(x: UInt16) -> UInt64` | Rust `from`/`into` / Swift `init(_:)` / Kotlin `toLong()` 等 / Go `T(x)` | 据え置き | R-sized-01（行列のセル） |
| `to_float32` | `uint16.to_float32(x: UInt16) -> Float32` | Rust `as` / Kotlin `toDouble()` / Swift `init(_:)` / Gleam `to_float` (F) | 据え置き | R-sized-01（行列のセル） |
| `to_float64` | `uint16.to_float64(x: UInt16) -> Float64` | Rust `as` / Kotlin `toDouble()` / Swift `init(_:)` / Gleam `to_float` (F) | 据え置き | R-sized-01（行列のセル） |
| `to_string` | `uint16.to_string(x: UInt16) -> String` | Rust `to_string` / Kotlin `toString` / Swift `String(x)` / Go `strconv.FormatInt(int64(x),10)` | 据え置き | — |
| `to_int8_checked` | `uint16.to_int8_checked(x: UInt16) -> Option[Int8]` | Rust `try_from` / Swift `init(exactly:)` (F) / OCaml Base `to_int_exn`系 / 他 6 言語 — | 据え置き | R-sized-01（行列のセル） |
| `to_int8_saturating` | `uint16.to_int8_saturating(x: UInt16) -> Int8` | Swift `init(clamping:)` (F) / Kotlin `coerceIn` / 他 7 言語 — | 据え置き | R-sized-01（行列のセル） |
| `to_int16_checked` | `uint16.to_int16_checked(x: UInt16) -> Option[Int16]` | Rust `try_from` / Swift `init(exactly:)` (F) / OCaml Base `to_int_exn`系 / 他 6 言語 — | 据え置き | R-sized-01（行列のセル） |
| `to_int16_saturating` | `uint16.to_int16_saturating(x: UInt16) -> Int16` | Swift `init(clamping:)` (F) / Kotlin `coerceIn` / 他 7 言語 — | 据え置き | R-sized-01（行列のセル） |
| `to_uint8_checked` | `uint16.to_uint8_checked(x: UInt16) -> Option[UInt8]` | Rust `try_from` / Swift `init(exactly:)` (F) / OCaml Base `to_int_exn`系 / 他 6 言語 — | 据え置き | R-sized-01（行列のセル） |
| `to_uint8_saturating` | `uint16.to_uint8_saturating(x: UInt16) -> UInt8` | Swift `init(clamping:)` (F) / Kotlin `coerceIn` / 他 7 言語 — | 据え置き | R-sized-01（行列のセル） |
| `min_value` | `uint16.min_value() -> UInt16` | Rust `MIN` / Go `math.Min…` / Kotlin `MIN_VALUE` / Swift `.min` | 据え置き | — |
| `max_value` | `uint16.max_value() -> UInt16` | Rust `MAX` / Go `math.Max…` / Kotlin `MAX_VALUE` / Swift `.max` | 据え置き | — |
| `to_int` | （追加）`uint16.to_int(x: UInt16) -> Int` | Rust `from`/`into` / Swift `init(_:)` / Kotlin `toLong()` 等 / Go `T(x)` | 追加 | R-sized-01 |
| `to_float` | （追加）`uint16.to_float(x: UInt16) -> Float` | Rust `as` / Kotlin `toDouble()` / Swift `init(_:)` / Gleam `to_float` (F) | 追加 | R-sized-01 |
| `parse` | （追加）`uint16.parse(s: String) -> Result[UInt16, String]` | Rust `str::parse::<T>` / Go `ParseInt(s,10,bits)` / OCaml `Int32.of_string_opt` / Kotlin `toByteOrNull` 等 / Swift `T(s)` | 追加 | R-sized-02 |
| `compare` | （追加）`uint16.compare(a: UInt16, b: UInt16) -> Int` | Rust `cmp`/`total_cmp` / Go `cmp.Compare` / Kotlin `compareTo` / Swift `<` | 追加 | R-sized-02 |

### L3: `uint32`

現行 22 関数と、追加する 5 関数です。

| 関数 | シグネチャ(現行, mut 表示) | 他言語の主な綴り | 判定 | 案 |
|---|---|---|---|---|
| `to_int8` | `uint32.to_int8(x: UInt32) -> Int8` | Rust `as` / Swift `init(truncatingIfNeeded:)` (F) / Kotlin `toByte()` 等 / Go `T(x)` | 据え置き | R-sized-01（行列のセル） |
| `to_int16` | `uint32.to_int16(x: UInt32) -> Int16` | Rust `as` / Swift `init(truncatingIfNeeded:)` (F) / Kotlin `toByte()` 等 / Go `T(x)` | 据え置き | R-sized-01（行列のセル） |
| `to_int32` | `uint32.to_int32(x: UInt32) -> Int32` | Rust `as` / Swift `init(truncatingIfNeeded:)` (F) / Kotlin `toByte()` 等 / Go `T(x)` | 据え置き | R-sized-01（行列のセル） |
| `to_int64` | `uint32.to_int64(x: UInt32) -> Int64` | Rust `from`/`into` / Swift `init(_:)` / Kotlin `toLong()` 等 / Go `T(x)` | 据え置き | R-sized-01（行列のセル） |
| `to_uint8` | `uint32.to_uint8(x: UInt32) -> UInt8` | Rust `as` / Swift `init(truncatingIfNeeded:)` (F) / Kotlin `toByte()` 等 / Go `T(x)` | 据え置き | R-sized-01（行列のセル） |
| `to_uint16` | `uint32.to_uint16(x: UInt32) -> UInt16` | Rust `as` / Swift `init(truncatingIfNeeded:)` (F) / Kotlin `toByte()` 等 / Go `T(x)` | 据え置き | R-sized-01（行列のセル） |
| `to_uint64` | `uint32.to_uint64(x: UInt32) -> UInt64` | Rust `from`/`into` / Swift `init(_:)` / Kotlin `toLong()` 等 / Go `T(x)` | 据え置き | R-sized-01（行列のセル） |
| `to_float32` | `uint32.to_float32(x: UInt32) -> Float32` | Rust `as` / Kotlin `toDouble()` / Swift `init(_:)` / Gleam `to_float` (F) | 据え置き | R-sized-01（行列のセル） |
| `to_float64` | `uint32.to_float64(x: UInt32) -> Float64` | Rust `as` / Kotlin `toDouble()` / Swift `init(_:)` / Gleam `to_float` (F) | 据え置き | R-sized-01（行列のセル） |
| `to_string` | `uint32.to_string(x: UInt32) -> String` | Rust `to_string` / Kotlin `toString` / Swift `String(x)` / Go `strconv.FormatInt(int64(x),10)` | 据え置き | — |
| `to_int8_checked` | `uint32.to_int8_checked(x: UInt32) -> Option[Int8]` | Rust `try_from` / Swift `init(exactly:)` (F) / OCaml Base `to_int_exn`系 / 他 6 言語 — | 据え置き | R-sized-01（行列のセル） |
| `to_int8_saturating` | `uint32.to_int8_saturating(x: UInt32) -> Int8` | Swift `init(clamping:)` (F) / Kotlin `coerceIn` / 他 7 言語 — | 据え置き | R-sized-01（行列のセル） |
| `to_int16_checked` | `uint32.to_int16_checked(x: UInt32) -> Option[Int16]` | Rust `try_from` / Swift `init(exactly:)` (F) / OCaml Base `to_int_exn`系 / 他 6 言語 — | 据え置き | R-sized-01（行列のセル） |
| `to_int16_saturating` | `uint32.to_int16_saturating(x: UInt32) -> Int16` | Swift `init(clamping:)` (F) / Kotlin `coerceIn` / 他 7 言語 — | 据え置き | R-sized-01（行列のセル） |
| `to_int32_checked` | `uint32.to_int32_checked(x: UInt32) -> Option[Int32]` | Rust `try_from` / Swift `init(exactly:)` (F) / OCaml Base `to_int_exn`系 / 他 6 言語 — | 据え置き | R-sized-01（行列のセル） |
| `to_int32_saturating` | `uint32.to_int32_saturating(x: UInt32) -> Int32` | Swift `init(clamping:)` (F) / Kotlin `coerceIn` / 他 7 言語 — | 据え置き | R-sized-01（行列のセル） |
| `to_uint8_checked` | `uint32.to_uint8_checked(x: UInt32) -> Option[UInt8]` | Rust `try_from` / Swift `init(exactly:)` (F) / OCaml Base `to_int_exn`系 / 他 6 言語 — | 据え置き | R-sized-01（行列のセル） |
| `to_uint8_saturating` | `uint32.to_uint8_saturating(x: UInt32) -> UInt8` | Swift `init(clamping:)` (F) / Kotlin `coerceIn` / 他 7 言語 — | 据え置き | R-sized-01（行列のセル） |
| `to_uint16_checked` | `uint32.to_uint16_checked(x: UInt32) -> Option[UInt16]` | Rust `try_from` / Swift `init(exactly:)` (F) / OCaml Base `to_int_exn`系 / 他 6 言語 — | 据え置き | R-sized-01（行列のセル） |
| `to_uint16_saturating` | `uint32.to_uint16_saturating(x: UInt32) -> UInt16` | Swift `init(clamping:)` (F) / Kotlin `coerceIn` / 他 7 言語 — | 据え置き | R-sized-01（行列のセル） |
| `min_value` | `uint32.min_value() -> UInt32` | Rust `MIN` / Go `math.Min…` / Kotlin `MIN_VALUE` / Swift `.min` | 据え置き | — |
| `max_value` | `uint32.max_value() -> UInt32` | Rust `MAX` / Go `math.Max…` / Kotlin `MAX_VALUE` / Swift `.max` | 据え置き | — |
| `to_int` | （追加）`uint32.to_int(x: UInt32) -> Int` | Rust `from`/`into` / Swift `init(_:)` / Kotlin `toLong()` 等 / Go `T(x)` | 追加 | R-sized-01 |
| `to_float` | （追加）`uint32.to_float(x: UInt32) -> Float` | Rust `as` / Kotlin `toDouble()` / Swift `init(_:)` / Gleam `to_float` (F) | 追加 | R-sized-01 |
| `to_float32_checked` | （追加）`uint32.to_float32_checked(x: UInt32) -> Option[Float32]` | Rust `try_from` / Swift `init(exactly:)` (F) / OCaml Base `to_int_exn`系 / 他 6 言語 — | 追加 | R-sized-01 |
| `parse` | （追加）`uint32.parse(s: String) -> Result[UInt32, String]` | Rust `str::parse::<T>` / Go `ParseInt(s,10,bits)` / OCaml `Int32.of_string_opt` / Kotlin `toByteOrNull` 等 / Swift `T(s)` | 追加 | R-sized-02 |
| `compare` | （追加）`uint32.compare(a: UInt32, b: UInt32) -> Int` | Rust `cmp`/`total_cmp` / Go `cmp.Compare` / Kotlin `compareTo` / Swift `<` | 追加 | R-sized-02 |

### L3: `uint64`

現行 26 関数と、追加する 9 関数です。

| 関数 | シグネチャ(現行, mut 表示) | 他言語の主な綴り | 判定 | 案 |
|---|---|---|---|---|
| `to_int8` | `uint64.to_int8(x: UInt64) -> Int8` | Rust `as` / Swift `init(truncatingIfNeeded:)` (F) / Kotlin `toByte()` 等 / Go `T(x)` | 据え置き | R-sized-01（行列のセル） |
| `to_int16` | `uint64.to_int16(x: UInt64) -> Int16` | Rust `as` / Swift `init(truncatingIfNeeded:)` (F) / Kotlin `toByte()` 等 / Go `T(x)` | 据え置き | R-sized-01（行列のセル） |
| `to_int32` | `uint64.to_int32(x: UInt64) -> Int32` | Rust `as` / Swift `init(truncatingIfNeeded:)` (F) / Kotlin `toByte()` 等 / Go `T(x)` | 据え置き | R-sized-01（行列のセル） |
| `to_int64` | `uint64.to_int64(x: UInt64) -> Int64` | Rust `as` / Swift `init(truncatingIfNeeded:)` (F) / Kotlin `toByte()` 等 / Go `T(x)` | 据え置き | R-sized-01（行列のセル） |
| `to_uint8` | `uint64.to_uint8(x: UInt64) -> UInt8` | Rust `as` / Swift `init(truncatingIfNeeded:)` (F) / Kotlin `toByte()` 等 / Go `T(x)` | 据え置き | R-sized-01（行列のセル） |
| `to_uint16` | `uint64.to_uint16(x: UInt64) -> UInt16` | Rust `as` / Swift `init(truncatingIfNeeded:)` (F) / Kotlin `toByte()` 等 / Go `T(x)` | 据え置き | R-sized-01（行列のセル） |
| `to_uint32` | `uint64.to_uint32(x: UInt64) -> UInt32` | Rust `as` / Swift `init(truncatingIfNeeded:)` (F) / Kotlin `toByte()` 等 / Go `T(x)` | 据え置き | R-sized-01（行列のセル） |
| `to_float32` | `uint64.to_float32(x: UInt64) -> Float32` | Rust `as` / Kotlin `toDouble()` / Swift `init(_:)` / Gleam `to_float` (F) | 据え置き | R-sized-01（行列のセル） |
| `to_float64` | `uint64.to_float64(x: UInt64) -> Float64` | Rust `as` / Kotlin `toDouble()` / Swift `init(_:)` / Gleam `to_float` (F) | 据え置き | R-sized-01（行列のセル） |
| `to_string` | `uint64.to_string(x: UInt64) -> String` | Rust `to_string` / Kotlin `toString` / Swift `String(x)` / Go `strconv.FormatInt(int64(x),10)` | 据え置き | — |
| `to_int8_checked` | `uint64.to_int8_checked(x: UInt64) -> Option[Int8]` | Rust `try_from` / Swift `init(exactly:)` (F) / OCaml Base `to_int_exn`系 / 他 6 言語 — | 据え置き | R-sized-01（行列のセル） |
| `to_int8_saturating` | `uint64.to_int8_saturating(x: UInt64) -> Int8` | Swift `init(clamping:)` (F) / Kotlin `coerceIn` / 他 7 言語 — | 据え置き | R-sized-01（行列のセル） |
| `to_int16_checked` | `uint64.to_int16_checked(x: UInt64) -> Option[Int16]` | Rust `try_from` / Swift `init(exactly:)` (F) / OCaml Base `to_int_exn`系 / 他 6 言語 — | 据え置き | R-sized-01（行列のセル） |
| `to_int16_saturating` | `uint64.to_int16_saturating(x: UInt64) -> Int16` | Swift `init(clamping:)` (F) / Kotlin `coerceIn` / 他 7 言語 — | 据え置き | R-sized-01（行列のセル） |
| `to_int32_checked` | `uint64.to_int32_checked(x: UInt64) -> Option[Int32]` | Rust `try_from` / Swift `init(exactly:)` (F) / OCaml Base `to_int_exn`系 / 他 6 言語 — | 据え置き | R-sized-01（行列のセル） |
| `to_int32_saturating` | `uint64.to_int32_saturating(x: UInt64) -> Int32` | Swift `init(clamping:)` (F) / Kotlin `coerceIn` / 他 7 言語 — | 据え置き | R-sized-01（行列のセル） |
| `to_int64_checked` | `uint64.to_int64_checked(x: UInt64) -> Option[Int64]` | Rust `try_from` / Swift `init(exactly:)` (F) / OCaml Base `to_int_exn`系 / 他 6 言語 — | 据え置き | R-sized-01（行列のセル） |
| `to_int64_saturating` | `uint64.to_int64_saturating(x: UInt64) -> Int64` | Swift `init(clamping:)` (F) / Kotlin `coerceIn` / 他 7 言語 — | 据え置き | R-sized-01（行列のセル） |
| `to_uint8_checked` | `uint64.to_uint8_checked(x: UInt64) -> Option[UInt8]` | Rust `try_from` / Swift `init(exactly:)` (F) / OCaml Base `to_int_exn`系 / 他 6 言語 — | 据え置き | R-sized-01（行列のセル） |
| `to_uint8_saturating` | `uint64.to_uint8_saturating(x: UInt64) -> UInt8` | Swift `init(clamping:)` (F) / Kotlin `coerceIn` / 他 7 言語 — | 据え置き | R-sized-01（行列のセル） |
| `to_uint16_checked` | `uint64.to_uint16_checked(x: UInt64) -> Option[UInt16]` | Rust `try_from` / Swift `init(exactly:)` (F) / OCaml Base `to_int_exn`系 / 他 6 言語 — | 据え置き | R-sized-01（行列のセル） |
| `to_uint16_saturating` | `uint64.to_uint16_saturating(x: UInt64) -> UInt16` | Swift `init(clamping:)` (F) / Kotlin `coerceIn` / 他 7 言語 — | 据え置き | R-sized-01（行列のセル） |
| `to_uint32_checked` | `uint64.to_uint32_checked(x: UInt64) -> Option[UInt32]` | Rust `try_from` / Swift `init(exactly:)` (F) / OCaml Base `to_int_exn`系 / 他 6 言語 — | 据え置き | R-sized-01（行列のセル） |
| `to_uint32_saturating` | `uint64.to_uint32_saturating(x: UInt64) -> UInt32` | Swift `init(clamping:)` (F) / Kotlin `coerceIn` / 他 7 言語 — | 据え置き | R-sized-01（行列のセル） |
| `min_value` | `uint64.min_value() -> UInt64` | Rust `MIN` / Go `math.Min…` / Kotlin `MIN_VALUE` / Swift `.min` | 据え置き | — |
| `max_value` | `uint64.max_value() -> UInt64` | Rust `MAX` / Go `math.Max…` / Kotlin `MAX_VALUE` / Swift `.max` | 据え置き | — |
| `to_int` | （追加）`uint64.to_int(x: UInt64) -> Int` | Rust `as` / Swift `init(truncatingIfNeeded:)` (F) / Kotlin `toByte()` 等 / Go `T(x)` | 追加 | R-sized-01 |
| `to_int_checked` | （追加）`uint64.to_int_checked(x: UInt64) -> Option[Int]` | Rust `try_from` / Swift `init(exactly:)` (F) / OCaml Base `to_int_exn`系 / 他 6 言語 — | 追加 | R-sized-01 |
| `to_int_saturating` | （追加）`uint64.to_int_saturating(x: UInt64) -> Int` | Swift `init(clamping:)` (F) / Kotlin `coerceIn` / 他 7 言語 — | 追加 | R-sized-01 |
| `to_float` | （追加）`uint64.to_float(x: UInt64) -> Float` | Rust `as` / Kotlin `toDouble()` / Swift `init(_:)` / Gleam `to_float` (F) | 追加 | R-sized-01 |
| `to_float_checked` | （追加）`uint64.to_float_checked(x: UInt64) -> Option[Float]` | Rust `try_from` / Swift `init(exactly:)` (F) / OCaml Base `to_int_exn`系 / 他 6 言語 — | 追加 | R-sized-01 |
| `to_float32_checked` | （追加）`uint64.to_float32_checked(x: UInt64) -> Option[Float32]` | Rust `try_from` / Swift `init(exactly:)` (F) / OCaml Base `to_int_exn`系 / 他 6 言語 — | 追加 | R-sized-01 |
| `to_float64_checked` | （追加）`uint64.to_float64_checked(x: UInt64) -> Option[Float64]` | Rust `try_from` / Swift `init(exactly:)` (F) / OCaml Base `to_int_exn`系 / 他 6 言語 — | 追加 | R-sized-01 |
| `parse` | （追加）`uint64.parse(s: String) -> Result[UInt64, String]` | Rust `str::parse::<T>` / Go `ParseInt(s,10,bits)` / OCaml `Int32.of_string_opt` / Kotlin `toByteOrNull` 等 / Swift `T(s)` | 追加 | R-sized-02 |
| `compare` | （追加）`uint64.compare(a: UInt64, b: UInt64) -> Int` | Rust `cmp`/`total_cmp` / Go `cmp.Compare` / Kotlin `compareTo` / Swift `<` | 追加 | R-sized-02 |

### L3: `float32`

現行 10 関数と、追加する 20 関数です。

| 関数 | シグネチャ(現行, mut 表示) | 他言語の主な綴り | 判定 | 案 |
|---|---|---|---|---|
| `to_int8` | `float32.to_int8(x: Float32) -> Int8` | Rust `as`（飽和） / Kotlin `toInt()`（飽和）(F) / Swift `init(_:)`（trap） / Go `T(x)` | 据え置き | R-sized-01（行列のセル） |
| `to_int16` | `float32.to_int16(x: Float32) -> Int16` | Rust `as`（飽和） / Kotlin `toInt()`（飽和）(F) / Swift `init(_:)`（trap） / Go `T(x)` | 据え置き | R-sized-01（行列のセル） |
| `to_int32` | `float32.to_int32(x: Float32) -> Int32` | Rust `as`（飽和） / Kotlin `toInt()`（飽和）(F) / Swift `init(_:)`（trap） / Go `T(x)` | 据え置き | R-sized-01（行列のセル） |
| `to_int64` | `float32.to_int64(x: Float32) -> Int64` | Rust `as`（飽和） / Kotlin `toInt()`（飽和）(F) / Swift `init(_:)`（trap） / Go `T(x)` | 据え置き | R-sized-01（行列のセル） |
| `to_uint8` | `float32.to_uint8(x: Float32) -> UInt8` | Rust `as`（飽和） / Kotlin `toInt()`（飽和）(F) / Swift `init(_:)`（trap） / Go `T(x)` | 据え置き | R-sized-01（行列のセル） |
| `to_uint16` | `float32.to_uint16(x: Float32) -> UInt16` | Rust `as`（飽和） / Kotlin `toInt()`（飽和）(F) / Swift `init(_:)`（trap） / Go `T(x)` | 据え置き | R-sized-01（行列のセル） |
| `to_uint32` | `float32.to_uint32(x: Float32) -> UInt32` | Rust `as`（飽和） / Kotlin `toInt()`（飽和）(F) / Swift `init(_:)`（trap） / Go `T(x)` | 据え置き | R-sized-01（行列のセル） |
| `to_uint64` | `float32.to_uint64(x: Float32) -> UInt64` | Rust `as`（飽和） / Kotlin `toInt()`（飽和）(F) / Swift `init(_:)`（trap） / Go `T(x)` | 据え置き | R-sized-01（行列のセル） |
| `to_float64` | `float32.to_float64(x: Float32) -> Float64` | Rust `as` / Kotlin `toFloat()` / Swift `init(_:)` | 据え置き | R-sized-01（行列のセル） |
| `to_string` | `float32.to_string(x: Float32) -> String` | Rust `to_string` / Kotlin `toString` / Swift `String(x)` / Go `strconv.FormatInt(int64(x),10)` | 据え置き | — |
| `to_int` | （追加）`float32.to_int(x: Float32) -> Int` | Rust `as`（飽和） / Kotlin `toInt()`（飽和）(F) / Swift `init(_:)`（trap） / Go `T(x)` | 追加 | R-sized-01 |
| `to_int_checked` | （追加）`float32.to_int_checked(x: Float32) -> Option[Int]` | Rust `try_from` / Swift `init(exactly:)` (F) / OCaml Base `to_int_exn`系 / 他 6 言語 — | 追加 | R-sized-01 |
| `to_int8_checked` | （追加）`float32.to_int8_checked(x: Float32) -> Option[Int8]` | Rust `try_from` / Swift `init(exactly:)` (F) / OCaml Base `to_int_exn`系 / 他 6 言語 — | 追加 | R-sized-01 |
| `to_int16_checked` | （追加）`float32.to_int16_checked(x: Float32) -> Option[Int16]` | Rust `try_from` / Swift `init(exactly:)` (F) / OCaml Base `to_int_exn`系 / 他 6 言語 — | 追加 | R-sized-01 |
| `to_int32_checked` | （追加）`float32.to_int32_checked(x: Float32) -> Option[Int32]` | Rust `try_from` / Swift `init(exactly:)` (F) / OCaml Base `to_int_exn`系 / 他 6 言語 — | 追加 | R-sized-01 |
| `to_int64_checked` | （追加）`float32.to_int64_checked(x: Float32) -> Option[Int64]` | Rust `try_from` / Swift `init(exactly:)` (F) / OCaml Base `to_int_exn`系 / 他 6 言語 — | 追加 | R-sized-01 |
| `to_uint8_checked` | （追加）`float32.to_uint8_checked(x: Float32) -> Option[UInt8]` | Rust `try_from` / Swift `init(exactly:)` (F) / OCaml Base `to_int_exn`系 / 他 6 言語 — | 追加 | R-sized-01 |
| `to_uint16_checked` | （追加）`float32.to_uint16_checked(x: Float32) -> Option[UInt16]` | Rust `try_from` / Swift `init(exactly:)` (F) / OCaml Base `to_int_exn`系 / 他 6 言語 — | 追加 | R-sized-01 |
| `to_uint32_checked` | （追加）`float32.to_uint32_checked(x: Float32) -> Option[UInt32]` | Rust `try_from` / Swift `init(exactly:)` (F) / OCaml Base `to_int_exn`系 / 他 6 言語 — | 追加 | R-sized-01 |
| `to_uint64_checked` | （追加）`float32.to_uint64_checked(x: Float32) -> Option[UInt64]` | Rust `try_from` / Swift `init(exactly:)` (F) / OCaml Base `to_int_exn`系 / 他 6 言語 — | 追加 | R-sized-01 |
| `to_float` | （追加）`float32.to_float(x: Float32) -> Float` | Rust `as` / Kotlin `toFloat()` / Swift `init(_:)` | 追加 | R-sized-01 |
| `parse` | （追加）`float32.parse(s: String) -> Result[Float32, String]` | Rust `str::parse::<T>` / Go `ParseInt(s,10,bits)` / OCaml `Int32.of_string_opt` / Kotlin `toByteOrNull` 等 / Swift `T(s)` | 追加 | R-sized-02 |
| `compare` | （追加）`float32.compare(a: Float32, b: Float32) -> Int`（IEEE totalOrder） | Rust `cmp`/`total_cmp` / Go `cmp.Compare` / Kotlin `compareTo` / Swift `<` | 追加 | R-sized-02 |
| `to_bits` | （追加）`float32.to_bits(x: Float32) -> Int` | Rust `to_bits` (F) / Go `math.Float32bits` / Kotlin `toBits` | 追加 | R-sized-03 |
| `from_bits` | （追加）`float32.from_bits(bits: Int) -> Float32` | Rust `from_bits` (F) / Go `math.Float32frombits` / Kotlin `fromBits` | 追加 | R-sized-03 |
| `infinity` | （追加）`float32.infinity() -> Float32` | Rust `INFINITY` / Swift `.infinity` | 追加 | R-sized-03 |
| `nan` | （追加）`float32.nan() -> Float32` | Rust `NAN` / Swift `.nan` | 追加 | R-sized-03 |
| `epsilon` | （追加）`float32.epsilon() -> Float32` | Rust `EPSILON` / Swift `.ulpOfOne` | 追加 | R-sized-03 |
| `max_value` | （追加）`float32.max_value() -> Float32` | Rust `MAX` / Go `math.MaxFloat32` / Kotlin `MAX_VALUE` | 追加 | R-sized-03 |
| `min_value` | （追加）`float32.min_value() -> Float32` | Rust `MIN` / Swift `-.greatestFiniteMagnitude` | 追加 | R-sized-03 |

### L3: `float64`

現行 10 関数と、追加する 21 関数です。

| 関数 | シグネチャ(現行, mut 表示) | 他言語の主な綴り | 判定 | 案 |
|---|---|---|---|---|
| `to_int8` | `float64.to_int8(x: Float64) -> Int8` | Rust `as`（飽和） / Kotlin `toInt()`（飽和）(F) / Swift `init(_:)`（trap） / Go `T(x)` | 据え置き | R-sized-01（行列のセル） |
| `to_int16` | `float64.to_int16(x: Float64) -> Int16` | Rust `as`（飽和） / Kotlin `toInt()`（飽和）(F) / Swift `init(_:)`（trap） / Go `T(x)` | 据え置き | R-sized-01（行列のセル） |
| `to_int32` | `float64.to_int32(x: Float64) -> Int32` | Rust `as`（飽和） / Kotlin `toInt()`（飽和）(F) / Swift `init(_:)`（trap） / Go `T(x)` | 据え置き | R-sized-01（行列のセル） |
| `to_int64` | `float64.to_int64(x: Float64) -> Int64` | Rust `as`（飽和） / Kotlin `toInt()`（飽和）(F) / Swift `init(_:)`（trap） / Go `T(x)` | 据え置き | R-sized-01（行列のセル） |
| `to_uint8` | `float64.to_uint8(x: Float64) -> UInt8` | Rust `as`（飽和） / Kotlin `toInt()`（飽和）(F) / Swift `init(_:)`（trap） / Go `T(x)` | 据え置き | R-sized-01（行列のセル） |
| `to_uint16` | `float64.to_uint16(x: Float64) -> UInt16` | Rust `as`（飽和） / Kotlin `toInt()`（飽和）(F) / Swift `init(_:)`（trap） / Go `T(x)` | 据え置き | R-sized-01（行列のセル） |
| `to_uint32` | `float64.to_uint32(x: Float64) -> UInt32` | Rust `as`（飽和） / Kotlin `toInt()`（飽和）(F) / Swift `init(_:)`（trap） / Go `T(x)` | 据え置き | R-sized-01（行列のセル） |
| `to_uint64` | `float64.to_uint64(x: Float64) -> UInt64` | Rust `as`（飽和） / Kotlin `toInt()`（飽和）(F) / Swift `init(_:)`（trap） / Go `T(x)` | 据え置き | R-sized-01（行列のセル） |
| `to_float32` | `float64.to_float32(x: Float64) -> Float32` | Rust `as` / Kotlin `toFloat()` / Swift `init(_:)` | 据え置き | R-sized-01（行列のセル） |
| `to_string` | `float64.to_string(x: Float64) -> String` | Rust `to_string` / Kotlin `toString` / Swift `String(x)` / Go `strconv.FormatInt(int64(x),10)` | 据え置き | — |
| `to_int` | （追加）`float64.to_int(x: Float64) -> Int` | Rust `as`（飽和） / Kotlin `toInt()`（飽和）(F) / Swift `init(_:)`（trap） / Go `T(x)` | 追加 | R-sized-01 |
| `to_int_checked` | （追加）`float64.to_int_checked(x: Float64) -> Option[Int]` | Rust `try_from` / Swift `init(exactly:)` (F) / OCaml Base `to_int_exn`系 / 他 6 言語 — | 追加 | R-sized-01 |
| `to_int8_checked` | （追加）`float64.to_int8_checked(x: Float64) -> Option[Int8]` | Rust `try_from` / Swift `init(exactly:)` (F) / OCaml Base `to_int_exn`系 / 他 6 言語 — | 追加 | R-sized-01 |
| `to_int16_checked` | （追加）`float64.to_int16_checked(x: Float64) -> Option[Int16]` | Rust `try_from` / Swift `init(exactly:)` (F) / OCaml Base `to_int_exn`系 / 他 6 言語 — | 追加 | R-sized-01 |
| `to_int32_checked` | （追加）`float64.to_int32_checked(x: Float64) -> Option[Int32]` | Rust `try_from` / Swift `init(exactly:)` (F) / OCaml Base `to_int_exn`系 / 他 6 言語 — | 追加 | R-sized-01 |
| `to_int64_checked` | （追加）`float64.to_int64_checked(x: Float64) -> Option[Int64]` | Rust `try_from` / Swift `init(exactly:)` (F) / OCaml Base `to_int_exn`系 / 他 6 言語 — | 追加 | R-sized-01 |
| `to_uint8_checked` | （追加）`float64.to_uint8_checked(x: Float64) -> Option[UInt8]` | Rust `try_from` / Swift `init(exactly:)` (F) / OCaml Base `to_int_exn`系 / 他 6 言語 — | 追加 | R-sized-01 |
| `to_uint16_checked` | （追加）`float64.to_uint16_checked(x: Float64) -> Option[UInt16]` | Rust `try_from` / Swift `init(exactly:)` (F) / OCaml Base `to_int_exn`系 / 他 6 言語 — | 追加 | R-sized-01 |
| `to_uint32_checked` | （追加）`float64.to_uint32_checked(x: Float64) -> Option[UInt32]` | Rust `try_from` / Swift `init(exactly:)` (F) / OCaml Base `to_int_exn`系 / 他 6 言語 — | 追加 | R-sized-01 |
| `to_uint64_checked` | （追加）`float64.to_uint64_checked(x: Float64) -> Option[UInt64]` | Rust `try_from` / Swift `init(exactly:)` (F) / OCaml Base `to_int_exn`系 / 他 6 言語 — | 追加 | R-sized-01 |
| `to_float` | （追加）`float64.to_float(x: Float64) -> Float` | Rust `as` / Kotlin `toFloat()` / Swift `init(_:)` | 追加 | R-sized-01 |
| `to_float32_checked` | （追加）`float64.to_float32_checked(x: Float64) -> Option[Float32]` | Rust `try_from` / Swift `init(exactly:)` (F) / OCaml Base `to_int_exn`系 / 他 6 言語 — | 追加 | R-sized-01 |
| `parse` | （追加）`float64.parse(s: String) -> Result[Float64, String]` | Rust `str::parse::<T>` / Go `ParseInt(s,10,bits)` / OCaml `Int32.of_string_opt` / Kotlin `toByteOrNull` 等 / Swift `T(s)` | 追加 | R-sized-02 |
| `compare` | （追加）`float64.compare(a: Float64, b: Float64) -> Int`（IEEE totalOrder） | Rust `cmp`/`total_cmp` / Go `cmp.Compare` / Kotlin `compareTo` / Swift `<` | 追加 | R-sized-02 |
| `to_bits` | （追加）`float64.to_bits(x: Float64) -> Int` | Rust `to_bits` (F) / Go `math.Float64bits` / Kotlin `toBits` | 追加 | R-sized-03 |
| `from_bits` | （追加）`float64.from_bits(bits: Int) -> Float64` | Rust `from_bits` (F) / Go `math.Float64frombits` / Kotlin `fromBits` | 追加 | R-sized-03 |
| `infinity` | （追加）`float64.infinity() -> Float64` | Rust `INFINITY` / Swift `.infinity` | 追加 | R-sized-03 |
| `nan` | （追加）`float64.nan() -> Float64` | Rust `NAN` / Swift `.nan` | 追加 | R-sized-03 |
| `epsilon` | （追加）`float64.epsilon() -> Float64` | Rust `EPSILON` / Swift `.ulpOfOne` | 追加 | R-sized-03 |
| `max_value` | （追加）`float64.max_value() -> Float64` | Rust `MAX` / Go `math.MaxFloat64` / Kotlin `MAX_VALUE` | 追加 | R-sized-03 |
| `min_value` | （追加）`float64.min_value() -> Float64` | Rust `MIN` / Swift `-.greatestFiniteMagnitude` | 追加 | R-sized-03 |
