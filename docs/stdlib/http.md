# http

HTTP client and server. import http, effect.

`HttpRequest` / `HttpResponse` are the module's runtime-backed nominal types.
With `import http` they resolve in user annotations too — bare or qualified —
so typed helpers over requests are writable:

```almd check
import http

fn handle(req: HttpRequest) -> http.HttpResponse = http.response(200, http.path(req))

effect fn main() -> Unit = {
  http.serve(3000, handle)
}
```

Both types are an `HttpMessage`, so ONE function reads or writes the headers
and body of either kind of message: `header`, `header_values`, `headers`,
`body`, `body_bytes`, `set_header`, `set_body` and `set_body_bytes` take a
request or a response and give back the same kind. A header list is
`List[(String, String)]` everywhere — `with_headers`, `new_request`,
`request` and what `headers` returns — in wire order, a
repeated field kept.

The client functions are `effect fn`s that return `Result`. Propagation is
explicit (ADR-0008): write `http.send(req)!` in an effect fn, or handle the
`Result` with `match` / `??`. A bare `http.get(url)` statement is E042.

### `http.serve(port: Int, handler: HttpHandler) -> Unit`

Start an HTTP server on the given port with a request handler

```almd check
import http

effect fn main() -> Unit = {
  http.serve(3000, (req) => http.response(200, "ok"))
}
```

