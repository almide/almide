# HTTP Module [DONE]

## Implemented

### Server
- `http.serve(port, handler)` — HTTP/1.1 server with multithreading (thread per connection)
- `http.response(status, body)` — plain text response
- `http.json(status, body)` — JSON response
- `http.with_headers(status, body, headers)` — custom headers
- Keep-alive support
- Dynamic request body parsing (no size limit)
- WASM handler export pattern

### Client
- `http.get(url)` — HTTP GET
- `http.post(url, body)` — HTTP POST
- HTTPS via curl fallback (native Rust), native in TS/JS targets

### Targets
| Feature | Rust | Rust WASM | TS (Deno) | JS (Node) |
|---------|------|-----------|-----------|-----------|
| serve | thread per conn | handler export | Deno.serve | http.createServer |
| get/post | TcpStream | host import | fetch | http/https module |
| HTTPS | curl fallback | host provided | native | native |
| with_headers | full | full | full | full |

### Design Decisions
- No external crate deps (bare rustc compatibility)
- HTTPS via curl on native Rust (zero-dep design; native TLS deferred)
- WASM uses host imports for HTTP client
