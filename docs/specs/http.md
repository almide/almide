# HTTP Module Specification

> Verified by manual testing. No dedicated exercise exists yet.

---

## 1. Overview

The `http` module provides a zero-dependency HTTP/1.1 server and client. It is a hardcoded stdlib module (not a bundled `.almd` file) with type signatures handled directly in codegen rather than `stdlib.rs::lookup_sig`. The runtime is defined in `src/emit_rust/http_runtime.txt` (Rust) and `src/emit_ts_runtime.rs` (TypeScript/Node.js).

The module is classified as a platform module (`PLATFORM_MODULES` in `src/check/mod.rs`), meaning it requires OS access and is not available on bare WASM without host imports.

---

## 2. HTTP Server

### 2.1 `http.serve(port, handler)`

Starts an HTTP/1.1 server listening on `0.0.0.0:{port}`. The handler receives a request record and returns a response.

```almide
import http

effect fn main(args: List[String]) -> Result[Unit, String] = {
  http.serve(3000, fn(req) => {
    match req.path {
      "/" => http.response(200, "Hello, world!"),
      "/api" => http.json(200, "{\"status\": \"ok\"}"),
      _ => http.response(404, "Not Found")
    }
  })
}
```

**Signature:** `http.serve(port: Int, handler: fn(Request) -> Response) -> Result[Unit, String]`

`http.serve` is an effect function. It must be called from within an `effect fn` (typically `main`). The server blocks the calling thread and loops indefinitely accepting connections.

### 2.2 Request Fields

The handler receives a request record with the following fields:

| Field | Type | Description |
|---|---|---|
| `req.method` | `String` | HTTP method (`"GET"`, `"POST"`, etc.) |
| `req.path` | `String` | Request path (e.g. `"/"`, `"/api/users"`) |
| `req.headers` | `Map[String, String]` | HTTP headers (key-value pairs) |
| `req.body` | `String` | Request body (empty string for bodyless requests) |

Request parsing reads the `Content-Length` header and dynamically allocates a buffer for the body. There is no fixed size limit.

### 2.3 Routing

Routing is done via `match` on request fields. There is no built-in router.

```almide
http.serve(8080, fn(req) => {
  match req.method {
    "GET" => match req.path {
      "/" => http.response(200, "Home"),
      "/health" => http.json(200, "{\"ok\": true}"),
      _ => http.response(404, "Not Found")
    },
    "POST" => match req.path {
      "/api/data" => http.json(201, req.body),
      _ => http.response(405, "Method Not Allowed")
    },
    _ => http.response(405, "Method Not Allowed")
  }
})
```

---

## 3. Response Construction

Three functions construct HTTP responses. All return a `Response` value for use inside the `http.serve` handler.

### 3.1 `http.response(status, body)`

Creates a plain text response with `Content-Type: text/plain`.

```almide
http.response(200, "Hello")
http.response(404, "Not Found")
```

**Signature:** `http.response(status: Int, body: String) -> Response`

### 3.2 `http.json(status, body)`

Creates a JSON response with `Content-Type: application/json`.

```almide
http.json(200, json.stringify(data))
http.json(201, "{\"id\": 1}")
```

**Signature:** `http.json(status: Int, body: String) -> Response`

### 3.3 `http.with_headers(status, body, headers)`

Creates a response with custom headers. The headers map completely replaces the default headers (including `Content-Type`), so callers must include all desired headers.

```almide
let headers = map.set(map.set(map.new(), "Content-Type", "text/html"), "X-Custom", "value")
http.with_headers(200, "<h1>Hello</h1>", headers)
```

**Signature:** `http.with_headers(status: Int, body: String, headers: Map[String, String]) -> Response`

### 3.4 Supported Status Codes

The runtime maps the following status codes to their standard reason phrases:

| Code | Phrase |
|---|---|
| 200 | OK |
| 201 | Created |
| 204 | No Content |
| 301 | Moved Permanently |
| 302 | Found |
| 304 | Not Modified |
| 400 | Bad Request |
| 401 | Unauthorized |
| 403 | Forbidden |
| 404 | Not Found |
| 405 | Method Not Allowed |
| 409 | Conflict |
| 422 | Unprocessable Entity |
| 429 | Too Many Requests |
| 500 | Internal Server Error |
| 502 | Bad Gateway |
| 503 | Service Unavailable |

Any other status code defaults to `"OK"` as the reason phrase.

---

## 4. HTTP Client

### 4.1 `http.get(url)`

Performs an HTTP GET request and returns the response body.

```almide
let body = http.get("http://example.com/api/data")
```

**Signature:** `http.get(url: String) -> Result[String, String]` (effect)

The `?` operator is automatically inserted by codegen, so in effect context the result is auto-unwrapped to `String`.

### 4.2 `http.post(url, body)`

Performs an HTTP POST request with `Content-Type: application/json` and returns the response body.

```almide
let response = http.post("http://example.com/api/data", json.stringify(payload))
```

**Signature:** `http.post(url: String, body: String) -> Result[String, String]` (effect)

### 4.3 URL Format

- `http://` URLs are handled natively via `std::net::TcpStream` (raw HTTP/1.1 request).
- `https://` URLs are handled via the curl fallback (see Section 5).
- URLs without a scheme produce an error: `"Use http:// or https:// URLs"`.
- Default port is 80 for HTTP. Custom ports are specified as `http://host:port/path`.

---

## 5. HTTPS Support (curl Fallback)

HTTPS requests are delegated to the system `curl` command. This avoids bundling a TLS library in the compiled binary.

**Behavior:**

- `http.get("https://...")` executes `curl -s -X GET <url>`.
- `http.post("https://...", body)` executes `curl -s -X POST -H "Content-Type: application/json" -d <body> <url>`.
- If `curl` is not found on the system, the error `"curl not found: ..."` is returned.
- If the curl process exits with a non-zero status, the error `"HTTPS request failed: <stderr>"` is returned.

This is a deliberate design choice: the Almide compiler produces zero-dependency binaries, and TLS support is provided by the system rather than a vendored library.

---

## 6. Keep-Alive and Multithreading

### 6.1 Multithreading

The server spawns a new OS thread (`std::thread::spawn`) for each incoming TCP connection. The handler closure is wrapped in `Arc` for thread-safe sharing. This means:

- Multiple requests can be processed concurrently.
- The handler function must be `Send + Sync + 'static`.
- There is no thread pool; a new thread is created per connection.

### 6.2 Keep-Alive

HTTP/1.1 keep-alive is supported. When a client sends `Connection: keep-alive`, the server loops on the same TCP connection, reading and handling multiple requests without closing the socket. When the header is absent or set to `close`, the connection is closed after a single request-response cycle.

The response includes a `Connection: keep-alive` or `Connection: close` header mirroring the client's preference. `Content-Length` is always set in the response.

---

## 7. WASM Support

The same Almide HTTP code compiles to both native and WASM targets. The runtime uses `#[cfg(target_arch = "wasm32")]` to select the appropriate implementation at compile time.

### 7.1 Server (Handler Export Pattern)

On WASM, `http.serve` does not start a listener. Instead, it stores the handler globally and the host calls an exported function per request.

```
Native:
  main() -> http.serve() -> TcpListener loop -> handler(req) -> response

WASM:
  host -> almide_http_handle(req_ptr, req_len) -> handler(req) -> response JSON -> host
```

The WASM module exports three functions:

| Export | Signature | Purpose |
|---|---|---|
| `almide_http_handle` | `(req_ptr: *const u8, req_len: usize) -> u64` | Process one HTTP request |
| `almide_alloc` | `(len: usize) -> *mut u8` | Allocate memory for host to write into |
| `almide_dealloc` | `(ptr: *mut u8, len: usize)` | Free previously allocated memory |