The server listens on `0.0.0.0:port` (every interface; ADR-0020 §5.7). It
accepts and reads up to 64 connections at once, and calls the handler for one
request at a time, in the order the requests arrive whole. At 64 connections in
flight it stops accepting until one closes; new connections wait in the
operating system's backlog. A client that sends its request slowly holds only
its own connection, so other clients are still answered (#4021). The server
reads each request within limits before the handler sees it:

| Limit | Value | Answer |
|---|---|---|
| Request head (request line and header fields), from accept | 10 s | `408`; a client that sent nothing by then is closed without an answer |
| Whole request, from accept (the body read and the wait for the handler included) | 30 s | `503` |
| Body, declared by `Content-Length` or sent chunked | 1 MiB | `413` |
| Request line / header line | 8 KiB | `414` / `431` |
| Header fields | 100 | `431` |

A body shorter than its `Content-Length` is `400`. The handler is not called
for any of these. A chunked request body is decoded.
The request head is read by RFC 9112 (C-405): a line that does not end in
CRLF, a request line that is not `METHOD SP target SP HTTP/1.1` (or
`HTTP/1.0`) with a token method, an `OPTIONS *` or `CONNECT host:port`
target, whitespace before a header colon (`Transfer-Encoding : chunked`), a
header line without a colon, an obs-fold continuation line, two `Host`
fields, and an HTTP/1.1 request without `Host` are each `400` before the
handler runs. An absolute-form target (`GET http://host/p?q HTTP/1.1`) reaches
the handler as `/p?q`.
The response goes out as `HTTP/1.1 <status> <reason>` (the registered reason,
empty for an unregistered code), the response's headers in order,
`Connection: close`, `Content-Length`, the body, and then the connection
closes. A HEAD request, a `204` and a `304` get no body. The server owns the
framing: a handler's `Connection`, `Content-Length`, `Transfer-Encoding` and
`Keep-Alive` fields are dropped. Every response carries
`X-Content-Type-Options: nosniff` unless the handler set that field (C-407).
A response header whose name is not a token,
or whose value holds a control character other than HTAB, and a status
outside `200..=599` (a 1xx included) are refused: the answer is a `500` whose
body is `Internal Server Error`, and the reason goes to stderr as one line
(`http.serve: refused the response: response status 42 is not a final status
code`). A response later than 30 s is replaced by `503`.

A handler `err(m)` never reaches the client (C-409, #4150): `m` can carry a
path, an operand or errno text (`fs.read_text(p)!` in a handler puts the
absolute path in it). The client gets `500` with the fixed body
`Internal Server Error (request <id>)`, where `id` is 8 lowercase hex digits of
the 32-bit FNV-1a (`hash.fnv1a32`) of the request's method, a space, its
target, a LF and its body — the same on every target for the same request.
stderr gets `http.serve: request <id>: handler error: "<m>"`, with `\`, `"`
and control characters escaped so input in `m` cannot forge a log line. Find
the error by the id the client reports. If the port cannot be bound, the
program stops with `Error: bind failed: <reason>` and exit code 1.

A runtime abort inside the handler — an out-of-bounds index, a division by
zero, `panic(msg)` — fails only that request (C-434, #4203). The client gets
the same `500` and fixed body as for an `err`, and stderr gets one line
`http.serve: request <id>: handler aborted: "<text>"` (`PANIC: <msg>` for a
`panic`). Sockets, unix descriptors and http calls the handler opened and
did not close are closed with it. The server keeps serving the next request.
Stack exhaustion and `process.exit` still end the program, even after the
handler printed an `Error: ` line itself. Running out of memory does too: an
allocation the handler cannot get ends the program with `Error: out of memory`
and exit code 1 (C-197), and that request gets no answer, since what the
handler was building is left half made.

**On wasm** (C-367): `almide run app.almd --target wasm` serves the same
program on the embedded host. One instance handles every request, one at a
time, exactly as the native process does: `main` runs once. The host reads
connections through the same server code as native, with the same limits (64
connections, the 10 s request-head deadline and its `408`), so a slow client
does not hold up the others there either. The status line, headers and body
are byte-identical to native. A stock artifact from
`almide build --target wasm` has no listening socket, so `almide build` and
`almide check --target wasm` still refuse `http.serve` (E081, #2659).

## Routing and middleware

A handler is a plain function from a request to a response —
`type HttpHandler = effect (HttpRequest) -> HttpResponse` — and so are a
router and a wrapped app. `http.serve` is one way to run a handler;
`http.new_request` is another: a test builds a request in code and calls the
app directly, with no socket. Routing and request construction are pure and
behave identically on native and wasm, and a router is served like any other
handler.
The example prints the lines below on both targets; on wasm the router is
served by the structural leg (#2664; held by `spec/stdlib/http_router_test.almd`).

The router and any middleware in front of it read ONE canonical path (C-406,
#4133): a path with an empty segment inside it (`//admin`), a `.` or `..`
segment, a percent-encoded unreserved byte (`%61dmin`, `%2e`), a backslash or
a control character is `400 Bad Request: the path is not in canonical form`.
So a guard such as `string.starts_with(http.path(req), "/admin")` checks
the same path the router dispatches on. A bound parameter that decodes to `.`
or `..`, or holds `/` (`%2F`), `\` or NUL, is `400 Bad Request: a path
parameter is a dot segment or holds a /, \ or NUL` (#4134), so
`/static/{file}` never binds `../../etc/passwd`. There is no static-file
helper yet (`http.serve_dir` does not exist); when a handler serves files,
resolve the name inside a root with the rooted `fs` family, never by string
concatenation.

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

- **Patterns** — `http.route("METHOD /path", handler)`; the method is
  optional (`"/health"` answers every method). `{name}` binds one segment,
  percent-decoded (`+` stays `+`), read with `http.param(req, name)`; a last
  `{name...}` binds the rest of the path, possibly empty. Matching uses the
  path without its query string; empty segments do not count, so `/users/`
  is `/users`.
- **Precedence** — the most specific matching route answers, whatever the
  registration order: `GET /users/me` beats `GET /users/{id}`, and a route
  with a method beats the same path without one.
- **Refusal** — `http.router` returns `err` naming every problem, so a
  broken table is never served: two routes some request matches with neither
  more specific (Go 1.22's rule, two spellings of one pattern included), a
  `{name...}` that is not last, a name bound twice, a method that is not
  upper-case letters, a path not starting with `/`.
- **Router answers** — no route matches the path: `404 Not Found`. Routes
  match the path but not the method: `405 Method Not Allowed` with an
  `Allow` header. `HEAD` falls back to the `GET` route and drops the body. A
  request target not starting with `/`, or a `%` not followed by two hex
  digits in the path: `400 Bad Request`. A handler's `err` stays an `err`
  (`http.serve` turns it into its 500).
- **Mounting** — `http.mount("/api", sub)` hands `/api/items?x=1` to `sub` as
  `/items?x=1`; parameters the prefix bound (`/orgs/{org}`) stay visible to
  `sub`.
- **Middleware** — `type HttpMiddleware = (HttpHandler) -> HttpHandler`.
  `http.wrap(app, [a, b])` is `a(b(app))`: the first in the list is the
  outermost. A middleware may answer without calling the handler it wraps.
  Per-request data travels as arguments; there is no mutable context.
- **Request bodies** — `http.decode_json[T](req)` parses the body into an
  input type `T`: a record or variant declared `: Decode` and not also
  `Encode`, whose every field is input (C-436). A `Codec` type — one the
  program also sends or stores — is the check error E110, and so is a `Codec`
  record nested in `T`: decode into the input type and build the domain value
  in code, so a field the client must not set (`is_admin`) has no way in. The
  body decodes with `T.decode_strict`; its `err` is a ready `400 Bad Request`
  response naming why: not JSON, a wrong field type, a missing field, or a
  key outside `T` at any depth (`unknown key '<k>'`, with its path, such as
  `unknown key 'verified', at address`).
- **Out of scope** — built-in logger / CORS / sessions, streaming responses
  and WebSocket are left to packages.

### `http.response(status: Int, body: String) -> HttpResponse`

Create a plain text HTTP response with status code. Seeds
`Content-Type: text/plain` — the signature gives the caller no other way to
name one.

```almd run
import http

fn main() -> Unit = {
  let resp = http.response(200, "Hello!")
  println(http.body(resp))
  println(http.header(resp, "Content-Type") ?? "none")
}
```
```output
Hello!
text/plain
```

### `http.json(status: Int, body: String) -> HttpResponse`

Create a JSON HTTP response with status code

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

### `http.with_headers(status: Int, body: String, headers: List[(String, String)]) -> HttpResponse`

Create a response with EXACTLY the given headers, in list order. Unlike
`response` / `json` it seeds nothing — pass `"Content-Type"` yourself when you
want one (ALS-R7, contract C-275). A name given twice, in any case, keeps its
first spelling and its last value.

```almd run
import http

fn main() -> Unit = {
  let body = "<h1>Hello</h1>"
  let resp = http.with_headers(200, body, [("Content-Type", "text/html")])
  println(http.header(resp, "Content-Type") ?? "none")
  println(http.header(resp, "X-Nope") ?? "none")
}
```
```output
text/html
none
```

### `http.redirect(url: String) -> HttpResponse`

Create a 302 temporary redirect response

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

Set the status code on a response. The code is not range-checked here;
`http.serve` refuses a status outside `200..=599` when it sends the response.

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

Read the status code of a response — the getter twin of `http.set_status`.
A fetched response whose status line could not be read is `0`.

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

## Headers and body of either message

Every function in this section takes an `HttpRequest` or an `HttpResponse`
(`[M: HttpMessage]`) and, for the writers, returns the same kind. A request
built with `http.new_request` and a response built with `http.response` are
read and changed with the same calls.

### `http.header(m, name: String) -> Option[String]`

The FIRST value of one header field; `none` when the field is absent. Field
names are case-insensitive (RFC 9110 §5.1), so `"content-type"` and
`"Content-Type"` are the same field.

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
text/plain
text/plain
none
Bearer t
```

### `http.header_values(m, name: String) -> List[String]`

EVERY value of one field, in wire order — the accessor for a field that
repeats (`Set-Cookie`), where `header` answers only the first occurrence. The
lookup is case-insensitive; an absent field is `[]`.

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

Every header as a `(name, value)` pair, with the name spelled as it was sent
or set, in wire order, a repeated field kept once per occurrence. Nothing is
lowercased or merged: look a field up with `header` / `header_values`, which
ignore case.

```almd run
import http

fn main() -> Unit = {
  let resp = http.with_headers(200, "", [("X-Frame-Options", "DENY"), ("Content-Type", "text/html")])
  let lines = http.headers(resp) |> list.map(((k, v)) => "${k}: ${v}")
  println(list.join(lines, "\n"))
}
```
```output
X-Frame-Options: DENY
Content-Type: text/html
```

### `http.body(m) -> String`

The body text. A body fetched by the client is decoded as lossy UTF-8.

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

The body as `Bytes`: exactly the UTF-8 bytes of `http.body(m)`. A message
body is held as text today, so this does not recover bytes a lossy decode
replaced; a body that is not UTF-8 (an image, audio) is not recovered
byte for byte.

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

Set one header field. Field names are case-insensitive, so the FIRST field
with that name, in any spelling, gets the new value in place (its spelling and
position stay); when there is none, the field is appended. A message never
gains a second entry this way.

```almd run
import http

fn main() -> Unit = {
  let resp = http.with_headers(200, "hi", [("Content-Type", "text/plain")])
  let tagged = http.set_header(http.set_header(resp, "X-Custom", "value"), "content-type", "text/html")
  let lines = http.headers(tagged) |> list.map(((k, v)) => "${k}: ${v}")
  println(list.join(lines, "\n"))
}
```
```output
Content-Type: text/html
X-Custom: value
```

### `http.set_body(m, body: String) -> M`

Replace the body text; the headers stay as they are.

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

Replace the body with `Bytes`. A message body is held as text today, so the
bytes are decoded as lossy UTF-8: a byte sequence that is not UTF-8 becomes
U+FFFD and does not survive. Use it for text you hold as `Bytes`; it is not a
lossless binary upload.

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

## Reading a request

`http.new_request(method, target, body, headers)` builds a request in code —
the way a test calls an app without a socket, and the way a request is built
for `http.send` / `http.start`, where the target is the full URL.

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

Get the HTTP method of a request as sent (GET, POST, etc.)

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

Get the request target, query string included (`/search?q=x`). For a request
built to send, it is the full URL.

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

Headers and body of a request are read with the shared functions above:

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

A credential in a header is checked, not just looked for: wrap both sides in
`secret.new`, whose `==` on `Secret[String]` is constant time (C-426), and
answer 401 to a missing or wrong one. Plain `==` on the header text returns at
the first differing byte, so its timing leaks the token.

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

The query as `url.parse_query` reads it (C-404, #4117): the target's text after
its FIRST `?`, split on `&`, empty pieces dropped, each pair split at its first
`=` (a bare key has the value `""`), `+` a space, `%XX` decoded as strict
UTF-8. Every pair is kept in arrival order, so `?tag=a&tag=b` gives both. A
truncated or non-hex escape, or percent data that is not UTF-8, is an `err`
(`http.query_params: invalid percent escape`, …). The same on native and wasm.

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

### Decoding a form value

`http` has no decoder of its own: `http.url_decode` was removed. Decode a
form-encoded value (`+` a space, `%XX` a byte) with the `url` module, which
refuses a bad escape or percent data that is not UTF-8 instead of keeping it.
Fall back to the input when it may be malformed:

```almd check
import url

fn main() -> Unit = {
  println(url.decode_form("%E7%8C%AB") ?? "%E7%8C%AB") // "猫"
  println(url.decode_form("a+b%20c") ?? "a+b%20c")
}
```

## Client

Any complete response is `ok` — a 404 and a 3xx included; `err` is a
transport failure only. Redirects are never followed: a 3xx and its
`Location` header are the answer. The one-shot client is a two-by-two grid,
plus the call handle; the body as `Bytes` is `http.body_bytes` of the whole
response:

| | body text | whole response |
|---|---|---|
| GET | `http.get(url)` | `http.get_response(url)` |
| any method, body and headers | `http.request(method, url, body, headers)` | `http.send(http.new_request(method, url, body, headers))` |
| as a handle, with limits | | `http.start(req, limits)` — see [Per-call limits and cancellation](#per-call-limits-and-cancellation) |

There is no per-verb function: a POST is `http.request("POST", url, body,
headers)`, and a POST whose status you need is
`http.send(http.new_request("POST", url, body, headers))`, then
`http.status(resp)` and `http.body(resp)`. A streamed body is read through the
handle (`http.read_new` between `http.poll`s).

### `http.get(url: String) -> Result[String, String]`

Send an HTTP GET request and return the response body.

Every client function reads a response whole or answers
`malformed or incomplete response from "<url>"` (C-408, #4137): a response
with no complete head, a body shorter than its `Content-Length`, a chunked
body without its terminating 0-chunk, and a head over 1 MiB are errors, never
a partial `ok`. The URL is split as `url.parse` reads it where a browser could
disagree (C-402): a backslash or control character and a non-canonical
numeric host (`127.1`, `0x7f.0.0.1`) are refused, surrounding whitespace and
tab/LF/CR are removed, and `.` / `..` segments (`%2e` too) are resolved before
the request is sent.

```almd check
import http

effect fn main() -> Unit = {
  let html = http.get("https://example.com")!
  println(html)
}
```

### `http.request(method: String, url: String, body: String, headers: List[(String, String)]) -> Result[String, String]`

Send a request with any method, body and headers, and return the response
body. Pass `""` for no body and `[]` for no extra headers.

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

#### Read timeout (`ALMIDE_HTTP_TIMEOUT_SECS`)

Every client call that takes no limits waits at most **30 seconds** for the
server to answer. The
`ALMIDE_HTTP_TIMEOUT_SECS` environment variable overrides that DEFAULT for
all of them; `0` means **no timeout** — block until the server responds. It
is the default only: a call that carries its own limits (`http.start`)
ignores it — see
[Per-call limits and cancellation](#per-call-limits-and-cancellation). The
limit is between bytes, not for the whole call, so a server that keeps
talking never trips it.
A slow endpoint (a local LLM evaluating a long prompt routinely needs
30–120 s before the first byte) fails past the limit with:

```
timed out waiting for "http://localhost:8080/v1/chat" (raise ALMIDE_HTTP_TIMEOUT_SECS; 0 = no timeout)
```

```sh
ALMIDE_HTTP_TIMEOUT_SECS=300 ./app   # five minutes
ALMIDE_HTTP_TIMEOUT_SECS=0 ./app     # wait forever
```

The same variable bounds **connecting** (#2825): a dial that gets no answer
gives up after 30 s (Go's dialer default) instead of the OS default (75 s on
macOS, longer on Linux), with the same text.

#### Client limits, per path

| | connect | between reads | whole call | response size |
|---|---|---|---|---|
| `get` / `request` / `get_response` / `send` | 30 s | 30 s | none | 1 GiB |
| `http.start` | `total_ms` | `idle_ms` | `total_ms` | none |

- The 30 s defaults are `ALMIDE_HTTP_TIMEOUT_SECS` (`0` = none).
- The size cap counts the response as received (head and framed body):
  `ALMIDE_HTTP_MAX_RESPONSE_BYTES` overrides it, `0` = no cap. Past it the
  call is `err("response from \"<url>\" is larger than N bytes (raise ALMIDE_HTTP_MAX_RESPONSE_BYTES; 0 = no limit)")`.
- Every path refuses a response HEAD over 1 MiB (Go's default).
- There is no default whole-call deadline — none of curl, Go, Python, Node
  or reqwest has one. Ask for one with `http.start` and `total_ms`.

#### Error texts (ADR-0023 §4.2)

Every client failure is put in a class, and each class has one text. The
text names the URL the program passed (quoted, with `\"`, `\\`, `\t`, `\r`,
`\n`, `\0` and `\u{..}` escapes) and never an OS error number, so the same
failure reads the same natively, on `almide run --target wasm`, and in a
WASI 0.3 component on a stock runtime (`wasmtime run -S http=y`).

The userinfo password never appears: everything between the userinfo's
first `:` and its `@` reads `***`, an empty password included, as Go's
net/http prints it (`http://alice:s3cret@host/p` reads
`"http://alice:***@host/p"`). The user name, host, port, path, query and
fragment are quoted as written, so a credential in the query string reaches
the text. Send credentials in a header instead (CWE-598).

| class | text |
|---|---|
| invalid-url | `invalid URL "<url>": <what is wrong>` |
| refused-header | `invalid header name "<name>": …` / `invalid header value for "<name>": …` / `forbidden header name "<name>": …` |
| dns | `cannot resolve the host of "<url>"` |
| connect | `could not connect to "<url>" (connection refused or unreachable)` |
| timeout | `timed out waiting for "<url>" (raise ALMIDE_HTTP_TIMEOUT_SECS; 0 = no timeout)` |
| tls | `TLS handshake with "<url>" failed` |
| too-large | `response from "<url>" is larger than <n> bytes (raise ALMIDE_HTTP_MAX_RESPONSE_BYTES; 0 = no limit)` |
| protocol | `malformed or incomplete response from "<url>"` |

A proxy's own failures and a trust store that does not load keep their own
texts (they name the proxy or the variable). In a component, a host error
outside these classes reads `http request to "<url>" failed (<wasi:http case>)`,
and a host's `internal-error` text is passed through. The component reads
`ALMIDE_HTTP_TIMEOUT_SECS` and `ALMIDE_HTTP_MAX_RESPONSE_BYTES` from its own
environment (`wasmtime run --env …`); there, `0` timeout leaves the host's
default. Of the URL reasons, the component finds the scheme's three
(missing, unsupported, `expected "//"`) itself; a bad host or port is refused
by the runtime and reads as the unclassified text.

#### URLs (#2821)

URLs are split the way curl, Go and the WHATWG URL standard split them:

- The scheme is case-insensitive and must be `http://` or `https://`. A URL
  without one, or with another scheme, is an error: it is never sent as
  plain HTTP by guesswork.
- `user:password@` becomes `Authorization: Basic …`, unless you pass your
  own `Authorization` header. The userinfo is never taken for the host. A
  WASI 0.3 component sends the same field value.
- IPv6 hosts are bracketed: `http://[::1]:8080/`.
- A port is 1–65535. An unreadable port is an error, never a silent 80.
- An internationalised host is sent as punycode (`bücher.example` →
  `xn--bcher-kva.example`).
- `http://host:8080?x=1` is `GET /?x=1` on port 8080.
- The `#fragment` is not sent.
- Spaces, controls and non-ASCII in the path and query are percent-encoded.
- `Host` carries the port when it is not the scheme's default.

A refused URL is an `err` reading `invalid URL "…": <what is wrong>`, returned
before anything is dialled. `http.start` returns it synchronously.

#### Header injection (#2822)

A header name must be a token (RFC 9110). A header value may not contain CR,
LF, NUL or another control character (HTAB is fine). The method must be a
token too. The fields the client or the runtime manages — `connection`,
`keep-alive`, `proxy-authenticate`, `proxy-authorization`,
`proxy-connection`, `transfer-encoding`, `upgrade`, `host`, `http2-settings`,
in any case — cannot be set (C-370; a stock WASI runtime refuses the same
nine). Otherwise the request is refused before anything is sent: the call is
an `err` starting `invalid header value for "X-A": …`,
`invalid header name …`, `forbidden header name "Host": …` or
`invalid HTTP method …`. The request is never cleaned up and sent anyway.
Python, Go and Node refuse the same input.

#### Proxies (#2819)

Every client path (`get`, `request`, `get_response`, `send`,
`http.start`) reads the proxy variables the way curl and
reqwest read them:

- `https://` URLs use `HTTPS_PROXY` / `https_proxy`.
- `http://` URLs use `HTTP_PROXY` / `http_proxy`. `HTTP_PROXY` is ignored
  when `REQUEST_METHOD` is set (CGI, httpoxy), as in Go and reqwest.
- Both then fall back to `ALL_PROXY` / `all_proxy`.
- `NO_PROXY` / `no_proxy` is a comma-separated list. `*` bypasses the proxy
  for every host. An IP or CIDR block (`10.0.0.0/8`) matches address hosts.
  A name (`example.com`, `.example.com`) matches itself and its subdomains.
  Nothing is exempt by default, `localhost` included (as in curl and
  reqwest).

Proxy schemes:

- `http://` proxies. A proxy without a scheme counts as `http://`.
  `https://` targets are tunnelled with `CONNECT host:port`, and TLS to the
  origin runs inside the tunnel. `http://` targets are sent to the proxy in
  absolute form.
- `socks5://` (names resolved locally) and `socks5h://` (the proxy resolves
  them). The default port is 1080.

`user:password@` in the proxy URL is the proxy login: `Proxy-Authorization:
Basic` for an HTTP proxy, RFC 1929 for SOCKS5. Any other proxy scheme is an
error naming the variable. It is never a silent direct connection. Errors
name the proxy (`proxy 127.0.0.1:3128 (from HTTPS_PROXY) refused CONNECT
api.example.com:443: HTTP/1.1 407 …`) and never repeat its credentials.

#### TLS trust (#2820)

HTTPS trusts two sets of roots:

- the bundled webpki (Mozilla) roots;
- the platform trust store (the macOS keychain, the Windows store, or the
  OpenSSL bundle/directory on Linux), read through `rustls-native-certs`.
  When `SSL_CERT_FILE` or `SSL_CERT_DIR` is set, the PEM certificates they
  name are used instead of the platform store.

So a corporate CA, a test CA or a TLS-inspecting proxy's CA can be trusted
the way curl, Python and Go allow. If a variable names no loadable
certificate, the error says so (`TLS error: could not load the CA
certificates SSL_CERT_FILE names: …`). A certificate or handshake failure
reads as the tls class, not as a failed write:

```
TLS handshake with "https://internal.example/" failed
```

#### Interim responses and chunk extensions (#2824)

- `1xx` responses before the final one (`100 Continue`, `103 Early Hints`)
  are skipped, on every path. `101` is final.
- A chunk-size line's `;extension` is ignored. A size line that is not hex
  is an error (the protocol class, `malformed or incomplete response from
  "<url>"`), not an empty body.

### The whole response — `get_response` and `send`

`http.get_response(url)` and `http.send(req)` answer the whole `HttpResponse`
instead of the body text. Both come from the same exchange in the runtime as
`get` / `request`, so the shapes cannot drift: `get` is the `body` of
`get_response`, and `request(method, url, body, headers)` is the `body` of
`send(new_request(method, url, body, headers))`.

What the response holds:

- `http.status(resp)` — **any** complete response is `ok`: a 404 and a
  3xx included. `err` is a transport failure only (connection / TLS /
  timeout).
- **Redirects are never followed.** A 3xx arrives as-is with its `Location`
  header, so the response is always to the URL you passed — there is no
  separate "final URL".
- Headers keep their wire spelling and a repeated field keeps **every**
  occurrence: `http.header(resp, k)` answers the first,
  `http.header_values(resp, k)` all of them, `http.headers(resp)` every
  `(name, value)` pair in wire order.
- `http.body(resp)` — the transfer-decoded body text, lossy UTF-8.

On wasm, `almide run --target wasm` and the WASI 0.3 component
(`ALMIDE_COMPONENT_P3=1 --component`, run on wasmtime 46+ with `-S http`)
serve the whole-response client; the default stock artifact refuses it at
build. `proofs/target-availability.toml` declares the legs.

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

Send a request built with `http.new_request`, whose target is the full URL,
and return the whole response, any status. Redirects are not followed, so a
redirect check reads the 3xx and its `Location` straight off the response:

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

Any method; read every value of a repeated field with `header_values`:

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

The request is a value, so it can be built up before it is sent:

```almd check
import http

effect fn main() -> Unit = {
  let empty = http.new_request("PATCH", "https://api.example.com/items/1", "", [])
  let req = http.set_body(http.set_header(empty, "Content-Type", "application/json"), '{"name": "bob"}')
  let resp = http.send(req)!
  println("${http.status(resp)} ${http.body(resp)}")
}
```

## Per-call limits and cancellation

A call can be held as a handle, `HttpCall`, instead of blocking until it
ends. `http.start` returns at once; a runtime thread does the exchange while
the program keeps running its own loop — reading keys, redrawing, starting a
second request.

| | |
|---|---|
| `http.start(req, limits) -> Result[HttpCall, String]` | begins sending `req` (built with `http.new_request`, target the full URL) and returns at once |
| `http.poll(c) -> Option[Result[HttpResponse, String]]` | never blocks: `none` while the call runs, then its result |
| `http.read_new(c) -> String` | never blocks: the body text that arrived since the previous `read_new` |
| `http.wait(c) -> Result[HttpResponse, String]` | blocks until the call ends — never past its limits |
| `http.cancel(c) -> Unit` | closes the connection; the call ends as `err("request cancelled")` |

**Family rule** (machine-checked by `tests/http_call_family_gate_test.rs`):
the handle has exactly these five verbs. A streamed body is read with
`read_new` between `poll`s; there is no callback client.

**Limits are per call.** `HttpLimits = { total_ms: Int, idle_ms: Int }`, both
in milliseconds, `0` = no limit:

- `total_ms` is a wall clock that starts at `http.start`: dialing, the wait
  for the first byte and the body all count. A server that keeps talking is
  still stopped at it. It is checked by the runtime's thread and by every
  `poll` / `read_new` / `wait`.
- `idle_ms` is the longest gap between two arrivals of bytes; the wait for
  the first byte counts as a gap.

A limit that fires ends the call with an error that names it:

```
request timeout: total_ms 5000 exceeded
request timeout: idle_ms 60000 exceeded
```

`ALMIDE_HTTP_TIMEOUT_SECS` does not apply to a call with limits — it stays the
default of the calls that take none. So "this LLM call may take 30 minutes,
this page fetch 10 seconds" is two `start` calls with two limits, in one
process.

**Ending a call closes the connection.** When the call ends — answered,
failed, timed out, cancelled — the runtime shuts the socket down, so the
server sees the close at once and nothing more arrives. After `cancel`,
`read_new` returns `""` (bytes that arrived but were not read are dropped) and
`poll` / `wait` answer `err("request cancelled")`. `cancel` on a call that
already ended changes nothing, and calling it twice is fine. Dropping the last
copy of the handle cancels the call too.

**The answer is the whole response.** `wait` and `poll` answer like
`http.send`: any complete response is `ok` — a 404 included — with its
status, every header line and the whole body; `err` is a transport failure, a
fired limit or a cancel. The body bytes `read_new` handed out are still in
that response's body. A multibyte character split across two reads is held
back until it is whole.

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

A streaming POST — an LLM endpoint that sends server-sent events — is the
same loop over a request with a body and headers:

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

(The OpenAI and Anthropic streaming clients moved to package
[almide/llm](https://github.com/almide/llm) at dialect epoch 14:
`almide add almide/llm@v0.1.0`, `import llm`, then
`llm.openai_streaming_call(...)`.)

On the wasm target, `almide run --target wasm` serves `start`, `poll`,
`read_new`, `wait` and `cancel` with the same behaviour as native — the
embedded host runs the native call core, so the limit and cancel errors are
the same text, and dropping the last copy of a handle cancels its call there
too. A standalone `.wasm` built with `almide build --target wasm` cannot
carry them yet (there is no stock WASI host for an in-flight call), so
`almide check --target wasm`, which checks the build route, still refuses
them.

<!-- BEGIN GENERATED SIGNATURE INDEX (make stdlib-docs) — do not edit by hand -->

## Signature index (34 functions)

```
// Serves 0.0.0.0:port forever; handler err is a 500.
// @since 0.5.0 or earlier
effect http.serve(port: Int, handler: (HttpRequest) -> Result[HttpResponse, String]) -> Unit

// Reply with Content-Type text/plain.
// @since 0.5.0 or earlier
http.response(status: Int, body: String) -> HttpResponse

// Reply with Content-Type application/json.
// @since 0.5.0 or earlier
http.json(status: Int, body: String) -> HttpResponse

// Reply with exactly these headers in order; a name given twice, in any case, keeps its first spelling and last value.
// @since 0.5.0 or earlier
http.with_headers(status: Int, body: String, headers: List[(String, String)]) -> HttpResponse

// 302 with Location url and empty body.
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

// Body as bytes: the UTF-8 bytes of body(m).
// @since unreleased
http.body_bytes(m: M) -> Bytes

// m with header name set, any case: the first match replaced in place, else appended.
// @since 0.6.0 or earlier
http.set_header(m: M, name: String, value: String) -> M

// m with its body replaced.
// @since unreleased
http.set_body(m: M, body: String) -> M

// m with its body replaced by body, decoded as lossy UTF-8.
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
http.route(pattern: String, handler: (HttpRequest) -> Result[HttpResponse, String]) -> HttpRoute

// Sub-app under prefix: /api/x reaches it as /x.
// @since unreleased
http.mount(prefix: String, sub: (HttpRequest) -> Result[HttpResponse, String]) -> HttpRoute

// Route table as a handler; err on conflicting routes.
// @since unreleased
http.router(routes: List[HttpRoute]) -> Result[(HttpRequest) -> Result[HttpResponse, String], String]

// Applies middleware; the first is the outermost.
// @since unreleased
http.wrap(handler: (HttpRequest) -> Result[HttpResponse, String], middleware: List[((HttpRequest) -> Result[HttpResponse, String]) -> (HttpRequest) -> Result[HttpResponse, String]]) -> (HttpRequest) -> Result[HttpResponse, String]

// JSON body into input type T, strictly; err is a 400 response.
// @since unreleased
http.decode_json(req: HttpRequest) -> Result[T, HttpResponse]
```

## Type index (4 types)

```
// Per-call limits in ms; 0 = no limit.
// @since unreleased
type http.HttpLimits = { total_ms: Int, idle_ms: Int }

// Request to response; a router is one too.
// @since unreleased
type http.HttpHandler = (HttpRequest) -> Result[HttpResponse, String]

// Wraps a handler: (next) => (req) => … next(req)! ….
// @since unreleased
type http.HttpMiddleware = ((HttpRequest) -> Result[HttpResponse, String]) -> (HttpRequest) -> Result[HttpResponse, String]

// Route table row; build it with route or mount.
// @since unreleased
type http.HttpRoute = { method: String, pattern: String, segments: List[String], handler: (HttpRequest) -> Result[HttpResponse, String] }
```

<!-- END GENERATED SIGNATURE INDEX -->
