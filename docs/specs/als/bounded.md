# ALS §B — The Bounded Profile (normative)

> Last updated: 2026-08-21

> **Status**: normative（ADR-0017、2026-08-21 裁定）。本章は**有界プロファイル**
> — 言語の**サブセット**であって方言ではない — を定める。`@bounded` を付けた関数は、
> 実行時間と記憶域が**静的に有界**で、呼び出しグラフが**閉じ**、効果が **capability
> で有界**であることを、実装が**検査器の判定として**保証する。判定は観測可能
> （受理 / 拒否コード）であり、各節の拒否規則は `tests/diagnostics/e07x-bounded-*/`
> の broken / fixed 対で実行検証される。本章は実装より先に規範化された
> （CONTRIBUTING「要求が先」）— 2026-08-21 時点の 0.58.0 リリースは `@bounded` を
> 未知の属性として警告（E053）し拒否規則を持たないので、本章の拒否テストはその
> リリースに対して赤である。それは判定者がリリースを記述している状態であり、
> 実装がピンを前進させると緑になる。
>
> 「bounded」はこの言語で**上限を持つこと**を指す一語である: `fan.bounded(c) { … }`
> （ALS-DT2）は計算を**実行時に予算で**有界化し、`@bounded`（本章）は関数を
> **コンパイル時に証明で**有界化する。機構は異なり、意味は同じ。
>
> **本章が表現しないもの（限界の自己申告 — 主張と取り違えないために）**:
> (1) 有界な深さの再帰（全面禁止、B6）; (2) 固定小数点および決定性 Float 演算
> （Float 演算は暫定禁止、B10 — 解禁条件は ALS-T19 ファミリーの規範化と認証席の
> Float 命令集合の両方）; (3) 静的サイズ配列（全ヒープ容器は動的サイズ、B8 は
> 実行時長の構築を拒否する）; (4) バイト単位のメモリ上限（オブジェクト数上限のみ）;
> (5) 上限以下（`≤ B`）の早期脱出（B5/B11 は厳密上限のため禁止）; (6) 機能正しさの
> トレーサビリティ — 本プロファイルは**安全**（メモリ・名前・capability・上限）を
> 保証し、制御則が**正しい**かは保証しない。

## ALS-B1 `@bounded` 属性と有界プロファイル

`@bounded` は `fn` / `effect fn` 宣言に置く属性で、その関数が有界プロファイルに
属することを宣言する。属性を置ける位置は**関数宣言のみ**（モジュール宣言に置く
糖衣は存在しない）。`@bounded` 関数が呼んでよいのは `@bounded` 関数と pure な
標準ライブラリモジュールの一階関数だけであり（B7）、ゆえに `@bounded` 関数から
到達可能な呼び出しグラフは閉じている。属性は型にも値にも影響しない: `@bounded`
関数は通常の関数としてそのまま呼べる。

プロファイルの各規則に違反した `@bounded` 関数は**型検査時に拒否**され、診断
コードは **E070–E079**（本章に予約）、メッセージは
`<construct> is not admissible in a @bounded function` の形で、各節が名指す
hint を伴う。違反は `@bounded` でない関数には一切影響しない（サブセットは
属性の内側だけを狭める）。

```almide
@bounded
fn scale(x: Int) -> Int = x * 3

test "a @bounded function is an ordinary function" {
  assert_eq(scale(4), 12)
}
```

テスト: `spec/wasm_cross/bounded_kernel.almd`、`spec/stdlib/bounded_profile_test.almd`。
Contracts: C-308。

## ALS-B2 サブセットであって方言ではない

`@bounded` が付いたプログラムは、属性を全て取り除いても**同じプログラム**である:
観測可能な挙動（stdout・stderr・exit code）は属性の有無で変わらない（SPARK ⊂ Ada
と同じ関係）。属性が変えるのは「検査器がさらに何を拒否するか」だけである。
`spec/wasm_cross/bounded_kernel.almd` と `bounded_kernel_plain.almd` は属性の
有無だけが異なる同一プログラムで、同じ出力を印字する。

```almide
@bounded
fn grid_sum() -> Int = {
  var acc = 0
  for i in 0..<4 { for j in 0..<3 { acc = acc + i * j } }
  acc
}

fn grid_sum_plain() -> Int = {
  var acc = 0
  for i in 0..<4 { for j in 0..<3 { acc = acc + i * j } }
  acc
}

test "the attribute changes nothing observable" {
  assert_eq(grid_sum(), grid_sum_plain())
  assert_eq(grid_sum(), 18)
}
```

