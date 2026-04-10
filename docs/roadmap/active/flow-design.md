<!-- description: Flow[T] lazy streaming sequences with separate flow.* namespace aligned with list.* -->
# Flow[T] — Lazy Streaming Sequences

## Design Philosophy

Almide は一般に語彙削減を重視するが、Flow については **lazy/eager 境界と stream 性を局所的に明示する価値が高い** ため、`flow.*` 名前空間を採用する。

ただし学習コストを抑えるため、**操作名は `list.*` と可能な限り揃える**。`list.map` と `flow.map`、`list.filter` と `flow.filter` のように、動詞は共有し prefix だけ変える。LLM は動詞の語彙を 1 セット覚えればよく、「型 → prefix」のマッピングは 1 ルールだけ追加で覚えればよい。

境界 API は `flow.collect` 1 個のみ。**暗黙変換は行わない**。`file.lines` のような streaming source は必ず `Flow[T]` を返す。

## Why Not the "shared namespace" approach?

検討の結果、`list.*` を List[T] / Flow[T] 両対応にする案は以下の理由で却下した:

1. **前例がない**: MoonBit, Gleam, Kotlin, Rust どれも名前空間を分けている
2. **user 定義関数が不自然**: `Seq[T]` のような抽象が必要になり、実装コストが高い
3. **転移学習が効きにくい**: LLM は `iterator.filter`, `stream.map` 等を既に大量に訓練データで見ている
4. **長距離依存で型を見失う**: モジュール名が型情報を運ばないと、LLM が遠くの定義を追えない
5. **stdlib だけ多態 magic で user は普通、という不整合**: これが一番 LLM を混乱させる

保守的な `flow.*` 分離 + 動詞揃えの方が、**Almide のミッション (LLM が最も正確に書ける言語) により適合する**。

## Core Type

`Flow[T]` は **lazy, single-pass, forward-only** なシーケンス。

| 性質 | 意味 |
|---|---|
| **lazy** | terminal operation (fold/each/collect) が呼ばれるまで値を pull しない |
| **single-pass** | 一度消費したら再利用不可。`let x = flow; fold(x); fold(x)` は 2 回目がエラー or 空 |
| **forward-only** | ランダムアクセス不可。`len`, `get`, `reverse`, `sort` はコンパイル時に拒否 |

これは MoonBit の `Iter[T]` の性質を継承している。

## Minimum API (12 functions)

### Transformations (6)

`list.*` と動詞を揃える。

```almide
flow.map[T, U](xs: Flow[T], f: (T) -> U) -> Flow[U]
flow.filter[T](xs: Flow[T], pred: (T) -> Bool) -> Flow[T]
flow.filter_map[T, U](xs: Flow[T], f: (T) -> Option[U]) -> Flow[U]
flow.flat_map[T, U](xs: Flow[T], f: (T) -> Flow[U]) -> Flow[U]
flow.take[T](xs: Flow[T], n: Int) -> Flow[T]
flow.drop[T](xs: Flow[T], n: Int) -> Flow[T]
```

### Terminal operations (3)

**ここで評価が走る**。docs で強調する。

```almide
flow.fold[T, U](xs: Flow[T], init: U, combine: (U, T) -> U) -> U  // 🔴 terminal
flow.each[T](xs: Flow[T], f: (T) -> Unit) -> Unit                  // 🔴 terminal
flow.collect[T](xs: Flow[T]) -> List[T]                            // 🔴 terminal
```

### Source constructors (3)

```almide
flow.from_list[T](xs: List[T]) -> Flow[T]
flow.generate[S, T](seed: S, step: (S) -> Option[(T, S)]) -> Flow[T]  // 無限対応
flow.empty[T]() -> Flow[T]
```

### 組み込みソース (stdlib 他モジュール経由)

- `file.lines(path: String) -> Flow[String]` — 必ず Flow を返す
- その他の streaming I/O が将来追加される場合も Flow を返す方針

## Key Rules (決定事項)

### R1. `file.lines` は `Flow[String]` を返す

小さいファイル全体を読む場合は `fs.read_text` + `string.lines` を使う。**曖昧にしない**。

```almide
// 小さい config → fs.read_text
let content = fs.read_text("config.toml")!
let lines: List[String] = string.lines(content)

// 巨大ログ → file.lines
let lines: Flow[String] = file.lines("system.log")!
```

### R2. `flow.collect` が唯一の Flow → List 境界

暗黙変換なし。型注釈による強制変換もなし。**`flow.collect()` と明示的に書く**のが慣用。

```almide
let xs: List[String] = flow |> flow.collect()  // 明示
```

### R3. `flow.take(n)` の戻り型は `Flow[T]`

一貫性優先。特例なし。List が欲しければ `flow.take(n) |> flow.collect()`。

### R4. Terminal operations は docs で明示マーク

cheatsheet と API リファレンスで `fold`, `each`, `collect` に **🔴 terminal** マークを付ける。「ここで実行される」を視覚的に強調する。

### R5. `list.*` と `flow.*` の動詞セットは必ず揃える

両方にある関数は **同じ動詞名** を使う。片方だけ別名にしない。

