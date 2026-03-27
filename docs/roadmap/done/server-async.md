<!-- description: Make http.serve handler an effect context for I/O calls -->
<!-- done: 2026-03-18 -->
# Server Async — http.serve Effect Integration

## Overview

Make the `http.serve` handler an effect context so that effect fn can be called from within the handler.

## Current Problem

```almide
// Current: handler is pure context — cannot call effect fn
effect fn main() -> Unit = {
  http.serve(3000, (req) => {
    // Calling fs.read_text() or http.get() here causes a compile error
    http.json(200, "hello")
  })
}
```

## Goal

```almide
// Handler becomes effect fn
effect fn handle(req: Request) -> Response = {
  let (user, prefs) = fan {
    fetch_user(http.req_path(req))
    fetch_preferences(http.req_path(req))
  }
  http.json(200, json.stringify(render(user, prefs)))
}

effect fn main() -> Unit = {
  http.serve(3000, handle)
}
```

## Implementation Approach

### Rust (thread backend)

- Spawn each request as an independent thread via `std::thread::spawn`
- Call handler as a normal `fn(Request) -> Result<Response, String>`
- Limit concurrent connections with a thread pool

### Rust (async backend — future)

- `tokio::spawn` per request
- connection pooling
- graceful shutdown (`tokio::signal`)

### TS

- Express / Hono based server
- Handler is `async function`
- No changes needed (TS is already async)

## Prerequisites

- Implementation of async backend (on-hold/async-backend.md)
- Or handler effect-ification on thread backend (can be implemented first)

## Priority

Medium. Required for web application development, but unnecessary at the current CLI-tool-focused stage.
