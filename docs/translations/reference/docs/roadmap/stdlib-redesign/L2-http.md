> Description (fixed history): Stdlib redesign L2/L3 — http: client becomes a complete 2×2 (GET shortcut/any method × body/whole response) plus call handle; other shapes are accessors (#3854); status becomes set_status, url_decode moves to url, and #2827 defaults are decided.
<a id="l2l3-http"></a>

# L2/L3: http

[![English](https://img.shields.io/badge/English-24292f?style=flat-square)](L2-http.md) [![日本語](https://img.shields.io/badge/%E6%97%A5%E6%9C%AC%E8%AA%9E-d0d7de?style=flat-square)](L2-http.ja.md) [![简体中文](https://img.shields.io/badge/%E7%AE%80%E4%BD%93%E4%B8%AD%E6%96%87-d0d7de?style=flat-square)](L2-http.zh-CN.md)

> Non-normative historical redesign snapshot. Pinned original authoritative and unchanged; no automatic synchronization, fresh official-page retrieval or independent runtime/network tests. Measurements and (F) are original-author reports. Preserve early 2x2 retaining request_response versus later send/start consolidation and removal; request-header maps versus later duplicate-preserving lists; early req_ triple versus generic HttpMessage; plain query list versus dated Result correction. Status setter/getter reuse has different epochs. Stock p1 lacks HTTP; provisional embedded-host ops are not stock WASI. URL-decoding compatibility and status_code table cross-reference are source claims, not silently corrected or newly verified. Proposed security/runtime behavior is not current certification. [Source](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/stdlib-redesign/L2-http.md)

> Last updated: 2026-10-21
> Prerequisites: [L0](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/stdlib-redesign/L0-conventions.md) / [L1](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/stdlib-redesign/L1-categories.md) (R-L1-28: URL encoding belongs to `url`).
> Sources: Rust https://docs.rs/reqwest (no standard HTTP; reference) / Go https://pkg.go.dev/net/http / Python https://docs.python.org/3/library/urllib.request.html / TS/JS https://developer.mozilla.org/en-US/docs/Web/API/Fetch_API / Gleam https://hexdocs.pm/gleam_httpc/ (official-org package) / Elixir https://www.erlang.org/doc/apps/inets/httpc.html / OCaml — (not standard) / Kotlin https://ktor.io/docs/client-requests.html (official package) / Swift https://developer.apple.com/documentation/foundation/urlsession
> Existing proposals: #3854 §2 (23 client functions), #2827 (defaults), #2828 (tracking); ADR-0020 (serve), ADR-0023 (wasm HTTP is `wasi:http@0.3`).

<a id="l2-役割の比較"></a>

## L2: Role comparison

| Role | Almide | Rust(reqwest) | Go | Python | TS/JS | Gleam(httpc) | Elixir(httpc) | OCaml | Kotlin(Ktor) | Swift |
|---|---|---|---|---|---|---|---|---|---|---|
| GET shortcut (body) | `get` | `reqwest::get`+`text` | `http.Get` | `urlopen(u).read()` | `fetch(u).then(r => r.text())` | `httpc.send` | `:httpc.request(url)` | — | `client.get(u).bodyAsText()` | `URLSession.data(from:)` |
| Per-verb shortcuts | `post`・`put`・`patch`・`delete` | `Client::post` etc. | `http.Post`・`Head` | `Request(method=)` | `fetch(u, {method})` | `request.set_method` | `request(:post, …)` | — | `client.post` etc. | `httpMethod =` |
| Arbitrary-method exchange | `request` (body)・`request_response` (whole response)・`request_status`・`request_bytes` | `Client::request` | `Client.Do` | `urlopen(Request)` | `fetch(Request)` | `httpc.send` | `:httpc.request/4` | — | `client.request` | `data(for:)` |
| Response status/headers/body | `status_code`・`get_header`・`header_values`・`headers`・`body` | `status`・`headers`・`text`・`bytes` | `StatusCode`・`Header.Get`・`Body` | `status`・`getheader`・`read` | `status`・`headers.get`・`text`・`arrayBuffer` | `status`・`get_header`・`body` | Tuple | — | `status`・`headers`・`bodyAsText` | `statusCode`・`value(forHTTPHeaderField:)` |
| Body as bytes | `get_bytes`・`request_bytes` | `bytes` | `io.ReadAll(Body)` | `read()` | `arrayBuffer` | `BitArray` version | `body_format: :binary` | — | `bodyAsBytes` | `Data` |
| Stream reception | `request_stream`・`request_stream_with_limits` | `bytes_stream` | `Body.Read` | `read(n)` | `body.getReader()` | — | `stream:` | — | `bodyAsChannel` | `bytes(for:)` |
| Nonblocking observation handle | `start`・`poll`・`read_new`・`wait`・`cancel` | `Future` | goroutine | — | `Promise`・`AbortController` | — | `sync: false` | — | coroutine | `Task` |
| Server | `serve`・`route`・`router`・`mount`・`wrap`・`param` | — | `ListenAndServe`・`ServeMux` | `http.server` | `Deno.serve` etc. | — | `:httpd` | — | Ktor server | — |
| Build a response | `response`・`json`・`with_headers`・`redirect`・`status` (setter)・`set_header` | — | `WriteHeader`・`Header().Set` | `send_response` | `new Response(body, {status})` | `response.new` | — | — | `call.respond` | — |
| Read requests | `req_method`・`req_path`・`req_body`・`req_header`・`query_params`・`decode_json` | — | `r.Method`・`r.URL`・`r.Header.Get`・`r.Header.Values` | `self.path`・`headers` | `req.method`・`req.headers` | — | — | — | `call.request` | — |
| URL decoding | `url_decode` | `url` crate | `url.QueryUnescape` | `unquote_plus` | `decodeURIComponent` | `uri.percent_decode` | `URI.decode` | — | — | — |

<a id="完全性の規則"></a>

### Completeness rules

- **Client is a 2×2 matrix plus handle.** Vertical axis {GET shortcut, any method}, horizontal {body string, whole `HttpResponse`}. Four cells: `get`, `get_response`, `request`, `request_response`. Other shapes (status/body tuple, bytes, headers) come from `HttpResponse` accessors. Nonblocking operation, streaming and time limits share one handle path: `start`, `poll`, `read_new`, `wait`, `cancel`.
- **One status rule.** Every client returns ok for a fully received response (including 404 and 3xx); err only for communication failure.
- **Response accessors** (subject `HttpResponse`): `status_code`, `body`, `body_bytes`, `get_header`, `header_values`, `headers`.
- **Request accessors** (subject `HttpRequest`; `req_` avoids collisions with response functions in the same module): `req_method`, `req_path`, `req_body`, `req_header`/`req_header_values`/`req_headers` (same triple as responses), `query_params`, `param`.
- **Response modification uses copy-returning `set_`** (R-L0-04): `set_status`, `set_header`.
- URL encoding/decoding belongs to `url` (R-L1-28).

<a id="l2-の判定"></a>

### L2 verdict

- Axis multiplication and holes: verbs × shape (body/tuple/whole/bytes) × limits produces 23 functions and inferable but absent cells such as `post_status`/`post_bytes` (#3854 §2) → R-http-01.
- Status-rule outlier: only `request_stream` makes non-2xx err (#3854) → R-http-01 (remove).
- Naming trap: `http.status(resp, code)` sets status but resembles accessor `status_code` → R-http-02.
- Family hole: request headers lack “all values” and “map” → R-http-03.
- Duplication: `http.url_decode` is form decoding, belonging to `url` (differs from `url.decode_component` on `+`; measurements below) → R-http-04.
- Defaults (#2827) → R-http-05.
- Sentinel: `status_code` returns `0` for unreadable status lines (R-L0-06 item 4) → R-http-01.

<a id="案"></a>

## Proposals

<a id="r-http-01-client-を-22--ハンドルにまとめる3854-2---採用"></a>

### R-http-01 Consolidate client to 2×2 plus handle (#3854 §2) — ○ Accepted

- Content:
  - Keep `get(url) -> String`, `get_response(url) -> HttpResponse`, `request(method, url, body, headers) -> String`, `request_response(method, url, body, headers) -> HttpResponse`, plus five handle functions.
  - Add `fn body_bytes(resp: HttpResponse) -> Bytes` (body without UTF-8 conversion).
  - Remove 14 functions with replacements:
    - `post(u, b)` → `request("POST", u, b, [:])` (same shape for `put`/`patch`/`delete`).
    - `post_response(u, b)` → `request_response("POST", u, b, [:])` (same for `put_`/`patch_`/`delete_response`).
    - `get_status(u)` → `status_code` and `body` from `get_response(u)`; `request_status(…)` likewise from `request_response(…)`.
    - `get_bytes(u)` → `body_bytes(get_response(u)!)`; `request_bytes(…)` → `body_bytes(request_response(…)!)`.
    - `request_stream`/`request_stream_with_limits` → `start` + `read_new` + `poll`/`wait`.
  - An unreadable status line makes the exchange err, not a response with `status_code = 0` (remove sentinel, R-L0-06 item 4).
- Difference from #3854: it proposed one GET shortcut plus one exchange (8 functions). Here `get_response` and body-returning `request` remain, filling 2×2, for three reasons: (1) CHEATSHEET teaches both `http.get` and `http.request`, neither need disappear. (2) No matrix holes: “if `get` has `_response`, so does `request`” always holds. (3) Every removal maps one-to-one to retained forms, mechanically fixable by `almide fix`. No repurposing `request` with a different return type (R-L0-14 item 4).
- Supporting L0 rules: R-L0-01 (one spelling per exchange), CLAUDE.md “families as matrices”, R-L0-14 item 4.
- (a) Go/Python/JS (`fetch` default)/Ktor/Swift have GET shortcuts, and all have a single arbitrary-method exchange. Go (only `Get`/`Post`/`Head`) and Ktor have verb shortcuts, but no language splits shape (status-only/bytes-only) into function names; all extract from response values.
- (b) Wasm (ADR-0023): stock p1 has no HTTP import; p3 components transport via `wasi:http/client@0.3.0`. Embedded host currently uses provisional ops 43–50/53–59 (ledger `pending-port`, retired in step 10). ADR-0023 step 4 put six `*_response`, `request_stream`, and handles in the shim. Here client becomes 4 + 5, reducing shim-carried functions 23 → 9. Step 5 already demonstrates `request_stream` SSE can be written in Almide atop handles.
- (c) Inferable nonexistent names such as `post_status` disappear; all clients share one status rule.
- (d) Remove/add. Each function gets `@deprecated(since=15, use="…")` (`use` names replacements above), `removal_epoch = 16`. `almide fix` handles one-to-one rewrites (only `request_stream` family changes shape and needs handle-usage hints). Removing `status_code`'s `0` does not change correctly parsed responses, so is not epoch `breaks`. Counts (spec/tests/docs/stdlib): `post` 2/1/13/0, `put` 1/1/8/0, `patch` 1/2/7/0, `delete` 1/1/8/0, `get_status` 6/4/6/1, `request_status` 5/5/5/1, `post_response` 0/1/6/1, `put_response` 0/2/6/1, `patch_response` 0/2/6/1, `delete_response` 0/1/6/1, `get_bytes` 2/6/5/1, `request_bytes` 2/7/5/1, `request_stream` 0/14/3/1, `request_stream_with_limits` 2/8/5/0.
- Gate: replace `tests/http_response_family_gate_test.rs` with 2×2 checks (exactly four cells, paired argument order). Rewrite `http_call_family_gate_test.rs`'s `_with_limits` rule to “stream only through handles” since streaming functions disappear. Differential-test all clients against one test server for identical status/body/err classification (#3854 proposal).
- Issue: #3854 (existing; settle in this form).

<a id="r-http-02-httpstatus-を-httpset_status-にする---採用"></a>

### R-http-02 Rename `http.status` to `http.set_status` — ○ Accepted

- Content: add `fn set_status(resp: HttpResponse, code: Int) -> HttpResponse`, remove `http.status`.
- Spelling: same-module response-copy modification is `set_header`; R-L0-04 makes `set` a copy verb. Do not use #3854 draft's `with_status`, which shares `with_` with constructors `with_headers` and `list.with_capacity`, mixing creation and modification.
- Supporting L0 rules: R-L0-01, R-L0-04 item 3.
- (a) Go `WriteHeader`, JS `new Response(body, {status})`, Ktor `call.respond(status)`; spellings differ.
- (b) Pure. (c) Writing `http.status(resp)` to read status produces an argument-count error because it is a setter.
- (d) Rename: `@deprecated(since=15, use="http.set_status")`, `removal_epoch = 16`; one-to-one `almide fix`. Counts 11/6/9/0.
- Issue: #3854 (existing).

<a id="r-http-03-要求の側のヘッダを応答と同じ-3-つ組にする---採用"></a>

### R-http-03 Give request headers the same triple as responses — ○ Accepted

- Content: add `fn req_header_values(req: HttpRequest, key: String) -> List[String]` (all values in arrival order), `fn req_headers(req: HttpRequest) -> Map[String, String]` (lowercase names, first value).
- Basis: CLAUDE.md “families as matrices”. Responses had {first value, all, map}; requests only first value.
- (a) Go `Header.Get`/`Header.Values`, JS `headers.get`/`headers.entries`; same shapes in both directions.
- (b) Pure (used inside `http.serve` handlers).
- (d) Add; usage count 0.
- Gate: check request/response header functions form matching triples.
- Issue: #3917.

<a id="r-http-04-httpurl_decode-を撤去するr-l1-28---採用"></a>

### R-http-04 Remove `http.url_decode` (R-L1-28) — ○ Accepted

- Content: remove `http.url_decode(s)`; replace with `url.decode_form(s)` (form decoding added by [L2-url](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/stdlib-redesign/L2-url.md) R-url-01).
- Measurement (probe-http): `http.url_decode("a+b%20c%zz")` yields `a b c%zz` (`+` becomes space, malformed `%` remains, returns `String`). Converting `+` to space is form decoding, unlike `url.decode_component` (keeps `+`, errors on malformed `%`), so replacement is `url.decode_form`.
- Supporting L0 rules: R-L0-01, R-L0-02 (URL-string functions belong to `url`), R-L0-06 item 4 (do not hide failure with sentinels), R-L1-28.
- (d) Remove: `@deprecated(since=15, use="url.decode_form")`, `removal_epoch = 16`. `almide fix`: `http.url_decode(s)` → `url.decode_form(s) ?? s` (invalid input yields original string). Only invalid UTF-8 (`%ff`) differs: original string instead of replacement characters. State this in the hint. Counts 9/0/9/2.
- Issue: #3950.

<a id="r-http-05-client-の既定の振る舞い2827の判定---採用項目ごとに判定"></a>

### R-http-05 Decide client defaults (#2827) — ○ Accepted (itemwise decisions)

| # | #2827 item | Verdict | Reason |
|---|---|---|---|
| 1 | Follow redirects | × | Documentation promises “3xx and its `Location` are the answer; the response belongs to the called URL”. Following changes results under the same name (R-L0-14 item 4). To follow, call the next URL from `get_header(r, "location")`. |
| 2 | Send `Accept-Encoding: gzip` and decompress | ○ | `body` still means the received body string. Currently compressed data becomes unreadable `String`. Use standard `zlib` to decompress; p3 shim can share the same Almide implementation. |
| 3 | Default `User-Agent: almide/<version>` | ○ | Caller-provided header takes precedence. Eliminates 403s from APIs requiring User-Agent (such as GitHub). |
| 4 | Add `Content-Type: application/json` when a body exists | × | Not added if caller provides `content-type` (one writer in `http_route_core.rs`). Changing the default changes what the same call sends (R-L0-14 item 4). |
| 5 | Send Bytes bodies | × Do not add now | Adding “outgoing body type” as a third axis doubles exchange functions (CLAUDE.md “families as matrices”). If added, repropose in #2827 an exchange accepting `HttpRequest` (built with `http.new_request`), placing body type in one location. |
| 6 | Keep-alive and connection reuse | Out of scope | Function surface unchanged. Pursue as runtime-internal improvements in #2827. |

- #2828's items (proxy, trust store, URL decomposition, header newlines, server limits) are runtime-correctness issues not changing function surfaces; this page does not decide them.
- (b) Apply items 2 and 3 identically to native `http_client_core.rs` and p3 shim (ADR-0023 §4 “semantics in one place”).
- Issue: #2827 (existing).

<a id="r-http-06-応答の本文を差し替える-httpset_body-を足す---採用"></a>

### R-http-06 Add response-body replacement `http.set_body` — ○ Accepted

- Proposal: `fn http.set_body(resp: HttpResponse, body: String) -> HttpResponse`. Each response part {status, headers, body} gets a `set_` function.
- (a) Gleam `response.set_body` (F).
- (c) Middleware (`http.wrap`) currently must rebuild with `with_headers` and `headers()` to change bodies, silently losing multivalued headers such as `set-cookie`.
- (d) Add 1 (publish internal `__set_body`).
- Issue: #3974.

<a id="r-http-07-応答のヘッダの-get_header-と要求の-req_header-の不揃い---不採用規則を書く-r-http-12-で解消"></a>

### R-http-07 Inconsistent response `get_header` and request `req_header` — × Rejected (document rules; → resolved by R-http-12)

- Reason: many single-header spellings include get: Gleam `get_header` (F), Go `Header.Get`, JS `headers.get`; little benefit in renaming to `header`. Instead document two completeness rules: “add `req_` only when response names collide”; “`get_` has two meanings: `get`/`get_response` (HTTP GET), and `get_header` (extraction)”.
- Issue: none.

<a id="r-http-08-取り出しの-status_code-を-status-にする---採用epoch-17"></a>

### R-http-08 Rename accessor `status_code` to `status` — ○ Accepted (epoch 17)

- Current: accessor `http.status_code(resp) -> Int` and setter `http.set_status(resp, code)` have different stems. Old setter `http.status(resp, code)` is removed epoch 16.
- Proposal: epoch 17 adds accessor `fn http.status(resp: HttpResponse) -> Int` and removes `status_code`. Leave one epoch after removing the old setter meaning to shorten overlap with training-data meaning.
- (a) HTTP libraries in JS/Python/Ktor/Gleam/Rust use `status` (five); Go/Swift use `StatusCode` (two).
- (d) Rename. Old setter `http.status(r, 404)` produces an argument-count error against the new getter, satisfying R-L0-14.4. Annotate `status_code` in epoch 16 with `@deprecated(since=16, use="http.status")`, remove epoch 17.
- Issue: #3974.

<a id="r-http-09-ヘッダの全体を-liststring-string-で表す---採用"></a>

### R-http-09 Represent all headers as `List[(String, String)]` — ○ Accepted

- Current: `header_values`/`req_header_values` read all values for one name, but whole-header readers `headers`/`req_headers` and header-taking constructors/calls `with_headers`/`new_request`/`request`/`request_response`/`start` use `Map[String, String]`. Repeated names retain only one value. `Set-Cookie` cannot be comma-joined (RFC 9110 §5.3), so merely reading and rebuilding breaks it.
- Proposal: read/write all headers as `List[(String, String)]` (arrival/send order, lowercase names).
  - `fn headers(resp: HttpResponse) -> List[(String, String)]`, `fn req_headers(req: HttpRequest) -> List[(String, String)]`.
  - `headers` arguments of `with_headers`/`new_request`/`request`/`request_response`/`start` also become `List[(String, String)]`.
  - Keep single-value `get_header`, all-values `header_values`, and single-header replacement `set_header`.
- Supporting L0 rule: R-L0-01 (not two shapes for the same information). Same pair-list shape as duplicate-preserving `url.parse_query` (R-http-10).
- (a) Go `http.Header` (`map[string][]string`), JS `Headers` (`append` preserves repeats), Python `http.client`'s `getheaders()` (pair list), Rust http crate `HeaderMap` (multivalue) all preserve duplicates.
- (b) Pass header sequences unchanged on native client and p3 shim.
- (c) Middleware read/rebuild no longer loses `Set-Cookie`.
- (d) Semantic change (types change, so old calls type-error rather than silently changing results). `almide fix` inserts `map.from_list`/`map.to_list` conversions.
- Completeness: all arguments/returns passing whole headers are `List[(String, String)]`; no public `Map[String, String]` headers remain.
- Issue: #3976.

<a id="r-http-10-query_params-を-liststring-string-にする---採用"></a>

### R-http-10 Make `query_params` return `List[(String, String)]` — ○ Accepted

- Current: `fn query_params(req: HttpRequest) -> Map[String, String]` drops the second value in `?tag=a&tag=b`. `url.parse_query` reading the same query returns `List[(String, String)]`, preserving duplicates/order.
- Proposal: `fn query_params(req: HttpRequest) -> List[(String, String)]`, decode with the same rules as `url.parse_query`.
- Addition (2026-10-24, #4117/C-404): use the same single decoder as `url.parse_query`; malformed escapes and invalid UTF-8 are err. Return type is `Result[List[(String, String)], String]`.
- (a) Go `r.URL.Query()` (multivalue), Python `parse_qsl`, JS `URLSearchParams.getAll`.
- (d) Semantic change (noticed through type errors). For one value, use `list.find(ps, ((k, _)) => k == "tag")`.
- Issue: #3976.

<a id="r-http-11-本文の-bytes-の族を閉じるreq_body_bytesset_body_bytes---採用"></a>

### R-http-11 Complete body `Bytes` family (`req_body_bytes`/`set_body_bytes`) — ○ Accepted

- Current: response bodies have string `body` and bytes `body_bytes`, but requests only string `req_body`, and response modification only string `set_body`. `body` uses replacement characters for invalid UTF-8 (lossy), corrupting server-received binary data.
- Proposal: add `fn req_body_bytes(req: HttpRequest) -> Bytes`, `fn set_body_bytes(resp: HttpResponse, body: Bytes) -> HttpResponse`.
- Completeness: readers/replacers have string/bytes pairs (`body`/`body_bytes`, `req_body`/`req_body_bytes`, `set_body`/`set_body_bytes`). R-http-12 consolidates `req_` into unsuffixed forms. String forms use lossy UTF-8, documented explicitly.
- Deferred: client binary-body sending (`request`'s `body: String`) is decided alongside request-construction type design, not added here (→ resolved by R-http-13 `http.send`).
- (d) Add 2.
- Issue: #3976.

<a id="r-http-12-要求と応答を-httpmessage-で-1-つの関数にする---採用r-http-07-を置き換え"></a>

### R-http-12 Unify requests/responses through `HttpMessage` generic functions — ○ Accepted (replaces R-http-07)

- Current: same-role response functions are unprefixed (`body`/`headers`/`header_values`), request forms have `req_` (`req_body`/`req_headers`/`req_header_values`). Single-header readers even differ in stem: response `get_header`, request `req_header`. R-http-09's whole-header `headers`/`req_headers` also left only singular `get_`. Three external reviews successively flagged this.
- Proposal: make both types implement protocol `HttpMessage` (R-L0-31), use one generic function per read/write role.
  - `fn header[M: HttpMessage](m: M, name: String) -> String?`
  - `fn header_values[M: HttpMessage](m: M, name: String) -> List[String]`
  - `fn headers[M: HttpMessage](m: M) -> List[(String, String)]`
  - `fn body[M: HttpMessage](m: M) -> String`
  - `fn body_bytes[M: HttpMessage](m: M) -> Bytes`
  - `fn set_header[M: HttpMessage](m: M, name: String, value: String) -> M`
  - `fn set_body[M: HttpMessage](m: M, body: String) -> M`
  - `fn set_body_bytes[M: HttpMessage](m: M, body: Bytes) -> M`

  - Remove `get_header` (→ `header`), `req_header`/`req_header_values`/`req_headers`/`req_body`/`req_body_bytes` (→ unprefixed).
  - Requests can use `set_body_bytes`, enabling arbitrary bytes as request bodies (R-http-13).
- Supporting L0 rules: R-L0-31, R-L0-01, R-L0-00.
- (a) Go `Request.Header.Get`/`Response.Header.Get` use the same method on the same `Header` type; JS fetch `Request`/`Response` share `Body` and `Headers`.
- (d) Rename/remove: `@deprecated(since=15, use=…)`, `removal_epoch = 16`, one-to-one `almide fix`.
- Issue: #3989.

<a id="r-http-13-組み立てた要求を送る-httpsend-を足す---採用"></a>

### R-http-13 Add `http.send` for constructed requests — ○ Accepted

- Current: client request bodies in `request`/`request_response`/`start` are all `body: String`; no path sends arbitrary bytes.
- Proposal: add `effect fn http.send(req: HttpRequest) -> HttpResponse`. Write `http.new_request(…) |> http.set_body_bytes(b) |> http.send` (R-http-12 enables request `set_body_bytes`). String bodies remain expressible through `request`/`request_response`.
- (a) Go `client.Do(req)`, Rust reqwest `Client::execute(request)`, JS `fetch(request)`.
- (b) Same path as `request_response` on both targets.
- (d) Add 1.
- Issue: #3989.

<a id="r-http-14-要求を組み立てる道を-1-つにする-startreq-limitsrequest_response-の撤去req_-を外す---採用"></a>

### R-http-14 One request-construction path: `start(req, limits)`, remove `request_response` and `req_` — ○ Accepted

- Current: R-http-13 added `send(req: HttpRequest)`, so constructed requests (e.g. bytes bodies via `set_body_bytes`) can be sent synchronously. But async `start` retains five arguments `method, url, body: String, headers, limits`, unable to send bytes. Whole-response general forms duplicate `request_response(method, url, body, headers)` and `send(new_request(…))`. Request-only `req_method`/`req_path` have `req_`, while `param`/`query_params` do not (R-http-12 removed it from eight shared functions).
- Proposal:
  - `effect fn http.start(req: HttpRequest, limits: HttpLimits) -> HttpCall`, accepting the same request as `send`.
  - Remove `request_response` (→ `send(new_request(method, url, body, headers))`). Retain body-string `request` and GET shortcuts `get`/`get_response`. Only `send` is the general whole-response path.
  - Rename `req_method` to `method`, `req_path` to `path`. No prefix for request-only functions; argument types identify their subject (R-L0-02: “prefixes are not namespaces”).
- Supporting L0 rules: R-L0-01 (one spelling per role), R-L0-02.
- (a) Go `client.Do(req)` shares requests for sync/async, with no prefixes in `r.Method`/`r.URL.Path`. Same for JS `fetch(request)`.
- (b) Call-shape change; communication internals unchanged.
- (c) Rewrite sync `send(req)` to async `start(req, limits)` preserving request argument shape.
- (d) `start` changes meaning (old five-argument calls type-error). Remove `request_response`; rename `req_method`/`req_path`. All get `@deprecated` in epoch 15, removal epoch 16, one-to-one `almide fix`.
- Issue: #3996.
<a id="l3-関数表"></a>

## L3: Function table

| Function | Signature (current, mut shown) | Main spellings in other languages | Verdict | Proposal |
|---|---|---|---|---|
| `http.serve` | `effect fn serve(port: Int, f: HttpHandler) -> Unit` | Go `ListenAndServe` / Deno `serve` | Keep | — |
| `http.response` | `fn response(status: Int, body: String) -> HttpResponse` | JS `new Response` / Ktor `respondText` | Keep | — |
| `http.json` | `fn json(status: Int, body: String) -> HttpResponse` | JS `Response.json` | Keep | — |
| `http.with_headers` | `fn with_headers(status: Int, body: String, headers: List[(String, String)]) -> HttpResponse` | JS `new Response(b, {headers})` | Semantic change (`headers` to `List[(String, String)]`) | R-http-09 |
| `http.redirect` | `fn redirect(url: String) -> HttpResponse` | JS `Response.redirect` / Go `http.Redirect` | Keep | — |
| `http.status` | `fn status(resp: HttpResponse, code: Int) -> HttpResponse` | Go `WriteHeader` | Rename (→ `set_status`) | R-http-02 |
| `http.set_status` | `fn set_status(resp: HttpResponse, code: Int) -> HttpResponse` (Proposal) | Same as above | Add | R-http-02 |
| `http.set_header` | `fn set_header(resp: HttpResponse, key: String, value: String) -> HttpResponse` | Go `Header().Set` / JS `headers.set` | Semantic change (`HttpRequest` also accepted) | R-http-12 |
| `http.body` | `fn body(resp: HttpResponse) -> String` | JS `text()` / Go `Body` / Ktor `bodyAsText` | Semantic change (`HttpRequest` also accepted) | R-http-12 |
| `http.body_bytes` | `fn body_bytes(resp: HttpResponse) -> Bytes` (Proposal) | JS `arrayBuffer` / reqwest `bytes` / Ktor `bodyAsBytes` | Add (`HttpRequest` also accepted) | R-http-12 |
| `http.status_code` | `fn status_code(resp: HttpResponse) -> Int` | Go `StatusCode` / JS `status` | Rename → `status` (epoch 17; `0` sentinel also removed) | R-http-04, R-http-08 |
| `http.get_header` | `fn get_header(resp: HttpResponse, key: String) -> String?` | Go `Header.Get` / JS `headers.get` | Rename → `header` (`HttpRequest` also accepted) | R-http-12 |
| `http.header_values` | `fn header_values(resp: HttpResponse, key: String) -> List[String]` | Go `Header.Values` | Semantic change (`HttpRequest` also accepted) | R-http-12 |
| `http.headers` | `fn headers(resp: HttpResponse) -> List[(String, String)]` | JS `headers` / reqwest `headers` | Semantic change (`HttpRequest` also accepted) | R-http-12 |
| `http.req_method` | `fn req_method(req: HttpRequest) -> String` | Go `r.Method` / JS `req.method` | Rename → `method` | R-http-14 |
| `http.req_path` | `fn req_path(req: HttpRequest) -> String` | Go `r.URL.RequestURI()` | Rename → `path` (document explicitly that query is included) | R-http-14 |
| `http.req_body` | `fn req_body(req: HttpRequest) -> String` | Go `r.Body` / JS `req.text()` | Remove → `http.body` | R-http-12 |
| `http.req_header` | `fn req_header(req: HttpRequest, key: String) -> String?` | Go `r.Header.Get` | Remove → `http.header` | R-http-12 |
| `http.req_header_values` | `fn req_header_values(req: HttpRequest, key: String) -> List[String]` (Proposal) | Go `r.Header.Values` | Do not add (consolidate into R-http-12 `http.header_values`) | R-http-12 |
| `http.req_headers` | `fn req_headers(req: HttpRequest) -> List[(String, String)]` (Proposal) | JS `req.headers` | Do not add (consolidate into R-http-12 `http.headers`) | R-http-12 |
| `http.query_params` | `fn query_params(req: HttpRequest) -> Result[List[(String, String)], String]` | Go `r.URL.Query()` | Semantic change (`List[(String, String)]` returned) | R-http-10 |
| `http.url_decode` | `fn url_decode(s: String) -> String` | Go `QueryUnescape` / Python `unquote_plus` | Remove → `url.decode_form(s) ?? s` | R-http-04 |
| `http.get` | `effect fn get(url: String) -> String` | Go `http.Get` / reqwest `get` / Python `urlopen` | Keep | R-http-01 |
| `http.get_response` | `effect fn get_response(url: String) -> HttpResponse` | JS `fetch(u)` / Ktor `client.get(u)` | Keep | R-http-01 |
| `http.request` | `effect fn request(method: String, url: String, body: String, headers: List[(String, String)]) -> String` | Go `Client.Do` / JS `fetch(u, {method})` | Semantic change (`headers` to `List[(String, String)]`) | R-http-09 |
| `http.request_response` | `effect fn request_response(method: String, url: String, body: String, headers: List[(String, String)]) -> HttpResponse` | Same as above | Remove (→ `send(new_request(method, url, body, headers))`) | R-http-14 |
| `http.post` | `effect fn post(url: String, body: String) -> String` | Go `http.Post` / Ktor `post` | Remove (→ `request("POST", …)`) | R-http-01 |
| `http.put` | `effect fn put(url: String, body: String) -> String` | Ktor `put` | Remove (→ `request("PUT", …)`) | R-http-01 |
| `http.patch` | `effect fn patch(url: String, body: String) -> String` | Ktor `patch` | Remove (→ `request("PATCH", …)`) | R-http-01 |
| `http.delete` | `effect fn delete(url: String) -> String` | Ktor `delete` | Remove (→ `request("DELETE", …)`) | R-http-01 |
| `http.get_status` | `effect fn get_status(url: String) -> (Int, String)` | — | Remove (→ `get_response`) | R-http-01 |
| `http.request_status` | `effect fn request_status(method: String, url: String, body: String, headers: Map[String, String]) -> (Int, String)` | — | Remove (→ `send`) | R-http-01 |
| `http.post_response` | `effect fn post_response(url: String, body: String) -> HttpResponse` | — | Remove (→ `send(new_request("POST", …))`) | R-http-01 |
| `http.put_response` | `effect fn put_response(url: String, body: String) -> HttpResponse` | — | Remove (→ `send(new_request("PUT", …))`) | R-http-01 |
| `http.patch_response` | `effect fn patch_response(url: String, body: String) -> HttpResponse` | — | Remove (→ `send(new_request("PATCH", …))`) | R-http-01 |
| `http.delete_response` | `effect fn delete_response(url: String) -> HttpResponse` | — | Remove (→ `send(new_request("DELETE", …))`) | R-http-01 |
| `http.get_bytes` | `effect fn get_bytes(url: String) -> Bytes` | reqwest `bytes` | Remove (→ `body_bytes(get_response(u)!)`) | R-http-01 |
| `http.request_bytes` | `effect fn request_bytes(method: String, url: String, body: String, headers: Map[String, String]) -> Bytes` | — | Remove (→ `body_bytes(send(new_request(…))!)`) | R-http-01 |
| `http.request_stream` | `effect fn request_stream(method: String, url: String, body: String, headers: Map[String, String], on_chunk: (String) -> Unit) -> Unit` | reqwest `bytes_stream` / JS `body.getReader` | Remove (→ handle) | R-http-01 |
| `http.request_stream_with_limits` | `effect fn request_stream_with_limits(method: String, url: String, body: String, headers: Map[String, String], limits: HttpLimits, on_chunk: (String) -> Unit) -> Unit` | — | Remove (→ handle) | R-http-01 |
| `http.start` | `effect fn start(method: String, url: String, body: String, headers: List[(String, String)], limits: HttpLimits) -> HttpCall` | JS `fetch` (Promise) | Semantic change (`start(req: HttpRequest, limits: HttpLimits)`) | R-http-09, R-http-14 |
| `http.poll` | `effect fn poll(c: HttpCall) -> Result[HttpResponse, String]?` | Python `Popen.poll` equivalent | Keep (`process.poll` same spelling) | R-http-01 |
| `http.read_new` | `effect fn read_new(c: HttpCall) -> String` | — | Keep | R-http-01 |
| `http.wait` | `effect fn wait(c: HttpCall) -> HttpResponse` | — | Keep | R-http-01 |
| `http.cancel` | `effect fn cancel(c: HttpCall) -> Unit` | JS `AbortController.abort` | Keep | R-http-01 |
| `http.new_request` | `fn new_request(method: String, target: String, body: String, headers: List[(String, String)]) -> HttpRequest` | JS `new Request` | Semantic change (`headers` to `List[(String, String)]`) | R-http-09 |
| `http.param` | `fn param(req: HttpRequest, name: String) -> String?` | Go `r.PathValue` | Keep | — |
| `http.route` | `fn route(pattern: String, handler: HttpHandler) -> HttpRoute` | Go `ServeMux.HandleFunc` | Keep | — |
| `http.mount` | `fn mount(prefix: String, sub: (HttpRequest) -> Result[HttpResponse, String]) -> HttpRoute` | Go `StripPrefix` | Keep | — |
| `http.router` | `fn router(routes: List[HttpRoute]) -> Result[HttpHandler, String]` | Go `ServeMux` | Keep | — |
| `http.wrap` | `fn wrap(handler: HttpHandler, middleware: List[HttpMiddleware]) -> HttpHandler` | — | Keep | — |
| `http.decode_json` | `fn decode_json[T: Decode](req: HttpRequest) -> Result[T, HttpResponse]` | FastAPI `UserCreate`, Rails strong parameters | Semantic change (`T` is the input type; `Codec` types produce E110; keys outside the type produce 400) | Line 22, C-436 |
| `http.set_body` | `fn set_body(resp: HttpResponse, body: String) -> HttpResponse` (Proposal) | Gleam `set_body` (F) | Add (`HttpRequest` also accepted) | R-http-12 |
| `http.req_body_bytes` | `fn req_body_bytes(req: HttpRequest) -> Bytes` (Proposal) | Go `io.ReadAll(r.Body)` / JS `req.arrayBuffer()` | Do not add (consolidate into R-http-12 `http.body_bytes`) | R-http-12 |
| `http.set_body_bytes` | `fn set_body_bytes(resp: HttpResponse, body: Bytes) -> HttpResponse` (Proposal) | JS `new Response(bytes)` | Add (`HttpRequest` also accepted) | R-http-12 |
| `http.send` | `effect fn send(req: HttpRequest) -> HttpResponse` (Proposal) | Go `client.Do` / reqwest `execute` / JS `fetch(request)` | Add | R-http-13 |
