<!-- description: Stdlib redesign L2/L3 — url: owns all percent coding (http.url_decode moves in as decode_form), parses queries decoded, and gains resolve -->
# L2/L3: url

> Last updated: 2026-10-21
> 前提: [L0](L0-conventions.md)・[L1](L1-categories.md)（R-L1-28）
> 出典: Rust —（標準に無い。https://doc.rust-lang.org/std/ ）・ Go https://pkg.go.dev/net/url (F) ・ Python https://docs.python.org/3/library/urllib.parse.html (F) ・ TS/JS https://developer.mozilla.org/en-US/docs/Web/API/URL ・ https://developer.mozilla.org/en-US/docs/Web/API/URLSearchParams ・ Gleam https://gleam-stdlib.hexdocs.pm/gleam/uri.html ・ Elixir https://elixir.hexdocs.pm/URI.html (F) ・ OCaml —（標準に無い）・ Kotlin https://docs.oracle.com/en/java/javase/21/docs/api/java.base/java/net/URI.html ・ Swift https://developer.apple.com/documentation/foundation/urlcomponents
> 実測: `almide 0.68.0 (dev, 437b8f127)`、native と `--target wasm`。プローブは scratchpad の `probe-url/p1.almd`。

## L2: 役割の比較

| 役割 | Almide | Rust | Go | Python | TS/JS | Gleam | Elixir | OCaml | Kotlin | Swift |
|---|---|---|---|---|---|---|---|---|---|---|
| 部品を % 符号化（`+` は `+` のまま） | `encode_component` / `decode_component` | — | `PathEscape` / `PathUnescape` (F) | `quote` / `unquote` (F) | `encodeURIComponent` / `decodeURIComponent` | `percent_encode` / `percent_decode` | `encode` / `decode` (F) | — | — | `addingPercentEncoding` |
| フォーム形式（空白 ⇄ `+`） | `http.url_decode`（復号だけ、http にある） | — | `QueryEscape` / `QueryUnescape`（`+` → 空白）(F) | `quote_plus` / `unquote_plus` (F) | `URLSearchParams` | — | `encode_www_form` / `decode_www_form`（`+` → 空白）(F) | — | `URLEncoder` / `URLDecoder`(JVM) | — |
| URL の分解・組み立て | `parse` / `to_string`（`Url` 型） | — | `Parse` / `String` | `urlparse` / `urlunparse` (F) | `new URL` / `href` | `parse` / `to_string` | `parse` / `to_string` (F) | — | `URI` | `URLComponents` |
| クエリを組の列に（復号つき） | `query_pairs`（**復号しない**） | — | `ParseQuery` (F) | `parse_qsl`（復号する）(F) | `URLSearchParams` | `parse_query` | `decode_query` (F) | — | — | `queryItems` |
| 組の列からクエリを作る | `build_query` | — | `Values.Encode` | `urlencode` (F) | `URLSearchParams.toString` | `query_to_string` | `encode_query` (F) | — | — | `queryItems` |
| 相対 URL を基準で解決 | **—** | — | `ResolveReference` (F) | `urljoin` (F) | `new URL(ref, base)` | `merge` | `merge` (F) | — | `URI.resolve`(JVM) | `URL(string:relativeTo:)` |

相対 URL の解決は 6 言語（Go・Python・JS・Gleam・Elixir・Swift。Kotlin は JVM の ◐）が持っています。

### 完全性の規則

1. % 符号化は `url` だけが持ちます（R-L1-28、R-L0-01）。
2. 符号化には 2 つの方式があり、それぞれ `encode_<方式>` / `decode_<方式>` の対を持ちます: `component`（`+` を文字として扱う。RFC 3986）と `form`（空白を `+` にする。application/x-www-form-urlencoded）。
3. 復号は `Result[String, String]` を返します。不正なエスケープと不正な UTF-8 は err です。番兵（入力をそのまま返す、U+FFFD に置き換える）は使いません（R-L0-06 の 4）。
4. 組の対: `parse` ⇄ `to_string`（URL）、`parse_query` ⇄ `build_query`（クエリ）。`parse_query(build_query(ps)) == ok(ps)` が成り立ちます。
5. **意図した抜け**: クエリの組を Map で返す版は持ちません（同じキーが複数あり得るため。`map.from_list` で書けます）。

