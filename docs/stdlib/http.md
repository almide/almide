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

## Signature index (28 functions)

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
effect http.get_status(url: String) -> ()
effect http.request_status(method: String, url: String, body: String, headers: Map[String, String]) -> ()
effect http.get_bytes(url: String) -> Bytes
effect http.request_bytes(method: String, url: String, body: String, headers: Map[String, String]) -> Bytes
effect http.request_stream(method: String, url: String, body: String, headers: Map[String, String], on_chunk: (String) -> Unit) -> Unit
effect http.openai_streaming_call(base_url: String, api_key: String, body_json: String, on_text_delta: (String) -> Unit) -> String
effect http.anthropic_streaming_call(api_key: String, body_json: String, on_text_delta: (String) -> Unit) -> String
```

<!-- END GENERATED SIGNATURE INDEX -->
