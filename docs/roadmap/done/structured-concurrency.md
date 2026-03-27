<!-- description: Async model with async fn, await, and async let constructs -->
# Structured Concurrency

## Philosophy

> **Almide keeps async boring on purpose: explicit fork, explicit join, automatic cancellation, and the same fail-fast semantics as `do`.**

Non-goals: novel concurrency syntax, implicit parallelism, actor primitives in the language. Almide's async is intentionally conservative — readable, hard to break, easy to implement.

## Overview

Layer 2 of Almide's async model. Three language constructs only:

| Construct | Purpose |
|-----------|---------|
| `async fn` | Declares an async function (implicitly `effect`) |
| `await expr` | Resolves `Future[T]` to `T` — one operation, always explicit |
| `async let x = expr` | Starts a concurrent task, binds a single-use handle |

Everything else (`race`, `timeout`, `sleep`) is a stdlib function, not syntax.

## Design Principles

- **No fire-and-forget** — all tasks complete within their scope
- **Cancellation propagation** — parent cancelled → children stop too
- **No task leaks** — structurally impossible in AI-generated code
- **Composes with `do` blocks** — error propagation works inside concurrent scopes
- **Minimal syntax delta** — sequential → parallel is adding one word (`async` before `let`)
- **Boring on purpose** — no novel concurrency constructs; consistency over cleverness

## Core Syntax

### `async let` for parallel execution

`async let` forks a task at the declaration site. `await` joins at the use site.

```almide
async fn load_dashboard(user_id: String) -> Dashboard =
  do {
    async let profile = fetch_profile(user_id)
    async let posts = fetch_posts(user_id)
    async let stats = fetch_stats(user_id)
    Dashboard { await profile, await posts, await stats }
  }
```

Sequential → parallel is a one-word change:

```almide
// Sequential — await each result before starting the next
let a = await fetch_a()
let b = await fetch_b()

// Parallel — start all, then await results
async let a = fetch_a()
async let b = fetch_b()
use(await a, await b)
```

Note: `async fn` returns `Future[T]`. Calling it without `await` or `async let` creates an unevaluated future. `await` resolves it (sequential). `async let` starts it immediately and binds a handle (parallel).

## Semantics

### `await`: one operation

`await` is a single operation: **`Future[T]` → `T`**.

- `await fetch_user(id)` — `fetch_user` returns `Future[Result[User, E]]`, `await` unwraps the future
- `async let x = expr` — `x` is a `Future[T]` (task handle), `await x` joins it
- Inside `do` block: `await` unwraps the future, `do` propagates the `Result`

This unification means `await` always does one thing: resolve a future. Error handling is always `do`'s job.

### `async let`: task lifecycle

- `async let x = expr` — immediately starts evaluating `expr` as a concurrent task. `x` is a `Future[T]` handle.
- `await x` — suspends until the task completes and returns its value. **Consumes** the handle.
- `await x` a second time is a **compile error** (handle is already consumed). To reuse the value, bind it: `let v = await x; use(v, v)`.
- Scope exit with un-awaited bindings → automatic cancellation.
- Inside `do` block: any task failure → **cancel all sibling tasks** → propagate error. Partial success is not observable.
- No new keywords — `async` and `let` are both existing.

### Failure and cancellation

```almide
do {
  async let a = fetch_a()   // starts
  async let b = fetch_b()   // starts
  async let c = fetch_c()   // starts
  use(await a, await b, await c)
}
// If a fails: b and c are cancelled. do propagates a's error.
// If b fails: a and c are cancelled. do propagates b's error.
// This matches do's existing behavior: first error exits the block.
```

**Rules:**

1. `do` exits on the first `Result` error → `async let` + `do` exits on the first failed task
2. All sibling tasks are cancelled before error propagation
3. Scope exit (normal or error) cancels all un-awaited handles
4. Partial success is never observable — all succeed or all fail

**Rationale:**
- Consistent with `do` — sequential and concurrent code follow the same fail-fast rule
- AI doesn't need to write cleanup logic for partially-succeeded parallel operations
- "All succeed or all fail" is the simplest mental model

### Comparison with Swift

