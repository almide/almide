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

## 残 15 件の地形

### week 級 — 8 件(1 件 = 1 集中セッション)

| issue | 内容 | 閉じると何が変わるか |
|---|---|---|
| #514 | fan race/any/map-err 乖離の決着(align or contract) | silent-wrong 監視枠が空になる |
| #515 | §3 メタモルフィック binding ゲート | let⟺assign 受理等価が機械検証に |
| #516 | §9 interp 第三審の fuzzer 配線(Oracle trait) | 「両ターゲット同罪」バグが可視化 |
| #521 | §8 stdlib @semantics マニフェスト | doc↔impl 一致が regen-and-diff ゲート化 |
| #531 | §4 2c CopyClass 正準化(挙動レビュー付き) | Copy 述語 4 個 → 1 個 |
| #532 | §7 fmt multi-module 再型検査 + §10 release 昇格 | ゲート被覆の完成 |
| #533 | §13+PR-3 乖離キュー + []-typing | 記録済み cross-target 乖離ゼロ |
| #534 | 仕様矛盾バーンダウン(??/Never/fan順序/空ページ) | prose↔挙動の一致 |

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

- 既知の穴ゼロ(week 級完売): **4〜7 日**(実績ペース換算)
- 「完全」と言える状態(背骨 3 本 + hunt 乾き): **+3〜6 週**

## 推奨順

#515/#516(設計済み・即着手可)→ #514(silent-wrong 枠の空化)→ #531/#532/#533/#534
→ hunt ラウンド 2 → 背骨(#528 → #529 → #530)。
