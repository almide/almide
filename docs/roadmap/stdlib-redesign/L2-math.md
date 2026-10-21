<!-- description: Stdlib redesign L2/L3 — math: keep only what has no type-module home; move abs/min/max/sign/pow to int and sqrt/fmin/fmax/fpow to float; add the missing inverse trig, hyperbolic, hypot and cbrt -->
# L2/L3: math

> Last updated: 2026-10-21
> 前提: [L0](L0-conventions.md)・[L1](L1-categories.md)
> 関連: [L2-int](L2-int.md)・[L2-float](L2-float.md)
> 出典: Rust https://doc.rust-lang.org/std/primitive.f64.html (F) ・ Go https://pkg.go.dev/math ・ Python https://docs.python.org/3/library/math.html ・ TS/JS https://developer.mozilla.org/en-US/docs/Web/JavaScript/Reference/Global_Objects/Math ・ Gleam https://gleam-stdlib.hexdocs.pm/gleam/float.html (F) ・ Elixir https://www.erlang.org/doc/apps/stdlib/math.html ・ OCaml https://ocaml.org/manual/latest/api/Float.html (F) ・ Kotlin https://kotlinlang.org/api/core/kotlin-stdlib/kotlin.math/ ・ Swift https://developer.apple.com/documentation/foundation/1779833-pow

(F) = 2026-10-21 に公式ページの関数一覧を取り寄せて確かめた綴り。Gleam の標準には三角関数がありません（float に `power`・`square_root`・`exponential`・`logarithm` だけ (F)）。

## L2: 役割の比較

| 役割 | Almide | Rust | Go | Python | TS/JS | Gleam | Elixir | OCaml | Kotlin | Swift |
|---|---|---|---|---|---|---|---|---|---|---|
| 定数 π・e | `pi()` `e()` | `consts::PI` | `math.Pi` | `math.pi` | `Math.PI` | — | `:math.pi` | `Float.pi` (F) | `PI` `E` | `.pi` |
| 三角関数 | `sin` `cos` `tan` `atan` | `sin` 等 (F) | `Sin` 等 | `sin` 等 | `Math.sin` 等 | — | `:math.sin` 等 | `sin` 等 (F) | `sin` 等 | `sin` 等 |
| 逆三角関数・`atan2` | `atan` だけ | `asin` `acos` `atan2` (F) | `Asin` `Acos` `Atan2` | `asin` `acos` `atan2` | `asin` `acos` `atan2` | — | `asin` `acos` `atan2` | `asin` `acos` `atan2` (F) | `asin` `acos` `atan2` | `asin` `acos` `atan2` |
| 双曲線関数 | `tanh` だけ | `sinh` `cosh` `tanh` (F) | `Sinh` `Cosh` `Tanh` | あり | あり | — | あり | あり (F) | あり | あり |
| 指数・対数 | `exp` `log` `log10` `log2` | `exp` `ln` `log10` `log2` (F) | `Exp` `Log` 等 | `exp` `log` 等 | `Math.exp` `log` 等 | `exponential` `logarithm` (F) | `exp` `log` 等 | `exp` `log` 等 (F) | `exp` `ln` 等 | `exp` `log` 等 |
| 平方根・立方根・斜辺 | `sqrt`（`float.sqrt` と双子） | `sqrt` `cbrt` `hypot` (F) | `Sqrt` `Cbrt` `Hypot` | `sqrt` `cbrt` `hypot` | `sqrt` `cbrt` `hypot` | `square_root` (F) | `sqrt` | `sqrt` `cbrt` `hypot` (F) | `sqrt` `cbrt` `hypot` | `sqrt` `cbrt` `hypot` |
| 累乗 | `pow`（Int）`fpow`（Float） | `i64::pow` `f64::powf` (F) | `Pow` | `pow` `**` | `**` | `int.power` `float.power` (F) | `Integer.pow` (F) | `Float.pow` (F) | `pow` | `pow` |
| 絶対値・最小・最大・符号（Int） | `abs` `min` `max` `sign`（`int.*` と双子） | Int のメソッド | `min` `max` | 組み込み | `Math.*` | `int.*` (F) | `abs` | `Int.abs` (F) | `abs` `sign` | `abs` |
| 最小・最大（Float） | `fmin` `fmax`（`float.min/max` と双子） | Float のメソッド (F) | `min` `max` | 組み込み | `Math.min` | `float.min` (F) | `min` | `Float.min` (F) | `minOf` | `min` |
| ガンマ関数の対数 | `log_gamma` | `ln_gamma`（nightly）(F) | `Lgamma` | `lgamma` | — | — | — | — | — | `lgamma` |
| 階乗・二項係数 | `factorial` `choose` | — | — | `factorial` `comb` | — | — | — | — | — | — |

