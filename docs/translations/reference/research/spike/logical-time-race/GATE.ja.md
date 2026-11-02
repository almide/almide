# Logical-time race 決定ゲート — 結果記録

[![English](https://img.shields.io/badge/English-d0d7de?style=flat-square)](GATE.md) [![日本語](https://img.shields.io/badge/%E6%97%A5%E6%9C%AC%E8%AA%9E-24292f?style=flat-square)](GATE.ja.md) [![简体中文](https://img.shields.io/badge/%E7%AE%80%E4%BD%93%E4%B8%AD%E6%96%87-d0d7de?style=flat-square)](GATE.zh-CN.md)

> 非規範の原文固定・歴史的読者用スナップショット。原文を正本とし、機械入力や実行 gate として置き換えない。自動同期しない。記載されたテスト結果・件数・実装状態・証明の主張は原文の範囲に属し、ここで独立再実行したものでも現在の保証として確立したものでもない。 これは gate 結果記録の読者用写しであり、実行 gate ではない。74,898 の限定構成は普遍的証明ではない。原文の Lean 核定理に関する主張と、核の外で実装の前提を検査することを区別する。「乖離は必ず現れる」という主張は原文の例の説明として保持し、一般的な完全性保証にはしない。 [Source](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/research/spike/logical-time-race/GATE.md)

状態: **PASSED — 74,898 構成、乖離 0（2026-08-01）**
仕様: [docs/roadmap/active/logical-time-proofs.md](../../../../../roadmap/active/logical-time-proofs.md)
再実行: `research/spike/logical-time-race/run-gate.sh`

<a id="ゲートが問うこと"></a>

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

<a id="スコープ"></a>

## スコープ

- 枝 ≤ 3、枝あたり charge ≤ 2（コスト 1..2）、終端 ∈ {Complete, Trap, Diverge}、予算 0..=5
- 枝 ≤ 2、charge ≤ 3、予算 0..=7（トレースを広げた第二スコープ）
- 外側 bounded: 予算 0..=9 × 追加 charge {0, 1}

計 74,898 構成 × ADV の全スケジュール（構成ごとに状態 memo 付き全列挙）。

<a id="開発中にモデルが捕まえたもの"></a>

## 開発中にモデルが捕まえたもの

- 参照意味論の occurred stream が Diverge 枝の cost-1 尾部を落としていた
  （SEQ との consumed 乖離として即検出 — モデル側のバグだったが、これが
  「乖離は必ず現れる」ことの実演にもなっている）。

<a id="機械化との関係"></a>

## 機械化との関係

選択代数の核（決定的事象の一意性・部分集合安定性・cap の可視窓保存・合流）は
[crates/almide-race-belt/](../../../../../../crates/almide-race-belt/) の Lean 定理として
kernel-check される（0 sorry、CI `lean-proofs` job）。この spike はその外側 —
トレース生成・消費量再構成・入れ子 streaming という「定理の前提を実装が満たすか」
の側 — を全数で反証しにいく装置である。fixture は反証し、定理は証明する。
