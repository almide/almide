<!-- description: Unified async/concurrency design using effect fn and fan syntax -->
# Fan Concurrency

> **`effect fn` が非同期の境界。`fan` が並行の構文。async/await は書かない。**

本ドキュメントは以下を統合し、Almide の非同期・並行処理の唯一の設計仕様とする:
- structured-concurrency.md（`async let` / `await` 設計 → `fan` に置換）
- platform-async.md（`parallel` ブロック設計 → `fan` に置換、透過的 async 思想は継承）
- http モジュールの非同期対応

---

## 1. 設計原則

### 1.1 透過的 async

ユーザーは `async` / `await` / `Promise` / `Future` を一切書かない。

```almide
effect fn get_user(id: String) -> User = {
  let text = http.get("/users/${id}")
  json.parse(text)
}
```

このコードから各ターゲットに自動変換される:

| ターゲット | 生成コード | ランタイム |
|-----------|-----------|-----------|
| `almide run` (native) | `async fn` + `.await` | tokio |
| `--target ts` | `async function` + `await` | fetch |
| `--target js` | `async function` + `await` | fetch |
| `--target wasm` (browser) | `async function` + `await` | fetch API |
| `--target wasm` (WASI) | `async fn` | wasi-http |
| `--target py` (将来) | `async def` + `await` | asyncio |
| `--target go` (将来) | 通常関数 | goroutine |
| `--target rb` (将来) | 通常メソッド | Thread / Fiber |
| `--target c` (将来) | 通常関数 | pthread |

### 1.2 関数カラーリングの解消

```
pure fn       → sync（副作用なし）
effect fn     → async（I/O、副作用あり）
```

- `effect fn` は自動的に async になる。`async fn` キーワードは不要
- `effect fn` 内から `effect fn` を呼ぶと、コンパイラが `.await` を自動挿入
- `pure fn` から `effect fn` を呼ぶ → コンパイルエラー（既存の制約）
- `main` → async エントリポイント（`#[tokio::main]` / top-level await）

### 1.3 LLM 最適化

LLM コーディングエージェントが async/await で失敗するパターンを構造的に不可能にする。

| LLM が犯すミス | Almide での対処 |
|----------------|----------------|
| `await` 忘れ → Promise が変数に入る | `await` が存在しない。コンパイラ自動挿入 |
| 直列にすべきところを並列に | fan 内では他の式の結果を参照できない |
| 並列にすべきところを直列に | fan に入れるだけで並列になる |
| task を作って join を忘れる | タスクハンドルが露出しない |
| `async` / `await` の配置ミス | そもそもキーワードが存在しない |

判断分岐は1つだけ: **「この2つの処理は依存関係があるか？」**

- ある → `let` を2行書く（直列）
- ない → `fan { }` に入れる（並列）

---

## 2. `fan` — 並行処理の統一構文

### 2.1 `fan { }` — 静的 fan-out/fan-in

固定個数の独立 effect を同時開始し、全完了を待つ。

```almide
effect fn dashboard(id: String) -> Dashboard = {
  let (user, posts) = fan {
    fetch_user(id)
    fetch_posts(id)
  }
  Dashboard { user, posts }
}
```

### 2.2 `fan.map(xs, f)` — 動的 fan-out/fan-in

コレクションの各要素に対して effect を並行実行し、結果をリストで返す。

```almide
effect fn fetch_all(ids: List[String]) -> List[User] = {
  fan.map(ids, fn(id) { fetch_user(id) })
}
```

### 2.3 `fan.race(thunks)` — 最速1つだけ

thunk（遅延実行の関数）群を同時開始し、最初に完了した結果を返す。残りはキャンセル。

```almide
effect fn fast_fetch(id: String) -> String = {
  fan.race([
    fn() { http.get("https://primary.api/users/" ++ id) },
    fn() { http.get("https://replica.api/users/" ++ id) },
  ])
}
```

### 2.4 段階的な依存グラフ

`fan` の連鎖で依存グラフがコードの上下関係にそのまま現れる。

```almide
effect fn full_dashboard(user_id: String) -> Dashboard = {
  // 第1段: 互いに独立な2つを並行
  let (user, location) = fan {
    fetch_user(user_id)
    geo.current_location()
  }

  // 第2段: 第1段の結果に依存する3つを並行
  let (weather, posts, recs) = fan {
    fetch_weather(location.city)
    fetch_posts(user.id)
    fetch_recommendations(user.id, location.country)
  }

  // 第3段: 直列（書き込みは順序が要る）
  fs.mkdir_p(folder)
  fs.write(path, render(user, weather, posts))
}
```

