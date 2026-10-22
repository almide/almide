<!-- description: Stdlib redesign L2/L3 — option: retire the eager unwrap_or on the shared window, keep collect, and gate the option/result family as one matrix -->
# L2/L3: option

> Last updated: 2026-10-21
> 前提: [L0](L0-conventions.md)・[L1](L1-categories.md)・共通の判定 [L2-result](L2-result.md)
> 出典: Rust https://doc.rust-lang.org/std/option/enum.Option.html (F) ・ Go — (Option 型なし。comma-ok https://go.dev/ref/spec#Index_expressions) ・ Python — (`None` と型注釈だけ https://docs.python.org/3/library/typing.html#typing.Optional) ・ TS/JS — (`?.` と `??` https://developer.mozilla.org/en-US/docs/Web/JavaScript/Reference/Operators/Nullish_coalescing) ・ Gleam https://gleam-stdlib.hexdocs.pm/gleam/option.html (F) ・ Elixir — (`nil` と `||` https://hexdocs.pm/elixir/Kernel.html) ・ OCaml https://ocaml.org/manual/latest/api/Option.html (F) ・ Kotlin https://kotlinlang.org/docs/null-safety.html ・ Swift https://developer.apple.com/documentation/swift/optional

読み方: `??`（読み: ナル合体）= 左が none なら右を使う。`?.`（読み: オプショナルチェーン）= some のときだけ中身に進む。`!`（読み: 伝搬）= effect fn の中で失敗をそのまま返す。どれも ADR-0005 で stdlib の関数に脱糖（読み: だっとう＝別の書き方に置き換えること）されると決まっています。

Option を型として持つのは 9 言語のうち 5 言語（Rust、Gleam、OCaml、Kotlin の `T?`、Swift の `T?`）です。Go・Python・JS・Elixir は「値がない」を `nil`/`None`/`undefined` で表し、関数ではなく演算子や構文で扱います。そのため、この表の「5/9 以上」は、ほぼ「Option を持つ 5 言語すべて」という意味になります。

## L2: 役割の比較

| 役割 | Almide | Rust | Go | Python | TS/JS | Gleam | Elixir | OCaml | Kotlin | Swift |
|---|---|---|---|---|---|---|---|---|---|---|
| 中身を変換 | `option.map` / `?.` | `map` (F) | — | — | `?.` | `map` (F) | — | `map` (F) | `?.let {}` | `map` |
| 変換をつなぐ | `option.flat_map` | `and_then` (F) | — | — | `?.` | `then` (F) | — | `bind` (F) | `?.let {}` | `flatMap` |
| 入れ子を外す | `option.flatten` | `flatten` (F) | — | — | — | `flatten` (F) | — | `join` (F) | — | — |
| 既定値（先に評価） | `option.unwrap_or`（非推奨） | `unwrap_or` (F) | — | `x if x is not None else d` | `??` | `unwrap` (F) | `\|\|` | `value ~default` (F) | `?:` | `??` |
| 既定値（必要なときだけ評価） | `??` = `option.unwrap_or_else` | `unwrap_or_else` (F) | — | — | `??` | `lazy_unwrap` (F) | `\|\|` | — | `?:` | `??` |
| some/none の判定 | `option.is_some` / `is_none` | `is_some` / `is_none` (F) | — | `is not None` | `!== undefined` | `is_some` / `is_none` (F) | `!= nil` | `is_some` / `is_none` (F) | `!= null` | `!= nil` |
| Result に変える | `option.to_result` | `ok_or` (F) | — | — | — | `to_result` (F) | — | `to_result ~none` (F) | — | — |
| 条件で絞る | `option.filter` | `filter` (F) | — | — | — | — | — | — | `takeIf {}` | — |
| 2 つを組にする | `option.zip` | `zip` (F) | — | — | — | — | — | — | — | — |
| none のとき別の Option | `option.or_else` | `or_else` (F) | — | — | — | `lazy_or` (F) | — | — | `?:` | `??` |
| リストに変える | `option.to_list` | `iter` | — | — | — | — | — | `to_list` (F) | `listOfNotNull` | — |
| List[Option] → Option[List]（最初の none で止める） | `option.collect` | `collect` | — | — | — | `all` (F) | — | — | — | — |
| 変換しながら上と同じ | `option.collect_map` | `map(..).collect()` | — | — | — | — | — | — | — | — |
| 中身を取り出し、none なら止める | `?? panic(msg)` | `expect` / `unwrap` (F) | — | — | — | `let assert` | — | `get` (F) | `!!` | `!` |
| List[Option] から some だけ集める | `list.filter_map((x) => x)` | `flatten` | — | — | — | `values` (F) | — | `List.filter_map Fun.id` | `filterNotNull` | `compactMap` |
| 中身をのぞく（副作用） | — | `inspect` (F) | — | — | — | — | — | `iter` (F) | `also` | — |
| none のとき別の Option（先に評価） | — | `or` (F) | — | — | — | `or` (F) | — | — | `?:` | `??` |
| 組をほどく | — | `unzip` (F) | — | — | — | — | — | — | — | — |
| 条件つき判定 | — | `is_some_and` (F) | — | — | — | — | — | — | — | — |

