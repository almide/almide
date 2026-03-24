# Web Framework [ON HOLD]

## Vision

Almide の first-party web framework。Hono 相当の DX を Almide の思想で実現する。

stdlib の `http` primitive の上に乗る薄いレイヤー。external library として提供するが、template / Codec との統合は Almide ならでは。

## Design Principles

1. **Immutable** — mutable context なし。`Request -> Response` の pure な変換
2. **Pipe chain で組む** — Almide の自然な合成手段
3. **Template 統合** — `HtmlDoc` をそのまま返せる
4. **Codec 統合** — `deriving Codec` で request/response の JSON を型安全に
5. **Multi-target** — Rust / TS 両方で動く
6. **薄い** — routing + middleware + response builder。それ以上は入れない

## API Design

### Basic

```almide
import web

let app = web.new()
  |> web.get("/", fn(req) => web.text("Hello"))
  |> web.get("/about", fn(req) => web.text("About"))

effect fn main() -> Result[Unit, String] = {
  web.serve(app, 3000)
}
```

### Routing

```almide
let app = web.new()
  |> web.get("/users/:id", fn(req) => {
    let id = req.param("id")              // UFCS (requires ufcs-external)
    web.json(json.object([("id", json.s(id))]))
  })
  |> web.post("/users", fn(req) => {
    web.json_status(201, req.body)
  })
  |> web.get("/files/*", fn(req) => {
    web.text("File: ${req.wildcard()}")    // UFCS
  })
```

### Template Integration

```almide
import web

type User = { name: String, email: String }

template user_page(user: User) -> HtmlDoc = html {
  head { title { user.name } }
  body {
    h1 { user.name }
    p { user.email }
  }
}

let app = web.new()
  |> web.get("/users/:id", fn(req) => {
    let user = find_user(req.param("id"))
    web.html(user_page(user))
  })
```

`web.html(HtmlDoc) -> Response` は内部で `render(doc)` して `Content-Type: text/html` を付ける。

### Codec Integration (deriving Codec 実装後)

```almide
type CreateUser = { name: String, email: String } deriving Codec
type UserResponse = { id: String, name: String, email: String } deriving Codec

let app = web.new()
  |> web.post("/users", fn(req) => {
    let input = req.body_json[CreateUser]()
    let user = create_user(input)
    web.json(user)                            // auto encode via Codec
  })
```

### Middleware

関数合成。`Handler -> Handler`。

```almide
fn logger(next: web.Handler) -> web.Handler =
  fn(req) => {
    io.println("${req.method} ${req.path}")
    next(req)
  }

fn cors(origin: String) -> fn(web.Handler) -> web.Handler =
  fn(next) => fn(req) => {
    let res = next(req)
    match res {
      ok(r) => ok(r.add_header("Access-Control-Allow-Origin", origin)),
      err(e) => err(e),
    }
  }

let app = web.new()
  |> web.use(logger)
  |> web.use(cors("*"))
  |> web.get("/", fn(req) => web.text("Hello"))
```

### Route Grouping

```almide
let api = web.group("/api")
  |> web.get("/users", list_users)
  |> web.post("/users", create_user)

let pages = web.group("/")
  |> web.get("/", home_page)
  |> web.get("/about", about_page)

let app = web.new()
  |> web.use(logger)
  |> web.mount(api)
  |> web.mount(pages)
```

### Error Handling

```almide
let app = web.new()
  |> web.get("/", fn(req) => web.text("Home"))
  |> web.on_error(fn(err, req) => {
    io.println("Error: ${err}")
    web.text_status(500, "Internal Server Error")
  })
  |> web.on_not_found(fn(req) =>
    web.html_status(404, error_page("Not Found", req.path))
  )
```

## API Summary

```
// App lifecycle
web.new            : () -> App
web.serve          : (App, Int) -> Unit                     // effect

// Routing
web.get            : (App, String, Handler) -> App
web.post           : (App, String, Handler) -> App
web.put            : (App, String, Handler) -> App
web.delete         : (App, String, Handler) -> App

// Grouping
web.group          : (String) -> App
web.mount          : (App, App) -> App

// Middleware
web.use            : (App, fn(Handler) -> Handler) -> App

// Request (UFCS-able)
req.param          : (Request, String) -> String
req.query          : (Request, String) -> Option[String]
req.wildcard       : (Request) -> String
req.body_json[T]   : (Request) -> Result[T, String]         // Codec integration

// Response builders
web.text           : (String) -> Response
web.text_status    : (Int, String) -> Response
web.json           : (Json) -> Response                      // status 200 default
web.json_status    : (Int, Json) -> Response
web.html           : (HtmlDoc) -> Response                   // template integration
web.html_status    : (Int, HtmlDoc) -> Response
web.redirect       : (String) -> Response
web.redirect_status: (Int, String) -> Response

// Response mutation (UFCS-able)
res.add_header     : (Response, String, String) -> Response
res.status         : (Response, Int) -> Response

// Error handling
web.on_error       : (App, fn(String, Request) -> Response) -> App
web.on_not_found   : (App, fn(Request) -> Response) -> App
```