| | Swift | Almide |
|---|---|---|
| Fork | `async let a = fetchA()` | `async let a = fetch_a()` |
| Join | `await a` | `await a` |
| Error handling | `try await a` (explicit per use) | `do` block handles all errors automatically |
| Scope exit | Un-awaited tasks auto-cancelled | Same |
| race/timeout | `TaskGroup` (manual) | `race()` / `timeout()` stdlib functions |

Almide's advantage: `do` block absorbs error handling, so no `try` noise at every join point.

## Composition with `do` blocks

```almide
async fn checkout(cart: Cart) -> Result[Order, AppError] =
  do {
    async let stock = verify_stock(cart.items)
    async let payment = authorize_payment(cart.total)
    // both must succeed — if either fails, the other is cancelled, do propagates error
    await finalize_order(await stock, await payment)
  }
```

## race / timeout — stdlib functions, not syntax

No new syntax needed. These are async stdlib functions:

```almide
// Race — first to complete wins, rest cancelled
let fastest = await race(fetch_cache(key), fetch_db(key))

// Timeout — fail if not complete within duration
let data = await timeout(5s, fetch(url))

// Sleep
await sleep(100ms)
```

---

## Multi-Target Async: 調査結果と設計判断

### 他言語の WASM async 対応状況

| 言語 | WASM async | アプローチ | 制約 |
|------|-----------|-----------|------|
| **Rust** | wasm-bindgen-futures | Future↔Promise ブリッジ。ブラウザの microtask queue に委譲。独自 executor 不要 | WASM 側は完全シングルスレッド |
| **SwiftWasm** | 2つの executor | ① cooperative executor（CLI/WASI用、ホストに制御を返さない）② JS event loop executor（ブラウザ用、明示的切替） | libdispatch 非対応 |
| **AssemblyScript** | **未対応** | event loop がないため async/await 自体が存在しない。Stack Switching 提案待ち | WASM stack switching (Phase 3) 依存 |
| **Kotlin/Wasm** | Beta | GC proposal 必須。コルーチンの WASM 対応は未公開 | ブラウザ版のみ、coroutine 対応不明 |
| **Gleam** | JSターゲットのみ | 「Erlang と JS は非互換な concurrency システム」。concurrency はライブラリ層で提供 | actor model は Erlang でのみ動作 |

### WASM エコシステムの async 関連仕様

| 仕様 | フェーズ | 内容 | Almide への影響 |
|------|---------|------|----------------|
| **JSPI** (JS Promise Integration) | **Phase 4 (標準化済)** | WASM↔JS Promise の自動ブリッジ。同期的 WASM コードから async JS API を透過的に呼べる。~1μs/call。Chrome 137+, Firefox 139+ | **最重要**。Almide WASM ターゲットのベース技術 |
| **Asyncify** (Binaryen) | 利用可能 | コンパイル時変換で WASM スタックを保存/復元。コードサイズ +50% | JSPI が使えない環境でのフォールバック |
| **Threads + SharedArrayBuffer** | 標準化済 | Worker 間メモリ共有。CORS 制約あり | 真の並列が必要な場合のみ。Phase 1 では不要 |
| **Stack Switching** | Phase 3 | WASM レベルのコルーチン/fiber | 将来的には cooperative executor の基盤になりうる |

### 核心的な洞察

**WASM 環境での「並行」はすべてシングルスレッド上の協調的マルチタスク。** 真の並列実行は存在しない。

これは Almide にとって都合がいい：
- `async let` の意味論が「I/O 待ちの並行」に限定される（CPU 並列ではない）
- LLM が書くコードの大半は「複数の fetch を同時に発火して全部待つ」パターン
- 複雑なスレッド安全性の問題が発生しない

### 設計判断

#### 判断 1: `Future[T]` を型システムに入れるか

**判断: 入れない。暗黙的に扱う。**

理由:
- `async fn foo() -> Int` の戻り型は `Int`（`Future[Int]` ではない）
- `await` は型レベルでは no-op（`T → T`）。効果は codegen のみ
- `async let x = foo()` で `x` の型は `Int`。`await x` も `Int`
- Swift と同じアプローチ: `async let` のバインディングは「まだ利用不可の T」であり、`await` が「利用可能にする」
- `Future[T]` を露出させると、LLM が `Future[Future[T]]` やジェネリクス境界で混乱する

