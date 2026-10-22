<!-- description: Stdlib redesign L2/L3 — bigint (new module, R-L1-03): the proposed surface of an arbitrary-precision integer that mirrors int, with operators desugared to it and byte-identical results on both targets -->
# L2/L3: bigint（新しいモジュール。提案する面だけ）

> Last updated: 2026-10-21
> 前提: [L0](L0-conventions.md)・[L1](L1-categories.md)（R-L1-03: `bigint` を足す）
> 関連: [L2-int](L2-int.md)（`int` と同じ綴りを使う）・[L2-sized-numbers](L2-sized-numbers.md)（変換の行列の規則）
> 出典: Rust —（標準になし。https://docs.rs/num-bigint/ が事実上の標準） ・ Go https://pkg.go.dev/math/big#Int ・ Python https://docs.python.org/3/library/stdtypes.html#numeric-types-int-float-complex ・ TS/JS https://developer.mozilla.org/en-US/docs/Web/JavaScript/Reference/Global_Objects/BigInt ・ Gleam https://gleam-stdlib.hexdocs.pm/gleam/int.html (F)（Erlang ターゲットの Int は任意精度） ・ Elixir https://elixir.hexdocs.pm/Integer.html (F)（Integer は任意精度） ・ OCaml —（Zarith は外部 https://github.com/ocaml/Zarith ） ・ Kotlin https://docs.oracle.com/en/java/javase/21/docs/api/java.base/java/math/BigInteger.html（JVM だけ） ・ Swift —

いまの Almide の `Int` は 64 bit で、桁あふれは折り返します（実測: `int.max_value() + 1` は `-9223372036854775808`。`math.factorial(21)` も折り返す）。階乗・大きなフィボナッチ数・組み合わせの数で、静かに間違った答えになります。

## L2: 役割の比較

| 役割 | 提案（Almide） | Rust | Go | Python | TS/JS | Gleam | Elixir | OCaml | Kotlin（JVM） | Swift |
|---|---|---|---|---|---|---|---|---|---|---|
| 型 | `BigInt` | — | `big.Int` | `int` | `BigInt` | ◐ `Int`（Erlang） | `Integer` | — | `BigInteger` | — |
| Int から | `int.to_bigint` | — | `NewInt` | （同じ型） | `BigInt(n)` | — | — | — | `valueOf` | — |
| Int へ | `to_int`（`_checked`・`_saturating`） | — | `Int64` `IsInt64` | （同じ型） | `Number(b)` `asIntN` | — | — | — | `toLong` `longValueExact` | — |
| 文字列から・へ | `parse` `to_string`（`_radix` 版も） | — | `SetString` `String` `Text(base)` | `int(s)` `str` | `BigInt(s)` `toString(r)` | — | `parse` `to_string` (F) | — | `BigInteger(s)` `toString(r)` | — |
| 四則と剰余 | `add` `sub` `mul` `div` `rem`（演算子） | — | `Add` `Sub` `Mul` `Quo` `Rem` | `+ - * // %` | `+ - * / %` | — | `+ - * div rem` | — | `add` `subtract` `multiply` `divide` `remainder` | — |
| 床方向の割り算・剰余 | `floor_div` `mod` | — | `Div` `Mod`（Euclid） | `//` `%` | — | — | `floor_div` `mod` (F) | — | `mod`（非負） | — |
| 符号・絶対値・累乗 | `neg` `abs` `sign` `pow` | — | `Neg` `Abs` `Sign` `Exp` | `-` `abs` `**` | `-` `**` | — | `abs` `pow` (F) | — | `negate` `abs` `signum` `pow` | — |
| 比較・最小・最大 | `compare` `min` `max`（演算子 `<` も） | — | `Cmp` | `<` | `<` | — | `<` | — | `compareTo` `min` `max` | — |
| Float へ | `to_float`（`_checked`） | — | `Float64` | `float` | `Number` | — | — | — | `toDouble` | — |

### 完全性の規則

1. **`int` と同じ綴り**: `BigInt` の関数は、`int` に同じ役割の関数があるときは同じ名前・同じ引数の順にします（`abs` `min` `max` `sign` `pow` `compare` `parse` `to_string` `to_string_radix` `parse_radix` `floor_div` `mod`）。`int` の L2 で足す関数（R-int-03・R-int-04）とそろえます。
2. **変換の向き**（R-L0-07）: `Int` → `BigInt` は元の型の側の `int.to_bigint`、`BigInt` → `Int` は `bigint.to_int`。`BigInt` → `Int` は損失があるので 3 つ組（`to_int`・`to_int_checked`・`to_int_saturating`）、`BigInt` → `Float` は `to_float` と `to_float_checked` を置きます（[L2-sized-numbers](L2-sized-numbers.md) の規則 3・4 と同じ）。固定幅の型との直接の変換は置きません（`to_int` を経由します）。
3. **演算子**: `+` `-` `*` `/` `%` `==` `<` などは `bigint.add` … に脱糖されます（L1 R-L1-03）。`/` と `%` は `Int` と同じく 0 方向へ切り捨て、0 で割ると abort します。
4. **意図的に置かないもの**: ビット演算（`band` など）、`gcd`（標準に持つのは Go・Python・Elixir・JVM の 4 つで多数派に届かない）、平方根、`Int` との暗黙の変換、`123n` のようなリテラル（R-bigint-02）。