```
第1段:  [fetch_user]  [get_location]     <- fan（並行）
              |              |
第2段:  [fetch_posts] [fetch_weather] [fetch_recs]  <- fan（並行）
              |              |             |
第3段:  [write file]                       <- let（直列）
```

---

## 3. 意味論

### 3.1 `fan { e1; e2; ...; en }`

- **型**: `(T1, T2, ..., Tn)` — 各式の **成功値** の型のタプル
- **実行**: 全式を同時開始、全完了待ち
- **結果順**: 記述順（実行完了順ではない）
- **Result 伝播**: 式が `Result[T, E]` を返す場合、成功値 `T` がタプルに入る。`Err` なら fan 全体が effect failure + 残りキャンセル
- **構文制限**: 式のみ。`let` / `var` / `for` / `match` は禁止
- **外部変数**: 外のスコープの `let` 束縛は読取可能。`var` のキャプチャは禁止（データ競合防止）
- **文位置**: 代入なしの `fan { ... }` は許可。結果は `Unit` に潰す

### 3.2 `fan.map(xs, f)`

- **型**: `List[T]` — `f` の **成功値** の型のリスト
- **実行**: 各要素に `f` を並行適用
- **結果順**: 入力順（実行完了順ではない）
- **Result 伝播**: `f` が `Result[T, E]` を返す場合、`Err` が出たら全体 failure + 残りキャンセル
- **将来拡張**: `fan.map(xs, limit: 16, f)` で並行数制限

### 3.3 `fan.race(thunks)`

- **型**: `T` — thunk の戻り型
- **引数**: `List[Fn[] -> T]` — thunk のリスト
- **実行**: 全 thunk を同時開始
- **結果**: **最初に完了したもの**（成功でも失敗でも）
- **残り**: キャンセル
- **空リスト**: コンパイルエラー

### 3.4 thunk が必要な理由

`fan.race` は関数（構文ではない）なので、引数は通常の評価ルールで先に評価される。`fn() { ... }` で包むことで `fan.race` が開始タイミングを管理できる。`fan { }` は構文なのでコンパイラが制御でき、thunk 不要。

### 3.5 Result 伝播の設計根拠

**fan は Result を自動 unwrap する。** `Err` が返ったら fan 全体が failure。

```almide
let (user, posts) = fan {
  fetch_user(id)    // Result[User, String] → 成功なら User
  fetch_posts(id)   // Result[List[Post], String] → 成功なら List[Post]
}
// user: User, posts: List[Post]
// どちらかが Err なら fan 全体が effect failure、残りはキャンセル
```

理由:

1. **effect fn の自動 `?` と同じ意味論**。一貫性がある
2. **全ターゲットの native 挙動と一致**。`Promise.all` は reject 伝播、`tokio::try_join!` は Err 伝播、`asyncio.gather` は例外伝播
3. **LLM にとって最もシンプル**。fan に入れたら成功値が出てくる、失敗したら fan ごと倒れる
4. **Almide には effect failure と Result Err の区別がない**。唯一のエラーチャネルが `Result`

検討したが採用しなかった案:
- **道 A** (effect failure と Result を別物に): 第2エラーチャネルが必要。設計が複雑化
- **道 B** (Err でもキャンセルしない): 「1つ失敗したら残り無駄」のケースで非効率

### 3.6 effect 制約

- `fan { }` は `effect fn` 内でのみ使用可能
- `fan.map` / `fan.race` も effect
- pure fn 内で fan → コンパイルエラー

---

## 4. `fan` の言語上の位置づけ

- `fan` は**予約語**（`let fan = 123` は不可）
- `fan { }` は**特別構文**（ブロックではなく clause list）
- `fan.map` / `fan.race` は **compiler-known namespace**
- ユーザー定義の `fan` モジュールは不可

---

## 5. 既存機能との相互作用

| 機能 | 影響 |
|------|------|
| `effect fn` | async 化。構文変更なし |
| `do` ブロック | 同じ動作。auto-`?` も健在 |
| `guard` | async コンテキスト内で動作 |
| `for...in` | 逐次イテレーション。各ステップで await |
| Result / Option | 変更なし。`?` 伝播も健在 |
| パイプ `\|>` | effect fn の場合、各ステップで await |
| UFCS | `x.method()` が effect fn なら await |
| Lambda | effect fn の lambda も可。fan 内でキャプチャ可 |
| `fan` | 新規。`effect fn` 内のみ |

