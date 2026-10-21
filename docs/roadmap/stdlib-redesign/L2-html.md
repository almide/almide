<!-- description: Stdlib redesign L2/L3 — html: the five SafeHtml functions stay; no unescape, no template engine -->
# L2/L3: html

> Last updated: 2026-10-21
> 前提: [L0](L0-conventions.md)・[L1](L1-categories.md)（R-L1-29）
> 出典: Rust —（標準に無い。https://doc.rust-lang.org/std/ ）・ Go https://pkg.go.dev/html ・ https://pkg.go.dev/html/template#HTML ・ Python https://docs.python.org/3/library/html.html ・ TS/JS https://jsr.io/@std/html ・ https://bun.sh/docs/api/utils#bun-escapehtml ・ Gleam —（標準に無い）・ Elixir —（標準に無い。Phoenix.HTML は外部）・ OCaml —（標準に無い）・ Kotlin —（kotlinx.html は pkg）・ Swift —（標準に無い）
> 実測: `almide 0.68.0 (dev, 437b8f127)`、native と `--target wasm`。プローブは scratchpad の `probe-url/p1.almd`。

## L2: 役割の比較

| 役割 | Almide | Rust | Go | Python | TS/JS | Gleam | Elixir | OCaml | Kotlin | Swift |
|---|---|---|---|---|---|---|---|---|---|---|
| 特殊文字を実体参照にする | `escape` → `SafeHtml` | — | `html.EscapeString` | `html.escape` | ◐ Deno `escape`、`Bun.escapeHTML` | — | — | — | — | — |
| 実体参照を戻す | — | — | `html.UnescapeString` | `html.unescape` | ◐ Deno `unescape` | — | — | — | — | — |
| 「エスケープ済み」を型で表す | `SafeHtml`、`raw`、`to_string` | — | `template.HTML` | — | — | — | — | — | — | — |
| 連結・空 | `concat`、`empty` | — | （文字列の連結） | — | — | — | — | — | — | — |
| テンプレートエンジン | —（`"${}"` の埋め込み） | — | `html/template` | `string.Template` | — | — | EEx | — | — | — |

標準でエスケープを持つのは 2 言語＋◐1 です（[facts/categories.md](facts/categories.md)）。

### 完全性の規則

1. `SafeHtml` に入る道は `escape`（安全にする）と `raw`（信頼した入力をそのまま）の 2 つだけ、出る道は `to_string` の 1 つだけです。
2. `SafeHtml` どうしの組み合わせは `concat` と `empty` です。どちらも中身を二重にエスケープしません。
3. **意図した抜け**: `unescape` は持ちません（3 言語だけ。R-L1 の基準の 5 に届かない）。テンプレートエンジンは持ちません（R-L1 の 3 章「printf 系の書式化」と同じ理由で、文字列の埋め込みで書く）。

### L2 の判定

- 抜けている役割（5 言語以上）・重複・族の穴はありません。
- 実測: `html.escape("<a href='x'>&\"</a>")` と `html.raw("<b>")` の `concat` は、両 target で `&lt;a href=&#39;x&#39;&gt;&amp;&quot;&lt;/a&gt;<b>` でした。
- R-L0-07: `html.to_string`（`SafeHtml` → `String`）は元の型の `to_<型>` で、規則どおりです。

## 案

### R-html-01 5 関数を据え置く — ○ 採用（変更なし）

- 内容: `escape`、`raw`、`to_string`、`concat`、`empty` を据え置きます。
- 根拠の L0 規則: R-L1-29（動かさない）、R-L0-00。
- (b) 純粋な計算です。両 target で同じ結果です（実測）。
- (c) 文字列と `SafeHtml` を取り違えると型エラーになり、黙って二重エスケープ・エスケープ漏れになりません。
- `empty` と `new` の使い分け: 入れ物を作る関数は `new`（`map.new`、`set.new`、`deque.new`）ですが、`html.empty()` は入れ物ではなく「空の HTML 断片」という値で、`concat` の単位元です。値の名前として `empty` を残します（R-L0-01 の同義語には当たりません）。
- issue: なし

### R-html-02 `unescape` を足す — × 不採用

- 理由: 標準に持つのは Go・Python・Deno の 3 言語で、L1 の基準（5 言語以上）に届きません。HTML を読む処理は package（HTML の構文解析器）の関心事です。
- issue: なし

### R-html-03 `SafeHtml` の列をまとめる `html.join` を足す — × 不採用

- 理由: エスケープの型を持つ言語が 2 つしかなく、5 言語の基準に届きません。`list.fold(xs, html.empty(), html.concat)` で書けます。
- issue: なし

## L3: 関数表

| 関数 | シグネチャ(現行, mut 表示) | 他言語の主な綴り | 判定 | 案 |
|---|---|---|---|---|
| `escape` | `html.escape(s: String) -> SafeHtml` | Go `html.EscapeString` / Python `html.escape` / Bun `escapeHTML` | 据え置き | R-html-01 |
| `raw` | `html.raw(s: String) -> SafeHtml` | Go `template.HTML(s)` | 据え置き | R-html-01 |
| `to_string` | `html.to_string(h: SafeHtml) -> String` | Go `string(h)` | 据え置き | R-html-01 |
| `concat` | `html.concat(a: SafeHtml, b: SafeHtml) -> SafeHtml` | —（多数: 文字列の連結） | 据え置き | R-html-01 |
| `empty` | `html.empty() -> SafeHtml` | — | 据え置き | R-html-01 |
