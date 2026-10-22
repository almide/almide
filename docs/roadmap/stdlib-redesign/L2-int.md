<!-- description: Stdlib redesign L2/L3 — int: retire the from_<sized> column and the u32/u8 one-offs, complete the wrap family, add floored division and radix conversion -->
# L2/L3: int

> Last updated: 2026-10-21
> 前提: [L0](L0-conventions.md)・[L1](L1-categories.md)
> 関連: [L2-sized-numbers](L2-sized-numbers.md)（変換の行列）・[L2-math](L2-math.md)（型の双子）・[L2-float](L2-float.md)
> 出典: Rust https://doc.rust-lang.org/std/primitive.i64.html ・ Go https://pkg.go.dev/math/bits と https://pkg.go.dev/strconv ・ Python https://docs.python.org/3/library/stdtypes.html#numeric-types-int-float-complex ・ TS/JS https://developer.mozilla.org/en-US/docs/Web/JavaScript/Reference/Global_Objects/Number ・ Gleam https://gleam-stdlib.hexdocs.pm/gleam/int.html (F) ・ Elixir https://elixir.hexdocs.pm/Integer.html (F) と https://hexdocs.pm/elixir/Bitwise.html ・ OCaml https://ocaml.org/manual/latest/api/Int.html (F) ・ Kotlin https://kotlinlang.org/api/core/kotlin-stdlib/kotlin/-long/ (F) ・ Swift https://developer.apple.com/documentation/swift/int

(F) = 今回その公式ページを取り寄せて綴りを確かめたセル。Gleam・Elixir・OCaml・Kotlin の関数名の一覧は 2026-10-21 に取得しました。

実測（`almide 0.68.0 dev 437b8f127`、native と `--target wasm` で同じ結果）:

| 式 | 結果 | 読み取れること |
|---|---|---|
| `int.wrap_add(1, 2, 0)` | `0` | `bits = 0` は何も言わずに 0 を返す |
| `int.wrap_add(1, 2, -1)` | `3` | 負の `bits` は「全部のビット」として扱われる。`rotate_left` は同じ入力で abort する（C-047） |
| `int.to_hex(-1)` → `int.from_hex` | `err: number too large…` | `to_hex` は 64 bit の並び、`from_hex` は符号付きの数。往復できない |
| `int.from_hex("-ff")` | `-255` | `from_hex` は符号を受け付ける |
| `-7 / 2`、`-7 % 3` | `-3`、`-1` | 割り算は 0 方向へ切り捨て。床（floor）方向の剰余はない |
| `5 & 3` | 字句エラー | ビット演算子はない。`int.band` などが唯一の書き方 |
| `int.max_value() + 1` | `-9223372036854775808` | `Int` の演算は折り返す |

## L2: 役割の比較

| 役割 | Almide | Rust | Go | Python | TS/JS | Gleam | Elixir | OCaml | Kotlin | Swift |
|---|---|---|---|---|---|---|---|---|---|---|
| 文字列へ | `to_string` | `to_string` | `strconv.Itoa` | `str` | `toString` | `to_string` (F) | `to_string` (F) | `to_string` (F) | `toString` | `String(n)` |
| 文字列から | `parse` | `parse` | `strconv.Atoi` | `int(s)` | `parseInt` | `parse` (F) | `parse` (F) | `of_string_opt` | `toIntOrNull` | `Int(s)` |
| 基数を指定して文字列へ | `to_hex` だけ | `format!("{:b}")` | `FormatInt(n, b)` | `bin`/`oct`/`hex` | `toString(r)` | `to_base_string` (F) | `to_string(n, b)` (F) | — | `toString(r)` | `String(n, radix:)` |
| 基数を指定して読む | `from_hex` だけ | `from_str_radix` | `ParseInt(s, b, 64)` | `int(s, b)` | `parseInt(s, r)` | `base_parse` (F) | `parse(s, b)` (F) | — | `toIntOrNull(r)` | `Int(s, radix:)` |
| Float へ | `to_float` | `as f64` | `float64(n)` | `float(n)` | （同じ型） | `to_float` (F) | — | `to_float` (F) | `toDouble` | `Double(n)` |
| 絶対値・最小・最大・範囲に収める | `abs` `min` `max` `clamp`（`math` にも双子） | `abs` `min` `max` `clamp` | `min` `max` | `abs` `min` `max` | `Math.*` | `absolute_value` `min` `max` `clamp` (F) | `abs` | `abs` `min` `max` (F) | `absoluteValue` `coerceIn` (F) | `abs` `min` `max` |
| 符号 | `math.sign` | `signum` | — | — | `Math.sign` | — | — | — | `sign` (F) | `signum()` |
| 累乗 | `math.pow` | `pow` | — | `**` | `**` | `power`（Float）(F) | `pow` (F) | — | — | — |
| 偶奇 | `is_even` `is_odd` | — | — | — | — | `is_even` `is_odd` (F) | `is_even` `is_odd` (F) | — | — | `isMultiple(of:)` |
| 床方向の割り算・剰余 | — | `div_euclid` `rem_euclid` | — | `//` `%` | — | `floor_divide` `modulo` (F) | `floor_div` `mod` (F) | `fdiv` `erem` (F) | `floorDiv` `mod` (F) | — |
| ビット AND/OR/XOR/NOT | `band` `bor` `bxor` `bnot` | 演算子 | 演算子 | 演算子 | 演算子 | `bitwise_and` 等 (F) | `band` `bor` `bxor` `bnot` | `logand` 等 (F) | `and` `or` `xor` `inv` (F) | 演算子 |
| シフト（算術） | `bshl` `bshr` | `<<` `>>` | `<<` `>>` | `<<` `>>` | `<<` `>>` | `bitwise_shift_*` (F) | `bsl` `bsr` | `shift_left` `shift_right` (F) | `shl` `shr` (F) | `<<` `>>` |
| 論理右シフト（0 で埋める） | — | 符号なし型の `>>` | 符号なし型の `>>` | — | `>>>` | — | — | `shift_right_logical` (F) | `ushr` (F) | 符号なし型の `>>` |
| ビット数え・回転・反転 | `pop_count` `count_*_zeros` `rotate_*` `bit_reverse` `byte_swap` `bit_width` | あり | `math/bits` | `bit_count` `bit_length` | `Math.clz32` | — | `popcount` (F) | `popcount` `leading_zeros` (F) | `countOneBits` `rotateLeft` (F) | あり |
| N bit で折り返す演算 | `wrap_add` `wrap_mul`（`wrap_sub` なし） | `wrapping_*` | 型変換そのもの | — | `BigInt.asUintN` | — | — | — | — | `&+` `&-` `&*` |
| 下限・上限 | `min_value` `max_value` | `MIN` `MAX` | `math.MinInt64` | — | `MIN_SAFE_INTEGER` | — | — | `min_int` `max_int` (F) | `MIN_VALUE` (F) | `.min` `.max` |
| 固定幅の型との変換 | `to_<型>` と `from_<型>` の両方 | `as`・`from`・`try_from` | `T(x)` | — | — | — | — | `Int32.of_int` | `toByte()` 等 (F) | `init(truncatingIfNeeded:)` 等 |

