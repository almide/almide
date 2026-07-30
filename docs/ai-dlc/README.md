# AI-DLC Unit 台帳 — Inception / Construction の対

> 運用モデル本体は [docs/AI_DLC.md](../AI_DLC.md)、ループ手順は [docs/AI_DLC_BOLT_LOOP.md](../AI_DLC_BOLT_LOOP.md)。
> ここは Unit（= [ROAD_TO_1_0.md](../roadmap/ROAD_TO_1_0.md) の 1 行）ごとの成果物を置く場所。

## 構造

```
docs/ai-dlc/
  units/<version>/
    inception.md      — Unit の詳細化: intent / scope / DoD / リスク / 提案 Bolt / Mob 承認記録
    construction.md   — Bolt 実行台帳: 各 Bolt の DoD・状態・証拠（commit SHA / CI run）
```

## 規律

- **対は必ず揃える** — inception の無い construction は作らない。construction の証拠が
  inception の DoD に対応しない Unit は完了と呼ばない。
- **just-in-time で作る** — Unit の着工直前に inception を書く。59 個を先に量産しない。
  Inception は鮮度が命で、前の Unit の結果（findings、計測値）が次の inception の入力になる。
- **Mob 承認（M0）が Construction の開始条件** — inception.md の承認記録が埋まるまで
  Bolt は 1 つも実行しない。承認は Unit につき 1 回。まとめて先承認してもよい。
- **construction.md が Bolt 計画の正本** — GitHub issue には重複させず、issue からここへ
  リンクする。証拠（commit SHA・CI run URL）の無いチェックは無効。
- **traceability** — ladder 行 ↔ `units/<version>/` ↔ issue ↔ commit が相互リンクで辿れること。

## テンプレート

- [inception-template.md](./inception-template.md)
- [construction-template.md](./construction-template.md)
