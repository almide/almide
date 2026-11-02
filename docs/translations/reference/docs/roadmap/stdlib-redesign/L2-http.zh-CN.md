> 描述（固定历史）：标准库重设计 L2/L3 — http：client 成为完整的 2×2（GET 捷径/任意方法×正文/完整响应）加调用句柄，其他形态通过访问器获得（#3854）；status 改为 set_status，url_decode 迁往 url，并裁决 #2827 默认行为。
<a id="l2l3-http"></a>

# L2/L3: http

[![English](https://img.shields.io/badge/English-d0d7de?style=flat-square)](L2-http.md) [![日本語](https://img.shields.io/badge/%E6%97%A5%E6%9C%AC%E8%AA%9E-d0d7de?style=flat-square)](L2-http.ja.md) [![简体中文](https://img.shields.io/badge/%E7%AE%80%E4%BD%93%E4%B8%AD%E6%96%87-24292f?style=flat-square)](L2-http.zh-CN.md)

> 非规范性历史重设计参照版。固定原本权威且不变。不自动同步、重取官方页面或独立进行runtime/network测试。实测与(F)为原作者报告。保留早期2x2保留request_response与后来send/start统一撤除、头部map与后来保留重复的list、req_三件组与泛型HttpMessage、普通query列表与带日期Result修正。status设置/读取复用处于不同epoch。stock p1无HTTP，暂定内置host op不是stock WASI。URL解码兼容及status_code表交叉引用为原文主张，不擅自修正或重新验证。提案安全性/runtime行为并非当前认证。 [Source](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/stdlib-redesign/L2-http.md)

> 最后更新：2026-10-21
> 前提：[L0](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/stdlib-redesign/L0-conventions.md) / [L1](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/stdlib-redesign/L1-categories.md)（R-L1-28：URL 编码属于 `url`）。
> 来源：Rust https://docs.rs/reqwest（标准库无 HTTP，仅参考）/ Go https://pkg.go.dev/net/http / Python https://docs.python.org/3/library/urllib.request.html / TS/JS https://developer.mozilla.org/en-US/docs/Web/API/Fetch_API / Gleam https://hexdocs.pm/gleam_httpc/（官方 org 的包）/ Elixir https://www.erlang.org/doc/apps/inets/httpc.html / OCaml —（标准库无）/ Kotlin https://ktor.io/docs/client-requests.html（官方包）/ Swift https://developer.apple.com/documentation/foundation/urlsession
> 现有提案：#3854 §2（client 23 函数）、#2827（默认行为）、#2828（追踪）；ADR-0020（serve）、ADR-0023（wasm HTTP 为 `wasi:http@0.3`）。

<a id="l2-役割の比較"></a>

## L2：角色比较

| 角色 | Almide | Rust(reqwest) | Go | Python | TS/JS | Gleam(httpc) | Elixir(httpc) | OCaml | Kotlin(Ktor) | Swift |
|---|---|---|---|---|---|---|---|---|---|---|
| GET 捷径（正文） | `get` | `reqwest::get`+`text` | `http.Get` | `urlopen(u).read()` | `fetch(u).then(r => r.text())` | `httpc.send` | `:httpc.request(url)` | — | `client.get(u).bodyAsText()` | `URLSession.data(from:)` |
| 各动词捷径 | `post`・`put`・`patch`・`delete` | `Client::post` 等 | `http.Post`・`Head` | `Request(method=)` | `fetch(u, {method})` | `request.set_method` | `request(:post, …)` | — | `client.post` 等 | `httpMethod =` |
| 任意方法交换 | `request`（正文）・`request_response`（完整响应）・`request_status`・`request_bytes` | `Client::request` | `Client.Do` | `urlopen(Request)` | `fetch(Request)` | `httpc.send` | `:httpc.request/4` | — | `client.request` | `data(for:)` |
| 响应状态/头部/正文 | `status_code`・`get_header`・`header_values`・`headers`・`body` | `status`・`headers`・`text`・`bytes` | `StatusCode`・`Header.Get`・`Body` | `status`・`getheader`・`read` | `status`・`headers.get`・`text`・`arrayBuffer` | `status`・`get_header`・`body` | 元组 | — | `status`・`headers`・`bodyAsText` | `statusCode`・`value(forHTTPHeaderField:)` |
| 字节正文 | `get_bytes`・`request_bytes` | `bytes` | `io.ReadAll(Body)` | `read()` | `arrayBuffer` | `BitArray` 版 | `body_format: :binary` | — | `bodyAsBytes` | `Data` |
| 流式接收 | `request_stream`・`request_stream_with_limits` | `bytes_stream` | `Body.Read` | `read(n)` | `body.getReader()` | — | `stream:` | — | `bodyAsChannel` | `bytes(for:)` |
| 非阻塞查看状态的句柄 | `start`・`poll`・`read_new`・`wait`・`cancel` | `Future` | goroutine | — | `Promise`・`AbortController` | — | `sync: false` | — | coroutine | `Task` |
| 服务器 | `serve`・`route`・`router`・`mount`・`wrap`・`param` | — | `ListenAndServe`・`ServeMux` | `http.server` | `Deno.serve` 等 | — | `:httpd` | — | Ktor server | — |
| 构造响应 | `response`・`json`・`with_headers`・`redirect`・`status`（设置）・`set_header` | — | `WriteHeader`・`Header().Set` | `send_response` | `new Response(body, {status})` | `response.new` | — | — | `call.respond` | — |
| 读取请求 | `req_method`・`req_path`・`req_body`・`req_header`・`query_params`・`decode_json` | — | `r.Method`・`r.URL`・`r.Header.Get`・`r.Header.Values` | `self.path`・`headers` | `req.method`・`req.headers` | — | — | — | `call.request` | — |
| URL 解码 | `url_decode` | `url` crate | `url.QueryUnescape` | `unquote_plus` | `decodeURIComponent` | `uri.percent_decode` | `URI.decode` | — | — | — |

<a id="完全性の規則"></a>

### 完备性规则

- **client 为 2×2 矩阵加句柄。** 纵轴 {GET 捷径、任意方法}，横轴 {正文字符串、完整响应 `HttpResponse`}。四格为 `get`、`get_response`、`request`、`request_response`。其他形态（状态元组、字节、头部）通过 `HttpResponse` 访问器构造。不等待、流式接收、时间上限只走一条句柄路径：`start`、`poll`、`read_new`、`wait`、`cancel`。
- **只有一条状态规则。** 所有 client 对完整收到的响应都返回 ok（包括 404、3xx），只有通信失败是 err。
- **响应访问器**（主语 `HttpResponse`）：`status_code`、`body`、`body_bytes`、`get_header`、`header_values`、`headers`。
- **请求访问器**（主语 `HttpRequest`；`req_` 避免与同模块响应函数冲突）：`req_method`、`req_path`、`req_body`、`req_header`/`req_header_values`/`req_headers`（同响应三件组）、`query_params`、`param`。
- **响应修改函数为副本式 `set_`**（R-L0-04）：`set_status`、`set_header`。
- URL 编解码属于 `url`（R-L1-28）。

<a id="l2-の判定"></a>

### L2 判定

- 轴乘积与缺口：动词×形态（正文/元组/全部/字节）×上限，三个轴形成 23 函数，有 `post_status`、`post_bytes` 等可推测却不存在的格（#3854 §2）→ R-http-01。
- 状态规则例外：仅 `request_stream` 将非 2xx 视为 err（#3854）→ R-http-01（撤除）。
- 命名陷阱：`http.status(resp, code)` 是设置函数，却与读取的 `status_code` 相近 → R-http-02。
- 族缺口：请求头没有“全部值”和“map” → R-http-03。
- 重复：`http.url_decode` 是表单解码，属于 `url`（与 `url.decode_component` 对 `+` 的处理不同，实测见下）→ R-http-04。
- 默认行为（#2827）→ R-http-05。
- 哨兵：无法读取状态行时 `status_code` 为 `0`（R-L0-06 第 4 项）→ R-http-01。

<a id="案"></a>

## 提案

<a id="r-http-01-client-を-22--ハンドルにまとめる3854-2---採用"></a>

### R-http-01 client 统一为 2×2 加句柄（#3854 §2）— ○ 采纳

- 内容：
  - 保留 `get(url) -> String`、`get_response(url) -> HttpResponse`、`request(method, url, body, headers) -> String`、`request_response(method, url, body, headers) -> HttpResponse` 及五个句柄函数。
  - 新增 `fn body_bytes(resp: HttpResponse) -> Bytes`（不转换为 UTF-8 的正文）。
  - 撤除 14 函数及替换：
    - `post(u, b)` → `request("POST", u, b, [:])`（`put`/`patch`/`delete` 同形）。
    - `post_response(u, b)` → `request_response("POST", u, b, [:])`（`put_`/`patch_`/`delete_response` 同形）。
    - `get_status(u)` → 从 `get_response(u)` 提取 `status_code` 与 `body`；`request_status(…)` 同理由 `request_response(…)` 提取。
    - `get_bytes(u)` → `body_bytes(get_response(u)!)`；`request_bytes(…)` → `body_bytes(request_response(…)!)`。
    - `request_stream`/`request_stream_with_limits` → `start` + `read_new` + `poll`/`wait`。
  - 无法读取状态行时，交换本身为 err，而非 `status_code = 0` 的响应（消除哨兵，R-L0-06 第 4 项）。
- 与 #3854 的差别：该案是一个 GET 捷径加一个交换（8 函数）；本案保留 `get_response` 和正文版 `request`，补全 2×2。理由三点：(1) CHEATSHEET 所教 `http.get`、`http.request` 都不用删除。(2) 矩阵无缺口，“`get` 有 `_response`，则 `request` 也有”的推测都成立。(3) 撤除侧均能一对一映射到保留侧，`almide fix` 可机械修正，不会把 `request` 改成不同返回类型（R-L0-14 第 4 项）。
- L0 依据：R-L0-01（同一交换只有一种拼写）、CLAUDE.md“族按矩阵展开”、R-L0-14 第 4 项。
- (a) Go/Python/JS（`fetch` 默认）/Ktor/Swift 有 GET 捷径，也都有任意方法交换的单一函数。动词捷径有 Go（仅 `Get`/`Post`/`Head`）、Ktor，但没有语言按状态/字节形态拆函数名，都是从响应值提取。
- (b) wasm（ADR-0023）：stock p1 无 HTTP import，p3 component 通过 `wasi:http/client@0.3.0` 传输。内置 host 当前用暂定 op 43–50/53–59（账本 `pending-port` 行，step 10 退役）。ADR-0023 step 4 是将六个 `*_response`、`request_stream`、句柄纳入 shim。本案剩 4 + 5，shim 传输函数从 23 减为 9。step 5 已展示 `request_stream` 的 SSE 可用 Almide 在句柄上编写。
- (c) 可推测却不存在的名称（如 `post_status`）消失，所有 client 状态规则一致。
- (d) 撤除与新增。各函数加 `@deprecated(since=15, use="…")`（`use` 为上述替换目标）、`removal_epoch = 16`。上述一对一形式由 `almide fix` 改写（仅 `request_stream` 族形态变化，需提示句柄写法）。消除 `status_code` 的 `0` 不改变正确读取响应的结果，因此不计 epoch `breaks`。使用数（spec/tests/docs/stdlib）：`post` 2/1/13/0、`put` 1/1/8/0、`patch` 1/2/7/0、`delete` 1/1/8/0、`get_status` 6/4/6/1、`request_status` 5/5/5/1、`post_response` 0/1/6/1、`put_response` 0/2/6/1、`patch_response` 0/2/6/1、`delete_response` 0/1/6/1、`get_bytes` 2/6/5/1、`request_bytes` 2/7/5/1、`request_stream` 0/14/3/1、`request_stream_with_limits` 2/8/5/0。
- gate：将 `tests/http_response_family_gate_test.rs` 改为 2×2 检查（恰好四格、参数排列成对一致）。`http_call_family_gate_test.rs` 的 `_with_limits` 规则改为“只用句柄流式接收”，因为流式函数消失。所有 client 对同一测试服务器做状态/正文/err 分类相同的差分测试（#3854 提案）。
- Issue：#3854（既有，按本案定案）。

<a id="r-http-02-httpstatus-を-httpset_status-にする---採用"></a>

### R-http-02 将 `http.status` 改为 `http.set_status` — ○ 采纳

- 内容：新增 `fn set_status(resp: HttpResponse, code: Int) -> HttpResponse`，撤除 `http.status`。
- 拼写：同模块改写响应副本的函数为 `set_header`，R-L0-04 规定 `set` 是副本动词。不采用 #3854 草案的 `with_status`，因为与构造响应的 `with_headers`、`list.with_capacity` 同以 `with_` 起头，混淆创建和修改。
- L0 依据：R-L0-01、R-L0-04 第 3 项。
- (a) Go `WriteHeader`、JS `new Response(body, {status})`、Ktor `call.respond(status)`，拼写不一。
- (b) 纯函数。(c) 写 `http.status(resp)` 想读取状态，会因其为设置函数而产生参数数量错误。
- (d) 改名：`@deprecated(since=15, use="http.set_status")`、`removal_epoch = 16`，`almide fix` 一对一改写。使用数 11/6/9/0。
- Issue：#3854（既有）。

<a id="r-http-03-要求の側のヘッダを応答と同じ-3-つ組にする---採用"></a>

### R-http-03 请求头与响应头组成相同三件组 — ○ 采纳

- 内容：新增 `fn req_header_values(req: HttpRequest, key: String) -> List[String]`（全部值，抵达顺序）、`fn req_headers(req: HttpRequest) -> Map[String, String]`（名称小写的 map，首值）。
- 依据：CLAUDE.md“族按矩阵”。响应有 {首值、全部、map} 三项，请求只有首值。
- (a) Go `Header.Get`/`Header.Values`、JS `headers.get`/`headers.entries`，双向同形。
- (b) 纯函数（在 `http.serve` handler 内使用）。
- (d) 新增，使用数 0。
- gate：确认响应/请求头函数形成相同三件组的族检查。
- Issue：#3917。

<a id="r-http-04-httpurl_decode-を撤去するr-l1-28---採用"></a>

### R-http-04 撤除 `http.url_decode`（R-L1-28）— ○ 采纳

- 内容：撤除 `http.url_decode(s)`，替换为 `url.decode_form(s)`（[L2-url](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/stdlib-redesign/L2-url.md) R-url-01 新增的表单解码）。
- 实测（probe-http）：`http.url_decode("a+b%20c%zz")` 得到 `a b c%zz`（`+` 变空格、损坏的 `%` 保持、返回 `String`）。将 `+` 变空格是表单解码，与 `url.decode_component`（保留 `+`、损坏 `%` 时 err）不同，故目标为 `url.decode_form`。
- L0 依据：R-L0-01、R-L0-02（URL 字符串函数属于 `url`）、R-L0-06 第 4 项（不以哨兵隐藏失败）、R-L1-28。
- (d) 撤除：`@deprecated(since=15, use="url.decode_form")`、`removal_epoch = 16`。`almide fix`：`http.url_decode(s)` → `url.decode_form(s) ?? s`（不合法输入返回原字符串）。只有不合法 UTF-8（`%ff`）不同：由替换字符改为原字符串，在提示中注明。使用数 9/0/9/2。
- Issue：#3950。

<a id="r-http-05-client-の既定の振る舞い2827の判定---採用項目ごとに判定"></a>

### R-http-05 裁决 client 默认行为（#2827）— ○ 采纳（逐项裁决）

| # | #2827 项目 | 判定 | 理由 |
|---|---|---|---|
| 1 | 跟随重定向 | × | 文档约定“3xx 及其 `Location` 就是答案，响应属于所调用的 URL”。跟随后同名结果改变（R-L0-14 第 4 项）。需要跟随时通过 `get_header(r, "location")` 调用下一个 URL。 |
| 2 | 发送 `Accept-Encoding: gzip` 并解压 | ○ | `body` 含义（收到的正文字串）不变。当前压缩数据直接成为不可读的 `String`。解压用标准库 `zlib`，p3 shim 也可共用同一 Almide 函数体。 |
| 3 | 默认发送 `User-Agent: almide/<version>` | ○ | 调用方提供头部时优先采用其值，消除要求 User-Agent 的 API（如 GitHub）返回 403 的问题。 |
| 4 | 有正文时添加 `Content-Type: application/json` | × | 调用方提供 `content-type` 时不添加（`http_route_core.rs` 一个写入处）。改默认值会改变同一调用发送的内容（R-L0-14 第 4 项）。 |
| 5 | 发送 Bytes 正文 | × 当前不新增 | 在 2×2 增加“发送正文类型”第三轴会使交换函数翻倍（CLAUDE.md“族按矩阵”）。将来新增时，在 #2827 重提交换函数接受 `HttpRequest`（由 `http.new_request` 构造）、正文类型集中一处的方案。 |
| 6 | keep-alive 与连接复用 | 范围外 | 函数表面不变，作为 runtime 内部改进在 #2827 推进。 |

- #2828 表中项目（代理、信任库、URL 分解、头部换行、服务器上限）均为不改变函数表面的 runtime 正确性问题，本页不裁决。
- (b) 第 2、3 项在 native `http_client_core.rs` 与 p3 shim 采用相同规则（ADR-0023 §4“语义只在一处”）。
- Issue：#2827（既有）。

<a id="r-http-06-応答の本文を差し替える-httpset_body-を足す---採用"></a>

### R-http-06 新增替换响应正文的 `http.set_body` — ○ 采纳

- 提案：`fn http.set_body(resp: HttpResponse, body: String) -> HttpResponse`。响应组成 {状态、头部、正文} 各具一个 `set_`。
- (a) Gleam `response.set_body` (F)。
- (c) middleware（`http.wrap`）改正文时，目前只能用 `with_headers` 与 `headers()` 重建，`set-cookie` 等多值头会静默消失。
- (d) 新增 1 个（公开内部 `__set_body`）。
- Issue：#3974。

<a id="r-http-07-応答のヘッダの-get_header-と要求の-req_header-の不揃い---不採用規則を書く-r-http-12-で解消"></a>

### R-http-07 响应 `get_header` 与请求 `req_header` 不一致 — × 否决（写明规则；→ R-http-12 解决）

- 理由：读取单个头部常含 get，如 Gleam `get_header` (F)、Go `Header.Get`、JS `headers.get`，改名 `header` 收益小。改为在完备性规则中写明：“仅与响应名称冲突时加 `req_`”；“`get_` 有 `get`/`get_response`（HTTP GET）与 `get_header`（提取）两种含义”。
- Issue：无。

<a id="r-http-08-取り出しの-status_code-を-status-にする---採用epoch-17"></a>

### R-http-08 读取用 `status_code` 改为 `status` — ○ 采纳（epoch 17）

- 现状：读取 `http.status_code(resp) -> Int` 与设置 `http.set_status(resp, code)` 词干不同。旧设置 `http.status(resp, code)` 在 epoch 16 撤除。
- 提案：epoch 17 设读取函数 `fn http.status(resp: HttpResponse) -> Int`，撤除 `status_code`。旧设置含义撤除后隔一个 epoch，缩短与训练数据旧含义混杂的期间。
- (a) JS/Python/Ktor/Gleam/Rust 五种 HTTP 库采用 `status`，Go/Swift 两种采用 `StatusCode`。
- (d) 改名。旧设置 `http.status(r, 404)` 对新读取函数产生参数数量错误，满足 R-L0-14.4。epoch 16 给 `status_code` 添加 `@deprecated(since=16, use="http.status")`，epoch 17 撤除。
- Issue：#3974。

<a id="r-http-09-ヘッダの全体を-liststring-string-で表す---採用"></a>

### R-http-09 全部头部用 `List[(String, String)]` 表示 — ○ 采纳

- 现状：虽有读取同名全部值的 `header_values`/`req_header_values`，但整体读取 `headers`/`req_headers` 与传头部构造/调用的 `with_headers`/`new_request`/`request`/`request_response`/`start` 都用 `Map[String, String]`，同名多个头只剩一个。`Set-Cookie` 不能逗号合并（RFC 9110 §5.3），仅仅读出重建就会破坏。
- 提案：整体读写采用 `List[(String, String)]`（抵达/发送顺序，名称小写）。
  - `fn headers(resp: HttpResponse) -> List[(String, String)]`、`fn req_headers(req: HttpRequest) -> List[(String, String)]`。
  - `with_headers`/`new_request`/`request`/`request_response`/`start` 的 `headers` 参数也为 `List[(String, String)]`。
  - 单值 `get_header`、全部值 `header_values`、改写一个头部的 `set_header` 不变。
- L0 依据：R-L0-01（相同信息不设两种形态）。同 `url.parse_query` 以键值对列表保留重复的形态（R-http-10）。
- (a) Go `http.Header`（`map[string][]string`）、JS `Headers`（`append` 保留重复）、Python `http.client` 的 `getheaders()`（键值对列表）、Rust http crate `HeaderMap`（多值）均保留重复。
- (b) native client 与 p3 shim 均原样传递头部序列。
- (c) middleware 读响应后重建，不会丢失 `Set-Cookie`。
- (d) 语义变更（类型改变使旧调用类型错误，不静默改结果）。`almide fix` 插入 `map.from_list`/`map.to_list` 转换。
- 完备性：传递整体头部的参数/返回值全为 `List[(String, String)]`，公开表面不留 `Map[String, String]` 头部。
- Issue：#3976。

<a id="r-http-10-query_params-を-liststring-string-にする---採用"></a>

### R-http-10 `query_params` 改为 `List[(String, String)]` — ○ 采纳

- 现状：`fn query_params(req: HttpRequest) -> Map[String, String]` 丢弃 `?tag=a&tag=b` 的第二项；同查询的 `url.parse_query` 返回 `List[(String, String)]`，保留重复和顺序。
- 提案：`fn query_params(req: HttpRequest) -> List[(String, String)]`，按 `url.parse_query` 同规则解码。
- 追加（2026-10-24，#4117/C-404）：采用与 `url.parse_query` 相同的唯一解码器，因此不合法转义及 UTF-8 返回 err。返回值为 `Result[List[(String, String)], String]`。
- (a) Go `r.URL.Query()`（多值）、Python `parse_qsl`、JS `URLSearchParams.getAll`。
- (d) 语义变更（由类型错误发现）。只要一个时用 `list.find(ps, ((k, _)) => k == "tag")`。
- Issue：#3976。

<a id="r-http-11-本文の-bytes-の族を閉じるreq_body_bytesset_body_bytes---採用"></a>

### R-http-11 补全正文 `Bytes` 族（`req_body_bytes`/`set_body_bytes`）— ○ 采纳

- 现状：响应正文有字符串 `body` 与字节 `body_bytes`，请求只有字符串 `req_body`，响应修改也只有字符串 `set_body`。`body` 将非法 UTF-8 转为替换字符（lossy），服务器收到二进制会损坏。
- 提案：新增 `fn req_body_bytes(req: HttpRequest) -> Bytes`、`fn set_body_bytes(resp: HttpResponse, body: Bytes) -> HttpResponse`。
- 完备性：正文读取/替换函数均有字符串和字节两项（`body`/`body_bytes`、`req_body`/`req_body_bytes`、`set_body`/`set_body_bytes`）。R-http-12 将 `req_` 侧统一为无前缀。字符串侧为 lossy UTF-8，应写明。
- 暂缓：client 发送二进制正文（`request` 的 `body: String`）与请求构造类型设计一并裁定，此处不增加（→ R-http-13 的 `http.send` 解决）。
- (d) 新增 2 个。
- Issue：#3976。

<a id="r-http-12-要求と応答を-httpmessage-で-1-つの関数にする---採用r-http-07-を置き換え"></a>

### R-http-12 通过 `HttpMessage` 将请求/响应统一为泛型函数 — ○ 采纳（替代 R-http-07）

- 现状：同角色响应函数无前缀（`body`/`headers`/`header_values`），请求有 `req_`（`req_body`/`req_headers`/`req_header_values`）。单头读取甚至词干不同，响应 `get_header`、请求 `req_header`。R-http-09 整体改成 `headers`/`req_headers`，留下仅单数带 `get_`。三个外部评审接连指出。
- 提案：两类型遵循 `HttpMessage` protocol（R-L0-31），每种读写角色统一为泛型函数。
  - `fn header[M: HttpMessage](m: M, name: String) -> String?`
  - `fn header_values[M: HttpMessage](m: M, name: String) -> List[String]`
  - `fn headers[M: HttpMessage](m: M) -> List[(String, String)]`
  - `fn body[M: HttpMessage](m: M) -> String`
  - `fn body_bytes[M: HttpMessage](m: M) -> Bytes`
  - `fn set_header[M: HttpMessage](m: M, name: String, value: String) -> M`
  - `fn set_body[M: HttpMessage](m: M, body: String) -> M`
  - `fn set_body_bytes[M: HttpMessage](m: M, body: Bytes) -> M`

  - 撤除 `get_header`（→ `header`）、`req_header`/`req_header_values`/`req_headers`/`req_body`/`req_body_bytes`（→ 无前缀）。
  - 请求也能使用 `set_body_bytes`，可将任意字节设为请求正文（R-http-13）。
- L0 依据：R-L0-31、R-L0-01、R-L0-00。
- (a) Go 的 `Request.Header.Get`、`Response.Header.Get` 为同一 `Header` 类型的同一方法；JS fetch 的 `Request`/`Response` 共享 `Body`、`Headers`。
- (d) 改名/撤除：`@deprecated(since=15, use=…)`、`removal_epoch = 16`，`almide fix` 一对一改写。
- Issue：#3989。

<a id="r-http-13-組み立てた要求を送る-httpsend-を足す---採用"></a>

### R-http-13 新增发送已构造请求的 `http.send` — ○ 采纳

- 现状：client 的 `request`/`request_response`/`start` 请求正文全为 `body: String`，没有发送任意字节的路径。
- 提案：新增 `effect fn http.send(req: HttpRequest) -> HttpResponse`，可写 `http.new_request(…) |> http.set_body_bytes(b) |> http.send`（R-http-12 使请求也可用 `set_body_bytes`）。字符串正文仍可用 `request`/`request_response`。
- (a) Go `client.Do(req)`、Rust reqwest `Client::execute(request)`、JS `fetch(request)`。
- (b) 两目标与 `request_response` 相同路径。
- (d) 新增 1 个。
- Issue：#3989。

<a id="r-http-14-要求を組み立てる道を-1-つにする-startreq-limitsrequest_response-の撤去req_-を外す---採用"></a>

### R-http-14 请求构造路径统一：`start(req, limits)`，撤除 `request_response`，去掉 `req_` — ○ 采纳

- 现状：R-http-13 新增 `send(req: HttpRequest)`，故构造后的请求（如用 `set_body_bytes` 设字节正文）可同步发送；但异步 `start` 仍为五参数 `method, url, body: String, headers, limits`，无法发送字节。完整响应通用拼写也有 `request_response(method, url, body, headers)` 和 `send(new_request(…))` 两种。仅请求使用的 `req_method`/`req_path` 有 `req_`，`param`/`query_params` 无（R-http-12 已从八个共通函数去掉）。
- 提案：
  - 改为 `effect fn http.start(req: HttpRequest, limits: HttpLimits) -> HttpCall`，可直接传入与 `send` 相同的请求。
  - 撤除 `request_response`（→ `send(new_request(method, url, body, headers))`）。保留正文字符串 `request`、GET 捷径 `get`/`get_response`，完整响应的通用路径只有 `send`。
  - `req_method` 改为 `method`，`req_path` 改为 `path`。请求专用函数不加前缀，由参数类型决定主语（R-L0-02：“前缀不是命名空间”）。
- L0 依据：R-L0-01（同角色只有一种拼写）、R-L0-02。
- (a) Go `client.Do(req)` 同步/异步共用同一请求，`r.Method`/`r.URL.Path` 无前缀；JS `fetch(request)` 亦然。
- (b) 调用形态变更，通信内部不变。
- (c) 将同步 `send(req)` 改为异步 `start(req, limits)` 时保持请求参数形态。
- (d) `start` 语义变更（旧五参数调用类型错误），`request_response` 撤除，`req_method`/`req_path` 改名；均 epoch 15 加 `@deprecated`，epoch 16 撤除，`almide fix` 一对一改写。
- Issue：#3996。
<a id="l3-関数表"></a>

## L3：函数表

| 函数 | 签名（当前，显示 mut） | 其他语言主要拼写 | 判定 | 提案 |
|---|---|---|---|---|
| `http.serve` | `effect fn serve(port: Int, f: HttpHandler) -> Unit` | Go `ListenAndServe` / Deno `serve` | 保留 | — |
| `http.response` | `fn response(status: Int, body: String) -> HttpResponse` | JS `new Response` / Ktor `respondText` | 保留 | — |
| `http.json` | `fn json(status: Int, body: String) -> HttpResponse` | JS `Response.json` | 保留 | — |
| `http.with_headers` | `fn with_headers(status: Int, body: String, headers: List[(String, String)]) -> HttpResponse` | JS `new Response(b, {headers})` | 语义变更（`headers` 为 `List[(String, String)]`） | R-http-09 |
| `http.redirect` | `fn redirect(url: String) -> HttpResponse` | JS `Response.redirect` / Go `http.Redirect` | 保留 | — |
| `http.status` | `fn status(resp: HttpResponse, code: Int) -> HttpResponse` | Go `WriteHeader` | 改名（→ `set_status`） | R-http-02 |
| `http.set_status` | `fn set_status(resp: HttpResponse, code: Int) -> HttpResponse`（提案） | 同上 | 新增 | R-http-02 |
| `http.set_header` | `fn set_header(resp: HttpResponse, key: String, value: String) -> HttpResponse` | Go `Header().Set` / JS `headers.set` | 语义变更（`HttpRequest` 也接受） | R-http-12 |
| `http.body` | `fn body(resp: HttpResponse) -> String` | JS `text()` / Go `Body` / Ktor `bodyAsText` | 语义变更（`HttpRequest` 也接受） | R-http-12 |
| `http.body_bytes` | `fn body_bytes(resp: HttpResponse) -> Bytes`（提案） | JS `arrayBuffer` / reqwest `bytes` / Ktor `bodyAsBytes` | 新增（`HttpRequest` 也接受） | R-http-12 |
| `http.status_code` | `fn status_code(resp: HttpResponse) -> Int` | Go `StatusCode` / JS `status` | 改名 → `status`（epoch 17。`0` 哨兵也消除） | R-http-04、R-http-08 |
| `http.get_header` | `fn get_header(resp: HttpResponse, key: String) -> String?` | Go `Header.Get` / JS `headers.get` | 改名 → `header`（`HttpRequest` 也接受） | R-http-12 |
| `http.header_values` | `fn header_values(resp: HttpResponse, key: String) -> List[String]` | Go `Header.Values` | 语义变更（`HttpRequest` 也接受） | R-http-12 |
| `http.headers` | `fn headers(resp: HttpResponse) -> List[(String, String)]` | JS `headers` / reqwest `headers` | 语义变更（`HttpRequest` 也接受） | R-http-12 |
| `http.req_method` | `fn req_method(req: HttpRequest) -> String` | Go `r.Method` / JS `req.method` | 改名 → `method` | R-http-14 |
| `http.req_path` | `fn req_path(req: HttpRequest) -> String` | Go `r.URL.RequestURI()` | 改名 → `path`（明确注明包含查询） | R-http-14 |
| `http.req_body` | `fn req_body(req: HttpRequest) -> String` | Go `r.Body` / JS `req.text()` | 撤除 → `http.body` | R-http-12 |
| `http.req_header` | `fn req_header(req: HttpRequest, key: String) -> String?` | Go `r.Header.Get` | 撤除 → `http.header` | R-http-12 |
| `http.req_header_values` | `fn req_header_values(req: HttpRequest, key: String) -> List[String]`（提案） | Go `r.Header.Values` | 不新增（统一为 R-http-12 的 `http.header_values`） | R-http-12 |
| `http.req_headers` | `fn req_headers(req: HttpRequest) -> List[(String, String)]`（提案） | JS `req.headers` | 不新增（统一为 R-http-12 的 `http.headers`） | R-http-12 |
| `http.query_params` | `fn query_params(req: HttpRequest) -> Result[List[(String, String)], String]` | Go `r.URL.Query()` | 语义变更（`List[(String, String)]` 返回） | R-http-10 |
| `http.url_decode` | `fn url_decode(s: String) -> String` | Go `QueryUnescape` / Python `unquote_plus` | 撤除 → `url.decode_form(s) ?? s` | R-http-04 |
| `http.get` | `effect fn get(url: String) -> String` | Go `http.Get` / reqwest `get` / Python `urlopen` | 保留 | R-http-01 |
| `http.get_response` | `effect fn get_response(url: String) -> HttpResponse` | JS `fetch(u)` / Ktor `client.get(u)` | 保留 | R-http-01 |
| `http.request` | `effect fn request(method: String, url: String, body: String, headers: List[(String, String)]) -> String` | Go `Client.Do` / JS `fetch(u, {method})` | 语义变更（`headers` 为 `List[(String, String)]`） | R-http-09 |
| `http.request_response` | `effect fn request_response(method: String, url: String, body: String, headers: List[(String, String)]) -> HttpResponse` | 同上 | 撤除（→ `send(new_request(method, url, body, headers))`） | R-http-14 |
| `http.post` | `effect fn post(url: String, body: String) -> String` | Go `http.Post` / Ktor `post` | 撤除（→ `request("POST", …)`） | R-http-01 |
| `http.put` | `effect fn put(url: String, body: String) -> String` | Ktor `put` | 撤除（→ `request("PUT", …)`） | R-http-01 |
| `http.patch` | `effect fn patch(url: String, body: String) -> String` | Ktor `patch` | 撤除（→ `request("PATCH", …)`） | R-http-01 |
| `http.delete` | `effect fn delete(url: String) -> String` | Ktor `delete` | 撤除（→ `request("DELETE", …)`） | R-http-01 |
| `http.get_status` | `effect fn get_status(url: String) -> (Int, String)` | — | 撤除（→ `get_response`） | R-http-01 |
| `http.request_status` | `effect fn request_status(method: String, url: String, body: String, headers: Map[String, String]) -> (Int, String)` | — | 撤除（→ `send`） | R-http-01 |
| `http.post_response` | `effect fn post_response(url: String, body: String) -> HttpResponse` | — | 撤除（→ `send(new_request("POST", …))`） | R-http-01 |
| `http.put_response` | `effect fn put_response(url: String, body: String) -> HttpResponse` | — | 撤除（→ `send(new_request("PUT", …))`） | R-http-01 |
| `http.patch_response` | `effect fn patch_response(url: String, body: String) -> HttpResponse` | — | 撤除（→ `send(new_request("PATCH", …))`） | R-http-01 |
| `http.delete_response` | `effect fn delete_response(url: String) -> HttpResponse` | — | 撤除（→ `send(new_request("DELETE", …))`） | R-http-01 |
| `http.get_bytes` | `effect fn get_bytes(url: String) -> Bytes` | reqwest `bytes` | 撤除（→ `body_bytes(get_response(u)!)`） | R-http-01 |
| `http.request_bytes` | `effect fn request_bytes(method: String, url: String, body: String, headers: Map[String, String]) -> Bytes` | — | 撤除（→ `body_bytes(send(new_request(…))!)`） | R-http-01 |
| `http.request_stream` | `effect fn request_stream(method: String, url: String, body: String, headers: Map[String, String], on_chunk: (String) -> Unit) -> Unit` | reqwest `bytes_stream` / JS `body.getReader` | 撤除（→ 句柄） | R-http-01 |
| `http.request_stream_with_limits` | `effect fn request_stream_with_limits(method: String, url: String, body: String, headers: Map[String, String], limits: HttpLimits, on_chunk: (String) -> Unit) -> Unit` | — | 撤除（→ 句柄） | R-http-01 |
| `http.start` | `effect fn start(method: String, url: String, body: String, headers: List[(String, String)], limits: HttpLimits) -> HttpCall` | JS `fetch`（Promise） | 语义变更（`start(req: HttpRequest, limits: HttpLimits)`） | R-http-09、R-http-14 |
| `http.poll` | `effect fn poll(c: HttpCall) -> Result[HttpResponse, String]?` | Python `Popen.poll` 相当 | 保留（`process.poll` 拼写相同） | R-http-01 |
| `http.read_new` | `effect fn read_new(c: HttpCall) -> String` | — | 保留 | R-http-01 |
| `http.wait` | `effect fn wait(c: HttpCall) -> HttpResponse` | — | 保留 | R-http-01 |
| `http.cancel` | `effect fn cancel(c: HttpCall) -> Unit` | JS `AbortController.abort` | 保留 | R-http-01 |
| `http.new_request` | `fn new_request(method: String, target: String, body: String, headers: List[(String, String)]) -> HttpRequest` | JS `new Request` | 语义变更（`headers` 为 `List[(String, String)]`） | R-http-09 |
| `http.param` | `fn param(req: HttpRequest, name: String) -> String?` | Go `r.PathValue` | 保留 | — |
| `http.route` | `fn route(pattern: String, handler: HttpHandler) -> HttpRoute` | Go `ServeMux.HandleFunc` | 保留 | — |
| `http.mount` | `fn mount(prefix: String, sub: (HttpRequest) -> Result[HttpResponse, String]) -> HttpRoute` | Go `StripPrefix` | 保留 | — |
| `http.router` | `fn router(routes: List[HttpRoute]) -> Result[HttpHandler, String]` | Go `ServeMux` | 保留 | — |
| `http.wrap` | `fn wrap(handler: HttpHandler, middleware: List[HttpMiddleware]) -> HttpHandler` | — | 保留 | — |
| `http.decode_json` | `fn decode_json[T: Decode](req: HttpRequest) -> Result[T, HttpResponse]` | FastAPI `UserCreate`、Rails strong parameters | 语义变更（`T` 是输入类型；`Codec` 类型为 E110，类型外的键为 400） | 第 22 行、C-436 |
| `http.set_body` | `fn set_body(resp: HttpResponse, body: String) -> HttpResponse`（提案） | Gleam `set_body` (F) | 新增（`HttpRequest` 也接受） | R-http-12 |
| `http.req_body_bytes` | `fn req_body_bytes(req: HttpRequest) -> Bytes`（提案） | Go `io.ReadAll(r.Body)` / JS `req.arrayBuffer()` | 不新增（统一为 R-http-12 的 `http.body_bytes`） | R-http-12 |
| `http.set_body_bytes` | `fn set_body_bytes(resp: HttpResponse, body: Bytes) -> HttpResponse`（提案） | JS `new Response(bytes)` | 新增（`HttpRequest` 也接受） | R-http-12 |
| `http.send` | `effect fn send(req: HttpRequest) -> HttpResponse`（提案） | Go `client.Do` / reqwest `execute` / JS `fetch(request)` | 新增 | R-http-13 |
