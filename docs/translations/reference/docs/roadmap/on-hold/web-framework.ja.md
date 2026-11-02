<!-- description: stdlib http 上の外部 package として、template と Codec を統合する Hono 風 web framework -->
<a id="web-framework"></a>

# Web Framework

[![English](https://img.shields.io/badge/English-d0d7de?style=flat-square)](web-framework.md) [![日本語](https://img.shields.io/badge/%E6%97%A5%E6%9C%AC%E8%AA%9E-24292f?style=flat-square)](web-framework.ja.md) [![简体中文](https://img.shields.io/badge/%E7%AE%80%E4%BD%93%E4%B8%AD%E6%96%87-d0d7de?style=flat-square)](web-framework.zh-CN.md)

> 非規範的な固定履歴参照版。原本は不変の正本で、自動同期しない。引用コマンド・CI変更・測定・現行issue再調査は実施していない。issue一覧の件数と日付時点の状態は、重複除去4件とscript7件/#2400説明、判断待ちと1PR修正分類の不一致も含め原文どおり保持する。現在の作業一覧ではない。Web APIとdeployment adapterは提案であり、前提機能が出荷済みでもframework出荷済みとはいえない。比較や260KBは過去の原文の主張で、独立再測定していない。コードと旧API表記は保持するが、実行可能と認証していない。 [Source](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/on-hold/web-framework.md)

<a id="vision"></a>

## 構想

Almide の first-party web framework。Almide の思想に沿って Hono 相当の DX を目指す。

stdlib の `http` primitive 上に置き、外部ライブラリとして提供する薄い層の提案。下記 `web.*` API と deployment 例は設計スケッチであり、この package や target adapter が出荷済みという証拠ではない。

<a id="status-2026-10-24-decision"></a>

## 状態（2026-10-24の決定）

- **配置：stdlib でなく外部 package。** stdlib `http` は stdlib 再設計で確定した表面を保持する（`route` / `router` / `mount` / `wrap` / `param` / `serve`。[L2-http](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/stdlib-redesign/L2-http.md) から不変）。この層はその上に載り、`stdlib/http.almd` に何も追加しない。
- **見た目は Hono、意味論は Almide。** モデルが既に知る語彙（`app |> get(...) |> post(...) |> use(...)`、`json` / `text` / `html` responder）を維持する。共有可変状態に依存するものは置き換える：
  - `c.set` / `c.get` は request とともに渡す値へ。
  - `await next()` の onion は `Handler -> Handler` という middleware へ（`http.wrap` が既に合成する形）。
  - 変更される `app` は pipe で組み立てる値へ。
- **更新された前提：** default parameter は実装済み（`spec/lang/default_args_test.almd`）。package は JSON helper の signature をなお選定・テストする必要があるが、default 不在という元の前提はもはや blocker ではない。
- **時期：** stdlib 再設計の各 wave と v0.68.0 release の後。

<a id="design-principles"></a>

## 設計原則

1. **不変** — 可変 context を持たず、`Request -> Response` の純粋変換
2. **pipe chain で合成** — Almide の自然な合成機構
3. **template 統合** — `HtmlDoc` を直接返す
4. **Codec 統合** — 既存の `: Codec` 宣言で request/response JSON を型安全に扱う
5. **target** — 対応する native/Wasm host 向けに設計。TypeScript 出力は廃止済みで、deployment adapter の検証はなお必要
6. **薄い層** — routing、middleware、response builder のみ

<a id="api-design"></a>

## API 設計

<a id="basic"></a>

### 基本

```almide
import web

let app = web.new()
  |> web.get("/", fn(req) => web.text("Hello"))
  |> web.get("/about", fn(req) => web.text("About"))

effect fn main() -> Result[Unit, String] = {
  web.serve(app, 3000)
}
```

<a id="routing"></a>

### Routing

```almide
let app = web.new()
  |> web.get("/users/:id", fn(req) => {
    let id = req.param("id")              // proposed request API using existing UFCS
    web.json(json.object([("id", json.s(id))]))
  })
  |> web.post("/users", fn(req) => {
    web.json_status(201, req.body)
  })
  |> web.get("/files/*", fn(req) => {
    web.text("File: ${req.wildcard()}")    // UFCS
  })
```

<a id="template-integration"></a>

### Template 統合

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

`web.html(HtmlDoc) -> Response` は内部で `render(doc)` を呼び、`Content-Type: text/html` を設定する。

<a id="codec-integration-using-the-existing--codec-declaration"></a>

### Codec 統合（既存の `: Codec` 宣言を利用）

```almide
type CreateUser: Codec = { name: String, email: String }
type UserResponse: Codec = { id: String, name: String, email: String }

let app = web.new()
  |> web.post("/users", fn(req) => {
    let input = req.body_json[CreateUser]()
    let user = create_user(input)
    web.json(user)                            // auto encode via Codec
  })
```

<a id="middleware"></a>

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

<a id="route-grouping"></a>

### Route の grouping

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

<a id="error-handling"></a>

### エラー処理

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

<a id="api-summary"></a>

## API 一覧

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

<a id="comparison-with-hono"></a>

## Hono との比較

| 観点 | Hono | Almide web |
|--------|------|-----------|
| Context | 可変 `c` object | 不変 `req` → `Response` |
| Middleware | `c.next()` onion model | `Handler -> Handler` 関数合成 |
| Response | `c.text()`、`c.json()` | `web.text()`、`web.json()`（pure fn） |
| HTML | `c.html(string)` | `web.html(HtmlDoc)` — 型付き document |
| JSON | object literal | `: Codec` で型安全に |
| Routing | `.get("/path", handler)` | `\|> web.get("/path", handler)` |
| Method access | `c.req.param()` | `req.param()`（UFCS） |

<a id="dx-gap-whats-needed-to-match-hono"></a>

## DX の差：Hono に並ぶために必要なもの

| 優先度 | 機能 | Roadmap | 効果 |
|----------|---------|---------|--------|
| **1** | `: Codec`（実装済み） | [CHEATSHEET](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/CHEATSHEET.md) | 生成 encode/decode を提案 response API で使う |
| **2** | UFCS（実装済み） | [型システム](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/specs/type-system.md) | 現行の解決規則に沿って package API を設計 |
| **3** | `(req) => ...` lambda（実装済み） | [CHEATSHEET](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/CHEATSHEET.md) | 現行 lambda 構文を使う |
| **4** | default param（実装済み） | `spec/lang/default_args_test.almd` | 提案 helper signature を選びテストする |

上記すべてが揃った形：

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

<a id="patterns-to-learn-from-merjs-zig"></a>

## merjs（Zig）から学ぶ pattern

[merjs](https://github.com/justrach/merjs) は単一 Zig binary（260KB）で Next.js 相当を実現する。Almide でも以下の pattern を採用する価値がある：

<a id="file-based-routing"></a>

### File-based routing

directory 構造から route table を自動生成する。

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

Almide では compiler 拡張で実現可能 — `almide build --web app/` が directory を走査し、IR level で route table を生成する。別の codegen binary は不要。

<a id="html-dsl"></a>

### HTML DSL

```almide
fn page(user: User) -> Html =
  html.div({class: "card"}, [
    html.h1({}, [html.text(user.name)]),
    html.p({}, [html.text(user.email)]),
  ])
```

props は型検査時に検証する。`Html` 型を response として直接返せる。

<a id="streaming-ssr"></a>

### Streaming SSR

```almide
effect fn render_stream(req: Request, w: Writer) -> Result[Unit, String] = {
  w.write(shell_head(meta))
  w.flush()
  let data = fetch_data(req.param("id"))  // Send shell ahead while fetching data asynchronously
  w.write(render_body(data))
  w.write(shell_tail())
  ok(())
}
```

chunked transfer encoding で shell を先に送り、データ到着後に body を stream する。

<a id="multi-target-deployment"></a>

### 複数 target への deployment

提案 deployment 対応表。compiler target があるだけでは host import や platform deployment の互換性は確立しない：

| 配置先 | Target | 注記 |
|---------------|--------|-------|
| VPS / container | `--target rust` | standalone binary |
| Wasm host / edge platform | `--target wasm` | host/import adapter と deployment 制約を検証。Workers 互換が自動的に得られるという主張ではない |
| Deno Deploy / Bun | TS 出力 target なし | 別途、対応 host との統合が必要 |

元の3-target／merjs比較は、現行の対応 deployment を主張するものではない。native と Wasm の統合にはそれぞれ実行可能な受入テストが必要。

<a id="depends-on"></a>

## 依存するもの

- `http` stdlib（完了）
- `template`（active — `web.html` 用）
- `: Codec`（実装済み。提案 `web.json` の package 実装はなお必要）
- UFCS（実装済み。提案 request helper を現行の解決規則に合わせ設計・テスト）
- package system（Git/local 解決は実装済み。外部配布は registry を待つ必要がない）
- directory traversal（self-hosting roadmap と共有）
- Streaming SSR 統合は提案のまま。既存の `io.Write` は基盤であり、実装済み SSR adapter ではない
- session layer は提案のまま。既存の `hash.hmac_sha256` は HMAC を提供するが、完全な session policy ではない

<a id="implementation-order"></a>

## 実装順

1. **Phase 1**：routing ＋ response builder ＋ `web.serve`（http.serve の wrapper）
2. **Phase 2**：middleware（`web.use`）＋ route group（`web.mount`）
3. **Phase 3**：template 統合（`web.html`）＋ HTML DSL
4. **Phase 4**：Codec 統合（`req.body_json[T]`、`web.json(record)`）
5. **Phase 5**：file-based routing（compiler 拡張）
6. **Phase 6**：Streaming SSR ＋複数 target deployment

<a id="position"></a>

## 位置づけ

- **stdlib ではない** — framework の方針は stdlib に置かない
- **first-party 外部 lib** — Almide が公式公開するもの
- Go の `net/http`（stdlib）に対する Echo/Gin（外部）と同じ関係
- **self-hosting roadmap と前提機能を共有** — web framework 開発が dogfooding となり、言語機能を強化する
