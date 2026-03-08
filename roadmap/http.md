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

## Phase 4: WASM / Edge Runtime 対応

### 目標
同じAlmideコードがネイティブでもWASMでも動く。

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

- `almide build app.almd` → ネイティブバイナリ（std::net サーバーループ）
- `almide build app.almd --target wasm` → WASM（ハンドラーexport、サーバーループなし）

### アーキテクチャ

```
ネイティブ:
  main() → http.serve() → TcpListener loop → handler(req) → response

WASM (Cloudflare Workers等):
  ホスト → HTTP request → WASM export handle(req_ptr) → response_ptr → ホスト
```

### 実装ステップ

#### 4.1 WASM向けHTTPランタイム分離
- `http_runtime.txt` の `almide_http_serve` は `#[cfg(not(target_arch = "wasm32"))]` で囲む
- WASM用に `#[cfg(target_arch = "wasm32")]` の別実装を用意
- WASM版 `http.serve` はハンドラーをグローバルに保存して `#[no_mangle] pub extern "C" fn handle()` をexport

#### 4.2 WASM - ホスト間のデータ受け渡し
- Request/Response を線形メモリ上でJSON としてやり取り（最もシンプル）
- ホスト側がJSON文字列をメモリに書き込み → Almide側がパースしてハンドラー呼び出し → レスポンスJSON をメモリに書き込み → ホスト側が読み取り

```rust
// WASM export (生成コード)
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

#### 4.3 Cloudflare Workers アダプター
- `almide build --target cloudflare` で Workers 用の glue JS + WASM を生成
- glue JS が `fetch` イベントを受けて WASM の `handle()` を呼ぶ

```javascript
// 生成される glue code
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

#### 4.4 fetch API (WASM用HTTPクライアント)
- WASM内からHTTPリクエストを発行するにはホストの `fetch` を呼ぶ必要がある
- ホスト側に `host_fetch(url_ptr, url_len) -> (resp_ptr, resp_len)` をimport
- `http.get("https://...")` は WASM ではこの host import を使う

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

#### 4.5 テスト
- wasmtime でローカルテスト（host_fetch をモック）
- Cloudflare Workers の `wrangler dev` で統合テスト
- ネイティブとWASMで同じAlmideコードが動くことを確認

### デプロイフロー（最終形）

```bash
almide init
almide add ...
almide build                           # ネイティブバイナリ
almide build --target wasm             # WASI WASM
almide build --target cloudflare       # Workers用 (WASM + glue JS)
almide deploy --cloudflare             # 将来: 直接デプロイ
```

### 優先順位
4.1 (ランタイム分離) → 4.2 (データ受け渡し) → 4.3 (Cloudflare) → 4.4 (fetch) → 4.5 (テスト)

### リスク
- WASI HTTP proposal がまだ不安定（wasi-http 0.2）
- Cloudflare Workers の WASM API が独自仕様
- メモリ管理（線形メモリ上のJSON受け渡し）のパフォーマンス
- ホストごとにアダプター必要（Cloudflare, Fastly, Deno Deploy, Vercel...）

### 判断
まず 4.1-4.2 でWASM HTTP の基盤を作り、4.3 で Cloudflare Workers 一点突破。
他プラットフォームは需要に応じて追加。