### L2 の判定

- 新しいモジュールです。重複も既存の綴りとの衝突もありません。
- 受け皿になるもの: `math.factorial` と `math.choose` の折り返し（[L2-math](L2-math.md)）は据え置き、大きな値が要るときは `bigint` で書く、と doc に書きます。

## 案

### R-bigint-01 任意精度整数のモジュール `bigint` を足す — ○ 採用

- 内容: 型 `BigInt` と、下の L3 表の 26 関数（`int.to_bigint` を含む）。
- 根拠の L0 規則: L1 R-L1-03、R-L0-02（返す型・主語の型のモジュール）、R-L0-07（変換の向き）、R-L0-06（`parse` は `Result`、`to_int_checked` は `Option`）、R-L0-16（順序のある型は `compare`）。
- (a) Go・Python・JS・Elixir・Kotlin（JVM）と、Erlang ターゲットの Gleam の 6 言語が持っています。
- (b) 純粋な計算として Almide で書きます（桁の列を `List[Int]` か `Bytes` に持つ）。host の機能は要らず、native と wasm で 1 ビットも違わない結果を、ADR-0016（数値の決定性）の約束に入れます。速さのために native だけ別の実装にすることはしません（ADR-0027 D8 の考え方: 片方だけの経路を終点にしない）。
- (c) 「階乗の 30」「100 番目のフィボナッチ数」のような問題で、LLM は Python の `int` の感覚で書き、`Int` の折り返しで静かに外れます。`bigint` があれば CHEATSHEET に「20 を超える階乗は `bigint`」と 1 行で書けます。
- (d) 追加（新しいモジュール。`AUTO_IMPORT_BUNDLED` には入れず、`import bigint` を要求するかは L1 の import 規則に従います）。使用数 0。
- ゲート案: `int` と `bigint` の同じ名前の関数を、`Int` に収まる入力で突き合わせる差分テスト（`bigint.to_int(bigint.add(a, b)) == a + b` など、折り返さない範囲）。両 target の一致を wasm_cross fixture で固定し、新しい契約 C 番号を als に先に置きます。完全性の規則 1 の「`int` と同じ綴り」をシグネチャから検査します。
- issue: #3931

### R-bigint-02 `123n` のような BigInt のリテラルを足す — × 不採用

- 内容: 足しません。`bigint.parse("123…")!` と `int.to_bigint(123)` で書きます。
- 根拠: 字句規則の変更は言語の変更で、ADR-0001 D3 が時間の単位のリテラル（`100ms`）を同じ理由で見送っています。JS だけが持つ綴りで、多数派でもありません。
- issue: なし

### R-bigint-03 `Int` と `BigInt` を混ぜた演算を自動で広げる — × 不採用

- 内容: しません。`a + b` で片方が `Int`、片方が `BigInt` なら型エラーにし、診断のヒントで `int.to_bigint(a)` を示します。
- 根拠: R-L0-07（変換は名前のある関数で書く）。暗黙の変換は、どちらの型の演算が走ったかを読み手から隠します（ADR-0001 が時計を型で分けた理由と同じ）。
- issue: なし

### R-bigint-04 `bigint.add`/`sub`/`mul`/`div`/`rem`/`neg` を公開しない（演算子だけにする） — ○ 採用

- いま: 案では `bigint.add(a, b)` などの 6 関数を公開し、演算子 `+ - * / %` と単項 `-` をそれに脱糖します。
- 案: 6 つを内部の名前（`__add` など）にし、公開の書き方は演算子だけにします。`floor_div`・`mod`・`abs`・`pow` は `int` にもあるので公開のままです。
- 根拠の L0 規則: R-L0-01（同じ役割に綴りは 1 つ）、bigint の完全性の規則 1（`int` と同じ綴り。`int.add`・`int.rem` は無い）、R-L0-26（演算子の脱糖先は公開しない）。前例は R-matrix-02 の `__` の名前です。
- (a) 演算子で書ける言語（Python・JS・Elixir）では関数の綴りは出てきません。関数で書く Go `Add`・JVM `add` は、演算子の多重定義がない言語です。
- (c) `a + b` と `bigint.add(a, b)` の 2 通りがあると、生成されるコードが揺れます。`bigint.rem` があって `int.rem` がない非対称から、LLM は `int.rem` を推測して外れます。
- (d) 新しいモジュールなので互換の影響はありません。Java 由来の `bigint.add` には「`a + b` と書く」案内を出します（R-L0-19）。
- issue: #3973

