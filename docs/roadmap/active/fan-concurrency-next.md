<!-- description: fan as a language-level concurrency primitive with Flow[T] and compiler-driven optimization -->
# Fan Concurrency — Next Generation

## Vision

fan を「並行処理ライブラリ」ではなく「コンパイラが理解する言語機能」として完成させる。

ユーザーが見る世界：

```almide
fan { a(); b() }           // 並行ブロック
fan.map(items, f)          // 並行マップ
fan.race([a, b])           // 最速が勝つ
```

3つの構文が fan の全表面。ストリーミングは `Flow[T]` 型 + 既存の `list` 操作で表現し、新しいモジュールは導入しない。

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

### Flow[T] — Typed Lazy Sequence

`Flow[T]` は遅延シーケンスの型。`List[T]` とは別の型にすることで、危険な操作（`list.len`, `list.get`, `list.reverse`）をコンパイル時に弾く。

新しいモジュールは導入しない。前方走査のみの `list` 操作（`list.filter`, `list.map`, `list.fold` 等）が `Flow[T]` にもそのまま動く。

```almide
let lines: Flow[String] = file.lines(path)

// OK — 前方走査操作は Flow で動く
lines
  |> list.filter((line) => string.contains(line, "ERROR"))
  |> list.map((line) => parse_log(line))
  |> list.fold(0, (acc, _) => acc + 1)

// OK — fan.map は List でも Flow でも動く
lines
  |> list.filter((line) => line != "")
  |> fan.map(limit: 10, (line) => process(line))

// コンパイルエラー — Flow は有限長・ランダムアクセスを保証しない
list.len(lines)       // error: list.len requires List[T], got Flow[T]
list.get(lines, 5)    // error: list.get requires List[T], got Flow[T]
list.reverse(lines)   // error: list.reverse requires List[T], got Flow[T]
```

### list Operations on Flow[T]

`list` 操作を2カテゴリに分類し、`Flow[T]` で使えるものをコンパイラが判別する。

| 使える（前方走査） | 使えない（ランダムアクセス / 全体走査） |
|---|---|
| `list.filter` | `list.len` |
| `list.map` | `list.get` |
| `list.fold` / `list.reduce` | `list.reverse` |
| `list.take` | `list.sort` |
| `list.drop` | `list.contains` |
| `list.each` | `list.find` (※後述) |
| `list.flat_map` | `list.zip` (List 同士のみ) |
| `list.enumerate` | |

`list.find` は前方走査で短絡するため Flow で使える可能性があるが、見つからなかった場合に全要素を消費する。要検討。

### Compiler Responsibilities

| 判断 | コンパイラの挙動 |
|---|---|
| パイプライン融合 | `list.filter \|> list.map \|> fan.map` → 単一 Iterator chain + parallel consumer |
| バックプレッシャー | `Flow[T]` + `fan.map(limit: n)` → bounded pipeline が自動成立 |
| バックエンド選択 | Rust → threads/tokio、WASM → sequential/JSPI |
| 並行度 | `limit:` あり → bounded、なし → 要素数分（List）/ コンパイラ警告（Flow） |

### fan.map on Flow[T]

`fan.map` は入力が `List[T]` ならバッチ並行、`Flow[T]` ならストリーミング並行。同じ構文。

```almide
// バッチ: 全部メモリに乗る
fan.map([1, 2, 3, 4, 5], (n) => compute(n))

// ストリーミング: pull 駆動、バックプレッシャーあり
fan.map(file.lines(path), limit: 10, (line) => process(line))
```

`Flow[T]` に対する `fan.map` で `limit:` が省略された場合、コンパイラ警告を出す（unbounded parallel on infinite source）。

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

`fan.link(capacity)` は `(Sender[T], Flow[T])` を返す。`rx` は `Flow[T]` なので `list.len` 等は使えない（コンパイルエラー）。

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

| Feature | WASM (current) | WASM (JSPI) | WASM (WASI 0.3) |
|---|---|---|---|
| `fan { }` | sequential | `Promise.all` | `task.spawn` |
| `fan.map` | sequential loop | async + bounded | `stream` |
| `fan.race` | first only | `Promise.race` | `waitable-set` |
| `fan.link` | sync queue | `MessageChannel` | `stream<T>` |
| `Flow[T]` | eager List | `AsyncIterator` | `stream<T>` |

target がバックエンドを決める。Almide コードは同一。

## Implementation Plan

### Phase 1: Flow[T] Type

言語の型システムに `Flow[T]` を追加する。

- [ ] `Flow[T]` 型を checker に追加
- [ ] `list` 操作の Flow 互換性を分類（前方走査 = OK、ランダムアクセス = エラー）
- [ ] `file.lines(path)` → `Flow[String]` を返すように
- [ ] `Flow[T]` に対する禁止操作のコンパイルエラーメッセージ
- [ ] Rust codegen: `Flow[T]` → `Box<dyn Iterator<Item = T>>`
- [ ] WASM codegen: eager fallback（即時 List として実行）
- [ ] テスト: `spec/lang/flow_test.almd`

### Phase 2: Pipeline Fusion

`list` 操作 + `fan.map` のパイプラインをコンパイラが融合する。

- [ ] `list.filter |> list.map |> fan.map` → 単一 Iterator chain + parallel consumer
- [ ] `fan.map(flow, limit: n, f)` → bounded parallel streaming
- [ ] `Flow[T]` に対する `fan.map` で `limit:` 省略時にコンパイラ警告
- [ ] Rust codegen: `Iterator` chain + thread pool
- [ ] テスト: 大量データのストリーミングパイプライン、バックプレッシャー検証

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

### Phase 1-2 (Flow + Fusion)
- `crates/almide-frontend/src/check/infer.rs` — `Flow[T]` 型推論、list 操作の互換性チェック
- `crates/almide-ir/src/lib.rs` — Flow ソース IR 表現
- `crates/almide-codegen/src/walker/expressions.rs` — Iterator chain codegen
- `crates/almide-codegen/src/pass_fan_lowering.rs` — pipeline fusion pass
- `runtime/rs/src/fan.rs` — bounded parallel consumer

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

**やること:**
- `Flow[T]` 型（list 操作の互換性をコンパイル時チェック）
- パイプライン融合（list 操作 + fan.map の最適化）
- fan の語彙拡張（rush, spawn, link）
- 構造的キャンセル
- Rust target の async 移行
- WASM の JSPI / WASI 0.3 対応

**やらないこと:**
- ストリーム専用モジュール（`flow.*`）— list 操作が Flow にも効く
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