### 完全性の規則

1. **置き場所**（R-L0-02）: `math` には、型のモジュールに置き場所がない関数だけを置きます。超越関数（三角・逆三角・双曲線・指数・対数・ガンマ）と定数、整数の組み合わせ論（`factorial`、`choose`）です。主語の型が決まる算術（`abs`、`min`、`max`、`sign`、`pow`、`sqrt`）は `int` か `float` に置きます。
2. **超越関数の族**: 三角関数（`sin` `cos` `tan`）、逆三角関数（`asin` `acos` `atan` `atan2`）、双曲線関数（`sinh` `cosh` `tanh`）がそろって存在します。逆双曲線関数（`asinh` など）は意図的に置きません（標準に持つのは Rust・Python・JS・OCaml・Swift ほかですが、使う場面が少なく、`log` と `sqrt` から 1 行で書けます）。
3. `math` の関数はすべて `Float` を取ります（`factorial` と `choose` の `Int` だけが例外）。`f` 接頭辞で型を区別する綴り（`fmin` `fmax` `fpow`）は置きません。

### L2 の判定

- 重複（#3851 B）: `math.abs` `min` `max` ≡ `int.*`、`math.sqrt` ≡ `float.sqrt`、`math.fmin` `fmax` ≡ `float.min` `max` → R-math-01。
- 置き場所: `math.sign`（Int）、`math.pow`（Int）、`math.fpow`（Float）は主語の型が決まる → R-math-01 で `int.sign`、`int.pow`、`float.pow` に移します。#3851 は「`int.pow` がないので統合するものがない」として `math.pow`・`fpow`・`sign` を残していましたが、その後に決まった L0 の R-L0-02 は「`math` には型モジュールに置き場所がないものだけ」と定めたので、移す側に判定を変えます。
- 抜けている役割: 逆三角関数 `asin` `acos` `atan2`（8/9）、双曲線 `sinh` `cosh`（8/9）、`hypot` `cbrt`（7/9）→ R-math-02。
- 据え置き: `factorial` が 20 を超えると折り返すことは、R-L1-03 の `bigint` が受け皿になります（[L2-bigint](L2-bigint.md)）。

## 案

### R-math-01 型の決まる算術を `int`・`float` に移す — ○ 採用

- 内容:
  - 撤去（残る側がすでにある）: `math.abs` → `int.abs`、`math.min` → `int.min`、`math.max` → `int.max`、（`math.sqrt` → `float.sqrt` は R-math-03 で取り消し）、`math.fmin` → `float.min`、`math.fmax` → `float.max`。
  - 移動（追加と撤去の組）: `math.sign` → `int.sign(n: Int) -> Int`、`math.pow` → `int.pow(base: Int, exp: Int) -> Int`、`math.fpow` → `float.pow(base: Float, exp: Float) -> Float`。意味は変えません（`int.pow` は折り返し、負の `exp` は abort。`int.sign(0)` は 0）。
