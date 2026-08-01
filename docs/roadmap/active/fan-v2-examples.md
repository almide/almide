<!-- description: Reference examples for the fan v2 surface: every head, edge semantics, diagnostics -->
# Fan v2 — reference examples

> 憲章: [async-inception.md](./async-inception.md)。文法は [fan-v2.md](./fan-v2.md)、
> 意味論は [logical-time-async.md](./logical-time-async.md)。
>
> **コードの原本は [fan-v2-examples/](./fan-v2-examples/) の `.almd` ファイル**である。
> 本文書はその索引と転写表であり、コード断片を複製しない（複製はドリフト源 —
> §13 の教訓）。各ファイルは実装時に `spec/` の fixture へ転写される — 例中の
> 挙動注記（`// →` 行）がそのまま受け入れ基準になる。
> `docs/roadmap/` 配下の `.almd` はどのゲートにも拾われない（確認済み）ので、
> 未実装構文をコードとして置ける。

## 1. 早見表 — 全セル 1 行ずつ

```almide
// all — 全部待つ（既存・不変）
let (user, posts) = fan { fetch_user(id); fetch_posts(id) }
let bodies = fan.map(urls, (u) => http.get(u))

// settle — 失敗も含めて全部集める
let (ra, rb) = fan.settle { risky_a(); risky_b() }
let results  = fan.settle(inputs, (x) => validate(x))

// any — リスト順で最初の成功（逐次フォールバック）
let cfg  = fan.any { fs_config(); env_config(); default_config() }
let body = fan.any(mirrors, (m) => fetch_from(m))

// bounded — 計算量の上限（Stage 2）
let plan = fan.bounded(compute.ms(100)) { optimal_plan(g) } ?? greedy_plan(g)

// race — 最安の成功が勝つ（Stage 3、枝は pure）
let ans = fan.race(compute.s(1)) { exact(input); heuristic(input) } ?? default_ans

// timeout — 環境が切る（Stage 4、oracle 層。それまで tombstone）
let page = fan.timeout(duration.s(5)) { http.get(url) } ?? cached_page
```

## 2. ファイル索引

| ファイル | 内容 | status |
|---|---|---|
| [bounded.almd](./fan-v2-examples/bounded.almd) | 全域化イディオム、`let` の書ける単一 body、`0ms`/発散の端、min-cap 入れ子 | Stage 2 |
| [race.almd](./fan-v2-examples/race.almd) | ポートフォリオ解法、**全 8 ケースの挙動表**（同着・trap 可視窓・枯渇）、二文法パース、mapper 形 | Stage 3 |
| [any.almd](./fan-v2-examples/any.almd) | 設定フォールバックチェーン、ミラー順次試行。「並行ではなくフォールバック」の意味論注記 | Wave 1 |
| [settle.almd](./fan-v2-examples/settle.almd) | 異型 tuple の block 形、全エラー収集の mapper 形 | Wave 1 |
| [fan_all_unchanged.almd](./fan-v2-examples/fan_all_unchanged.almd) | 不変面の対照（今日コンパイルする形）。C-004/C-199/C-200 の生存参照点 | current |
| [diagnostics.almd](./fan-v2-examples/diagnostics.almd) | **コンパイルしないことが正しい**例 6 件と期待診断（pure 制約、ticks ラベル、thunk tombstone、E008 等） | Wave 1〜Stage 3 |
| [migration.almd](./fan-v2-examples/migration.almd) | v1 → v2 対訳（any/settle/race/timeout）。funcref wall 消滅の注記 | Wave 1〜Stage 4 |

## 3. fixture への転写表

| 例 | 転写先 | pin する契約 |
|---|---|---|
| bounded.almd 基本形・端 | `spec/wasm_cross/fuel_bounded_*.almd` | bounded の枯渇等価（新 C-NNN）+ consumed ratchet |
| race.almd の挙動表 8 行 | `spec/wasm_cross/fuel_race_*.almd`（1 行 1 fixture） | 勝者等価・trap 可視窓・同着ソース順 |
| bounded.almd 入れ子 | `spec/wasm_cross/fuel_nest_*.almd` | min-cap と streaming 消費 |
| any.almd / settle.almd | `spec/lang/fan_any_block_test.almd` 等（Wave 1、fuel 不要） | any/settle の form 契約 |
| diagnostics.almd 全 6 件 | `tests/diagnostics/` の fixture（E0xx 各 1） | 診断文言 |
| fan_all_unchanged.almd | 既存 fixture が不変であること自体が回帰ゲート | C-004 / C-199 / C-200 |

例を足すときは、`.almd` ファイルに足し、この転写表に行を足すこと。例だけ増えて
fixture の種にならない状態は、SPEC.md が実装と乖離した §13 の再演になる。