**禁止例**:
- ❌ `flow.keep_if` (list が `filter` なら flow も `filter`)
- ❌ `flow.transform` (list が `map` なら flow も `map`)
- ❌ `flow.reduce` (list が `fold` なら flow も `fold`)

### R6. Single-pass を明示

仕様書の冒頭で宣言する:

> Flow は single-pass です。一度 consume (fold/each/collect) した Flow は再利用できません。
> ```almide
> let flow = file.lines("log")!
> let n = flow |> flow.fold(0, (acc, _) => acc + 1)  // OK
> let m = flow |> flow.fold(0, (acc, _) => acc + 1)  // ❌ error: flow already consumed
> ```

### R7. `List[T] → Flow[T]` は `flow.from_list` で明示

自動昇格なし。**使う時は書く**。

```almide
let xs: List[Int] = [1, 2, 3]
let f: Flow[Int] = flow.from_list(xs)  // 明示
```

## Forbidden Operations (compile-time 拒否)

以下は `Flow[T]` に対して呼ぶと **コンパイルエラー** になる:

| 操作 | 理由 | 代替 |
|---|---|---|
| `list.len(flow)` | Flow は長さを持たない (無限かも) | `flow.fold(0, (acc, _) => acc + 1)` |
| `list.get(flow, i)` | Flow はランダムアクセス不可 | `flow.drop(i) \|> flow.take(1) \|> flow.collect()` |
| `list.reverse(flow)` | Flow は forward-only | `flow.collect()` してから `list.reverse` |
| `list.sort(flow)` | ソートは全要素 in-memory が必要 | `flow.collect()` してから `list.sort` |
| `list.contains(flow, x)` | 無限ループ可能性 | `flow.fold(false, (acc, y) => acc or y == x)` |
| `list.find(flow, pred)` | 見つからない場合に無限走査 | `flow.filter(pred) \|> flow.take(1) \|> flow.collect() \|> list.first` |

## Error Message Templates

```
error[E011]: Flow[T] does not support random access
  --> bad.almd:5:11
   |
 5 |   let n = list.len(lines)
   |           ^^^^^^^^^^^^^^^
   = note: `lines` has type Flow[String] (lazy, possibly infinite)
   = hint: to count elements lazily:
           flow.fold(lines, 0, (acc, _) => acc + 1)
   = hint: to materialize first (may use unbounded memory):
           list.len(flow.collect(lines))
```

**hint は 2 つ提示する**:
1. Flow 的な解決策 (推奨)
2. materialize 経由の解決策 (memory 警告付き)

LLM がどちらも学べるように。

## fan との兼ね合い

Flow と fan は **直交しつつ統合される**。詳細は [`fan-concurrency-next.md`](./fan-concurrency-next.md) に委譲。ここでは interaction の原則のみ:

### 原則 1: fan.map は入力型で挙動が変わる

```almide
// List 入力 → List 出力 (バッチ並行)
fan.map(xs: List[T], limit: Int?, f: (T) -> U) -> List[U]

// Flow 入力 → Flow 出力 (ストリーミング並行)
fan.map(xs: Flow[T], limit: Int?, f: (T) -> U) -> Flow[U]
```

**戻り型は入力型に追従**。これで `file.lines |> fan.map |> flow.filter |> flow.collect` が自然に書ける。

### 原則 2: `fan.map(flow, limit: n)` で自動バックプレッシャー

`limit` は「最大同時ワーカー数」。Flow に対して使うと、**ワーカーが空くまで上流から pull しない** ので、upstream のメモリ圧迫を防ぐ。

### 原則 3: `limit:` 省略時の挙動

- List: 要素数分の並列 (上限なし)
- Flow: **コンパイラ警告** (unbounded parallel on possibly-infinite source)

### 原則 4: 順序保証

`fan.map(flow, limit: n, f)` の出力順は **未定義** (worker 終了順)。入力順保証が必要な場合は `fan.ordered_map` (将来追加検討) を使うか、手動で enumerate + sort を行う。

Phase 2 ではまず **unordered** で実装。ordered は demand が出てから追加。

### 原則 5: fan スコープの cancel が Flow を cancel する

```almide
fan(timeout: 30000) {
  file.lines("huge.log")!
    |> flow.filter(is_error)
    |> fan.map(limit: 10, process)
    |> flow.each(write_result)
}
// 30 秒で timeout → 全 worker cancel、file handle close、upstream Flow drop
```

**構造的キャンセル** が Flow にも波及する。これは Rust `thread::scope` + `Drop` で自然に実現できる。

## Implementation Phases

### Phase 1: Core Flow type + 12 API

- [ ] `Ty::Flow(Box<Ty>)` を checker に追加
- [ ] `flow` モジュールの 12 関数を `stdlib/defs/flow.toml` に定義
- [ ] `file.lines(path) -> Flow[String]` の runtime 実装 (`runtime/rs/src/file.rs`)
- [ ] Forbidden operations のコンパイル時エラー (`E011`)
- [ ] Error message の hint 2 本立て
- [ ] Rust codegen: `Flow[T]` → `Box<dyn Iterator<Item = T>>` or `impl Iterator`
- [ ] WASM codegen: eager fallback (最初は無効化、Phase 4 で対応)
- [ ] テスト: `spec/lang/flow_test.almd`, `spec/lang/flow_error_test.almd`