- 根拠の L0 規則: R-L0-01、R-L0-02。
- (a) 主語の型のモジュールやメソッドに置く言語は 7/9（Rust・Kotlin・Swift はメソッド、Gleam・Elixir・OCaml は型モジュール）。Gleam は `int.power` と `float.power` を型ごとに持っています (F)。
- (b) 本体は同じです（`runtime/rs/src/math.rs` と `int.rs`・`float.rs` の同じ計算、#3851 B）。`int.pow` と `float.pow` は `math.pow`・`math.fpow` の intrinsic と self-host 本体をそのまま付け替えます。両 target で変わりません。
- (c) `int.abs` と `math.abs` のように置き場所が 2 つあると、LLM はどちらに置かれているかを当てられません。`fpow` の `f` 接頭辞は 9 言語のどれにもない綴りで、`float.pow` のほうが規則（「Float の関数は `float.`」）から当たります。
- (d) 撤去 9、追加 3（`int.sign`・`int.pow`・`float.pow`。表は L2-int・L2-float）。`@deprecated(since=15, use="int.abs")` など、removal epoch 16。`almide fix` はすべて 1 対 1（`math.fpow(a, b)` → `float.pow(a, b)` など）。使用数（spec/tests/docs/stdlib）: `abs` 13/4/9/0、`min` 7/1/10/0、`max` 7/0/10/0、`sqrt` 22/0/11/2（+20 research）、`fmin` 12/4/10/2、`fmax` 11/0/8/0、`sign` 6/0/6/1、`pow` 20/1/19/3（+8 research）、`fpow` 35/3/12/6。llms.txt:413・419・426 の例を書き換えます。
- ゲート案: #3851 F の同義語ゲート規則 5（モジュールをまたいで本体が同じもの）で `math`↔`int`/`float` の再発を止めます。R-L0-G の置き場所の検査（`math` の公開関数の第 1 引数が `Float`、例外は `factorial`・`choose` だけ）を許容リスト 0 で置きます。
- issue: #3851 B（既存。`sign`・`pow`・`fpow` の判定を変えたことをこの issue に追記）

### R-math-02 逆三角関数・双曲線関数・`hypot`・`cbrt` を足す — ○ 採用

- 内容: `math.asin(x: Float) -> Float`、`math.acos(x: Float) -> Float`、`math.atan2(y: Float, x: Float) -> Float`、`math.sinh(x: Float) -> Float`、`math.cosh(x: Float) -> Float`、`math.hypot(x: Float, y: Float) -> Float`、`math.cbrt(x: Float) -> Float`。定義域の外は NaN（`asin(2.0)`）、既存の `math.*` と同じ流儀。
- 根拠の L0 規則: L1 の追加基準（7〜8/9）、完全性の規則 2。`atan2` の引数の順（y, x）は 9 言語すべてで同じです。
- (a) Rust (F)・Go・Python・JS・Elixir（`:math`）・OCaml (F)・Kotlin・Swift。Gleam だけが持ちません。
- (b) 純粋な計算です。wasm には三角関数の命令がないので、既存の `math_trig.almd`・`math_atan.almd` と同じく musl の移植を Almide で書き、ADR-0016（数値の決定性）の検品対象に入れます。native は libm ではなく同じ Almide 本体を通すか、byte 一致を fixture で確かめます。
- (c) 角度の計算で LLM が最初に書くのは `atan2` です。いまは `atan(y / x)` で象限を間違えます。
- (d) 追加 7。使用数 0。
- ゲート案: 既存の数値検品（semantics-manifest の行と inspection ledger）に 7 行を足し、両 target の byte 一致を wasm_cross fixture で固定します。
- issue: #3926

### R-math-03 実数の数学関数は `math` に集める（`sqrt` を `math` に戻し、`float.sqrt` を撤去） — ○ 採用（R-math-01 の `sqrt` の部分を見直す）

