# http

[![English](https://img.shields.io/badge/English-d0d7de?style=flat-square)](http.md) [![日本語](https://img.shields.io/badge/%E6%97%A5%E6%9C%AC%E8%AA%9E-d0d7de?style=flat-square)](http.ja.md) [![简体中文](https://img.shields.io/badge/%E7%AE%80%E4%BD%93%E4%B8%AD%E6%96%87-24292f?style=flat-square)](http.zh-CN.md)



> 原文：[http.md @ 35297a6](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/stdlib/http.md)。这是固定源版本的非规范性快照，以英文原文为准，不会自动生成或自动同步。签名、类型、版本注记和代码示例（包括注释）保持原样，文末附 API 和类型说明译文。代码示例、行为以及 TLS 与安全性主张未经独立执行或重新验证。

> 原文注意：保留了不能按名称写入 Content-Type 的一般性表述，也保留了后文“请求可以设置”的范围限定。此外，状态行不合法时 getter 返回 0 的说明，以及客户端拒绝格式错误或不完整响应的说明，均按原文保留，未在翻译中自行统一。

HTTP 客户端与服务器。import http，effect。

`HttpRequest` / `HttpResponse` 是此模块由运行时支持的名义类型。通过 `import http`，用户类型注解中的非限定名和限定名都能解析，因此可以编写处理请求的带类型辅助函数：

```almd check
import http

fn handle(req: HttpRequest) -> http.HttpResponse = http.response(200, http.path(req))

effect fn main() -> Unit = {
  http.serve(3000, handle)
}
```

两种类型都是 `HttpMessage`，所以同一函数即可读写任一类消息的头部和正文：`header`、`header_values`、`headers`、`body`、`body_bytes`、`set_header`、`set_body` 和 `set_body_bytes` 接受请求或响应，并返回同一种类。头部列表在所有位置都是 `List[(String, String)]`，包括 `with_headers`、`new_request`、`request` 和 `headers` 的返回值。接收到的头部保留线上顺序和重复字段。而 `with_headers` 等构建器按不区分大小写的名称插入或更新，保留首次拼写和最后一个值。

客户端函数是返回 `Result` 的 `effect fn`。传播是显式的（ADR-0008）：在 effect fn 中写 `http.send(req)!`，或用 `match` / `??` 处理 `Result`。单独的 `http.get(url)` 语句是 E042。

### `http.serve(port: Int, handler: HttpHandler) -> Unit`

在指定端口启动带请求处理器的 HTTP 服务器。

```almd check
import http

effect fn main() -> Unit = {
  http.serve(3000, (req) => http.response(200, "ok"))
}
```

服务器监听 `0.0.0.0:port`（所有接口；ADR-0020 §5.7）。它同时接受并读取最多 64 个连接，按请求完整到达的顺序，每次为一个请求调用处理器。并发连接达到 64 时会停止接受，直到有连接关闭；新连接在操作系统的 backlog 中等待。缓慢发送请求的客户端只占用自己的连接，因此其他客户端仍会得到响应（#4021）。服务器在处理器看到请求前，先在以下限制内读取它：

| 限制 | 值 | 响应 |
|---|---|---|
| 从接受连接起的请求头部（请求行和头字段） | 10 s | `408`；此前没有发送任何内容的客户端会直接关闭，不作响应 |
| 从接受连接起的整个请求（包括正文读取和等待处理器） | 30 s | `503` |
| 由 `Content-Length` 声明或分块发送的正文 | 1 MiB | `413` |
| 请求行 / 头字段行 | 8 KiB | `414` / `431` |
| 头字段数 | 100 | `431` |

正文短于其 `Content-Length` 时为 `400`。以上情况均不调用处理器。分块请求正文会被解码。
请求头部按 RFC 9112 读取（C-405）：行未以 CRLF 结尾，请求行不是具有 token 形式方法的 `METHOD SP target SP HTTP/1.1`（或 `HTTP/1.0`），目标为 `OPTIONS *` 或 `CONNECT host:port`，头字段冒号前有空白（`Transfer-Encoding : chunked`），头字段行没有冒号，obs-fold 续行，存在两个 `Host` 字段，以及 HTTP/1.1 请求没有 `Host`，都会在处理器运行前返回 `400`。绝对形式目标（`GET http://host/p?q HTTP/1.1`）到达处理器时为 `/p?q`。
响应依次发送 `HTTP/1.1 <status> <reason>`（注册的原因短语；未注册代码为空）、按顺序的响应头、`Connection: close`、`Content-Length`、正文，然后关闭连接。HEAD 请求、`204` 和 `304` 不带正文。服务器管理消息分帧：处理器的 `Connection`、`Content-Length`、`Transfer-Encoding` 和 `Keep-Alive` 字段会被丢弃。除非处理器设置了该字段，每个响应都带 `X-Content-Type-Options: nosniff`（C-407）。名称不是 token、值含 HTAB 以外控制字符的响应头，以及 `200..=599` 之外的状态（含 1xx），都会被拒绝：响应为 `500`，正文是 `Internal Server Error`，原因作为一行写入 stderr（`http.serve: refused the response: response status 42 is not a final status
code`）。超过 30 s 的响应替换为 `503`。

处理器的 `err(m)` 永远不会到达客户端（C-409、#4150）：`m` 可能含路径、操作数或 errno 文本（处理器中的 `fs.read_text(p)!` 会把绝对路径放进去）。客户端收到 `500` 和固定正文 `Internal Server Error (request <id>)`；`id` 是对请求方法、一个空格、目标、一个 LF 和正文计算 32-bit FNV-1a（`hash.fnv1a32`）所得的 8 位小写十六进制数，同一请求在所有目标上相同。stderr 收到 `http.serve: request <id>: handler error: "<m>"`，其中 `\`、`"` 和控制字符经过转义，使 `m` 中的输入无法伪造日志行。用客户端报告的 id 查找错误。若端口无法绑定，程序以 `Error: bind failed: <reason>` 和退出码 1 停止。

处理器内部的运行时中止——越界索引、除零、`panic(msg)`——只使该请求失败（C-434、#4203）。客户端收到与 `err` 相同的 `500` 和固定正文，stderr 收到一行 `http.serve: request <id>: handler aborted: "<text>"`（`panic` 对应 `PANIC: <msg>`）。处理器打开但未关闭的套接字、unix 描述符和 http 调用也一并关闭。服务器继续处理下一个请求。栈耗尽和 `process.exit` 仍会结束程序，即便处理器已自行打印一行 `Error: `。内存耗尽也一样：若处理器无法取得所需分配，程序以 `Error: out of memory` 和退出码 1 结束（C-197），该请求没有响应，因为处理器构建中的内容仍是半成品。

**在 wasm 上**（C-367），`almide run app.almd --target wasm` 在嵌入式宿主上提供同一程序。一个实例逐个处理所有请求，与 native 进程完全一样：`main` 只运行一次。宿主通过与 native 相同的服务器代码读取连接，使用相同限制（64 个连接、10 s 请求头部期限及其 `408`），因此慢客户端也不会阻塞其他客户端。状态行、头部和正文与 native 逐字节一致。`almide build --target wasm` 生成的标准产物没有监听套接字，所以 `almide build` 和 `almide check --target wasm` 仍拒绝 `http.serve`（E081、#2659）。

## 路由与中间件

处理器是从请求到响应的 effect 函数值，即 `type HttpHandler = effect (HttpRequest) -> HttpResponse`，路由器和包装后的应用也是如此。`http.serve` 是运行处理器的一种方式；`http.new_request` 是另一种：测试在代码中构建请求，直接调用应用，不使用套接字。路由和请求构建是纯操作，在 native 和 wasm 上行为一致，路由器也像其他处理器一样提供服务。
此示例在两个目标上打印以下各行；在 wasm 上，路由器由结构化分支提供（#2664，由 `spec/stdlib/http_router_test.almd` 约束）。

路由器及其前面的任何中间件读取同一个规范路径（C-406、#4133）：路径内部有空段（`//admin`）、`.` 或 `..` 段、百分号编码的非保留字节（`%61dmin`、`%2e`）、反斜杠或控制字符时，返回 `400 Bad Request: the path is not in canonical form`。因此，`string.starts_with(http.path(req), "/admin")` 这样的守卫检查的是与路由器分派相同的路径。绑定参数解码为 `.` 或 `..`，或含 `/`（`%2F`）、`\` 或 NUL 时，返回 `400 Bad Request: a path
parameter is a dot segment or holds a /, \ or NUL`（#4134），因此 `/static/{file}` 永远不会绑定 `../../etc/passwd`。目前没有静态文件辅助函数（`http.serve_dir` 不存在）；处理器提供文件时，应使用有根目录约束的 `fs` 家族在根内解析名称，绝不能拼接字符串。

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

- **模式** — `http.route("METHOD /path", handler)`；方法可选（`"/health"` 响应所有方法）。`{name}` 绑定一个段并进行百分号解码（`+` 仍为 `+`），用 `http.param(req, name)` 读取；末尾的 `{name...}` 绑定剩余路径，可能为空。匹配使用不带查询字符串的路径；空段不计，因此 `/users/` 即 `/users`。
- **优先级** — 无论注册顺序如何，最具体的匹配路由响应：`GET /users/me` 优先于 `GET /users/{id}`，带方法的路由优先于同路径不带方法的路由。
- **拒绝** — `http.router` 返回列出所有问题的 `err`，因此损坏的路由表不会用于服务：两个路由匹配某个请求却没有一个更具体（Go 1.22 的规则，包括同一模式的两种拼写）、`{name...}` 不在末尾、同一名称绑定两次、方法不是大写字母、路径不以 `/` 开头。
- **路由器响应** — 没有路由匹配路径：`404 Not Found`。路由匹配路径但不匹配方法：带 `Allow` 头的 `405 Method Not Allowed`。`HEAD` 回退到 `GET` 路由并丢弃正文。请求目标不以 `/` 开头，或路径中的 `%` 后没有两个十六进制数字：`400 Bad Request`。处理器的 `err` 仍是 `err`（`http.serve` 将其转换为自己的 500）。
- **挂载** — `http.mount("/api", sub)` 将 `/api/items?x=1` 以 `/items?x=1` 交给 `sub`；前缀绑定的参数（`/orgs/{org}`）对 `sub` 仍可见。
- **中间件** — `type HttpMiddleware = (HttpHandler) -> HttpHandler`。`http.wrap(app, [a, b])` 就是 `a(b(app))`：列表中的第一个位于最外层。中间件可不调用被包装的处理器就响应。每请求数据通过参数传递，没有可变上下文。
- **请求正文** — `http.decode_json[T](req)` 将正文解析为输入类型 `T`：声明 `: Decode` 且不同时声明 `Encode` 的记录或变体，每个字段都是输入（C-436）。`Codec` 类型——程序也会发送或存储的类型——会触发检查错误 E110，嵌套在 `T` 中的 `Codec` 记录也是如此：解码到输入类型，再在代码中构建领域值，让客户端不得设置的字段（`is_admin`）无从进入。正文使用 `T.decode_strict` 解码；其 `err` 是可直接使用的 `400 Bad Request` 响应，说明原因：不是 JSON、字段类型错误、缺少字段，或任意深度存在 `T` 之外的键（`unknown key '<k>'`，带路径，例如 `unknown key 'verified', at address`）。
- **范围之外** — 内置日志器 / CORS / 会话、流式响应和 WebSocket 留给包实现。

### `http.response[B: Body](status: Int, body: B) -> HttpResponse`

创建一个 `Content-Type` 由正文类型决定的响应（ADR-0032）：String 为 `text/plain; charset=utf-8`，Bytes 为 `application/octet-stream`，`Value` 为 `application/json`，`html.*` 页面为 `text/html; charset=utf-8`，包的安全类型使用自身的字面量类型（运行脚本时需在 `[trust] media` 中授权，ADR-0032 D7），`http.typed(t, bytes)` 则使用字面量类型 `t`。任何头字符串都不能重新标记正文：Content-Type 绝不能按名称写入（E116）。在标准库之外拥有可执行脚本的媒体类型，需要应用授权（E118）。

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

其他媒体类型的正文。类型以字面量书写，在检查时读取（E117）：计算得出的类型、格式错误的类型，或浏览器会在其中运行脚本的类型——HTML 和 XHTML（由 `html` 拥有）、SVG（属于获授权的包）、XML 和 `+xml` 类型——会被拒绝，因此 String 不能绕过安全类型，以标记内容形式到达浏览器。目前构建的正文以文本保存，所以非 UTF-8 字节会被有损解码（#4311）。

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

同时替换响应正文及其 `Content-Type`；保留状态和其他头部。

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

创建带状态码的 JSON HTTP 响应。

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

创建响应，先放正文的 `Content-Type`，然后严格按列表顺序放入给定头部（契约 C-275）。不区分大小写，同名出现两次时保留首次拼写和最后一个值。列表中的 `Content-Type` 会被拒绝，因为它属于正文（E116；计算得出的名称则中止执行）。

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

创建 302 临时重定向响应：`Location`、空正文，以及空 String 正文的 `Content-Type: text/plain; charset=utf-8`。

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

设置响应状态码。此处不检查范围；发送响应时，`http.serve` 拒绝 `200..=599` 之外的状态。

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

读取响应状态码，是 `http.set_status` 对应的 getter。获取的响应若无法读取状态行，则为 `0`。

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

## 两种消息共用的头部与正文

本节每个函数都接受 `HttpRequest` 或 `HttpResponse`（`[M: HttpMessage]`）；写入函数返回相同种类。由 `http.new_request` 构建的请求和由 `http.response` 构建的响应，使用相同调用读取和修改。

### `http.header(m, name: String) -> Option[String]`

某个头字段的第一个值；字段不存在时为 `none`。字段名不区分大小写（RFC 9110 §5.1），所以 `"content-type"` 和 `"Content-Type"` 是同一字段。

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

按线上顺序返回某个字段的所有值，这是重复字段（`Set-Cookie`）的访问器，而 `header` 只返回第一次出现的值。查找不区分大小写；字段不存在时为 `[]`。

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

每个头部作为 `(name, value)` 对返回，名称保持发送或设置时的拼写，按线上顺序排列，重复字段每次出现都保留。不转成小写，也不合并；用忽略大小写的 `header` / `header_values` 查找字段。

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

正文文本。客户端获取的正文以有损 UTF-8 解码。

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

以 `Bytes` 返回正文。对于客户端获取的响应（`get_response`、`send`、`*_response` 家族），这是服务器发送的原始字节，逐字节保留，所以非 UTF-8 正文（图像、音频）不会丢失；`http.body` 是这些字节的有损 UTF-8 解码。对于在代码中构建的消息，则是 `http.body(m)` 的 UTF-8 字节。

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

设置一个头字段。字段名不区分大小写，因此无论拼写如何，第一个同名字段都会原位更新值（保留拼写和位置）；若没有则追加。此方式不会为消息增加第二个条目。响应拒绝 `Content-Type`，因为它是正文的类型（E116；计算得出的名称则中止执行）；请求可以设置它。

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

替换正文文本。响应的 `Content-Type` 变为 String 正文对应的 `text/plain; charset=utf-8`（正文与类型一起替换；`http.with_body` 接受任何 `Body`）；请求的头部保持不变。

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

用 `Bytes` 替换正文。目前消息正文以文本保存，因此字节会被有损 UTF-8 解码：非 UTF-8 字节序列变成 U+FFFD，无法保留。将它用于以 `Bytes` 持有的文本；这不是无损二进制上传。

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

## 读取请求

`http.new_request(method, target, body, headers)` 在代码中构建请求，用于测试无套接字调用应用，也用于构建 `http.send` / `http.start` 的请求；后一种情况下目标是完整 URL。

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

获取请求发送时的 HTTP 方法（GET、POST 等）。

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

获取请求目标，包括查询字符串（`/search?q=x`）。对于为发送而构建的请求，它是完整 URL。

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

请求的头部和正文用上述共用函数读取：

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

头部中的凭据需要验证，而不只是检查存在性：将两侧都包在 `secret.new` 中，其对 `Secret[String]` 的 `==` 是恒定时间（C-426），缺失或错误均响应 401。头文本上的普通 `==` 会在首个不同字节处返回，因此耗时会泄露令牌。

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

查询按 `url.parse_query` 的方式读取（C-404、#4117）：取目标中第一个 `?` 后的文本，以 `&` 分割并丢弃空片段，每对在第一个 `=` 处分割（裸键的值为 `""`），`+` 为空格，`%XX` 按严格 UTF-8 解码。每一对都按到达顺序保留，因此 `?tag=a&tag=b` 会给出两项。截断或非十六进制转义、不是 UTF-8 的百分号数据均为 `err`（`http.query_params: invalid percent escape` 等）。native 与 wasm 相同。

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

### 解码表单值

`http` 没有自己的解码器：`http.url_decode` 已移除。使用 `url` 模块解码表单编码值（`+` 为空格，`%XX` 为一个字节）；它会拒绝错误转义或非 UTF-8 百分号数据，而不是保留它们。输入可能不合法时回退到输入：

```almd check
import url

fn main() -> Unit = {
  println(url.decode_form("%E7%8C%AB") ?? "%E7%8C%AB") // "猫"
  println(url.decode_form("a+b%20c") ?? "a+b%20c")
}
```

## 客户端

任何完整响应都是 `ok`，包括 404 和 3xx；`err` 仅表示传输失败。绝不跟随重定向：3xx 及其 `Location` 头就是答案。一次性客户端是一个二乘二矩阵，外加调用句柄；要取得 `Bytes` 正文，对完整响应调用 `http.body_bytes`：

| | 正文文本 | 完整响应 |
|---|---|---|
| GET | `http.get(url)` | `http.get_response(url)` |
| 任意方法、正文和头部 | `http.request(method, url, body, headers)` | `http.send(http.new_request(method, url, body, headers))` |
| 带限制的句柄 | | `http.start(req, limits)` — 参见[逐次调用限制与取消](#per-call-limits-and-cancellation) |

没有逐方法函数：POST 使用 `http.request("POST", url, body,
headers)`；若需 POST 的状态，使用 `http.send(http.new_request("POST", url, body, headers))`，然后调用 `http.status(resp)` 和 `http.body(resp)`。流式正文通过句柄读取（在 `http.poll` 之间调用 `http.read_new`）。

### `http.get(url: String) -> Result[String, String]`

发送 HTTP GET 请求并返回响应正文。

每个客户端函数都完整读取响应，否则返回 `malformed or incomplete response from "<url>"`（C-408、#4137）：没有完整头部、正文短于 `Content-Length`、分块正文缺少终止的 0 块、头部超过 1 MiB，均是错误，绝不返回部分 `ok`。在可能与浏览器产生歧义处，URL 按 `url.parse` 的方式拆分（C-402）：拒绝反斜杠或控制字符以及非规范数字主机（`127.1`、`0x7f.0.0.1`），移除周围空白和 tab/LF/CR，并在发送前解析 `.` / `..` 段（包括 `%2e`）。

```almd check
import http

effect fn main() -> Unit = {
  let html = http.get("https://example.com")!
  println(html)
}
```

### `http.request(method: String, url: String, body: String, headers: List[(String, String)]) -> Result[String, String]`

发送带任意方法、正文和头部的请求，并返回响应正文。无正文传 `""`，无额外头部传 `[]`。

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

#### 读取超时（`ALMIDE_HTTP_TIMEOUT_SECS`）

每个不接收限制的客户端调用最多等待服务器 **30 秒**。环境变量 `ALMIDE_HTTP_TIMEOUT_SECS` 为所有这些调用覆盖该默认值；`0` 表示**无超时**，阻塞直到服务器响应。这只是默认值：自带限制的调用（`http.start`）忽略它；参见[逐次调用限制与取消](#per-call-limits-and-cancellation)。限制作用于字节之间，而非整个调用，所以持续发送的服务器不会触发它。缓慢端点（本地 LLM 评估长提示时，首字节前通常需要 30–120 s）超限后以如下错误失败：

```
timed out waiting for "http://localhost:8080/v1/chat" (raise ALMIDE_HTTP_TIMEOUT_SECS; 0 = no timeout)
```

```sh
ALMIDE_HTTP_TIMEOUT_SECS=300 ./app   # five minutes
ALMIDE_HTTP_TIMEOUT_SECS=0 ./app     # wait forever
```

同一变量也限制**连接**（#2825）：没有应答的连接尝试会在 30 s（Go dialer 的默认值）后放弃，而非使用 OS 默认值（macOS 为 75 s，Linux 更长），错误文本相同。

#### 各路径的客户端限制

| | 连接 | 两次读取之间 | 整个调用 | 响应大小 |
|---|---|---|---|---|
| `get` / `request` / `get_response` / `send` | 30 s | 30 s | 无 | 1 GiB |
| `http.start` | `total_ms` | `idle_ms` | `total_ms` | 无 |

- 30 s 默认值由 `ALMIDE_HTTP_TIMEOUT_SECS` 控制（`0` = 无）。
- 大小上限按实际接收的响应计算（头部和带分帧的正文）：`ALMIDE_HTTP_MAX_RESPONSE_BYTES` 可覆盖，`0` = 无上限。超限时调用为 `err("response from \"<url>\" is larger than N bytes (raise ALMIDE_HTTP_MAX_RESPONSE_BYTES; 0 = no limit)")`。
- 每条路径都拒绝超过 1 MiB 的响应 HEAD（Go 默认值）。
- 没有默认的全调用期限，curl、Go、Python、Node、reqwest 都没有。需要时通过 `http.start` 和 `total_ms` 指定。

#### 错误文本（ADR-0023 §4.2）

每个客户端失败都会归类，每类只有一种文本。文本显示程序传入的 URL（加引号，并使用 `\"`、`\\`、`\t`、`\r`、`\n`、`\0` 和 `\u{..}` 转义），从不显示 OS 错误编号，因此同一失败在 native、`almide run --target wasm` 和标准运行时上的 WASI 0.3 组件（`wasmtime run -S http=y`）中读起来相同。

userinfo 密码永不出现：从 userinfo 的第一个 `:` 到 `@` 之间全部显示为 `***`，包括空密码，与 Go net/http 的输出一致（`http://alice:s3cret@host/p` 显示为 `"http://alice:***@host/p"`）。用户名、主机、端口、路径、查询和片段会按原样加引号，因此查询字符串中的凭据会进入错误文本。请改用头部发送凭据（CWE-598）。

| 类别 | 文本 |
|---|---|
| invalid-url | `invalid URL "<url>": <what is wrong>` |
| refused-header | `invalid header name "<name>": …` / `invalid header value for "<name>": …` / `forbidden header name "<name>": …` |
| dns | `cannot resolve the host of "<url>"` |
| connect | `could not connect to "<url>" (connection refused or unreachable)` |
| timeout | `timed out waiting for "<url>" (raise ALMIDE_HTTP_TIMEOUT_SECS; 0 = no timeout)` |
| tls | `TLS handshake with "<url>" failed` |
| too-large | `response from "<url>" is larger than <n> bytes (raise ALMIDE_HTTP_MAX_RESPONSE_BYTES; 0 = no limit)` |
| protocol | `malformed or incomplete response from "<url>"` |

代理自身的失败和无法加载的信任存储保留自己的文本（会点明代理或变量）。组件中，这些类别之外的宿主错误显示为 `http request to "<url>" failed (<wasi:http case>)`，宿主的 `internal-error` 文本直接传递。组件从自己的环境（`wasmtime run --env …`）读取 `ALMIDE_HTTP_TIMEOUT_SECS` 和 `ALMIDE_HTTP_MAX_RESPONSE_BYTES`；在那里，超时 `0` 保留宿主默认值。URL 原因中，组件自己识别三种协议方案问题（缺失、不支持、`expected "//"`）；错误主机或端口由运行时拒绝，显示为未分类文本。

#### URL（#2821）

URL 按 curl、Go 和 WHATWG URL 标准的方式拆分：

- 协议方案不区分大小写，必须为 `http://` 或 `https://`。缺少方案或使用其他方案都是错误，绝不会猜测为明文 HTTP 后发送。
- 除非传入自己的 `Authorization` 头，`user:password@` 会转成 `Authorization: Basic …`。userinfo 绝不视为主机。WASI 0.3 组件发送相同字段值。
- IPv6 主机使用方括号：`http://[::1]:8080/`。
- 端口范围为 1–65535。不可解析的端口是错误，绝不静默改成 80。
- 国际化主机以 punycode 发送（`bücher.example` → `xn--bcher-kva.example`）。
- `http://host:8080?x=1` 是在端口 8080 上发送 `GET /?x=1`。
- 不发送 `#fragment`。
- 路径和查询中的空格、控制字符、非 ASCII 内容使用百分号编码。
- 若端口不是该协议方案的默认端口，`Host` 会带上端口。

被拒绝的 URL 以 `err` 返回，文本为 `invalid URL "…": <what is wrong>`，在任何连接尝试之前返回。`http.start` 同步返回它。

#### 头部注入（#2822）

头名称必须是 token（RFC 9110）。头值不得包含 CR、LF、NUL 或其他控制字符（HTAB 可以）。方法也必须是 token。客户端或运行时管理的字段——`connection`、`keep-alive`、`proxy-authenticate`、`proxy-authorization`、`proxy-connection`、`transfer-encoding`、`upgrade`、`host`、`http2-settings`——无论大小写都不能设置（C-370；标准 WASI 运行时也拒绝这九项）。否则请求会在发送任何内容前被拒绝：调用是以 `invalid header value for "X-A": …`、`invalid header name …`、`forbidden header name "Host": …` 或 `invalid HTTP method …` 开头的 `err`。绝不会清理请求后仍然发送。Python、Go 和 Node 拒绝相同输入。

#### 代理（#2819）

每条客户端路径（`get`、`request`、`get_response`、`send`、`http.start`）都按 curl 和 reqwest 的方式读取代理变量：

- `https://` URL 使用 `HTTPS_PROXY` / `https_proxy`。
- `http://` URL 使用 `HTTP_PROXY` / `http_proxy`。设置了 `REQUEST_METHOD` 时会忽略 `HTTP_PROXY`（CGI、httpoxy），与 Go 和 reqwest 一样。
- 两者随后都回退到 `ALL_PROXY` / `all_proxy`。
- `NO_PROXY` / `no_proxy` 是逗号分隔列表。`*` 为所有主机绕过代理。IP 或 CIDR 块（`10.0.0.0/8`）匹配地址主机。名称（`example.com`、`.example.com`）匹配自身及其子域。默认没有豁免，包括 `localhost`（与 curl 和 reqwest 一样）。

代理协议方案：

- `http://` 代理。不带方案的代理按 `http://` 处理。`https://` 目标通过 `CONNECT host:port` 建立隧道，到源站的 TLS 在隧道内运行。`http://` 目标以绝对形式发送给代理。
- `socks5://`（本地解析名称）和 `socks5h://`（代理解析名称）。默认端口为 1080。

代理 URL 中的 `user:password@` 是代理登录信息：HTTP 代理使用 `Proxy-Authorization:
Basic`，SOCKS5 使用 RFC 1929。任何其他代理方案都会报错并指出变量，绝不静默直接连接。错误会指出代理（`proxy 127.0.0.1:3128 (from HTTPS_PROXY) refused CONNECT
api.example.com:443: HTTP/1.1 407 …`），从不重复其凭据。

#### TLS 信任（#2820）

HTTPS 信任两组根证书：

- 随附的 webpki（Mozilla）根证书；
- 通过 `rustls-native-certs` 读取的平台信任存储（macOS 钥匙串、Windows 存储、Linux 上的 OpenSSL 证书包/目录）。设置了 `SSL_CERT_FILE` 或 `SSL_CERT_DIR` 时，使用它们指定的 PEM 证书取代平台存储。

因此，可以像 curl、Python 和 Go 允许的那样信任企业 CA、测试 CA 或 TLS 检查代理的 CA。若变量未指向可加载证书，错误会说明这一点（`TLS error: could not load the CA
certificates SSL_CERT_FILE names: …`）。证书或握手失败显示为 tls 类别，而非写入失败：

```
TLS handshake with "https://internal.example/" failed
```

#### 中间响应与分块扩展（#2824）

- 所有路径都跳过最终响应之前的 `1xx` 响应（`100 Continue`、`103 Early Hints`）。`101` 是最终响应。
- 分块大小行中的 `;extension` 被忽略。大小行不是十六进制时是错误（protocol 类别，`malformed or incomplete response from
  "<url>"`），而不是空正文。

### 完整响应 — `get_response` 与 `send`

`http.get_response(url)` 和 `http.send(req)` 返回整个 `HttpResponse`，而非正文文本。两者与 `get` / `request` 来自运行时中的同一次通信，因此形态不会漂移：`get` 是 `get_response` 的 `body`，`request(method, url, body, headers)` 是 `send(new_request(method, url, body, headers))` 的 `body`。

响应包含：

- `http.status(resp)` — **任何**完整响应都是 `ok`，包括 404 和 3xx。`err` 仅表示传输失败（连接 / TLS / 超时）。
- **绝不跟随重定向。** 3xx 连同 `Location` 头原样到达，所以响应始终针对传入的 URL，没有单独的“最终 URL”。
- 头部保留线上拼写，重复字段保留**每次**出现：`http.header(resp, k)` 返回第一个，`http.header_values(resp, k)` 返回全部，`http.headers(resp)` 按线上顺序返回每个 `(name, value)` 对。
- `http.body(resp)` — 经传输解码后的正文文本，有损 UTF-8。

在 wasm 上，`almide run --target wasm` 和 WASI 0.3 组件（`ALMIDE_COMPONENT_P3=1 --component`，在 wasmtime 46+ 上以 `-S http` 运行）提供完整响应客户端；默认标准产物在构建时拒绝它。`proofs/target-availability.toml` 声明了这些分支。

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

发送由 `http.new_request` 构建、目标为完整 URL 的请求，返回任何状态的完整响应。不跟随重定向，所以重定向检查直接从响应读取 3xx 和 `Location`：

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

支持任意方法；用 `header_values` 读取重复字段的每个值：

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

请求是一个值，因此可以在发送前逐步构建：

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

## 逐次调用限制与取消

调用可持有为句柄 `HttpCall`，而不必阻塞直到结束。`http.start` 立即返回；运行时线程执行通信，程序继续运行自己的循环——读按键、重绘、发起第二个请求。

| | |
|---|---|
| `http.start(req, limits) -> Result[HttpCall, String]` | 开始发送 `req`（由 `http.new_request` 构建，目标为完整 URL），并立即返回 |
| `http.poll(c) -> Option[Result[HttpResponse, String]]` | 从不阻塞：调用运行时为 `none`，之后为结果 |
| `http.read_new(c) -> String` | 从不阻塞：上次 `read_new` 之后到达的正文文本 |
| `http.wait(c) -> Result[HttpResponse, String]` | 阻塞到调用结束，绝不超过其限制 |
| `http.cancel(c) -> Unit` | 关闭连接；调用以 `err("request cancelled")` 结束 |

**家族规则**（由 `tests/http_call_family_gate_test.rs` 进行机器检查）：句柄恰好具有这五个操作。流式正文在 `poll` 之间用 `read_new` 读取；没有回调式客户端。

**限制逐次调用设置。** `HttpLimits = { total_ms: Int, idle_ms: Int }`，两者都以毫秒计，`0` = 无限制：

- `total_ms` 是从 `http.start` 开始的墙钟时间：连接、等待首字节和正文都计入。服务器即使持续发送，也会到时停止。运行时线程和每次 `poll` / `read_new` / `wait` 都会检查它。
- `idle_ms` 是两次字节到达之间的最长间隔；等待首字节也算间隔。

限制触发后，调用以指出该限制的错误结束：

```
request timeout: total_ms 5000 exceeded
request timeout: idle_ms 60000 exceeded
```

`ALMIDE_HTTP_TIMEOUT_SECS` 不适用于带限制的调用，仍只是无显式限制调用的默认值。所以“此 LLM 调用可耗时 30 分钟，此页面获取为 10 秒”可在同一进程中用两个带不同限制的 `start` 调用表达。

**调用结束就关闭连接。** 调用无论因响应、失败、超时还是取消而结束，运行时都会关闭套接字，服务器立即看到关闭，不再有数据到达。`cancel` 后，`read_new` 返回 `""`（到达但未读取的字节被丢弃），`poll` / `wait` 返回 `err("request cancelled")`。对已结束调用执行 `cancel` 不改变任何内容，调用两次也可以。丢弃句柄的最后一份副本也会取消调用。

**答案是完整响应。** `wait` 和 `poll` 与 `http.send` 一样：任何完整响应都是 `ok`，包括 404，带状态、每个头字段行和整个正文；`err` 表示传输失败、触发限制或取消。`read_new` 已交出的正文字节仍在该响应正文中。跨两次读取的多字节字符会暂存，直到完整。

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

流式 POST——发送 server-sent events 的 LLM 端点——也是同样的循环，只是请求带有正文和头部：

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

（OpenAI 和 Anthropic 流式客户端在 dialect epoch 14 移到了 [almide/llm](https://github.com/almide/llm) 包：`almide add almide/llm@v0.1.0`、`import llm`，然后使用 `llm.openai_streaming_call(...)`。）

在 wasm 目标上，`almide run --target wasm` 以与 native 相同的行为提供 `start`、`poll`、`read_new`、`wait` 和 `cancel`：嵌入式宿主运行 native 调用核心，因此限制和取消错误的文本相同，丢弃句柄的最后一份副本也会取消调用。通过 `almide build --target wasm` 构建的独立 `.wasm` 尚无法支持它们（没有可承载进行中调用的标准 WASI 宿主），因此检查构建路径的 `almide check --target wasm` 仍会拒绝它们。


## 固定快照 API 索引（36 个函数）

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

## 固定快照类型索引（4 个类型）

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



## API 与类型说明译文（按上述索引顺序）

- `effect http.serve(port: Int, handler: HttpHandler) -> Unit` — 持续在 0.0.0.0:port 提供服务；处理器 err 为 500。
- `http.response(status: Int, body: B) -> HttpResponse` — 携带正文响应；其类型决定 Content-Type（String: text/plain; charset=utf-8）。
- `http.json(status: Int, body: String) -> HttpResponse` — 以 Content-Type application/json 响应；正文是 JSON 文本。
- `http.with_headers(status: Int, body: B, headers: List[(String, String)]) -> HttpResponse` — 携带正文响应，在其 Content-Type 之后按给定顺序放入头部；同名重复时保留首次拼写和最后一个值。
- `http.with_body(resp: HttpResponse, body: B) -> HttpResponse` — 将 resp 的正文替换为 body，Content-Type 替换为 body 的类型。
- `http.typed(media_type: String, body: Bytes) -> TypedBody` — 媒体类型为 media_type 的正文；类型必须是字面量，不能是可运行脚本或标准库拥有的类型。
- `http.redirect(url: String) -> HttpResponse` — 302，Location 为 url，空正文，Content-Type 为 text/plain; charset=utf-8。
- `http.set_status(resp: HttpResponse, code: Int) -> HttpResponse` — 为 resp 设置 code；不检查范围。
- `http.status(resp: HttpResponse) -> Int` — HTTP 状态码；状态行不合法时为 0。
- `http.header(m: M, name: String) -> Option[String]` — 头 name 的第一个值，不区分大小写；缺失时为 none。
- `http.header_values(m: M, name: String) -> List[String]` — 按线上顺序返回头 name 的每个值，不区分大小写；缺失时为 []。
- `http.headers(m: M) -> List[(String, String)]` — 每个头作为 (name, value) 返回，保持线上拼写和顺序，保留重复项。
- `http.body(m: M) -> String` — 正文文本；获取的正文以有损 UTF-8 解码。
- `http.body_bytes(m: M) -> Bytes` — 正文的字节：获取的正文与发送的完全相同，构建的正文则为 body(m) 的 UTF-8 字节。
- `http.set_header(m: M, name: String, value: String) -> M` — 为 m 设置头 name，不区分大小写：原位替换第一个匹配项，否则追加；响应拒绝 Content-Type。
- `http.set_body(m: M, body: String) -> M` — 替换 m 的正文；响应的 Content-Type 变为 text/plain; charset=utf-8。
- `http.set_body_bytes(m: M, body: Bytes) -> M` — 将 m 的正文替换为按有损 UTF-8 解码的 body；响应的 Content-Type 变为 application/octet-stream。
- `http.method(req: HttpRequest) -> String` — 发送时的请求方法，例如 GET。
- `http.path(req: HttpRequest) -> String` — 请求目标，包含查询字符串（为发送构建的请求中是完整 URL）。
- `http.query_params(req: HttpRequest) -> Result[List[(String, String)], String]` — 对目标中第一个 ? 之后的部分调用 url.parse_query；各对按顺序排列；错误转义返回 err。
- `effect http.get(url: String) -> String` — 即使 404 也返回正文；仅传输失败为 err。
- `effect http.request(method: String, url: String, body: String, headers: List[(String, String)]) -> String` — 任意方法的响应正文；任何状态都是 ok。
- `effect http.get_response(url: String) -> HttpResponse` — 完整响应，包含 404；不跟随重定向。
- `effect http.send(req: HttpRequest) -> HttpResponse` — 将 req 发送到其目标指定的 URL；返回任何状态的完整响应；不跟随重定向。
- `effect http.start(req: HttpRequest, limits: HttpLimits) -> HttpCall` — 开始发送 req；立即返回句柄。
- `effect http.poll(c: HttpCall) -> Option[Result[HttpResponse, String]]` — 从不阻塞：运行时为 none，之后为结果。
- `effect http.read_new(c: HttpCall) -> String` — 自上次 read_new 起的正文文本；从不阻塞。
- `effect http.wait(c: HttpCall) -> HttpResponse` — 阻塞到调用结束；任何状态都是 ok。
- `effect http.cancel(c: HttpCall) -> Unit` — 关闭连接；调用以 err 结束。
- `http.new_request(method: String, target: String, body: String, headers: List[(String, String)]) -> HttpRequest` — 在代码中构建的请求；目标保留 ?query（send / start 使用完整 URL）。
- `http.param(req: HttpRequest, name: String) -> Option[String]` — 路由器绑定并解码的路径参数；未绑定时为 none。
- `http.route(pattern: String, handler: HttpHandler) -> HttpRoute` — 将 "GET /users/{id}" 转为路由；无方法表示任意方法。
- `http.mount(prefix: String, sub: HttpHandler) -> HttpRoute` — 前缀下的子应用：/api/x 以 /x 到达它。
- `http.router(xs: List[HttpRoute]) -> Result[HttpHandler, String]` — 将路由表作为处理器；路由冲突时为 err。
- `http.wrap(handler: HttpHandler, middleware: List[HttpMiddleware]) -> HttpHandler` — 应用中间件；第一个位于最外层。
- `http.decode_json(req: HttpRequest) -> Result[T, HttpResponse]` — 严格将 JSON 正文解码为输入类型 T；err 是 400 响应。
- `type http.HttpLimits = { total_ms: Int, idle_ms: Int }` — 逐次调用的限制，单位 ms；0 = 无限制。
- `type http.HttpHandler = (HttpRequest) -> Result[HttpResponse, String]` — 从请求到响应；路由器也是此类型。
- `type http.HttpMiddleware = (HttpHandler) -> HttpHandler` — 包装处理器：(next) => (req) => … next(req)! …。
- `type http.HttpRoute = { method: String, pattern: String, segments: List[String], handler: HttpHandler }` — 路由表行；用 route 或 mount 构建。