### R-bigint-05 `bigint.clamp` を足す — ○ 採用

- 案: `fn bigint.clamp(n: BigInt, lo: BigInt, hi: BigInt) -> BigInt`。`lo > hi` は `int.clamp` と同じく abort します。
- 根拠の L0 規則: bigint の完全性の規則 1（`int` に同じ役割があれば同じ名前で置く）。`min`・`max` を置いたのに `clamp` だけが漏れていました。
- (a) Rust `Ord::clamp`、Kotlin `coerceIn`、Gleam `int.clamp`。
- (d) 追加 1。新しいモジュールです。
- ゲート案: bigint のゲートを「`int` にあって除外表にないものは bigint にもある」の向きでも検査します。
- issue: #3972

### R-bigint-06 `uint64` と `BigInt` を値を失わずに行き来させる — ○ 採用

- いま: bigint の規則 2 は「固定幅の型との直接の変換は置かない（`to_int` を経由する）」ですが、`uint64.to_int` は 2^63 以上を負の数に折り返します（実測: `int.from_uint64(uint64.max_value())` は `-1`）。
- 案: `fn uint64.to_bigint(x: UInt64) -> BigInt`（値を失わない）と、`fn bigint.to_uint64(b: BigInt) -> UInt64` の 3 つ組（`_checked`・`_saturating`）。
- 根拠の L0 規則: R-L0-07（変換は元の型の `to_<先>`）、R-L0-08（損失のある向きは 3 つ組）。
- (a) Go `SetUint64`/`Uint64`/`IsUint64`、JS `BigInt.asUintN(64, b)`。
- (d) 追加 4。
- 完全性の規則: `to_int` を経由すると値を失う運び手（いまは `uint64` だけ）は、`BigInt` と直接つなぎます。
- issue: #3972

### R-bigint-07 `bigint` に `Integer` のビット演算を置く — ○ 採用

- いま: `bigint` は `Integer` の算術（`floor_div`・`mod`・`pow`・`is_even`・`is_odd`）を持ちますが、ビット演算を持ちません。
- 案: `band`・`bor`・`bxor`・`bnot`・`bshl`・`bshr` を置きます（`int` と同じ関数。R-int-10）。負の数は、上に無限に 1 が続く 2 の補数として扱います（Python の `int`、Swift の `BinaryInteger` と同じ）。`bnot(x)` は `-x - 1` です。
- 意図した抜け: `count_leading_zeros`・`count_trailing_zeros`・`count_ones`・`byte_swap` は置きません（`FixedWidthInteger` の要件で、幅のない型には決まった答えがない）。
- (a) Swift の `BinaryInteger` はビット演算を要件にし、任意精度の整数もこれを持つ設計です。Python の `int`、Go の `big.Int`（`And`・`Or`・`Xor`・`Not`・`Lsh`・`Rsh`）、Java/Kotlin の `BigInteger`（`and`・`or`・`xor`・`not`・`shiftLeft`・`shiftRight`）も持ちます。
- (d) 追加 6。
- issue: #4000

## L3: 関数表（提案）

