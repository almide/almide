<a id="mission-critical-stage-status--measured-regenerated"></a>

# ミッションクリティカル段階の状況 — 測定・再生成される記録

[![English](https://img.shields.io/badge/English-d0d7de?style=flat-square)](STAGE-STATUS.md) [![日本語](https://img.shields.io/badge/%E6%97%A5%E6%9C%AC%E8%AA%9E-24292f?style=flat-square)](STAGE-STATUS.ja.md) [![简体中文](https://img.shields.io/badge/%E7%AE%80%E4%BD%93%E4%B8%AD%E6%96%87-d0d7de?style=flat-square)](STAGE-STATUS.zh-CN.md)

> [正本](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/proofs/STAGE-STATUS.md)（固定コミット 35297a612fb29e3f78f6587aba253aa91d51f466）の非規範的な履歴参照用スナップショット。正本が優先され、この写しは自動同期されず、生成器や証明の入力ではない。コマンド、証明、測定、資格認定評価を独立に実行していない。出典参照、生成マーカー、日付付き数値は履歴資料として保持し、認証・資格認定・当局の受理・公開の証拠とはしない。

5 段階計画（Stage 1 accept-and-wrong の族滅 → Stage 2 translation validation 全数化 → Stage 3 意味論凍結 → Stage 4 持続性計測 → Stage 5 DO-330 ギャップ分析）は継続的な採用ロードマップである。このページは、その唯一の検査可能な状況成果物である。マーカー間の数値はすべて、コミット済み台帳から `proofs/ledger-counts.toml` へ測定して記録し、`scripts/gen-ledger-counts.sh` で日付を付ける（リリース手順として、または毎夜の `scripts/check-ledger-counts.sh` が乖離を報告した場合の対応として）。`scripts/gen-claims.sh` で描画し、手編集すれば CI が失敗する（`check-contracts.sh` が `--check` を実行する）。監査者は、セッションに散在した文章の主張ではなく、このページを読む。段階の意味と証拠の参照先：`docs/roadmap/active/flight-evidence-gaps.md`（監査指摘の台帳、2026-08-12 に再測定）および `proofs/TOR.md`（運用契約）。

<!-- stages:generated:start — derived from the proofs/ ledgers by scripts/gen-claims.sh; DO NOT EDIT between the markers -->
<!-- counts:generated:start (as of 2026-10-21) — stamped totals from proofs/ledger-counts.toml; refreshed only by scripts/gen-ledger-counts.sh, never by a fixture/contract PR; DO NOT EDIT between the markers -->
> **Stage 1（accept-and-wrong の根絶）：監査は完了、ゲートあり** —
> scalar-read 64 分岐／0 UNGUARDED。
> platform-libm 4 箇所を分類済み。未分類の新規項目は追加できない。
>
> **Stage 2（translation validation）：940/957 fixture が実際の 3 者間投票を行う（98%）** —
> 棄権の残りは分類済みで、減少のみ（interp-heap の取り組み、#1226）。
>
> **Stage 3（意味論凍結）：388/388 契約に spec キーがあり、構文要素カバレッジは
> 73/73 が節に対応付け済み（0 UNWRITTEN、減少のみ。凍結前提は 0）。**
>
> **Stage 4（持続性）：fuzz の true-green 連続記録 = 0 日**（日付付きの計測器。
> 正しさだけに基づく毎夜判定は 2026-08-12 に導入。90 日がマイルストーン）。
>
> **Stage 5（監査可能性）：19 個の release seal、126 個の検証ゲートを分類済み
> （減少のみの上限のもとで 0 UNVERIFIED）。TOR は強制手段付きの 10 行。
> ギャップ分析は proofs/DO330-GAP.md に統合（参照ゲートあり）。**
<!-- counts:generated:end -->
<!-- stages:generated:end -->

これらの数値は、外部での採用（台帳上ではなく市場の事実）、分野レベルの MC/DC 資格認定クレジット、完全な成果物の精緻化を主張しない。行／条件カバレッジと safety-set 判断ベクトルは別の証拠手段である。C-WCET / C-FAITHFUL の範囲と残る仮定は receipt を参照する。上記の日付付き構文要素対応付けは、その対応付けの前提条件を満たすのであって、意味論の完全性や、認定当局による凍結済み仕様の受理を意味しない。
