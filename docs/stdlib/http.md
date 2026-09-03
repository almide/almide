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

Every client call waits at most **30 seconds** for the server to answer
(the SSE streaming client: 120 s between events). The
`ALMIDE_HTTP_TIMEOUT_SECS` environment variable overrides the limit for
all clients; `0` means **no timeout** — block until the server responds.
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

<!-- BEGIN GENERATED SIGNATURE INDEX (make stdlib-docs) — do not edit by hand -->

## Signature index (37 functions)

```
effect http.serve(port: Int, f: (HttpRequest) -> Result[HttpResponse, String]) -> Unit
http.response(status: Int, body: String) -> HttpResponse
http.json(status: Int, body: String) -> HttpResponse
http.with_headers(status: Int, body: String, headers: Map[String, String]) -> HttpResponse
http.redirect(url: String) -> HttpResponse
http.status(resp: HttpResponse, code: Int) -> HttpResponse
http.body(resp: HttpResponse) -> String
http.set_header(resp: HttpResponse, key: String, value: String) -> HttpResponse
http.get_header(resp: HttpResponse, key: String) -> Option[String]
http.status_code(resp: HttpResponse) -> Int
http.headers(resp: HttpResponse) -> Map[String, String]
http.header_values(resp: HttpResponse, key: String) -> List[String]
http.req_method(req: HttpRequest) -> String
http.req_path(req: HttpRequest) -> String
http.req_body(req: HttpRequest) -> String
http.req_header(req: HttpRequest, key: String) -> Option[String]
http.query_params(req: HttpRequest) -> Map[String, String]
http.url_decode(s: String) -> String
effect http.get(url: String) -> String
effect http.post(url: String, body: String) -> String
effect http.put(url: String, body: String) -> String
effect http.patch(url: String, body: String) -> String
effect http.delete(url: String) -> String
effect http.request(method: String, url: String, body: String, headers: Map[String, String]) -> String
effect http.get_status(url: String) -> (Int, String)
effect http.request_status(method: String, url: String, body: String, headers: Map[String, String]) -> (Int, String)
effect http.get_response(url: String) -> HttpResponse
effect http.post_response(url: String, body: String) -> HttpResponse
effect http.put_response(url: String, body: String) -> HttpResponse
effect http.patch_response(url: String, body: String) -> HttpResponse
effect http.delete_response(url: String) -> HttpResponse
effect http.request_response(method: String, url: String, body: String, headers: Map[String, String]) -> HttpResponse
effect http.get_bytes(url: String) -> Bytes
effect http.request_bytes(method: String, url: String, body: String, headers: Map[String, String]) -> Bytes
effect http.request_stream(method: String, url: String, body: String, headers: Map[String, String], on_chunk: (String) -> Unit) -> Unit
effect http.openai_streaming_call(base_url: String, api_key: String, body_json: String, on_text_delta: (String) -> Unit) -> String
effect http.anthropic_streaming_call(api_key: String, body_json: String, on_text_delta: (String) -> Unit) -> String
```

<!-- END GENERATED SIGNATURE INDEX -->