### Phase 2: fan.map on Flow

- [ ] `fan.map` を Flow 対応 (入力型で return 型をディスパッチ)
- [ ] Rust codegen: `thread::scope` + channel で bounded consumer
- [ ] `limit:` 省略時の Flow 警告
- [ ] 構造的 cancel (fan scope 抜けで Flow drop)
- [ ] テスト: 大量データのストリーミングパイプライン、backpressure 検証

詳細は `fan-concurrency-next.md` の Phase 2 と並行。

### Phase 3: Polish & Extension

- [ ] 追加関数 (demand 次第):
  - [ ] `flow.zip(other)` — 2 つの Flow を lockstep で zip
  - [ ] `flow.chain(other)` — 2 つの Flow を連結
  - [ ] `flow.take_while(pred)` / `flow.drop_while(pred)`
  - [ ] `flow.enumerate` — index 付き
  - [ ] `flow.inspect(f)` — デバッグ用 passthrough
- [ ] source builder 構文 `flow { emit(...) }` の要否判断
- [ ] Single-pass 検出 (2 回目の consume を compile/runtime で検出)

### Phase 4: Async / WASM backend

- [ ] Rust async 移行時の Flow → `Stream` mapping
- [ ] WASM (WASI 0.3) での `Flow[T]` → `stream<T>` mapping
- [ ] JSPI での Flow 実装
- [ ] Feature detection

詳細は `fan-concurrency-next.md` の Phase 5-6 と並行。

## Files to Modify

### Phase 1
- `crates/almide-types/src/types/mod.rs` — `Ty::Flow` 追加
- `crates/almide-frontend/src/check/calls.rs` — Forbidden op の検出
- `crates/almide-ir/src/lib.rs` — Flow 操作の IR 表現
- `crates/almide-codegen/src/walker/expressions.rs` — Flow codegen
- `runtime/rs/src/file.rs` — `file.lines` 実装
- `stdlib/defs/flow.toml` — **新規**、12 関数の定義
- `stdlib/defs/file.toml` — `lines` 関数追加
- `build.rs` — Flow module 登録
- `spec/lang/flow_test.almd` — **新規**
- `spec/lang/flow_error_test.almd` — **新規**
- `docs/specs/flow.md` — **新規**、仕様書 (この roadmap の詳細版)

### Phase 2
- `runtime/rs/src/fan.rs` — `fan_map_flow` 追加
- `crates/almide-codegen/src/pass_fan_lowering.rs` — Flow 対応
- `spec/integration/flow_fan_test.almd` — **新規**

## MSR Benchmark (方向の実証)

Phase 1 実装と並行して、Flow を活かす exercise を **10 問** benchmark に追加する:

1. ログファイルから ERROR 件数を集計 (`flow.filter + flow.fold`)
2. CSV → JSON Lines 変換 (`flow.map + flow.each`)
3. URL リストを並列ダウンロード (`flow + fan.map`)
4. 巨大ファイルの先頭 N 行取得 (`flow.take + flow.collect`)
5. 指定パターン match 行抽出 (grep-like)
6. 重複行除去 (accumulator pattern)
7. 複数ファイルのマージ (将来 `flow.chain` が入れば自然)
8. stdin 読み込みの streaming (`io.read_lines → Flow[String]`)
9. 無限生成から先頭 N 個 (`flow.generate + flow.take`)
10. 複雑な pipeline (filter + map + fold + side effect)

`research/benchmark/msr/exercises/` に追加。LLM が `flow.*` を自然に書けるか、`list.*` との混同が起きないかを実測する。

## References

- **MoonBit Iter[T]** — single-pass, lazy, Iter を関数間で渡すことを推奨する設計。最近い前例
- **Kotlin Flow** — `flow { emit(...) }` builder、backpressure の suspension-based 実装
- **Gleam iterator** — List / Iterator を別モジュールにする分離、fold on infinite の注意喚起
- **Rust Iterator** — semantics の直接元
- **Haskell lazy I/O** — 反面教師。`file.lines` の resource cleanup 設計で踏まない

## Open Questions (Phase 1 着手前に決める)

1. **`flow.take(n)` 後の `flow.collect()` の可変サイズ**: 無限 source から take して collect は安全 (n 個だけ pull)。ただし LLM が `flow.collect` を先に書いてしまうミスをどう防ぐか?
2. **Error propagation**: `file.lines` が `Flow[Result[String, IOError]]` か `Result[Flow[String], IOError]` か。前者は行ごとエラー対応、後者はオープンのみエラー。
3. **Single-pass 違反の検出タイミング**: compile-time (線形型が必要) / runtime panic / silent re-consume のどれか
4. **`flow.generate` の有限性**: `step` が None を返したら終わる契約だが、型としては無限と区別しない
5. **Drop semantics**: Flow が drop されたとき upstream にどう通知するか (Rust の `Drop` trait で自然に流れる想定)
