<a id="outside-review-audit-2026-07--証拠層の負債バーンダウン"></a>

# 外部視点監査 2026-07 — 証拠層の負債バーンダウン

[![English](https://img.shields.io/badge/English-d0d7de?style=flat-square)](outside-review-audit-2026-07.md) [![日本語](https://img.shields.io/badge/%E6%97%A5%E6%9C%AC%E8%AA%9E-24292f?style=flat-square)](outside-review-audit-2026-07.ja.md) [![简体中文](https://img.shields.io/badge/%E7%AE%80%E4%BD%93%E4%B8%AD%E6%96%87-d0d7de?style=flat-square)](outside-review-audit-2026-07.zh-CN.md)

> [日英混在の正本](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/active/outside-review-audit-2026-07.md)（固定コミット `35297a612fb29e3f78f6587aba253aa91d51f466`）の非規範的な読者用スナップショット。元の文書が正本であり、この写しは自動同期されず、生成器・チェッカーの入力でもない。以下の日付、計画、完了状態、測定値、比較主張は当時の原文の記述を保持したもので、独立した再現や現行版についての検証ではない。コードブロックとコメントは原文のまま保持する。過去の収集用メタデータは、動作しない読者向け注記として表示する。

> 過去の説明：2026-07-27 の外部レビュアー視点の 5 観点監査。証拠層が v0→v1 に遅れ、正直さが逆転（内部文書は正直、外向き主張は偽）。issue #913–#932 とともに「偽の主張ゼロ・全ゲート実効」へ層別に負債を減らす。

> 原文の注意点：これは日付付きの 7 月の監査記録であり、新たな監査でも、全所見が今も未解決だという証明でもない。強い比較評価、欠陥数、想定期間は原文の記述であり、独立に検証した推薦や日程の約束ではない。修正案と issue 操作は過去の記述として引用し、翻訳で実行しない。失敗の証拠を保持しても、コンパイラ全体や認証の保証にはならない。

> 2026-07-27 実施。「2026年最高峰のコンパイラとして外部の目利き
> (Rust/MLIR/Koka/CompCert 界隈のレビュアー) に見られたときのツッコミどころ」を
> 5 観点 — アーキテクチャ/IR、性能、DX、証明/認証、エコシステム/外形 — で
> 並列走査した結果の統合。全所見は issue #913–#932 + bonsai-almide#1 に
> file:line 証拠と構造的閉鎖条件つきで起票済み。本文書はその地図。

<a id="核心の構図"></a>

## 核心の構図

**v0→v1 移行でコンパイラ本体は前進したが、周りの証拠層 — 主張・ベンチ・
ゲート・ドキュメント — が移行に追随せず、正直さの勾配が逆転した。**

- 内部文書 (TRUSTED_BASE.md / proven-vs-trusted.md / flight-*.md) は
  査読者が驚くほど正直 — 「学術的な verified-compiler 論文より正直」評価。
- 外向きの面 (README / TRUST-SPINE.md / BENCHMARKS.md / docs-site) に
  事実として偽の主張が集中。
- evidence-first を看板にするプロジェクトにとって、これが最深の脆弱性:
  **外部レビュアーは偽の主張を 1 つ見つけた時点で、本物の強み
  (契約台帳、coqchk、skip ledger) まで割引いて読む。**

同型のパターンが 5 レポート全部に出た: 性能スコアボードは削除済み v0 emitter の
測定値、roadmap/active に消えたディレクトリを語る文書、CI ゲートの一部は
形だけ (fmt --check、axiom-clean、LEDGERED examples)、fuzz は 20 晩中 1 成功。
**コンパイラが良くなったのに証拠が古いまま**、が根本原因。

<a id="第1層--外向き主張の虚偽-数時間1日級最優先"></a>

## 第1層 — 外向き主張の虚偽 (数時間〜1日級、最優先)

| issue | 内容 |
|---|---|
| #913 | README の Lean 主張が三重に偽: `allHeapFreed` = 「Dec ≥1」(リーク排除でない)、`perceusTransform` は 12 行トイ、「Formally Verified」が証明ゼロの native レグを覆う |
| #914 | TRUST-SPINE.md の 4 虚偽: V = substring 検索、ALS 規範意味論は不在 (80 行/定理 2)、TCB に未使用の CompCert/CertiCoq、checker「数百行」実際 1,144 行 |
| #915 | fan 並行性: `fan.map` sequential、`fan.race` は先頭 thunk のみ実行。docs-site は存在しない tokio/TS backend と削除済み fan.timeout を記載 |
| #916 | 性能主張: README「faster than Gleam/MoonBit」は LLM 作業時間、BENCHMARKS「100 ops 1.1s」は無基準、vs-Rust 表は v0 の遺骸、wasm-opt フラグ docs≠code |
| #918 | stale docs 一掃: PRODUCTION_READY が v0.8.0 凍結、wasm-engine-redesign.md が消えた emit_wasm/ を記述、847 vs 834 |
| bonsai#1 | PERF_ROADMAP「724 tok/s」実測 0.725 — 1000 倍誤記 (勝って見える方向) |

<a id="第2層--動かないゲート-アイデンティティの傷"></a>

## 第2層 — 動かないゲート (アイデンティティの傷)

| issue | 内容 |
|---|---|
| #919 | `almide fmt --check` は常に exit 0 — これを使う CI ゲートは全部 no-op |
| #920 | axiom-clean「ゲート」は printf — Axiom/Admitted を足しても green。修正は grep 3 行 + tamper drill |
| #921 | trust-spine.yml がリリースパス (main への PR) で走らない + paths filter が frontend を除外 + wasmtime 不在で check-wasm-exec が silent exit 0 |
| #922 | LEDGERED examples は一度もコンパイルされない — 22 例中 6 例が 4 ヶ月壊れたまま |
| #923 | `almide explain` が installed binary で 32 コード中 21 失敗 (docs 未埋め込み) |
| #924 | fuzz-nightly 20 晩中 1 成功 — silent-wrong class (closed 258 件中 49 件の支配クラス) の計測器が停止中。#796 のブロッカー |

<a id="第3層--構造債-arc-級依存関係あり"></a>

## 第3層 — 構造債 (arc 級、依存関係あり)

| issue | 内容 | 依存 |
|---|---|---|
| #925 | frontend が毎ビルド 2 回走り 1 回目の IR を捨てる + 手動同期ドライバ 6 系統 → 単一ドライバ化 | #928 の前提 |
| #926 | `is_heap_type` が 3 crate 6 箇所で乖離定義 (pass_anf の Ty::Named 欠落は実リーク症状つき) → 単一ソース化 | — |
| #927 | LSP 正しさ: UTF-16 位置をバイト扱い (非 ASCII で panic)、rename は文字列置換、didChange が git fetch + lock 書き込み | — |
| #928 | incremental/query 基盤ゼロ — LSP は毎キーストロークでバッチコンパイラ | #925 |
| #929 | wasm レグに nanopass optimizer 不在 (almide-mir は almide-codegen 非依存) + SIMD ゼロ — 最適化品質が構造的に二又 | #917 |
| #930 | 死コード掃除: 到達不能 26-pass wasm パイプライン (2 つ目の Perceus ごと)、死んだ Rust エミッタ 2 本、未使用 wasm-encoder | — |
| #931 | wall 診断: `LowerError::Unsupported(String)` に span なし、`{:?}` 入れ子ダンプ + 「issue 書いて」 → Diagnostic 化 + shape 別 rewrite hint | — |
| #917 | v1 レグの対 Rust 再ベンチ + CI perf ratchet (現状ベンチはどこでも走っていない) | — |
| #932 | renderer→証明 byte link の機械化 (check-wasm-bytes.sh の手書き heredoc 比較を renderer 実出力に) | — |

<a id="第4層--証明のコアギャップ-内部では自認済み対外整合が先"></a>

## 第4層 — 証明のコア・ギャップ (内部では自認済み、対外整合が先)

起票対象外 (既存 arc / TRUSTED_BASE 残余台帳の管轄)。外向き文書 (#913/#914)
さえ直せば、以下は「正直に開示された研究課題」として成立する:

- 証明対象は untrusted なコンパイラ自身が書いた certificate 文字列 —
  生成者 = 主張者で相関バグは不可視 (この class の出荷 5 回を自己記録済み)
- Almide の形式意味論が存在せず `⟦s⟧ ≈ ⟦compile(s)⟧` 型の定理はゼロ —
  byte 一致は差分テスト由来 (→ #530 ALS 昇格が真の解)
- native レグの証明カバレッジゼロ (→ #764 native trust spine)
- Perceus reuse `r` は Coq で健全性証明済みだが `Op::Reuse` を emit する
  パスが存在しない — 証明済み未実装

<a id="守るべき強み-割引かれる前に"></a>

## 守るべき強み (割引かれる前に)

187 契約の CI 強制台帳 (README ドリフト検知つき)、coqchk 独立再検証、
Lean 0 sorry、wasm skip 台帳 (30→11、理由必須)、org-trust-status.md
(自分の失敗を公開)、770B verified Hello World、wasm-opt differential parity
gate、rustc 級の診断データモデル (`try_replace_span`)、playground。
**検証文化そのものは funded プロジェクト以上** — 5 レポートの一致評価。
第1〜2層はこの資産の毀損を止める作業であって、新規投資ではない。

<a id="着手順"></a>

## 着手順

1. **Sprint 1 = 第1層 + 第2層** (全部 数時間〜1日級)。完了状態:
   **「偽の主張ゼロ・ゲートは全部本物」**。数値 (定理数/checker 行数/
   stdlib fn 数) は gen-claims 機構に載せて再ドリフト不能に。
2. **#924 fuzz 復旧** — 計測器が戻るまで「open silent bug 0 件」は無意味。
3. **第3層は既存 arc に編入して通常運転** — #925→#928 の順序だけ固定。
   #929 は #917 の測定結果でスコープを決める (測ってから作る)。

<a id="閉鎖条件"></a>

## 閉鎖条件

第1層 + 第2層の全 issue クローズ後に同型の 5 観点再監査を 1 回走らせ、
**外向き文書に fatal 所見ゼロ**であること。第3層は各 issue が自分の
閉鎖条件を持つ (本文書はそれらを待たない)。達成時点でこの文書は
done/ へ移動し、再監査の所見が新しい台帳になる。

<a id="未起票の-serious-次の-dx-arc-でまとめる"></a>

## 未起票の serious (次の DX arc でまとめる)

REPL が cargo-per-line (almide-interp 未使用)、test 失敗の per-file 報告
(assertion が生成 Rust の panic として出る)、parse error が診断フォーマット外
の二級 tier (span を文字列往復で復元)、error dedup なし (import 1 個欠落で
同一エラー 8 件)、formatter 出力が自リポジトリのスタイルと乖離 + import を
書き換える。