- いま: R-math-01 で `math.sqrt` を撤去して `float.sqrt` に寄せ、R-math-02 で `cbrt`・`hypot` を `math` に足しました。`sqrt` だけが `float`、`cbrt`・`hypot` と三角・指数は `math` という分け方です。
- 案: `math.sqrt(x: Float) -> Float` を残し、`float.sqrt` を撤去します。置き場所の境界を次の 1 文にします。「`Int` にも同じ役割があるもの（`abs`・`min`・`max`・`clamp`・`sign`・`pow`）は型のモジュール。実数だけの数学関数（`sqrt`・`cbrt`・`hypot`、三角・指数・対数・ガンマ）は `math`。丸め・判定・定数・ビット列・変換は表現の関数として `float`」。
- 根拠の L0 規則: R-L0-02（置き場所は 1 つの規則で決まる。L0 の文言をこの 1 文に直します）、R-L0-00 の 2、R-L0-01。
- (a) 9 言語のうち 8 言語は `sqrt` と `sin` を同じ場所に置き、そのうち 6 言語（Go・Python・JS・Elixir・Kotlin・Swift）は math の側です。`sqrt` を `cbrt`・`hypot` と別の場所に置く言語はありません。
- (c) いまの案では `math.cbrt(x)` は通るのに `math.sqrt(x)` は E002 です。`math.sqrt` は LLM がいちばん書きやすい綴りです。
- (d) `float.sqrt` の撤去（`@deprecated(since=15, use="math.sqrt")`、epoch 16、`almide fix` で 1 対 1）。`math.sqrt` の撤去はやめます。使用数は `math.sqrt` 17/0/5/0/18、`float.sqrt` 14/2/4/1/0（spec/tests/docs/stdlib/research）で、こちらのほうが書き換えが少なくて済みます。
- 完全性の規則: `math` の公開関数は引数がすべて `Float`（`factorial`・`choose` は例外）。`float` と `math` に同じ名前は置きません。
- ゲート案: R-L0-G に `math`/`float` の名前の振り分け表を足し、両モジュールの公開名と突き合わせます。
- issue: #3972

## L3: 関数表

`math` の現行 23 関数と、追加する 7 関数です。

