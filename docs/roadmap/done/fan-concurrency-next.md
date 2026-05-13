<!-- description: fan as a language-level concurrency primitive with rush/spawn/link/cancel -->
# Fan Concurrency — Next Generation

## Vision

fan を「並行処理ライブラリ」ではなく「コンパイラが理解する言語機能」として完成させる。

ユーザーが見る世界：

```almide
fan { a(); b() }           // 並行ブロック
fan.map(items, f)          // 並行マップ
fan.race([a, b])           // 最速が勝つ
```

3 つの構文が fan の全表面。ストリーミングは [`flow-design.md`](./flow-design.md) で扱う `Flow[T]` 型と統合される。

## Why Language-Level

stdlib 関数としての fan：
- `fan.map(items, f)` はただの関数呼び出し。コンパイラは意味を知らない
- パイプラインの融合最適化ができない

言語機能としての fan：
- コンパイラが fan の意味論を理解する
- `list.filter |> list.map |> fan.map` を単一パスに融合できる
- target に応じて threads / tokio / Promise.all / sequential を自動選択

fan は既にキーワードで、既に IR ノードがあり、既に専用 codegen パスがある。これを徹底する。

## Design Influences

| 着想元 | 何を学んだか |
|---|---|
| Verse (Epic) | `race`/`rush`/`sync`/`branch` — 4語で並行パターンの全空間をカバー |
| Zig 0.16 | async ≠ concurrent の明確な分離 |
| Kotlin Flow | cold + pull-based + suspension-based backpressure の実績 |
| Gleam | 型付きチャネルでコンパイル時安全性 |
| OCaml 5 | Domain（物理並列）+ Effect handler（論理並行）の二層構造 |
| Vale | parallel foreach で外部変数を自動 freeze |
| Roc | target が実行戦略を決める |

## What Already Works

| Feature | Status |
|---|---|
| `fan { }` (static fan-out) | ✅ `std::thread::scope` |
| `fan.map(xs, f)` / `fan.map(xs, limit: n, f)` | ✅ thread pool |
| `fan.race(thunks)` | ✅ `mpsc::channel` |
| `fan.any(thunks)` | ✅ first-success |
| `fan.settle(thunks)` | ✅ collect all |
| `fan.timeout(ms, thunk)` | ✅ deadline |
| `effect fn` = 副作用境界 | ✅ `Result<T, String>` |
| `var` キャプチャ禁止 | ✅ データ競合の構造的排除 |
| WASM fan | ✅ sequential fallback |

## Design

### Surface: Three Constructs

```almide
// 1. 並行ブロック — 全部散らして全部待つ
let (a, b) = fan { fetch(url1); fetch(url2) }

// 2. 並行マップ — リストを並行処理
let results = fan.map(urls, (url) => fetch(url))

// 3. レース — 最速が勝ち
let fastest = fan.race([fetch(url1), fetch(url2)])
```

### Flow[T] との統合

`Flow[T]` (lazy streaming sequence) の設計は別 roadmap [`flow-design.md`](./flow-design.md) に切り出した。**`flow.*` 名前空間**で提供され、動詞は `list.*` と揃える (`flow.map`, `flow.filter`, `flow.fold`, ...)。

ここでは fan との interaction 原則のみを述べる。詳細は `flow-design.md` 参照。

#### 原則 1: `fan.map` は入力型で戻り型がディスパッチされる

```almide
// List 入力 → List 出力 (バッチ並行)
fan.map(xs: List[T], limit: Int?, f: (T) -> U) -> List[U]

// Flow 入力 → Flow 出力 (ストリーミング並行)
fan.map(xs: Flow[T], limit: Int?, f: (T) -> U) -> Flow[U]
```

戻り型が入力型に追従するので、`file.lines(path) |> fan.map(limit: 10, f) |> flow.filter(p) |> flow.collect()` が自然に書ける。

#### 原則 2: Flow + `fan.map(limit: n)` で自動バックプレッシャー

`limit` は「最大同時ワーカー数」。Flow に対して使うと、**ワーカーが空くまで上流から pull しない** ので、upstream のメモリ圧迫を防ぐ。ユーザーは buffer サイズや channel 容量を明示的に書かなくていい。

#### 原則 3: `limit:` 省略時の挙動

| 入力型 | `limit:` 省略時 |
|---|---|
| `List[T]` | 要素数分の並列 (上限なし) |
| `Flow[T]` | **コンパイラ警告** (unbounded parallel on possibly-infinite source) |

#### 原則 4: fan スコープの cancel が Flow を cancel する

```almide
fan(timeout: 30000) {
  file.lines("huge.log")!
    |> flow.filter(is_error)
    |> fan.map(limit: 10, process)
    |> flow.each(write_result)
}
// 30 秒で timeout → 全 worker cancel、file handle close、upstream Flow drop
```

**構造的キャンセル** が Flow にも波及する。Rust `thread::scope` + `Drop` で自然に実現。

#### 原則 5: 順序保証