テスト: `spec/wasm_cross/bounded_kernel.almd`、`spec/wasm_cross/bounded_kernel_plain.almd`。
Contracts: C-309。

## ALS-B3 回数付きループのみ

`@bounded` 関数のループは**回数付きループ**に限る: `for i in a..<b` / `for i in a...b`
で、`a`・`b` が**コンパイル時定数**（整数リテラル、またはリテラルから定数畳み込み
できる束縛）のもの。反復回数は範囲長として静的に決まり、入れ子は許される
（上限は各ループの回数の積）。`while`、コンテナに対する `for x in xs`、および
実行時値を端に持つ範囲は **E070** で拒否される（hint: `counted range`）。

```almide
@bounded
fn triangle() -> Int = {
  var acc = 0
  for i in 1...10 { acc = acc + i }
  acc
}

test "counted loops with literal bounds are admissible" {
  assert_eq(triangle(), 55)
}
```

```almide check-fail=E070
@bounded
fn count_down(n: Int) -> Int = {
  var k = n
  while k > 0 { k = k - 1 }
  k
}

fn main() -> Unit = println("${count_down(3)}")
```

テスト: `tests/diagnostics/e070-bounded-unbounded-loop/`、
`tests/diagnostics/e070-bounded-for-over-container/`。Contracts: C-310。

## ALS-B4 ループ内確保の禁止

回数付きループの本体で**新しいヒープオブジェクトを確保する**式 — リスト / 文字列 /
Map / Set の構築・連結・`push`、クロージャの生成 — は **E071** で拒否される
（hint: `allocate outside the loop`）。既存のヒープ値を読む・借りる・参照カウント
を増やすだけの操作（添字読み、フィールド読み、同じ値の受け渡し）は許される。
ループの外での確保は許される。

```almide
@bounded
fn dot(xs: List[Int], ys: List[Int]) -> Int = {
  var acc = 0
  for i in 0..<3 { acc = acc + (list.get(xs, i) ?? 0) * (list.get(ys, i) ?? 0) }
  acc
}

test "reading heap values inside a counted loop is admissible" {
  assert_eq(dot([1, 2, 3], [4, 5, 6]), 32)
}
```

```almide check-fail=E071
@bounded
fn squares() -> List[Int] = {
  var xs: List[Int] = []
  for i in 0..<8 { xs = xs + [i * i] }
  xs
}

fn main() -> Unit = println("${list.len(squares())}")
```

テスト: `tests/diagnostics/e071-bounded-alloc-in-loop/`。Contracts: C-311。

## ALS-B5 `break` / `continue` の禁止

`@bounded` 関数のループ本体に `break` / `continue` は置けない — **E072**
（hint: `single exit`）。反復回数は厳密に範囲長であり、早期脱出は「上限以下」へ
弱めるので許さない（MISRA の single-exit と同じ規律）。

```almide check-fail=E072
@bounded
fn first_cube_over(limit: Int) -> Int = {
  var found = 0
  for i in 0..<100 {
    if i * i * i > limit then break
    found = i
  }
  found
}

fn main() -> Unit = println("${first_cube_over(100)}")
```

テスト: `tests/diagnostics/e072-bounded-break/`。Contracts: C-312。

## ALS-B6 再帰の禁止 — 呼び出しグラフの非循環

`@bounded` 関数から到達可能な呼び出しグラフは **DAG** でなければならない: 自己再帰・
相互再帰（back edge）は **E073** で拒否される（hint: `call graph`）。解決できない
呼び出し先（別ファイルの未解決関数）は隠れた再帰辺を通さないため**保守的に拒否**
される。再帰的**型**（`Boxed`）を持つことは許されるが、再帰による走査は許されない。

```almide
@bounded
fn factorial_10() -> Int = {
  var acc = 1
  for i in 1...10 { acc = acc * i }
  acc
}

test "iteration replaces recursion" {
  assert_eq(factorial_10(), 3628800)
}
```

```almide check-fail=E073
@bounded
fn fact(n: Int) -> Int = if n <= 1 then 1 else n * fact(n - 1)

fn main() -> Unit = println("${fact(5)}")
```

テスト: `tests/diagnostics/e073-bounded-recursion/`。Contracts: C-313。

## ALS-B7 呼び出し閉包 — 呼べるもの

`@bounded` 関数が呼べるのは、(a) `@bounded` 関数、(b) pure な標準ライブラリ
モジュール（`int` `float` `string` `list` `map` `set` `math` `option` `result`
`value` `json` `bytes` `regex` `matrix` 等）の**一階**関数、の二種だけである。
次は **E074** で拒否される（hint: `@bounded callee`）: `@bounded` でないユーザ関数の
呼び出し、クロージャ / ラムダの生成と呼び出し、高階関数（`list.map(xs, f)` 等、
関数値を渡す呼び出し）、関数参照、`any P` による動的ディスパッチ。（一階のみの
理由: 間接呼び出し先は静的な呼び出しグラフ・capability・上限の全てを失う。）

