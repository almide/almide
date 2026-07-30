# Unit <version> — 実行台帳

> English (canonical): [construction-template.md](./construction-template.md) — 実際の Unit 文書は英語で書く。

> 対になる計画書: [inception.ja.md](./inception.ja.md)（承認済みであること）
> ルール: 証拠（commit SHA / CI run の URL）の無いチェックは無効。
> Bolt N の証拠は、次の iteration の冒頭（前回分の CI 確認時）に記入する。

## Bolt 台帳

| Bolt | 何をやるか | この Bolt の完了条件 | 状態 | 証拠 |
|---|---|---|---|---|
| B1 | <作業> | <条件> | 未着手 | — |
| B2 | … | … | 未着手 | — |

## 実行メモ

<やってみて判明したこと。計画とのずれとその理由。ずれが計画書の「やること」を超えるなら M6 で人間を呼ぶ>

## Unit 完了判定

- [ ] 全 Bolt が証拠つきで完了
- [ ] 証拠が計画書の完了条件を満たしている（どの証拠がどの条件に対応するか明記）
- [ ] リリース（普通の minor は自動 / 節目は M1 承認）
