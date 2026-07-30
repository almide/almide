# Unit 0.41 — Construction: Bolt 実行台帳

> 対になる仕様: [inception.md](./inception.md)（M0 承認済みであること）
> 規律: 証拠（commit SHA / CI run URL）の無いチェックは無効。Bolt N の証拠は
> 次 iteration の冒頭（前 Bolt の CI 確認時）に記録する。

## Bolt 台帳

| Bolt | Intent | DoD | 状態 | 証拠 |
|---|---|---|---|---|
| B1 | fuzzer prebuild で nightly のビルド税を除去 | nightly ジョブがビルドをスキップし、フォズ時間 ≈ 予算全体になる | 未着手 | — |
| B2 | campaign を短 shard N 分割し runner 回収クラスの失敗を排除 | 分割後のワークフローが 1 夜完走する | 未着手 | — |
| B3 | 予算復元 + programs/night を run summary に記録 | summary に programs/night が出る。予算が削減前の水準以上 | 未着手 | — |
| B4 | #917 残余検分 | #917 close、または残余 Bolt がこの台帳に追加済み | 未着手 | — |
| B5 | 3 夜連続フル予算完走の観測 | 3 夜分の run URL を証拠に記録 | 未着手 | — |

## 実行メモ

（未着工）

## Unit 完了判定

- [ ] 全 Bolt が証拠付きで完了
- [ ] inception の DoD を証拠が満たしている（対応を明記）
- [ ] リリース v0.41.0（通常 minor — 自動可）。#924 は 14 夜条件まで open のまま 0.42 へ引き継ぎ
