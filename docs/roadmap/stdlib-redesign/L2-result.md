<!-- description: Stdlib redesign L2/L3 — result: retire the eager unwrap_or with option's, keep filter / to_err_option / partition, and add nothing the operators already cover -->
# L2/L3: result

> Last updated: 2026-10-21
> 前提: [L0](L0-conventions.md)・[L1](L1-categories.md)・対になるページ [L2-option](L2-option.md)
> 出典: Rust https://doc.rust-lang.org/std/result/enum.Result.html (F) ・ Go — (`(T, error)` の 2 値 https://pkg.go.dev/errors) ・ Python — (例外 https://docs.python.org/3/tutorial/errors.html) ・ TS/JS — (例外と Promise https://developer.mozilla.org/en-US/docs/Web/JavaScript/Reference/Statements/try...catch) ・ Gleam https://gleam-stdlib.hexdocs.pm/gleam/result.html (F) ・ Elixir — (`{:ok, v}` / `{:error, e}` と `with` https://hexdocs.pm/elixir/Kernel.SpecialForms.html#with/1) ・ OCaml https://ocaml.org/manual/latest/api/Result.html (F) ・ Kotlin https://kotlinlang.org/api/core/kotlin-stdlib/kotlin/-result/ (F) ・ Swift https://developer.apple.com/documentation/swift/result

読み方: `??`（読み: ナル合体）= err なら右を使う。`?`（読み: オプション化）= Result を Option に変える。`!`（読み: 伝搬）= effect fn の中で err をそのまま返す（ADR-0008 で明示だけ）。どれも ADR-0005 で stdlib の関数に脱糖（読み: だっとう）されます。

Result を型として持つのは Rust、Gleam、OCaml、Kotlin、Swift の 5 言語です。Go は 2 値、Python・JS は例外、Elixir はタプルで表します。

## L2: 役割の比較

| 役割 | Almide | Rust | Go | Python | TS/JS | Gleam | Elixir | OCaml | Kotlin | Swift |
|---|---|---|---|---|---|---|---|---|---|---|
| ok の中身を変換 | `result.map` | `map` (F) | — | — | `then` | `map` (F) | `with` | `map` (F) | `map` (F) | `map` |
| err の中身を変換 | `result.map_err` | `map_err` (F) | `fmt.Errorf("%w")` | `raise .. from` | — | `map_error` (F) | — | `map_error` (F) | `recover` (F) | `mapError` |
| 変換をつなぐ | `result.flat_map` | `and_then` (F) | — | — | `then` | `try` (F) | `with` | `bind` (F) | — | `flatMap` |
| 入れ子を外す | `result.flatten` | `flatten` (F, 1.89) | — | — | — | `flatten` (F) | — | `join` (F) | — | — |
| 既定値（先に評価） | `result.unwrap_or`（非推奨） | `unwrap_or` (F) | — | — | — | `unwrap` (F) | — | `value ~default` (F) | `getOrDefault` (F) | — |
| 既定値（必要なときだけ評価） | `??` = `result.unwrap_or_else` | `unwrap_or_else` (F) | — | — | `catch` | `lazy_unwrap` (F) | — | — | `getOrElse` (F) | — |
| ok/err の判定 | `result.is_ok` / `is_err` | `is_ok` / `is_err` (F) | `err == nil` | — | — | `is_ok` / `is_error` (F) | — | `is_ok` / `is_error` (F) | `isSuccess` / `isFailure` (F) | — |
| Option に変える（ok 側） | `result.to_option` / `?` | `ok` (F) | — | — | — | — | — | `to_option` (F) | `getOrNull` (F) | `try?` |
| Option に変える（err 側） | `result.to_err_option` | `err` (F) | — | — | — | — | — | `get_error`（例外版） | `exceptionOrNull` (F) | — |
| List[Result] を ok と err に分ける | `result.partition` | `Iterator::partition` | — | — | `Promise.allSettled` | `partition` (F) | `Enum.split_with` | — | — | — |
| リストに変える | `result.to_list` | `iter` | — | — | — | — | — | `to_list` (F) | — | — |
| 2 つを組にする | `result.zip` | — | — | — | `Promise.all` | — | — | `product` (F) | — | — |
| err のとき別の Result | `result.or_else` | `or_else` (F) | — | `except` | `catch` | `try_recover` (F) | — | — | `recoverCatching` (F) | `flatMapError` |
| ok を条件で err にする | `result.filter` | — | — | — | — | — | — | — | — | — |
| 両側を 1 つの値に畳む | `match` | `map_or_else` (F) | — | — | — | — | — | `fold` (F) | `fold` (F) | — |
| 中身を取り出し、err なら止める | `?? panic(msg)` | `expect` / `unwrap` (F) | — | — | — | `let assert` | — | `get_ok`（例外版, F） | `getOrThrow` (F) | `get()` |
| err の中身を取り出す（ok なら止める） | `match` | `unwrap_err` / `expect_err` (F) | — | — | — | `unwrap_error` (F) | — | `get_error` (F) | `exceptionOrNull` | — |
| 中身をのぞく（副作用） | — | `inspect` / `inspect_err` (F) | — | — | — | — | — | `iter` / `iter_error` (F) | `onSuccess` / `onFailure` (F) | — |