```almide
@bounded
fn double(x: Int) -> Int = x * 2

@bounded
fn quadruple(x: Int) -> Int = double(double(x))

test "@bounded calls @bounded and first-order stdlib" {
  assert_eq(quadruple(5), 20)
  assert_eq(math.max(quadruple(1), 3), 4)
}
```

```almide check-fail=E074
fn helper(x: Int) -> Int = x + 1

@bounded
fn uses_plain(x: Int) -> Int = helper(x)

fn main() -> Unit = println("${uses_plain(1)}")
```

テスト: `tests/diagnostics/e074-bounded-unbounded-callee/`、
`tests/diagnostics/e074-bounded-higher-order/`。Contracts: C-314。

## ALS-B8 実行時長のヒープ構築の禁止

長さ・サイズが**実行時の値**で決まるヒープ構築 — `string.repeat(s, n)`、
`list.with_capacity(n)`、`list.range(a, b)` の `n`/`a`/`b` が定数でないもの、実行時
長の文字列 / リストの生成 — は **E075** で拒否される（hint: `compile-time size`）。
リテラル（`[1, 2, 3]`、`"abc"`）と定数サイズの構築は許される。ループ外での `push` は
許される（B4 はループ内を禁じる）。

```almide check-fail=E075
@bounded
fn pad(n: Int) -> String = string.repeat("x", n)

fn main() -> Unit = println(pad(3))
```

テスト: `tests/diagnostics/e075-bounded-runtime-length-heap/`。Contracts: C-315。

## ALS-B9 効果と capability

`@bounded effect fn` は宣言した capability の範囲でのみ効果を持ち、使える効果は
標準出力（`println` 系）に限る。次の呼び出しは **E076** で拒否される（hint:
`declared capability`）: 効果モジュール `env` `fs` `http` `io`（print 以外）`net`
`process` `random` `zlib`、`effect` を伴わずホストに到達するモジュール `datetime`
`args` `mem` `testing`、および `fan.*`（非決定スケジューリング）。

```almide check-fail=E076
import env

@bounded
effect fn which_os() -> String = env.os()

effect fn main() -> Unit = println(which_os() ?? "unknown")
```

テスト: `tests/diagnostics/e076-bounded-effect-outside-capability/`。Contracts: C-316。

## ALS-B10 浮動小数演算の禁止（暫定）

`@bounded` 関数内の Float に対する演算子（算術・比較）と、Float を受け取る標準
ライブラリ関数の呼び出しは **E077** で拒否される（hint: `Int arithmetic`）。Float
型の値を保持し受け渡すこと自体は許される。この規則は**暫定**で、ALS-T19 の数値
決定性ファミリーが規範であり、かつ認証席に Float 命令集合が存在する時点で改訂
される — 黙って緩めることはない。

```almide check-fail=E077
@bounded
fn scale(x: Float) -> Float = x * 2.0

fn main() -> Unit = println(float.to_string(scale(1.5)))
```

テスト: `tests/diagnostics/e077-bounded-float-arithmetic/`。Contracts: C-317。

## ALS-B11 早期脱出の制限

回数付きループの**本体内**での早期脱出 — `!` による伝播（effect fn）、`guard … else` —
は **E078** で拒否される（hint: `outside the loop`）。ループの外での伝播と guard は
許される（関数全体の早期 return は有界）。（`?` は Result → Option の変換であって
脱出ではない — ADR-0008。）理由は B5 と同じ: 本体からの途中脱出は
反復ごとの heap 解放を飛ばし、厳密な回数上限を壊す。

```almide
@bounded
fn sum_or_zero(xs: List[Int]) -> Int = {
  var acc = 0
  for i in 0..<3 { acc = acc + (list.get(xs, i) ?? 0) }
  acc
}

test "defaulting inside the loop is admissible" {
  assert_eq(sum_or_zero([4, 5]), 9)
}
```

```almide check-fail=E078
@bounded
effect fn sum_first(xs: List[Int]) -> Int! = {
  var acc = 0
  for i in 0..<3 { acc = acc + list.get(xs, i)! }
  ok(acc)
}

effect fn main() -> Unit = println("${sum_first([1, 2, 3]) ?? -1}")
```

テスト: `tests/diagnostics/e078-bounded-early-exit-in-loop/`。Contracts: C-318。