| 関数 | シグネチャ(提案) | 他言語の主な綴り | 判定 | 案 |
|---|---|---|---|---|
| `parse` | `bigint.parse(s: String) -> Result[BigInt, String]` | Go `SetString` / Python `int(s)` / JS `BigInt(s)` / Elixir `Integer.parse` (F) | 追加 | R-bigint-01 |
| `parse_radix` | `bigint.parse_radix(s: String, radix: Int) -> Result[BigInt, String]` | Go `SetString(s, base)` / Python `int(s, base)` / Kotlin `BigInteger(s, radix)` | 追加 | R-bigint-01 |
| `to_string` | `bigint.to_string(b: BigInt) -> String` | Go `String` / Python `str` / JS `toString` / Elixir `to_string` (F) | 追加 | R-bigint-01 |
| `to_string_radix` | `bigint.to_string_radix(b: BigInt, radix: Int) -> String` | Go `Text(base)` / JS `toString(radix)` / Kotlin `toString(radix)` | 追加 | R-bigint-01 |
| `add` | `bigint.add(a: BigInt, b: BigInt) -> BigInt`（`+`） | Go `Add` / Kotlin `add` / 他は `+` | 追加しない（演算子の内部の脱糖先にする） | R-bigint-04 |
| `sub` | `bigint.sub(a: BigInt, b: BigInt) -> BigInt`（`-`） | Go `Sub` / Kotlin `subtract` | 追加しない（演算子の内部の脱糖先にする） | R-bigint-04 |
| `mul` | `bigint.mul(a: BigInt, b: BigInt) -> BigInt`（`*`） | Go `Mul` / Kotlin `multiply` | 追加しない（演算子の内部の脱糖先にする） | R-bigint-04 |
| `div` | `bigint.div(a: BigInt, b: BigInt) -> BigInt`（`/`、0 方向。0 で割ると abort） | Go `Quo` / Kotlin `divide` / JS `/` | 追加しない（演算子の内部の脱糖先にする） | R-bigint-04 |
| `rem` | `bigint.rem(a: BigInt, b: BigInt) -> BigInt`（`%`） | Go `Rem` / Kotlin `remainder` / JS `%` | 追加しない（演算子の内部の脱糖先にする） | R-bigint-04 |
| `floor_div` | `bigint.floor_div(a: BigInt, b: BigInt) -> BigInt` | Python `//` / Elixir `Integer.floor_div` (F) | 追加 | R-bigint-01（R-int-03 と同じ綴り） |
| `mod` | `bigint.mod(a: BigInt, b: BigInt) -> BigInt` | Python `%` / Elixir `Integer.mod` (F) / Kotlin `mod` | 追加 | R-bigint-01（R-int-03 と同じ綴り） |
| `neg` | `bigint.neg(a: BigInt) -> BigInt`（単項 `-`） | Go `Neg` / Kotlin `negate` | 追加しない（演算子の内部の脱糖先にする） | R-bigint-04 |
| `abs` | `bigint.abs(a: BigInt) -> BigInt` | Go `Abs` / Python `abs` / Kotlin `abs` | 追加 | R-bigint-01 |
| `sign` | `bigint.sign(a: BigInt) -> Int` | Go `Sign` / Kotlin `signum` | 追加 | R-bigint-01 |
| `pow` | `bigint.pow(base: BigInt, exp: Int) -> BigInt`（負の `exp` は abort） | Go `Exp` / Python `**` / Elixir `Integer.pow` (F) / Kotlin `pow` | 追加 | R-bigint-01 |
| `compare` | `bigint.compare(a: BigInt, b: BigInt) -> Int` | Go `Cmp` / Kotlin `compareTo` | 追加 | R-bigint-01 |
| `min` | `bigint.min(a: BigInt, b: BigInt) -> BigInt` | Kotlin `min` / Python `min` | 追加 | R-bigint-01 |
| `max` | `bigint.max(a: BigInt, b: BigInt) -> BigInt` | Kotlin `max` / Python `max` | 追加 | R-bigint-01 |
| `is_even` | `bigint.is_even(a: BigInt) -> Bool` | Elixir `Integer.is_even` (F) / Kotlin `!testBit(0)` | 追加 | R-bigint-01（`int.is_even` と同じ綴り） |
| `is_odd` | `bigint.is_odd(a: BigInt) -> Bool` | Elixir `Integer.is_odd` (F) | 追加 | R-bigint-01 |
| `clamp` | `bigint.clamp(n: BigInt, lo: BigInt, hi: BigInt) -> BigInt` | Rust `clamp` / Kotlin `coerceIn` | 追加 | R-bigint-05 |
| `from` | （追加）`bigint.from[S: Numeric](x: S) -> BigInt` | Rust `From`・`as` / Swift `init(_:)` / Python `int(x)` | 追加 | R-L0-40 |
| `from_checked` | （追加）`bigint.from_checked[S: Numeric](x: S) -> BigInt?` | Rust `TryFrom` / Swift `init(exactly:)` | 追加 | R-L0-40 |
| `bit_length` | （追加）`bigint.bit_length(n: BigInt) -> Int` | Python `bit_length` / Go `BitLen` | 追加 | R-int-11 |
| `band` | `bigint.band(a: BigInt, b: BigInt) -> BigInt` | Elixir `band` / Rust・Go・Swift 演算子 `&` | 追加 | R-bigint-07 |
| `bor` | `bigint.bor(a: BigInt, b: BigInt) -> BigInt` | Elixir `bor` / Rust・Go・Swift 演算子 `\|` | 追加 | R-bigint-07 |
| `bxor` | `bigint.bxor(a: BigInt, b: BigInt) -> BigInt` | Elixir `bxor` / Rust・Go・Swift 演算子 `^` | 追加 | R-bigint-07 |
| `bnot` | `bigint.bnot(a: BigInt) -> BigInt` | Elixir `bnot` / Swift `~` | 追加 | R-bigint-07 |
| `bshl` | `bigint.bshl(a: BigInt, n: Int) -> BigInt` | Elixir `bsl` / 演算子 `<<` | 追加 | R-bigint-07 |
| `bshr` | `bigint.bshr(a: BigInt, n: Int) -> BigInt` | Elixir `bsr` / 演算子 `>>` | 追加 | R-bigint-07 |