---

## 6. Node Promise 表現力との対応

| Node.js | Almide | 挙動 |
|---------|--------|------|
| `Promise.all()` | `fan { }` / `fan.map` | 全成功 or 最初の失敗で reject |
| `Promise.race()` | `fan.race` | 最初の完了（成功でも失敗でも） |
| `Promise.any()` | `fan.any`（将来） | 最初の成功。全失敗なら AggregateError |
| `Promise.allSettled()` | `fan.settle`（将来） | 全結果回収（失敗も含む） |

fan ファミリー全体:

| API | 挙動 | 返り値 | 初版 |
|-----|------|--------|------|
| `fan { }` | 全部やって全部待つ（静的） | タプル | Yes |
| `fan.map` | 全部やって全部待つ（動的） | リスト | Yes |
| `fan.race` | 全部やって最速を取る | 単一値 | Yes |
| `fan.any` | 最初の成功を取る | 単一値 | 後で |
| `fan.settle` | 全結果を取る（失敗含む） | リスト | 後で |
| `fan.timeout` | タイムアウト付き実行 | 単一値 | 後で |

---

## 7. HTTP モジュールとの統合

### 7.1 現在の状態

- HTTP クライアント関数（`http.get`, `http.post` 等）は `effect = true`
- Rust: `std::net::TcpStream` による同期ブロッキング I/O
- TS: `await fetch(...)` による非同期 I/O
- `http.serve` ハンドラは純粋コンテキスト（effect fn を呼べない）

### 7.2 fan 統合後の姿

**クライアント**: effect fn として自動 async 化。変更なし。

```almide
effect fn load_data() -> (User, List[Post]) = {
  // fan で並行リクエスト
  let (user, posts) = fan {
    http.get_json("/users/1")
    http.get_json("/posts?user=1")
  }
  (parse_user(user), parse_posts(posts))
}
```

**サーバー**: ハンドラを effect fn 化（各リクエストを独立タスクとして処理）。

```almide
// 将来: ハンドラが effect コンテキストになる
effect fn handle(req: Request) -> Response = {
  let id = http.req_path(req)
  // ハンドラ内で他の effect fn を呼べるようになる
  let (user, prefs) = fan {
    fetch_user(id)
    fetch_preferences(id)
  }
  http.json(200, json.stringify(render(user, prefs)))
}

effect fn main() -> Unit = {
  http.serve(3000, handle)
}
```

### 7.3 HTTP 非同期の移行ステップ

| 段階 | Rust | TS |
|------|------|----|
| 現在 | `std::net` 同期 I/O | `await fetch()` |
| Phase 0 | tokio + `reqwest` 非同期 I/O | 変更なし |
| Phase 1 | `http.serve` → `tokio::spawn` per request | 変更なし |
| 将来 | connection pooling, graceful shutdown | 変更なし |

---

## 8. Codegen 詳細

### 8.1 `effect fn` の codegen

| ターゲット | `effect fn` | I/O 呼び出し | エントリポイント |
|-----------|-------------|-------------|----------------|
| Rust | `async fn -> Result<T, String>` | `.await` 自動挿入 | `#[tokio::main]` |
| TS/JS | `async function` | `await` 自動挿入 | top-level await |
| Python | `async def` | `await` 自動挿入 | `asyncio.run()` |
| Go | 通常関数 | 同期呼び出し | `func main()` |
| Ruby | 通常メソッド | 同期呼び出し | 通常実行 |
| C | 通常関数 | 同期呼び出し | `int main()` |

非 I/O の effect fn（乱数生成、タイムスタンプ等）も async codegen になる。オーバーヘッドは軽微。モデルのシンプルさを優先。

### 8.2 `fan { fetch_a(); fetch_b() }`

**TypeScript**:
```typescript
const [a, b] = await Promise.all([
  fetchA(),
  fetchB(),
]);
```

**Rust (tokio)**:
```rust
let (a, b) = tokio::try_join!(
    fetch_a(),
    fetch_b(),
)?;
```

**Python**:
```python
a, b = await asyncio.gather(
    fetch_a(),
    fetch_b(),
)
```

**Go**:
```go
var a TypeA
var b TypeB
var wg sync.WaitGroup
var errOnce sync.Once
var firstErr error
wg.Add(2)
go func() { defer wg.Done(); v, e := fetchA(); if e != nil { errOnce.Do(func(){firstErr=e}) } else { a = v } }()
go func() { defer wg.Done(); v, e := fetchB(); if e != nil { errOnce.Do(func(){firstErr=e}) } else { b = v } }()
wg.Wait()
if firstErr != nil { return firstErr }
```

