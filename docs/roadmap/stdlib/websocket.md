# stdlib: websocket [Tier 3]

WebSocket クライアント/サーバー。リアルタイム通信の基盤。

## 他言語比較

| 機能 | Go (`gorilla/websocket`) | Python (`websockets`) | Rust (`tungstenite`) | Deno (built-in) |
|------|------------------------|-----------------------|----------------------|-----------------|
| クライアント接続 | `websocket.Dial(url)` | `websockets.connect(url)` | `connect(url)` | `new WebSocket(url)` |
| メッセージ送信 | `conn.WriteMessage(TextMessage, msg)` | `await ws.send(msg)` | `ws.send(Message::Text(msg))` | `ws.send(msg)` |
| メッセージ受信 | `conn.ReadMessage()` | `await ws.recv()` | `ws.read()` | `ws.onmessage` |
| 切断 | `conn.Close()` | `await ws.close()` | `ws.close()` | `ws.close()` |
| サーバー | `websocket.Upgrader{}` | `websockets.serve(handler, host, port)` | `accept(stream)` | `Deno.upgradeWebSocket(req)` |
| ping/pong | `conn.SetPingHandler` | automatic | automatic | automatic |

## 追加候補 (~8 関数)

### P0
- `websocket.connect(url) -> Result[WsConnection, String]`
- `websocket.send(conn, message) -> Result[Unit, String]`
- `websocket.receive(conn) -> Result[String, String]`
- `websocket.close(conn) -> Result[Unit, String]`

### P1
- `websocket.on_message(conn, handler)` — コールバック（async 前提）
- `websocket.upgrade(request) -> Result[WsConnection, String]` — HTTP → WS アップグレード

### P2
- `websocket.send_binary(conn, data) -> Result[Unit, String]`
- `websocket.is_open?(conn) -> Bool`

## 実装戦略

@extern。Rust: `tungstenite` (同期) / `tokio-tungstenite` (async)。TS: `WebSocket` API。
async/await (Phase D structured-concurrency) が前提。同期版を先に出して後で async 化も可能。

## 前提条件

- Structured Concurrency (async/await) が望ましいが、同期ブロッキング版は先行実装可能