### 完全性の規則

1. **変換の行列**は [L2-sized-numbers](L2-sized-numbers.md) の規則に従います。`int` は 12 個の運び手（carrier）の 1 つで、`int` のモジュールに置くのは「`Int` から出ていく」変換だけです（R-L0-07）。`Int` へ入ってくる変換は、元の型のモジュールの `to_int` です。
2. **折り返し演算の族**（R-L0-08、#3854）: `wrap(n, bits)`、`wrap_add`、`wrap_sub`、`wrap_mul` の 4 セルがそろって存在します。シグネチャは `(…, bits: Int) -> Int`、`bits` の規則は回転（`rotate_*`）と同じにします（`bits <= 0` は abort、`bits >= 64` は 64 bit のまま）。`wrap_neg` は意図的に置きません（`int.wrap(0 - a, bits)` と同じで、`0 - min_value` も折り返して正しい値になります）。
3. **型の双子は置かない**（R-L0-01、R-L0-02）: 主語が `Int` の算術（`abs`、`min`、`max`、`sign`、`pow`）は `int` だけに置きます。`math` には残しません（[L2-math](L2-math.md) R-math-01）。
4. **基数の変換は対で置く**: 文字列へ（`to_string_radix`）と文字列から（`parse_radix`）は、同じ基数・同じ符号の規則で往復できる対にします。
5. **順序のある型は `compare` を持つ**（R-L0-16）: `int.compare(a, b) -> Int`（負・0・正）。追加は [R-sized-02](L2-sized-numbers.md) の 1 セルとして、すべての数の運び手に同時に入れます。
6. ビット演算の綴りは `b` 接頭辞の族（`band` `bor` `bxor` `bnot` `bshl` `bshr`）のままにし、足すセルもこの族の形にします。

### L2 の判定

- 抜けている役割: `compare`（R-L0-16 で必須）→ [R-sized-02](L2-sized-numbers.md)。床方向の割り算・剰余（6/9）→ R-int-03。基数の変換（8/9）→ R-int-04。論理右シフト（6/9）→ R-int-06。`sign` と `pow` は `math` から移す → R-math-01。
- 重複: `int.to_u32` / `int.to_u8` と `int.to_uint32` / `int.to_uint8`（#3851 E）→ R-int-02。`int.from_hex` と新しい `int.parse_radix(s, 16)` → R-int-04。`int.from_<型>` と `<型>.to_int`（R-L0-07）→ R-int-01。
- family の穴: `wrap_sub` と `wrap` がない（#3854）→ R-int-02。`int.to_float_checked` と `int.to_float64_checked` がない（`Int` から `Float` は 2^53 を超えると丸まる、損失のある変換）→ [R-sized-01](L2-sized-numbers.md)。
- 綴り: `band`/`bor`/`bxor` は多数派がないので据え置き → R-int-05（×）。

## 案

### R-int-01 `int.from_<型>` の列を撤去し、元の型の `to_int` に移す — ○ 採用

- 内容: `int.from_int8` `from_int16` `from_int32` `from_int64` `from_uint8` `from_uint16` `from_uint32` `from_uint64` と `int.from_uint64_checked` `int.from_uint64_saturating` の 10 関数を撤去します。代わりに `int8.to_int` … `uint64.to_int`、`uint64.to_int_checked`、`uint64.to_int_saturating` を足します（追加の本体は [R-sized-01](L2-sized-numbers.md)）。
- 根拠の L0 規則: R-L0-07（値の変換は元の型の `to_<型>`。両側にあるときは 1 の側だけ残す）、R-L0-01。
- (a) 元の型の側に `to_X` を置く言語は 5/9（Rust、Gleam、Elixir、OCaml、Kotlin `toLong()`）。`Int.from_int8` の形は 9 言語のどれにもありません。
- (b) 純粋な計算です。いまの `int.from_<型>` は intrinsic なので、新しい `to_int` は同じ intrinsic を呼ぶ別名として足し、`self_host_registry.rs` の行を付け替えます。両 target で値は変わりません。
- (c) `x |> int8.to_int` の形は、パイプの先頭の型からモジュールが決まります。いまは `int8.to_int16` は書けるのに `int8.to_int` は書けず、LLM が規則から推測すると確実に外れます。
- (d) 撤去と追加の組。`@deprecated(since=15, use="int8.to_int")` など、removal epoch 16。`almide fix` の書き換え: `int.from_<T>(x)` → `<T>.to_int(x)`（1 対 1）。使用数（spec/tests/docs/stdlib）: 8 関数の合計 70/38/47/362（stdlib の 362 は sized モジュールの本体が自分で呼んでいるもの。移行で 0 になります）、`from_uint64_checked` 9/2/7/1、`from_uint64_saturating` 4/2/4/0。CHEATSHEET:1068 と llms.txt:412 の書き換えを同じ変更に入れます。
- ゲート案: `src/cli/docs_gen.rs` の `check_numeric_conversion_matrix` の「Int の列は `int.from_<s>`」の部分を「`<s>.to_int`」に書き換え、`int.from_<s>` が存在しないことも検査します（R-sized-01 のゲートと同じ変更）。
- issue: #3935

