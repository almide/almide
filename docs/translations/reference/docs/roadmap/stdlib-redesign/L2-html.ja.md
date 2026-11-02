<a id="l2l3-html"></a>

# L2/L3: html

[![English](https://img.shields.io/badge/English-d0d7de?style=flat-square)](L2-html.md) [![日本語](https://img.shields.io/badge/%E6%97%A5%E6%9C%AC%E8%AA%9E-24292f?style=flat-square)](L2-html.ja.md) [![简体中文](https://img.shields.io/badge/%E7%AE%80%E4%BD%93%E4%B8%AD%E6%96%87-d0d7de?style=flat-square)](L2-html.zh-CN.md)

> [正本](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/stdlib-redesign/L2-html.md)を 35297a612fb29e3f78f6587aba253aa91d51f466 に固定した非規範的な読者向け翻訳です。自動同期されません。原本が引き続き権威を持ちます。ロードマップの決定・日付・比較・実装・測定の主張は独立した実行や再検証なしに保持しており、現行リリースの挙動を主張するものではありません。原本メタデータは読者向け注記として表示し、生成器への有効な入力にはしません。

> 原本の説明（無効化したメタデータ）: 標準ライブラリ再設計 L2/L3 — html: 5 つの SafeHtml 関数を据え置く。unescape もテンプレートエンジンも設けない。

> 最終更新: 2026-10-21
> 前提: [L0](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/stdlib-redesign/L0-conventions.md)・[L1](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/stdlib-redesign/L1-categories.md)（R-L1-29）
> 出典: Rust —（標準に無い。https://doc.rust-lang.org/std/ ）・ Go https://pkg.go.dev/html ・ https://pkg.go.dev/html/template#HTML ・ Python https://docs.python.org/3/library/html.html ・ TS/JS https://jsr.io/@std/html ・ https://bun.sh/docs/api/utils#bun-escapehtml ・ Gleam —（標準に無い）・ Elixir —（標準に無い。Phoenix.HTML は外部）・ OCaml —（標準に無い）・ Kotlin —（kotlinx.html は pkg）・ Swift —（標準に無い）
> 実測: `almide 0.68.0 (dev, 437b8f127)`、native と `--target wasm`。プローブは scratchpad の `probe-url/p1.almd`。

<a id="l2-役割の比較"></a>

## L2: 役割の比較

| 役割 | Almide | Rust | Go | Python | TS/JS | Gleam | Elixir | OCaml | Kotlin | Swift |
|---|---|---|---|---|---|---|---|---|---|---|
| 特殊文字を実体参照にする | `escape` → `SafeHtml` | — | `html.EscapeString` | `html.escape` | ◐ Deno `escape`、`Bun.escapeHTML` | — | — | — | — | — |
| 実体参照を戻す | — | — | `html.UnescapeString` | `html.unescape` | ◐ Deno `unescape` | — | — | — | — | — |
| 「エスケープ済み」を型で表す | `SafeHtml`、`raw`、`to_string` | — | `template.HTML` | — | — | — | — | — | — | — |
| 連結・空 | `concat`、`empty` | — | （文字列の連結） | — | — | — | — | — | — | — |
| テンプレートエンジン | —（`"${}"` の埋め込み） | — | `html/template` | `string.Template` | — | — | EEx | — | — | — |

標準でエスケープを持つのは 2 言語＋◐1 です（[facts/categories.md](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/stdlib-redesign/facts/categories.md)）。

<a id="完全性の規則"></a>

### 完全性の規則

1. `SafeHtml` に入る道は `escape`（安全にする）と `raw`（信頼した入力をそのまま）の 2 つだけ、出る道は `to_string` の 1 つだけです。
2. `SafeHtml` どうしの組み合わせは `concat` と `empty` です。どちらも中身を二重にエスケープしません。
3. **意図した抜け**: `unescape` は持ちません（3 言語だけ。R-L1 の基準の 5 に届かない）。テンプレートエンジンは持ちません（R-L1 の 3 章「printf 系の書式化」と同じ理由で、文字列の埋め込みで書く）。

<a id="l2-の判定"></a>

### L2 の判定

- 抜けている役割（5 言語以上）・重複・族の穴はありません。
- 実測: `html.escape("<a href='x'>&\"</a>")` と `html.raw("<b>")` の `concat` は、両 target で `&lt;a href=&#39;x&#39;&gt;&amp;&quot;&lt;/a&gt;<b>` でした。
- R-L0-07: `html.to_string`（`SafeHtml` → `String`）は元の型の `to_<型>` で、規則どおりです。

<a id="案"></a>

## 案

<a id="r-html-01-5-関数を据え置く---採用変更なし"></a>

### R-html-01 5 関数を据え置く — ○ 採用（変更なし）

- 内容: `escape`、`raw`、`to_string`、`concat`、`empty` を据え置きます。
- 根拠の L0 規則: R-L1-29（動かさない）、R-L0-00。
- (b) 純粋な計算です。両 target で同じ結果です（実測）。
- (c) 文字列と `SafeHtml` を取り違えると型エラーになり、黙って二重エスケープ・エスケープ漏れになりません。
- `empty` と `new` の使い分け: 入れ物を作る関数は `new`（`map.new`、`set.new`、`deque.new`）ですが、`html.empty()` は入れ物ではなく「空の HTML 断片」という値で、`concat` の単位元です。値の名前として `empty` を残します（R-L0-01 の同義語には当たりません）。
- issue: なし

<a id="r-html-02-unescape-を足す---不採用"></a>

### R-html-02 `unescape` を足す — × 不採用

- 理由: 標準に持つのは Go・Python・Deno の 3 言語で、L1 の基準（5 言語以上）に届きません。HTML を読む処理は package（HTML の構文解析器）の関心事です。
- issue: なし

<a id="r-html-03-safehtml-の列をまとめる-htmljoin-を足す---不採用"></a>

### R-html-03 `SafeHtml` の列をまとめる `html.join` を足す — × 不採用

- 理由: エスケープの型を持つ言語が 2 つしかなく、5 言語の基準に届きません。`list.fold(xs, html.empty(), html.concat)` で書けます。
- issue: なし

<a id="r-html-04-文脈ごとの組み立て関数-attrurl_attrscript_json-を足す---採用4120"></a>

### R-html-04 文脈ごとの組み立て関数 `attr`・`url_attr`・`script_json` を足す — ○ 採用（#4120）

- 内容: `escape` は本文（テキスト文脈）専用です。属性値は `html.attr(name, value)`（二重引用符で囲んでエスケープ）、URL 属性は `html.url_attr(name, url)`（相対 URL と `http`・`https`・`mailto` だけを通し、ほかは `about:invalid#almide-blocked`）、`<script>` の中のデータは `html.script_json(v)`（`<` `>` `&` U+2028 U+2029 を JSON の `\u` エスケープにする）で作ります。`http.html(status, SafeHtml)` は `SafeHtml` を `text/html` で返します。
- 根拠: セキュリティ表の行 11e（HTML への注入）。Go の `html/template` は文脈ごとにエスケープを変え、許されない URL を `#ZgotmplZ` に置き換えます。5 言語の基準より、注入を型で防ぐ方針（R-html-01 の (c)）を優先します。
- issue: #4120

<a id="l3-関数表"></a>

## L3: 関数表

| 関数 | シグネチャ(現行, mut 表示) | 他言語の主な綴り | 判定 | 案 |
|---|---|---|---|---|
| `escape` | `html.escape(s: String) -> SafeHtml` | Go `html.EscapeString` / Python `html.escape` / Bun `escapeHTML` | 据え置き | R-html-01 |
| `raw` | `html.raw(s: String) -> SafeHtml` | Go `template.HTML(s)` | 据え置き | R-html-01 |
| `to_string` | `html.to_string(h: SafeHtml) -> String` | Go `string(h)` | 据え置き | R-html-01 |
| `concat` | `html.concat(a: SafeHtml, b: SafeHtml) -> SafeHtml` | —（多数: 文字列の連結） | 据え置き | R-html-01 |
| `empty` | `html.empty() -> SafeHtml` | — | 据え置き | R-html-01 |

## 原本に関する注意

原本は 5 関数の説明・表と入口を 2 つとする完全性規則を残す一方、R-html-04 で文脈別の構築関数も採択しています。表を黙って更新せず、両方を保持しています。実測例は一般的なセキュリティ証明ではありません。raw の入力は信頼済みであり、テキスト用エスケープを属性・URL・script 文脈へ拡張して扱うことはできません。文脈別規則は記載された範囲を保持します。
