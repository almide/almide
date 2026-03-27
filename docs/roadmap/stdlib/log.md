<!-- description: Structured logging with levels and key-value context -->
# stdlib: log [Tier 3]

構造化ログ。アプリケーション開発の基盤。

## 他言語比較

| 機能 | Go (`log/slog`) | Python (`logging`) | Rust (`log` + `tracing`) | Deno (`console`) |
|------|-----------------|-------------------|-----------------------------|------------------|
| レベル | Debug, Info, Warn, Error | DEBUG, INFO, WARNING, ERROR, CRITICAL | trace, debug, info, warn, error | debug, info, warn, error |
| 構造化 | `slog.Info("msg", "key", val)` | `logger.info("msg", extra={})` | `info!(key=val, "msg")` | `console.log({})` |
| フォーマット | Handler (JSON/Text) | Formatter | subscriber (json/pretty) | 固定 |
| 出力先 | `slog.NewJSONHandler(os.Stdout)` | `FileHandler`, `StreamHandler` | subscriber 設定 | stdout |
| コンテキスト | `slog.With("key", val)` | `LoggerAdapter(extra={})` | `span!(Level::INFO, "name")` | ❌ |
| フィルタ | Handler level | `logger.setLevel()` | `RUST_LOG=info` | ❌ |

## 設計方針

Almide は LLM が書くコードなので、ログ API は極限までシンプルにする。

```almide
log.info("user logged in", ["user_id": user.id, "ip": req.ip])
log.error("failed to read", ["path": path, "error": err])
log.debug("cache hit", ["key": key])
```

- レベル: `debug`, `info`, `warn`, `error` の 4 つ
- 構造化: 第 2 引数に `Map[String, String]` でキーバリュー
- フォーマット: デフォルトは human-readable、`--log-json` で JSON
- 出力先: Phase 1 は stderr のみ

## 追加候補 (~8 関数)

### P0
- `log.debug(msg, fields?)` — デバッグログ
- `log.info(msg, fields?)` — 情報ログ
- `log.warn(msg, fields?)` — 警告ログ
- `log.error(msg, fields?)` — エラーログ

### P1
- `log.set_level(level)` — ログレベル設定
- `log.with_fields(fields) -> Logger` — フィールド付きロガー（コンテキスト）

### P2
- `log.set_format(format)` — text / json 切替
- `log.set_output(path)` — ファイル出力

## 実装戦略

TOML + runtime。Rust: `eprintln!` ベース（外部 crate 不要で最小実装可能）。TS: `console.error` ベース。
構造化 JSON 出力は `json.stringify` を内部で使用。
