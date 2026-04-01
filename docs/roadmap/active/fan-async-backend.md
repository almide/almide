<!-- description: Migrate fan runtime from std::thread to tokio async -->
# Fan Async Backend

## Current State

Fan concurrency is fully designed and implemented with a **thread-based backend**:

| Feature | Status | Backend |
|---|---|---|
| `fan { }` (static fan-out) | ✅ | `std::thread::scope` |
| `fan.map(xs, f)` | ✅ | `std::thread::scope` + spawn per item |
| `fan.map(xs, limit: n, f)` | ✅ | Semaphore-style thread pool |
| `fan.race(thunks)` | ✅ | `std::thread::scope` + `mpsc::channel` |
| `fan.any(thunks)` | ✅ | thread + first-success channel |
| `fan.settle(thunks)` | ✅ | thread + collect all |
| `fan.timeout(ms, thunk)` | ✅ | thread + deadline loop |
| Effect isolation | ✅ | effect fn / pure fn boundary |
| TS codegen | ✅ | `Promise.all` / `Promise.race` etc. |
| WASM codegen | ⚠️ | Sequential fallback |

`effect fn` は同期関数として生成される。`fan` は `std::thread::scope` で OS スレッドを使う。

## Why Migrate

### 1. Scalability

`fan.map(urls, (url) => http.get(url))` で 1000 URL を処理すると 1000 OS スレッドが起動する（`limit:` なしの場合）。async task なら数千の並行処理も軽量。

### 2. HTTP server

`http.serve` のハンドラが `effect fn` になるとき、リクエストごとに OS スレッドを生やすのは非効率。tokio task ならリクエスト/接続あたりのコストが桁違いに小さい。

### 3. WASM true concurrency

JSPI (Chrome 137+, Firefox 139+) を使えば WASM から JS の async API を直接呼べる。現在の sequential fallback を `Promise.all` 委譲に置き換えられる。

### 4. I/O efficiency

現在の `http.get` は `std::net::TcpStream` による同期 I/O。接続中スレッドがブロックされる。`reqwest` + async なら接続待ちの間に他の task が進む。

## Design Decisions

### 1. tokio, single-threaded executor

`tokio::task::LocalSet` を使い `Send` 制約を回避。Almide の値は `Send` でないものがある（`Rc`, `RefCell` 等を含む生成コード）。

```rust
#[tokio::main(flavor = "current_thread")]
async fn main() -> Result<(), String> { ... }
```

### 2. effect fn → async fn

`effect fn` のRustコード生成を `fn(...) -> Result<T, String>` から `async fn(...) -> Result<T, String>` に変更。

effect fn 内の effect fn 呼び出しには `.await` を自動挿入（既にIRレベルで追跡済み）。

### 3. fan codegen changes

| Feature | Current (thread) | After (async) |
|---|---|---|
| `fan { e1; e2 }` | `std::thread::scope` + `spawn` | `tokio::try_join!(e1, e2)` |
| `fan.map(xs, f)` | `std::thread::scope` + spawn per item | `futures::future::try_join_all` |
| `fan.race(thunks)` | `mpsc::channel` | `tokio::select!` |
| `fan.any(thunks)` | thread + first-success | `tokio::select!` with error collection |
| `fan.settle(thunks)` | thread + collect all | `futures::future::join_all` |
| `fan.timeout(ms, f)` | thread + deadline | `tokio::time::timeout` |

### 4. HTTP client migration

`std::net::TcpStream` → `reqwest` (async HTTP client)。

### 5. No user-visible change

Almide ソースコードに変更なし。`fan { }` も `effect fn` もそのまま。コード生成だけが変わる。

## Implementation Plan

### Phase 1: async codegen foundation

- [ ] `effect fn` → Rust `async fn` codegen
- [ ] `.await` auto-insertion for effect fn calls
- [ ] `#[tokio::main(flavor = "current_thread")]` at entry point
- [ ] Generated `Cargo.toml` に `tokio = { version = "1", features = ["rt", "time", "macros"] }` 追加
- [ ] 既存テスト全パス確認

### Phase 2: fan async codegen

- [ ] `fan { }` → `tokio::try_join!`
- [ ] `fan.map` → `futures::future::try_join_all`
- [ ] `fan.race` → `tokio::select!`
- [ ] `fan.any` / `fan.settle` / `fan.timeout` の async 版
- [ ] Generated `Cargo.toml` に `futures = "0.3"` 追加
- [ ] fan テスト全パス確認

### Phase 3: HTTP async

- [ ] `http.get` / `http.post` etc. → `reqwest` async
- [ ] `http.serve` ハンドラを effect context に（`tokio::spawn` per request）
- [ ] connection pooling
- [ ] graceful shutdown (`tokio::signal`)
- [ ] HTTP テスト確認

### Phase 4: WASM async (JSPI)

- [ ] WASM target で JSPI-based async bridge
- [ ] `fan { }` → JS 側 `Promise.all` 委譲
- [ ] Sequential fallback を JSPI 利用に置き換え
- [ ] Browser / WASI 両対応

## Dependencies

- `tokio = { version = "1", features = ["rt", "time", "macros"] }`
- `futures = "0.3"`
- `reqwest = { version = "0.12", features = ["json"] }` (Phase 3)

Generated binary size increase: ~数百 KB。WASM target は tokio 不使用（別パス）。

## Files to Modify

- `crates/almide-codegen/src/walker/declarations.rs` — `effect fn` → `async fn`
- `crates/almide-codegen/src/walker/expressions.rs` — `.await` 挿入
- `crates/almide-codegen/src/walker/mod.rs` — fan codegen (try_join!, select!, etc.)
- `codegen/templates/rust.toml` — async テンプレート
- `src/cli/build.rs` — generated Cargo.toml に tokio/futures 追加
- `runtime/rs/src/lib.rs` — `almide_block_on` 削除、tokio エントリポイント
- `runtime/rs/src/http.rs` — reqwest async migration (Phase 3)
