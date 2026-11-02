<!-- description: stdlib http 上的 Hono 风格外部框架，集成 template 和 Codec -->
<a id="web-framework"></a>

# Web Framework

[![English](https://img.shields.io/badge/English-d0d7de?style=flat-square)](web-framework.md) [![日本語](https://img.shields.io/badge/%E6%97%A5%E6%9C%AC%E8%AA%9E-d0d7de?style=flat-square)](web-framework.ja.md) [![简体中文](https://img.shields.io/badge/%E7%AE%80%E4%BD%93%E4%B8%AD%E6%96%87-24292f?style=flat-square)](web-framework.zh-CN.md)

> 非规范性固定历史阅读版；原本不变且为权威，不自动同步。未执行引用命令、CI修改、测量或当前issue复查。保留清单数量与日期状态，包括去重4项与script7项/#2400解释、等待决策与单PR修复分类的原文不一致；这不是当前任务清单。Web API和deployment adapter是提案，前提功能交付不证明框架已交付。比较及260KB是历史原文主张，未独立重测。保留代码及旧API写法，但不认证可运行。 [Source](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/on-hold/web-framework.md)

<a id="vision"></a>

## 愿景

Almide 第一方 web framework，以 Almide 理念实现 Hono 相当的 DX。

提议在 stdlib 的 `http` primitive 上构建薄层，以外部库提供。下方 `web.*` API 与部署示例是设计草图，不证明该 package 或 target adapter 已交付。

<a id="status-2026-10-24-decision"></a>

## 状态（2026-10-24 决定）

- **位置：外部 package，不在 stdlib。** stdlib `http` 保持 stdlib 重设计确定的表面（`route` / `router` / `mount` / `wrap` / `param` / `serve`，[L2-http](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/stdlib-redesign/L2-http.md) 不变）。本层位于其上，不向 `stdlib/http.almd` 添加内容。
- **Hono 外观，Almide 语义。** 保留模型已熟悉的词汇（`app |> get(...) |> post(...) |> use(...)`，`json` / `text` / `html` responder），替换依赖共享可变状态的部分：
  - `c.set` / `c.get` 变为随 request 传递的值；
  - `await next()` onion 变为 `Handler -> Handler` middleware（即 `http.wrap` 已组合的形式）；
  - 可变 `app` 变为 pipe 构建的值。
- **更新前提：** default parameter 已实现（`spec/lang/default_args_test.almd`）。package 仍需选择并测试 JSON helper signature；原先没有 default 的前提不再阻挡它。
- **时机：** stdlib 重设计各 wave 和 v0.68.0 release 之后。

<a id="design-principles"></a>

## 设计原则

1. **不可变** — 无可变 context，纯粹变换 `Request -> Response`
2. **用 pipe chain 组合** — Almide 自然的组合机制
3. **template 集成** — 直接返回 `HtmlDoc`
4. **Codec 集成** — 通过既有 `: Codec` 声明提供类型安全的 request/response JSON
5. **target** — 面向支持的 native/Wasm host；TypeScript 输出已退役，deployment adapter 仍需验证
6. **薄层** — 仅 routing、middleware、response builder

<a id="api-design"></a>

## API 设计

<a id="basic"></a>

### 基础

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

### Template 集成

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

`web.html(HtmlDoc) -> Response` 内部调用 `render(doc)` 并设置 `Content-Type: text/html`。

<a id="codec-integration-using-the-existing--codec-declaration"></a>

### Codec 集成（使用既有 `: Codec` 声明）

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

函数组合。`Handler -> Handler`。

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

### Route 分组

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

### 错误处理

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

## API 汇总

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

## 与 Hono 比较

| 方面 | Hono | Almide web |
|--------|------|-----------|
| Context | 可变 `c` object | 不可变 `req` → `Response` |
| Middleware | `c.next()` onion model | `Handler -> Handler` 函数组合 |
| Response | `c.text()`、`c.json()` | `web.text()`、`web.json()`（pure fn） |
| HTML | `c.html(string)` | `web.html(HtmlDoc)` — 有类型的 document |
| JSON | object literal | 通过 `: Codec` 类型安全处理 |
| Routing | `.get("/path", handler)` | `\|> web.get("/path", handler)` |
| Method access | `c.req.param()` | `req.param()`（UFCS） |

<a id="dx-gap-whats-needed-to-match-hono"></a>

## DX 差距：达到 Hono 需要什么

| 优先级 | 功能 | Roadmap | 效果 |
|----------|---------|---------|--------|
| **1** | `: Codec`（已实现） | [CHEATSHEET](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/CHEATSHEET.md) | 在拟议 response API 使用生成的 encode/decode |
| **2** | UFCS（已实现） | [类型系统](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/specs/type-system.md) | 按当前解析规则设计 package API |
| **3** | `(req) => ...` lambda（已实现） | [CHEATSHEET](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/CHEATSHEET.md) | 使用当前 lambda 语法 |
| **4** | default param（已实现） | `spec/lang/default_args_test.almd` | 选择并测试拟议 helper signature |

具备以上各项后：

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

## 从 merjs（Zig）学习的 pattern

[merjs](https://github.com/justrach/merjs) 以单一 Zig binary（260KB）实现 Next.js 相当功能。以下 pattern 值得 Almide 采用：

<a id="file-based-routing"></a>

### 基于文件的 routing

从 directory 结构自动生成 route table。

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

在 Almide 可用 compiler 扩展实现：`almide build --web app/` 扫描 directory，在 IR level 生成 route table，不需要单独的 codegen binary。

<a id="html-dsl"></a>

### HTML DSL

```almide
fn page(user: User) -> Html =
  html.div({class: "card"}, [
    html.h1({}, [html.text(user.name)]),
    html.p({}, [html.text(user.email)]),
  ])
```

props 在类型检查时验证。`Html` 类型可直接作为 response 返回。

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

通过 chunked transfer encoding 先发送 shell，数据到达后 stream body。

<a id="multi-target-deployment"></a>

### 多 target 部署

拟议部署映射；单有 compiler target，并不确立 host import 或 platform deployment 兼容性：

| 部署位置 | Target | 说明 |
|---------------|--------|-------|
| VPS / container | `--target rust` | 独立 binary |
| Wasm host / edge platform | `--target wasm` | 验证 host/import adapter 及部署约束，不是自动兼容 Workers 的主张 |
| Deno Deploy / Bun | 无 TS 输出 target | 需要另外的受支持 host 集成 |

原先三 target／merjs 比较不是当前受支持部署的声明。native 和 Wasm 集成都需各自可执行的接受测试。

<a id="depends-on"></a>

## 依赖

- `http` stdlib（完成）
- `template`（active，用于 `web.html`）
- `: Codec`（已实现；拟议 `web.json` 仍需 package 实现）
- UFCS（已实现；按当前解析规则设计／测试拟议 request helper）
- package system（Git/local 解析已实现；外部分发不必等 registry）
- directory traversal（与 self-hosting roadmap 共享）
- Streaming SSR 集成仍是提案；既有 `io.Write` 是基础，不是已实现 SSR adapter
- session layer 仍是提案；既有 `hash.hmac_sha256` 提供 HMAC，不是完整 session policy

<a id="implementation-order"></a>

## 实现顺序

1. **Phase 1**：routing ＋ response builder ＋ `web.serve`（http.serve wrapper）
2. **Phase 2**：middleware（`web.use`）＋ route group（`web.mount`）
3. **Phase 3**：template 集成（`web.html`）＋ HTML DSL
4. **Phase 4**：Codec 集成（`req.body_json[T]`、`web.json(record)`）
5. **Phase 5**：基于文件的 routing（compiler 扩展）
6. **Phase 6**：Streaming SSR ＋多 target 部署

<a id="position"></a>

## 定位

- **不属于 stdlib** — framework 的取舍不应放入 stdlib
- **第一方外部 lib** — 由 Almide 官方发布
- 如同 Echo/Gin（外部）与 Go `net/http`（stdlib）的关系
- **与 self-hosting roadmap 共享前提功能** — web framework 开发成为 dogfooding，强化语言功能
