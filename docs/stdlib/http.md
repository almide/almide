# http

HTTP client and server. import http, effect.

`HttpRequest` / `HttpResponse` are the module's runtime-backed nominal types.
With `import http` they resolve in user annotations too — bare or qualified —
so typed helpers over requests are writable:

```almd check
import http

fn handle(req: HttpRequest) -> http.HttpResponse = http.response(200, http.req_path(req))

effect fn main() -> Unit = {
  http.serve(3000, handle)
}
```

### `http.serve(port: Int, f: (HttpRequest) -> HttpResponse) -> Unit`

Start an HTTP server on the given port with a request handler

```almd check
import http

effect fn main() -> Unit = {
  http.serve(3000, (req) => http.response(200, "ok"))
}
```

The server listens on `0.0.0.0:port` and handles one request at a time, in
the order they arrive. The response goes out as `HTTP/1.1 <status> <reason>`,
the response's headers in order, `Content-Length`, the body, and then the
connection closes. A handler `err(m)` answers `500` with the body
`Internal error: <m>`. If the port cannot be bound, the program stops with
`Error: bind failed: <reason>` and exit code 1.

**On wasm** (C-367): `almide run app.almd --target wasm` serves the same
program on the embedded host. One instance handles every request, exactly as
the native process does: `main` runs once, and a value it computed before
`http.serve` is the same on every request. The status line, headers and body
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
served by the incumbent leg today, which runs through the `wasmtime` CLI
(#2664).

```almd check
import http

type User: Codec = { name: String, age: Int }

effect fn get_user(req: HttpRequest) -> HttpResponse = http.response(200, "user " + (http.param(req, "id") ?? ""))

effect fn create_user(req: HttpRequest) -> HttpResponse = match http.decode_json(req, (v) => User.decode(v)) {
  ok(u) => http.response(201, u.name),
  err(bad) => bad,
}

fn server_header(next: HttpHandler) -> HttpHandler = (req) => http.set_header(next(req)!, "Server", "almide")

effect fn show(app: HttpHandler, method: String, target: String, body: String) -> String = {
  let res = app(http.new_request(method, target, body, [:]))!
  "${http.status_code(res)} ${http.body(res)}"
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
  println(show(app, "DELETE", "/users/42", "")!)
  println(show(app, "GET", "/nope", "")!)
}
```
```text
200 user 42
201 ann
400 Bad Request: missing field 'name'
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
- **Bad input** — `http.decode_json(req, decode)` parses the body and runs a
  Codec decoder; its `err` is a ready `400 Bad Request` response naming why.
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
  println(http.get_header(resp, "Content-Type") ?? "none")
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
  println(http.get_header(resp, "Content-Type") ?? "none")
}
```
```output
{"ok":true}
application/json
```

### `http.with_headers(status: Int, body: String, headers: Map[String, String]) -> HttpResponse`

Create a response with EXACTLY the given headers, in map order. Unlike
`response` / `json` it seeds nothing — pass `"Content-Type"` yourself when you
want one (ALS-R7, contract C-275).

```almd run
import http

fn main() -> Unit = {
  let body = "<h1>Hello</h1>"
  let resp = http.with_headers(200, body, ["Content-Type": "text/html"])
  println(http.get_header(resp, "Content-Type") ?? "none")
  println(http.get_header(resp, "X-Nope") ?? "none")
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
  println(http.get_header(resp, "Location") ?? "none")
}
```
```output
/new-path
```

### `http.status(resp: HttpResponse, code: Int) -> HttpResponse`

Set the status code on a response

```almd run
import http

fn main() -> Unit = {
  let resp = http.response(200, "created")
  let created = http.status(resp, 201)
  println(http.body(created))
}
```
```output
created
```

### `http.body(resp: HttpResponse) -> String`

Get the body string from a response

```almd run
import http

fn main() -> Unit = {
  let resp = http.response(200, "Hello!")
  let text = http.body(resp)
  println(text)
}
```
```output
Hello!
```

### `http.set_header(resp: HttpResponse, key: String, value: String) -> HttpResponse`

Set a header on a response. Field names are case-insensitive (RFC 9110 §5.1),
so this replaces the existing field's value whatever spelling it was stored
under — a response never carries two entries for one field name.

```almd run
import http

fn main() -> Unit = {
  let resp = http.response(200, "hi")
  let tagged = http.set_header(resp, "X-Custom", "value")
  println(http.get_header(tagged, "x-custom") ?? "none")
}
```
```output
value
```

### `http.get_header(resp: HttpResponse, key: String) -> Option[String]`

Get a header value from a response; `none` when the field is absent. The
lookup is case-insensitive, so `"content-type"` and `"Content-Type"` are the
same field.

```almd run
import http

fn main() -> Unit = {
  let resp = http.response(200, "hi")
  let ct = http.get_header(resp, "Content-Type")
  println(ct ?? "none")
  println(http.get_header(resp, "content-type") ?? "none")
  println(http.get_header(resp, "X-Nope") ?? "none")
}
```
```output
text/plain
text/plain
none
```

### `http.status_code(resp: HttpResponse) -> Int`

Read the status code of a response — the getter twin of `http.status`.

```almd run
import http

fn main() -> Unit = {
  let resp = http.status(http.response(200, "teapot"), 418)
  println("${http.status_code(resp)}")
}
```
```output
418
```

### `http.header_values(resp: HttpResponse, key: String) -> List[String]`

EVERY value of one field, in order — the accessor for a field that repeats
(`Set-Cookie`), where `get_header` answers only the first occurrence. The
lookup is case-insensitive; an absent field is `[]`.

```almd run
import http

fn main() -> Unit = {
  let resp = http.with_headers(200, "", ["Set-Cookie": "a=1; HttpOnly"])
  println("${http.header_values(resp, "set-cookie")}")
  println("${http.header_values(resp, "X-Nope")}")
}
```
```output
["a=1; HttpOnly"]
[]
```

### `http.headers(resp: HttpResponse) -> Map[String, String]`

All headers as a map keyed by the **lowercased** field name (RFC 9110 §5.1:
field names are case-insensitive ASCII tokens). A repeated field keeps its
**first** value — the rule `get_header` and `req_header` already apply — so a
`Set-Cookie` list is read through `header_values`, never through this map.

```almd run
import http

fn main() -> Unit = {
  let resp = http.with_headers(200, "", ["X-Frame-Options": "DENY", "Content-Type": "text/html"])
  let hs = http.headers(resp)
  println(map.get(hs, "x-frame-options") ?? "none")
  println(map.get(hs, "X-Frame-Options") ?? "none")
  println("${map.keys(hs)}")
}
```
```output
DENY
none
["x-frame-options", "content-type"]
```

### `http.req_method(req: HttpRequest) -> String`

Get the HTTP method of a request (GET, POST, etc.)

```almd check
import http

fn handle(req: HttpRequest) -> HttpResponse = {
  let method = http.req_method(req)
  http.response(200, "method: ${method}")
}

effect fn main() -> Unit = {
  http.serve(3000, handle)
}
```

### `http.req_path(req: HttpRequest) -> String`

Get the URL path of a request

```almd check
import http

fn handle(req: HttpRequest) -> HttpResponse = {
  let path = http.req_path(req)
  http.response(200, "path: ${path}")
}

effect fn main() -> Unit = {
  http.serve(3000, handle)
}
```

### `http.req_body(req: HttpRequest) -> String`

Get the body string of a request

```almd check
import http

fn handle(req: HttpRequest) -> HttpResponse = {
  let body = http.req_body(req)
  http.response(200, "received ${string.len(body)} chars")
}

effect fn main() -> Unit = {
  http.serve(3000, handle)
}
```

### `http.req_header(req: HttpRequest, key: String) -> Option[String]`

Get a header value from a request

```almd check
import http

fn handle(req: HttpRequest) -> HttpResponse = {
  let auth = http.req_header(req, "Authorization")
  match auth {
    some(_) => http.response(200, "ok"),
    none => http.response(401, "missing Authorization"),
  }
}

effect fn main() -> Unit = {
  http.serve(3000, handle)
}
```

### `http.query_params(req: HttpRequest) -> Map[String, String]`

Get all query parameters from a request as a map. Values are percent-decoded
(`%XX` → byte, `+` → space), so `?q=%E7%8C%AB` yields `{"q": "猫"}`.

```almd check
import http

fn handle(req: HttpRequest) -> HttpResponse = {
  let params = http.query_params(req) // {"page": "1", "q": "test"}
  let q = map.get(params, "q") ?? ""
  http.response(200, "q=${q}")
}

effect fn main() -> Unit = {
  http.serve(3000, handle)
}
```

### `http.url_decode(s: String) -> String`

Percent-decode a URL component (`%XX` → byte, `+` → space). `query_params`
already decodes its values; use this for manually-extracted query/form text.

```almd check
import http

fn main() -> Unit = {
  let q = http.url_decode("%E7%8C%AB") // "猫"
  println(q)
  println(http.url_decode("a+b%20c"))
}
```

### `http.get(url: String) -> Result[String, String]`

Send an HTTP GET request and return the response body

```almd check
import http

effect fn main() -> Unit = {
  let html = http.get("https://example.com")!
  println(html)
}
```

### `http.post(url: String, body: String) -> Result[String, String]`

Send an HTTP POST request with a body string

```almd check
import http

effect fn main() -> Unit = {
  let body = '{"name": "alice"}'
  let resp = http.post("https://api.example.com", body)!
  println(resp)
}
```

### `http.put(url: String, body: String) -> Result[String, String]`

Send an HTTP PUT request

```almd check
import http

effect fn main() -> Unit = {
  let url = "https://api.example.com/items/1"
  let body = '{"name": "alice"}'
  let resp = http.put(url, body)!
  println(resp)
}
```

### `http.patch(url: String, body: String) -> Result[String, String]`

Send an HTTP PATCH request

```almd check
import http

effect fn main() -> Unit = {
  let url = "https://api.example.com/items/1"
  let body = '{"name": "bob"}'
  let resp = http.patch(url, body)!
  println(resp)
}
```

### `http.delete(url: String) -> Result[String, String]`

Send an HTTP DELETE request

```almd check
import http

effect fn main() -> Unit = {
  let url = "https://api.example.com/items/1"
  let resp = http.delete(url)!
  println(resp)
}
```

### `http.request(method: String, url: String, body: String, headers: Map[String, String]) -> Result[String, String]`

Send a custom HTTP request with method, URL, body, and headers

```almd check
import http

effect fn main() -> Unit = {
  let url = "https://api.example.com/items/1"
  let body = '{"name": "alice"}'
  let headers = ["Content-Type": "application/json", "User-Agent": "my-app"]
  let resp = http.request("PUT", url, body, headers)!
  println(resp)
}
```

#### Read timeout (`ALMIDE_HTTP_TIMEOUT_SECS`)

Every client call that takes no limits waits at most **30 seconds** for the
server to answer (the SSE streaming client: 120 s between events). The
`ALMIDE_HTTP_TIMEOUT_SECS` environment variable overrides that DEFAULT for
all of them; `0` means **no timeout** — block until the server responds. It
is the default only: a call that carries its own limits (`http.start`, the
`*_with_limits` streaming clients) ignores it — see
[Per-call limits and cancellation](#per-call-limits-and-cancellation). The
limit is between bytes, not for the whole call, so a server that keeps
talking never trips it.
A slow endpoint (a local LLM evaluating a long prompt routinely needs
30–120 s before the first byte) fails past the limit with:

```
read timed out waiting for the server (raise ALMIDE_HTTP_TIMEOUT_SECS; 0 = no timeout)
```

```sh
ALMIDE_HTTP_TIMEOUT_SECS=300 ./app   # five minutes
ALMIDE_HTTP_TIMEOUT_SECS=0 ./app     # wait forever
```

### `http.get_status(url: String) -> Result[(Int, String), String]`

Send a GET and return `(status_code, body)`. Unlike `http.get`, a non-2xx
response is `Ok((code, body))` — a 404 does not become an `Err`. `Err` is
reserved for transport failures (connection / TLS / timeout). Use this when a
caller needs to branch on the numeric status rather than only body-or-error.

```almd check
import http

effect fn main() -> Unit = {
  let verdict = match http.get_status("https://example.com/x") {
    Ok(pair) => if pair.0 == 404 then "missing" else "ok"
    Err(e) => "network error: " + e
  }
  println(verdict)
}
```

### `http.request_status(method: String, url: String, body: String, headers: Map[String, String]) -> Result[(Int, String), String]`

Like `http.request` but returns `(status_code, body)`, with the same status
semantics as `http.get_status` (any complete response is `Ok`). Set custom
headers (e.g. a `User-Agent`) via the `headers` map.

```almd check
import http

effect fn main() -> Unit = {
  let url = "https://example.com/x"
  let r = http.request_status("GET", url, "", ["User-Agent": "my-app"])
  match r {
    ok(pair) => println("status ${pair.0}"),
    err(e) => println("network error: " + e),
  }
}
```

### The `*_response` family — status, headers and body together

Every verb-shaped String client has a `*_response` twin with the **same
parameters** that answers the whole `HttpResponse` record instead of the body:

| body only | full response |
|---|---|
| `http.get(url)` | `http.get_response(url)` |
| `http.post(url, body)` | `http.post_response(url, body)` |
| `http.put(url, body)` | `http.put_response(url, body)` |
| `http.patch(url, body)` | `http.patch_response(url, body)` |
| `http.delete(url)` | `http.delete_response(url)` |
| `http.request(method, url, body, headers)` | `http.request_response(method, url, body, headers)` |

**Family rule** (machine-checked by `tests/http_response_family_gate_test.rs`):
each of `get` / `post` / `put` / `patch` / `delete` / `request` has exactly one
`<verb>_response` twin, and nothing else grows one. The body-only fn is the
`body` projection of its twin and `request_status` the `(status, body)`
projection — all three shapes come from one exchange in the runtime, so they
cannot drift. Intentional omissions: `get_status` / `get_bytes` /
`request_bytes` are result-*shape* variants, not verbs (they get no twin), and
`request_stream` carries no response record (its body goes chunk-wise to the
callback).

What the record holds:

- `http.status_code(resp)` — **any** complete response is `Ok`: a 404 and a
  3xx included. `Err` is a transport failure only (connection / TLS /
  timeout).
- **Redirects are never followed.** A 3xx arrives as-is with its `Location`
  header, so the response is always to the URL you passed — there is no
  separate "final URL".
- Headers keep their wire spelling and a repeated field keeps **every**
  occurrence: `http.get_header(resp, k)` answers the first, `http.header_values(resp, k)`
  all of them, `http.headers(resp)` the lowercased-name map (first wins).
- `http.body(resp)` — the transfer-decoded body text.

The twins are **native-only** today: the embedded wasm lane serves the
body / status / bytes shapes through the framed ops (#1710 increment 3) and
grows the response shape when the wasi:http port lands (#1710);
`proofs/target-availability.toml` declares the legs.

### `http.get_response(url: String) -> Result[HttpResponse, String]`

```almd check
import http

effect fn main() -> Unit = {
  let resp = http.get_response("https://example.com/")!
  println("status ${http.status_code(resp)}")
  println(http.get_header(resp, "strict-transport-security") ?? "no HSTS")
  println(http.get_header(resp, "x-frame-options") ?? "no X-Frame-Options")
}
```

### `http.post_response(url: String, body: String) -> Result[HttpResponse, String]`

```almd check
import http

effect fn main() -> Unit = {
  let resp = http.post_response("https://api.example.com/login", '{"user": "alice"}')!
  for cookie in http.header_values(resp, "set-cookie") {
    println(cookie)
  }
}
```

### `http.put_response(url: String, body: String) -> Result[HttpResponse, String]`

```almd check
import http

effect fn main() -> Unit = {
  let resp = http.put_response("https://api.example.com/items/1", '{"name": "alice"}')!
  println("${http.status_code(resp) == 204}")
}
```

### `http.patch_response(url: String, body: String) -> Result[HttpResponse, String]`

```almd check
import http

effect fn main() -> Unit = {
  let resp = http.patch_response("https://api.example.com/items/1", '{"name": "bob"}')!
  println(http.body(resp))
}
```

### `http.delete_response(url: String) -> Result[HttpResponse, String]`

```almd check
import http

effect fn main() -> Unit = {
  let resp = http.delete_response("https://api.example.com/items/1")!
  println("${http.status_code(resp)}")
}
```

### `http.request_response(method: String, url: String, body: String, headers: Map[String, String]) -> Result[HttpResponse, String]`

The general form. A redirect check reads the 3xx and its `Location` straight
off the record:

```almd check
import http

effect fn main() -> Unit = {
  let resp = http.request_response("GET", "http://example.com/old", "", ["User-Agent": "checker"])!
  let code = http.status_code(resp)
  if code >= 300 and code < 400 then
    println("redirects to ${http.get_header(resp, "location") ?? "?"}")
  else
    println("answers ${code} directly")
}
```

### `http.get_bytes(url: String) -> Result[Bytes, String]`

Send an HTTP GET and return the raw response body as `Bytes` (no UTF-8
conversion), so binary payloads such as images or TTS audio survive intact.
The `String` client corrupts non-UTF-8 bodies.

```almd check
import fs
import http

effect fn main() -> Unit = {
  let audio = http.get_bytes("https://tts.example.com/say.mp3")!
  fs.write_bytes_raw("say.mp3", audio)!
}
```

### `http.request_bytes(method: String, url: String, body: String, headers: Map[String, String]) -> Result[Bytes, String]`

Like `http.request` but returns the raw response body as `Bytes`.

```almd check
import http

effect fn main() -> Unit = {
  let url = "https://api.example.com/render"
  let body = '{"text": "hello"}'
  let headers = ["Content-Type": "application/json"]
  let blob = http.request_bytes("POST", url, body, headers)!
  println(int.to_string(bytes.len(blob)))
}
```

## Per-call limits and cancellation

A call can be held as a handle, `HttpCall`, instead of blocking until it
ends. `http.start` returns at once; a runtime thread does the exchange while
the program keeps running its own loop — reading keys, redrawing, starting a
second request.

| | |
|---|---|
| `http.start(method, url, body, headers, limits) -> Result[HttpCall, String]` | begins the request and returns at once |
| `http.poll(c) -> Option[Result[HttpResponse, String]]` | never blocks: `none` while the call runs, then its result |
| `http.read_new(c) -> String` | never blocks: the body text that arrived since the previous `read_new` |
| `http.wait(c) -> Result[HttpResponse, String]` | blocks until the call ends — never past its limits |
| `http.cancel(c) -> Unit` | closes the connection; the call ends as `err("request cancelled")` |

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
`http.request_response`: any complete response is `ok` — a 404 included —
with its status, every header line and the whole body; `err` is a transport
failure, a fired limit or a cancel. The body bytes `read_new` handed out are
still in that response's body. A multibyte character split across two reads
is held back until it is whole.

```almd check
import env
import http
import io

// Print the stream as it arrives; stop after 3 s whatever the server does.
effect fn follow(c: HttpCall, started: Int) -> Result[String, String] = {
  io.print(http.read_new(c))
  match http.poll(c) {
    some(ok(resp)) => ok("done: ${http.status_code(resp)}"),
    some(err(e)) => ok(e),
    none => if env.millis() - started > 3000 then {
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
  let c = http.start("GET", "http://127.0.0.1:8080/events", "", [:], limits)!
  println(follow(c, env.millis())!)
}
```

### The streaming clients with limits

`request_stream`, `openai_streaming_call` and `anthropic_streaming_call` each
have a `_with_limits` twin: the same parameters with `limits: HttpLimits`
before the callback, the same answer, built on the handle — so a stream that
keeps talking ends at `total_ms`, and one that stalls ends at `idle_ms`, each
with the error that names it. The forms without limits are unchanged.

```almd check
import http

effect fn main() -> Unit = {
  let limits = { total_ms: 10 * 60 * 1000, idle_ms: 2 * 60 * 1000 }
  let body = """{"model": "m", "stream": true, "messages": []}"""
  let answer = http.openai_streaming_call_with_limits("http://127.0.0.1:8080/v1", "key", body, limits, (delta) => eprintln(delta))!
  println(answer)
}
```

On the wasm target, `almide run --target wasm` serves `start`, `poll`,
`read_new`, `wait`, `cancel` and `request_stream_with_limits` with the same
behaviour as native — the embedded host runs the native call core, so the
limit and cancel errors are the same text, and dropping the last copy of a
handle cancels its call there too. A standalone `.wasm` built with
`almide build --target wasm` cannot carry them yet (there is no stock WASI
host for an in-flight call), so `almide check --target wasm`, which checks
the build route, still refuses them. `openai_streaming_call_with_limits` and
`anthropic_streaming_call_with_limits` stay native-only, like the streaming
helpers they extend.

<!-- BEGIN GENERATED SIGNATURE INDEX (make stdlib-docs) — do not edit by hand -->

## Signature index (52 functions)

```
// Serves 0.0.0.0:port forever; handler err is a 500.
// @since 0.5.0 or earlier
effect http.serve(port: Int, f: (HttpRequest) -> Result[HttpResponse, String]) -> Unit

// Reply with Content-Type text/plain.
// @since 0.5.0 or earlier
http.response(status: Int, body: String) -> HttpResponse

// Reply with Content-Type application/json.
// @since 0.5.0 or earlier
http.json(status: Int, body: String) -> HttpResponse

// Reply with headers as-is; no default Content-Type.
// @since 0.5.0 or earlier
http.with_headers(status: Int, body: String, headers: Map[String, String]) -> HttpResponse

// 302 with Location url and empty body.
// @since 0.6.0 or earlier
http.redirect(url: String) -> HttpResponse

// resp with code set; not range-checked.
// @since 0.6.0 or earlier
http.status(resp: HttpResponse, code: Int) -> HttpResponse

// Payload text; lossy UTF-8 when fetched.
// @since 0.6.0 or earlier
http.body(resp: HttpResponse) -> String

// Case-insensitive upsert of header key.
// @since 0.6.0 or earlier
http.set_header(resp: HttpResponse, key: String, value: String) -> HttpResponse

// First value for key, any case; none if absent.
// @since 0.6.0 or earlier
http.get_header(resp: HttpResponse, key: String) -> Option[String]

// HTTP status; 0 if the status line was bad.
// @since 0.62.0
http.status_code(resp: HttpResponse) -> Int

// Map by lowercased name; first value wins.
// @since 0.62.0
http.headers(resp: HttpResponse) -> Map[String, String]

// Every value for key, wire order; [] if absent.
// @since 0.62.0
http.header_values(resp: HttpResponse, key: String) -> List[String]

// Request method as sent, e.g. GET.
// @since 0.6.0 or earlier
http.req_method(req: HttpRequest) -> String

// Request target, query string included.
// @since 0.6.0 or earlier
http.req_path(req: HttpRequest) -> String

// Body text; empty without a Content-Length.
// @since 0.6.0 or earlier
http.req_body(req: HttpRequest) -> String

// First value for key, any case; none if absent.
// @since 0.6.0 or earlier
http.req_header(req: HttpRequest, key: String) -> Option[String]

// Decoded query map; last duplicate wins.
// @since 0.6.0 or earlier
http.query_params(req: HttpRequest) -> Map[String, String]

// + to space, %XX to byte; bad escapes kept.
// @since 0.27.7 or earlier
http.url_decode(s: String) -> String

// Body even for a 404; err only on transport.
// @since 0.5.0 or earlier
effect http.get(url: String) -> String

// Reply body of a POST; JSON type by default.
// @since 0.5.0 or earlier
effect http.post(url: String, body: String) -> String

// Reply body of a PUT; JSON type by default.
// @since 0.6.0 or earlier
effect http.put(url: String, body: String) -> String

// Reply body of a PATCH; JSON type by default.
// @since 0.6.0 or earlier
effect http.patch(url: String, body: String) -> String

// Reply body of a DELETE; any status is ok.
// @since 0.6.0 or earlier
effect http.delete(url: String) -> String

// Reply body for any method; any status is ok.
// @since 0.5.0 or earlier
effect http.request(method: String, url: String, body: String, headers: Map[String, String]) -> String

// (status, body); a 404 is ok, not err.
// @since 0.61.0
effect http.get_status(url: String) -> (Int, String)

// (status, body) for any method; 3xx not followed.
// @since 0.61.0
effect http.request_status(method: String, url: String, body: String, headers: Map[String, String]) -> (Int, String)

// Whole reply, 404 included; redirects not followed.
// @since 0.62.0
effect http.get_response(url: String) -> HttpResponse

// Whole reply to a POST; any status is ok.
// @since 0.62.0
effect http.post_response(url: String, body: String) -> HttpResponse

// Whole reply to a PUT; any status is ok.
// @since 0.62.0
effect http.put_response(url: String, body: String) -> HttpResponse

// Whole reply to a PATCH; any status is ok.
// @since 0.62.0
effect http.patch_response(url: String, body: String) -> HttpResponse

// Whole reply to a DELETE; any status is ok.
// @since 0.62.0
effect http.delete_response(url: String) -> HttpResponse

// Whole reply for any method; 3xx not followed.
// @since 0.62.0
effect http.request_response(method: String, url: String, body: String, headers: Map[String, String]) -> HttpResponse

// Raw body bytes, not UTF-8 decoded.
// @since 0.27.7 or earlier
effect http.get_bytes(url: String) -> Bytes

// Raw body bytes for any method and headers.
// @since 0.27.7 or earlier
effect http.request_bytes(method: String, url: String, body: String, headers: Map[String, String]) -> Bytes

// Body chunks to on_chunk; err on a non-2xx.
// @since 0.15.1 or earlier
effect http.request_stream(method: String, url: String, body: String, headers: Map[String, String], on_chunk: (String) -> Unit) -> Unit

// Streams base_url/chat/completions; LLM-response JSON.
// @since 0.15.1 or earlier
effect http.openai_streaming_call(base_url: String, api_key: String, body_json: String, on_text_delta: (String) -> Unit) -> String

// Streams Anthropic Messages; LLM-response JSON.
// @since 0.15.1 or earlier
effect http.anthropic_streaming_call(api_key: String, body_json: String, on_text_delta: (String) -> Unit) -> String

// Begins a request; returns at once with a handle.
// @since unreleased
effect http.start(method: String, url: String, body: String, headers: Map[String, String], limits: HttpLimits) -> HttpCall

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

// request_stream bounded by per-call limits.
// @since unreleased
effect http.request_stream_with_limits(method: String, url: String, body: String, headers: Map[String, String], limits: HttpLimits, on_chunk: (String) -> Unit) -> Unit

// openai_streaming_call bounded by per-call limits.
// @since unreleased
effect http.openai_streaming_call_with_limits(base_url: String, api_key: String, body_json: String, limits: HttpLimits, on_text_delta: (String) -> Unit) -> String

// anthropic_streaming_call bounded by per-call limits.
// @since unreleased
effect http.anthropic_streaming_call_with_limits(api_key: String, body_json: String, limits: HttpLimits, on_text_delta: (String) -> Unit) -> String

// Request built in code; target keeps its ?query.
// @since unreleased
http.new_request(method: String, target: String, body: String, headers: Map[String, String]) -> HttpRequest

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

// JSON body through decode; err is a 400 response.
// @since unreleased
http.decode_json(req: HttpRequest, decode: (Value) -> Result[T, String]) -> Result[T, HttpResponse]
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
