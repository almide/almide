# Logical-time race 決定ゲート — 結果記録

Status: **PASSED — 74,898 構成、乖離 0（2026-08-01）**
Spec: [docs/roadmap/active/logical-time-proofs.md](../../../docs/roadmap/active/logical-time-proofs.md)
再実行: `research/spike/logical-time-race/run-gate.sh`

## ゲートが問うこと

logical-time-async.md の race 意味論 —

> 全枝が 1 tick に 1 fuel 進む lockstep の最初の決定的事象（Complete | Trap）が
> 結果を決める。キャンセル・枝刈り・遅延 cap 検査は最適化であり、観測を変えない。

— が **スケジューラの選択に対して合流する（confluent）** ことを、小スコープ全数で検査する。

各構成（枝のトレース組 × 予算）について 4 者を比較する:

| 実装 | 内容 |
|---|---|
| REF | merge 順の決定的事象規則（参照意味論そのもの） |
| SEQ | リスト順逐次 scan + 縮小 cap + trap 遅延判定 + 消費量再構成（wasm 戦略） |
| ADV | **全物理スケジュール**を memoized DFS で列挙。cap ちょうどでの枝刈りに加え、cap を跨いで走り過ぎる **overrun**（実機の周期的 fuel 検査のモデル）も敵対者の選択肢 |
| 入れ子 | REF / SEQ の occurred stream 上で外側 bounded の streaming 消費が一致 |

比較対象は outcome（勝者 / trap / exhausted）だけでなく **consumed fuel** と
**occurred charge stream**（外側 region が観測する merge 順の消費列）まで。

## スコープ

- 枝 ≤ 3、枝あたり charge ≤ 2（コスト 1..2）、終端 ∈ {Complete, Trap, Diverge}、予算 0..=5
- 枝 ≤ 2、charge ≤ 3、予算 0..=7（トレースを広げた第二スコープ）
- 外側 bounded: 予算 0..=9 × 追加 charge {0, 1}

計 74,898 構成 × ADV の全スケジュール（構成ごとに状態 memo 付き全列挙）。

## 開発中にモデルが捕まえたもの

- 参照意味論の occurred stream が Diverge 枝の cost-1 尾部を落としていた
  （SEQ との consumed 乖離として即検出 — モデル側のバグだったが、これが
  「乖離は必ず現れる」ことの実演にもなっている）。

## 機械化との関係

選択代数の核（決定的事象の一意性・部分集合安定性・cap の可視窓保存・合流）は
[crates/almide-race-belt/](../../../crates/almide-race-belt/) の Lean 定理として
kernel-check される（0 sorry、CI `lean-proofs` job）。この spike はその外側 —
トレース生成・消費量再構成・入れ子 streaming という「定理の前提を実装が満たすか」
の側 — を全数で反証しにいく装置である。fixture は反証し、定理は証明する。