### 完全性の規則

- option と result の対応は [L2-option](L2-option.md) の「完全性の規則」と同じ表で守ります（R-option-04）。result にだけあるセル: `map_err`、`to_err_option`、`partition`。result にないセル: `collect` / `collect_map`（ADR-0007 D1 で撤去済み。全エラー収集は `partition`、最初のエラーで止める形は `list.map(xs, (x) => f(x)!)!`）。
- エラーの型 E は String が既定です（ADR-0002 D2）。関数の型は E について多相のままにし、String 専用の関数（文脈を前に付ける等）は作りません（ADR-0004 D2。[L2-error](L2-error.md) を参照）。
- 既定値は `??` だけにします（R-L0-06）。

### L2 の判定

- 重複: `result.unwrap_or` は `??` と同じ役割です。#1107 の窓で撤去します（R-result-01）。
- 抜けている役割で 5/9 以上: 「err なら止めて取り出す」（5 言語）は `r ?? panic(msg)` で書けます。「両側を畳む」「err の中身を取り出す」「のぞく」は 3〜4 言語で、`match` で書けます。関数は足しません（R-result-04）。
- Almide だけの形: `result.filter` の 3 引数と `result.to_err_option` の名前。どちらも据え置きます（R-result-02、R-result-03）。
- 置き場所: `result.partition` は第 1 引数が List ですが、R-L0-02 の但し書き（入れ物の要素の型に意味がある関数は、要素の型のモジュールに置く）に当たるので、result に置くのが規則どおりです（R-result-05、R-option-02 と同じ）。

## 案

### R-result-01 `result.unwrap_or` を epoch 16 で撤去する — ○ 採用

- 内容: R-option-01 と同じです。`@deprecated(since=3, use="??")` に `[[deprecation]]` ブロック（`since_epoch = 3`、`removal_epoch = 16`）を足し、epoch 16 の `breaks` で撤去します。撤去後の E002 のヒントは「`r ?? d` と書きます。err の中身を使うときは `result.unwrap_or_else(r, (e) => ...)`」。
- 根拠の L0 規則: R-L0-06、R-L0-14
- (a)〜(c) R-option-01 と同じ。
- (d) 撤去。`almide fix` は `result.unwrap_or(r, d)` → `r ?? d` に書き換えます。使用数: spec 27 / tests 6 / docs 16 / stdlib 2（定義を含む）、research 1。
- ゲート: R-option-01 と同じ。
- issue: #1107（既存）

### R-result-02 `result.filter` の 3 引数を据え置く — × 不採用（撤去も改名もしない）

- 内容: `result.filter(r, pred, err_val)` を残します。
- 根拠の L0 規則: R-L0-00、R-L0-14 の 4（意味の付け替え禁止）
- (a) Result に filter を持つ言語は 0 です（Rust の Result にも無いことを (F) で確認）。
- (c) option から類推して `result.filter(r, pred)` と 2 引数で書くと、型エラー（引数の数）になり、黙って通ることはありません。err の値がないと err を作れないので、3 引数は意味から決まる形です。撤去すると `result.flat_map(r, (x) => if pred(x) then ok(x) else err(e))` と長くなるだけで、綴りの衝突は解けません。
- (d) なし。使用数: spec 11 / tests 2 / docs 12 / stdlib 1。
- issue: なし

### R-result-03 `result.to_err_option` を改名しない — × 不採用

- 内容: Rust の `err()` や Kotlin の `exceptionOrNull` に寄せる改名はしません。
- 根拠の L0 規則: R-L0-00、R-L0-07
- (a) 綴りは割れています（Rust `err`、Kotlin `exceptionOrNull`、OCaml は例外版の `get_error` だけ）。多数派はありません。
- (c) `to_option`（ok 側）と並ぶ `to_err_option`（err 側）は、R-L0-07 の `to_<先の型>` の形のままで、対で読めます。
- (d) なし。使用数: spec 9 / tests 4 / docs 7 / stdlib 1。
- issue: なし

### R-result-04 抜けている役割の関数を足さない — × 不採用

- 内容: `expect`、`unwrap_err`、`map_or_else` / `fold`、`inspect` / `inspect_err`、`and` / `or`（先に評価）を足しません。
- 根拠の L0 規則: R-L0-00、R-L0-01、R-L0-06
- (a) 5/9 に届くのは「err なら止めて取り出す」だけで、`r ?? panic("msg")` で書けます（probe-option/q.almd で確認）。`fold` は OCaml・Kotlin、`map_or_else` は Rust の 3 言語、`inspect` は 3 言語です。
- (c) 2 つの関数を受け取る `fold` は、引数の順（ok が先か err が先か）が言語ごとに違い、書き間違いの元になります。`match r { ok(v) => .., err(e) => .. }` は順を名前で書けます。
- (d) なし。
- issue: なし

