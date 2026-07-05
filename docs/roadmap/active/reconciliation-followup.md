<!-- description: v0.28.0 reconciliation follow-up: deferred develop commits for 0.28.1 -->
# reconciliation follow-up — v0.28.0 で見送った develop 側の残件

> 2026-07-06 記録。v0.28.0 の [reconciliation](v1-release-path.md) は develop(0.27.13)の
> **capability を全て v1 線に再適用**して出荷した。ここは、その際に**意図的に見送った
> develop 側コミット**（capability ではないもの）と、0.28.1 以降の取り込み方針を残す。
> **関連**: [v1-release-path](v1-release-path.md) / [v1-v0-parity](v1-v0-parity.md)。

## 前提

- v0.28.0 で develop の **substantive fix は全て再適用済み**（mut-record writeback、wasm
  aliasing、E026、native cdylib、http、npm削除、dep 0.252 ほか）。「v0 でできること」は
  capability として全て包括した。
- 下の残件は **capability ではない**（perf 最適化 / docs / CI 基盤 / cosmetic リファクタ）。
  develop の履歴 (`git log origin/main`) に残っているので、いつでも cherry-pick できる。
- reconciliation は `git merge -s ours` で履歴を記録しつつ v1 tree を保持したので、
  これらは「tree に無い」だけで「失われて」はいない。

## 0.28.1 で取り込む候補（cherry-pick 可）

### c2f87402 — WASM SIMD perf（q1_0/transpose/fold）
- **性質**: perf 最適化。**correctness core（variant-eq tail-zero）は v1 で冗長**と実証済み
  （develop-v1 の iter_scope free-list reset + iter-scope escape がカバー、leak-test は
  WASM 経由 pass + PCC ownership ACCEPT）。よって残るのは **SIMD による matrix 高速化のみ**。
- **判断材料**: develop-v1 は既に独自の SIMD 実装を持つ（matrix fast-exp、Q1_0 dequant）。
  c2f87402 の q1_0/transpose/fold SIMD と**どこまで重複するか要調査**。重複なら不要、
  差分があれば nn/qwen 系ワークロードの v0-wasm perf のために port。
- **工数/リスク**: 12 files、emit_wasm の SIMD emit。correctness に影響しない perf なので
  優先度は中。まず develop-v1 側の既存 SIMD カバレッジを測ってから判断する。

### c5e13f61 — certification-grade roadmap の terminal goal 記録
- **性質**: docs のみ。`docs/roadmap/active/certification-grade.md` への追記。
- **工数**: 極小。cherry-pick で衝突する可能性はあるが手で解決可。

### 13691202 — nightly fuzz harness の子プロセス group reap + 出力 bound
- **性質**: CI/test 基盤の hardening（orphan 子プロセス回収、出力上限）。
- **工数**: 小。fuzz harness スクリプトのみ。CI 安定性のために取り込む価値あり。

## 永久スキップ

### c3648caa — WASM emitter の magic-number 撤廃（named constants）
- **性質**: cosmetic リファクタ（49 files）。develop の ~26-file 構造前提の named-constant 化。
- **理由**: develop-v1 は emit_wasm を **109 ファイルに分割した独自構造**を持つ。develop の
  named-constant 化は v1 の構造と噛み合わず、reconciliation の emit_wasm 衝突の主因だった。
  v1 側で同等の可読性改善が要るなら、v1 構造に合わせて**別途**やる（develop のを移植しない）。

## 受入方針

- 取り込む場合も **honest-wall + 全 gate green を維持**（parity 210 MISMATCH=0、PCC ACCEPT、
  mir/spec/cargo/drift/contracts/docs-gen）。perf 変更は byte-identical gate を必ず通す。
- version bump 時は **llms.txt の "Current stable" narrative を更新**しないと
  `docs_gen_check_passes_on_clean_checkout` が落ちる（docs-gen の version 記載チェック）。