### 7.2 Data Exchange Format

Request and response data are exchanged as JSON strings over WASM linear memory.

**Request JSON (host -> WASM):**

```json
{
  "method": "GET",
  "path": "/api/data",
  "headers": {"Content-Type": "application/json"},
  "body": ""
}
```

**Response JSON (WASM -> host):**

```json
{
  "status": 200,
  "headers": {"Content-Type": "text/plain"},
  "body": "Hello"
}
```

The return value of `almide_http_handle` is a `u64` packing the response JSON pointer and length: `(ptr << 32) | len`. The host reads the JSON from linear memory at that location.

### 7.3 WASM HTTP Client

On WASM, `http.get` and `http.post` delegate to a host-provided import:

```rust
extern "C" {
    fn host_http_fetch(
        url_ptr: *const u8, url_len: usize,
        method_ptr: *const u8, method_len: usize,
        body_ptr: *const u8, body_len: usize,
        resp_ptr: *mut *const u8, resp_len: *mut usize
    ) -> i32;
}
```

The host must provide this import. A return code of 0 indicates success; any other value results in `Err("fetch failed")`.

### 7.4 Build Commands

```bash
almide build app.almd              # Native binary (std::net server loop)
almide build app.almd --target wasm  # WASM (handler export, no server loop)
```

---

## 8. TypeScript Target

On the TypeScript target, the HTTP module uses platform-appropriate implementations:

- **TypeScript (Deno/browser):** Uses the `fetch` API for client, `Deno.serve` for server.
- **JavaScript (Node.js):** Uses `require("http")` for server, `require("http")`/`require("https")` for client.

Response construction uses plain objects with `status`, `body`, and `headers` fields.

---

## 9. Error Handling

- If the handler throws an error (returns `Err`), the server responds with `500 Internal Error: <message>`.
- If request parsing fails (malformed HTTP), the server responds with `400 Bad Request`.
- On WASM, if no handler is registered when `almide_http_handle` is called, the response is `500 No handler registered`.
- Client errors (`http.get`, `http.post`) propagate as `Result[String, String]` and are auto-unwrapped via `?` in effect context.

---

## 10. Complete Example

```almide
import http
import json
import map

effect fn main(args: List[String]) -> Result[Unit, String] = {
  http.serve(3000, fn(req) => {
    match req.method {
      "GET" => match req.path {
        "/" => http.response(200, "Welcome to Almide"),
        "/api/status" => http.json(200, json.stringify(
          json.from_map(map.set(map.new(), "status", json.from_string("ok")))
        )),
        _ => http.response(404, "Not Found")
      },
      "POST" => match req.path {
        "/api/echo" => http.json(200, req.body),
        _ => http.response(405, "Method Not Allowed")
      },
      _ => http.response(405, "Method Not Allowed")
    }
  })
}
```

---

## 11. Implementation Files

| File | Purpose |
|---|---|
| `src/emit_rust/http_runtime.txt` | Full Rust HTTP runtime (server, client, WASM exports) |
| `src/emit_rust/calls.rs` | `gen_http_call` — Rust codegen for http.* calls |
| `src/emit_ts_runtime.rs` | TypeScript/Node.js HTTP runtime |
| `src/stdlib.rs` | Module registration (`STDLIB_MODULES`) |
| `src/check/mod.rs` | Platform module classification |

---

## 12. Known Limitations

- HTTP/1.1 only (no HTTP/2).
- No built-in TLS for server (use a reverse proxy for HTTPS termination).
- HTTPS client uses system `curl` (must be installed).
- One thread per connection (no thread pool or async I/O).
- `http.post` always sends `Content-Type: application/json`; other content types require manual construction.
- No streaming response support; the full body must fit in memory.
- No timeout configuration for client requests.
- No cookie or session management.