### R-result-05 `result.partition` を result に残す — ○ 採用（据え置き）

- 内容: 置き場所を動かしません。R-L0-02 の但し書き（要素の型のモジュールに置く。L0 本文が `result.partition` を例に挙げています）に当たります。名前の許容リストには載せず、R-L0-G の置き場所の検査が要素の型を見るように書きます（R-option-02 と同じ）。
- 根拠の L0 規則: R-L0-02、R-L0-01
- (a) Gleam は `result.partition` です (F)。
- (c) `list.partition(xs, f)` は述語で分ける関数として既にあります。List[Result] 版を `list` に移すと、同じ名前で 2 つの意味になり、R-L0-01 に反します。ADR-0007 D2 が `result.partition` を全エラー収集の唯一の実体と決めています。
- (d) なし。使用数: spec 11 / tests 7 / docs 17 / stdlib 2。
- issue: #3949

## L3: 関数表

| 関数 | シグネチャ(現行, mut 表示) | 他言語の主な綴り | 判定 | 案 |
|---|---|---|---|---|
| `result.map` | `map(r: Result[A, E], f: (A) -> B) -> Result[B, E]` | Rust・Gleam・OCaml・Kotlin・Swift map | 据え置き | — |
| `result.map_err` | `map_err(r: Result[A, E], f: (E) -> F) -> Result[A, F]` | Rust map_err / Gleam・OCaml map_error / Swift mapError | 据え置き | ADR-0004 D2 の正準形 |
| `result.flat_map` | `flat_map(r: Result[A, E], f: (A) -> Result[B, E]) -> Result[B, E]` | Rust and_then / Gleam try / OCaml bind / Swift flatMap | 据え置き | R-L0-10 |
| `result.unwrap_or` | `unwrap_or(r: Result[A, E], default: A) -> A`（deprecated） | Rust unwrap_or / Gleam unwrap / Kotlin getOrDefault | 撤去 | R-result-01 |
| `result.unwrap_or_else` | `unwrap_or_else(r: Result[A, E], f: (E) -> A) -> A` | Rust unwrap_or_else / Gleam lazy_unwrap / Kotlin getOrElse | 据え置き | `??` の脱糖先（ADR-0005 D1） |
| `result.is_ok` | `is_ok(r: Result[A, E]) -> Bool` | Rust・Gleam・OCaml is_ok / Kotlin isSuccess | 据え置き | — |
| `result.is_err` | `is_err(r: Result[A, E]) -> Bool` | Rust is_err / Gleam・OCaml is_error / Kotlin isFailure | 据え置き | — |
| `result.to_option` | `to_option(r: Result[A, E]) -> Option[A]` | Rust ok / OCaml to_option / Kotlin getOrNull | 据え置き | `?` の脱糖先 |
| `result.to_err_option` | `to_err_option(r: Result[A, E]) -> Option[E]` | Rust err / Kotlin exceptionOrNull | 据え置き | R-result-03 |
| `result.partition` | `partition(rs: List[Result[T, E]]) -> (List[T], List[E])` | Gleam partition | 据え置き | R-result-05 |
| `result.flatten` | `flatten(r: Result[Result[A, E], E]) -> Result[A, E]` | Rust・Gleam flatten / OCaml join | 据え置き | R-option-04 |
| `result.to_list` | `to_list(r: Result[A, E]) -> List[A]` | OCaml to_list / Rust iter | 据え置き | R-option-04 |
| `result.zip` | `zip(a: Result[A, E], b: Result[B, E]) -> Result[(A, B), E]` | OCaml product | 据え置き | R-option-04 |
| `result.or_else` | `or_else(r: Result[A, E], f: (E) -> Result[A, F]) -> Result[A, F]` | Rust or_else / Gleam try_recover / Swift flatMapError | 据え置き | — |
| `result.filter` | `filter(r: Result[A, E], pred: (A) -> Bool, err_val: E) -> Result[A, E]` | （他言語になし） | 据え置き | R-result-02 |

（15 関数。追加なし）

R-L0-05 との関係: `result.map(r, (x) => println(x))` のように副作用のために文として書くと、R-L0-05 の後は E042 になります（いまの E042 は Result を文で捨てたときに既に止めます）。`match` で書きます。

## L0 への差し戻し（処理済み）

- R-L0-02 の例外（`result.partition`）は [L2-option](L2-option.md) の差し戻しと同じ項目です。R-L0-02 に但し書きとして入ったので、R-result-05 はそれを根拠にしています。