### L2 の判定

- 重複と見えたもの: `http.url_decode` と `url.decode_component` は**同じではありません**（実測）。

  | 入力 | `url.decode_component` | `http.url_decode` |
  |---|---|---|
  | `a+b%20c` | `ok("a+b c")` | `"a b c"`（`+` を空白に） |
  | `%zz` | err（invalid percent escape） | `"%zz"`（そのまま返す） |
  | `%ff` | err（invalid UTF-8） | `"�"`（置換文字） |
  | `50%` | err（truncated） | `"50%"` |

  `http.url_decode` はフォーム形式の復号で、失敗を番兵（入力そのまま・置換文字）で隠します。R-L1-28 の「撤去」は保ちつつ、置き換え先はフォーム形式の新しい関数にします → R-url-01。
- 意味の食い違い: `query_pairs` は復号しません（実測: `"c=x%20y&d=p+q"` → `("c", "x%20y")`、`("d", "p+q")`）。一方 `build_query` は符号化します。対になっていません。他の 5 言語の同じ役割はすべて復号します → R-url-02。
- 抜けている役割: 相対 URL の解決 → R-url-03。
- `Url` の欄: `port` だけが Option で、他の欄は無いとき `""` です。`parse` が成功したあとの表現なので、R-L0-06 の 4（失敗したときに番兵を返さない）には当たりません。据え置きます。

## 案

### R-url-01 フォーム形式の対 `encode_form` / `decode_form` を足し、`http.url_decode` を撤去する — ○ 採用

- 内容:
  - 追加: `url.encode_form(s: String) -> String`（空白は `+`、それ以外は `encode_component` と同じ）、`url.decode_form(s: String) -> Result[String, String]`（`+` を空白にしてから `decode_component` と同じ規則で復号）。
  - 撤去: `http.url_decode(s) -> String`。**R-L1-28 の判定をここで確定します: 撤去する。置き換え先は `url.decode_component` ではなく `url.decode_form`**（`+` の扱いが同じなのはこちら）。
- 根拠の L0 規則: R-L1-28、R-L0-01、R-L0-02（文字列の % 符号化は url の役割）、R-L0-06 の 4（番兵を返さない）、R-L0-14。
- (a) 2 つの方式を別の名前で持つのは Go（`PathUnescape`/`QueryUnescape`）(F)、Python（`unquote`/`unquote_plus`）(F)、Elixir（`decode`/`decode_www_form`）(F)、JS（`decodeURIComponent`/`URLSearchParams`）、Kotlin（JVM の `URLDecoder`）です。
- (b) 純粋な計算です。いまの `url.decode_component` と同じ self-host の実装に乗せるので、両 target で同じ答えです。
- (c) `+` を空白にするかどうかは名前からは分かりません。方式名を名前に入れて、どちらを呼んでいるかを読めるようにします。
- (d) 追加 2（http 側は撤去 1）。`http.url_decode` に `@deprecated(since=15, use="url.decode_form")` を付け、`removal_epoch = 16` で撤去します。`almide fix` は `http.url_decode(s)` → `url.decode_form(s) ?? s` に書き換えます（不正な入力のとき元の文字列になる形）。違うのは `%ff` のような不正な UTF-8 だけで、いまの置換文字ではなく元の文字列になります。この違いはヒントの文に書きます。使用数（`http.url_decode`）: spec 9 / tests 0 / docs 9 / stdlib 2。stdlib の 2 件（`http.almd:366` の `__decode_part` と `http_url_decode.almd`）は同じ変更で書き換えます。
- ゲート案: `spec/stdlib/url_test.almd` に、上の 4 入力の表を 2 方式 × 2 target で置きます。`decode_form(encode_form(s)) == ok(s)` の往復を性質テストにします。
- issue: #3950