### R-int-02 折り返しの族を 4 セルにそろえ、`to_u32`/`to_u8` を撤去する — ○ 採用

- 内容:
  - 追加: `int.wrap(n: Int, bits: Int) -> Int`（`n mod 2^bits`、`bits < 64` なら負にならない）、`int.wrap_sub(a: Int, b: Int, bits: Int) -> Int`。
  - 端をそろえる: `wrap`・`wrap_add`・`wrap_sub`・`wrap_mul` の `bits <= 0` は abort（`rotate_*` の C-047 と同じ文言の型）。`bits >= 64` は 64 bit のまま（C-048 と同じ）。
  - 撤去: `int.to_u32`（= `int.wrap(n, 32)`）、`int.to_u8`（= `int.wrap(n, 8)`）。
  - `wrap_neg` は足しません（完全性の規則 2）。
- 根拠の L0 規則: R-L0-08（行列の穴を埋める）、R-L0-01（`to_u32` と `to_uint32` は同じ値で型だけ違う紛らわしい対）。
- (a) 折り返しの減算は、折り返し演算を持つ言語すべてにあります（Rust `wrapping_sub`、Swift `&-`）。足し算と掛け算だけがあって引き算がない言語はありません。
- (b) 純粋な計算です。`stdlib/int_wrap.almd` の `__wrap_mask` を共有して書けます。`bits <= 0` の扱いは、いまは native と wasm のどちらも「0 または全ビット」を返していて（実測は冒頭の表）、両 target で同じように abort に変わります。
- (c) `wrap_add` と `wrap_mul` を見た LLM は `int.wrap_sub(a, b, 32)` を書きます（#3854）。`to_u32` と `to_uint32` は、1 文字の違いで戻り型が `Int` と `UInt32` に分かれます。
- (d) 追加 2、撤去 2、端の意味変更 1 組。端の変更は「意味のない値を返していた入力を abort にする」定義域の狭めで、値を返す入力の結果は 1 つも変わりません（R-L0-14 の 4 の「同じ名前で別の意味」には当たりません）。定義域を狭める変更なので、R-L0-12 の位置書きと同じく epoch の `breaks` には入れません。撤去は `@deprecated(since=15, use="int.wrap")`、removal epoch 16。`almide fix`: `int.to_u32(x)` → `int.wrap(x, 32)`、`int.to_u8(x)` → `int.wrap(x, 8)`。使用数: `to_u32` 6/2/10/0、`to_u8` 6/2/9/0、`wrap_add` 8/2/8/0、`wrap_mul` 7/2/6/0。
- ゲート案: #3854 の案どおり、`stdlib_synonym_gate_test.rs` の隣に「wrap 族の 4 セルがそろい、シグネチャが同じで、`self_host_registry.rs` に行がある」ことを検査するテストを置きます。`bits = 0`・`-1`・`64`・`65` の端を `spec/wasm_cross` の fixture で両 target 一致にします（新しい契約 C 番号を als に先に置く）。
- issue: #3854（1 節）、#3851 E

### R-int-03 床方向の割り算と剰余 `int.floor_div` / `int.mod` を足す — ○ 採用

- 内容: `int.floor_div(a: Int, b: Int) -> Int`（−∞ 方向へ丸める商）、`int.mod(a: Int, b: Int) -> Int`（結果の符号は `b` と同じ。`a == floor_div(a, b) * b + mod(a, b)`）。`b == 0` は `/` と同じく abort（契約違反、R-L0-06 の 3）。`floor_div(min_value, -1)` は `Int` の演算と同じく折り返します。
- 根拠の L0 規則: R-L0-02（主語が `Int` なので `int`）、L1 の追加基準（6/9 が持つ）。綴りは R-L0-00 の範囲で選びます: 剰余は `mod`（Elixir、Kotlin、Python の演算子名）、商は `floor_div`（Elixir、OCaml `fdiv` の意味、Kotlin `floorDiv`）。
- (a) 6/9 が持っています（Python、Gleam、Elixir、OCaml、Kotlin、Rust の euclid 版）。Go、JS、Swift だけがありません。
- (b) 純粋な計算です。`/` と `%` から Almide で書けます。
- (c) いまの `%` は 0 方向の剰余なので、`(i - 1) % n` で配列を回すと負の添字になります。他言語で `%` が床方向の Python から来た LLM が、ここで静かに外れます。名前で引ける関数があれば、規則（「床方向は `int.mod`」）を CHEATSHEET に 1 行で書けます。
- (d) 追加。使用数 0。
- ゲート案: `spec/stdlib/int_floor_div_test.almd` で符号の 4 象限と `b = 0` の abort を固定し、wasm_cross fixture を 1 本置きます。
- issue: #3918

### R-int-04 基数の変換を `to_string_radix` / `parse_radix` の対にし、`from_hex` を撤去する — ○ 採用

- 内容:
  - 追加: `int.to_string_radix(n: Int, radix: Int) -> String`（符号付き。`-255` は radix 16 で `"-ff"`。小文字）、`int.parse_radix(s: String, radix: Int) -> Result[Int, String]`（前後の空白を除き、`+`/`-` を受け付け、大文字小文字を問わない。radix が 16・8・2 のときに限り、対応する `0x`/`0o`/`0b` の接頭辞を省略可能として受け付ける）。`radix` が 2〜36 の外なら abort（契約違反）。
  - 撤去: `int.from_hex`（= `int.parse_radix(s, 16)`。「0x は省略可・符号可・2^63−1 を超えたら err」がそのまま同じになるように `parse_radix` を定義します）。
  - 据え置き: `int.to_hex`。これは「64 bit の並びを 16 進で書く」別の役割です（`-1` → 16 個の `f`）。符号付きの 16 進が欲しいときは `to_string_radix(n, 16)` を使う、と CHEATSHEET に 1 行書きます。
