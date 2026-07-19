<!-- description: Issue-ledger burn-down tracker for the completeness campaign — GitHub open-issue count as the completeness residual -->
# Issue Ledger Burn-down — 完全性キャンペーンの残量計

> 2026-06-11 時点。台帳→tracker の単射は完成済み: **GitHub の open issue 数 =
> 既知の完全性残量**。閉鎖規律 = クラス殺し(各 issue に構造的閉鎖条件を記載済み、
> パッチでは閉じない)。停止条件 = #535 の loop-until-dry。

## 戦果(2026-06-10〜11 の 2 日間)

リリース: v0.26.20 → **v0.27.3**(6 本)。クローズ: #484-490 系 + #500/501/502/505
+ #522/523/524/525/526/527/517(全てクラス殺し形)。
PR: #487〜#543。マトリクスゲート 0→29 セル、churn ゲート 0→3 本、
契約 58→67(C-066 含む)、wasm メモリモデル「全リーク」→「既定 O(1)」。

## 残 7 件の地形

### week 級 — 0 件(全 8 件クローズ済み、2026-06-11)

#514/#515/#516/#521/#531/#532/#533/#534 は本ドキュメント作成と同日の
2026-06-11 中に全てクラス殺し形でクローズ済み(`gh issue view <N> --json state`
で再確認、2026-07-19)。silent-wrong 監視枠は空、let⟺assign 受理等価は機械検証済み、
interp 第三審は fuzzer に配線済み、記録済み cross-target 乖離はゼロ。

### weeks 級(背骨)— 6 件

| issue | 内容 | 規模 |
|---|---|---|
| #528 | QualifiedRef newtype(#433 機械化、§4 サイクル適用) | 1-3 週 |
| #529 | WasmIR 中間層(emitter 構造不変条件の by-construction 化) | 2-3 週 |
| #530 | ALS 規範仕様の昇格(CG-1) | 1-2 週 |
| #518 | construct-from-temp 過剰計上(Koka 流 dup/drop 精密化) | 研究寄り |
| #519 | アロケータ方針(単調成長 acc × first-fit) | 研究寄り |
| #520 | Lean のランタイム frees 拡張(アロケータ状態機械の証明) | 研究寄り |

### プロセス — 1 件

- **#535 loop-until-dry**: hole-hunt をレンズ替えで回し、2-3 連続空振りで初めて
  「現在の知見で完全」。ラウンド 1(multi-truth / silent-fallback / rule-bypass)
  = 6 クラス検出 → 全消化済み。**ラウンド 2 未実施** — 次レンズ候補:
  pass 順序仮定、checker-accepts-lowering-reinterprets、interp 意味差、診断乖離、
  hash 順以外のホスト環境依存。

## 見積もり

- 既知の穴ゼロ(week 級完売): **完了(2026-06-11)**
- 「完全」と言える状態(背骨 3 本 + hunt 乾き): 残 7 件(週級ゼロ、背骨 3 本 + 研究寄り 3 本
  + hunt ラウンド 2)

## 推奨順

hunt ラウンド 2(#535)→ 背骨(#528 → #529 → #530)→ 研究(#518〜#520)。
week 級ステップ(旧 #515/#516/#514/#531/#532/#533/#534)は全クローズ済みにつき削除。