型チェッカーの実装:
- `async let x = expr` → `x` の型は `expr` の戻り型 `T`。ただし `consumed: false` フラグ付き
- `await x` → 型は `T`。`consumed = true` に変更
- 2回目の `await x` → コンパイルエラー「handle already consumed」
- スコープ終了時に `consumed = false` のバインディング → 警告（cancellation が発生する）

#### 判断 2: Rust ターゲットの executor

**判断: tokio を使う。ただし `Send + 'static` 制約を回避する設計にする。**

理由:
- `almide_block_on` の busy-wait (dummy waker + `yield_now` ループ) は本番では使えない
- 独自 executor は保守コストが高く、エコシステムとの互換性がない
- tokio は Rust async のデファクト標準

回避策:
- `async let` は `tokio::spawn` ではなく `tokio::task::JoinSet` + ローカル参照で実装
- `Send` 制約が問題になる場合は `tokio::task::LocalSet` を使う（シングルスレッド executor）
- `almide_block_on` → `tokio::runtime::Runtime::block_on` に置換

```rust
// 現在の almide_block_on (busy-wait — 廃止予定)
fn almide_block_on<F: std::future::Future>(future: F) -> F::Output {
    let waker = unsafe { Waker::from_raw(dummy_raw_waker()) };
    let mut cx = Context::from_waker(&waker);
    let mut future = Box::pin(future);
    loop {
        match future.as_mut().poll(&mut cx) {
            Poll::Ready(val) => return val,
            Poll::Pending => std::thread::yield_now(),
        }
    }
}

// 置換後
fn almide_block_on<F: std::future::Future>(future: F) -> F::Output {
    tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap()
        .block_on(future)
}
```

依存関係の影響:
- `Cargo.toml` に `tokio = { version = "1", features = ["rt", "time", "macros"] }` 追加
- 生成バイナリのサイズ増 (~数百KB)
- WASM ターゲットでは tokio を使わない（別パス）

#### 判断 3: TS ターゲット

**判断: native async/await をそのまま使う。`async let` は即座に Promise を開始する変数束縛に変換。**

```typescript
// Almide: async let a = fetch_a()
// TS:     const __a_promise = fetch_a();  // 即座に開始

// Almide: await a
// TS:     await __a_promise

// キャンセルは AbortController で実装
const __a_ctrl = new AbortController();
const __a_promise = fetch_a({ signal: __a_ctrl.signal });
// スコープ終了時: __a_ctrl.abort()
```

課題:
- AbortController はすべての async 関数が `signal` パラメータを受け取る必要がある
- Almide stdlib の async 関数には暗黙的に signal を注入するか、キャンセルは best-effort にするか

**判断: キャンセルは best-effort。** Promise を abort しても実行中の fetch は止まらない場合がある。これは JS の制約であり、Almide が解決すべき問題ではない。スコープ終了時に `Promise.allSettled` で待つだけで十分。

#### 判断 4: WASM ターゲット

**判断: JSPI ベースで実装。Phase 1 では eager sequential fallback。**

JSPI (Phase 4、Chrome 137+ / Firefox 139+) により、同期的な WASM コードから async JS API を呼べる。Almide の WASM ターゲットでは:

```
async let a = fetch_a()   →  WASM 内では逐次実行（JSPI が suspend/resume）
async let b = fetch_b()   →  a 完了後に b 開始
await a                    →  既に完了済
await b                    →  既に完了済
```

Phase 1 では「`async let` はシングルスレッドで逐次実行」に degradation する。これは正しくないが安全（デッドロックしない、結果は同じ、ただし遅い）。

将来の改善パス:
1. **JSPI + Promise.all**: `async let` の複数タスクを JS 側で `Promise.all` にまとめ、WASM は1回の suspend で全完了を待つ
2. **Stack Switching (Phase 3 待ち)**: WASM 内で cooperative scheduling が可能になれば、真の並行が実現

#### 判断 5: `async let` の `do` ブロック必須制約

**判断: `async let` は `do` ブロック内でなくても使える。ただし `do` 内ではキャンセル伝播が自動化される。**

理由:
- `do` 必須にすると、エラーを返さない async 関数（`async fn foo() -> Int`）が `do` を強制される
- Swift も `async let` を `do` なしで使える