- 根拠の L0 規則: R-L0-01（`from_hex` と `parse_radix(s, 16)` は同じ役割）、L1 の追加基準（8/9 が基数を指定できる）。綴りは R-L0-00 の範囲で選びます（多数派の綴りはなく、Gleam `to_base_string`/`base_parse`、Kotlin `toString(radix)`/`toInt(radix)`、Swift `radix:`）。Almide には省略可能引数がないので、`radix` を名前に入れて既存の `to_string`/`parse` と分けます。
- (a) 8/9 が持っています（OCaml Stdlib だけがない）。
- (b) 純粋な計算です。
- (c) 2 進表記（`to_string_radix(n, 2)`）はアルゴリズム問題の定番で、いまは手で書くしかありません。`to_hex` と `from_hex` が往復しない罠（実測は冒頭の表）は、往復する対を別に置くことで逃げ道ができます。
- (d) 追加 2、撤去 1。`@deprecated(since=15, use="int.parse_radix")`、removal epoch 16。`almide fix`: `int.from_hex(s)` → `int.parse_radix(s, 16)`。使用数: `from_hex` 52/2/22/1、`to_hex` 23/2/10/2。
- ゲート案: `to_string_radix` → `parse_radix` の往復を radix 2〜36、`min_value`・`max_value`・`0`・`-1` で検査する spec と、`from_hex` の既存 spec を `parse_radix(s, 16)` で通す差分テスト（撤去までの窓のあいだ）。
- issue: #3920

### R-int-05 `band`/`bor`/`bxor` などを多数派の綴りに改名する — × 不採用

- 内容: 改名しません。
- 根拠の L0 規則: R-L0-00。関数名でビット演算を書く 4 言語の綴りは割れています（Gleam `bitwise_and`、Elixir `band`、OCaml `logand`、Kotlin `and`、すべて (F)）。残りの 5 言語は演算子だけです。多数派がないので、いまの綴り（Elixir と同じ `b` 接頭辞）を据え置きます。
- (c) 演算子 `&` を書くと字句エラーになり、ヒントは出ますが `int.band` を名指ししません。改名ではなく診断で助けるのが筋なので、E0xx（unknown-char）のヒントに「ビット演算は `int.band` / `int.bor` / `int.bxor` / `int.bshl` / `int.bshr`」を出す変更を、別の小さな issue にします。
- issue: #3905（診断ヒントのみ）

### R-int-06 論理右シフト `int.bshr_logical` を足す — ○ 採用

- 内容: `int.bshr_logical(a: Int, n: Int) -> Int`。上位を 0 で埋める右シフト。`n` は `bshr` と同じく 64 で割った余り。
- 根拠の L0 規則: L1 の追加基準（6/9）、R-L0-00（綴りは OCaml `shift_right_logical` の語を Almide の `b` 族の形にしたもの。Kotlin `ushr`、JS `>>>` は族の形に合いません）。
- (a) 6/9（Rust・Go・Swift は符号なし型の `>>`、JS `>>>`、OCaml、Kotlin）。
- (b) 純粋な計算です。runtime にはすでに `prim.bshr_u` があり、公開の名前を付けるだけです。
- (c) ハッシュ関数や乱数（R-random-01 の SplitMix64 もこれを使う）を書くと、算術シフトでは符号が混ざって静かに違う値になります。いまは `int.band(int.bshr(x, k), mask)` と手で書くしかありません。
- (d) 追加。使用数 0。
- ゲート案: `spec/stdlib/int_bits_test.almd` に負の数の論理シフトを足し、wasm_cross fixture で両 target を一致させます。
- issue: #3919

### R-int-07 `pop_count` を `count_ones` に改名する — ○ 採用

- いま: ビットを数える族は `count_leading_zeros`・`count_trailing_zeros` で、1 のビットを数える関数だけ `pop_count` です。
- 案: `int.count_ones` に改名します。族が `count_` で揃います。
- 根拠の L0 規則: R-L0-00（族の中で綴りが割れている）。
- (a) Rust `count_ones`、Kotlin `countOneBits`、Python `bit_count`、Go `OnesCount64`。`pop_count` の綴りは OCaml・Elixir の `popcount` に近いだけです。
- (d) 改名。`@deprecated(since=15, use="int.count_ones")`、`removal_epoch = 16`、`almide fix` で書き換えます。
- issue: #3985

### R-int-08 `int.to_hex` を `to_string_radix(x, 16)` にまとめる — × 不採用

- 理由: 2 つは負の数で結果が違い、同義語ではありません。`int.to_hex(-1)` は 64 bit の並びの 16 進（`"ffffffffffffffff"`）、`int.to_string_radix(-1, 16)` は符号つき（`"-1"`）です。ハッシュ値やビットの並びを表示する用途と、数を別の基数で書く用途に分かれます。
- issue: なし

### R-int-09 `int.sign` と `float.signum` の綴りを 1 つにする — × 不採用

- 理由: 0 の扱いが違うので、同じ名前にすると R-float-05 で避けた誤りが戻ります。`int.sign(0)`・`bigint.sign(0)` は 0、`float.signum(0.0)` は 1.0（Rust の `signum` と同じ）です。`sign` という名前は「0 は 0」の関数だけに使います（R-float-05 の完全性の規則）。
- issue: なし

### R-int-10 ビット演算の綴りを計測で決める — 保留（Dojo で測る）

- いま: `band`・`bor`・`bxor`・`bnot`・`bshl`・`bshr`・`bshr_logical`（Elixir と同じ `b` 接頭辞）。R-sized-10・R-bigint-07 で `Integer` の要件になり、整数 10 型の 61 関数に広がりました。
- R-int-05（×）の理由「関数名で書く 4 言語の綴りは割れていて、多数派がない」は今も成り立ちます。改名を、型エラーの理由だけで決めることもしません（R-L0-36）。
- 測る候補:
  1. いまの `int.band(a, b)`（Elixir）
  2. 語の形 `int.bit_and(a, b)`・`bit_or`・`bit_xor`・`bit_not`・`shift_left`・`shift_right`（Gleam の `bitwise_and`、Roc の `bitwiseAnd` に近い）
  3. 演算子 `a & b`・`a | b`・`a ^ b`・`~a`（R-L0-39 で `^` を空けたあと。Rust・Go・Python・JS・Swift の 5 言語と同じ）。シフトは `>>` が関数の合成なので、関数のまま（`shift_left`・`shift_right`）。
- 測り方: ビット演算を使う課題（ハッシュ、フラグ、CRC、ビット集合）を LLM に書かせ、そのまま通る割合を候補ごとに比べます。
- issue: almide-dojo#65