## Hono との比較

| 観点 | Hono | Almide web |
|------|------|-----------|
| Context | mutable `c` object | immutable `req` → `Response` |
| Middleware | `c.next()` onion model | `Handler -> Handler` 関数合成 |
| Response | `c.text()`, `c.json()` | `web.text()`, `web.json()` (pure fn) |
| HTML | `c.html(string)` | `web.html(HtmlDoc)` — typed document |
| JSON | object literal | `deriving Codec` で型安全 |
| Routing | `.get("/path", handler)` | `\|> web.get("/path", handler)` |
| Method access | `c.req.param()` | `req.param()` (UFCS) |

## DX Gap: 何が揃えば Hono 同等か

| 優先度 | 機能 | roadmap | 効果 |
|--------|------|---------|------|
| **1** | `deriving Codec` | codec-and-json.md Phase 2 | JSON の冗長性が消える |
| **2** | UFCS for external libs | ufcs-external.md | `req.param("id")` が書ける |
| **3** | Lambda 短縮構文 | syntax-sugar.md | `fn(req) =>` → 短縮形 |
| **4** | Default params | syntax-sugar.md | `web.json(data)` で status 省略 |

全部揃った場合:

```almide
let app = web.new()
  |> get("/", (req) => text("Hello"))
  |> get("/users/:id", (req) => {
    let id = req.param("id")
    json(UserResponse { id: id, name: "Alice" })
  })
  |> post("/users", (req) => {
    let input = req.body_json[CreateUser]()
    json_status(201, create_user(input))
  })
```

## merjs (Zig) から学ぶべきパターン

[merjs](https://github.com/justrach/merjs) は Next.js 相当をZig単一バイナリ（260KB）で実現。以下のパターンはAlmideに取り込む価値がある:

### ファイルベースルーティング

ディレクトリ構造からルートテーブルを自動生成。

```
app/
  index.almd          → /
  about.almd          → /about
  users/
    index.almd        → /users
    [id].almd         → /users/:id
api/
  users.almd          → /api/users (JSON API)
```

Almideではコンパイラ拡張として実現可能 — `almide build --web app/` がディレクトリを走査してルートテーブルをIRレベルで生成。別途codegenバイナリ不要。

### HTML DSL

```almide
fn page(user: User) -> Html =
  html.div({class: "card"}, [
    html.h1({}, [html.text(user.name)]),
    html.p({}, [html.text(user.email)]),
  ])
```

Props を型チェック時に検証。`Html` 型をレスポンスに直接返せる。

### ストリーミングSSR

```almide
effect fn render_stream(req: Request, w: Writer) -> Result[Unit, String] = {
  w.write(shell_head(meta))
  w.flush()
  let data = fetch_data(req.param("id"))  // 非同期データ取得中にシェルを先行送信
  w.write(render_body(data))
  w.write(shell_tail())
  ok(())
}
```

chunked transfer encoding でシェルを先行送信、データ到着後に本体を流す。

### マルチターゲットデプロイ

Almideの既存3ターゲットと対応:

| デプロイ先 | ターゲット | 備考 |
|-----------|-----------|------|
| VPS / コンテナ | `--target rust` | スタンドアロンバイナリ |
| Cloudflare Workers | `--target wasm` | WASI + edge deploy |
| Deno Deploy / Bun | `--target ts` | Node互換ランタイム |

同一の `.almd` ソースから3つのデプロイ形態を出し分ける。これはmerjs（native + WASM の2ターゲット）より優位。

## Depends On

- `http` stdlib (done)
- `template` (active — for `web.html`)
- `deriving Codec` (active — for `web.json` with typed records)
- UFCS for external libs (active — for `req.param`)
- Package system (on-hold — for distribution as external lib)
- ディレクトリ走査 (self-hosting roadmap と共通)
- Writer 抽象 / trait (ストリーミングSSRに必須)
- セッション / HMAC (crypto stdlib 追加)

## Implementation Order

1. **Phase 1**: Routing + Response builders + `web.serve` (http.serve の wrapper)
2. **Phase 2**: Middleware (`web.use`) + route groups (`web.mount`)
3. **Phase 3**: Template integration (`web.html`) + HTML DSL
4. **Phase 4**: Codec integration (`req.body_json[T]`, `web.json(record)`)
5. **Phase 5**: ファイルベースルーティング (コンパイラ拡張)
6. **Phase 6**: ストリーミングSSR + マルチターゲットデプロイ

## Position

- **Not stdlib** — framework の opinions は stdlib に入れない
- **First-party external lib** — Almide が公式で出す
- Go の `net/http` (stdlib) に対する Echo/Gin (外部) と同じ関係
- **Self-hosting roadmap と前提機能を共有** — Web framework 開発が言語機能を鍛える dogfooding になる