**Ruby**:
```ruby
results = [
  Thread.new { fetch_a },
  Thread.new { fetch_b },
].map(&:value)
a, b = results
```

**C**:
```c
pthread_t t1, t2;
void *r1, *r2;
pthread_create(&t1, NULL, fetch_a_wrapper, args);
pthread_create(&t2, NULL, fetch_b_wrapper, args);
pthread_join(t1, &r1);
pthread_join(t2, &r2);
```

### 8.3 `fan.map(xs, f)`

| ターゲット | 生成コード |
|-----------|-----------|
| TS | `await Promise.all(xs.map(x => f(x)))` |
| Rust | `futures::future::try_join_all(xs.iter().map(\|x\| f(x))).await?` |
| Python | `await asyncio.gather(*[f(x) for x in xs])` |
| Go | `WaitGroup` + goroutine per element |
| Ruby | `xs.map { \|x\| Thread.new { f(x) } }.map(&:value)` |
| C | `pthread_create` per element + `pthread_join` all |

### 8.4 `fan.race(thunks)`

| ターゲット | 生成コード |
|-----------|-----------|
| TS | `await Promise.race(thunks.map(f => f()))` |
| Rust | `tokio::select! { v = f1() => v, v = f2() => v }` |
| Python | `asyncio.wait(..., return_when=FIRST_COMPLETED)` + cancel pending |
| Go | buffered channel + goroutines, first send wins |
| Ruby | `Thread` array, first `.value` wins |
| C | `pthread_create` + shared flag for first completion |

---

## 9. ターゲット別設計判断

### 9.1 Rust: tokio

**判断: tokio をデフォルト executor とする。**

```rust
// 現在の almide_block_on (busy-wait — 廃止)
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
#[tokio::main]
async fn main() -> Result<(), String> { ... }
```

- `Send` 制約回避: `tokio::task::LocalSet` を使用（シングルスレッド executor）
- `fan { }`: `tokio::try_join!` — fail-fast
- `fan.map`: `futures::future::try_join_all`
- `fan.race`: `tokio::select!`
- HTTP client: `std::net` → `reqwest`（async http client）に移行

生成 `Cargo.toml` への追加:
```toml
[dependencies]
tokio = { version = "1", features = ["rt", "time", "macros"] }
reqwest = { version = "0.12", features = ["json"] }
futures = "0.3"
```

バイナリサイズ増: ~数百 KB。WASM ターゲットでは tokio を使わない（別パス）。

### 9.2 TypeScript / JavaScript

**判断: native async/await をそのまま使う。**

- `effect fn` → `async function`
- effect fn 呼び出し → `await`
- `fan { }` → `Promise.all([...])`
- `fan.race` → `Promise.race([...])`
- キャンセルは **best-effort**（JS の制約。`Promise.race` は loser を abort しない）
- 追加依存: なし（`fetch` と `Promise` は built-in）

### 9.3 WASM

**判断: JSPI ベース。Phase 0 では逐次実行 fallback。**

| WASM 仕様 | フェーズ | Almide への影響 |
|-----------|---------|----------------|
| **JSPI** (JS Promise Integration) | Phase 4（標準化済） | 最重要。WASM↔JS Promise 自動ブリッジ。Chrome 137+, Firefox 139+ |
| **Asyncify** (Binaryen) | 利用可能 | JSPI 非対応環境のフォールバック。コードサイズ +50% |
| **Threads + SharedArrayBuffer** | 標準化済 | Phase 0 では不要 |
| **Stack Switching** | Phase 3 | 将来: WASM 内 cooperative scheduling |

核心的洞察: **WASM の「並行」はすべてシングルスレッド上の協調的マルチタスク。** 真の並列は存在しない。これは Almide に好都合 — データ競合の問題が発生しない。

Phase 0: `fan { }` は WASM で逐次実行に degradation（正しいが遅い。デッドロックしない、結果は同じ）。
Phase 1: JSPI + `Promise.all` で JS 側に委譲し、真の並行を実現。

### 9.4 他言語の WASM async 対応（参考調査）

| 言語 | WASM async | 制約 |
|------|-----------|------|
| Rust | wasm-bindgen-futures | Future↔Promise ブリッジ。シングルスレッド |
| SwiftWasm | 2つの executor | cooperative (CLI) / JS event loop (browser) |
| AssemblyScript | 未対応 | Stack Switching 待ち |
| Kotlin/Wasm | Beta | GC proposal 必須 |
| Gleam | JS ターゲットのみ | concurrency はライブラリ層 |

