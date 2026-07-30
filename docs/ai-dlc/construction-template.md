# Unit <version> — Construction: Bolt 実行台帳

> 対になる仕様: [inception.md](./inception.md)（M0 承認済みであること）
> 規律: 証拠（commit SHA / CI run URL）の無いチェックは無効。Bolt N の証拠は
> 次 iteration の冒頭（前 Bolt の CI 確認時）に記録する。

## Bolt 台帳

| Bolt | Intent | DoD | 状態 | 証拠 |
|---|---|---|---|---|
| B1 | <intent> | <この Bolt 単体の完了条件> | 未着手 | — |
| B2 | … | … | 未着手 | — |

## 実行メモ

<Bolt 実行中に判明したこと・計画からの逸脱とその理由。逸脱が Scope を超えるなら M6 escalate>

## Unit 完了判定

- [ ] 全 Bolt が証拠付きで完了
- [ ] inception の DoD を証拠が満たしている（対応を明記）
- [ ] リリース（通常 minor は自動 / decade ゲートは M1 承認）
