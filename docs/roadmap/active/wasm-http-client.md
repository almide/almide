# WASM HTTP Client

**優先度:** 中 — V8 Isolate 環境での実用性に直結するが、WASI の制約で短期解決が難しい
**前提:** WASM fs I/O 実装済み（read_text, write, exists）

---

## 現状

- HTTP response 構築系（response, json, set_header 等）は WASM で動作済み
- HTTP client 系（get, post, put 等）は stub → デフォルト値を返す
- WASI preview1 にはネットワーキング API が存在しない

## なぜ難しいか

1. **WASI preview1 にソケット/HTTP がない** — 仕様外
2. **wasi-http (preview2)** は Component Model 前提 — Almide は Core WASM (preview1) ベース
3. **ホスト固有 import** は移植性を損なう — Cloudflare Workers 固有にすると汎用 WASI バイナリの価値がなくなる

## 選択肢

### A. WASI preview2 + Component Model 対応（大規模）
- wasm-encoder の Component Model 対応が必要
- wasi-http インターフェースの実装
- wasmtime / Cloudflare Workers 両対応
- **工数: 大（数週間）**

### B. ホスト提供 import 方式（中規模）
- `__almide_http_get(url_ptr, url_len) -> (status, body_ptr, body_len)` のようなカスタム import
- ホストランタイム（wasmtime wrapper / CF Worker）が実装を提供
- 移植性は低いが即座に動く
- **工数: 中（数日）**

### C. 現状維持 + エラー Result 返却（最小）
- http client を WASM で呼ぶと `err("http client not supported on WASM target")` を返す
- コンパイル時 warning も検討
- **工数: 小（数時間）**

## 推奨

短期は **C**（エラー Result）、中期で **B**（ホスト import）を検討。A は WASI preview2 の安定化を待つ。