### 9.5 Python / Go / Ruby / C（将来ターゲット）

| ターゲット | effect fn | fan | 依存 |
|-----------|-----------|-----|------|
| Python | `async def` + `await` | `asyncio.gather` / `asyncio.wait` | asyncio (stdlib) |
| Go | 通常関数 | goroutine + WaitGroup / channel | なし |
| Ruby | 通常メソッド | Thread / Async gem | なし |
| C | 通常関数 | pthread | pthread (POSIX) |

---

## 10. 型システム設計判断

### 10.1 `Future[T]` は型システムに入れない

**判断: 入れない。暗黙的に扱う。**

- `effect fn foo() -> Int` の戻り型は `Int`（`Future[Int]` ではない）
- コンパイラが内部的に async を追跡
- `Future[T]` を露出させると LLM が `Future[Future[T]]` で混乱する

### 10.2 Duration リテラルは入れない

**判断: Int（ミリ秒）で表現。**

```almide
fan.timeout(5000, fn() { http.get(url) })
env.sleep_ms(100)
```

Duration 型は将来 stdlib で定義可能。Vocabulary Economy の原則。

### 10.3 `var` キャプチャの禁止

```almide
var count = 0
let (a, b) = fan {
  do { count = count + 1; fetch_a() }   // <- コンパイルエラー
  fetch_b()
}
```

fan 内から親スコープの `var` は変更不可。`let` の読み取りのみ許可。データ競合を構造的に防ぐ。

---

## 11. 現在の実装状況

### 11.1 実装済み

- `async fn` / `await` のパース（AST: `Decl::Fn { async: Some(bool) }`, `Expr::Await`）
- 型チェック: `async fn` は `effect fn` と同等扱い
- IR: `IrExprKind::Await`, `IrFunction { is_async }`
- Rust codegen: `async fn` → Rust `async fn`、`await` → `almide_block_on(expr)`
- TS codegen: `async fn` → TS `async function`、`await` → `await expr`
- HTTP stdlib: 22 クライアント/サーバー関数実装済み（Rust: `std::net` 同期 I/O）
- **`fan { }` 基盤実装完了** (2026-03-16):
  - Lexer: `fan` 予約語
  - AST: `Expr::Fan { exprs }`
  - Parser: `fan { expr; expr; ... }` パース
  - Checker: effect fn 内のみ許可、Result auto-unwrap、型はタプル
  - IR: `IrExprKind::Fan { exprs }`
  - Rust codegen: `std::thread::scope` + `spawn` per expr（tokio 不要）
  - TS codegen: `await Promise.all([...])`
  - Formatter: `fan { }` 対応
  - E2E 動作確認済み (`examples/fan_demo.almd`)
- **Effect isolation (Layer 1 security)** (2026-03-16):
  - pure fn → effect fn 呼び出しをコンパイルエラーに
  - fan block も pure fn 内ではエラー

### 11.2 既知の問題

1. **`almide_block_on` が busy-wait**: dummy waker + `yield_now` ループ。CPU 浪費、真の async I/O 不動作
2. **`Future[T]` 型がない**: 型システムは `Result` で代用。`await` の型チェック不完全
3. **async テストがゼロ**: async 関連テストファイル未作成
4. **`http.serve` ハンドラが純粋コンテキスト**: effect fn を呼べない
5. ~~**fan 未実装の制約**~~: `var` キャプチャ禁止チェック済み

---

## 12. 実装フェーズ

### Phase 0: `fan { }` 基盤 — sync/thread backend

**設計方針変更**: tokio 非依存。`effect fn` は同期のまま。`fan` は `std::thread::scope` で並行化。

**パーサー**:
- [x] `fan` を予約語に追加
- [x] `fan { expr; expr; ... }` → `Expr::Fan { exprs: Vec<Expr> }`
- [x] fan 内の `let` / `var` / `for` / `while` → パースエラー

**型チェッカー**:
- [x] `fan { e1; ...; en }` の型 → `(T1, ..., Tn)`（Result 自動 unwrap）
- [x] effect fn 内のみ許可
- [ ] 各式間で変数非共有を検証
- [x] 外部 `var` キャプチャ禁止

**IR**:
- [x] `IrExprKind::Fan { exprs: Vec<IrExpr> }` 追加

**Rust codegen**:
- [x] `std::thread::scope` + `spawn` per expr に変換