### 完全性の規則

- option と result は 1 つの family（読み: ファミリー＝同じ形の関数の組）として扱います。次のセルは両方に必要です: `map`、`flat_map`、`flatten`、`unwrap_or_else`、`is_<状態>` の 2 つ、`to_<相手の型>`、`filter`、`zip`、`or_else`、`to_list`。
- 意図して片方だけにあるもの:
  - `collect` / `collect_map` は option だけ。result 側は ADR-0007 D1 で撤去し、全エラー収集は `result.partition`、最初のエラーで止める形は `list.map(xs, (x) => f(x)!)!`（ADR-0006）が担います。option 側は Rust の `collect` と意味が一致するので残す、と ADR-0007 D4 で決まっています。
  - `map_err`・`to_err_option`・`partition` は result だけ（none は中身を持たないので対がない）。
- 既定値は `??` だけにします（R-L0-06）。先に評価する `unwrap_or` は置きません。

### L2 の判定

- 重複: `option.unwrap_or` は `??` と同じ役割です。#1107 の窓で撤去します（R-option-01）。
- 抜けている役割で 5/9 以上: 「none なら止めて取り出す」（5 言語）と「List[Option] から some だけ集める」（5 言語）。どちらも既存の書き方（`?? panic(msg)`、`list.filter_map((x) => x)`）で書けるので、関数は足しません（R-option-03）。
- family の穴: なし。option と result の対応は、表にしたうえでゲートで守ります（R-option-04）。
- 置き場所: `option.collect` / `collect_map` は第 1 引数が List なので、R-L0-02 の主語の規則だけなら list に置くことになりますが、R-L0-02 の但し書き（入れ物の要素の型に意味がある関数は、要素の型のモジュールに置く）に当たるので、option に置くのが規則どおりです（R-option-02）。

## 案

### R-option-01 `option.unwrap_or` を epoch 16 で撤去する — ○ 採用

- 内容: `option.unwrap_or(o, d)` は `@deprecated(since=3, use="??")` のまま、撤去の日付がありません。`proofs/dialect-epochs.toml` に `[[deprecation]]` ブロック（`since_epoch = 3`、`removal_epoch = 16`）を置き、`list.get_or`・`map.get_or`・`result.unwrap_or` と同じ窓で撤去します。撤去後は E002 に「`o ?? d` と書きます。関数の形が要るときは `option.unwrap_or_else`」というヒントを出します（ADR-0005 D4 の文面）。
- 根拠の L0 規則: R-L0-06、R-L0-14
- (a) 先に評価する既定値の関数は Rust・Gleam・OCaml にあります。ただし Almide では `??` が唯一の書き方で、必要なときだけ評価する点も Rust の `unwrap_or_else`、Swift・TS の `??` と同じです。
- (b) 影響なし。`??` は両 target で `unwrap_or_else` に脱糖済みです（C-291）。
- (c) 綴りが `??` だけになれば、先に評価されるかどうかを書き手が気にする必要がなくなります。
- (d) 撤去。`almide fix` は `option.unwrap_or(o, d)` → `o ?? d` を書き換えます（PR #1902 で実装済み）。使用数: spec 100 / tests 15 / docs 30 / stdlib 2（定義を含む）、research 22。#1107 の記録では `.almd` 内に 128 か所。
- ゲート: 撤去後の E002 ヒントの fixture（`tests/diagnostics/`）。`check-interface-diff.sh --allow-breaking` で宣言つきの breaking として通します。
- issue: #1107（既存）

