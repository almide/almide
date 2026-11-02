# http

[![English](https://img.shields.io/badge/English-d0d7de?style=flat-square)](http.md) [![日本語](https://img.shields.io/badge/%E6%97%A5%E6%9C%AC%E8%AA%9E-24292f?style=flat-square)](http.ja.md) [![简体中文](https://img.shields.io/badge/%E7%AE%80%E4%BD%93%E4%B8%AD%E6%96%87-d0d7de?style=flat-square)](http.zh-CN.md)



> 原文：[http.md @ 35297a6](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/stdlib/http.md)。非規範的なソース固定スナップショットです。英語原文を正とし、自動生成・自動同期は行いません。署名・型・バージョン注記とコード例（コメントを含む）は原文のまま保持し、API と型の説明の訳を末尾に併記しています。コード例、動作、TLS・セキュリティ上の主張は独立に実行・再検証していません。

> 原文上の注意：Content-Type を名前で書けないという一般的な説明と、後段の「リクエストでは設定可能」という限定をともに保持しています。また、不正なステータス行で取得関数が 0 を返す説明と、不正・不完全なレスポンスをクライアントが拒否する説明も原文どおり併記し、翻訳で整合性を補正していません。

HTTP クライアントとサーバー。import http、effect。

`HttpRequest` / `HttpResponse` は、このモジュールのランタイムに支えられた名義型です。`import http` により、ユーザーの型注釈でも非修飾名・修飾名のどちらも解決され、リクエストを扱う型付きヘルパーを書けます。

```almd check
import http

fn handle(req: HttpRequest) -> http.HttpResponse = http.response(200, http.path(req))

effect fn main() -> Unit = {
  http.serve(3000, handle)
}
```

どちらの型も `HttpMessage` なので、どちらの種類のメッセージでも同じ関数でヘッダーとボディを読み書きできます。`header`、`header_values`、`headers`、`body`、`body_bytes`、`set_header`、`set_body`、`set_body_bytes` はリクエストまたはレスポンスを取り、同じ種類を返します。ヘッダーリストはどこでも `List[(String, String)]` です。`with_headers`、`new_request`、`request`、`headers` の戻り値が該当します。受信ヘッダーは通信上の順序と重複フィールドを保持します。一方、`with_headers` などのビルダーは大文字・小文字を区別せず名前を挿入・更新し、最初の綴りと最後の値を保持します。

クライアント関数は `Result` を返す `effect fn` です。伝播は明示的です（ADR-0008）。effect fn 内では `http.send(req)!` と書くか、`match` / `??` で `Result` を処理します。単独の `http.get(url)` 文は E042 です。

### `http.serve(port: Int, handler: HttpHandler) -> Unit`

指定ポートでリクエストハンドラー付きの HTTP サーバーを起動します。

```almd check
import http

effect fn main() -> Unit = {
  http.serve(3000, (req) => http.response(200, "ok"))
}
```

サーバーは `0.0.0.0:port`（すべてのインターフェース、ADR-0020 §5.7）で待ち受けます。同時に最大 64 接続を受け付けて読み取り、リクエスト全体が到着した順に 1 件ずつハンドラーを呼び出します。処理中の接続が 64 に達すると、1 つが閉じるまで受け付けを止め、新しい接続は OS のバックログで待ちます。リクエストをゆっくり送るクライアントが占有するのは自分の接続だけなので、他のクライアントには引き続き応答します（#4021）。サーバーはハンドラーに渡す前に、次の制限内で各リクエストを読みます。

| 制限 | 値 | 応答 |
|---|---|---|
| 接続受け付けからのリクエストヘッド（リクエスト行とヘッダーフィールド） | 10 s | `408`。それまで何も送らなかったクライアントは応答なしで切断 |
| 接続受け付けからのリクエスト全体（ボディの読み取りとハンドラー待ちを含む） | 30 s | `503` |
| `Content-Length` で宣言された、またはチャンク送信されたボディ | 1 MiB | `413` |
| リクエスト行 / ヘッダー行 | 8 KiB | `414` / `431` |
| ヘッダーフィールド数 | 100 | `431` |

`Content-Length` より短いボディは `400` です。これらの場合、ハンドラーは呼ばれません。チャンク化されたリクエストボディはデコードされます。
リクエストヘッドは RFC 9112 に従って読みます（C-405）。CRLF で終わらない行、トークン形式のメソッドを持つ `METHOD SP target SP HTTP/1.1`（または `HTTP/1.0`）ではないリクエスト行、`OPTIONS *` または `CONNECT host:port` のターゲット、ヘッダーのコロン前の空白（`Transfer-Encoding : chunked`）、コロンのないヘッダー行、obs-fold の継続行、2 つの `Host` フィールド、`Host` のない HTTP/1.1 リクエストは、いずれもハンドラー実行前に `400` になります。絶対形式のターゲット（`GET http://host/p?q HTTP/1.1`）は、ハンドラーには `/p?q` として届きます。
レスポンスは `HTTP/1.1 <status> <reason>`（登録済みの理由句、未登録コードなら空）、順序どおりのレスポンスヘッダー、`Connection: close`、`Content-Length`、ボディの順で送られ、その後接続が閉じます。HEAD リクエスト、`204`、`304` にはボディがありません。フレーミングはサーバーが管理し、ハンドラーの `Connection`、`Content-Length`、`Transfer-Encoding`、`Keep-Alive` フィールドは除去します。ハンドラーが設定していない限り、すべてのレスポンスに `X-Content-Type-Options: nosniff` が付きます（C-407）。名前がトークンでないレスポンスヘッダー、HTAB 以外の制御文字を値に含むレスポンスヘッダー、`200..=599` 外のステータス（1xx を含む）は拒否されます。応答はボディが `Internal Server Error` の `500` となり、理由は stderr に 1 行で出力されます（`http.serve: refused the response: response status 42 is not a final status
code`）。30 s より遅いレスポンスは `503` に置き換えます。

ハンドラーの `err(m)` はクライアントに届きません（C-409、#4150）。`m` にはパス、オペランド、errno テキストが含まれ得ます（ハンドラー内の `fs.read_text(p)!` には絶対パスが入ります）。クライアントには固定ボディ `Internal Server Error (request <id>)` の `500` が返ります。`id` は、リクエストのメソッド、空白、ターゲット、LF、ボディに対する 32-bit FNV-1a（`hash.fnv1a32`）の小文字 16 進数 8 桁で、同じリクエストなら全ターゲットで同じです。stderr には `http.serve: request <id>: handler error: "<m>"` を出し、`m` 内の入力がログ行を偽造できないよう `\`、`"`、制御文字をエスケープします。クライアントが報告する id でエラーを探してください。ポートをバインドできない場合、プログラムは `Error: bind failed: <reason>` と終了コード 1 で停止します。

ハンドラー内の実行時中断（範囲外インデックス、ゼロ除算、`panic(msg)`）で失敗するのは、そのリクエストだけです（C-434、#4203）。クライアントには `err` と同じ `500` と固定ボディが返り、stderr には `http.serve: request <id>: handler aborted: "<text>"` を 1 行出します（`panic` なら `PANIC: <msg>`）。ハンドラーが開いたまま閉じなかったソケット、unix 記述子、http 呼び出しも一緒に閉じられます。サーバーは次のリクエストの処理を続けます。スタック枯渇と `process.exit` は、ハンドラー自身が `Error: ` 行を出力した後でもプログラムを終了させます。メモリ不足も同様です。ハンドラーが割り当てを得られないと、プログラムは `Error: out of memory` と終了コード 1 で終了します（C-197）。ハンドラーが構築中のものは未完成のままなので、そのリクエストには応答しません。

**wasm では**（C-367）、`almide run app.almd --target wasm` が同じプログラムを埋め込みホストで提供します。native プロセスとまったく同様、1 インスタンスがすべてのリクエストを 1 件ずつ処理し、`main` は 1 回だけ実行されます。ホストは native と同じサーバーコード、同じ制限（64 接続、リクエストヘッドの 10 s 期限と `408`）で接続を読むため、ここでも遅いクライアントは他を妨げません。ステータス行、ヘッダー、ボディは native とバイト単位で同一です。`almide build --target wasm` の標準成果物には待ち受けソケットがないため、`almide build` と `almide check --target wasm` は引き続き `http.serve` を拒否します（E081、#2659）。

## ルーティングとミドルウェア

ハンドラーはリクエストからレスポンスへの effect 関数値、`type HttpHandler = effect (HttpRequest) -> HttpResponse` です。ルーターとラップされたアプリも同様です。`http.serve` はハンドラーを動かす方法の 1 つで、`http.new_request` も別の方法です。テストがコード内でリクエストを作り、ソケットなしでアプリを直接呼びます。ルーティングとリクエスト構築は純粋で、native と wasm で同じ動作をし、ルーターも他のハンドラーと同様に提供できます。
この例は両ターゲットで以下の行を出力します。wasm ではルーターは構造的な系統で提供されます（#2664、`spec/stdlib/http_router_test.almd` で維持）。

ルーターとその前にあるミドルウェアは、1 つの正規形パスを読みます（C-406、#4133）。内部に空セグメント（`//admin`）、`.` または `..` セグメント、パーセントエンコードされた非予約バイト（`%61dmin`、`%2e`）、バックスラッシュ、制御文字を含むパスは `400 Bad Request: the path is not in canonical form` です。そのため `string.starts_with(http.path(req), "/admin")` のようなガードは、ルーターが振り分けに使うものと同じパスを検査します。バインドされたパラメーターがデコード後に `.` または `..` になるか、`/`（`%2F`）、`\`、NUL を含むと `400 Bad Request: a path
parameter is a dot segment or holds a /, \ or NUL` になります（#4134）。したがって `/static/{file}` が `../../etc/passwd` をバインドすることはありません。静的ファイル用ヘルパーはまだありません（`http.serve_dir` は存在しません）。ハンドラーでファイルを提供する場合は、文字列連結ではなく、ルート付きの `fs` ファミリーでルート内の名前を解決してください。

```almd check
import http

// What the client may set: an input type (`: Decode`, not `Codec`).
type NewUser: Decode = { name: String, age: Int }

effect fn get_user(req: HttpRequest) -> HttpResponse = http.response(200, "user " + (http.param(req, "id") ?? ""))

effect fn create_user(req: HttpRequest) -> HttpResponse = match http.decode_json[NewUser](req) {
  ok(u) => http.response(201, u.name),
  err(bad) => bad,
}

fn server_header(next: HttpHandler) -> HttpHandler = (req) => http.set_header(next(req)!, "Server", "almide")

effect fn show(app: HttpHandler, method: String, target: String, body: String) -> String = {
  let res = app(http.new_request(method, target, body, []))!
  "${http.status(res)} ${http.body(res)}"
}

effect fn main() -> Unit = {
  let routes = http.router([
    http.route("GET /users/{id}", get_user),
    http.route("POST /users", create_user),
  ])!
  let app = http.wrap(routes, [server_header])
  println(show(app, "GET", "/users/42?x=1", "")!)
  println(show(app, "POST", "/users", "{\"name\":\"ann\",\"age\":3}")!)
  println(show(app, "POST", "/users", "{}")!)
  println(show(app, "POST", "/users", "{\"name\":\"ann\",\"age\":3,\"is_admin\":true}")!)
  println(show(app, "DELETE", "/users/42", "")!)
  println(show(app, "GET", "/nope", "")!)
}
```
```text
200 user 42
201 ann
400 Bad Request: missing field 'name'
400 Bad Request: unknown key 'is_admin'
405 Method Not Allowed
404 Not Found
```

- **パターン** — `http.route("METHOD /path", handler)`。メソッドは省略可能です（`"/health"` はすべてのメソッドに応答）。`{name}` は 1 セグメントをバインドしてパーセントデコードし（`+` は `+` のまま）、`http.param(req, name)` で読みます。末尾の `{name...}` は残りのパスをバインドし、空の場合もあります。照合にはクエリ文字列を除くパスを使い、空セグメントは数えないため、`/users/` は `/users` です。
- **優先順位** — 登録順に関係なく、最も具体的に一致するルートが応答します。`GET /users/me` は `GET /users/{id}` より優先され、メソッド付きルートは同じパスのメソッドなしルートより優先されます。
- **拒否** — `http.router` はすべての問題を示す `err` を返し、壊れたテーブルは提供されません。問題は、あるリクエストに一致する 2 ルートのどちらもより具体的ではない場合（Go 1.22 の規則。同一パターンの異なる表記も含む）、末尾でない `{name...}`、同じ名前の 2 回のバインド、大文字の英字でないメソッド、`/` で始まらないパスです。
- **ルーターの応答** — パスに一致するルートがなければ `404 Not Found`。パスには一致するがメソッドに一致しなければ `Allow` ヘッダー付きの `405 Method Not Allowed`。`HEAD` は `GET` ルートにフォールバックしてボディを除去します。`/` で始まらないリクエストターゲット、またはパス内で 16 進数 2 桁が続かない `%` は `400 Bad Request`。ハンドラーの `err` は `err` のままです（`http.serve` が 500 に変換）。
- **マウント** — `http.mount("/api", sub)` は `/api/items?x=1` を `/items?x=1` として `sub` に渡します。プレフィックスがバインドしたパラメーター（`/orgs/{org}`）は `sub` から引き続き見えます。
- **ミドルウェア** — `type HttpMiddleware = (HttpHandler) -> HttpHandler`。`http.wrap(app, [a, b])` は `a(b(app))` で、リストの先頭が最も外側です。ミドルウェアはラップしたハンドラーを呼ばずに応答してもかまいません。リクエストごとのデータは引数で渡し、可変コンテキストはありません。
- **リクエストボディ** — `http.decode_json[T](req)` はボディを入力型 `T` に解析します。これは `: Decode` を宣言し、同時に `Encode` ではなく、全フィールドが入力であるレコードまたは variant です（C-436）。プログラムが送信や保存にも使う `Codec` 型は検査エラー E110 で、`T` 内にネストした `Codec` レコードも同様です。入力型にデコードし、ドメイン値はコードで構築することで、クライアントが設定してはならないフィールド（`is_admin`）の入り口をなくします。ボディは `T.decode_strict` でデコードされ、その `err` は理由を示す、そのまま使える `400 Bad Request` レスポンスです。理由には JSON でないこと、フィールド型の誤り、必須フィールドの欠落、どの深さでも `T` にないキー（`unknown key '<k>'`、`unknown key 'verified', at address` のようにパス付き）が含まれます。
- **範囲外** — 組み込みロガー / CORS / セッション、ストリーミングレスポンス、WebSocket はパッケージに委ねます。

### `http.response[B: Body](status: Int, body: B) -> HttpResponse`

`Content-Type` がボディの型となるレスポンスを作ります（ADR-0032）。String は `text/plain; charset=utf-8`、Bytes は `application/octet-stream`、`Value` は `application/json`、`html.*` のページは `text/html; charset=utf-8`、パッケージの安全な型はその固有のリテラル型（スクリプトを実行する場合は `[trust] media` で許可、ADR-0032 D7）、`http.typed(t, bytes)` はリテラル型 `t` です。ヘッダー文字列でボディに別の型ラベルを付けることはできず、Content-Type は名前で書き込めません（E116）。標準ライブラリ外でスクリプト実行可能なメディア型を所有するには、アプリケーションの許可が必要です（E118）。

```almd run
import html
import http

fn main() -> Unit = {
  let resp = http.response(200, "Hello!")
  println(http.body(resp))
  println(http.header(resp, "Content-Type") ?? "none")
  let page = http.response(200, html.escape("<b>hi</b>"))
  println(http.body(page))
  println(http.header(page, "Content-Type") ?? "none")
}
```
```output
Hello!
text/plain; charset=utf-8
&lt;b&gt;hi&lt;/b&gt;
text/html; charset=utf-8
```

### `http.typed(media_type: String, body: Bytes) -> TypedBody`

他のメディア型のボディです。型はリテラルで記述し、検査時に読み取ります（E117）。計算した型、不正な型、ブラウザがスクリプトを実行する型（`html` が所有する HTML と XHTML、許可されたパッケージの SVG、XML と `+xml` 型）は拒否されます。そのため String が安全な型を経由せずマークアップとしてブラウザに届くことはありません。現在、構築したボディはテキストとして保持するため、UTF-8 でないバイトは非可逆にデコードされます（#4311）。

```almd run
import http

fn main() -> Unit = {
  let resp = http.response(200, http.typed("text/csv", bytes.from_string("id,name\n1,ada")))
  println(http.header(resp, "Content-Type") ?? "none")
}
```
```output
text/csv
```

### `http.with_body[B: Body](resp: HttpResponse, body: B) -> HttpResponse`

レスポンスのボディと `Content-Type` を一緒に置き換えます。ステータスとその他のヘッダーは保持します。

```almd run
import http

fn main() -> Unit = {
  let resp = http.with_body(http.response(201, "draft"), http.typed("text/markdown", bytes.from_string("# final")))
  println("${http.status(resp)} ${http.header(resp, "Content-Type") ?? "none"} ${http.body(resp)}")
}
```
```output
201 text/markdown # final
```

### `http.json(status: Int, body: String) -> HttpResponse`

ステータスコード付きの JSON HTTP レスポンスを作ります。

```almd run
import http
import json

fn main() -> Unit = {
  let data = value.object([("ok", value.bool(true))])
  let resp = http.json(200, json.stringify(data))
  println(http.body(resp))
  println(http.header(resp, "Content-Type") ?? "none")
}
```
```output
{"ok":true}
application/json
```

### `http.with_headers[B: Body](status: Int, body: B, headers: List[(String, String)]) -> HttpResponse`

ボディの `Content-Type` を先頭とし、その後に指定されたヘッダーだけをリスト順に持つレスポンスを作ります（契約 C-275）。同じ名前を大文字・小文字を問わず 2 回指定すると、最初の綴りと最後の値を保持します。リスト内の `Content-Type` は拒否します。これはボディのものだからです（E116、計算された名前なら実行中断）。

```almd run
import http

fn main() -> Unit = {
  let body = http.typed("text/csv", bytes.from_string("id,name\n1,ada"))
  let resp = http.with_headers(200, body, [("Cache-Control", "no-store")])
  println(http.header(resp, "Content-Type") ?? "none")
  println(http.header(resp, "X-Nope") ?? "none")
}
```
```output
text/csv
none
```

### `http.redirect(url: String) -> HttpResponse`

302 の一時リダイレクトレスポンスを作ります。`Location`、空のボディ、空の String ボディに対応する `Content-Type: text/plain; charset=utf-8` を持ちます。

```almd run
import http

fn main() -> Unit = {
  let resp = http.redirect("/new-path")
  println(http.header(resp, "Location") ?? "none")
}
```
```output
/new-path
```

### `http.set_status(resp: HttpResponse, code: Int) -> HttpResponse`

レスポンスのステータスコードを設定します。ここでは範囲検査しません。送信時に `http.serve` が `200..=599` 外のステータスを拒否します。

```almd run
import http

fn main() -> Unit = {
  let resp = http.response(200, "created")
  let created = http.set_status(resp, 201)
  println("${http.status(created)} ${http.body(created)}")
}
```
```output
201 created
```

### `http.status(resp: HttpResponse) -> Int`

レスポンスのステータスコードを読みます。`http.set_status` に対応する取得関数です。取得したレスポンスのステータス行を読めなかった場合は `0` です。

```almd run
import http

fn main() -> Unit = {
  let resp = http.set_status(http.response(200, "teapot"), 418)
  println("${http.status(resp)}")
}
```
```output
418
```

## どちらのメッセージでも使えるヘッダーとボディ

この節の各関数は `HttpRequest` または `HttpResponse`（`[M: HttpMessage]`）を取り、書き込み関数は同じ種類を返します。`http.new_request` で構築したリクエストと `http.response` で構築したレスポンスを、同じ呼び出しで読み書きします。

### `http.header(m, name: String) -> Option[String]`

ヘッダーフィールドの最初の値です。フィールドがなければ `none`。フィールド名は大文字・小文字を区別しないため（RFC 9110 §5.1）、`"content-type"` と `"Content-Type"` は同じフィールドです。

```almd run
import http

fn main() -> Unit = {
  let resp = http.response(200, "hi")
  println(http.header(resp, "Content-Type") ?? "none")
  println(http.header(resp, "content-type") ?? "none")
  println(http.header(resp, "X-Nope") ?? "none")
  let req = http.new_request("GET", "/", "", [("Authorization", "Bearer t")])
  println(http.header(req, "authorization") ?? "none")
}
```
```output
text/plain; charset=utf-8
text/plain; charset=utf-8
none
Bearer t
```

### `http.header_values(m, name: String) -> List[String]`

あるフィールドのすべての値を通信上の順序で返します。繰り返し現れるフィールド（`Set-Cookie`）用のアクセサーで、`header` は最初の出現だけを返します。検索は大文字・小文字を区別せず、存在しないフィールドは `[]` です。

```almd run
import http

fn main() -> Unit = {
  let resp = http.with_headers(200, "", [("Set-Cookie", "a=1; HttpOnly")])
  println("${http.header_values(resp, "set-cookie")}")
  println("${http.header_values(resp, "X-Nope")}")
}
```
```output
["a=1; HttpOnly"]
[]
```

### `http.headers(m) -> List[(String, String)]`

各ヘッダーを `(name, value)` の組で返します。名前は送信時または設定時の綴り、順序は通信上の順序で、重複フィールドは出現ごとに保持します。小文字化もマージもしません。検索には大文字・小文字を無視する `header` / `header_values` を使ってください。

```almd run
import http

fn main() -> Unit = {
  let resp = http.with_headers(200, "", [("X-Frame-Options", "DENY"), ("X-Kind", "csv")])
  let lines = http.headers(resp) |> list.map(((k, v)) => "${k}: ${v}")
  println(list.join(lines, "\n"))
}
```
```output
Content-Type: text/plain; charset=utf-8
X-Frame-Options: DENY
X-Kind: csv
```

### `http.body(m) -> String`

ボディのテキストです。クライアントが取得したボディは非可逆な UTF-8 デコードを行います。

```almd run
import http

fn main() -> Unit = {
  let resp = http.response(200, "Hello!")
  println(http.body(resp))
  let req = http.new_request("POST", "/items", "{\"n\":1}", [])
  println(http.body(req))
}
```
```output
Hello!
{"n":1}
```

### `http.body_bytes(m) -> Bytes`

ボディを `Bytes` で返します。クライアントが取得したレスポンス（`get_response`、`send`、`*_response` ファミリー）では、サーバーが送ったバイトそのものであり、UTF-8 でないボディ（画像、音声）も保持されます。`http.body` はそれを非可逆に UTF-8 デコードしたものです。コード内で構築したメッセージでは、`http.body(m)` の UTF-8 バイトです。

```almd run
import http

fn main() -> Unit = {
  let resp = http.response(200, "héllo")
  println("${bytes.len(http.body_bytes(resp))}")
}
```
```output
6
```

### `http.set_header(m, name: String, value: String) -> M`

1 つのヘッダーフィールドを設定します。名前は大文字・小文字を区別しないので、綴りによらず最初に一致するフィールドの値をその位置で置き換えます（綴りと位置は維持）。なければ末尾に追加します。この方法で 2 つ目のエントリが増えることはありません。レスポンスでは `Content-Type` を拒否します。これはボディの型だからです（E116、計算された名前なら実行中断）。リクエストでは設定できます。

```almd run
import http

fn main() -> Unit = {
  let resp = http.with_headers(200, "hi", [("X-Kind", "a")])
  let tagged = http.set_header(http.set_header(resp, "X-Custom", "value"), "x-kind", "b")
  let lines = http.headers(tagged) |> list.map(((k, v)) => "${k}: ${v}")
  println(list.join(lines, "\n"))
}
```
```output
Content-Type: text/plain; charset=utf-8
X-Kind: b
X-Custom: value
```

### `http.set_body(m, body: String) -> M`

ボディのテキストを置き換えます。レスポンスでは `Content-Type` が String ボディの型 `text/plain; charset=utf-8` になります（ボディと型は一緒に置き換えます。`http.with_body` は任意の `Body` を受け付けます）。リクエストのヘッダーは保持します。

```almd run
import http

fn main() -> Unit = {
  let resp = http.set_body(http.response(200, "draft"), "final")
  println(http.body(resp))
}
```
```output
final
```

### `http.set_body_bytes(m, body: Bytes) -> M`

ボディを `Bytes` で置き換えます。現在メッセージボディはテキストとして保持されるため、バイトは非可逆に UTF-8 デコードされます。UTF-8 でないバイト列は U+FFFD になり、保持されません。`Bytes` として持つテキストに使ってください。可逆なバイナリアップロードではありません。

```almd run
import http

fn main() -> Unit = {
  let req = http.set_body_bytes(http.new_request("POST", "/echo", "", []), bytes.from_string("hi"))
  println(http.body(req))
}
```
```output
hi
```

## リクエストの読み取り

`http.new_request(method, target, body, headers)` はコード内でリクエストを作ります。テストからソケットなしでアプリを呼ぶ方法であり、`http.send` / `http.start` に渡すリクエストの構築方法でもあります。その場合、ターゲットは完全な URL です。

```almd run
import http

fn main() -> Unit = {
  let req = http.new_request("POST", "/items?x=1", "hello", [("Content-Type", "text/plain")])
  println(http.method(req))
  println(http.path(req))
  println(http.body(req))
}
```
```output
POST
/items?x=1
hello
```

### `http.method(req: HttpRequest) -> String`

リクエストの HTTP メソッドを送信されたまま取得します（GET、POST など）。

```almd check
import http

fn handle(req: HttpRequest) -> HttpResponse = {
  let method = http.method(req)
  http.response(200, "method: ${method}")
}

effect fn main() -> Unit = {
  http.serve(3000, handle)
}
```

### `http.path(req: HttpRequest) -> String`

クエリ文字列を含むリクエストターゲット（`/search?q=x`）を取得します。送信用に構築したリクエストでは完全な URL です。

```almd check
import http

fn handle(req: HttpRequest) -> HttpResponse = {
  let path = http.path(req)
  http.response(200, "path: ${path}")
}

effect fn main() -> Unit = {
  http.serve(3000, handle)
}
```

リクエストのヘッダーとボディは、上記の共通関数で読みます。

```almd check
import http

fn handle(req: HttpRequest) -> HttpResponse = {
  let body = http.body(req)
  http.response(200, "received ${string.len(body)} chars")
}

effect fn main() -> Unit = {
  http.serve(3000, handle)
}
```

ヘッダー内の認証情報は、存在を探すだけでなく検証します。両側を `secret.new` で包んでください。`Secret[String]` 上の `==` は定数時間です（C-426）。欠落または不一致には 401 を返します。ヘッダーテキストへの通常の `==` は最初に異なるバイトで戻るため、その時間がトークンを漏らします。

```almd check
import env
import http
import secret

effect fn handle(req: HttpRequest) -> HttpResponse = {
  let expected = env.get("API_TOKEN") |> option.map((t) => secret.new("Bearer " + t))
  match (http.header(req, "Authorization"), expected) {
    (some(got), some(want)) => if secret.new(got) == want then http.response(200, "ok")
    else http.response(401, "invalid token"),
    (none, _) => http.response(401, "missing Authorization"),
    (_, none) => http.response(500, "API_TOKEN is not set"),
  }
}

effect fn main() -> Unit = {
  http.serve(3000, handle)
}
```

### `http.query_params(req: HttpRequest) -> Result[List[(String, String)], String]`

`url.parse_query` と同じ読み方のクエリです（C-404、#4117）。ターゲットの最初の `?` より後を `&` で分割し、空の部分を捨て、各組を最初の `=` で分割します（値なしキーの値は `""`）。`+` は空白、`%XX` は厳密な UTF-8 としてデコードします。すべての組を到着順で保持するため、`?tag=a&tag=b` は両方を返します。切り詰められたエスケープ、16 進数でないエスケープ、UTF-8 でないパーセントデータは `err` です（`http.query_params: invalid percent escape` など）。native と wasm で同じです。

```almd check
import http

fn handle(req: HttpRequest) -> HttpResponse = match http.query_params(req) {
  ok(params) => {
    let q = list.find(params, ((k, _)) => k == "q") |> option.map(((_, v)) => v) ?? ""
    http.response(200, "q=${q}")
  },
  err(e) => http.response(400, e),
}

effect fn main() -> Unit = {
  http.serve(3000, handle)
}
```

### フォーム値のデコード

`http` に独自のデコーダーはありません。`http.url_decode` は削除されました。フォームエンコード値（`+` は空白、`%XX` はバイト）は `url` モジュールでデコードします。不正なエスケープや UTF-8 でないパーセントデータを保持せず拒否します。不正な可能性がある場合は入力にフォールバックします。

```almd check
import url

fn main() -> Unit = {
  println(url.decode_form("%E7%8C%AB") ?? "%E7%8C%AB") // "猫"
  println(url.decode_form("a+b%20c") ?? "a+b%20c")
}
```

## クライアント

完全なレスポンスは、404 や 3xx を含めすべて `ok` です。`err` はトランスポートの失敗のみです。リダイレクトには追従しません。3xx とその `Location` ヘッダーが応答です。単発クライアントは 2×2 の組み合わせと呼び出しハンドルで構成されます。`Bytes` としてのボディは、レスポンス全体に `http.body_bytes` を適用して取得します。

| | ボディのテキスト | レスポンス全体 |
|---|---|---|
| GET | `http.get(url)` | `http.get_response(url)` |
| 任意のメソッド、ボディ、ヘッダー | `http.request(method, url, body, headers)` | `http.send(http.new_request(method, url, body, headers))` |
| 制限付きのハンドル | | `http.start(req, limits)` — [呼び出しごとの制限とキャンセル](#per-call-limits-and-cancellation)を参照 |

メソッド別の関数はありません。POST は `http.request("POST", url, body,
headers)` で、ステータスが必要なら `http.send(http.new_request("POST", url, body, headers))` の後に `http.status(resp)` と `http.body(resp)` を使います。ストリーミングボディはハンドル経由で読みます（`http.poll` の間に `http.read_new`）。

### `http.get(url: String) -> Result[String, String]`

HTTP GET リクエストを送り、レスポンスボディを返します。

すべてのクライアント関数はレスポンス全体を読むか、`malformed or incomplete response from "<url>"` を返します（C-408、#4137）。完全なヘッドがないレスポンス、`Content-Length` より短いボディ、終端の 0 チャンクのないチャンク化ボディ、1 MiB を超えるヘッドはエラーであり、不完全な `ok` にはなりません。ブラウザと解釈が分かれ得る箇所では、URL を `url.parse` と同じ方法で分割します（C-402）。バックスラッシュや制御文字、非正規形の数値ホスト（`127.1`、`0x7f.0.0.1`）は拒否し、前後の空白と tab/LF/CR は除去し、`.` / `..` セグメント（`%2e` も）を解決してから送信します。

```almd check
import http

effect fn main() -> Unit = {
  let html = http.get("https://example.com")!
  println(html)
}
```

### `http.request(method: String, url: String, body: String, headers: List[(String, String)]) -> Result[String, String]`

任意のメソッド、ボディ、ヘッダーでリクエストを送り、レスポンスボディを返します。ボディなしには `""`、追加ヘッダーなしには `[]` を渡します。

```almd check
import http

effect fn main() -> Unit = {
  let url = "https://api.example.com/items/1"
  let body = '{"name": "alice"}'
  let headers = [("Content-Type", "application/json"), ("User-Agent", "my-app")]
  println(http.request("PUT", url, body, headers)!)
  println(http.request("DELETE", url, "", [])!)
}
```

#### 読み取りタイムアウト（`ALMIDE_HTTP_TIMEOUT_SECS`）

制限を受け取らない各クライアント呼び出しは、サーバーの応答を最大 **30 秒**待ちます。環境変数 `ALMIDE_HTTP_TIMEOUT_SECS` は全呼び出しのこのデフォルトを上書きします。`0` は**タイムアウトなし**で、サーバーが応答するまでブロックします。これはデフォルトにすぎず、独自の制限を持つ呼び出し（`http.start`）は無視します。[呼び出しごとの制限とキャンセル](#per-call-limits-and-cancellation)を参照してください。制限は呼び出し全体ではなくバイト間に適用され、送信し続けるサーバーでは発動しません。遅いエンドポイント（長いプロンプトを評価するローカル LLM は最初のバイトまで普通に 30–120 s かかる）は、制限を超えると次のように失敗します。

```
timed out waiting for "http://localhost:8080/v1/chat" (raise ALMIDE_HTTP_TIMEOUT_SECS; 0 = no timeout)
```

```sh
ALMIDE_HTTP_TIMEOUT_SECS=300 ./app   # five minutes
ALMIDE_HTTP_TIMEOUT_SECS=0 ./app     # wait forever
```

同じ変数が**接続**にも制限を設けます（#2825）。応答のない接続試行は OS のデフォルト（macOS では 75 s、Linux ではさらに長い）ではなく、30 s（Go の dialer のデフォルト）で同じテキストを返して断念します。

#### 経路ごとのクライアント制限

| | 接続 | 読み取り間 | 呼び出し全体 | レスポンスサイズ |
|---|---|---|---|---|
| `get` / `request` / `get_response` / `send` | 30 s | 30 s | なし | 1 GiB |
| `http.start` | `total_ms` | `idle_ms` | `total_ms` | なし |

- 30 s のデフォルトは `ALMIDE_HTTP_TIMEOUT_SECS` で指定します（`0` = なし）。
- サイズ上限は受信したままのレスポンス（ヘッドとフレーミング付きボディ）を数えます。`ALMIDE_HTTP_MAX_RESPONSE_BYTES` で上書きし、`0` = 上限なしです。超えると呼び出しは `err("response from \"<url>\" is larger than N bytes (raise ALMIDE_HTTP_MAX_RESPONSE_BYTES; 0 = no limit)")` になります。
- すべての経路で 1 MiB を超えるレスポンス HEAD を拒否します（Go のデフォルト）。
- 呼び出し全体の期限はデフォルトではありません。curl、Go、Python、Node、reqwest のいずれにもありません。必要なら `http.start` と `total_ms` で指定します。

#### エラーテキスト（ADR-0023 §4.2）

すべてのクライアント失敗はクラス分けされ、各クラスには 1 つのテキストがあります。テキストはプログラムが渡した URL を引用して示し（`\"`、`\\`、`\t`、`\r`、`\n`、`\0`、`\u{..}` エスケープ）、OS のエラー番号は含めません。そのため同じ失敗は native、`almide run --target wasm`、標準ランタイム上の WASI 0.3 コンポーネント（`wasmtime run -S http=y`）で同じ表示になります。

userinfo のパスワードは表示しません。最初の `:` から `@` までのすべてを、空のパスワードも含め `***` にします。Go の net/http と同じ表示です（`http://alice:s3cret@host/p` は `"http://alice:***@host/p"`）。ユーザー名、ホスト、ポート、パス、クエリ、フラグメントは書かれたまま引用されるため、クエリ文字列内の認証情報はテキストに出ます。代わりにヘッダーで認証情報を送ってください（CWE-598）。

| クラス | テキスト |
|---|---|
| invalid-url | `invalid URL "<url>": <what is wrong>` |
| refused-header | `invalid header name "<name>": …` / `invalid header value for "<name>": …` / `forbidden header name "<name>": …` |
| dns | `cannot resolve the host of "<url>"` |
| connect | `could not connect to "<url>" (connection refused or unreachable)` |
| timeout | `timed out waiting for "<url>" (raise ALMIDE_HTTP_TIMEOUT_SECS; 0 = no timeout)` |
| tls | `TLS handshake with "<url>" failed` |
| too-large | `response from "<url>" is larger than <n> bytes (raise ALMIDE_HTTP_MAX_RESPONSE_BYTES; 0 = no limit)` |
| protocol | `malformed or incomplete response from "<url>"` |

プロキシ自体の失敗と読み込めないトラストストアは、固有のテキストを維持します（プロキシや変数の名前を示します）。コンポーネントでは、これらのクラス外のホストエラーは `http request to "<url>" failed (<wasi:http case>)` と表示し、ホストの `internal-error` テキストはそのまま渡します。コンポーネントは自身の環境（`wasmtime run --env …`）から `ALMIDE_HTTP_TIMEOUT_SECS` と `ALMIDE_HTTP_MAX_RESPONSE_BYTES` を読みます。そこではタイムアウト `0` はホストのデフォルトを維持します。URL の理由のうち、スキームの 3 種（欠落、未サポート、`expected "//"`）はコンポーネント自身が見つけます。不正なホストやポートはランタイムが拒否し、未分類のテキストで表示します。

#### URL（#2821）

URL は curl、Go、WHATWG URL 標準と同じ方法で分割します。

- スキームは大文字・小文字を区別せず、`http://` または `https://` が必須です。欠落や別のスキームはエラーで、推測で平文 HTTP として送ることはありません。
- 独自の `Authorization` ヘッダーを渡さない限り、`user:password@` は `Authorization: Basic …` になります。userinfo をホストと解釈しません。WASI 0.3 コンポーネントも同じフィールド値を送ります。
- IPv6 ホストは角括弧で囲みます：`http://[::1]:8080/`。
- ポートは 1–65535 です。読めないポートはエラーで、黙って 80 にしません。
- 国際化ホストは punycode で送ります（`bücher.example` → `xn--bcher-kva.example`）。
- `http://host:8080?x=1` はポート 8080 の `GET /?x=1` です。
- `#fragment` は送りません。
- パスとクエリ内の空白、制御文字、非 ASCII 文字はパーセントエンコードします。
- ポートがスキームのデフォルトでなければ、`Host` にポートを含めます。

拒否された URL は `invalid URL "…": <what is wrong>` という `err` になり、接続試行前に返します。`http.start` はこれを同期的に返します。

#### ヘッダーインジェクション（#2822）

ヘッダー名はトークンでなければなりません（RFC 9110）。値には CR、LF、NUL、その他の制御文字を含められません（HTAB は可）。メソッドもトークンが必要です。クライアントまたはランタイムが管理するフィールド、`connection`、`keep-alive`、`proxy-authenticate`、`proxy-authorization`、`proxy-connection`、`transfer-encoding`、`upgrade`、`host`、`http2-settings` は、大文字・小文字を問わず設定できません（C-370、標準 WASI ランタイムも同じ 9 個を拒否）。違反すると送信前にリクエストを拒否し、呼び出しは `invalid header value for "X-A": …`、`invalid header name …`、`forbidden header name "Host": …`、または `invalid HTTP method …` で始まる `err` になります。不正箇所を除去してそのまま送ることはありません。Python、Go、Node も同じ入力を拒否します。

#### プロキシ（#2819）

すべてのクライアント経路（`get`、`request`、`get_response`、`send`、`http.start`）は curl、reqwest と同じようにプロキシ変数を読みます。

- `https://` URL は `HTTPS_PROXY` / `https_proxy` を使います。
- `http://` URL は `HTTP_PROXY` / `http_proxy` を使います。Go、reqwest と同様、`REQUEST_METHOD` が設定されていると `HTTP_PROXY` は無視します（CGI、httpoxy）。
- どちらもその後 `ALL_PROXY` / `all_proxy` にフォールバックします。
- `NO_PROXY` / `no_proxy` はコンマ区切りのリストです。`*` は全ホストでプロキシを迂回します。IP または CIDR ブロック（`10.0.0.0/8`）はアドレス形式のホストに一致します。名前（`example.com`、`.example.com`）は自身とそのサブドメインに一致します。デフォルトで除外されるものはなく、`localhost` も同様です（curl、reqwest と同じ）。

プロキシのスキーム：

- `http://` プロキシ。スキームなしは `http://` とみなします。`https://` ターゲットは `CONNECT host:port` でトンネル化し、オリジンとの TLS はトンネル内で動きます。`http://` ターゲットは絶対形式でプロキシに送ります。
- `socks5://`（名前はローカルで解決）と `socks5h://`（プロキシが解決）。デフォルトポートは 1080 です。

プロキシ URL の `user:password@` はプロキシへのログインです。HTTP プロキシでは `Proxy-Authorization:
Basic`、SOCKS5 では RFC 1929 を使います。それ以外のプロキシスキームは、変数名を示すエラーです。黙って直接接続しません。エラーはプロキシ名を示し（`proxy 127.0.0.1:3128 (from HTTPS_PROXY) refused CONNECT
api.example.com:443: HTTP/1.1 407 …`）、その認証情報を再掲しません。

#### TLS の信頼（#2820）

HTTPS は 2 種類のルートを信頼します。

- 同梱の webpki（Mozilla）ルート。
- `rustls-native-certs` で読むプラットフォームのトラストストア（macOS のキーチェーン、Windows のストア、Linux の OpenSSL バンドル / ディレクトリ）。`SSL_CERT_FILE` または `SSL_CERT_DIR` が設定されていると、それらが指す PEM 証明書をプラットフォームストアの代わりに使います。

そのため curl、Python、Go と同様に、企業 CA、テスト CA、TLS を検査するプロキシの CA を信頼できます。変数が読み込める証明書を指していなければ、エラーでその旨を示します（`TLS error: could not load the CA
certificates SSL_CERT_FILE names: …`）。証明書またはハンドシェイクの失敗は、書き込み失敗ではなく tls クラスとして表示されます。

```
TLS handshake with "https://internal.example/" failed
```

#### 中間レスポンスとチャンク拡張（#2824）

- 最終レスポンスより前の `1xx` レスポンス（`100 Continue`、`103 Early Hints`）は、全経路で読み飛ばします。`101` は最終です。
- チャンクサイズ行の `;extension` は無視します。サイズ行が 16 進数でなければ、空ボディではなくエラーです（protocol クラス、`malformed or incomplete response from
  "<url>"`）。

### レスポンス全体 — `get_response` と `send`

`http.get_response(url)` と `http.send(req)` はボディテキストではなく `HttpResponse` 全体を返します。どちらもランタイム内で `get` / `request` と同じ通信から得られるため、形がずれることはありません。`get` は `get_response` の `body` で、`request(method, url, body, headers)` は `send(new_request(method, url, body, headers))` の `body` です。

レスポンスに含まれるもの：

- `http.status(resp)` — 完全なレスポンスは**すべて** `ok` で、404 と 3xx も含みます。`err` はトランスポートの失敗（接続 / TLS / タイムアウト）のみです。
- **リダイレクトには追従しません。** 3xx は `Location` ヘッダーとともにそのまま届くので、レスポンスは常に渡した URL に対するもので、別の「最終 URL」はありません。
- ヘッダーは通信上の綴りを保ち、重複フィールドは**すべての**出現を保持します。`http.header(resp, k)` は最初、`http.header_values(resp, k)` はすべて、`http.headers(resp)` はすべての `(name, value)` の組を通信順で返します。
- `http.body(resp)` — 転送デコード済みボディテキスト、非可逆な UTF-8。

wasm では `almide run --target wasm` と WASI 0.3 コンポーネント（`ALMIDE_COMPONENT_P3=1 --component`、wasmtime 46+ で `-S http` を付けて実行）がレスポンス全体のクライアントを提供します。デフォルトの標準成果物はビルド時に拒否します。`proofs/target-availability.toml` に各系統の宣言があります。

### `http.get_response(url: String) -> Result[HttpResponse, String]`

```almd check
import http

effect fn main() -> Unit = {
  let resp = http.get_response("https://example.com/")!
  println("status ${http.status(resp)}")
  println(http.header(resp, "strict-transport-security") ?? "no HSTS")
  println(http.header(resp, "x-frame-options") ?? "no X-Frame-Options")
}
```

### `http.send(req: HttpRequest) -> Result[HttpResponse, String]`

`http.new_request` で構築した、ターゲットが完全 URL のリクエストを送り、どのステータスでもレスポンス全体を返します。リダイレクトには追従しないため、リダイレクト検査ではレスポンスから 3xx とその `Location` を直接読みます。

```almd check
import http

effect fn main() -> Unit = {
  let req = http.new_request("GET", "http://example.com/old", "", [("User-Agent", "checker")])
  let resp = http.send(req)!
  let code = http.status(resp)
  if code >= 300 and code < 400 then
    println("redirects to ${http.header(resp, "location") ?? "?"}")
  else
    println("answers ${code} directly")
}
```

任意のメソッドを使用できます。繰り返しフィールドの全値は `header_values` で読みます。

```almd check
import http

effect fn main() -> Unit = {
  let req = http.new_request("POST", "https://api.example.com/login", '{"user": "alice"}', [("Content-Type", "application/json")])
  let resp = http.send(req)!
  for cookie in http.header_values(resp, "set-cookie") {
    println(cookie)
  }
}
```

リクエストは値なので、送信前に組み立てられます。

```almd check
import http

effect fn main() -> Unit = {
  let empty = http.new_request("PATCH", "https://api.example.com/items/1", "", [])
  let req = http.set_body(http.set_header(empty, "Content-Type", "application/json"), '{"name": "bob"}')
  let resp = http.send(req)!
  println("${http.status(resp)} ${http.body(resp)}")
}
```

<a id="per-call-limits-and-cancellation"></a>

## 呼び出しごとの制限とキャンセル

呼び出しが終わるまでブロックする代わりに、ハンドル `HttpCall` として保持できます。`http.start` は直ちに戻ります。ランタイムスレッドが通信する間、プログラムはキー読み取り、再描画、2 件目のリクエスト開始など、自身のループを動かし続けます。

| | |
|---|---|
| `http.start(req, limits) -> Result[HttpCall, String]` | `req`（`http.new_request` で構築、ターゲットは完全 URL）の送信を始め、直ちに戻る |
| `http.poll(c) -> Option[Result[HttpResponse, String]]` | ブロックしない。実行中は `none`、終了後は結果 |
| `http.read_new(c) -> String` | ブロックしない。前回の `read_new` 以降に届いたボディテキスト |
| `http.wait(c) -> Result[HttpResponse, String]` | 呼び出し終了までブロックするが、制限は超えない |
| `http.cancel(c) -> Unit` | 接続を閉じ、呼び出しを `err("request cancelled")` で終了 |

**ファミリーの規則**（`tests/http_call_family_gate_test.rs` が機械的に検査）：ハンドルが持つ操作は、この 5 つだけです。ストリーミングボディは `poll` の間に `read_new` で読みます。コールバック型クライアントはありません。

**制限は呼び出しごとです。** `HttpLimits = { total_ms: Int, idle_ms: Int }` はともにミリ秒単位で、`0` = 制限なしです。

- `total_ms` は `http.start` から始まる実時間です。接続、最初のバイト待ち、ボディをすべて数えます。送信し続けるサーバーでも期限で停止します。ランタイムのスレッドと、各 `poll` / `read_new` / `wait` が検査します。
- `idle_ms` はバイト到着間の最大間隔です。最初のバイト待ちも間隔に数えます。

制限が発動すると、その名前を示すエラーで呼び出しを終了します。

```
request timeout: total_ms 5000 exceeded
request timeout: idle_ms 60000 exceeded
```

`ALMIDE_HTTP_TIMEOUT_SECS` は制限付きの呼び出しには適用されず、制限を取らない呼び出しのデフォルトのままです。したがって「この LLM 呼び出しは 30 分、このページ取得は 10 秒まで」という指定は、1 プロセス内で 2 種類の制限を持つ 2 つの `start` 呼び出しになります。

**呼び出しの終了で接続を閉じます。** 応答、失敗、タイムアウト、キャンセルのいずれで終了しても、ランタイムはソケットをシャットダウンするため、サーバーは直ちに切断を認識し、それ以上届きません。`cancel` 後、`read_new` は `""` を返し（到着済みでも未読のバイトは破棄）、`poll` / `wait` は `err("request cancelled")` を返します。終了済みの呼び出しへの `cancel` は何も変えず、2 回呼んでもかまいません。ハンドルの最後のコピーを破棄してもキャンセルされます。

**戻るのはレスポンス全体です。** `wait` と `poll` は `http.send` と同様、404 を含む完全なレスポンスをすべて `ok` とし、ステータス、すべてのヘッダー行、ボディ全体を返します。`err` はトランスポート失敗、制限の発動、キャンセルです。`read_new` が渡したボディバイトも、そのレスポンスのボディに残っています。2 回の読み取りにまたがるマルチバイト文字は、全体が揃うまで保留します。

```almd check
import env
import http
import io

// Print the stream as it arrives; stop after 3 s whatever the server does.
effect fn follow(c: HttpCall, started: Int) -> Result[String, String] = {
  io.print(http.read_new(c))
  match http.poll(c) {
    some(ok(resp)) => ok("done: ${http.status(resp)}"),
    some(err(e)) => ok(e),
    none => if datetime.now_ms() - started > 3000 then {
      http.cancel(c)
      ok("stopped")
    } else {
      env.sleep_ms(100)
      follow(c, started)
    },
  }
}

effect fn main() -> Unit = {
  let limits = { total_ms: 30 * 60 * 1000, idle_ms: 120 * 1000 }
  let c = http.start(http.new_request("GET", "http://127.0.0.1:8080/events", "", []), limits)!
  println(follow(c, datetime.now_ms())!)
}
```

ストリーミング POST（server-sent events を送る LLM エンドポイント）でも、ボディとヘッダーを持つリクエストを使う同じループです。

```almd check
import http

effect fn main() -> Unit = {
  let body = """{"model": "m", "stream": true, "messages": []}"""
  let req = http.new_request("POST", "http://127.0.0.1:8080/v1/chat/completions", body, [("Content-Type", "application/json")])
  let c = http.start(req, { total_ms: 10 * 60 * 1000, idle_ms: 2 * 60 * 1000 })!
  let resp = http.wait(c)!
  println("${http.status(resp)}")
  println(http.body(resp))
}
```

（OpenAI と Anthropic のストリーミングクライアントは dialect epoch 14 で [almide/llm](https://github.com/almide/llm) パッケージに移りました。`almide add almide/llm@v0.1.0`、`import llm`、その後 `llm.openai_streaming_call(...)` を使います。）

wasm ターゲットでは `almide run --target wasm` が `start`、`poll`、`read_new`、`wait`、`cancel` を native と同じ動作で提供します。埋め込みホストが native の呼び出しコアを実行するため、制限とキャンセルのエラーテキストは同じで、最後のハンドルコピーの破棄も呼び出しをキャンセルします。`almide build --target wasm` で作る単独の `.wasm` は、まだこれらを扱えません（実行中呼び出し用の標準 WASI ホストがないため）。そのためビルド経路を検査する `almide check --target wasm` は、引き続きこれらを拒否します。


## 固定スナップショットの API 索引（36 関数）

```
// Serves 0.0.0.0:port forever; handler err is a 500.
// @since 0.5.0 or earlier
effect http.serve(port: Int, handler: HttpHandler) -> Unit

// Reply with body; its type gives the Content-Type (String: text/plain; charset=utf-8).
// @since 0.5.0 or earlier
http.response(status: Int, body: B) -> HttpResponse

// Reply with Content-Type application/json; body is JSON text.
// @since 0.5.0 or earlier
http.json(status: Int, body: String) -> HttpResponse

// Reply with body and these headers in order after its Content-Type; a repeated name keeps its first spelling and last value.
// @since 0.5.0 or earlier
http.with_headers(status: Int, body: B, headers: List[(String, String)]) -> HttpResponse

// resp with its body replaced by body and its Content-Type by body's type.
// @since unreleased
http.with_body(resp: HttpResponse, body: B) -> HttpResponse

// A body of media type media_type; a literal, never a script-capable or stdlib-owned one.
// @since unreleased
http.typed(media_type: String, body: Bytes) -> TypedBody

// 302 with Location url, an empty body and Content-Type text/plain; charset=utf-8.
// @since 0.6.0 or earlier
http.redirect(url: String) -> HttpResponse

// resp with code set; not range-checked.
// @since unreleased
http.set_status(resp: HttpResponse, code: Int) -> HttpResponse

// HTTP status code; 0 if the status line was bad.
// @since 0.6.0 or earlier
http.status(resp: HttpResponse) -> Int

// First value of header name, any case; none if absent.
// @since unreleased
http.header(m: M, name: String) -> Option[String]

// Every value of header name, any case, in wire order; [] if absent.
// @since 0.62.0
http.header_values(m: M, name: String) -> List[String]

// Every header as (name, value), wire spelling and order, repeats kept.
// @since 0.62.0
http.headers(m: M) -> List[(String, String)]

// Body text; a fetched body is decoded as lossy UTF-8.
// @since 0.6.0 or earlier
http.body(m: M) -> String

// Body as bytes: a fetched body exactly as sent, a built one as the UTF-8 bytes of body(m).
// @since unreleased
http.body_bytes(m: M) -> Bytes

// m with header name set, any case: the first match replaced in place, else appended; a response refuses Content-Type.
// @since 0.6.0 or earlier
http.set_header(m: M, name: String, value: String) -> M

// m with its body replaced; a response's Content-Type becomes text/plain; charset=utf-8.
// @since unreleased
http.set_body(m: M, body: String) -> M

// m with its body replaced by body, decoded as lossy UTF-8; a response's Content-Type becomes application/octet-stream.
// @since unreleased
http.set_body_bytes(m: M, body: Bytes) -> M

// Request method as sent, e.g. GET.
// @since unreleased
http.method(req: HttpRequest) -> String

// Request target, query string included (a full URL for a request built to send).
// @since unreleased
http.path(req: HttpRequest) -> String

// url.parse_query of the target after its first ?; pairs in order; err on a bad escape.
// @since 0.6.0 or earlier
http.query_params(req: HttpRequest) -> Result[List[(String, String)], String]

// Body even for a 404; err only on transport.
// @since 0.5.0 or earlier
effect http.get(url: String) -> String

// Reply body for any method; any status is ok.
// @since 0.5.0 or earlier
effect http.request(method: String, url: String, body: String, headers: List[(String, String)]) -> String

// Whole reply, 404 included; redirects not followed.
// @since 0.62.0
effect http.get_response(url: String) -> HttpResponse

// Sends req to the URL its target names; the whole reply, any status; redirects not followed.
// @since unreleased
effect http.send(req: HttpRequest) -> HttpResponse

// Begins sending req; returns at once with a handle.
// @since unreleased
effect http.start(req: HttpRequest, limits: HttpLimits) -> HttpCall

// Never blocks: none while running, then the result.
// @since unreleased
effect http.poll(c: HttpCall) -> Option[Result[HttpResponse, String]]

// Body text since the last read_new; never blocks.
// @since unreleased
effect http.read_new(c: HttpCall) -> String

// Blocks until the call ends; any status is ok.
// @since unreleased
effect http.wait(c: HttpCall) -> HttpResponse

// Closes the connection; the call ends as err.
// @since unreleased
effect http.cancel(c: HttpCall) -> Unit

// Request built in code; target keeps its ?query (a full URL for send / start).
// @since unreleased
http.new_request(method: String, target: String, body: String, headers: List[(String, String)]) -> HttpRequest

// Router-bound path parameter, decoded; none if unbound.
// @since unreleased
http.param(req: HttpRequest, name: String) -> Option[String]

// "GET /users/{id}" to a route; no method = any.
// @since unreleased
http.route(pattern: String, handler: HttpHandler) -> HttpRoute

// Sub-app under prefix: /api/x reaches it as /x.
// @since unreleased
http.mount(prefix: String, sub: HttpHandler) -> HttpRoute

// Route table as a handler; err on conflicting routes.
// @since unreleased
http.router(xs: List[HttpRoute]) -> Result[HttpHandler, String]

// Applies middleware; the first is the outermost.
// @since unreleased
http.wrap(handler: HttpHandler, middleware: List[HttpMiddleware]) -> HttpHandler

// JSON body into input type T, strictly; err is a 400 response.
// @since unreleased
http.decode_json(req: HttpRequest) -> Result[T, HttpResponse]
```

## 固定スナップショットの型索引（4 型）

```
// Per-call limits in ms; 0 = no limit.
// @since unreleased
type http.HttpLimits = { total_ms: Int, idle_ms: Int }

// Request to response; a router is one too.
// @since unreleased
type http.HttpHandler = (HttpRequest) -> Result[HttpResponse, String]

// Wraps a handler: (next) => (req) => … next(req)! ….
// @since unreleased
type http.HttpMiddleware = (HttpHandler) -> HttpHandler

// Route table row; build it with route or mount.
// @since unreleased
type http.HttpRoute = { method: String, pattern: String, segments: List[String], handler: HttpHandler }
```



## API・型の説明の訳（上記索引順）

- `effect http.serve(port: Int, handler: HttpHandler) -> Unit` — 0.0.0.0:port で継続的に提供する。ハンドラーの err は 500 になる。
- `http.response(status: Int, body: B) -> HttpResponse` — ボディ付きで応答する。その型が Content-Type を決める（String: text/plain; charset=utf-8）。
- `http.json(status: Int, body: String) -> HttpResponse` — Content-Type application/json で応答する。ボディは JSON テキスト。
- `http.with_headers(status: Int, body: B, headers: List[(String, String)]) -> HttpResponse` — ボディと、Content-Type の後に指定順のヘッダーを付けて応答する。同名の繰り返しでは最初の綴りと最後の値を保持する。
- `http.with_body(resp: HttpResponse, body: B) -> HttpResponse` — resp のボディを body に、Content-Type を body の型に置き換える。
- `http.typed(media_type: String, body: Bytes) -> TypedBody` — メディア型 media_type のボディ。リテラルであり、スクリプト実行可能な型や標準ライブラリが所有する型ではない。
- `http.redirect(url: String) -> HttpResponse` — Location が url、空のボディ、Content-Type text/plain; charset=utf-8 を持つ 302。
- `http.set_status(resp: HttpResponse, code: Int) -> HttpResponse` — resp に code を設定する。範囲検査はしない。
- `http.status(resp: HttpResponse) -> Int` — HTTP ステータスコード。ステータス行が不正なら 0。
- `http.header(m: M, name: String) -> Option[String]` — 大文字・小文字を問わずヘッダー name の最初の値。なければ none。
- `http.header_values(m: M, name: String) -> List[String]` — 大文字・小文字を問わずヘッダー name の全値を通信順に返す。なければ []。
- `http.headers(m: M) -> List[(String, String)]` — 各ヘッダーを (name, value) として返す。通信上の綴りと順序、重複を保持する。
- `http.body(m: M) -> String` — ボディのテキスト。取得したボディは非可逆に UTF-8 デコードする。
- `http.body_bytes(m: M) -> Bytes` — ボディのバイト。取得したボディは送信どおり、構築したボディは body(m) の UTF-8 バイト。
- `http.set_header(m: M, name: String, value: String) -> M` — m にヘッダー name を大文字・小文字を問わず設定する。最初の一致をその場で置換し、なければ追加。レスポンスでは Content-Type を拒否する。
- `http.set_body(m: M, body: String) -> M` — m のボディを置換する。レスポンスの Content-Type は text/plain; charset=utf-8 になる。
- `http.set_body_bytes(m: M, body: Bytes) -> M` — m のボディを、非可逆な UTF-8 でデコードした body に置換する。レスポンスの Content-Type は application/octet-stream になる。
- `http.method(req: HttpRequest) -> String` — 送信されたままのリクエストメソッド。例：GET。
- `http.path(req: HttpRequest) -> String` — クエリ文字列を含むリクエストターゲット（送信用に構築したリクエストでは完全 URL）。
- `http.query_params(req: HttpRequest) -> Result[List[(String, String)], String]` — ターゲットの最初の ? 以降に url.parse_query を適用。組は順序を保持。不正なエスケープは err。
- `effect http.get(url: String) -> String` — 404 でもボディを返す。err はトランスポートの失敗のみ。
- `effect http.request(method: String, url: String, body: String, headers: List[(String, String)]) -> String` — 任意のメソッドに対する応答ボディ。どのステータスでも ok。
- `effect http.get_response(url: String) -> HttpResponse` — 404 を含む応答全体。リダイレクトには追従しない。
- `effect http.send(req: HttpRequest) -> HttpResponse` — req をターゲットが示す URL に送る。どのステータスでも応答全体を返し、リダイレクトには追従しない。
- `effect http.start(req: HttpRequest, limits: HttpLimits) -> HttpCall` — req の送信を開始し、ハンドルを返して直ちに戻る。
- `effect http.poll(c: HttpCall) -> Option[Result[HttpResponse, String]]` — ブロックしない。実行中は none、その後は結果。
- `effect http.read_new(c: HttpCall) -> String` — 前回の read_new 以降のボディテキスト。ブロックしない。
- `effect http.wait(c: HttpCall) -> HttpResponse` — 呼び出し終了までブロックする。どのステータスでも ok。
- `effect http.cancel(c: HttpCall) -> Unit` — 接続を閉じる。呼び出しは err で終了する。
- `http.new_request(method: String, target: String, body: String, headers: List[(String, String)]) -> HttpRequest` — コード内で構築するリクエスト。ターゲットは ?query を保持する（send / start では完全 URL）。
- `http.param(req: HttpRequest, name: String) -> Option[String]` — ルーターがバインドした、デコード済みパスパラメーター。未バインドなら none。
- `http.route(pattern: String, handler: HttpHandler) -> HttpRoute` — "GET /users/{id}" からルートを作る。メソッドなしなら任意。
- `http.mount(prefix: String, sub: HttpHandler) -> HttpRoute` — プレフィックス配下のサブアプリ。/api/x は /x として届く。
- `http.router(xs: List[HttpRoute]) -> Result[HttpHandler, String]` — ルートテーブルをハンドラーにする。ルート競合は err。
- `http.wrap(handler: HttpHandler, middleware: List[HttpMiddleware]) -> HttpHandler` — ミドルウェアを適用する。先頭が最も外側。
- `http.decode_json(req: HttpRequest) -> Result[T, HttpResponse]` — JSON ボディを入力型 T に厳密にデコードする。err は 400 レスポンス。
- `type http.HttpLimits = { total_ms: Int, idle_ms: Int }` — 呼び出しごとの制限（ms）。0 = 制限なし。
- `type http.HttpHandler = (HttpRequest) -> Result[HttpResponse, String]` — リクエストからレスポンスへ。ルーターもこの型。
- `type http.HttpMiddleware = (HttpHandler) -> HttpHandler` — ハンドラーを包む：(next) => (req) => … next(req)! …。
- `type http.HttpRoute = { method: String, pattern: String, segments: List[String], handler: HttpHandler }` — ルートテーブルの行。route または mount で構築する。