`fan.map(flow, limit: n, f)` の出力順は **未定義** (worker 終了順)。入力順保証が必要な場合は `fan.ordered_map` (将来追加検討) か、`enumerate + sort` を手動で。Phase 2 はまず unordered で実装。

### Compiler Responsibilities

| 判断 | コンパイラの挙動 |
|---|---|
| `fan.map` ディスパッチ | 入力が List なら batch、Flow なら streaming |
| バックプレッシャー | `Flow[T]` + `fan.map(limit: n)` → bounded pipeline が自動成立 |
| バックエンド選択 | Rust → threads/tokio、WASM → sequential/JSPI |
| 並行度 | `limit:` あり → bounded、なし → 要素数分 (List) / 警告 (Flow) |

### Advanced: Progressive Disclosure

初日の3つ（`fan {}`, `fan.map`, `fan.race`）で足りなくなったら、段階的に発見する：

```
既存（実装済み）
  fan.any         最初の成功を取る
  fan.settle      全結果（成功+失敗）を収集
  fan.timeout     期限付き実行

新規
  fan.rush        最速の値を返す、残りは走り続ける
  fan.spawn       投げっぱなし、スコープ脱出で自動回収
  fan.link        型付きチャネル（Flow[T] を生成）
```

#### fan.rush — First Result, Others Continue

```almide
effect fn fetch_with_cache(url: String) -> String = {
  fan.rush([
    () => cache.get(url),
    () => {
      let body = http.get(url)
      cache.set(url, body)
      body
    },
  ])
}
```

#### fan.spawn — Fire-and-Forget with Scope Cleanup

```almide
effect fn server(port: Int) -> Unit = {
  fan.spawn(() => metrics.flush_loop(5000))
  fan.spawn(() => healthcheck.loop(10000))

  http.serve(port, (req) => handle(req))
  // serve 終了 → spawn されたタスクは自動中止
}
```

#### fan.link — Typed Channel

```almide
effect fn pipeline(items: List[String]) -> Unit = {
  let (tx, rx) = fan.link(10)   // rx: Flow[T]

  fan {
    items |> list.each((item) => tx.send(item)); tx.close()
    rx |> fan.map(limit: 5, (item) => store(item))
  }
}
```

`fan.link(capacity)` は `(Sender[T], Flow[T])` を返す。`rx` は `Flow[T]` なので `list.len` 等は使えない (コンパイルエラー)。Flow 側の詳細は [`flow-design.md`](./flow-design.md) 参照。

### Scoped Cancellation

```almide
fan(timeout: 30000) {
  fan.map(urls, (url) => http.get(url))
}
```

- `fan(timeout: ms) { }` — スコープ全体に期限。期限到達で全子タスクキャンセル
- `fan.race` の敗者 — 自動キャンセル
- `fan` スコープ脱出 — 全子タスク自動キャンセル
- `fan.spawn` — スコープ終了で自動回収

キャンセルは構造的。明示的なキャンセル API はない。スコープが寿命を管理する。

### WASM Strategy

Primary target: **Component Model async (WASI 0.3)**. Browser target uses Web APIs.

| Feature | WASM (current) | WASM (browser) | WASM (WASI 0.3) |
|---|---|---|---|
| `fan { }` | sequential | `Promise.all` | `future<T>` + `waitable-set` |
| `fan.map` | sequential loop | async + bounded | `stream<T>` + bounded consumer |
| `fan.race` | first only | `Promise.race` | `waitable-set.poll` |
| `fan.link` | sync queue | `MessageChannel` | `stream<T>` |
| `Flow[T]` | eager List | `AsyncIterator` | `stream<T>` |

- **WASI containers** (Spin, wasmCloud, Docker+WASM): Component Model async. Host runtime is the executor. No threads, no language-side scheduler
- **Browser**: SharedArrayBuffer + Web Workers for true parallelism, or JSPI for async
- **Fallback**: Sequential execution when neither is available

target がバックエンドを決める。Almide コードは同一。

## Implementation Plan

### Phase 1: Flow[T] Type

別 roadmap [`flow-design.md`](./flow-design.md) の Phase 1 で実装。`Flow[T]` 型、`flow.*` 12 関数 API、`file.lines` の runtime、forbidden ops のエラー、Rust codegen までを扱う。fan との interaction はこの段階では入れない (Flow 単体で完結)。

### Phase 2: fan.map × Flow Integration

`fan.map` を Flow 対応させる。これは fan 側の作業。

- [ ] `fan.map(xs: Flow[T], limit: Int?, f)` → `Flow[U]` のディスパッチ
- [ ] Rust codegen: `thread::scope` + channel で bounded parallel consumer
- [ ] `limit:` 省略時の Flow 警告
- [ ] 構造的 cancel: fan scope 抜けで Flow drop、file handle close
- [ ] テスト: 大量データのストリーミングパイプライン、backpressure 検証
- [ ] 前提: `flow-design.md` Phase 1 完了

### Phase 3: fan.rush + fan.spawn

