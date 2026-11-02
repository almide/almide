<!-- description: Reference examples for the fan v2 surface: every head, edge semantics, diagnostics -->
# Fan v2 — reference examples

> この索引は fan v2 導入時の設計例と fixture への転写計画を保存する。
> `fan-v2-examples/` の `.almd` は歴史的スケッチであり、現在の文法や診断の
> 正本ではない。旧 `any` / `race` の例を現在のプログラムへコピーしない。

## 1. 現在の参照先

- 文法・意味論: [SPEC §13](../../SPEC.md#13-concurrency-fan)
- 選択名と clock: [ADR-0028](../../adr/0028-fan-selection-names-and-cost-contract.md)
  — `fan.any` は `??` / `list.find_map`、`fan.race` は clock 必須の `fan.min` へ。
- 実装済み表面と拒否する形: `tests/fan_surface_matrix_test.rs`
- 回帰例: `spec/wasm_cross/fuel_bounded_*.almd`、`fuel_race_*.almd` 等。
  ファイル名に旧 `race` が残ることと、旧構文が使えることは別である。
- `fan.timeout(duration.ms(n)) { body }` は body 形として存在する。
  以下の Stage 4/tombstone は当初の実装順序であり、現行の未実装一覧ではない。

ここではコードを再転記せず、実行可能な fixture を正本とする。

## 2. 歴史的な設計例の索引

| ファイル | 内容 | 当初の段階 |
|---|---|---|
| [bounded.almd](./fan-v2-examples/bounded.almd) | 全域化イディオム、`let` の書ける単一 body、`0ms`/発散の端、min-cap 入れ子 | Stage 2 |
| [race.almd](./fan-v2-examples/race.almd) | ポートフォリオ解法、**全 8 ケースの挙動表**（同着・trap 可視窓・枯渇）、二文法パース、mapper 形 | Stage 3 |
| [any.almd](./fan-v2-examples/any.almd) | 設定フォールバックチェーン、ミラー順次試行。「並行ではなくフォールバック」の意味論注記 | Wave 1 |
| [settle.almd](./fan-v2-examples/settle.almd) | 異型 tuple の block 形、全エラー収集の mapper 形 | Wave 1 |
| [fan_all_unchanged.almd](./fan-v2-examples/fan_all_unchanged.almd) | 導入時の不変面の対照。C-004/C-199/C-200 の生存参照点 | 導入時の既存面 |
| [diagnostics.almd](./fan-v2-examples/diagnostics.almd) | **コンパイルしないことが正しい**例 6 件と期待診断（pure 制約、ticks ラベル、thunk tombstone、E008 等） | Wave 1〜Stage 3 |
| [migration.almd](./fan-v2-examples/migration.almd) | v1 → v2 対訳（any/settle/race/timeout）。funcref wall 消滅の注記 | Wave 1〜Stage 4 |

## 3. 当初の fixture 転写計画（現在の網羅性一覧ではない）

| 例 | 転写先 | pin する契約 |
|---|---|---|
| bounded.almd 基本形・端 | `spec/wasm_cross/fuel_bounded_*.almd` | bounded の枯渇等価（新 C-NNN）+ consumed ratchet |
| race.almd の挙動表 8 行 | `spec/wasm_cross/fuel_race_*.almd`（1 行 1 fixture） | 勝者等価・trap 可視窓・同着ソース順 |
| bounded.almd 入れ子 | `spec/wasm_cross/fuel_nest_*.almd` | min-cap と streaming 消費 |
| any.almd / settle.almd | `spec/lang/fan_any_block_test.almd` 等（Wave 1、fuel 不要） | any/settle の form 契約 |
| diagnostics.almd 全 6 件 | `tests/diagnostics/` の fixture（E0xx 各 1） | 診断文言 |
| fan_all_unchanged.almd | 既存 fixture が不変であること自体が回帰ゲート | C-004 / C-199 / C-200 |

現在の例を追加・更新する際は `spec/` の fixture と現行 SPEC を更新する。
この歴史的スケッチ群への追記だけでは、現在の文法や契約の受け入れ証拠にならない。