```almide
// do なし — エラーなし関数の並行実行
async fn fast_compute() -> Int =
  async let a = compute_a()
  async let b = compute_b()
  await a + await b

// do あり — エラーありの並行実行（キャンセル伝播付き）
async fn risky_compute() -> Result[Int, String] =
  do {
    async let a = try_compute_a()
    async let b = try_compute_b()
    await a + await b
  }
```

---

## 現在の実装状況（Layer 1）

### 実装済み

- `async fn` / `await` のパース（AST: `Decl::Fn { async: Some(bool) }`, `Expr::Await`）
- 型チェック: `async fn` は `effect fn` と同等に扱う。`await` は `Result<T, E>` → `T` のアンラップ
- IR: `IrExprKind::Await`, `IrFunction { is_async }`
- Rust codegen: `async fn` → Rust `async fn`。`await` → `almide_block_on(expr)`
- TS codegen: `async fn` → TS `async function`。`await` → `await expr`
- HTTP stdlib にネイティブ async 関数あり

### 既知の問題

1. **`almide_block_on` が busy-wait**: dummy waker + `yield_now` ループ。CPU を浪費し、真の async I/O が動作しない
2. **`Future[T]` 型がない**: 型システムは `Result` で代用。`await` の型チェックが不完全
3. **テストがゼロ**: async 関連のテストファイルが存在しない
4. **`async let` 未実装**: パーサー、チェッカー、codegen すべて未着手

---

## 実装フェーズ（改訂版）

### Phase 0: Layer 1 安定化（前提条件）

Layer 2 に進む前に、既存の async/await を安定化する。

- [ ] **`almide_block_on` を tokio に置換**
  - `tokio::runtime::Builder::new_current_thread().enable_all().build().unwrap().block_on(future)`
  - 生成コードの `Cargo.toml` に tokio 依存を追加する codegen 変更
  - WASM ターゲットでは tokio を使わない分岐を入れる
- [ ] **Layer 1 テスト追加** (`spec/lang/async_test.almd`)
  - `async fn` の宣言と `await` での呼び出し
  - `async fn` + `do` ブロックでのエラー伝播
  - `async fn` 内での stdlib async 関数呼び出し (`http.get` 等)
  - フォーマッタの `async fn` / `await` 対応確認
- [ ] **`await` 型チェック修正**: 現在 `Result<T, E> → T` だが、非 Result 型もパススルーしている。`async fn` の戻り型に基づいて正しくチェック

### Phase 1: `async let` + scope-based cancellation

**パーサー**:
- [ ] `async let name = expr` を新しい `Stmt::AsyncLet` として追加
  - `Stmt` enum に `AsyncLet { name, value, span }` バリアント追加
  - `parse_stmt()` で `TokenType::Async` + `TokenType::Let` のペアを検出
  - braceless block / do block / braced block すべてで使用可能

**型チェッカー**:
- [ ] `async let x = expr` → `x` の型は `expr` の戻り型 `T`
- [ ] `x` に `awaited: bool` トラッキングフラグを付与
- [ ] `await x` → `awaited = true` に変更。型は `T`
- [ ] 2回目の `await x` → コンパイルエラー「future handle already consumed」
- [ ] スコープ終了時に未 await の `async let` バインディング → 警告「un-awaited async binding will be cancelled」
- [ ] `async let` は `async fn` 内でのみ使用可能（それ以外ではエラー）

**IR**:
- [ ] `IrStmtKind::AsyncLet { var: VarId, value: IrExpr }` 追加
- [ ] `IrExprKind::AwaitHandle { var: VarId }` 追加（`Await` とは別。ハンドル変数の join）

**Rust codegen**:
```rust
// async let a = fetch_a()
// ↓
let __handle_a = tokio::task::spawn_local(async move { fetch_a().await });

// await a
// ↓
let a = __handle_a.await.unwrap();

// スコープ終了（do ブロック）— sibling cancellation
// ↓
// JoinSet::abort_all() + drop
```
- [ ] `tokio::task::LocalSet` ベースの spawn（`Send` 制約回避）
- [ ] `do` ブロック内: 最初のエラーで `JoinSet::abort_all()`
- [ ] スコープ終了時のドロップガード生成
- [ ] main 関数のエントリポイントに `#[tokio::main]` または `LocalSet::new().run_until()` ラッパー

