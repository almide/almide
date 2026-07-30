# AI-DLC の Unit 置き場 — 計画書と実行台帳の対

> English (canonical): [README.md](./README.md) — 正は英語版。ループが読み書きするのは英語版だけで、
> `.ja.md` は参考訳です（追随が遅れることがあります）。

ここには Unit（= [ROAD_TO_1_0.md](../roadmap/ROAD_TO_1_0.md) の 1 行）ごとの文書を置きます。
1 Unit = 1 フォルダ。中身は必ずこの 2 枚です。

```
docs/ai-dlc/units/<version>/
  inception.md      — 計画書: やること / やらないこと / 完了条件 / 危ない所 / Bolt 案。人間の承認欄つき
  construction.md   — 実行台帳: Bolt ごとの完了条件・状態・証拠（commit SHA と CI run の URL）
```

運用モデルの全体像は [docs/AI_DLC.ja.md](../AI_DLC.ja.md)、毎回の手順は [docs/AI_DLC_BOLT_LOOP.ja.md](../AI_DLC_BOLT_LOOP.ja.md)。

## 守ること 5 つ

1. **対で置く** — 計画書の無い実行台帳を作らない。証拠が計画書の完了条件に対応しない Unit は完了と呼ばない。
2. **着工直前に書く** — 59 個まとめて先に書かない。前の Unit の結果（findings や計測値）が
   次の計画書の材料になるので、先に書いた計画書は着工時には古くなっている。
3. **承認（M0）前に Bolt を始めない** — 承認は Unit につき 1 回。まとめて先にもらってもよい。
4. **Bolt 計画の正本はここ** — issue には重複させず、issue からここへリンクする。
   証拠（commit SHA と CI run の URL）の無いチェックは無効。
5. **相互に辿れること** — 台帳の行 ↔ このフォルダ ↔ issue ↔ commit がリンクでつながっている。

## テンプレート

- [inception-template.ja.md](./inception-template.ja.md) — 計画書
- [construction-template.ja.md](./construction-template.ja.md) — 実行台帳
