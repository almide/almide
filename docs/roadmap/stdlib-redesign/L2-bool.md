<!-- description: Stdlib redesign L2/L3 — bool: a new module holding bool.parse, bool.to_string and bool.compare (Bool is ordered, as in seven of the nine languages) -->
# L2/L3: bool

> Last updated: 2026-10-22
> 前提: [L0](L0-conventions.md)・[L1](L1-categories.md)（R-L1-30）
> 出典: Rust https://doc.rust-lang.org/std/primitive.bool.html ・ Go https://pkg.go.dev/strconv#ParseBool ・ Python —（`bool("false")` は True） ・ TS/JS —（`JSON.parse`） ・ Gleam —（標準に無い） ・ Elixir —（`String.to_existing_atom`） ・ OCaml https://ocaml.org/manual/latest/api/Stdlib.html#VALbool_of_string_opt ・ Kotlin https://kotlinlang.org/api/core/kotlin-stdlib/kotlin.text/to-boolean-strict-or-null.html ・ Swift https://developer.apple.com/documentation/swift/bool/init(_:)-4dsgl

## L2: 役割の比較

| 役割 | Almide | Rust | Go | Python | TS/JS | Gleam | Elixir | OCaml | Kotlin | Swift |
|---|---|---|---|---|---|---|---|---|---|---|
| 文字列から読む（不正なら失敗） | `bool.parse` | `parse::<bool>` | `ParseBool` | — | — | — | — | `bool_of_string_opt` | `toBooleanStrictOrNull` | `Bool(s)` |
| 比較する（3 値） | `bool.compare`（R-bool-02） | `cmp`（`Ord`） | — | `<`（`False < True`） | — | `bool.compare` | `<`（項の順序） | `Bool.compare` | `compareTo` | —（`Comparable` に準拠しない） |
| 文字列にする | `bool.to_string`（`Repr`。R-bool-03） | `to_string` | `FormatBool` | `str` | `String` | `bool.to_string` | `to_string` | `string_of_bool` | `toString` | `description` |

### 完全性の規則

1. `parse` の族: 文字列から読める基本の型（`Int`・`Float`・固定幅の数・`Bool`）は、それぞれ `<mod>.parse(s) -> Result[T, String]` を持ちます。
2. `compare` の族（R-L0-16）: `Bool` は順序のある型（`Ord`）で、`false < true` です。`bool.compare(a, b) -> Int` を置きます（R-bool-02）。
3. 意図した抜け: `not`・`and` などの関数は置きません（演算子 `not`・`and`・`or` で書く）。`to_int` も置きません（`if b then 1 else 0` で書ける。専用の関数を持つのは Gleam と OCaml だけ）。

## 案

### R-bool-01 `bool.parse` を足す — ○ 採用

- 内容: `fn bool.parse(s: String) -> Result[Bool, String]`。`"true"` → `ok(true)`、`"false"` → `ok(false)`、それ以外は err。R-L1-30 のとおりです。
- (b) 純粋な計算です。両 target で同じです。
- (d) 新しいモジュールと追加 1。`bool` は `int` と同じく import なしで使える一覧（`AUTO_IMPORT_BUNDLED`）に入れます。
- issue: #3980

### R-bool-02 `bool.compare` を足す — ○ 採用（十一巡目に保留から戻す。9 言語中 7 つが順序をつける）

- 内容: `fn bool.compare(a: Bool, b: Bool) -> Int`。`false < true` の順で、小さければ負、等しければ 0、大きければ正を返します。
- (a) Rust（`Ord`）・Gleam・OCaml・Kotlin が持っています。
- (b) 純粋な計算です。両 target で同じです。いまの `<` と `list.sort` の順序（`false` が先）と同じです。
- (c) R-L0-16 は「順序のある型には `compare` を置く」と決めていますが、対象の一覧から `Bool` が抜けていました。R-L1-30 で `bool` モジュールができたので、ここで埋めます。
- 実装: 別の関数を書きません。`Bool` の `Ord` の `compare` そのものです（R-L0-22 の完全性の規則）。`a.compare(b)` と `bool.compare(a, b)` は同じ関数です。
- (d) 追加 1。
- 判定の経緯: 九巡目に、protocol の階層を Swift に合わせて組むとき、Swift の `Bool` が `Comparable` でないことから保留にしました。十一巡目に 9 言語を並べ直すと、順序をつけるのは Rust（`Ord`）・Python（`False < True`）・TS/JS（数に直して比べる）・Gleam（`bool.compare`）・Elixir（項の順序）・OCaml（`Bool.compare`）・Kotlin（`Comparable`）の 7 言語、つけないのは Go（`<` はコンパイルエラー）と Swift の 2 言語でした。個々の型の振る舞いは多数派で決めます（R-L0-00）。階層の骨組みは Swift を手本にしますが、どの型をどの protocol に入れるかは型ごとに多数派で決めます。
- (c) Almide はいま `false < true` が通り、`list.sort` で `List[Bool]` を並べられます。`Ord` から外すと今通るコードが壊れます。
- issue: #3980

### R-bool-03 `bool.to_string` を置く — ○ 採用

- 内容: `fn bool.to_string(b: Bool) -> String`。`"true"`・`"false"` を返し、`bool.parse` と往復します。R-L0-38 の `Repr` の関数で、中身は `"${b}"` と同じです。
- (a) 9 言語すべてが関数の形を持ちます（Rust `to_string`、Go `FormatBool`、Python `str`、TS/JS `String`、Gleam `bool.to_string`、Elixir `to_string`、OCaml `string_of_bool`、Kotlin `toString`、Swift `description`）。Swift は `Bool(s)` と `description` を `LosslessStringConvertible` で対にしています。
- 一巡目で「`Repr` があるので置かない」としたのを改めます（R-L0-38 で `Repr` の関数そのものになり、別の実装はありません）。
- (d) 追加 1。
- issue: #3980

## L3: 関数表

| 関数 | シグネチャ(現行, mut 表示) | 他言語の主な綴り | 判定 | 案 |
|---|---|---|---|---|
| `parse` | （追加）`bool.parse(s: String) -> Result[Bool, String]` | Rust `parse::<bool>` / Go `ParseBool` / Kotlin `toBooleanStrictOrNull` | 追加 | R-bool-01 |
| `compare` | （追加）`bool.compare(a: Bool, b: Bool) -> Int` | Rust `cmp` / Gleam `bool.compare` / OCaml `Bool.compare` / Kotlin `compareTo` | 追加 | R-bool-02 |
| `to_string` | （追加）`bool.to_string(b: Bool) -> String` | Rust `to_string` / Go `FormatBool` / Swift `description` | 追加 | R-bool-03 |
