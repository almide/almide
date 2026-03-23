# HTTPS Native Support [ACTIVE]

**目標**: `http.get("https://...")` が全ターゲットで動く

## 現状

| ターゲット | HTTP | HTTPS | 実装方法 | 状態 |
|---|---|---|---|---|
| Rust (almide run) | OK | **OK** | rustls (pure Rust TLS) | ✅ CLI経由で動作確認済 |
| Rust (almide build) | OK | **未検証** | 生成バイナリに rustls が含まれるか要確認 | 要検証 |
| TS (Deno) | OK | OK | `fetch` ネイティブ | ✅ |
| WASM | NG | NG | WASI にソケット API なし | 将来対応 (wasi:http) |

## Done

- [x] **Phase 1: rustls 統合** — `runtime/rs/src/http.rs` に rustls + webpki-roots を統合。`parse_url` が scheme を認識し、https なら `ClientConnection` + `StreamOwned` で TLS 接続。
- [x] **CLI 動作確認** — `almide run` 経由で `https://` URL へのリクエストが成功

## Remaining

### `almide build` での HTTPS 対応

`almide build` で生成されるスタンドアロンバイナリに rustls が含まれるか未検証。`cli/build.rs` の cargo build 設定で rustls crate が依存に入る必要がある。

### WASM ターゲット (将来)

WASM ではソケット自体がないので、ホストインポートで解決:
```
// WASI HTTP が安定したら
import wasi:http/outgoing-handler@0.2.0
```

WASI の HTTP 仕様が安定するまで待つ。
