# ストリーミング — WebSocket, SSE, Stream

## 概要

リアルタイム通信とストリーミングデータ処理のサポート。

## スコープ

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

### Stream と for...in の統合

既存の `for...in` が Stream を認識し、到着順にイテレーション。

```almide
effect fn process_stream(url: String) -> Unit = {
  for item in http.stream(url) {
    println(item)
  }
}
```

## 実装方針

### Rust

- WebSocket: `tungstenite` (sync) or `tokio-tungstenite` (async backend)
- SSE: HTTP chunked response のパース
- Stream trait: `Iterator` ベース (sync) or `futures::Stream` (async)

### TS

- WebSocket: `WebSocket` API (browser) / `ws` package (Node)
- SSE: `EventSource` API / `ReadableStream`
- Stream: `AsyncIterator`

## stdlib モジュール

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

## 前提条件

- async backend (on-hold/async-backend.md) — sync WebSocket は可能だが実用的でない
- Stream 型の型システムへの導入
- `for...in` の Stream 対応

## 優先度

低。リアルタイムアプリケーション開発が必要になってから。