- [ ] `fan.rush(thunks)` — 最速の値を返し、残りは続行
- [ ] `fan.spawn(thunk)` — スコープ脱出で自動キャンセル
- [ ] IR に `Rush`, `Spawn` ノード追加
- [ ] Rust codegen: `thread::scope` + channel
- [ ] WASM codegen: rush = sequential、spawn = immediate
- [ ] テスト: `spec/lang/fan_rush_test.almd`, `spec/lang/fan_spawn_test.almd`

### Phase 4: fan.link + Scoped Cancellation

- [ ] `fan.link(capacity)` → `(Sender[T], Flow[T])`
- [ ] `tx.send(value)` — バッファ満杯で suspend
- [ ] `tx.close()` — Flow の終端
- [ ] `fan(timeout: ms) { }` — スコープ全体に期限
- [ ] スコープ脱出時の子タスク自動キャンセル
- [ ] `fan.race` の敗者自動キャンセル
- [ ] Rust codegen: `sync_channel` + cancel flag
- [ ] テスト: producer-consumer、タイムアウト、キャンセル

### Phase 5: Async Runtime (Rust Target)

バックエンドを async に移行。ユーザーから見える変更なし。

- [ ] `effect fn` → Rust `async fn` codegen
- [ ] `.await` 自動挿入（effect fn 呼び出し時）
- [ ] `#[tokio::main(flavor = "current_thread")]` エントリポイント
- [ ] `fan { }` → `tokio::try_join!`
- [ ] `fan.map` → `Flow` なら `StreamExt` + `buffer_unordered`、`List` なら `try_join_all`
- [ ] `fan.race` → `tokio::select!`
- [ ] `fan.link` → `tokio::sync::mpsc`
- [ ] HTTP: `reqwest` async + `http.serve` with tokio
- [ ] Generated `Cargo.toml` に `tokio`, `futures` 追加
- [ ] 既存テスト全パス確認

### Phase 6: WASM Concurrency

- [ ] JSPI: `fan { }` → `Promise.all`、`fan.race` → `Promise.race`
- [ ] WASI 0.3: `Flow[T]` → `stream<T>` マッピング
- [ ] Feature detection: JSPI 可能なら使用、不可なら sequential
- [ ] テスト: ブラウザ + WASI 両環境

## Files to Modify

### Phase 1 (Flow[T])
[`flow-design.md`](./flow-design.md) の Files to Modify 参照。

### Phase 2 (fan × Flow)
- `runtime/rs/src/fan.rs` — `fan_map_flow` bounded consumer
- `crates/almide-codegen/src/pass_fan_lowering.rs` — Flow 対応
- `stdlib/defs/fan.toml` — `fan.map` の Flow overload 定義
- `spec/integration/flow_fan_test.almd` — ストリーミング × 並行のテスト

### Phase 3-4 (Fan modes + Channel + Cancel)
- `crates/almide-frontend/src/check/infer.rs` — rush, spawn, link の型推論
- `crates/almide-ir/src/lib.rs` — `Rush`, `Spawn`, `Link` IR ノード
- `crates/almide-codegen/src/walker/expressions.rs` — 新 fan モードの codegen
- `crates/almide-codegen/src/pass_fan_lowering.rs` — cancel propagation
- `runtime/rs/src/fan.rs` — rush, spawn, channel, cancel

### Phase 5-6 (Async + WASM)
- `crates/almide-codegen/src/walker/declarations.rs` — `effect fn` → `async fn`
- `crates/almide-codegen/src/walker/expressions.rs` — `.await` 挿入
- `codegen/templates/rust.toml` — async テンプレート
- `src/cli/build.rs` — generated Cargo.toml に依存追加
- `runtime/rs/src/http.rs` — reqwest async
- `crates/almide-codegen/src/emit_wasm/` — JSPI / WASI 0.3

## Scope Boundary

**やること (fan スコープ):**
- `fan.map` の Flow 対応 (入力型で戻り型をディスパッチ)
- fan の語彙拡張 (rush, spawn, link)
- 構造的キャンセル (fan スコープ抜けで Flow も含めて drop)
- Rust target の async 移行
- WASM の JSPI / WASI 0.3 対応

**やらないこと:**
- `Flow[T]` 型自体の設計 — [`flow-design.md`](./flow-design.md) で扱う
- Actor モデル — effect fn + fan + 不変性で不要
- ユーザー定義 effect
- 分散並行、GPU 並列、トランザクショナルメモリ

## References

- Verse: `race`/`rush`/`sync`/`branch` — [Epic Games](https://verselang.github.io/book/14_concurrency/)
- Zig: async/concurrent split — [Andrew Kelley](https://andrewkelley.me/post/zig-new-async-io-text-version.html)
- Kotlin Flow: backpressure — [kotlinlang.org](https://kotlinlang.org/docs/flow.html)
- Gleam: typed channels — [hexdocs.pm](https://hexdocs.pm/gleam_otp/gleam/otp/actor.html)
- OCaml 5 Eio: effects — [github.com](https://github.com/ocaml-multicore/eio)
- Vale: regions — [verdagon.dev](https://verdagon.dev/blog/seamless-fearless-structured-concurrency)
- WASI 0.3: async — [Component Model](https://github.com/WebAssembly/component-model/blob/main/design/mvp/Async.md)