### R-url-02 クエリの読みを、復号する `parse_query` にする — ○ 採用

- 内容: `url.parse_query(query: String) -> Result[List[(String, String)], String]` を追加します。キーと値を `decode_form` の規則で復号します。値のないキーは `""` です（いまの `query_pairs` と同じ）。`query_pairs`（復号しない）を撤去します。
- 根拠の L0 規則: R-L0-00 の条件 1（`query_pairs` と `build_query` が対になっていない）、R-L0-14 の 4。R-L0-14 の 4 は、旧い書き方が型エラーになる変更なら同じ名前のままでも許します。それでも `query_pairs` の名前は残しません。戻り型の型エラーを `?? []` で直しただけのコードでは、その後ろで値を `decode_component` にかけている呼び出しが型を通ったまま残り、`%2520` が空白になるような二重の復号を黙って起こすからです。名前を移せば、ヒントを読んで書き直すことになります。名前は Go の `ParseQuery`(F) と Gleam の `parse_query` に合わせ、`parse` ⇄ `to_string` と同じ「`parse_<対象>`」の形にします。
- (a) 同じ役割の 5 言語（Go・Python・JS・Gleam・Elixir）はすべて復号します。
- (b) 純粋な計算です。
- (c) いまは `query_pairs` の結果を `decode_component` にかけ忘れると、`%20` の入った値が黙って残ります。
- (d) 撤去 1・追加 1。`@deprecated(since=15, use="url.parse_query")`、`removal_epoch = 16`。戻り型が `Result` になり、値が復号されるので、`almide fix` では書き換えません。ヒントに「`url.parse_query(q) ?? []` は復号した組を返す。復号しない値を前提にしていた `decode_component` の呼び出しは外す」と出します。使用数（`query_pairs`）: spec 3 / tests 0 / docs 4 / stdlib 5（定義とコメント）。
- ゲート案: `parse_query(build_query(ps)) == ok(ps)` の性質テスト。
- issue: #3951

### R-url-03 相対 URL の解決 `resolve` を足す — ○ 採用

- 内容: `url.resolve(base: Url, ref: String) -> Result[Url, String]`。RFC 3986 の 5 章（参照の解決）どおりに動きます。`ref` が絶対 URL なら、それをそのまま parse した結果です。
- 根拠の L0 規則: L2 の基準（5 言語以上が持ち、Almide に無い役割）、R-L0-02（主語は `Url`）、R-L0-03（主語を先頭に）。
- (a) 6 言語が持っています。綴りは `ResolveReference`(F)、`urljoin`(F)、`merge`(F)、`resolve` に割れていて多数派はありません。JVM の `URI.resolve` と同じ動詞を選びます（「解決する」という RFC 3986 の用語と同じ）。
- (b) 純粋な計算です。RFC 3986 5.4 の例（5.4.1 通常・5.4.2 異常）を native と wasm の fixture にします。
- (c) 相対リンクを手で連結すると `../` と `?` の扱いを誤ります。
- (d) 追加 1。
- ゲート案: RFC 3986 5.4 の例の表をそのまま test にします。
- issue: #3952

### R-url-04 `Url` の欄を Option にする — × 不採用

- 理由: `query` と `fragment` の「無い」と「空」を区別したい場面は少なく、欄の型を変えると、すべての `u.query` の読みが壊れます（R-L0-14 の 4）。`port` はすでに Option です。
- issue: なし

### R-url-05 `Url` に userinfo を足す — ○ 採用

