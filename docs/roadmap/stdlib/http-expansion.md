# stdlib: http (拡充) [Tier 1]

現在 8 関数（GET/POST/PUT/DELETE + サーバー基本）。ヘッダ操作・ステータス・レスポンスビルダーが足りない。

## 現状 (v0.5.13)

client: get, post, put, delete
server: serve, json_response, text_response, html_response

## 他言語比較

### クライアント

| 操作 | Go (`net/http`) | Python (`requests`) | Rust (`reqwest`) | Deno (`fetch`) |
|------|----------------|--------------------|--------------------|----------------|
| GET | `http.Get(url)` | `requests.get(url)` | `reqwest::get(url)` | `fetch(url)` |
| POST (JSON) | `http.Post(url, "application/json", body)` | `requests.post(url, json=data)` | `client.post(url).json(&data)` | `fetch(url, {method: "POST", body})` |
| カスタムヘッダ | `req.Header.Set("k", "v")` | `requests.get(url, headers={})` | `.header("k", "v")` | `fetch(url, {headers: {}})` |
| タイムアウト | `client.Timeout = 5s` | `timeout=5` | `.timeout(Duration::from_secs(5))` | `AbortController` |
| レスポンスヘッダ | `resp.Header.Get("k")` | `resp.headers["k"]` | `resp.headers().get("k")` | `resp.headers.get("k")` |
| ステータスコード | `resp.StatusCode` | `resp.status_code` | `resp.status()` | `resp.status` |
| レスポンスボディ | `io.ReadAll(resp.Body)` | `resp.text` / `resp.json()` | `resp.text()` / `resp.json()` | `resp.text()` / `resp.json()` |

### サーバー

| 操作 | Go | Python (Flask) | Rust (axum) | Deno (Hono) |
|------|-----|----------------|-------------|-------------|
| ルーティング | `http.HandleFunc("/", handler)` | `@app.route("/")` | `Router::new().route("/", get(handler))` | `app.get("/", handler)` |
| パスパラメータ | `r.PathValue("id")` | `<id>` | `Path(id)` | `c.req.param("id")` |
| クエリパラメータ | `r.URL.Query().Get("k")` | `request.args.get("k")` | `Query(params)` | `c.req.query("k")` |
| JSON レスポンス | `json.NewEncoder(w).Encode(data)` | `jsonify(data)` | `Json(data)` | `c.json(data)` |
| ステータスコード | `w.WriteHeader(code)` | `return data, code` | `StatusCode::OK` | `c.status(code)` |
| ミドルウェア | `func(next http.Handler)` | `@app.before_request` | `.layer(middleware)` | `app.use(middleware)` |

## 追加候補 (~20 関数)

### P0 (クライアント拡充)
- `http.request(method, url, options) -> Result[Response, String]` — 汎用リクエスト
- `http.get_json(url) -> Result[Json, String]` — GET + JSON パース
- `http.post_json(url, body) -> Result[Response, String]` — POST with JSON body
- `http.status(response) -> Int` — ステータスコード取得
- `http.headers(response) -> Map[String, String]` — レスポンスヘッダ
- `http.body(response) -> String` — レスポンスボディ

### P1 (リクエストビルダー)
- `http.with_header(request, key, value) -> Request`
- `http.with_timeout(request, ms) -> Request`
- `http.with_bearer_token(request, token) -> Request`

### P1 (サーバー拡充)
- `http.status_response(code, body) -> Response`
- `http.redirect(url) -> Response`
- `http.set_header(response, key, value) -> Response`
- `http.cookie(response, name, value) -> Response`

### P2 (Web Framework 統合)
web-framework.md と連携。routing, middleware, request parsing は web モジュールへ。

## 実装戦略

TOML + runtime。Rust: `ureq` (同期) or `reqwest` (async)。TS: `fetch` API。
サーバー側は web-framework.md の設計に従う。