| 関数 | シグネチャ(現行, mut 表示) | 他言語の主な綴り | 判定 | 案 |
|---|---|---|---|---|
| `abs` | `math.abs(n: Int) -> Int` | Rust `abs` / Gleam `int.absolute_value` (F) / OCaml `Int.abs` (F) / Kotlin `absoluteValue` (F) | 撤去 | R-math-01（`int.abs`） |
| `atan` | `math.atan(x: Float) -> Float` | Rust `atan` (F) / Go `Atan` / Python・JS `atan` / OCaml `atan` (F) / Kotlin `atan` | 据え置き | — |
| `choose` | `math.choose(n: Int, k: Int) -> Int` | Python `math.comb` / 他 — | 据え置き | — |
| `cos` | `math.cos(x: Float) -> Float` | Rust `cos` (F) / Go `Cos` / Python・JS `cos` / OCaml `cos` (F) | 据え置き | — |
| `e` | `math.e() -> Float` | Rust `consts::E` / Go `math.E` / Python `math.e` / JS `Math.E` / Kotlin `E` | 据え置き | — |
| `exp` | `math.exp(x: Float) -> Float` | Rust `exp` (F) / Go `Exp` / Gleam `exponential` (F) / OCaml `exp` (F) | 据え置き | — |
| `factorial` | `math.factorial(n: Int) -> Int` | Python `math.factorial` / 他 — | 据え置き | —（大きな値は [L2-bigint](L2-bigint.md)） |
| `fmax` | `math.fmax(a: Float, b: Float) -> Float` | Rust `f64::max` (F) / Go `max` / Gleam `float.max` (F) / OCaml `Float.max` (F) | 撤去 | R-math-01（`float.max`） |
| `fmin` | `math.fmin(a: Float, b: Float) -> Float` | Rust `f64::min` (F) / Go `min` / Gleam `float.min` (F) / OCaml `Float.min` (F) | 撤去 | R-math-01（`float.min`） |
| `fpow` | `math.fpow(base: Float, exp: Float) -> Float` | Rust `powf` (F) / Go `Pow` / Gleam `float.power` (F) / OCaml `Float.pow` (F) / Kotlin `pow` | 撤去 | R-math-01（`float.pow`） |
| `log` | `math.log(x: Float) -> Float` | Rust `ln` (F) / Go `Log` / Python・JS `log` / OCaml `log` (F) / Kotlin `ln` | 据え置き | — |
| `log10` | `math.log10(x: Float) -> Float` | Rust `log10` (F) / Go `Log10` / Python・JS `log10` / OCaml `log10` (F) | 据え置き | — |
| `log2` | `math.log2(x: Float) -> Float` | Rust `log2` (F) / Go `Log2` / Python・JS `log2` / OCaml `log2` (F) | 据え置き | — |
| `log_gamma` | `math.log_gamma(x: Float) -> Float` | Rust `ln_gamma`（nightly）(F) / Go `Lgamma` / Python `lgamma` / Swift `lgamma` | 据え置き | — |
| `max` | `math.max(a: Int, b: Int) -> Int` | Rust `max` / Gleam `int.max` (F) / OCaml `Int.max` (F) | 撤去 | R-math-01（`int.max`） |
| `min` | `math.min(a: Int, b: Int) -> Int` | Rust `min` / Gleam `int.min` (F) / OCaml `Int.min` (F) | 撤去 | R-math-01（`int.min`） |
| `pi` | `math.pi() -> Float` | Rust `consts::PI` / Go `math.Pi` / Python `math.pi` / JS `Math.PI` / OCaml `Float.pi` (F) | 据え置き | — |
| `pow` | `math.pow(base: Int, exp: Int) -> Int` | Rust `i64::pow` / Gleam `int.power`（Float）(F) / Elixir `Integer.pow` (F) | 撤去 | R-math-01（`int.pow`） |
| `sign` | `math.sign(n: Int) -> Int` | Rust `signum` / Kotlin `sign` (F) / JS `Math.sign` | 撤去 | R-math-01（`int.sign`） |
| `sin` | `math.sin(x: Float) -> Float` | Rust `sin` (F) / Go `Sin` / Python・JS `sin` / OCaml `sin` (F) | 据え置き | — |
| `sqrt` | `math.sqrt(x: Float) -> Float` | Rust `sqrt` (F) / Go `Sqrt` / Gleam `float.square_root` (F) / OCaml `Float.sqrt` (F) | 据え置き | R-math-03（R-math-01 の見直し） |
| `tan` | `math.tan(x: Float) -> Float` | Rust `tan` (F) / Go `Tan` / Python・JS `tan` / OCaml `tan` (F) | 据え置き | — |
| `tanh` | `math.tanh(x: Float) -> Float` | Rust `tanh` (F) / Go `Tanh` / Python・JS `tanh` / OCaml `tanh` (F) | 据え置き | — |
| `asin` | （追加）`math.asin(x: Float) -> Float` | Rust `asin` (F) / Go `Asin` / Python・JS `asin` / OCaml `asin` (F) / Kotlin `asin` / Swift `asin` | 追加 | R-math-02 |
| `acos` | （追加）`math.acos(x: Float) -> Float` | Rust `acos` (F) / Go `Acos` / Python・JS `acos` / OCaml `acos` (F) / Kotlin `acos` / Swift `acos` | 追加 | R-math-02 |
| `atan2` | （追加）`math.atan2(y: Float, x: Float) -> Float` | Rust `y.atan2(x)` (F) / Go `Atan2` / Python・JS `atan2` / OCaml `atan2` (F) / Kotlin `atan2` / Swift `atan2` | 追加 | R-math-02 |
| `sinh` | （追加）`math.sinh(x: Float) -> Float` | Rust `sinh` (F) / Go `Sinh` / Python・JS `sinh` / OCaml `sinh` (F) | 追加 | R-math-02 |
| `cosh` | （追加）`math.cosh(x: Float) -> Float` | Rust `cosh` (F) / Go `Cosh` / Python・JS `cosh` / OCaml `cosh` (F) | 追加 | R-math-02 |
| `hypot` | （追加）`math.hypot(x: Float, y: Float) -> Float` | Rust `hypot` (F) / Go `Hypot` / Python・JS `hypot` / OCaml `hypot` (F) / Kotlin `hypot` | 追加 | R-math-02 |
| `cbrt` | （追加）`math.cbrt(x: Float) -> Float` | Rust `cbrt` (F) / Go `Cbrt` / Python・JS `cbrt` / OCaml `cbrt` (F) / Kotlin `cbrt` | 追加 | R-math-02 |
