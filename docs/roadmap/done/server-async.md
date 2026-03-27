<!-- description: Make http.serve handler an effect context for I/O calls -->
# Server Async — http.serve Effect Integration

## 概要

`http.serve` のハンドラを effect コンテキスト化し、ハンドラ内から effect fn を呼べるようにする。

## 現状の問題

```almide
// 現在: ハンドラは pure コンテキスト — effect fn を呼べない
effect fn main() -> Unit = {
  http.serve(3000, (req) => {
    // ここで fs.read_text() や http.get() を呼ぶとコンパイルエラー
    http.json(200, "hello")
  })
}
```

## 目標

```almide
// ハンドラが effect fn になる
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

## 実装方針

### Rust (thread backend)

- 各リクエストを `std::thread::spawn` で独立スレッド化
- ハンドラは通常の `fn(Request) -> Result<Response, String>` として呼ぶ
- スレッドプールで同時接続数を制限

### Rust (async backend — 将来)

- `tokio::spawn` per request
- connection pooling
- graceful shutdown (`tokio::signal`)

### TS

- Express / Hono ベースのサーバー
- ハンドラは `async function`
- 変更なし（TS は既に async）

## 前提条件

- async backend (on-hold/async-backend.md) の実装
- または thread backend でのハンドラ effect 化（先行実装可能）

## 優先度

中。Web アプリケーション開発に必要だが、CLI ツール中心の現段階では不要。
