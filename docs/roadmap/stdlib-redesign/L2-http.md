<!-- description: Stdlib redesign L2/L3 — http: the client becomes a complete 2x2 (GET shortcut / any method x body / whole response) plus the call handle, every other shape is an accessor (#3854), status becomes set_status, url_decode leaves for url, and the #2827 defaults are decided -->
# L2/L3: http

> Last updated: 2026-10-21
> 前提: [L0](L0-conventions.md)・[L1](L1-categories.md)（R-L1-28: URL の符号化は `url`）
> 出典: Rust https://docs.rs/reqwest （標準に HTTP は無い。参考）・ Go https://pkg.go.dev/net/http ・ Python https://docs.python.org/3/library/urllib.request.html ・ TS/JS https://developer.mozilla.org/en-US/docs/Web/API/Fetch_API ・ Gleam https://hexdocs.pm/gleam_httpc/ （公式 org の pkg）・ Elixir https://www.erlang.org/doc/apps/inets/httpc.html ・ OCaml —（標準に無い）・ Kotlin https://ktor.io/docs/client-requests.html （公式 pkg）・ Swift https://developer.apple.com/documentation/foundation/urlsession
> 既存の提案: #3854 §2（client の 23 関数）、#2827（既定の振る舞い）、#2828（追跡）。ADR-0020（serve）、ADR-0023（wasm の HTTP は `wasi:http@0.3`）。

## L2: 役割の比較

| 役割 | Almide | Rust(reqwest) | Go | Python | TS/JS | Gleam(httpc) | Elixir(httpc) | OCaml | Kotlin(Ktor) | Swift |
|---|---|---|---|---|---|---|---|---|---|---|
| GET の近道（本文） | `get` | `reqwest::get`+`text` | `http.Get` | `urlopen(u).read()` | `fetch(u).then(r => r.text())` | `httpc.send` | `:httpc.request(url)` | — | `client.get(u).bodyAsText()` | `URLSession.data(from:)` |
| 動詞ごとの近道 | `post`・`put`・`patch`・`delete` | `Client::post` ほか | `http.Post`・`Head` | `Request(method=)` | `fetch(u, {method})` | `request.set_method` | `request(:post, …)` | — | `client.post` ほか | `httpMethod =` |
| 任意のメソッドで交換 | `request`（本文）・`request_response`（全部）・`request_status`・`request_bytes` | `Client::request` | `Client.Do` | `urlopen(Request)` | `fetch(Request)` | `httpc.send` | `:httpc.request/4` | — | `client.request` | `data(for:)` |
| 応答の状態・ヘッダ・本文 | `status_code`・`get_header`・`header_values`・`headers`・`body` | `status`・`headers`・`text`・`bytes` | `StatusCode`・`Header.Get`・`Body` | `status`・`getheader`・`read` | `status`・`headers.get`・`text`・`arrayBuffer` | `status`・`get_header`・`body` | タプル | — | `status`・`headers`・`bodyAsText` | `statusCode`・`value(forHTTPHeaderField:)` |
| 本文をバイトで | `get_bytes`・`request_bytes` | `bytes` | `io.ReadAll(Body)` | `read()` | `arrayBuffer` | `BitArray` 版 | `body_format: :binary` | — | `bodyAsBytes` | `Data` |
| 流して受け取る | `request_stream`・`request_stream_with_limits` | `bytes_stream` | `Body.Read` | `read(n)` | `body.getReader()` | — | `stream:` | — | `bodyAsChannel` | `bytes(for:)` |
| 待たずに様子を見るハンドル | `start`・`poll`・`read_new`・`wait`・`cancel` | `Future` | goroutine | — | `Promise`・`AbortController` | — | `sync: false` | — | coroutine | `Task` |
| サーバ | `serve`・`route`・`router`・`mount`・`wrap`・`param` | — | `ListenAndServe`・`ServeMux` | `http.server` | `Deno.serve` ほか | — | `:httpd` | — | Ktor server | — |
| 応答を作る | `response`・`json`・`with_headers`・`redirect`・`status`（設定）・`set_header` | — | `WriteHeader`・`Header().Set` | `send_response` | `new Response(body, {status})` | `response.new` | — | — | `call.respond` | — |
| 要求を読む | `req_method`・`req_path`・`req_body`・`req_header`・`query_params`・`decode_json` | — | `r.Method`・`r.URL`・`r.Header.Get`・`r.Header.Values` | `self.path`・`headers` | `req.method`・`req.headers` | — | — | — | `call.request` | — |
| URL の復号 | `url_decode` | `url` crate | `url.QueryUnescape` | `unquote_plus` | `decodeURIComponent` | `uri.percent_decode` | `URI.decode` | — | — | — |

### 完全性の規則

- **client は 2×2 の行列 ＋ ハンドル。** 縦の軸は {GET の近道, 任意のメソッド}、横の軸は {本文の文字列, 応答の全部 `HttpResponse`}。4 つのセルは `get`・`get_response`・`request`・`request_response` です。それ以外の形（状態つきの組、バイト、ヘッダ）は、`HttpResponse` から取り出す関数で作ります。待たずに動かす・流して受け取る・時間の上限をつけるのは、ハンドル `start`・`poll`・`read_new`・`wait`・`cancel` の 1 つの道だけです。
- **状態の規則は 1 つ。** どの client も、最後まで受け取れた応答は ok です（404 も 3xx も）。err は通信の失敗だけです。
- **応答から取り出す関数**（`HttpResponse` が主語）: `status_code`、`body`、`body_bytes`、`get_header`、`header_values`、`headers`。
- **要求から取り出す関数**（`HttpRequest` が主語。`req_` は同じモジュールの応答の関数と名前をぶつけないための接頭辞）: `req_method`、`req_path`、`req_body`、`req_header`・`req_header_values`・`req_headers`（応答の 3 つ組と同じ形）、`query_params`、`param`。
- **応答を変える関数はコピーの `set_`**（R-L0-04）: `set_status`、`set_header`。
- URL の符号化・復号は `url` が持ちます（R-L1-28）。

### L2 の判定

- 軸の掛け算と穴: 動詞 × 形（本文・組・全部・バイト）× 上限の 3 軸で 23 関数、`post_status` や `post_bytes` のような推測できるのに無いセル（#3854 §2）→ R-http-01
- 状態の規則の外れ値: `request_stream` だけが 2xx 以外を err にする（#3854）→ R-http-01（撤去）
- 名前の罠: `http.status(resp, code)` は設定する関数なのに、取り出しの `status_code` と名前が近い → R-http-02
- 族の穴: 要求の側のヘッダに「全部の値」「map」が無い → R-http-03
- 重複: `http.url_decode` はフォーム形式の復号で、`url` の持ち分です（`url.decode_component` とは `+` の扱いが違う。実測は下）→ R-http-04
- 既定の振る舞い（#2827）→ R-http-05
- 番兵: `status_code` は状態行が読めないとき `0`（R-L0-06 の 4）→ R-http-01

## 案

### R-http-01 client を 2×2 ＋ ハンドルにまとめる（#3854 §2） — ○ 採用

- 内容:
  - 残す: `get(url) -> String`、`get_response(url) -> HttpResponse`、`request(method, url, body, headers) -> String`、`request_response(method, url, body, headers) -> HttpResponse`、ハンドルの 5 つ。
  - 足す: `fn body_bytes(resp: HttpResponse) -> Bytes`（UTF-8 に直さない本文）。
  - 撤去する 14 関数と書き換え:
    - `post(u, b)` → `request("POST", u, b, [:])`（`put`・`patch`・`delete` も同じ形）
    - `post_response(u, b)` → `request_response("POST", u, b, [:])`（`put_`・`patch_`・`delete_response` も同じ）
    - `get_status(u)` → `get_response(u)` から `status_code` と `body`、`request_status(…)` → `request_response(…)` から同じ
    - `get_bytes(u)` → `body_bytes(get_response(u)!)`、`request_bytes(…)` → `body_bytes(request_response(…)!)`
    - `request_stream`・`request_stream_with_limits` → `start` ＋ `read_new` ＋ `poll`/`wait`
  - 状態行が読めない応答は、`status_code = 0` の応答ではなく、交換そのものの err にします（番兵をなくす。R-L0-06 の 4）。
- #3854 との違い: #3854 は GET の近道 1 つ ＋ 交換 1 つ（8 関数）を出しました。この案は `get_response` と `request`（本文版）も残し、2×2 を埋めます。理由は 3 つです。(1) `http.get` と `http.request` は CHEATSHEET が教えている綴りで、どちらも消さずに済みます。(2) 行列に穴がないので、「`get` に `_response` があるなら `request` にも」という推測がすべて当たります。(3) 撤去する側の書き換えが、どれも残る側への 1 対 1 の置き換えになり、`almide fix` が機械的に直せます。戻り型の違う `request` の付け替えも起きません（R-L0-14 の 4）。
- 根拠の L0 規則: R-L0-01（同じ交換に綴りを 1 つ）、CLAUDE.md「族は行列で」、R-L0-14 の 4。
- (a) GET の近道を持つのは Go・Python・JS（`fetch` の既定）・Ktor・Swift。任意のメソッドの交換を 1 つの関数で持つのも全員です。動詞ごとの近道は Go（`Get`・`Post`・`Head` だけ）・Ktor にありますが、形の軸（状態だけ・バイトだけ）を関数名で分ける言語はありません。どこも応答の値から取り出します。
- (b) wasm（ADR-0023）: stock の p1 には HTTP の import が無く、p3 component が `wasi:http/client@0.3.0` で運びます。組み込み host は暫定の op 43〜50・53〜59 で動いています（台帳の `pending-port` の行。step 10 で退役）。ADR-0023 step 4 は「6 つの `*_response`、`request_stream`、ハンドルを shim に入れる」でした。この案で残る client は 4 ＋ 5 になり、shim が運ぶ関数が 23 → 9 に減ります。`request_stream` の SSE は、ハンドルの上に Almide で書けることを step 5 がすでに示しています。
- (c) 推測できるのに存在しない名前（`post_status` など）がなくなります。状態の規則が全部の client で同じになります。
- (d) 撤去と追加。各関数に `@deprecated(since=15, use="…")`（`use` は上の書き換え先）、`removal_epoch = 16`。上の書き換えはどれも 1 対 1 なので、`almide fix` が書き換えます（`request_stream` 系だけは形が変わるので、ハンドルを使う書き方をヒントで示します）。`status_code` の `0` をなくす変更は、正しく読めた応答の結果を変えないので、epoch の `breaks` には入れません。使用数（spec/tests/docs/stdlib）: `post` 2/1/13/0、`put` 1/1/8/0、`patch` 1/2/7/0、`delete` 1/1/8/0、`get_status` 6/4/6/1、`request_status` 5/5/5/1、`post_response` 0/1/6/1、`put_response` 0/2/6/1、`patch_response` 0/2/6/1、`delete_response` 0/1/6/1、`get_bytes` 2/6/5/1、`request_bytes` 2/7/5/1、`request_stream` 0/14/3/1、`request_stream_with_limits` 2/8/5/0。
- ゲート案: `tests/http_response_family_gate_test.rs` を 2×2 の行列の検査に置き換えます（4 セルがちょうどあること、同じ引数の並びの対であること）。`http_call_family_gate_test.rs` の `_with_limits` の規則は、流す関数が無くなるので「流すのはハンドルだけ」に書き換えます。全部の client を 1 つの試験用サーバーに当てて、状態・本文・err の分類が同じかを確かめる差分テスト（#3854 の案）を置きます。
- issue: #3854（既存。この案の形で決着）

### R-http-02 `http.status` を `http.set_status` にする — ○ 採用

- 内容: `fn set_status(resp: HttpResponse, code: Int) -> HttpResponse` を足し、`http.status` を撤去します。
- 綴り: 同じモジュールで応答を変えるコピーの関数は `set_header` で、R-L0-04 で `set` はコピーの動詞です。#3854 の叩き台の `with_status` は、`with_headers`（応答を作る関数）や `list.with_capacity` と同じ `with_` で始まり、「作る」と「変える」が混ざるので使いません。
- 根拠の L0 規則: R-L0-01、R-L0-04 の 3。
- (a) Go `WriteHeader`、JS `new Response(body, {status})`、Ktor `call.respond(status)`。綴りは割れています。
- (b) 純粋。(c) `http.status(resp)` と書いて状態を読もうとすると、引数の数のエラーになります（設定する関数なので）。
- (d) 改名。`@deprecated(since=15, use="http.set_status")`、`removal_epoch = 16`。`almide fix` で 1 対 1。使用数 11/6/9/0。
- issue: #3854（既存）

### R-http-03 要求の側のヘッダを応答と同じ 3 つ組にする — ○ 採用

- 内容: `fn req_header_values(req: HttpRequest, key: String) -> List[String]`（全部の値、届いた順）、`fn req_headers(req: HttpRequest) -> Map[String, String]`（小文字の名前の map、最初の値）を足します。
- 根拠: CLAUDE.md「族は行列で」。応答は {最初の値, 全部, map} の 3 つ、要求は最初の値だけでした。
- (a) Go の `Header.Get`・`Header.Values`、JS の `headers.get`・`headers.entries`。両方の向きで同じ形です。
- (b) 純粋（`http.serve` の handler の中で使う）。
- (d) 追加。使用数 0。
- ゲート案: 応答と要求のヘッダの関数が同じ 3 つ組であることを確かめる族ゲート。
- issue: #3917

### R-http-04 `http.url_decode` を撤去する（R-L1-28） — ○ 採用

- 内容: `http.url_decode(s)` を撤去します。置き換え先は `url.decode_form(s)`（[L2-url](L2-url.md) の R-url-01 で足す、フォーム形式の復号）です。
- 実測（probe-http）: `http.url_decode("a+b%20c%zz")` は `a b c%zz` です（`+` を空白に、壊れた `%` はそのまま、`String` を返す）。`+` を空白にするのはフォーム形式の復号で、`url.decode_component`（`+` を残し、壊れた `%` で err）とは違います。そのため置き換え先は `url.decode_form` です。
- 根拠の L0 規則: R-L0-01、R-L0-02（URL の文字列の関数は `url` に置く）、R-L0-06 の 4（失敗を番兵で隠さない）、R-L1-28。
- (d) 撤去。`@deprecated(since=15, use="url.decode_form")`、`removal_epoch = 16`。`almide fix` は `http.url_decode(s)` → `url.decode_form(s) ?? s` に書き換えます（不正な入力のとき元の文字列になる形）。違うのは不正な UTF-8（`%ff`）だけで、置換文字ではなく元の文字列になります。この違いはヒントの文に書きます。使用数 9/0/9/2。
- issue: #3950

### R-http-05 client の既定の振る舞い（#2827）の判定 — ○ 採用（項目ごとに判定）

| # | #2827 の項目 | 判定 | 理由 |
|---|---|---|---|
| 1 | リダイレクトを追う | × | 「3xx とその `Location` が答えで、応答は呼び出した URL のもの」が文書の約束です。追うようにすると同じ名前で結果が変わります（R-L0-14 の 4）。追いたいときは `get_header(r, "location")` で次を呼びます。 |
| 2 | `Accept-Encoding: gzip` を送り、展開する | ○ | `body` の意味（受け取った本文の文字列）は変わりません。いまは圧縮されたまま `String` になり、読めない値になっています。展開には stdlib の `zlib` を使い、p3 shim の側でも同じ Almide の本体を使えます。 |
| 3 | `User-Agent: almide/<version>` を既定で送る | ○ | 呼び出し側がヘッダを渡せばそれが優先です。User-Agent を必須にする API（GitHub など）が 403 になる問題が消えます。 |
| 4 | 本文があるとき `Content-Type: application/json` を付ける | × | 呼び出し側が `content-type` を渡せば付きません（`http_route_core.rs` の書き手 1 か所）。既定を変えると、同じ呼び出しで送るものが変わります（R-L0-14 の 4）。 |
| 5 | Bytes の本文を送る | × いまは足さない | 2×2 の行列に「送る本文の型」の 3 本目の軸を足すと、交換の関数が倍になります（CLAUDE.md「族は行列で」）。足すときは、交換の関数が `HttpRequest`（`http.new_request` で作る）を受け取る形にして、本文の型を 1 か所で持たせる案として、#2827 で改めて出します。 |
| 6 | keep-alive と接続の使い回し | 対象外 | 関数の表面は変わりません。ランタイムの中の改善として #2827 で進めます。 |

- #2828 の表の項目（プロキシ、信頼ストア、URL の分解、ヘッダの改行、サーバーの上限）は、どれも関数の表面を変えないランタイムの正しさの問題です。このページでは判定しません。
- (b) 2 と 3 は native の `http_client_core.rs` と p3 shim の両方に同じ規則で入れます（ADR-0023 §4 の「意味は 1 か所」）。
- issue: #2827（既存）

### R-http-06 応答の本文を差し替える `http.set_body` を足す — ○ 採用

- 案: `fn http.set_body(resp: HttpResponse, body: String) -> HttpResponse`。応答の部品 {状態, ヘッダ, 本文} のそれぞれに `set_` が 1 つそろいます。
- (a) Gleam `response.set_body` (F)。
- (c) middleware（`http.wrap`）で本文を書き換えるとき、いまは `with_headers` と `headers()` で組み立て直すしかなく、`set-cookie` のような複数の値のヘッダが黙って消えます。
- (d) 追加 1（内部の `__set_body` を公開します）。
- issue: #3974

### R-http-07 応答のヘッダの `get_header` と要求の `req_header` の不揃い — × 不採用（規則を書く。→ R-http-12 で解消）

- 理由: 1 つのヘッダを読む綴りは Gleam `get_header` (F)、Go `Header.Get`、JS `headers.get` と「get」を含むものが多く、`header` に改名する利点は小さいです。代わりに完全性の規則に次の 2 つを書きます。「`req_` は応答の関数と名前がぶつかるときだけ付ける」「`get_` は `get`・`get_response`（HTTP の GET）と `get_header`（取り出す）の 2 つの意味を持つ」。
- issue: なし

### R-http-08 取り出しの `status_code` を `status` にする — ○ 採用（epoch 17）

- いま: 取り出しは `http.status_code(resp) -> Int`、設定は `http.set_status(resp, code)` で語幹が違います。旧い設定の `http.status(resp, code)` は epoch 16 で撤去されます。
- 案: epoch 17 で `fn http.status(resp: HttpResponse) -> Int` を取り出しとして置き、`status_code` を撤去します。旧い意味（設定）の撤去から 1 epoch 空けて、学習データの旧い意味と混ざる期間を短くします。
- (a) `status` は JS・Python・Ktor・Gleam・Rust の HTTP ライブラリの 5 言語、`StatusCode` は Go・Swift の 2 言語です。
- (d) 改名。旧い設定の `http.status(r, 404)` は新しい取り出しに対して引数の数のエラーになるので、R-L0-14.4 を満たします。epoch 16 で `status_code` に `@deprecated(since=16, use="http.status")` を付け、epoch 17 で撤去します。
- issue: #3974

### R-http-09 ヘッダの全体を `List[(String, String)]` で表す — ○ 採用

- いま: 1 つの名前の値を全部読む `header_values`・`req_header_values` はあるのに、ヘッダの全体を読む `headers`・`req_headers` と、ヘッダを渡して作る `with_headers`・`new_request`・`request`・`request_response`・`start` は `Map[String, String]` です。同じ名前のヘッダが 2 つ以上あると、1 つしか残りません。`Set-Cookie` はカンマで 1 つにまとめられないので（RFC 9110 §5.3）、読んでから作り直すだけで壊れます。
- 案: ヘッダの全体は `List[(String, String)]`（届いた順・送る順。名前は小文字）で読み書きします。
  - `fn headers(resp: HttpResponse) -> List[(String, String)]`、`fn req_headers(req: HttpRequest) -> List[(String, String)]`
  - `with_headers`・`new_request`・`request`・`request_response`・`start` の `headers` 引数も `List[(String, String)]`
  - 1 つ読む `get_header`、全部読む `header_values`、1 つ書き換える `set_header` はそのままです。
- 根拠の L0 規則: R-L0-01（同じ情報を 2 つの形で持たない）。`url.parse_query` が組のリストで重複を保つのと同じ形です（R-http-10）。
- (a) Go `http.Header`（`map[string][]string`）、JS `Headers`（`append` で重複を持つ）、Python `http.client` の `getheaders()`（組のリスト）、Rust の http crate `HeaderMap`（多値）。どれも重複を保ちます。
- (b) native の client と p3 shim の両方で、ヘッダの列をそのまま渡します。
- (c) middleware で応答を読んで作り直しても、`Set-Cookie` が消えません。
- (d) 意味変更（型が変わるので、古い呼び出しは型エラー。黙って結果は変わらない）。`almide fix` は `map.from_list`/`map.to_list` の変換を差し込みます。
- 完全性の規則: ヘッダの全体を受け渡す引数と戻り値は、すべて `List[(String, String)]` です。`Map[String, String]` のヘッダは公開の面に残しません。
- issue: #3976

### R-http-10 `query_params` を `List[(String, String)]` にする — ○ 採用

- いま: `fn query_params(req: HttpRequest) -> Map[String, String]` は、`?tag=a&tag=b` の 2 つめを捨てます。同じクエリを読む `url.parse_query` は `List[(String, String)]` で、重複と順序を保ちます。
- 案: `fn query_params(req: HttpRequest) -> List[(String, String)]`。中身は `url.parse_query` と同じ規則で復号します。
- (a) Go `r.URL.Query()`（多値）、Python `parse_qsl`、JS `URLSearchParams.getAll`。
- (d) 意味変更（型エラーで気づく）。1 つだけ欲しいときは `list.find(ps, ((k, _)) => k == "tag")`。
- issue: #3976

### R-http-11 本文の `Bytes` の族を閉じる（`req_body_bytes`・`set_body_bytes`） — ○ 採用

- いま: 応答の本文は `body`（文字列）と `body_bytes`（バイト）の 2 つで読めますが、要求の本文は `req_body`（文字列）だけで、応答を作る側も `set_body`（文字列）だけです。`body` は不正な UTF-8 を置換文字にする（lossy）ので、サーバーでバイナリを受け取ると壊れます。
- 案: `fn req_body_bytes(req: HttpRequest) -> Bytes`、`fn set_body_bytes(resp: HttpResponse, body: Bytes) -> HttpResponse` を足します。
- 完全性の規則: 本文を読む・差し替える関数は、文字列とバイトの 2 つを持ちます（`body`/`body_bytes`、`req_body`/`req_body_bytes`、`set_body`/`set_body_bytes`）。R-http-12 で `req_` の側は無印に一本化します。文字列の側は lossy な UTF-8 で、そのことを文書に書きます。
- 保留: client がバイナリの本文を送る形（`request` の `body: String`）は、要求を組み立てる型の設計と一緒に決めます。ここでは足しません。（→ R-http-13 の `http.send` で解消）
- (d) 追加 2。
- issue: #3976

### R-http-12 要求と応答を `HttpMessage` で 1 つの関数にする — ○ 採用（R-http-07 を置き換え）

- いま: 同じ役割の関数が、応答は無印（`body`・`headers`・`header_values`）、要求は `req_` 付き（`req_body`・`req_headers`・`req_header_values`）です。1 つのヘッダを読む関数だけは、応答が `get_header`、要求が `req_header` で、語幹まで違います。R-http-09 で全体が `headers`・`req_headers` になったので、単数だけ `get_` が付く形も残りました。3 つの外部レビューが続けて挙げています。
- 案: 2 つの型を `HttpMessage` プロトコル（R-L0-31）に従わせ、読み書きを 1 つの総称関数にします。
  - `fn header[M: HttpMessage](m: M, name: String) -> String?`
  - `fn header_values[M: HttpMessage](m: M, name: String) -> List[String]`
  - `fn headers[M: HttpMessage](m: M) -> List[(String, String)]`
  - `fn body[M: HttpMessage](m: M) -> String`
  - `fn body_bytes[M: HttpMessage](m: M) -> Bytes`
  - `fn set_header[M: HttpMessage](m: M, name: String, value: String) -> M`
  - `fn set_body[M: HttpMessage](m: M, body: String) -> M`
  - `fn set_body_bytes[M: HttpMessage](m: M, body: Bytes) -> M`

  - 撤去: `get_header`（→ `header`）、`req_header`・`req_header_values`・`req_headers`・`req_body`・`req_body_bytes`（→ 無印）。
  - 要求にも `set_body_bytes` が使えるので、任意のバイト列を要求の本文にできます（R-http-13）。
- 根拠の L0 規則: R-L0-31、R-L0-01、R-L0-00。
- (a) Go は `Request.Header.Get` と `Response.Header.Get` が同じ `Header` 型の同じメソッド、JS fetch は `Request` と `Response` が同じ `Body` と `Headers` を持ちます。
- (d) 改名・撤去。`@deprecated(since=15, use=…)`、`removal_epoch = 16`、`almide fix` が 1 対 1 で書き換えます。
- issue: #3989

### R-http-13 組み立てた要求を送る `http.send` を足す — ○ 採用

- いま: クライアントの要求の本文は、`request`・`request_response`・`start` のどれも `body: String` です。任意のバイト列を送る道がありません。
- 案: `effect fn http.send(req: HttpRequest) -> HttpResponse` を足します。`http.new_request(…) |> http.set_body_bytes(b) |> http.send` と書けます（`set_body_bytes` は R-http-12 で要求にも使えます）。文字列の本文は今までどおり `request`・`request_response` で書けます。
- (a) Go `client.Do(req)`、Rust reqwest `Client::execute(request)`、JS `fetch(request)`。
- (b) 両 target で `request_response` と同じ経路です。
- (d) 追加 1。
- issue: #3989

### R-http-14 要求を組み立てる道を 1 つにする: `start(req, limits)`、`request_response` の撤去、`req_` を外す — ○ 採用

- いま: R-http-13 で `send(req: HttpRequest)` を足したので、組み立てた要求（`set_body_bytes` で本文をバイト列にしたものなど）は同期では送れます。ところが非同期の `start` は `method, url, body: String, headers, limits` の 5 引数のままで、バイト列の本文を送れません。応答全体を返す綴りも、`request_response(method, url, body, headers)` と `send(new_request(…))` の 2 つになりました。要求だけに使う関数は、`req_method`・`req_path` には `req_` が付き、`param`・`query_params` には付いていません（R-http-12 で共通の 8 関数からは外しました）。
- 案:
  - `effect fn http.start(req: HttpRequest, limits: HttpLimits) -> HttpCall` にします。`send` と同じ要求をそのまま渡せます。
  - `request_response` を撤去します（→ `send(new_request(method, url, body, headers))`）。`request`（本文の文字列を返す）と `get`・`get_response`（GET の近道）は残します。応答全体を返す一般の道は `send` だけです。
  - `req_method` を `method` に、`req_path` を `path` に改名します。要求だけに使う関数に接頭辞は付けません。どの型の関数かは引数の型が決めます（R-L0-02。「接頭辞は名前空間ではない」）。
- 根拠の L0 規則: R-L0-01（同じ役割に綴りは 1 つ）、R-L0-02。
- (a) Go は `client.Do(req)` で同期・非同期とも同じ要求を使い、`r.Method`・`r.URL.Path` に接頭辞がありません。JS の `fetch(request)` も同じです。
- (b) 呼び出しの形の変更です。通信の中身は変わりません。
- (c) 同期で書いた `send(req)` を非同期にするとき、`start(req, limits)` と引数の形を保ったまま書き換えられます。
- (d) `start` は意味変更（旧い 5 引数の呼び出しは型エラーになる）。`request_response` は撤去、`req_method`・`req_path` は改名で、どれも epoch 15 で `@deprecated`、epoch 16 で撤去、`almide fix` で 1 対 1 に書き換えます。
- issue: #3996
## L3: 関数表

| 関数 | シグネチャ(現行, mut 表示) | 他言語の主な綴り | 判定 | 案 |
|---|---|---|---|---|
| `http.serve` | `effect fn serve(port: Int, f: HttpHandler) -> Unit` | Go `ListenAndServe` / Deno `serve` | 据え置き | — |
| `http.response` | `fn response(status: Int, body: String) -> HttpResponse` | JS `new Response` / Ktor `respondText` | 据え置き | — |
| `http.json` | `fn json(status: Int, body: String) -> HttpResponse` | JS `Response.json` | 据え置き | — |
| `http.with_headers` | `fn with_headers(status: Int, body: String, headers: List[(String, String)]) -> HttpResponse` | JS `new Response(b, {headers})` | 意味変更（`headers` を `List[(String, String)]` に） | R-http-09 |
| `http.redirect` | `fn redirect(url: String) -> HttpResponse` | JS `Response.redirect` / Go `http.Redirect` | 据え置き | — |
| `http.status` | `fn status(resp: HttpResponse, code: Int) -> HttpResponse` | Go `WriteHeader` | 改名（→ `set_status`） | R-http-02 |
| `http.set_status` | `fn set_status(resp: HttpResponse, code: Int) -> HttpResponse`（案） | 同上 | 追加 | R-http-02 |
| `http.set_header` | `fn set_header(resp: HttpResponse, key: String, value: String) -> HttpResponse` | Go `Header().Set` / JS `headers.set` | 意味変更（`HttpRequest` も受け取る） | R-http-12 |
| `http.body` | `fn body(resp: HttpResponse) -> String` | JS `text()` / Go `Body` / Ktor `bodyAsText` | 意味変更（`HttpRequest` も受け取る） | R-http-12 |
| `http.body_bytes` | `fn body_bytes(resp: HttpResponse) -> Bytes`（案） | JS `arrayBuffer` / reqwest `bytes` / Ktor `bodyAsBytes` | 追加（`HttpRequest` も受け取る） | R-http-12 |
| `http.status_code` | `fn status_code(resp: HttpResponse) -> Int` | Go `StatusCode` / JS `status` | 改名 → `status`（epoch 17。`0` の番兵もなくす） | R-http-04、R-http-08 |
| `http.get_header` | `fn get_header(resp: HttpResponse, key: String) -> String?` | Go `Header.Get` / JS `headers.get` | 改名 → `header`（`HttpRequest` も受け取る） | R-http-12 |
| `http.header_values` | `fn header_values(resp: HttpResponse, key: String) -> List[String]` | Go `Header.Values` | 意味変更（`HttpRequest` も受け取る） | R-http-12 |
| `http.headers` | `fn headers(resp: HttpResponse) -> List[(String, String)]` | JS `headers` / reqwest `headers` | 意味変更（`HttpRequest` も受け取る） | R-http-12 |
| `http.req_method` | `fn req_method(req: HttpRequest) -> String` | Go `r.Method` / JS `req.method` | 改名 → `method` | R-http-14 |
| `http.req_path` | `fn req_path(req: HttpRequest) -> String` | Go `r.URL.RequestURI()` | 改名 → `path`（クエリを含むことを文書に明記） | R-http-14 |
| `http.req_body` | `fn req_body(req: HttpRequest) -> String` | Go `r.Body` / JS `req.text()` | 撤去 → `http.body` | R-http-12 |
| `http.req_header` | `fn req_header(req: HttpRequest, key: String) -> String?` | Go `r.Header.Get` | 撤去 → `http.header` | R-http-12 |
| `http.req_header_values` | `fn req_header_values(req: HttpRequest, key: String) -> List[String]`（案） | Go `r.Header.Values` | 追加しない（R-http-12 の `http.header_values` に一本化） | R-http-12 |
| `http.req_headers` | `fn req_headers(req: HttpRequest) -> List[(String, String)]`（案） | JS `req.headers` | 追加しない（R-http-12 の `http.headers` に一本化） | R-http-12 |
| `http.query_params` | `fn query_params(req: HttpRequest) -> List[(String, String)]` | Go `r.URL.Query()` | 意味変更（`List[(String, String)]` を返す） | R-http-10 |
| `http.url_decode` | `fn url_decode(s: String) -> String` | Go `QueryUnescape` / Python `unquote_plus` | 撤去 → `url.decode_form(s) ?? s` | R-http-04 |
| `http.get` | `effect fn get(url: String) -> String` | Go `http.Get` / reqwest `get` / Python `urlopen` | 据え置き | R-http-01 |
| `http.get_response` | `effect fn get_response(url: String) -> HttpResponse` | JS `fetch(u)` / Ktor `client.get(u)` | 据え置き | R-http-01 |
| `http.request` | `effect fn request(method: String, url: String, body: String, headers: List[(String, String)]) -> String` | Go `Client.Do` / JS `fetch(u, {method})` | 意味変更（`headers` を `List[(String, String)]` に） | R-http-09 |
| `http.request_response` | `effect fn request_response(method: String, url: String, body: String, headers: List[(String, String)]) -> HttpResponse` | 同上 | 撤去（→ `send(new_request(method, url, body, headers))`） | R-http-14 |
| `http.post` | `effect fn post(url: String, body: String) -> String` | Go `http.Post` / Ktor `post` | 撤去（→ `request("POST", …)`） | R-http-01 |
| `http.put` | `effect fn put(url: String, body: String) -> String` | Ktor `put` | 撤去（→ `request("PUT", …)`） | R-http-01 |
| `http.patch` | `effect fn patch(url: String, body: String) -> String` | Ktor `patch` | 撤去（→ `request("PATCH", …)`） | R-http-01 |
| `http.delete` | `effect fn delete(url: String) -> String` | Ktor `delete` | 撤去（→ `request("DELETE", …)`） | R-http-01 |
| `http.get_status` | `effect fn get_status(url: String) -> (Int, String)` | — | 撤去（→ `get_response`） | R-http-01 |
| `http.request_status` | `effect fn request_status(method: String, url: String, body: String, headers: Map[String, String]) -> (Int, String)` | — | 撤去（→ `send`） | R-http-01 |
| `http.post_response` | `effect fn post_response(url: String, body: String) -> HttpResponse` | — | 撤去（→ `send(new_request("POST", …))`） | R-http-01 |
| `http.put_response` | `effect fn put_response(url: String, body: String) -> HttpResponse` | — | 撤去（→ `send(new_request("PUT", …))`） | R-http-01 |
| `http.patch_response` | `effect fn patch_response(url: String, body: String) -> HttpResponse` | — | 撤去（→ `send(new_request("PATCH", …))`） | R-http-01 |
| `http.delete_response` | `effect fn delete_response(url: String) -> HttpResponse` | — | 撤去（→ `send(new_request("DELETE", …))`） | R-http-01 |
| `http.get_bytes` | `effect fn get_bytes(url: String) -> Bytes` | reqwest `bytes` | 撤去（→ `body_bytes(get_response(u)!)`） | R-http-01 |
| `http.request_bytes` | `effect fn request_bytes(method: String, url: String, body: String, headers: Map[String, String]) -> Bytes` | — | 撤去（→ `body_bytes(send(new_request(…))!)`） | R-http-01 |
| `http.request_stream` | `effect fn request_stream(method: String, url: String, body: String, headers: Map[String, String], on_chunk: (String) -> Unit) -> Unit` | reqwest `bytes_stream` / JS `body.getReader` | 撤去（→ ハンドル） | R-http-01 |
| `http.request_stream_with_limits` | `effect fn request_stream_with_limits(method: String, url: String, body: String, headers: Map[String, String], limits: HttpLimits, on_chunk: (String) -> Unit) -> Unit` | — | 撤去（→ ハンドル） | R-http-01 |
| `http.start` | `effect fn start(method: String, url: String, body: String, headers: List[(String, String)], limits: HttpLimits) -> HttpCall` | JS `fetch`（Promise） | 意味変更（`start(req: HttpRequest, limits: HttpLimits)` にする） | R-http-09、R-http-14 |
| `http.poll` | `effect fn poll(c: HttpCall) -> Result[HttpResponse, String]?` | Python `Popen.poll` に相当 | 据え置き（`process.poll` と同じ綴り） | R-http-01 |
| `http.read_new` | `effect fn read_new(c: HttpCall) -> String` | — | 据え置き | R-http-01 |
| `http.wait` | `effect fn wait(c: HttpCall) -> HttpResponse` | — | 据え置き | R-http-01 |
| `http.cancel` | `effect fn cancel(c: HttpCall) -> Unit` | JS `AbortController.abort` | 据え置き | R-http-01 |
| `http.new_request` | `fn new_request(method: String, target: String, body: String, headers: List[(String, String)]) -> HttpRequest` | JS `new Request` | 意味変更（`headers` を `List[(String, String)]` に） | R-http-09 |
| `http.param` | `fn param(req: HttpRequest, name: String) -> String?` | Go `r.PathValue` | 据え置き | — |
| `http.route` | `fn route(pattern: String, handler: HttpHandler) -> HttpRoute` | Go `ServeMux.HandleFunc` | 据え置き | — |
| `http.mount` | `fn mount(prefix: String, sub: (HttpRequest) -> Result[HttpResponse, String]) -> HttpRoute` | Go `StripPrefix` | 据え置き | — |
| `http.router` | `fn router(routes: List[HttpRoute]) -> Result[HttpHandler, String]` | Go `ServeMux` | 据え置き | — |
| `http.wrap` | `fn wrap(handler: HttpHandler, middleware: List[HttpMiddleware]) -> HttpHandler` | — | 据え置き | — |
| `http.decode_json` | `fn decode_json[T](req: HttpRequest, decode: (Value) -> Result[T, String]) -> Result[T, HttpResponse]` | — | 据え置き | — |
| `http.set_body` | `fn set_body(resp: HttpResponse, body: String) -> HttpResponse`（案） | Gleam `set_body` (F) | 追加（`HttpRequest` も受け取る） | R-http-12 |
| `http.req_body_bytes` | `fn req_body_bytes(req: HttpRequest) -> Bytes`（案） | Go `io.ReadAll(r.Body)` / JS `req.arrayBuffer()` | 追加しない（R-http-12 の `http.body_bytes` に一本化） | R-http-12 |
| `http.set_body_bytes` | `fn set_body_bytes(resp: HttpResponse, body: Bytes) -> HttpResponse`（案） | JS `new Response(bytes)` | 追加（`HttpRequest` も受け取る） | R-http-12 |
| `http.send` | `effect fn send(req: HttpRequest) -> HttpResponse`（案） | Go `client.Do` / reqwest `execute` / JS `fetch(request)` | 追加 | R-http-13 |