### R-option-02 `option.collect` と `option.collect_map` を残し、置き場所の例外として登録する — ○ 採用（据え置き）

- 内容: 名前も場所も変えません。`option.collect`・`option.collect_map`・`result.partition` は、R-L0-02 の但し書き「入れ物の要素の型に意味がある関数は、要素の型のモジュールに置く」に当たります（L0 本文が `option.collect` と `result.partition` を例に挙げています）。`collect_map` は、関数が返す `Option` が結果の型を決めるので、同じ但し書きに入ります。
- 根拠の L0 規則: R-L0-02、R-L0-00
- (a) Rust は `collect` を Option に向けて使い、Gleam は `option.all` を option モジュールに置いています。List 側に置く言語はありません。
- (b) 純粋な計算です。両 target で同じです。
- (c) `list.collect` に移すと、`list` の中に「Option 専用の関数」が混ざります。しかも ADR-0007 で消した `result.collect` と同じ綴りが別の場所に戻り、Rust の `collect` と書き間違える余地が増えます。
- (d) 変更なし。確認: `option.collect_map` は `list.map((x) => f(x)!)` の Option 版の代わりです。いまの可謬多相（読み: かびゅうたそう）の `list.map` は Result にしか効きません（probe-option/p.almd で E005 を確認）。
- ゲート: R-L0-G の置き場所の検査は、「第 1 引数の型」に加えて「第 1 引数の要素の型、または戻り型の外側の型」がモジュールの主語の型であるものを受け付けるように書きます。名前の許容リストには載せません。
- issue: #3949

### R-option-03 抜けている役割の関数を足さない — × 不採用

