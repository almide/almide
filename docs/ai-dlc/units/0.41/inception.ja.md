# Unit 0.41 — 計画書: fuzz-nightly を毎夜使える計測器に戻す

> English (canonical): [inception.md](./inception.md) — 正は英語版。承認記録も英語版に記入されます。

- **ねらい**: 0.4x「計測器と編集ループ」の第 1 原則「計測器が手術より先」。silent-wrong-code
  （黙って間違ったコードを出すバグ）の検出器を毎夜動く状態に戻さない限り、この先の codegen の手術
  （0.5x の wasm 最適化、0.6x の cranelift）を安全網なしでやることになる。Gate 0.50「fuzz 連続緑が常態」の前提。
- **Issues**: [#924](https://github.com/almide/almide/issues/924)（主）、
  [#917](https://github.com/almide/almide/issues/917)（残務の確認と close）

## 3 行でいうと

- 夜間 fuzz が直近 20 夜で 1 夜しか完走していない。原因はビルド税と runner の途中回収で、修理方法は見えている
- ビルドを事前に済ませ、ジョブを短く分割し、予算を戻して、スループットを毎夜記録する
- 3 夜連続で完走したらリリース。#924 自体の閉鎖（14 夜連続）は 0.42 に跨いで見届ける

## 背景

2026-07-27 の監査より（#924）: 直近 20 夜の内訳は成功 1・失敗 17・キャンセル 3。根因は 3 つ。
(a) runner が長時間ジョブを途中で回収する。(b) 10 分の予算のうち 4 分 48 秒がフォズァのビルドに消え、
実際にフォズしたのは 2 分 39 秒。(c) その後予算が 5 分に削られ、1 夜あたり約 1,000 プログラムまで低下。

これが重い理由: closed 258 issue のうち 49 が silent/miscompile/diverge 級（1 回の run で 478 件の
divergence を出した #727 を含む）。いまの「open な silent bug は 0」は、計測器が止まっているだけで、
コンパイラが綺麗な証明にはならない。#796（2 夜連続緑）は一度も満たされたことがない。

## やること

- S1 **ビルド税の除去** — fuzzer バイナリを release ビルドから cache し、予算の全部をフォズに使う
- S2 **runner 回収への耐性** — campaign を N 個の短い shard に分割する（回収に殺されるのは長いジョブ）。
  それでも不安定なら self-hosted / 大型 runner を検討
- S3 **予算の復元と可視化** — 予算を削減前の水準に戻し、programs/night を run summary に記録して
  スループットの後退が見えるようにする
- S4 **#917 の残務確認** — perf scoreboard と双方向 ratchet は 0.40.2 の後に native 側が着地済み。
  残り（wasm 側の計測が suite に入っているか、日付つき results の運用）を確認し、
  残があれば Bolt を足し、なければ証拠を添えて close

## やらないこと

- fuzz が見つけた findings の修正 — 0.42（#796 true green）の仕事
- SIMD 復活・wasm 最適化 — 0.53–0.54（#929）
- fuzz レンズの拡張（pass-ordering 検査など） — 0.52（#912）

## 完了条件

- fuzz-nightly が **3 夜連続でフル予算を完走**している（= 修理完了の判定。
  #924 自体の閉鎖条件は「14 夜連続」なので、issue は open のまま 0.42 期間へ跨いで見届ける。
  **リリース判定と issue 閉鎖を分ける、というこの判断が M0 の主要承認事項**）
- run summary に programs/night が出ていて、ビルド税がほぼゼロであることが数字で確認できる
- #917 が close されている、または残りが Bolt としてこの Unit の実行台帳に載っている

## 危ない所

- R1 GitHub-hosted runner の回収挙動はこちらで制御できない → shard 分割で吸収する。
  3 夜観測しても完走率が改善しなければ、self-hosted 移行の判断として M6 で人間を呼ぶ
- R2 「3 夜」「14 夜」は実時間がかかる → 完走の観測待ちの間、ループは次 Unit（0.42）の計画書起草など
  独立作業を先行してよい（実行台帳の作成は承認後のみ、のルール通り）

## Bolt 案

- B1 fuzzer prebuild — release ビルドの成果物を cache し、nightly ジョブからビルド税を消す
- B2 shard 分割 — campaign を短いジョブ N 個に組み替え、runner 回収による失敗クラスを消す
- B3 予算復元 + programs/night を run summary に記録
- B4 #917 の残務確認 → close または Bolt 追加
- B5 3 夜連続完走の観測と証拠記録 → リリース

## 承認（M0）

- 状態: 未承認
- 承認者 / 日付 / 判断メモ: —