## L3: 関数表

`int` の現行 72 関数と、追加する 13 関数です（`compare` と、[L2-bigint](L2-bigint.md) の `to_bigint` を含む）。
| 関数 | シグネチャ(現行, mut 表示) | 他言語の主な綴り | 判定 | 案 |
|---|---|---|---|---|
| `to_string` | `int.to_string(n: Int) -> String` | Rust `to_string` / Go `strconv.FormatInt` / Python `str` / JS `toString` / Gleam `to_string` (F) / Elixir `to_string` (F) / OCaml `to_string` (F) / Kotlin `toString` / Swift `String(x)` | 据え置き | — |
| `to_hex` | `int.to_hex(n: Int) -> String` | Rust `format!("{:x}")` / Go `strconv.FormatInt(n,16)` / Python `hex` / JS `toString(16)` / Kotlin `toHexString` (F) / Gleam `to_base16` (F) | 据え置き | R-int-04（64 bit の並びの 16 進。符号付きの基数変換は `to_string_radix`） |
| `parse` | `int.parse(s: String) -> Result[Int, String]` | Rust `parse` / Go `strconv.Atoi` / Gleam `parse` (F) / Elixir `parse` (F) / OCaml `of_string_opt` / Kotlin `toIntOrNull` / Swift `Int(s)` | 据え置き | — |
| `from_hex` | `int.from_hex(s: String) -> Result[Int, String]` | Rust `from_str_radix(s,16)` / Go `ParseInt(s,16,64)` / Python `int(s,16)` / Gleam `base_parse(s,16)` (F) / Elixir `parse(s,16)` (F) | 撤去 | R-int-04（`int.parse_radix(s, 16)` に一本化） |
| `to_float` | `int.to_float(n: Int) -> Float` | Rust `as f64` / Gleam `to_float` (F) / OCaml `to_float` (F) / Kotlin `toDouble` (F) | 据え置き | — |
| `bits_to_float` | `int.bits_to_float(bits: Int) -> Float` | Rust `f64::from_bits` (F) / Go `math.Float64frombits` / Kotlin `Double.fromBits` / Swift `Double(bitPattern:)` | 撤去 | R-float-02（`float.from_bits`） |
| `bits_to_f32` | `int.bits_to_f32(bits: Int) -> Float` | Rust `f32::from_bits` / Go `math.Float32frombits` / Kotlin `Float.fromBits` | 撤去 | R-float-02（`float32.from_bits` → `float32.to_float`） |
| `abs` | `int.abs(n: Int) -> Int` | Rust `abs` / Go — / Python `abs` / JS `Math.abs` / Gleam `absolute_value` (F) / Elixir `abs` / OCaml `abs` (F) / Kotlin `absoluteValue` (F) / Swift `abs` | 据え置き | R-math-01（`math.abs` の残る側） |
| `min` | `int.min(a: Int, b: Int) -> Int` | Rust `min` / Go `min` / Python `min` / JS `Math.min` / Gleam `min` (F) / OCaml `min` (F) / Kotlin `minOf` / Swift `min` | 据え置き | R-math-01（`math.min` の残る側） |
| `max` | `int.max(a: Int, b: Int) -> Int` | Rust `max` / Go `max` / Python `max` / JS `Math.max` / Gleam `max` (F) / OCaml `max` (F) / Kotlin `maxOf` / Swift `max` | 据え置き | R-math-01（`math.max` の残る側） |
| `clamp` | `int.clamp(n: Int, lo: Int, hi: Int) -> Int` | Rust `clamp` / Gleam `clamp` (F) / Kotlin `coerceIn` (F) / Swift — | 据え置き | — |
| `is_even` | `int.is_even(n: Int) -> Bool` | Gleam `is_even` (F) / Elixir `is_even` (F) / Swift `isMultiple(of: 2)` / 他 — | 据え置き | — |
| `is_odd` | `int.is_odd(n: Int) -> Bool` | Gleam `is_odd` (F) / Elixir `is_odd` (F) / 他 — | 据え置き | — |
| `band` | `int.band(a: Int, b: Int) -> Int` | Gleam `bitwise_and` (F) / Elixir `Bitwise.band` / OCaml `logand` (F) / Kotlin `and` (F) / Rust・Go・Python・JS・Swift 演算子 `&` | 据え置き | R-int-05 ×（改名しない） |
| `bor` | `int.bor(a: Int, b: Int) -> Int` | Gleam `bitwise_or` (F) / Elixir `Bitwise.bor` / OCaml `logor` (F) / Kotlin `or` (F) / 他 5 言語 演算子 `\|` | 据え置き | R-int-05 × |
| `bxor` | `int.bxor(a: Int, b: Int) -> Int` | Gleam `bitwise_exclusive_or` (F) / Elixir `Bitwise.bxor` / OCaml `logxor` (F) / Kotlin `xor` (F) / 他 演算子 `^` | 据え置き | R-int-05 × |
| `bnot` | `int.bnot(a: Int) -> Int` | Gleam `bitwise_not` (F) / Elixir `Bitwise.bnot` / OCaml `lognot` (F) / Kotlin `inv` (F) / 他 演算子 `!`/`~` | 据え置き | R-int-05 × |
| `bshl` | `int.bshl(a: Int, n: Int) -> Int` | Gleam `bitwise_shift_left` (F) / Elixir `Bitwise.bsl` / OCaml `shift_left` (F) / Kotlin `shl` (F) / 他 演算子 `<<` | 据え置き | R-int-05 × |
| `bshr` | `int.bshr(a: Int, n: Int) -> Int` | Gleam `bitwise_shift_right` (F) / Elixir `Bitwise.bsr` / OCaml `shift_right` (F) / Kotlin `shr` (F) / 他 演算子 `>>` | 据え置き | R-int-05 × |
| `count_leading_zeros` | `int.count_leading_zeros(n: Int) -> Int` | Rust `leading_zeros` / Go `bits.LeadingZeros64` / OCaml `leading_zeros` (F) / Kotlin `countLeadingZeroBits` (F) / Swift `leadingZeroBitCount` | 据え置き | — |
| `count_trailing_zeros` | `int.count_trailing_zeros(n: Int) -> Int` | Rust `trailing_zeros` / Go `bits.TrailingZeros64` / OCaml `trailing_zeros` (F) / Kotlin `countTrailingZeroBits` (F) / Swift `trailingZeroBitCount` | 据え置き | — |
| `pop_count` | `int.pop_count(n: Int) -> Int` | Rust `count_ones` / Go `bits.OnesCount64` / Python `bit_count` / OCaml `popcount` (F) / Elixir `popcount` (F) / Kotlin `countOneBits` (F) / Swift `nonzeroBitCount` | 改名 → `count_ones` | R-int-07 |
| `bit_reverse` | `int.bit_reverse(n: Int) -> Int` | Rust `reverse_bits` / Go `bits.Reverse64` / 他 — | 据え置き | — |
| `byte_swap` | `int.byte_swap(n: Int) -> Int` | Rust `swap_bytes` / Go `bits.ReverseBytes64` / Swift `byteSwapped` | 据え置き | — |
| `bit_width` | `int.bit_width(n: Int) -> Int` | Go `bits.Len64` / Python `bit_length` / OCaml `unsigned_bitsize` (F) / Swift `bitWidth`（意味違い） | 据え置き | — |
| `log2_floor` | `int.log2_floor(n: Int) -> Int` | Rust `ilog2` / 他 — | 据え置き | — |
| `log2_ceil` | `int.log2_ceil(n: Int) -> Int` | 他 —（Rust は `next_power_of_two` から） | 据え置き | — |
| `next_power_of_two` | `int.next_power_of_two(n: Int) -> Int` | Rust `next_power_of_two` / 他 — | 据え置き | — |
| `prev_power_of_two` | `int.prev_power_of_two(n: Int) -> Int` | Kotlin `takeHighestOneBit` (F) / 他 — | 据え置き | — |
| `wrap_add` | `int.wrap_add(a: Int, b: Int, bits: Int) -> Int` | Rust `wrapping_add` / Swift `&+` / JS `BigInt.asUintN` / 他 — | 意味変更 | R-int-02（`bits <= 0` を abort にそろえる。値は変えない） |
| `wrap_mul` | `int.wrap_mul(a: Int, b: Int, bits: Int) -> Int` | Rust `wrapping_mul` / Swift `&*` / 他 — | 意味変更 | R-int-02（同上） |
| `rotate_right` | `int.rotate_right(a: Int, n: Int, bits: Int) -> Int` | Rust `rotate_right` / Go `bits.RotateLeft64(x,-k)` / Kotlin `rotateRight` (F) | 据え置き | — |
| `rotate_left` | `int.rotate_left(a: Int, n: Int, bits: Int) -> Int` | Rust `rotate_left` / Go `bits.RotateLeft64` / Kotlin `rotateLeft` (F) | 据え置き | — |
| `to_u32` | `int.to_u32(a: Int) -> Int` | JS `x >>> 0` / Rust `as u32 as i64` / 他 — | 撤去 | R-int-02（`int.wrap(x, 32)`。#3851 E） |
| `to_u8` | `int.to_u8(a: Int) -> Int` | Rust `as u8` / 他 — | 撤去 | R-int-02（`int.wrap(x, 8)`。#3851 E） |
| `to_int8` | `int.to_int8(n: Int) -> Int8` | Rust `as` / Swift `init(truncatingIfNeeded:)` (F) / Kotlin `toByte()` 等 / Go `T(x)` | 意味変更（範囲外は abort） | R-L0-32 |
| `to_int16` | `int.to_int16(n: Int) -> Int16` | Rust `as` / Swift `init(truncatingIfNeeded:)` (F) / Kotlin `toByte()` 等 / Go `T(x)` | 意味変更（範囲外は abort） | R-L0-32 |
| `to_int32` | `int.to_int32(n: Int) -> Int32` | Rust `as` / Swift `init(truncatingIfNeeded:)` (F) / Kotlin `toByte()` 等 / Go `T(x)` | 意味変更（範囲外は abort） | R-L0-32 |
| `to_uint8` | `int.to_uint8(n: Int) -> UInt8` | Rust `as` / Swift `init(truncatingIfNeeded:)` (F) / Kotlin `toByte()` 等 / Go `T(x)` | 意味変更（範囲外は abort） | R-L0-32 |
| `to_uint16` | `int.to_uint16(n: Int) -> UInt16` | Rust `as` / Swift `init(truncatingIfNeeded:)` (F) / Kotlin `toByte()` 等 / Go `T(x)` | 意味変更（範囲外は abort） | R-L0-32 |
| `to_uint32` | `int.to_uint32(n: Int) -> UInt32` | Rust `as` / Swift `init(truncatingIfNeeded:)` (F) / Kotlin `toByte()` 等 / Go `T(x)` | 意味変更（範囲外は abort） | R-L0-32 |
| `to_uint64` | `int.to_uint64(n: Int) -> UInt64` | Rust `as` / Swift `init(truncatingIfNeeded:)` (F) / Kotlin `toByte()` 等 / Go `T(x)` | 意味変更（範囲外は abort） | R-L0-32 |
| `to_float32` | `int.to_float32(n: Int) -> Float32` | Rust `as` / Kotlin `toDouble()` / Swift `init(_:)` / Gleam `to_float` (F) | 据え置き | R-sized-01（行列のセル） |
| `to_float64` | `int.to_float64(n: Int) -> Float64` | Rust `as` / Kotlin `toDouble()` / Swift `init(_:)` / Gleam `to_float` (F) | 据え置き | R-sized-01（行列のセル） |
| `to_int64` | `int.to_int64(n: Int) -> Int64` | Rust `from`/`into` / Swift `init(_:)` / Kotlin `toLong()` 等 / Go `T(x)` | 据え置き | R-sized-01（行列のセル） |
| `from_int8` | `int.from_int8(n: Int8) -> Int` | Rust `from`/`into` / Swift `init(_:)` / Kotlin `toLong()` 等 / Go `T(x)` | 撤去 | R-sized-01（`int8.to_int` に移す。R-L0-07） |
| `from_int16` | `int.from_int16(n: Int16) -> Int` | Rust `from`/`into` / Swift `init(_:)` / Kotlin `toLong()` 等 / Go `T(x)` | 撤去 | R-sized-01（`int16.to_int` に移す。R-L0-07） |
| `from_int32` | `int.from_int32(n: Int32) -> Int` | Rust `from`/`into` / Swift `init(_:)` / Kotlin `toLong()` 等 / Go `T(x)` | 撤去 | R-sized-01（`int32.to_int` に移す。R-L0-07） |
| `from_int64` | `int.from_int64(n: Int64) -> Int` | Rust `from`/`into` / Swift `init(_:)` / Kotlin `toLong()` 等 / Go `T(x)` | 撤去 | R-sized-01（`int64.to_int` に移す。R-L0-07） |
| `from_uint8` | `int.from_uint8(n: UInt8) -> Int` | Rust `from`/`into` / Swift `init(_:)` / Kotlin `toLong()` 等 / Go `T(x)` | 撤去 | R-sized-01（`uint8.to_int` に移す。R-L0-07） |
| `from_uint16` | `int.from_uint16(n: UInt16) -> Int` | Rust `from`/`into` / Swift `init(_:)` / Kotlin `toLong()` 等 / Go `T(x)` | 撤去 | R-sized-01（`uint16.to_int` に移す。R-L0-07） |
| `from_uint32` | `int.from_uint32(n: UInt32) -> Int` | Rust `from`/`into` / Swift `init(_:)` / Kotlin `toLong()` 等 / Go `T(x)` | 撤去 | R-sized-01（`uint32.to_int` に移す。R-L0-07） |
| `from_uint64` | `int.from_uint64(n: UInt64) -> Int` | Rust `as` / Swift `init(truncatingIfNeeded:)` (F) / Kotlin `toByte()` 等 / Go `T(x)` | 撤去 | R-sized-01（`uint64.to_int` に移す。R-L0-07） |
| `to_int8_checked` | `int.to_int8_checked(n: Int) -> Option[Int8]` | Rust `try_from` / Swift `init(exactly:)` (F) / OCaml Base `to_int_exn`系 / 他 6 言語 — | 据え置き | R-sized-01（行列のセル） |
| `to_int16_checked` | `int.to_int16_checked(n: Int) -> Option[Int16]` | Rust `try_from` / Swift `init(exactly:)` (F) / OCaml Base `to_int_exn`系 / 他 6 言語 — | 据え置き | R-sized-01（行列のセル） |
| `to_int32_checked` | `int.to_int32_checked(n: Int) -> Option[Int32]` | Rust `try_from` / Swift `init(exactly:)` (F) / OCaml Base `to_int_exn`系 / 他 6 言語 — | 据え置き | R-sized-01（行列のセル） |
| `to_uint8_checked` | `int.to_uint8_checked(n: Int) -> Option[UInt8]` | Rust `try_from` / Swift `init(exactly:)` (F) / OCaml Base `to_int_exn`系 / 他 6 言語 — | 据え置き | R-sized-01（行列のセル） |
| `to_uint16_checked` | `int.to_uint16_checked(n: Int) -> Option[UInt16]` | Rust `try_from` / Swift `init(exactly:)` (F) / OCaml Base `to_int_exn`系 / 他 6 言語 — | 据え置き | R-sized-01（行列のセル） |
| `to_uint32_checked` | `int.to_uint32_checked(n: Int) -> Option[UInt32]` | Rust `try_from` / Swift `init(exactly:)` (F) / OCaml Base `to_int_exn`系 / 他 6 言語 — | 据え置き | R-sized-01（行列のセル） |
| `to_uint64_checked` | `int.to_uint64_checked(n: Int) -> Option[UInt64]` | Rust `try_from` / Swift `init(exactly:)` (F) / OCaml Base `to_int_exn`系 / 他 6 言語 — | 据え置き | R-sized-01（行列のセル） |
| `to_float32_checked` | `int.to_float32_checked(n: Int) -> Option[Float32]` | Rust `try_from` / Swift `init(exactly:)` (F) / OCaml Base `to_int_exn`系 / 他 6 言語 — | 据え置き | R-sized-01（行列のセル） |
| `to_int8_saturating` | `int.to_int8_saturating(n: Int) -> Int8` | Swift `init(clamping:)` (F) / Kotlin `coerceIn` / 他 7 言語 — | 据え置き | R-sized-01（行列のセル） |
| `to_int8_wrapping` | （追加）`int.to_int8_wrapping(x: Int) -> Int8` | Swift `init(truncatingIfNeeded:)` (F) / Rust `as` / Go `T(x)` / Kotlin `toByte()` 等 | 追加 | R-L0-34 |
| `to_int16_saturating` | `int.to_int16_saturating(n: Int) -> Int16` | Swift `init(clamping:)` (F) / Kotlin `coerceIn` / 他 7 言語 — | 据え置き | R-sized-01（行列のセル） |
| `to_int16_wrapping` | （追加）`int.to_int16_wrapping(x: Int) -> Int16` | Swift `init(truncatingIfNeeded:)` (F) / Rust `as` / Go `T(x)` / Kotlin `toByte()` 等 | 追加 | R-L0-34 |
| `to_int32_saturating` | `int.to_int32_saturating(n: Int) -> Int32` | Swift `init(clamping:)` (F) / Kotlin `coerceIn` / 他 7 言語 — | 据え置き | R-sized-01（行列のセル） |
| `to_int32_wrapping` | （追加）`int.to_int32_wrapping(x: Int) -> Int32` | Swift `init(truncatingIfNeeded:)` (F) / Rust `as` / Go `T(x)` / Kotlin `toByte()` 等 | 追加 | R-L0-34 |
| `to_uint8_saturating` | `int.to_uint8_saturating(n: Int) -> UInt8` | Swift `init(clamping:)` (F) / Kotlin `coerceIn` / 他 7 言語 — | 据え置き | R-sized-01（行列のセル） |
| `to_uint8_wrapping` | （追加）`int.to_uint8_wrapping(x: Int) -> UInt8` | Swift `init(truncatingIfNeeded:)` (F) / Rust `as` / Go `T(x)` / Kotlin `toByte()` 等 | 追加 | R-L0-34 |
| `to_uint16_saturating` | `int.to_uint16_saturating(n: Int) -> UInt16` | Swift `init(clamping:)` (F) / Kotlin `coerceIn` / 他 7 言語 — | 据え置き | R-sized-01（行列のセル） |
| `to_uint16_wrapping` | （追加）`int.to_uint16_wrapping(x: Int) -> UInt16` | Swift `init(truncatingIfNeeded:)` (F) / Rust `as` / Go `T(x)` / Kotlin `toByte()` 等 | 追加 | R-L0-34 |
| `to_uint32_saturating` | `int.to_uint32_saturating(n: Int) -> UInt32` | Swift `init(clamping:)` (F) / Kotlin `coerceIn` / 他 7 言語 — | 据え置き | R-sized-01（行列のセル） |
| `to_uint32_wrapping` | （追加）`int.to_uint32_wrapping(x: Int) -> UInt32` | Swift `init(truncatingIfNeeded:)` (F) / Rust `as` / Go `T(x)` / Kotlin `toByte()` 等 | 追加 | R-L0-34 |
| `to_uint64_saturating` | `int.to_uint64_saturating(n: Int) -> UInt64` | Swift `init(clamping:)` (F) / Kotlin `coerceIn` / 他 7 言語 — | 据え置き | R-sized-01（行列のセル） |
| `to_uint64_wrapping` | （追加）`int.to_uint64_wrapping(x: Int) -> UInt64` | Swift `init(truncatingIfNeeded:)` (F) / Rust `as` / Go `T(x)` / Kotlin `toByte()` 等 | 追加 | R-L0-34 |
| `from_uint64_checked` | `int.from_uint64_checked(n: UInt64) -> Option[Int]` | Rust `try_from` / Swift `init(exactly:)` (F) / OCaml Base `to_int_exn`系 / 他 6 言語 — | 撤去 | R-sized-01（`uint64.to_int_checked`） |
| `from_uint64_saturating` | `int.from_uint64_saturating(n: UInt64) -> Int` | Swift `init(clamping:)` (F) / Kotlin `coerceIn` / 他 7 言語 — | 撤去 | R-sized-01（`uint64.to_int_saturating`） |
| `min_value` | `int.min_value() -> Int` | Rust `i64::MIN` / Go `math.MinInt64` / OCaml `min_int` (F) / Kotlin `MIN_VALUE` (F) / Swift `.min` | 据え置き | — |
| `max_value` | `int.max_value() -> Int` | Rust `i64::MAX` / Go `math.MaxInt64` / OCaml `max_int` (F) / Kotlin `MAX_VALUE` (F) / Swift `.max` | 据え置き | — |
| `to_float_checked` | （追加）`int.to_float_checked(x: Int) -> Option[Float]` | Rust `try_from` / Swift `init(exactly:)` (F) / OCaml Base `to_int_exn`系 / 他 6 言語 — | 追加 | R-sized-01 |
| `to_float64_checked` | （追加）`int.to_float64_checked(x: Int) -> Option[Float64]` | Rust `try_from` / Swift `init(exactly:)` (F) / OCaml Base `to_int_exn`系 / 他 6 言語 — | 追加 | R-sized-01 |
| `wrap` | （追加）`int.wrap(n: Int, bits: Int) -> Int` | Rust `as uN`系 / JS `BigInt.asUintN(bits, n)` / 他 — | 追加 | R-int-02 |
| `wrap_sub` | （追加）`int.wrap_sub(a: Int, b: Int, bits: Int) -> Int` | Rust `wrapping_sub` / Swift `&-` / 他 — | 追加 | R-int-02 |
| `pow` | （追加）`int.pow(base: Int, exp: Int) -> Int` | Rust `pow` / Gleam `power`（Float）(F) / Elixir `Integer.pow` (F) / Python `**` | 追加 | R-math-01 |
| `sign` | （追加）`int.sign(n: Int) -> Int` | Rust `signum` / Kotlin `sign` (F) / JS `Math.sign` / Swift `signum()` | 追加 | R-math-01 |
| `mod` | （追加）`int.mod(a: Int, b: Int) -> Int` | Python `%` / Gleam `modulo` (F) / Elixir `Integer.mod` (F) / Kotlin `mod` (F) / Rust `rem_euclid` | 追加 | R-int-03 |
| `floor_div` | （追加）`int.floor_div(a: Int, b: Int) -> Int` | Python `//` / Gleam `floor_divide` (F) / Elixir `Integer.floor_div` (F) / Kotlin `floorDiv` (F) / OCaml `fdiv` (F) | 追加 | R-int-03 |
| `bshr_logical` | （追加）`int.bshr_logical(a: Int, n: Int) -> Int` | OCaml `shift_right_logical` (F) / Kotlin `ushr` (F) / JS `>>>` / Rust・Go・Swift 符号なし型の `>>` | 追加 | R-int-06 |
| `to_string_radix` | （追加）`int.to_string_radix(n: Int, radix: Int) -> String` | Go `FormatInt(n, base)` / JS `toString(radix)` / Gleam `to_base_string` (F) / Elixir `to_string(n, base)` (F) / Kotlin `toString(radix)` / Swift `String(n, radix:)` | 追加 | R-int-04 |
| `parse_radix` | （追加）`int.parse_radix(s: String, radix: Int) -> Result[Int, String]` | Rust `from_str_radix` / Go `ParseInt(s, base, 64)` / Python `int(s, base)` / Gleam `base_parse` (F) / Elixir `parse(s, base)` (F) / Kotlin `toIntOrNull(radix)` / Swift `Int(s, radix:)` | 追加 | R-int-04 |
| `compare` | （追加）`int.compare(a: Int, b: Int) -> Int` | Rust `cmp` / Go `cmp.Compare` / Gleam `compare`（Order）(F) / OCaml `compare` (F) / Kotlin `compareTo` (F) / Swift `<` | 追加 | R-sized-02 |
| `to_bigint` | （追加）`int.to_bigint(n: Int) -> BigInt` | Go `big.NewInt` / JS `BigInt(n)` / Kotlin `BigInteger.valueOf` | 追加 | [R-bigint-01](L2-bigint.md) |