- 内容: Rust・Kotlin などにある `expect`、`inspect`、`or`（先に評価）、`unzip`、`is_some_and`、`values` を足しません。
- 根拠の L0 規則: R-L0-00、R-L0-01、R-L0-06
- (a) 5/9 に達するのは「none なら止めて取り出す」と「some だけ集める」の 2 つです。前者は `o ?? panic("msg")` で書けます（probe-option/q.almd で、`??` の右に `panic` が型どおり置けることを確認）。後者は `list.filter_map((x) => x)` です。`inspect`・`or`・`unzip`・`is_some_and` は 4 言語以下です。
- (c) `or` を足すと「先に評価する `or`」と「必要なときだけ評価する `or_else`」の 2 つになり、ADR-0005 が `unwrap_or` で解消した二重綴りを作り直すことになります。`?? some(9)` は型エラー（E001）になり、`option.or_else` に誘導されます。
- (d) なし。
- ゲート: 関数は足しません。代わりに CHEATSHEET の「Unwrap operators」に `?? panic(msg)` の例を 1 つ足し、```` ```almide check ```` のフェンスにして `scripts/check-llm-surface.sh` で検査します。
- issue: #3905（CHEATSHEET の 1 行のみ）

### R-option-04 option と result の対応を行列のゲートにする — ○ 採用

- 内容: 上の「完全性の規則」を `tests/stdlib_surface_conventions_test.rs` の 1 規則にします。option にある族のセルが result にもあること（逆も）、意図して片方だけのセル（option: `collect`・`collect_map`、result: `map_err`・`to_err_option`・`partition`）は理由つきの許容リストに置くことを検査します。
- 根拠の L0 規則: R-L0-01、R-L0-G、CLAUDE.md「API の族は行列で広げる」
- (a) Rust・Gleam・OCaml は、Option と Result にほぼ同じ名前の組を持っています。
- (b) 影響なし。
- (c) 書き手は片方の名前からもう片方を推測します（`option.zip` があれば `result.zip` もあるはず）。いまは揃っていますが、ゲートがないと崩れます。
- (d) 変更なし（ゲートの追加だけ）。
- issue: #3949

## L3: 関数表

| 関数 | シグネチャ(現行, mut 表示) | 他言語の主な綴り | 判定 | 案 |
|---|---|---|---|---|
| `option.map` | `map(o: Option[A], f: (A) -> B) -> Option[B]` | Rust map / Gleam map / OCaml map / Swift map / Kotlin `?.let` | 据え置き | — |
| `option.flat_map` | `flat_map(o: Option[A], f: (A) -> Option[B]) -> Option[B]` | Rust and_then / Gleam then / OCaml bind / Swift flatMap | 据え置き | R-L0-10（`flat_map` の綴りは規則で固定） |
| `option.flatten` | `flatten(o: Option[Option[A]]) -> Option[A]` | Rust flatten / Gleam flatten / OCaml join | 据え置き | — |
| `option.unwrap_or` | `unwrap_or(o: Option[A], default: A) -> A`（deprecated） | Rust unwrap_or / Gleam unwrap / OCaml value / 多数: `??` 演算子 | 撤去 | R-option-01 |
| `option.unwrap_or_else` | `unwrap_or_else(o: Option[A], f: () -> A) -> A` | Rust unwrap_or_else / Gleam lazy_unwrap / Swift・TS `??` | 据え置き | `??` の脱糖先（ADR-0005 D1） |
| `option.is_some` | `is_some(o: Option[A]) -> Bool` | Rust・Gleam・OCaml is_some | 据え置き | — |
| `option.is_none` | `is_none(o: Option[A]) -> Bool` | Rust・Gleam・OCaml is_none | 据え置き | — |
| `option.to_result` | `to_result(o: Option[A], e: E) -> Result[A, E]` | Rust ok_or / Gleam to_result / OCaml to_result | 据え置き | 注: E は多相（probe-error/p.almd で独自の variant E を確認）。ADR-0007 D4 の「E は String 固定」は文面のほうが古い |
| `option.filter` | `filter(o: Option[A], f: (A) -> Bool) -> Option[A]` | Rust filter / Kotlin takeIf | 据え置き | R-option-04 |
| `option.zip` | `zip(a: Option[A], b: Option[B]) -> Option[(A, B)]` | Rust zip | 据え置き | R-option-04 |
| `option.or_else` | `or_else(o: Option[A], f: () -> Option[A]) -> Option[A]` | Rust or_else / Gleam lazy_or / Kotlin・Swift 演算子 | 据え置き | R-option-03 |
| `option.to_list` | `to_list(o: Option[A]) -> List[A]` | OCaml to_list / Kotlin listOfNotNull / Rust iter | 据え置き | — |
| `option.collect` | `collect(xs: List[Option[T]]) -> Option[List[T]]` | Rust collect / Gleam all | 据え置き | R-option-02 |
| `option.collect_map` | `collect_map(xs: List[T], f: (T) -> Option[U]) -> Option[List[U]]` | Rust map().collect() | 据え置き | R-option-02 |

（14 関数。追加なし）

R-L0-05 との関係: `option.map(o, (x) => println(x))` のように副作用のために `map` を文として書くと、`Option[Unit]` を捨てることになり、R-L0-05 の後は E042 になります。その場合は `match o { some(x) => println(x), none => () }` と書きます。

## L0 への差し戻し（処理済み）

- R-L0-02 の例外の型: 第 1 引数が `List[Option[T]]` や `List[Result[T, E]]` で、意味が要素の型で決まる関数（`option.collect`、`option.collect_map`、`result.partition`）は、要素の型のモジュールに置く、という但し書きを R-L0-02 に足すことを提案していました。
  - 処理: R-L0-02 に但し書きとして入りました（「入れ物の要素の型に意味がある関数は、要素の型のモジュールに置く」）。R-option-02 はこの但し書きを根拠にし、許容リストは使いません。
