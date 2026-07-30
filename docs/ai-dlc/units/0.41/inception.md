# Unit 0.41 — Inception: fuzz-nightly を毎夜使える計測器に戻す

- **Intent（decade 文脈）**: 0.4x「計測器と編集ループ」の第 1 原則「計測器が手術より先」。
  silent-wrong-code の検出器を常時稼働に戻さない限り、以後の codegen 手術（0.5x wasm 最適化、
  0.6x cranelift）は安全網なしで行うことになる。Gate 0.50 の「fuzz 連続緑が常態」の前提。
- **Issues**: [#924](https://github.com/almide/almide/issues/924)（主）、
  [#917](https://github.com/almide/almide/issues/917)（close-out）

## 背景

2026-07-27 監査より（#924）: 直近 20 夜で成功 1・失敗 17・キャンセル 3。根因は
(a) runner が長時間ジョブを途中回収する、(b) 10 分予算のうち 4m48s がフォズァのビルド税で
実フォズは 2m39s、(c) その後予算が 5 分に削減され ≈1,000 programs/night まで低下。
closed 258 issue 中 49 が silent/miscompile/diverge 級（1 run で 478 divergence の #727 を含む）
— 「open な silent bug 0」は現状、計測器停止の反映であって清浄の証明ではない。
#796（2 連続緑夜）は一度も満たされたことがない。

## Scope

- S1 **ビルド税の除去** — fuzzer バイナリを release ビルドから cache し、予算全部をフォズに使う
- S2 **runner 回収耐性** — campaign を N 個の短い shard に分割（回収が殺すのは長ジョブ）。
  それでも不安定なら self-hosted/大型 runner を検討
- S3 **予算復元と可視化** — 予算を戻し、programs/night を run summary に記録して
  スループット退行を見えるようにする
- S4 **#917 close-out** — perf scoreboard / 双方向 ratchet は 0.40.2 後に native leg 着地済み。
  残余（wasm leg の計測が suite に含まれるか、dated results の運用）を検分し、
  残があれば Bolt 追加、なければ証拠を添えて close

## Non-scope

- findings の修正 — 0.42（#796 true green）の仕事
- SIMD 復活・wasm 最適化 — 0.53–0.54（#929）
- fuzz レンズの拡張（pass-ordering 等） — 0.52（#912）

## DoD / 計測基準

- fuzz-nightly が **3 夜連続でフル予算を完走**（ワークフロー機構の修理完了の判定。
  #924 自体の閉鎖条件は「14 夜連続」なので issue は 0.42 期間へ跨いで open のまま監視する
  — リリース判定と issue 閉鎖の分離。**この分離が M0 の主要承認事項**）
- run summary に programs/night が出力され、ビルド税 ≈0 が数字で確認できる
- #917: close されている、または残余が Bolt として本台帳に追加されている

## リスク

- R1 GitHub-hosted runner の回収挙動は制御外 → shard 設計で吸収。3 夜観測しても
  完走率が改善しない場合は self-hosted 移行の判断として M6 escalate
- R2 3 夜/14 夜のクロックは実時間を要する → 完走観測中は次 Unit（0.42）の inception 起草など
  独立作業をループが先行してよい（Construction は M0 承認後のみ）

## 提案 Bolt

- B1 fuzzer prebuild — release ビルド成果物を cache し、nightly ジョブのビルド税を除去
- B2 shard 化 — campaign を短ジョブ N 分割に再構成し、回収による失敗クラスを排除
- B3 予算復元 + programs/night を run summary に記録
- B4 #917 残余検分 → close or Bolt 追加
- B5 3 夜連続完走の観測と証拠記録 → リリース

## Mob 承認（M0）

- 状態: 未承認
- 承認者 / 日付 / 判断メモ: —
