# HTTP Module Roadmap

## Current Status (v0.1)
Self-contained HTTP/1.1 server on std::net. Zero dependencies.

### What works
- `http.serve(port, fn(req) => response)` — シングルスレッドサーバー
- `http.response(status, body)` — テキストレスポンス
- `http.json(status, body)` — JSONレスポンス
- `req.path`, `req.method`, `req.headers`, `req.body` — リクエスト情報
- match on `req.path` でルーティング
- Benchmark: 8,831 req/s (Go 5,887, Python 5,220)

### Known Limitations
- シングルスレッド（並行リクエスト処理不可）
- HTTP/1.1 only（HTTP/2未対応）
- TLSなし（HTTPSはリバースプロキシ前提）
- ハンドラー内でeffect fn呼べない（型チェッカー制約）
- レスポンスヘッダーのカスタム設定不可
- ボディの大きいリクエストで8KB制限

## Phase 1: 基本機能の完成

### 1.1 レスポンスヘッダー
```almide
http.response_with_headers(200, body, map.set(map.new(), "X-Custom", "value"))
```

### 1.2 リクエストボディのサイズ上限撤廃
Content-Length を読んで動的にバッファ確保。

### 1.3 ハンドラー内でのeffect fn対応
ハンドラーをeffect対応にして、fs.read_textやenv.unix_timestamp等を使えるように。

### 1.4 HTTPメソッドのルーティング
```almide
http.serve(3000, fn(req) => {
  match req.method {
    "GET" => handle_get(req),
    "POST" => handle_post(req),
    _ => http.response(405, "Method Not Allowed")
  }
})
```

## Phase 2: パフォーマンス

### 2.1 マルチスレッド対応
`std::thread::spawn` でリクエストごとにスレッド生成。
シングルスレッド比で 5-10x のスループット改善見込み。

### 2.2 Connection: keep-alive
同一TCP接続で複数リクエスト処理。
ベンチマーク（ab -k）で大幅改善。

### 2.3 async I/O（将来）
epoll/kqueue ベースの非同期I/O。構造化並行性と統合。

## Phase 3: HTTP クライアント

### 3.1 基本クライアント
```almide
let body = await http.get("http://api.example.com/data")
let resp = await http.post("http://api.example.com/data", json_body)
```

### 3.2 HTTPS対応
rustls または system TLS を使用。クライアント側のみ（サーバーはプロキシ前提）。

## Phase 4: WASM対応

### 4.1 WASI HTTP
WASI preview 2 の wasi-http proposal を使用。
ホスト（Cloudflare Workers等）がHTTPを提供し、Almideはハンドラーを登録。

```almide
// WASM環境では http.serve の代わりにエクスポート
export fn handle(req: Request) -> Response = {
  http.json(200, json.stringify(data))
}
```

### 4.2 Cloudflare Workers / Fastly Compute 対応
プラットフォーム固有のエントリーポイントを生成。

## Priority
1.3 (effect fn in handler) > 2.1 (multithread) > 3.1 (client) > 1.1 (headers)
