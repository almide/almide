# html

[![English](https://img.shields.io/badge/English-d0d7de?style=flat-square)](html.md) [![日本語](https://img.shields.io/badge/%E6%97%A5%E6%9C%AC%E8%AA%9E-24292f?style=flat-square)](html.ja.md) [![简体中文](https://img.shields.io/badge/%E7%AE%80%E4%BD%93%E4%B8%AD%E6%96%87-d0d7de?style=flat-square)](html.zh-CN.md)



> [固定原文](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/stdlib/html.md) の非規範的な読者向け訳です。自動生成・自動同期されません。署名・コードとそのコメントは原文通りで、API 説明の訳を外側に併記します。この翻訳では例や実装を独立に実行検証していません。

> 安全性の説明は使用文脈の制約とセットです。`raw` や誤った文脈への挿入を含め、全経路で XSS が起こらないという保証ではありません。


HTML のエスケープと `SafeHtml` capability 型。`import html`。

このモジュールの要点は、`SafeHtml` と `String` が異なる型であることです。`SafeHtml` の値を連結するテンプレートには、型システムが許さないため、エスケープしていない文字列を誤って混ぜ込めません。文脈に応じたビルダー（`escape`、`attr`、`url_attr`、`script_json`）で断片を構築し、`concat` / `empty` で結合するか、`raw` でマークアップを明示的に信頼します。断片は、それでも想定された文脈で使う必要があります。

両ターゲットで pure 関数を公開します。ターゲット間の挙動は HTML 仕様フィクスチャで検査されますが、pure であることだけではバックエンドの同値性の証明になりません。

### `html.escape(s: String) -> SafeHtml`

XML/HTML の五つのメタ文字 — `&`、`<`、`>`、`"`、`'` — をエスケープして結果を包みます。ユーザー由来の入力に使う通常の方法ですが、**テキスト文脈（要素内容）専用**です。引用符で囲まず、`javascript:` URL は保持し、`<script>` 内では安全ではありません。属性には `attr`、URL 属性には `url_attr`、スクリプト用データには `script_json` を使います。

```almd check
import html

fn main() -> Unit = {
  println(html.to_string(html.escape("<script>")))
}
```

### `html.raw(s: String) -> SafeHtml`

エスケープせずに文字列を包みます。プログラム自身が生成したマークアップ用です。呼び出し箇所はすべてレビュー対象とし、呼び出し元が提供したテキストを決して渡さないでください。

```almd check
import html

fn main() -> Unit = {
  let br = html.raw("<br>")
  println(html.to_string(br))
}
```

### `html.to_string(h: SafeHtml) -> String`

包みを外して内部の文字列を取り出し、レスポンス本文に書き込めるようにします。

### `html.concat(a: SafeHtml, b: SafeHtml) -> SafeHtml`

二つの断片を結合します。両オペランドがすでに安全なので、結果も安全です。

```almd check
import html

fn main() -> Unit = {
  let name = "Tom & <Jerry>"
  let row = html.concat(html.raw("<li>"), html.concat(html.escape(name), html.raw("</li>")))
  println(html.to_string(row))
}
```

### `html.empty() -> SafeHtml`

空の断片です。`concat` の単位元であり、行のリストを fold する際の自然な初期値です。

```almd check
import html

fn main() -> Unit = {
  let items = ["a<b", "c&d"]
  let page = items
    |> list.map((i) => html.escape(i))
    |> list.fold(html.empty(), (acc, frag) => html.concat(acc, frag))
  println(html.to_string(page))
  println("[${html.to_string(html.empty())}]")
}
```

### `html.attr(name: String, value: String) -> SafeHtml`

`name="value"` の value を二重引用符付き属性用にエスケープし、属性やタグを途中で閉じられないようにします。`name` が通常の属性名でない場合、URL を取る属性（`href`、`src`、`action` など — `url_attr` を使用）やスクリプト・スタイルを実行する属性（`on…`、`style`、`srcdoc` — これらを出力するビルダーはない）の場合は中止します。

```almd check
import html

fn main() -> Unit = {
  let cls = "x\" onmouseover=\"alert(1)"
  println("<div ${html.to_string(html.attr("class", cls))}>")
}
```

### `html.url_attr(name: String, url: String) -> SafeHtml`

エスケープした `name="url"` を生成します。URL は相対形式、またはスキームが `http`、`https`、`mailto`（大文字小文字を問わない）の場合だけ保持します。その他のスキーム — `javascript:`、`data:`、制御文字や空白を含むスキーム — は、無害な `about:invalid#almide-blocked` に置き換えます。

```almd check
import html

fn main() -> Unit = {
  println(html.to_string(html.url_attr("href", "https://example.com/?a=1&b=2")))
  println(html.to_string(html.url_attr("href", "javascript:alert(1)")))
}
```

### `html.script_json(v: Value) -> SafeHtml`

`json.stringify(v)` の `<`、`>`、`&`、U+2028、U+2029 を JSON の `\u` エスケープで書きます。これによりテキストは `<script>` 要素を閉じられず、再パースすると同じ値になります。

```almd check
import html
import json

fn main() -> Unit = {
  let v = value.object([("x", value.string("</script>"))])
  println("<script>const d = ${html.to_string(html.script_json(v))}</script>")
}
```



## シグネチャ索引 (8 個の関数)

```
// s with & < > " ' turned into HTML entities; text context only.
// @since 0.18.1 or earlier
html.escape(s: String) -> SafeHtml

// s as SafeHtml, unescaped; trusted input only.
// @since 0.18.1 or earlier
html.raw(s: String) -> SafeHtml

// HTML text inside h, as-is.
// @since 0.18.1 or earlier
html.to_string(h: SafeHtml) -> String

// a then b as one SafeHtml; nothing re-escaped.
// @since 0.18.1 or earlier
html.concat(a: SafeHtml, b: SafeHtml) -> SafeHtml

// SafeHtml of the empty string.
// @since 0.18.1 or earlier
html.empty() -> SafeHtml

// name="value" with value escaped for a double-quoted attribute; aborts on a URL, event-handler or malformed name.
// @since unreleased
html.attr(name: String, value: String) -> SafeHtml

// name="url" escaped, url kept only if relative or http, https or mailto; else about:invalid#almide-blocked.
// @since unreleased
html.url_attr(name: String, url: String) -> SafeHtml

// json.stringify(v) with < > & U+2028 U+2029 as \u escapes; safe inside <script>.
// @since unreleased
html.script_json(v: Value) -> SafeHtml
```



## API 説明の参考訳

- `html.escape(s: String) -> SafeHtml` — s の & < > " ' を HTML 実体参照へ変換。テキスト文脈専用。
- `html.raw(s: String) -> SafeHtml` — エスケープしない s を SafeHtml とする。信頼済み入力専用。
- `html.to_string(h: SafeHtml) -> String` — h 内の HTML テキストをそのまま返す。
- `html.concat(a: SafeHtml, b: SafeHtml) -> SafeHtml` — a の次に b を一つの SafeHtml とする。再エスケープしない。
- `html.empty() -> SafeHtml` — 空文字列の SafeHtml。
- `html.attr(name: String, value: String) -> SafeHtml` — value を二重引用符付き属性用にエスケープした name="value"。URL、イベントハンドラー、不正な名前では中止。
- `html.url_attr(name: String, url: String) -> SafeHtml` — name="url" をエスケープ。url は相対形式または http/https/mailto のみ保持し、それ以外は about:invalid#almide-blocked。
- `html.script_json(v: Value) -> SafeHtml` — json.stringify(v) の < > & U+2028 U+2029 を \u エスケープにし、`<script>` 内で使える形にする。
