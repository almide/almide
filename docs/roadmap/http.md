# HTTP Module [DONE]

## Current Status (v0.1)
Self-contained HTTP/1.1 server on std::net. Zero dependencies.

### What works
- `http.serve(port, fn(req) => response)` — single-threaded server
- `http.response(status, body)` — text response
- `http.json(status, body)` — JSON response
- `req.path`, `req.method`, `req.headers`, `req.body` — request fields
- routing via match on `req.path`
- Benchmark: 8,831 req/s (Go 5,887, Python 5,220)

### Known Limitations
- Single-threaded (cannot handle concurrent requests)
- HTTP/1.1 only (no HTTP/2)
- No TLS (HTTPS assumed to be handled by a reverse proxy)
- Cannot call effect fn inside handler (type checker constraint)
- Cannot set custom response headers
- 8KB limit for large request bodies

## Phase 1: Core Feature Completion

### 1.1 Response Headers
```almide
http.response_with_headers(200, body, map.set(map.new(), "X-Custom", "value"))
```

### 1.2 Remove Request Body Size Limit
Read Content-Length and dynamically allocate buffer.

### 1.3 Effect fn Support in Handlers
Make handlers effect-compatible so fs.read_text, env.unix_timestamp etc. can be called.

### 1.4 HTTP Method Routing
```almide
http.serve(3000, fn(req) => {
  match req.method {
    "GET" => handle_get(req),
    "POST" => handle_post(req),
    _ => http.response(405, "Method Not Allowed")
  }
})
```

## Phase 2: Performance

### 2.1 Multithreading
Spawn a thread per request with `std::thread::spawn`.
Expected 5–10x throughput improvement over single-threaded.

### 2.2 Connection: keep-alive
Handle multiple requests over the same TCP connection.
Large improvement expected in benchmarks (ab -k).

### 2.3 Async I/O (future)
epoll/kqueue-based async I/O. Integrated with structured concurrency.

## Phase 3: HTTP Client

### 3.1 Basic Client
```almide
let body = await http.get("http://api.example.com/data")
let resp = await http.post("http://api.example.com/data", json_body)
```

### 3.2 HTTPS Support
Use rustls or system TLS. Client-side only (server uses reverse proxy).

## Phase 4: WASM / Edge Runtime Support

### Goal
The same Almide code runs on both native and WASM.

```almide
import http
import json

effect fn main(args: List[String]) -> Result[Unit, String] = {
  http.serve(3000, fn(req) => {
    match req.path {
      "/" => http.json(200, json.stringify(json.from_map(map.set(map.new(), "status", json.from_string("ok"))))),
      _ => http.response(404, "Not Found")
    }
  })
}
```

- `almide build app.almd` → native binary (std::net server loop)
- `almide build app.almd --target wasm` → WASM (handler export, no server loop)

### Architecture

```
Native:
  main() → http.serve() → TcpListener loop → handler(req) → response

WASM (Cloudflare Workers etc.):
  host → HTTP request → WASM export handle(req_ptr) → response_ptr → host
```

### Implementation Steps

#### 4.1 Isolate WASM HTTP Runtime
- Wrap `almide_http_serve` in `http_runtime.txt` with `#[cfg(not(target_arch = "wasm32"))]`
- Provide a separate `#[cfg(target_arch = "wasm32")]` implementation
- WASM `http.serve` stores the handler globally and exports `#[no_mangle] pub extern "C" fn handle()`

#### 4.2 WASM ↔ Host Data Exchange
- Exchange Request/Response as JSON on linear memory (simplest approach)
- Host writes JSON string to memory → Almide parses and calls handler → Almide writes response JSON to memory → host reads it

```rust
// WASM export (generated code)
#[no_mangle]
pub extern "C" fn handle(req_ptr: *const u8, req_len: usize) -> u64 {
    let req_json = unsafe { std::str::from_utf8_unchecked(std::slice::from_raw_parts(req_ptr, req_len)) };
    let req = parse_wasm_request(req_json);
    let resp = HANDLER.with(|h| h(req));
    let resp_json = serialize_wasm_response(&resp);
    // Return ptr and len packed in u64
    let ptr = resp_json.as_ptr() as u64;
    let len = resp_json.len() as u64;
    std::mem::forget(resp_json);
    (ptr << 32) | len
}
```

#### 4.3 Cloudflare Workers Adapter
- `almide build --target cloudflare` generates Workers glue JS + WASM
- glue JS receives `fetch` events and calls WASM `handle()`

```javascript
// generated glue code
export default {
  async fetch(request) {
    const url = new URL(request.url);
    const body = await request.text();
    const reqJson = JSON.stringify({
      method: request.method,
      path: url.pathname,
      headers: Object.fromEntries(request.headers),
      body: body
    });
    const respJson = wasmHandle(reqJson);
    const resp = JSON.parse(respJson);
    return new Response(resp.body, {
      status: resp.status,
      headers: resp.headers
    });
  }
};
```

#### 4.4 fetch API (WASM HTTP Client)
- Issuing HTTP requests from inside WASM requires calling the host's `fetch`
- Host exposes `host_fetch(url_ptr, url_len) -> (resp_ptr, resp_len)` as an import
- `http.get("https://...")` in WASM uses this host import

```rust
#[cfg(target_arch = "wasm32")]
extern "C" {
    fn host_fetch(url_ptr: *const u8, url_len: usize, resp_ptr: *mut *const u8, resp_len: *mut usize) -> i32;
}

#[cfg(target_arch = "wasm32")]
fn almide_http_get(url: &str) -> Result<String, String> {
    let mut resp_ptr: *const u8 = std::ptr::null();
    let mut resp_len: usize = 0;
    let rc = unsafe { host_fetch(url.as_ptr(), url.len(), &mut resp_ptr, &mut resp_len) };
    if rc != 0 { return Err("fetch failed".into()); }
    let bytes = unsafe { std::slice::from_raw_parts(resp_ptr, resp_len) };
    Ok(String::from_utf8_lossy(bytes).to_string())
}
```

#### 4.5 Testing
- Local testing with wasmtime (mock host_fetch)
- Integration testing with Cloudflare Workers `wrangler dev`
- Confirm the same Almide code runs on both native and WASM

### Deployment Flow (end state)

```bash
almide init
almide add ...
almide build                           # native binary
almide build --target wasm             # WASI WASM
almide build --target cloudflare       # Workers (WASM + glue JS)
almide deploy --cloudflare             # future: direct deploy
```

### Priority
4.1 (runtime isolation) → 4.2 (data exchange) → 4.3 (Cloudflare) → 4.4 (fetch) → 4.5 (testing)

### Risks
- WASI HTTP proposal is still unstable (wasi-http 0.2)
- Cloudflare Workers WASM API is proprietary
- Performance of JSON-over-linear-memory data exchange
- Each host platform needs its own adapter (Cloudflare, Fastly, Deno Deploy, Vercel...)

### Decision
Build the WASM HTTP foundation with 4.1–4.2 first, then target Cloudflare Workers specifically with 4.3.
Add other platforms based on demand.