**TS codegen**:
```typescript
// async let a = fetch_a()
// ↓
const __a_promise = fetch_a();

// await a
// ↓
const a = await __a_promise;

// do ブロック内のエラー伝播は既存の try/catch で処理
```
- [ ] `async let` → 即座に Promise を開始する `const` 束縛に変換
- [ ] `await x` → `await __x_promise`
- [ ] `do` ブロック内: `Promise.allSettled` でのクリーンアップは best-effort

**WASM codegen**:
- [ ] Phase 1 では eager sequential fallback: `async let a = f()` → `let a = await f()` と同等
- [ ] コンパイラ警告: 「WASM target: async let runs sequentially」

**テスト** (`spec/lang/async_let_test.almd`):
- [ ] 基本: `async let` + `await` で値を取得
- [ ] 複数: 3つの `async let` を同時に開始して `await`
- [ ] 消費: `await x` を2回呼ぶとコンパイルエラー
- [ ] エラー伝播: `do` 内で1つが失敗 → 残りがキャンセル
- [ ] スコープ終了: un-awaited binding の警告

### Phase 2: `race` / `timeout` / `sleep` stdlib

- [ ] `stdlib/defs/async.toml` に定義追加
- [ ] `race(futures...)`: 最初に完了したものを返し、残りをキャンセル
- [ ] `timeout(duration, future)`: 期限内に完了しなければ `err("timeout")`
- [ ] `sleep(duration)`: 指定時間待機
- [ ] Duration リテラル (`5s`, `100ms`) のパーサー対応 — or — `sleep(5000)` (ms as Int) で簡略化

**Rust codegen**:
```rust
// race(a, b) → tokio::select! { v = a => v, v = b => v }
// timeout(5000, f) → tokio::time::timeout(Duration::from_millis(5000), f).await
// sleep(100) → tokio::time::sleep(Duration::from_millis(100)).await
```

**TS codegen**:
```typescript
// race(a, b) → Promise.race([a, b])
// timeout(5000, f) → Promise.race([f, new Promise((_, rej) => setTimeout(() => rej(new Error("timeout")), 5000))])
// sleep(100) → new Promise(r => setTimeout(r, 100))
```

- [ ] テスト (`spec/stdlib/async_test.almd`)

### Phase 3: Async streams（将来）

- [ ] `Stream[T]` 型
- [ ] `stream.for_each(fn(item) => ...)`, `stream.map(...)`, `stream.collect()`
- [ ] `for item in stream { }` — 通常の `for...in` が Stream を認識
- [ ] Backpressure via bounded channels
- [ ] Note: `for await x in stream { }` 構文は追加しない。`for...in` の既存構文で対応

---

## 未決事項

### Q1: Duration リテラルを言語に入れるか

```almide
// Option A: Duration リテラル（新しい構文）
await sleep(100ms)
await timeout(5s, fetch(url))

// Option B: Int (ミリ秒) で表現（新構文なし）
await sleep(100)
await timeout(5000, fetch(url))
```

**仮判断: Option B**。Vocabulary Economy の原則。Duration 型は stdlib で定義可能。

### Q2: `async let` を `var` でも許可するか

```almide
async let x = fetch()     // immutable — OK
async var x = fetch()     // mutable? — 意味不明
```

**判断: `async let` のみ。** `async var` は意味論的に矛盾（handle は mutable にできない）。

### Q3: `async let` のキャプチャ制約

```almide
var count = 0
async let a = do {
  count = count + 1    // ← 親スコープの var を変更できるか?
  fetch(url)
}
```

**判断: 禁止。** `async let` のボディは親スコープの `var` をキャプチャできない。`let` バインディングの読み取りのみ許可。理由: 並行実行でのデータ競合を構造的に防ぐ。

### Q4: Nested `async let`

```almide
async let a = do {
  async let b = fetch_inner()    // ← ネスト可能か?
  await b
}
```

**仮判断: 許可。** Swift と同様。内側の `async let` は内側のスコープで管理。

---

## Dependencies

- Layer 1 (`async fn` / `await`) — DONE（安定化が必要）
- Phase 0: tokio 導入 + テスト追加
- Phase 1: パーサー、チェッカー、IR、codegen すべてに変更

## Status

設計改訂完了。実装は Phase 0（Layer 1 安定化）から開始。