**TS codegen**:
- [x] `await Promise.all([e1, e2, ...])` に変換

**フォーマッター**:
- [x] `fan { }` のフォーマット対応

**テスト**:
- [x] Rust unit テスト（checker_test.rs — fan in pure fn / fan in effect fn）
- [x] E2E 動作確認（`examples/fan_demo.almd` — `almide run` で実行成功）
- [x] spec テスト (`spec/lang/fan_test.almd` — 5 pass)

### Phase 1: async backend (将来)

tokio は backend の1実装として後から追加。言語仕様は runtime 非依存。

- [ ] `effect fn` → Rust `async fn` codegen (opt-in backend)
- [ ] `fan` → `tokio::try_join!` (async backend)
- [ ] runtime trait 経由で spawn/join/sleep を抽象化

### Phase 2: `fan.map` ✅

- [x] `fan.map(xs, f)` を compiler-known 関数として登録
- [x] 型: `(List[A], Fn(A) -> B) -> List[B]`（Result auto-unwrap）
- [x] Rust: `std::thread::scope` + `spawn` per item
- [x] TS: `await Promise.all(xs.map(f))`
- [x] テスト (`spec/lang/fan_map_test.almd` — 4 pass)

### Phase 3: `fan.race` ✅

- [x] `fan.race(thunks)` を compiler-known 関数として登録
- [x] 型: `(List[Fn() -> T]) -> T`（Result auto-unwrap）
- [x] Rust: `std::thread::scope` + `mpsc::channel`（最初の完了値を取得）
- [x] TS: `await Promise.race(thunks.map(f => f()))`
- [x] テスト (`spec/lang/fan_race_test.almd` — 2 pass)

### Phase 4: サーバー非同期

- [ ] `http.serve` ハンドラを effect コンテキスト化
- [ ] Rust: 各リクエストを `tokio::spawn` で独立タスク化
- [ ] connection pooling
- [ ] graceful shutdown
- [ ] テスト

### Phase 5: 拡張 ✅ (主要 API 完了)

- [x] `fan.any` — 最初の成功を返す。`Promise.any` 相当 (Rust: `mpsc::channel`, TS: `Promise.any`)
- [x] `fan.settle` — 全結果を返す。`Promise.allSettled` 相当 (Rust: thread + collect, TS: `Promise.allSettled`)
- [x] `fan.timeout(ms, thunk)` — タイムアウト付き実行 (Rust: deadline loop, TS: `Promise.race` + setTimeout)
- [x] テスト (`spec/lang/fan_ext_test.almd` — 4 pass)
- [ ] `fan.map(xs, limit: n, f)` — 並行数制限（将来）
- [ ] `env.sleep_ms` → `fan.sleep` に移動検討（将来）

### Phase 6 (将来): ストリーミング

- [ ] `websocket` モジュール（connect, send, receive, close）
- [ ] `http.stream` for SSE
- [ ] Rust: `tokio-tungstenite` / `reqwest` streaming
- [ ] TS: `WebSocket` API / `ReadableStream`
- [ ] `for item in stream { }` — 既存の `for...in` が Stream を認識

---

## 13. この設計が置換するもの

| 旧設計 | fan での対応 |
|--------|------------|
| `async fn` (structured-concurrency) | 不要。`effect fn` がそのまま async |
| `await expr` (structured-concurrency) | 不要。コンパイラが自動挿入 |
| `async let` (structured-concurrency) | `fan { }` に置換 |
| `parallel { }` (platform-async) | `fan { }` に置換 |
| `race()` stdlib | `fan.race` に統合 |
| `timeout()` stdlib | `fan.timeout`（将来） |
| `sleep()` stdlib | `env.sleep_ms` のまま / 将来 `fan.sleep` |

## 14. 追加しないもの

| 機能 | 理由 |
|------|------|
| `async` キーワード | 不要 — `effect fn` で代替 |
| `await` キーワード | 不要 — コンパイラ自動挿入 |
| `Future[T]` / `Promise` 型 | 内部のみ — ユーザーは `Result[T, E]` |
| 手動タスク spawn | `fan` を使う |
| チャネル / メッセージパッシング | supervision-and-actors.md に延期 |
| アクターモデル | supervision-and-actors.md に延期 |

## キーワード追加

| キーワード | 用途 |
|-----------|------|
| `fan` | 並行処理ブロック + 名前空間（唯一の追加） |

---

## Status

設計統合完了。実装は Phase 0（非同期基盤）から開始。
