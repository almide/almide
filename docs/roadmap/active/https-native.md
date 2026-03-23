# HTTPS Native Support [ACTIVE]

**目標**: `http.get("https://...")` が curl fallback なしで動く
**現状**: Rust ターゲットで HTTPS は curl 外部コマンドに依存。curl がない環境や、一部の HTTPS サイトで失敗する

## 問題

- `http.get("https://dummyjson.com/...")` がフォールバックに落ちる
- curl 依存は zero-dep 設計方針に反する
- WASM ターゲットでは HTTPS が動かない (WASI にソケット API がないため)

## 選択肢

| 方法 | Pros | Cons |
|------|------|------|
| rustls (pure Rust TLS) | zero C dep, WASM 互換性あり | crate 追加、バイナリサイズ増 |
| native-tls | OS の TLS を使う、安定 | C 依存、クロスプラットフォーム差 |
| curl 改善 | 変更最小 | 根本解決にならない |

## 推奨: rustls

- Pure Rust — Almide の zero-dep 方針と整合
- WASM ターゲットでも将来的に使える可能性
- ただしバイナリサイズへの影響を計測する必要あり

## 暫定対応

- HTTP (非 HTTPS) は正常動作確認済み
- HTTPS が必要な場合は TS/JS ターゲットで実行 (`fetch` ベース)
