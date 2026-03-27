<!-- description: WebSocket, SSE, and streaming data support -->
<!-- done: 2026-03-19 -->
# Streaming — WebSocket, SSE, Stream

## Overview

Support for real-time communication and streaming data processing.

## Scope

### WebSocket

```almide
effect fn chat() -> Unit = {
  let ws = websocket.connect("wss://example.com/chat")
  websocket.send(ws, "hello")
  let msg = websocket.receive(ws)
  println(msg)
  websocket.close(ws)
}
```

### SSE (Server-Sent Events)

```almide
effect fn stream_updates() -> Unit = {
  for event in http.stream("https://api.example.com/events") {
    println(event)
  }
}
```

### Stream and for...in integration

Existing `for...in` recognizes Stream and iterates in arrival order.

```almide
effect fn process_stream(url: String) -> Unit = {
  for item in http.stream(url) {
    println(item)
  }
}
```

## Implementation Approach

### Rust

- WebSocket: `tungstenite` (sync) or `tokio-tungstenite` (async backend)
- SSE: Parsing HTTP chunked responses
- Stream trait: `Iterator`-based (sync) or `futures::Stream` (async)

### TS

- WebSocket: `WebSocket` API (browser) / `ws` package (Node)
- SSE: `EventSource` API / `ReadableStream`
- Stream: `AsyncIterator`

## stdlib Module

```toml
# stdlib/defs/websocket.toml
[connect]
params = [{ name = "url", type = "String" }]
return = "Result[WebSocket, String]"
effect = true

[send]
params = [{ name = "ws", type = "WebSocket" }, { name = "msg", type = "String" }]
return = "Result[Unit, String]"
effect = true

[receive]
params = [{ name = "ws", type = "WebSocket" }]
return = "Result[String, String]"
effect = true

[close]
params = [{ name = "ws", type = "WebSocket" }]
return = "Result[Unit, String]"
effect = true
```

## Prerequisites

- async backend (on-hold/async-backend.md) — sync WebSocket is possible but impractical
- Introduction of Stream type to the type system
- Stream support for `for...in`

## Priority

Low. When real-time application development becomes necessary.