- いま: `type Url = { scheme, host, port: Int?, path, query, fragment }`。`url.parse("https://user:pw@example.com/x")` は `err("userinfo is not supported")` です。
- 案: 欄 `userinfo: String`（無いときは `""`。`query`・`fragment` と同じ扱い）を足し、`parse`・`to_string`・`resolve` が扱うようにします。
- (a) Go `URL.User`、Python `urlparse().username`、JS `URL.username`、Elixir `URI.userinfo`、Gleam `Uri.userinfo`、JVM `URI.getUserInfo` の 6 言語が読めます。
- (c) `DATABASE_URL`（`postgres://user:pass@host/db`）を読む課題はよく出ます。いまは実行して初めて err になります。
- (d) 型の変更。`Url { ... }` のレコードの組み立ては欄の不足で型エラーになるので、R-L0-14.4 を満たします（ヒント: `userinfo: ""` を足す）。読むだけの呼び出しは変わりません。
- ゲート案: RFC 3986 の authority の例を両 target の fixture にし、`parse(to_string(u)) == ok(u)` の性質テストに userinfo を入れます。
- issue: #3971

## L3: 関数表

| 関数 | シグネチャ(現行, mut 表示) | 他言語の主な綴り | 判定 | 案 |
|---|---|---|---|---|
| `encode_component` | `url.encode_component(s: String) -> String` | JS `encodeURIComponent` / Go `PathEscape` (F) / Python `quote` (F) / Elixir `encode` (F) / Gleam `percent_encode` | 据え置き | R-url-01 |
| `decode_component` | `url.decode_component(s: String) -> Result[String, String]` | JS `decodeURIComponent` / Go `PathUnescape` (F) / Python `unquote` (F) / Elixir `decode` (F) | 据え置き | R-url-01 |
| `parse` | `url.parse(s: String) -> Result[Url, String]` | Go `url.Parse` / Python `urlparse` (F) / JS `new URL` / Elixir `URI.parse` (F) / Gleam `uri.parse` | 意味変更（userinfo を受け取り、`Url` に `userinfo` の欄を足す） | R-url-05 |
| `to_string` | `url.to_string(u: Url) -> String` | Go `String` / Elixir `to_string` (F) / Gleam `to_string` / JS `href` | 据え置き | — |
| `query_pairs` | `url.query_pairs(query: String) -> List[(String, String)]` | Python `parse_qsl`（復号する）(F) / Go `ParseQuery` (F) / Elixir `decode_query` (F) | 撤去 → `parse_query` | R-url-02 |
| `build_query` | `url.build_query(pairs: List[(String, String)]) -> String` | Python `urlencode` (F) / Elixir `encode_query` (F) / Go `Values.Encode` / Gleam `query_to_string` | 据え置き | R-url-02 |
| `encode_form` | `url.encode_form(s: String) -> String`（案） | Go `QueryEscape` (F) / Python `quote_plus` (F) / Elixir `encode_www_form` (F) | 追加 | R-url-01 |
| `decode_form` | `url.decode_form(s: String) -> Result[String, String]`（案） | Go `QueryUnescape` (F) / Python `unquote_plus` (F) / Elixir `decode_www_form` (F) | 追加（`http.url_decode` の置き換え先） | R-url-01 |
| `parse_query` | `url.parse_query(query: String) -> Result[List[(String, String)], String]`（案） | Go `ParseQuery` (F) / Gleam `parse_query` / Python `parse_qsl` (F) | 追加 | R-url-02 |
| `resolve` | `url.resolve(base: Url, ref: String) -> Result[Url, String]`（案） | Go `ResolveReference` (F) / Python `urljoin` (F) / JS `new URL(ref, base)` / Elixir `merge` (F) / JVM `URI.resolve` | 追加 | R-url-03 |

（参考: 他モジュールの行）

| 関数 | シグネチャ(現行, mut 表示) | 他言語の主な綴り | 判定 | 案 |
|---|---|---|---|---|
| `http.url_decode` | `http.url_decode(s: String) -> String` | Go `QueryUnescape` (F) / Python `unquote_plus` (F) | 撤去 → `url.decode_form(s) ?? s` | R-url-01 |
