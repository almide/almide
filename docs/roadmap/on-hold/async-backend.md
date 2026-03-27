<!-- description: Optional tokio-based async backend for high-concurrency workloads -->
# Async Backend — tokio opt-in

## 概要

現在の sync/thread backend に加え、async backend を追加する。tokio は言語仕様に混ぜず、backend の1実装として提供。

## 動機

- 高並行 HTTP サーバー（10K+ 接続）ではスレッドモデルが限界
- async I/O で CPU 効率が大幅に向上
- WebSocket / SSE などストリーミング処理に必要

## 設計方針

- **言語仕様は変更しない** — `effect fn`, `fan` の意味論はそのまま
- **runtime trait** 経由で spawn/join/sleep を抽象化
- **entrypoint だけ backend 依存** — `#[tokio::main]` は生成コードのみ
- **feature flag** で切り替え: `almide build --runtime tokio`

## 変更点

### Rust codegen

- `effect fn` → `async fn`
- effect fn 呼び出しに `.await` 自動挿入
- `fan { }` → `tokio::try_join!`
- `fan.map` → `futures::future::try_join_all`
- `fan.race` → `tokio::select!`
- `main` → `#[tokio::main] async fn main()`

### 生成 Cargo.toml

```toml
[dependencies]
tokio = { version = "1", features = ["rt", "time", "macros"] }
futures = "0.3"
```

### WASM ターゲット

tokio を使わない。JSPI or 逐次 fallback。

## 前提条件

- fan の言語機能が安定（Phase 0-5 完了済み）
- HTTP サーバーの実用ケースが出てから

## 優先度

低。sync/thread backend で当面のユースケースはカバーできる。
